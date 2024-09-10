// Networking is not essential for verifier and indexer tests.
// Libp2p network is chosen for consensus requirement, mixnet is ignored.

//#[cfg(all(test, feature = "libp2p", not(feature = "mixnet")))]
mod common;
//#[cfg(all(test, feature = "libp2p", not(feature = "mixnet")))]
mod indexer_integration;
//#[cfg(all(test, feature = "libp2p", not(feature = "mixnet")))]
mod service_wrapper;
//#[cfg(all(test, feature = "libp2p", not(feature = "mixnet")))]
mod verifier_integration;
