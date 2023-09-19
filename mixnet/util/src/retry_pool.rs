use std::{
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use itertools::{Either, Itertools};
use tokio::sync::Mutex;

pub struct RetryMessage<M> {
    delay: Duration,
    now: Instant,
    msg: M,
}

impl<M> RetryMessage<M> {
    pub fn into_components(self) -> (Duration, M) {
        (self.delay, self.msg)
    }
}

impl<M> PartialEq for RetryMessage<M> {
    fn eq(&self, other: &Self) -> bool {
        self.remaining() == other.remaining()
    }
}

impl<M> Eq for RetryMessage<M> {}

impl<M> PartialOrd for RetryMessage<M> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<M> Ord for RetryMessage<M> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.remaining().cmp(&other.remaining())
    }
}

impl<M> RetryMessage<M> {
    pub fn new(delay: Duration, msg: M) -> Self {
        Self {
            delay,
            msg,
            now: Instant::now(),
        }
    }

    pub fn delay(&self) -> Duration {
        self.delay
    }

    pub fn msg(&self) -> &M {
        &self.msg
    }

    pub fn should_retry(&self) -> bool {
        self.now.elapsed() >= self.delay
    }

    pub fn remaining(&self) -> Duration {
        self.delay.saturating_sub(self.now.elapsed())
    }
}

pub enum OneOrMore<M> {
    One((Duration, M)),
    More(Vec<(Duration, M)>),
}

pub struct RetryPool<M> {
    pool: Mutex<Vec<RetryMessage<M>>>,
}

impl<M> Default for RetryPool<M> {
    fn default() -> Self {
        Self {
            pool: Mutex::new(Vec::new()),
        }
    }
}

impl<M> RetryPool<M> {
    pub fn new() -> Self {
        Self {
            pool: Mutex::new(Vec::new()),
        }
    }

    pub async fn get(&self) -> GetFuture<M> {
        let mut pool = self.pool.lock().await;

        if pool.is_empty() {
            return GetFuture { res: None };
        }

        let msgs = std::mem::take(&mut *pool);
        let (mut res, mut remaining): (Vec<_>, Vec<_>) = msgs.into_iter().partition_map(|v| {
            if v.should_retry() {
                Either::Left(v.into_components())
            } else {
                Either::Right(v)
            }
        });
        remaining.sort();
        *pool = remaining;
        match res.len() {
            0 => GetFuture { res: None },
            1 => GetFuture {
                res: Some(OneOrMore::One(res.pop().unwrap())),
            },
            _ => GetFuture {
                res: Some(OneOrMore::More(res)),
            },
        }
    }

    pub async fn insert(&self, msg: M, delay: Duration) {
        let mut pool = self.pool.lock().await;
        pool.push(RetryMessage::new(delay, msg));
    }

    pub async fn insert_many(&self, msgs: impl Iterator<Item = (Duration, M)>) {
        let mut pool = self.pool.lock().await;
        pool.extend(msgs.map(|(delay, msg)| RetryMessage::new(delay, msg)));
    }
}

pub struct GetFuture<M> {
    res: Option<OneOrMore<M>>,
}

impl<M> std::future::Future for GetFuture<M> {
    type Output = OneOrMore<M>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(result) = self.res.take() {
            Poll::Ready(result)
        } else {
            Poll::Pending
        }
    }
}

// This is correct, because we never pin res field.
impl<M> Unpin for GetFuture<M> {}
