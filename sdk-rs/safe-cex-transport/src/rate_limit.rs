use crate::{RestAdapter, RestExecution, RestRequest, RestResponse, TransportFuture};
use std::collections::VecDeque;
use std::future::poll_fn;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

/// Host clock and async sleep keep request throttling usable in native and browser workers.
pub trait RateLimitClock {
    fn now_ms(&self) -> u64;
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()>;
}
pub trait RateLimitTimer {
    fn cancel(&self);
}
pub trait RateLimitScheduler {
    fn schedule_reset(
        &self,
        milliseconds: u64,
        callback: Box<dyn FnOnce(Result<(), crate::TransportError>)>,
    ) -> Result<Box<dyn RateLimitTimer>, crate::TransportError>;
}
#[derive(Default)]
struct State {
    reset: Option<Box<dyn RateLimitTimer>>,
    window: u64,
    error: Option<crate::TransportError>,
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
    clock: Arc<dyn RateLimitScheduler>,
    max_requests: usize,
    per_milliseconds: u64,
    state: Rc<Mutex<State>>,
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
        clock: Arc<dyn RateLimitScheduler>,
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
    pub fn new(
        clock: Arc<dyn RateLimitScheduler>,
        max_requests: usize,
        per_milliseconds: u64,
    ) -> Self {
        assert!(max_requests > 0 && per_milliseconds > 0);
        Self {
            clock,
            max_requests,
            per_milliseconds,
            state: Rc::new(Mutex::new(State::default())),
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
            if let Some(error) = state.error.take() {
                return Poll::Ready(Err(error));
            }
            if state.count == self.max_requests {
                state.queue.front_mut().expect("front reservation").1 = Some(cx.waker().clone());
                return Poll::Pending;
            }
            state.count += 1;
            if state.count == 1 {
                state.window += 1;
                let window = state.window;
                let owner = Rc::downgrade(&self.state);
                drop(state);
                let reset = self.clock.schedule_reset(
                    self.per_milliseconds,
                    Box::new(move |result| {
                        let Some(owner) = owner.upgrade() else { return };
                        let (timer, wake) = {
                            let mut state = owner.lock().expect("rate limit lock");
                            if state.window != window {
                                return;
                            }
                            state.count = 0;
                            state.error = result.err();
                            let timer = state.reset.take();
                            let wake = state.queue.front_mut().and_then(|(_, waker)| waker.take());
                            (timer, wake)
                        };
                        drop(timer);
                        if let Some(wake) = wake {
                            wake.wake();
                        }
                    }),
                );
                let mut state = self.state.lock().expect("rate limit lock");
                match reset {
                    Ok(reset) => state.reset = Some(reset),
                    Err(error) => {
                        state.count = 0;
                        return Poll::Ready(Err(error));
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
impl Drop for RateLimiter {
    fn drop(&mut self) {
        let reset = self.state.lock().expect("rate limit lock").reset.take();
        if let Some(reset) = reset {
            reset.cancel();
        }
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
