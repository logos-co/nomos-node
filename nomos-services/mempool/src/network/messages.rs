use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PayloadMsg<Payload> {
    pub payload: Payload,
}
