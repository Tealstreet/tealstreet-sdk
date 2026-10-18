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
    reset: Option<TransportFuture<'static, ()>>,
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
        let result = poll_fn(|cx| {
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
            if let Some(reset) = state.reset.as_mut() {
                match reset.as_mut().poll(cx) {
                    Poll::Ready(result) => {
                        state.reset = None;
                        state.count = 0;
                        if let Err(error) = result {
                            return Poll::Ready(Err(error));
                        }
                    }
                    Poll::Pending => {}
                }
            }
            if state.count == self.max_requests {
                return Poll::Pending;
            }
            state.count += 1;
            if state.count == 1 {
                let clock = self.clock.clone();
                let delay = self.per_milliseconds;
                let mut reset: TransportFuture<'static, ()> =
                    Box::pin(async move { clock.sleep_ms(delay).await });
                // Register at the first dispatch; later clock reads cannot
                // postpone an elapsed window or strand a frozen replay clock.
                match reset.as_mut().poll(cx) {
                    Poll::Pending => state.reset = Some(reset),
                    Poll::Ready(result) => {
                        state.count = 0;
                        if let Err(error) = result {
                            return Poll::Ready(Err(error));
                        }
                    }
                }
            }
            Poll::Ready(Ok(()))
        })
        .await;
        drop(guard);
        result
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
