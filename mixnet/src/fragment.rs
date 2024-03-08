use std::collections::HashMap;

use sphinx_packet::{constants::PAYLOAD_SIZE, payload::PAYLOAD_OVERHEAD_SIZE};
use uuid::Uuid;

use crate::error::MixnetError;

pub(crate) struct FragmentSet(Vec<Fragment>);

impl FragmentSet {
    const MAX_PLAIN_PAYLOAD_SIZE: usize = PAYLOAD_SIZE - PAYLOAD_OVERHEAD_SIZE;
    const CHUNK_SIZE: usize = Self::MAX_PLAIN_PAYLOAD_SIZE - FragmentHeader::SIZE;

    pub(crate) fn new(msg: &[u8]) -> Result<Self, MixnetError> {
        // For now, we don't support more than `u8::MAX + 1` fragments.
        // If needed, we can devise the FragmentSet chaining to support larger messages, like Nym.
        let last_fragment_id = FragmentId::try_from(Self::num_chunks(msg) - 1)
            .map_err(|_| MixnetError::MessageTooLong(msg.len()))?;
        let set_id = FragmentSetId::new();

        Ok(FragmentSet(
            msg.chunks(Self::CHUNK_SIZE)
                .enumerate()
                .map(|(i, chunk)| Fragment {
                    header: FragmentHeader {
                        set_id,
                        last_fragment_id,
                        fragment_id: FragmentId::try_from(i)
                            .expect("i is always in the right range"),
                    },
                    body: Vec::from(chunk),
                })
                .collect(),
        ))
    }

    fn num_chunks(msg: &[u8]) -> usize {
        msg.len() / Self::CHUNK_SIZE + (msg.len() % Self::CHUNK_SIZE > 0) as usize
    }
}

impl AsRef<Vec<Fragment>> for FragmentSet {
    fn as_ref(&self) -> &Vec<Fragment> {
        &self.0
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub(crate) struct Fragment {
    header: FragmentHeader,
    body: Vec<u8>,
}

impl Fragment {
    pub(crate) fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(FragmentHeader::SIZE + self.body.len());
        out.extend(self.header.bytes());
        out.extend(&self.body);
        out
    }

    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        Ok(Self {
            header: FragmentHeader::from_bytes(&value[0..FragmentHeader::SIZE])?,
            body: value[FragmentHeader::SIZE..].to_vec(),
        })
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
struct FragmentSetId(Uuid);

impl FragmentSetId {
    const SIZE: usize = 16;

    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy)]
struct FragmentId(u8);

impl FragmentId {
    const SIZE: usize = std::mem::size_of::<u8>();
}

impl TryFrom<usize> for FragmentId {
    type Error = MixnetError;

    fn try_from(id: usize) -> Result<Self, Self::Error> {
        if id > u8::MAX as usize {
            return Err(MixnetError::InvalidFragmentId);
        }
        Ok(Self(id as u8))
    }
}

impl From<FragmentId> for usize {
    fn from(id: FragmentId) -> Self {
        id.0 as usize
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
struct FragmentHeader {
    set_id: FragmentSetId,
    last_fragment_id: FragmentId,
    fragment_id: FragmentId,
}

impl FragmentHeader {
    const SIZE: usize = FragmentSetId::SIZE + 2 * FragmentId::SIZE;

    fn bytes(&self) -> [u8; Self::SIZE] {
        let mut out = [0u8; Self::SIZE];
        out[0..FragmentSetId::SIZE].copy_from_slice(self.set_id.0.as_bytes());
        out[FragmentSetId::SIZE] = self.last_fragment_id.0;
        out[FragmentSetId::SIZE + FragmentId::SIZE] = self.fragment_id.0;
        out
    }

    fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        if value.len() != Self::SIZE {
            return Err(MixnetError::InvalidFragmentHeader);
        }

        Ok(Self {
            set_id: FragmentSetId(Uuid::from_slice(&value[0..FragmentSetId::SIZE])?),
            last_fragment_id: FragmentId(value[FragmentSetId::SIZE]),
            fragment_id: FragmentId(value[FragmentSetId::SIZE + FragmentId::SIZE]),
        })
    }
}

pub struct MessageReconstructor {
    fragment_sets: HashMap<FragmentSetId, FragmentSetReconstructor>,
}

impl MessageReconstructor {
    pub fn new() -> Self {
        Self {
            fragment_sets: HashMap::new(),
        }
    }

