pub mod consensus;
pub mod da;
pub mod mempool;
pub mod storage;

use once_cell::sync::Lazy;
use reqwest::Client;

static CLIENT: Lazy<Client> = Lazy::new(Client::new);
