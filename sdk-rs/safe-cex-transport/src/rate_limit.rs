use crate::{RestAdapter, RestExecution, RestRequest, RestResponse, TransportFuture};
use std::collections::VecDeque;
use std::future::poll_fn;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

/// Host clock and async sleep keep request throttling usable in native and browser workers.
pub trait RateLimitClock {
    fn now_ms(&self) -> u64;
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()>;
}
#[derive(Default)]
struct State {
    start: u64,
    count: usize,
    next_ticket: u64,
    queue: VecDeque<(u64, Option<Waker>)>,
}
pub struct RateLimitedAdapter {
    inner: Arc<dyn RestAdapter>,
    limiter: RateLimiter,
}
/// A FIFO rate slot acquired before request signing, shared by adapter and native clients.
pub struct RateLimiter {
    clock: Arc<dyn RateLimitClock>,
    max_requests: usize,
    per_milliseconds: u64,
    state: Mutex<State>,
}
struct QueueGuard<'a> {
    state: &'a Mutex<State>,
    ticket: u64,
}
impl Drop for QueueGuard<'_> {
    fn drop(&mut self) {
        let waker = {
            let mut state = self.state.lock().expect("rate limit lock");
            state.queue.retain(|(ticket, _)| *ticket != self.ticket);
            state.queue.front_mut().and_then(|(_, waker)| waker.take())
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}
impl RateLimitedAdapter {
    pub fn new(
        inner: Arc<dyn RestAdapter>,
        clock: Arc<dyn RateLimitClock>,
        max_requests: usize,
        per_milliseconds: u64,
    ) -> Self {
        Self {
            inner,
            limiter: RateLimiter::new(clock, max_requests, per_milliseconds),
        }
    }
}
impl RateLimiter {
    pub fn new(clock: Arc<dyn RateLimitClock>, max_requests: usize, per_milliseconds: u64) -> Self {
        assert!(max_requests > 0 && per_milliseconds > 0);
        Self {
            clock,
            max_requests,
            per_milliseconds,
            state: Mutex::new(State::default()),
        }
    }
    /// Reserve one dispatch slot; dropping a pending acquire releases its FIFO ticket.
    pub async fn acquire(&self) -> Result<(), crate::TransportError> {
        let ticket = {
            let mut state = self.state.lock().expect("rate limit lock");
            let ticket = state.next_ticket;
            state.next_ticket += 1;
            state.queue.push_back((ticket, None));
            ticket
        };
        let guard = QueueGuard {
            state: &self.state,
            ticket,
        };
        loop {
            let wait = poll_fn(|cx| {
                let now = self.clock.now_ms();
                let mut state = self.state.lock().expect("rate limit lock");
                if state
                    .queue
                    .front()
                    .is_none_or(|(front, _)| *front != ticket)
                {
                    if let Some((_, waker)) =
                        state.queue.iter_mut().find(|(queued, _)| *queued == ticket)
                    {
                        *waker = Some(cx.waker().clone());
                    }
                    return Poll::Pending;
                }
                if state.count == 0 || now.saturating_sub(state.start) >= self.per_milliseconds {
                    state.start = now;
                    state.count = 0;
                }
                if state.count < self.max_requests {
                    state.count += 1;
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(
                        self.per_milliseconds
                            .saturating_sub(now.saturating_sub(state.start)),
                    ))
                }
            })
            .await;
            if let Some(wait) = wait {
                self.clock.sleep_ms(wait).await?;
            } else {
                break;
            }
        }
        drop(guard);
        Ok(())
    }
}
impl RestAdapter for RateLimitedAdapter {
    fn query_in_url(&self) -> bool {
        self.inner.query_in_url()
    }
    fn execute(&self, request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.execute_with_receipt(request)
                .await
                .map(|execution| execution.response)
        })
    }
    fn execute_with_receipt(&self, request: RestRequest) -> crate::RestFuture<'_, RestExecution> {
        Box::pin(async move {
            self.limiter
                .acquire()
                .await
                .map_err(|error| crate::RestFailure::from_transport(error, request.clone()))?;
            self.inner.execute_with_receipt(request).await
        })
    }
}
