// crates
use axum::extract::{Json, State};
use axum::response::Response;
use nomos_node::make_request_and_return_response;
// internal
use full_replication::Blob;
use nomos_api::http::da::da_blobs;
use nomos_core::da::blob;
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub(crate) async fn blobs(
    State(handle): State<OverwatchHandle>,
    Json(items): Json<Vec<<Blob as blob::Blob>::Hash>>,
) -> Response {
    make_request_and_return_response!(da_blobs(&handle, items))
}
