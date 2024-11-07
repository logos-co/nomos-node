use blake2::Digest;
use futures::{Stream, StreamExt};
use nomos_mix_message::MixMessage;
use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;
use std::ops::{DerefMut, Div};
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Copy, Clone)]
pub struct CoverTrafficSettings {
    node_id: [u8; 32],
    number_of_hops: usize,
    slots_per_epoch: usize,
    network_size: usize,
}

pub struct CoverTraffic<EpochStream, SlotStream, Message> {
    winning_probability: f64,
    settings: CoverTrafficSettings,
    epoch_stream: EpochStream,
    slot_stream: SlotStream,
    selected_slots: HashSet<u32>,
    _message: PhantomData<Message>,
}

impl<EpochStream, SlotStream, Message> CoverTraffic<EpochStream, SlotStream, Message>
where
    EpochStream: Stream<Item = usize>,
    SlotStream: Stream<Item = usize>,
{
    pub fn new(
        settings: CoverTrafficSettings,
        epoch_stream: EpochStream,
        slot_stream: SlotStream,
    ) -> Self {
        let winning_probability = winning_probability(settings.number_of_hops);
        CoverTraffic {
            winning_probability,
            settings,
            epoch_stream,
            slot_stream,
            selected_slots: Default::default(),
            _message: Default::default(),
        }
    }
}

impl<EpochStream, SlotStream, Message> Stream for CoverTraffic<EpochStream, SlotStream, Message>
where
    EpochStream: Stream<Item = usize> + Unpin,
    SlotStream: Stream<Item = usize> + Unpin,
    Message: MixMessage + Unpin,
{
    type Item = Vec<u8>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self {
            winning_probability,
            settings,
            epoch_stream,
            slot_stream,
            selected_slots,
            ..
        } = self.deref_mut();
        if let Poll::Ready(Some(epoch)) = epoch_stream.poll_next_unpin(cx) {
            *selected_slots = select_slot(
                settings.node_id,
                epoch,
                settings.network_size,
                settings.slots_per_epoch,
                *winning_probability,
            );
        }
        if let Poll::Ready(Some(slot)) = slot_stream.poll_next_unpin(cx) {
            if selected_slots.contains(&(slot as u32)) {
                return Poll::Ready(Some(vec![]));
            }
        }
        Poll::Pending
    }
}

fn generate_ticket<Id: Hash + Eq + AsRef<[u8]>>(node_id: Id, r: usize, slot: usize) -> u32 {
    let mut hasher = blake2::Blake2b512::new();
    hasher.update(node_id);
    hasher.update(r.to_be_bytes());
    hasher.update(slot.to_be_bytes());
    let hash = &hasher.finalize()[..];
    u32::from_be_bytes(hash.try_into().unwrap())
}

fn select_slot<Id: Hash + Eq + AsRef<[u8]> + Copy>(
    node_id: Id,
    r: usize,
    network_size: usize,
    slots_per_epoch: usize,
    winning_probability: f64,
) -> HashSet<u32> {
    let i = (slots_per_epoch as f64).div(network_size as f64) * winning_probability;
    let i = i.ceil() as usize;
    let mut w = HashSet::new();
    while w.len() != i {
        w.insert(generate_ticket(node_id, r, slots_per_epoch));
    }
    w
}

fn winning_probability(number_of_hops: usize) -> f64 {
    1.0 / number_of_hops as f64
}