    pub fn add(&mut self, fragment: Fragment) -> Option<Vec<u8>> {
        let set_id = fragment.header.set_id;
        self.fragment_sets
            .entry(set_id)
            .or_insert(FragmentSetReconstructor::new(
                fragment.header.last_fragment_id,
            ))
            .add(fragment)
            .map(|msg| {
                // A message has been reconstructed completely from the fragment set.
                // Delete the fragment set from the reconstructor.
                self.fragment_sets.remove(&set_id);
                msg
            })
    }
}

struct FragmentSetReconstructor {
    last_fragment_id: FragmentId,
    fragments: HashMap<FragmentId, Fragment>,
    // For mem optimization, accumulates the expected message size
    // whenever a new fragment is added to the `fragments`.
    message_size: usize,
}

impl FragmentSetReconstructor {
    fn new(last_fragment_id: FragmentId) -> Self {
        Self {
            last_fragment_id,
            fragments: HashMap::new(),
            message_size: 0,
        }
    }

    fn add(&mut self, fragment: Fragment) -> Option<Vec<u8>> {
        self.message_size += fragment.body.len();
        if let Some(old_fragment) = self.fragments.insert(fragment.header.fragment_id, fragment) {
            // In the case when a new fragment replaces the old one, adjust the `meesage_size`.
            // e.g. The same fragment has been received multiple times.
            self.message_size -= old_fragment.body.len();
        }

        self.try_build_message()
    }

    /// Merges all fragments gathered if possible
    fn try_build_message(&self) -> Option<Vec<u8>> {
        if self.fragments.len() - 1 == self.last_fragment_id.into() {
            let mut msg = Vec::with_capacity(self.message_size);
            for id in 0..=self.last_fragment_id.0 {
                msg.extend(&self.fragments.get(&FragmentId(id)).unwrap().body);
            }
            Some(msg)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use rand::RngCore;

    use super::*;

    #[test]
    fn fragment_header() {
        let header = FragmentHeader {
            set_id: FragmentSetId::new(),
            last_fragment_id: FragmentId(19),
            fragment_id: FragmentId(0),
        };
        let bz = header.bytes();
        assert_eq!(FragmentHeader::SIZE, bz.len());
        assert_eq!(header, FragmentHeader::from_bytes(bz.as_slice()).unwrap());
    }

    #[test]
    fn fragment() {
        let fragment = Fragment {
            header: FragmentHeader {
                set_id: FragmentSetId::new(),
                last_fragment_id: FragmentId(19),
                fragment_id: FragmentId(0),
            },
            body: vec![1, 2, 3, 4],
        };
        let bz = fragment.bytes();
        assert_eq!(FragmentHeader::SIZE + fragment.body.len(), bz.len());
        assert_eq!(fragment, Fragment::from_bytes(bz.as_slice()).unwrap());
    }

    #[test]
    fn fragment_set() {
        let mut msg = vec![0u8; FragmentSet::CHUNK_SIZE * 3 + FragmentSet::CHUNK_SIZE / 2];
        rand::thread_rng().fill_bytes(&mut msg);

        assert_eq!(4, FragmentSet::num_chunks(&msg));

        let set = FragmentSet::new(&msg).unwrap();
        assert_eq!(4, set.as_ref().iter().len());
        assert_eq!(
            1,
            HashSet::<FragmentSetId>::from_iter(
                set.as_ref().iter().map(|fragment| fragment.header.set_id)
            )
            .len()
        );
        set.as_ref()
            .iter()
            .enumerate()
            .for_each(|(i, fragment)| assert_eq!(i, fragment.header.fragment_id.0 as usize));
    }

    #[test]
    fn message_reconstructor() {
        let mut msg = vec![0u8; FragmentSet::CHUNK_SIZE * 2];
        rand::thread_rng().fill_bytes(&mut msg);

        let set = FragmentSet::new(&msg).unwrap();

        let mut reconstructor = MessageReconstructor::new();
        let mut fragments = set.as_ref().iter();
        assert_eq!(None, reconstructor.add(fragments.next().unwrap().clone()));
        assert_eq!(
            Some(msg),
            reconstructor.add(fragments.next().unwrap().clone())
        );
    }
}
