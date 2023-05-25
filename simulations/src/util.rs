/// Create a random node id.
///
/// The format is:
///
/// [0..4]: node index in big endian
/// [4..32]: zeros
pub fn node_id(id: usize) -> consensus_engine::NodeId {
    let mut bytes = [0; 32];
    bytes[..4].copy_from_slice((id as u32).to_be_bytes().as_ref());
    bytes
}

/// Parse the original index from NodeId
pub(crate) fn parse_idx(id: &consensus_engine::NodeId) -> usize {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(&id[..4]);
    u32::from_be_bytes(bytes) as usize
}


pub(crate) mod millis_duration {
    
}