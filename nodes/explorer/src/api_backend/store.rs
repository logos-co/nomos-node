// std
use std::fmt::Debug;
use std::hash::Hash;

// crates
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
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

#[derive(Deserialize)]
pub(crate) struct BlocksQueryParams {
    from: BlockId,
    to: Option<BlockId>,
}

pub(crate) async fn blocks<Tx, S>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<BlocksQueryParams>,
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
    let BlocksQueryParams { from, to } = query;
    // get the from block
    let from = match storage::block_req::<S, Tx>(&store, from).await {
        Ok(from) => match from {
            Some(from) => from,
            None => {
                return IntoResponse::into_response((
                    StatusCode::NOT_FOUND,
                    "from block not found",
                ))
            }
        },
        Err(e) => {
            return IntoResponse::into_response((
                StatusCode::INTERNAL_SERVER_ERROR,
                e.to_string(),
            ))
        }
    };

    // check if to is valid
    let to = match to {
        Some(to) => match storage::block_req::<S, Tx>(&store, to).await {
            Ok(to) => match to {
                Some(to) => Some(to),
                None => {
                    return IntoResponse::into_response((
                        StatusCode::NOT_FOUND,
                        "to block not found",
                    ))
                }
            },
            Err(e) => {
                return IntoResponse::into_response((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                ))
            }
        },
        None => None,
    };

    let mut cur = Some(from.header().parent());
    let mut blocks = Vec::new();
    while let Some(id) = cur {
        if let Some(to) = to {
            if id == to.header().id {
                break;
            }
        }

        let block = match storage::block_req::<S, Tx>(&store, id).await {
            Ok(block) => block,
            Err(e) => {
                return IntoResponse::into_response((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    e.to_string(),
                ))
            }
        };

        match block {
            Some(block) => {
                cur = Some(block.header().parent());
                blocks.push(block);
            }
            None => {
                cur = None;
            }
        }
    }

    IntoResponse::into_response((::hyper::StatusCode::OK, ::axum::Json(blocks)))
}
