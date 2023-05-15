use consensus_engine::View;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

pub struct ViewCancel(CancellationToken);

impl ViewCancel {
    pub fn new() -> Self {
        ViewCancel(CancellationToken::new())
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.0.clone()
    }
}

impl Drop for ViewCancel {
    fn drop(&mut self) {
        if !self.0.is_cancelled() {
            self.cancel();
        }
    }
}

pub struct ViewCancelCache {
    cancels: HashMap<View, ViewCancel>,
}

impl ViewCancelCache {
    pub fn new() -> Self {
        ViewCancelCache {
            cancels: HashMap::new(),
        }
    }

    pub fn cancel(&mut self, view: View) {
        if let Some(cancel) = self.cancels.remove(&view) {
            cancel.cancel();
        }
    }

    pub fn cancel_token(&mut self, view: View) -> CancellationToken {
        self.cancels
            .entry(view)
            .or_insert_with(ViewCancel::new)
            .cancel_token()
    }
}
