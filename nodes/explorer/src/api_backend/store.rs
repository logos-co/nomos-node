// std
use std::fmt::Debug;
use std::hash::Hash;

// crates
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use either::Either;
use hyper::StatusCode;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal
use full_replication::Certificate;
use nomos_api::http::storage;
use nomos_core::block::{Block, BlockId};
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

type Depth = usize;

#[derive(Deserialize)]
pub(crate) struct BlocksByIdQueryParams {
    from: BlockId,
    to: Option<BlockId>,
}

pub(crate) async fn blocks<Tx, S>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<BlocksByIdQueryParams>,
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
    let BlocksByIdQueryParams { from, to } = query;
    // get the from block
    let from = match storage::block_req::<S, Tx>(&store, from).await {
        Ok(from) => match from {
            Some(from) => from,
            None => {
                return IntoResponse::into_response((StatusCode::NOT_FOUND, "from block not found"))
            }
        },
        Err(e) => {
            return IntoResponse::into_response((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    };

    // check if to is valid
    match to {
        Some(to) => match storage::block_req::<S, Tx>(&store, to).await {
            Ok(to) => match to {
                Some(to) => handle_to::<S, Tx>(store, from, Some(to)).await,
                None => IntoResponse::into_response((StatusCode::NOT_FOUND, "to block not found")),
            },
            Err(e) => {
                IntoResponse::into_response((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
            }
        },
        None => handle_to::<S, Tx>(store, from, None).await,
    }
}

async fn handle_to<S, Tx>(
    store: OverwatchHandle,
    from: Block<Tx, Certificate>,
    to: Option<Block<Tx, Certificate>>,
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
    let mut current = Some(from.header().parent());
    let mut blocks = Vec::new();
    while let Some(id) = current {
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
                current = Some(block.header().parent());
                blocks.push(block);
            }
            None => {
                current = None;
            }
        }
    }

    IntoResponse::into_response((StatusCode::OK, ::axum::Json(blocks)))
}

#[derive(Deserialize)]
pub(crate) struct BlocksByDepthQueryParams {
    from: BlockId,
    #[serde(default = "default_depth")]
    depth: usize,
}

fn default_depth() -> usize {
    500
}

pub(crate) async fn block_depth<Tx, S>(
    State(store): State<OverwatchHandle>,
    Query(query): Query<BlocksByDepthQueryParams>,
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
    let BlocksByDepthQueryParams { from, depth } = query;
    // get the from block
    let from = match storage::block_req::<S, Tx>(&store, from).await {
        Ok(from) => match from {
            Some(from) => from,
            None => {
                return IntoResponse::into_response((StatusCode::NOT_FOUND, "from block not found"))
            }
        },
        Err(e) => {
            return IntoResponse::into_response((StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
        }
    };

    let mut current = Some(from.header().parent());
    let mut blocks = Vec::new();
    while blocks.len() < depth {
        if let Some(id) = current {
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
                    current = Some(block.header().parent());
                    blocks.push(block);
                }
                None => {
                    current = None;
                }
            }
        } else {
            break;
        }
    }

    IntoResponse::into_response((StatusCode::OK, ::axum::Json(blocks)))
}
