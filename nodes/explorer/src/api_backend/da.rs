// crates
use axum::extract::{Json, Query, State};
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use nomos_node::make_request_and_return_response;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// internal
use full_replication::{Blob, Certificate};
use nomos_api::http::da::da_blobs;
use nomos_core::{da::blob, tx::Transaction};
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub(crate) async fn blobs(
    State(handle): State<OverwatchHandle>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> Response {
    make_request_and_return_response!(da_blobs(&handle, items))
}
