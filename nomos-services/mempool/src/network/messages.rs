// std
// crates
use serde::{Deserialize, Serialize};
// internal

#[derive(Serialize, Deserialize)]
pub struct PayloadMsg<Payload> {
    pub payload: Payload,
}
