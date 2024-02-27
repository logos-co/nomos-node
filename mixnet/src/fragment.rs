use std::{collections::HashMap, ops::Deref};

use sphinx_packet::{constants::PAYLOAD_SIZE, payload::PAYLOAD_OVERHEAD_SIZE};
use uuid::Uuid;

use crate::error::MixnetError;

pub(crate) struct FragmentSet(Vec<Fragment>);

const MAX_PLAIN_PAYLOAD_SIZE: usize = PAYLOAD_SIZE - PAYLOAD_OVERHEAD_SIZE;

impl FragmentSet {
    pub(crate) fn new(msg: &[u8]) -> Result<Self, MixnetError> {
        let chunk_size = MAX_PLAIN_PAYLOAD_SIZE - FRAGMENT_HEADER_SIZE;

        // For now, we don't support more than max_fragments() fragments.
        // If needed, we can devise the FragmentSet chaining to support larger messages, like Nym.
        let num_chunks = Self::num_chunks(msg, chunk_size);
        if num_chunks > MAX_NUM_FRAGMENTS {
            return Err(MixnetError::MessageTooLong(msg.len()));
        }

        let set_id = Uuid::new_v4();
        Ok(FragmentSet(
            msg.chunks(chunk_size)
                .enumerate()
                .map(|(i, chunk)| Fragment {
                    header: FragmentHeader {
                        set_id,
                        total_fragments: num_chunks as FragmentId,
                        fragment_id: i as FragmentId,
                    },
                    body: Vec::from(chunk),
                })
                .collect(),
        ))
    }

    fn num_chunks(msg: &[u8], chunk_size: usize) -> usize {
        msg.len() / chunk_size + (msg.len() % chunk_size > 0) as usize
    }
}

impl Deref for FragmentSet {
    type Target = Vec<Fragment>;

    fn deref(&self) -> &Self::Target {
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
        let mut out = Vec::with_capacity(FRAGMENT_HEADER_SIZE + self.body.len());
        out.extend(self.header.bytes());
        out.extend(&self.body);
        out
    }

    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        Ok(Self {
            header: FragmentHeader::from_bytes(&value[0..FRAGMENT_HEADER_SIZE])?,
            body: value[FRAGMENT_HEADER_SIZE..].to_vec(),
        })
    }
}

type FragmentSetId = Uuid;
type FragmentId = u8;
const MAX_NUM_FRAGMENTS: usize = FragmentId::MAX as usize + 1;
const FRAGMENT_HEADER_SIZE: usize = 18;

#[derive(PartialEq, Eq, Debug, Clone)]
struct FragmentHeader {
    set_id: FragmentSetId,
    total_fragments: FragmentId,
    fragment_id: FragmentId,
}

impl FragmentHeader {
    fn bytes(&self) -> [u8; FRAGMENT_HEADER_SIZE] {
        let mut out = [0u8; FRAGMENT_HEADER_SIZE];
        out[0..16].copy_from_slice(self.set_id.as_bytes());
        out[16] = self.total_fragments;
        out[17] = self.fragment_id;
        out
    }

    fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        if value.len() != FRAGMENT_HEADER_SIZE {
            return Err(MixnetError::InvalidFragmentHeader);
        }

        Ok(Self {
            set_id: Uuid::from_slice(&value[0..16])?,
            total_fragments: value[16],
            fragment_id: value[17],
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
                fragment.header.total_fragments,
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
    total_fragments: FragmentId,
    fragments: HashMap<FragmentId, Fragment>,
    // For mem optimization, accumulates the expected message size
    // whenever a new fragment is added to the `fragments`.
    message_size: usize,
}

impl FragmentSetReconstructor {
    fn new(total_fragments: FragmentId) -> Self {
        Self {
            total_fragments,
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
        if self.fragments.len() == self.total_fragments as usize {
            let mut msg = Vec::with_capacity(self.message_size);
            for i in 0..self.total_fragments {
                msg.extend(&self.fragments.get(&i).unwrap().body);
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
            set_id: Uuid::new_v4(),
            total_fragments: 20,
            fragment_id: 0,
        };
        let bz = header.bytes();
        assert_eq!(FRAGMENT_HEADER_SIZE, bz.len());
        assert_eq!(header, FragmentHeader::from_bytes(bz.as_slice()).unwrap());
    }

    #[test]
    fn fragment() {
        let fragment = Fragment {
            header: FragmentHeader {
                set_id: Uuid::new_v4(),
                total_fragments: 20,
                fragment_id: 0,
            },
            body: vec![1, 2, 3, 4],
        };
        let bz = fragment.bytes();
        assert_eq!(FRAGMENT_HEADER_SIZE + fragment.body.len(), bz.len());
        assert_eq!(fragment, Fragment::from_bytes(bz.as_slice()).unwrap());
    }

    #[test]
    fn fragment_set() {
        let chunk_size = MAX_PLAIN_PAYLOAD_SIZE;
        let mut msg = vec![0u8; chunk_size * 3 + chunk_size / 2];
        rand::thread_rng().fill_bytes(&mut msg);

        assert_eq!(4, FragmentSet::num_chunks(&msg, chunk_size));

        let set = FragmentSet::new(&msg).unwrap();
        assert_eq!(4, set.iter().len());
        assert_eq!(
            1,
            HashSet::<FragmentSetId>::from_iter(set.iter().map(|fragment| fragment.header.set_id))
                .len()
        );
        set.iter()
            .enumerate()
            .for_each(|(i, fragment)| assert_eq!(i, fragment.header.fragment_id.into()));
    }

    #[test]
    fn message_reconstructor() {
        let chunk_size = MAX_PLAIN_PAYLOAD_SIZE - FRAGMENT_HEADER_SIZE;
        let mut msg = vec![0u8; chunk_size * 2];
        rand::thread_rng().fill_bytes(&mut msg);

        let set = FragmentSet::new(&msg).unwrap();

        let mut reconstructor = MessageReconstructor::new();
        let mut fragments = set.iter();
        assert_eq!(None, reconstructor.add(fragments.next().unwrap().clone()));
        assert_eq!(
            Some(msg),
            reconstructor.add(fragments.next().unwrap().clone())
        );
    }
}
