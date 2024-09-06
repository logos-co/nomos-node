// Networking is not essential for verifier and indexer tests.
// Libp2p network is chosen for consensus requirement, mixnet is ignored.
//
// Note: To enable rust-analyzer in modules, comment out the
// `#[cfg(not(feature = "mixnet"))]` lines (reenable when pushing).

#[cfg(test)]
#[cfg(feature = "libp2p")]
#[cfg(not(feature = "mixnet"))]
mod common;

#[cfg(test)]
#[cfg(feature = "libp2p")]
#[cfg(not(feature = "mixnet"))]
mod indexer_integration;

#[cfg(test)]
#[cfg(feature = "libp2p")]
#[cfg(not(feature = "mixnet"))]
mod verifier_integration;

#[cfg(test)]
mod rng;
