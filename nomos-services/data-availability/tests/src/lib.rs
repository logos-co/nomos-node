// Networking is not essential for verifier and indexer tests.
// Libp2p network is chosen for consensus requirement, mixnet is ignored.
//
// Note: To enable rust-analyzer in modules, comment out the
// `#[cfg(not(feature = "mixnet"))]` lines (reenable when pushing).

#[cfg(test)]
#[cfg(feature = "libp2p")]
mod common;

#[cfg(test)]
#[cfg(feature = "libp2p")]
mod indexer_integration;

#[cfg(test)]
#[cfg(feature = "libp2p")]
mod verifier_integration;

#[cfg(test)]
mod rng;
