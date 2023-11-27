// std
use std::fmt::Debug;
use std::hash::Hash;

// crates
use axum::extract::{Query, State};
use axum::response::Response;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal
use nomos_api::http::storage;
use nomos_core::block::BlockId;
use nomos_core::tx::Transaction;
use nomos_node::make_request_and_return_response;
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;

#[derive(Deserialize)]
pub(crate) struct QueryParams {
    blocks: Vec<BlockId>,
}
pub(crate) async fn store_blocks<Tx, S>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<QueryParams>,
) -> Response
where
    Tx: Transaction
        + Clone
        + Debug
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    S: StorageSerde + Send + Sync + 'static,
{
    let QueryParams { blocks } = query;
    let results: Vec<_> = blocks
        .into_iter()
        .map(|id| storage::block_req::<S, Tx>(&store, id))
        .collect();
    make_request_and_return_response!(futures::future::try_join_all(results))
}
