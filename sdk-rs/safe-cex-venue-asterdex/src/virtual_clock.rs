//! Source clock lifecycle, with an explicit native realm and HTTP capability.
//! The runtime owns pending completions; the exchange owns monotonic intervals.
//! Stopping a generation suppresses late state writes without stranding waiters.
use crate::{clock_helpers, tables::ASTERDEX_ENDPOINTS, types::AsterDEXAPIVersion};
use safe_cex_exchange::{
    OwnedOperation,
    eager::start_eager,
    exchange::BaseExchange,
    runtime::{ExchangeError, ExchangeFuture},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

/// Axios.get / withPublicCorsProxy host boundaries. `get` dispatches on call,
/// not on the first poll; the future resolves to the response's decoded data.
pub trait ClockHttp {
    fn get(&self, url: String, timeout_ms: u64) -> ExchangeFuture<'static, serde_json::Value>;
    fn with_public_cors_proxy(&self, url: &str) -> Result<String, ExchangeError>;
}

struct Completion<T> {
    result: RefCell<Option<Result<T, ExchangeError>>>,
    waiters: RefCell<Vec<Waker>>,
}
/// A shareable completion, like a JS Promise: dropping a waiter does not abort
/// the operation, and each waiter receives the same result.
/// A stopped generation still settles its existing waiters.
pub struct ClockOperation<T>(Rc<Completion<T>>);
impl<T> Clone for ClockOperation<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<T: Clone> Future for ClockOperation<T> {
    type Output = Result<T, ExchangeError>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(result) = self.0.result.borrow().as_ref() {
            return Poll::Ready(result.clone());
        }
        let mut waiters = self.0.waiters.borrow_mut();
        if !waiters.iter().any(|w| w.will_wake(cx.waker())) {
            waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}
impl<T> ClockOperation<T> {
    fn pending() -> Self {
        Self(Rc::new(Completion {
            result: RefCell::new(None),
            waiters: RefCell::new(Vec::new()),
        }))
    }
    fn finish(&self, result: Result<T, ExchangeError>) {
        *self.0.result.borrow_mut() = Some(result);
        for waker in self.0.waiters.take() {
            waker.wake();
        }
    }
}

// ts: AsterDEXVirtualClock
pub struct AsterDEXVirtualClock {
    owner: Rc<BaseExchange>,
    http: Rc<dyn ClockHttp>,
    server: Cell<Option<f64>>,
    client: Cell<Option<f64>>,
    sync_interval: RefCell<Option<OwnedOperation>>,
    sync_in_flight: RefCell<Option<ClockOperation<()>>>,
    lifecycle_version: Cell<f64>,
    preferred_api_version: RefCell<AsterDEXAPIVersion>,
}
impl AsterDEXVirtualClock {
    pub fn new(owner: Rc<BaseExchange>, http: Rc<dyn ClockHttp>) -> Rc<Self> {
        Rc::new(Self {
            owner,
            http,
            server: Cell::new(None),
            client: Cell::new(None),
            sync_interval: RefCell::new(None),
            sync_in_flight: RefCell::new(None),
            lifecycle_version: Cell::new(0.0),
            preferred_api_version: RefCell::new(AsterDEXAPIVersion::V1),
        })
    }
    fn next_generation(&self) -> f64 {
        // Source ++ is a Number operation, including saturation at 2^53.
        let next = self.lifecycle_version.get() + 1.0;
        self.lifecycle_version.set(next);
        next
    }
    fn launch<T: Clone + 'static>(&self, work: ExchangeFuture<'static, T>) -> ClockOperation<T> {
        let operation = ClockOperation::pending();
        self.complete(operation.clone(), work);
        operation
    }
    fn complete<T: 'static>(&self, operation: ClockOperation<T>, work: ExchangeFuture<'static, T>) {
        drop(start_eager(&self.owner.runtime, async move {
            operation.finish(work.await);
            Ok(())
        }));
    }
    // ts: AsterDEXVirtualClock.start
    pub fn start(
        self: &Rc<Self>,
        testnet: bool,
        preferred: AsterDEXAPIVersion,
    ) -> ClockOperation<()> {
        let generation = self.next_generation();
        *self.preferred_api_version.borrow_mut() = preferred.clone();
        let syncing = self.sync_client_server_time(testnet, Some(preferred.clone()));
        let clock = self.clone();
        self.launch(Box::pin(async move {
            syncing.await?;
            if generation != clock.lifecycle_version.get() {
                return Ok(());
            }
            let previous = clock.sync_interval.borrow_mut().take();
            if let Some(interval) = previous {
                interval.cancel();
            }
            let weak = Rc::downgrade(&clock);
            let interval = clock.owner.start_interval(300_000, move || {
                if let Some(clock) = weak.upgrade()
                    && generation == clock.lifecycle_version.get()
                {
                    // Source deliberately discards the Promise; ownership keeps it running.
                    clock.sync_client_server_time(testnet, Some(preferred.clone()));
                }
                Ok(())
            })?;
            *clock.sync_interval.borrow_mut() = Some(interval);
            Ok(())
        }))
    }
    // ts: AsterDEXVirtualClock.stop
    pub fn stop(&self) {
        self.next_generation();
        let interval = self.sync_interval.borrow_mut().take();
        if let Some(interval) = interval {
            interval.cancel();
        }
        self.server.set(None);
        self.client.set(None);
        // Deliberately leave sync_in_flight untouched, as in the source.
    }
    // ts: AsterDEXVirtualClock.syncClientServerTime
    pub fn sync_client_server_time(
        self: &Rc<Self>,
        testnet: bool,
        preferred: Option<AsterDEXAPIVersion>,
    ) -> ClockOperation<()> {
        if let Some(operation) = self.sync_in_flight.borrow().as_ref() {
            return operation.clone();
        }
        let preferred = preferred.unwrap_or_else(|| self.preferred_api_version.borrow().clone());
        let generation = self.lifecycle_version.get();
        let operation = ClockOperation::pending();
        *self.sync_in_flight.borrow_mut() = Some(operation.clone());
        let work = self.sync_client_server_time_inner(testnet, preferred, generation);
        let clock = self.clone();
        self.complete(
            operation.clone(),
            Box::pin(async move {
                let result = work.await;
                clock.sync_in_flight.borrow_mut().take();
                result
            }),
        );
        operation
    }
    // ts: AsterDEXVirtualClock.syncClientServerTimeInner
    pub fn sync_client_server_time_inner(
        self: &Rc<Self>,
        testnet: bool,
        preferred: AsterDEXAPIVersion,
        generation: f64,
    ) -> ExchangeFuture<'static, ()> {
        *self.preferred_api_version.borrow_mut() = preferred.clone();
        let fetching = self.fetch_server_time(testnet, Some(preferred));
        let clock = self.clone();
        Box::pin(async move {
            let server = fetching.await?;
            let Some(server) = server else {
                return Ok(());
            };
            if generation != clock.lifecycle_version.get() {
                return Ok(());
            }
            clock.server.set(Some(server));
            clock.client.set(Some(safe_cex_num::js_time_clip(
                clock.owner.runtime.now_ms(),
            )));
            let offset = safe_cex_num::js_number_string(clock.get_time_offset());
            clock.owner.runtime.console(
                "log",
                &format!("[AsterDEX] Time sync: Server time offset is {offset}ms"),
            );
            Ok(())
        })
    }
    // ts: AsterDEXVirtualClock.fetchServerTime
    pub fn fetch_server_time(
        &self,
        testnet: bool,
        preferred: Option<AsterDEXAPIVersion>,
    ) -> ExchangeFuture<'static, Option<f64>> {
        let preferred = preferred.unwrap_or_else(|| self.preferred_api_version.borrow().clone());
        let key = if testnet {
            "REST.TESTNET"
        } else {
            "REST.PRODUCTION"
        };
        let base = ASTERDEX_ENDPOINTS
            .iter()
            .find(|(name, _)| *name == key)
            .expect("clock REST endpoint")
            .1;
        let paths = clock_helpers::get_time_path_candidates(preferred);
        let urls = paths.map(|path| format!("{base}{path}"));
        let http = self.http.clone();
        let first = http.get(urls[0].clone(), 5_000);
        Box::pin(async move {
            // Complete both direct candidates before trying either proxy candidate.
            let mut first = Some(first);
            for proxy in [false, true] {
                for url in &urls {
                    let request = if let Some(request) = first.take() {
                        request
                    } else {
                        let target = if proxy {
                            match http.with_public_cors_proxy(url) {
                                Ok(url) => url,
                                Err(_) => continue,
                            }
                        } else {
                            url.clone()
                        };
                        http.get(target, 5_000)
                    };
                    if let Ok(data) = request.await
                        && let Some(time) = clock_helpers::parse_server_time(data.get("serverTime"))
                    {
                        // A finite Number outside Date's range becomes an invalid,
                        // but truthy, Dayjs object. Do not fall through to fallback.
                        return Ok(Some(safe_cex_num::js_time_clip(time)));
                    }
                }
            }
            Ok(None)
        })
    }
    // ts: AsterDEXVirtualClock.forceSyncAndGetOffset
    pub fn force_sync_and_get_offset(
        self: &Rc<Self>,
        testnet: bool,
        preferred: Option<AsterDEXAPIVersion>,
    ) -> ClockOperation<f64> {
        let syncing = self.sync_client_server_time(testnet, preferred);
        let clock = self.clone();
        self.launch(Box::pin(async move {
            syncing.await?;
            Ok(clock.get_time_offset())
        }))
    }
    pub fn get_time_offset(&self) -> f64 {
        clock_helpers::get_time_offset(self.server.get(), self.client.get())
    }
    pub fn get_current_time_ms(&self) -> f64 {
        clock_helpers::get_current_time_ms(
            self.server.get(),
            self.client.get(),
            self.owner.runtime.now_ms(),
        )
    }
    pub fn needs_sync(&self) -> bool {
        clock_helpers::needs_sync(
            self.server.get(),
            self.client.get(),
            self.owner.runtime.now_ms(),
        )
    }
    /// Observe the shared Promise slot, independently of HTTP request progress.
    pub fn is_syncing(&self) -> bool {
        self.sync_in_flight.borrow().is_some()
    }
}

/// One instance per explicitly constructed native realm; callers sharing a
/// realm share the clock. Unlike a JS module singleton, separate realms isolate it.
pub struct AsterDEXClockRealm {
    clock: Rc<AsterDEXVirtualClock>,
}
impl AsterDEXClockRealm {
    pub fn new(owner: Rc<BaseExchange>, http: Rc<dyn ClockHttp>) -> Self {
        Self {
            clock: AsterDEXVirtualClock::new(owner, http),
        }
    }
}
// ts: asterDEXVirtualClock
pub fn aster_dex_virtual_clock(realm: &AsterDEXClockRealm) -> Rc<AsterDEXVirtualClock> {
    realm.clock.clone()
}
