use futures::{stream::FuturesUnordered, Future, Stream, StreamExt};
use std::collections::HashMap;
use std::hash::Hash;
use std::pin::Pin;
use tokio::select;
use tokio_util::sync::CancellationToken;

pub struct TaskManager<Group, Out> {
    tasks: FuturesUnordered<Pin<Box<dyn Future<Output = Option<Out>> + Send>>>,
    cancel_cache: CancelCache<Group>,
}

impl<Group, Out> TaskManager<Group, Out>
where
    Group: Eq + PartialEq + Hash + 'static,
    Out: 'static,
{
    pub fn new() -> Self {
        Self {
            tasks: FuturesUnordered::new(),
            cancel_cache: CancelCache::new(),
        }
    }

    pub fn push(&mut self, group: Group, task: impl Future<Output = Out> + Send + 'static) {
        self.tasks.push(Box::pin(
            self.cancel_cache.cancelable_event_future(group, task),
        ));
    }

    pub fn cancel(&mut self, group: Group) {
        self.cancel_cache.cancel(group);
    }
}

impl<Group: Unpin, Out> Stream for TaskManager<Group, Out> {
    type Item = Out;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        let tasks = &mut self.get_mut().tasks;

        loop {
            match tasks.poll_next_unpin(cx) {
                // we need to remove the outer Option that was inserted by the cancelabl future
                Poll::Ready(Some(Some(event))) => return Poll::Ready(Some(event)),
                // an empty output means the task was cancelled, ignore it
                Poll::Ready(Some(None)) => {}
                Poll::Ready(None) => return Poll::Ready(None),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

pub struct GroupCancel(CancellationToken);

impl GroupCancel {
    pub fn new() -> Self {
        Self(CancellationToken::new())
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.0.clone()
    }
}

impl Drop for GroupCancel {
    fn drop(&mut self) {
        if !self.0.is_cancelled() {
            self.cancel();
        }
    }
}

pub struct CancelCache<Group> {
    cancels: HashMap<Group, GroupCancel>,
}

impl<Group: Eq + PartialEq + Hash> CancelCache<Group> {
    pub fn new() -> Self {
        Self {
            cancels: HashMap::new(),
        }
    }

    pub fn cancel(&mut self, group: Group) {
        if let Some(cancel) = self.cancels.remove(&group) {
            cancel.cancel();
        }
    }

    pub fn cancel_token(&mut self, group: Group) -> CancellationToken {
        self.cancels
            .entry(group)
            .or_insert_with(GroupCancel::new)
            .cancel_token()
    }

    pub(crate) fn cancelable_event_future<Out, F: Future<Output = Out>>(
        &mut self,
        group: Group,
        f: F,
    ) -> impl Future<Output = Option<Out>> {
        let token = self.cancel_token(group);
        async move {
            select! {
                event = f => Some(event),
                _ = token.cancelled() => None,
            }
        }
    }
}
