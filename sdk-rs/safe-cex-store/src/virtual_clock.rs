use std::collections::BTreeMap;
use std::future::{Future, poll_fn};
use std::ops::Bound::{Excluded, Unbounded};
use std::pin::Pin;
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{SystemTime, UNIX_EPOCH};

// The host parses venue dates and supplies proxy/timeout-aware HTTP and timers.
pub type ServerTimeSource = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Option<f64>, String>> + Send>> + Send + Sync,
>;
type SyncFuture = Pin<Box<dyn Future<Output = ()> + Send>>;
type Listener = Arc<dyn Fn(f64) + Send + Sync>;
type Scheduler = Arc<dyn Fn(u64, SyncFuture) -> u64 + Send + Sync>;
pub const TIME_SYNC_INTERVAL_MS: u64 = 300_000;
// Native replay hosts run one isolated account on their host thread. The override
// supplies Date.now only; offset/server synchronization policy below is unchanged.
fn with_host_time<R>(read: impl FnOnce(&std::cell::Cell<Option<f64>>) -> R) -> R {
    std::thread_local! {
        static HOST_TIME: std::cell::Cell<Option<f64>> = const { std::cell::Cell::new(None) };
    }
    HOST_TIME.with(read)
}
pub fn set_thread_host_time(time: Option<f64>) {
    with_host_time(|value| value.set(time));
}

pub static VIRTUAL_CLOCK: LazyLock<VirtualClock> = LazyLock::new(VirtualClock::default);

#[derive(Clone)]
pub struct VirtualClock {
    /// Host epoch milliseconds; inject this on wasm before calling any method.
    pub now: Arc<dyn Fn() -> f64 + Send + Sync>,
    /// "native" (Node/Bun behavior), "browser", or "worker".
    pub runtime: String,
    /// Host venue HTTP hook: Bybit time URL through public proxy, timeout 5000ms.
    pub venue_time_source: Option<ServerTimeSource>,
    /// Runtime timer adapter. The native default leaves polling to the host.
    pub schedule: Scheduler,
    pub cancel: Arc<dyn Fn(u64) + Send + Sync>,
    state: Arc<Mutex<State>>,
}
#[derive(Default)]
struct State {
    started: bool,
    generation: u64,
    timeout_id: Option<u64>,
    server: Option<f64>,
    client: Option<f64>,
    offset_ms: f64,
    sync_in_flight: Option<Arc<Mutex<Flight>>>,
    server_time_source: Option<ServerTimeSource>,
    listeners: BTreeMap<u64, Listener>,
    next_listener_id: u64,
}
struct Flight {
    future: Option<SyncFuture>,
    done: bool,
    wake_pending: bool,
    wakers: Vec<Waker>,
}
struct FlightWake(Weak<Mutex<Flight>>);
impl Wake for FlightWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(flight) = self.0.upgrade() {
            let wakers = {
                let mut state = flight.lock().unwrap();
                if state.future.is_none() && !state.done {
                    state.wake_pending = true;
                    Vec::new()
                } else {
                    std::mem::take(&mut state.wakers)
                }
            };
            for waker in wakers {
                waker.wake();
            }
        }
    }
}
fn date_ms(value: f64) -> f64 {
    if value.is_finite() && value.abs() <= 8.64e15 {
        value.trunc()
    } else {
        f64::NAN
    }
}
fn truthy(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}
impl Default for VirtualClock {
    fn default() -> Self {
        Self {
            now: Arc::new(|| {
                if let Some(time) = with_host_time(|value| value.get()) {
                    return time;
                }
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as f64
            }),
            runtime: "native".into(),
            venue_time_source: None,
            schedule: Arc::new(|_, _| 0),
            cancel: Arc::new(|_| {}),
            state: Arc::new(Mutex::new(State::default())),
        }
    }
}
// The TypeScript module exports one singleton; trait access adds no mirror API.
impl Default for &'static VirtualClock {
    fn default() -> Self {
        &VIRTUAL_CLOCK
    }
}
impl VirtualClock {
    pub async fn start(&self) {
        let start = {
            let mut state = self.state.lock().unwrap();
            let start = !state.started;
            if start {
                state.started = true;
                state.generation += 1;
            }
            start
        };
        if start {
            self.sync_client_server_time().await;
            self.schedule_next_sync();
        }
    }
    pub fn stop(&self) {
        let timeout = {
            let mut state = self.state.lock().unwrap();
            state.started = false;
            state.generation += 1;
            state.timeout_id.take()
        };
        if let Some(id) = timeout.filter(|id| *id != 0) {
            (self.cancel)(id);
        }
    }
    pub fn set_server_time_source(&self, source: Option<ServerTimeSource>) {
        self.state.lock().unwrap().server_time_source = source;
    }
    async fn fetch_from_server_time_source(&self) -> Option<f64> {
        let source = self.state.lock().unwrap().server_time_source.clone();
        if let Some(source) = source {
            source()
                .await
                .ok()
                .flatten()
                .filter(|v| v.is_finite())
                .map(date_ms)
        } else {
            None
        }
    }
    async fn fetch_venue_time(&self) -> Option<f64> {
        if let Some(source) = &self.venue_time_source {
            source().await.ok().flatten().map(date_ms)
        } else {
            None
        }
    }
    fn notify_time_sync(&self) {
        let time = self.get_current_time_ms();
        let mut after = 0;
        loop {
            let next = self
                .state
                .lock()
                .unwrap()
                .listeners
                .range((Excluded(after), Unbounded))
                .next()
                .map(|(id, listener)| (*id, listener.clone()));
            let Some((id, listener)) = next else {
                break;
            };
            after = id;
            listener(time);
        }
    }
    pub fn subscribe_to_time_sync(
        &self,
        listener: Listener,
    ) -> impl Fn() -> bool + Send + Sync + 'static {
        {
            let mut state = self.state.lock().unwrap();
            if !state
                .listeners
                .values()
                .any(|existing| Arc::ptr_eq(existing, &listener))
            {
                state.next_listener_id += 1;
                let id = state.next_listener_id;
                state.listeners.insert(id, listener.clone());
            }
        }
        let state = self.state.clone();
        move || {
            let mut state = state.lock().unwrap();
            let id = state
                .listeners
                .iter()
                .find(|(_, existing)| Arc::ptr_eq(existing, &listener))
                .map(|(id, _)| *id);
            id.is_some_and(|id| state.listeners.remove(&id).is_some())
        }
    }

    pub fn sync_from_server_time(&self, time_ms: f64) {
        let client = date_ms((self.now)());
        let server = date_ms(time_ms);
        let mut state = self.state.lock().unwrap();
        if (server - client).abs() < 1000.0 {
            Self::clear_offset(&mut state);
        } else {
            state.server = Some(server);
            state.client = Some(client);
            state.offset_ms = server - client;
        }
    }
    fn clear_offset(state: &mut State) {
        state.server = None;
        state.client = None;
        state.offset_ms = 0.0;
    }
    fn schedule_next_sync(&self) {
        let generation = {
            let state = self.state.lock().unwrap();
            if !state.started {
                return;
            }
            state.generation
        };
        let scheduled_at = (self.now)();
        let clock = self.clone();
        let id = (self.schedule)(
            TIME_SYNC_INTERVAL_MS,
            Box::pin(async move {
                // ts-bug: VirtualClock.scheduleNextSync clears even normal 300000ms ticks.
                if (clock.now)() - scheduled_at > 90_000.0 {
                    Self::clear_offset(&mut clock.state.lock().unwrap());
                }
                clock.sync_client_server_time().await;
                clock.schedule_next_sync();
            }),
        );
        let stale = {
            let mut state = self.state.lock().unwrap();
            if state.started && state.generation == generation {
                state.timeout_id = Some(id);
                false
            } else {
                true
            }
        };
        if stale && id != 0 {
            (self.cancel)(id);
        }
    }
    pub fn get_current_time(&self) -> f64 {
        let now = date_ms((self.now)());
        let state = self.state.lock().unwrap();
        match (state.server, state.client) {
            (Some(server), Some(client)) => date_ms(server + (now - client)),
            _ => now,
        }
    }
    pub fn get_current_time_ms(&self) -> f64 {
        (self.now)() + self.state.lock().unwrap().offset_ms
    }
    pub fn sync_client_server_time(&self) -> impl Future<Output = ()> + Send + 'static {
        let flight = {
            let mut state = self.state.lock().unwrap();
            if let Some(flight) = &state.sync_in_flight {
                flight.clone()
            } else {
                let clock = self.clone();
                let future = Box::pin(async move {
                    clock.sync_client_server_time_inner().await;
                    clock.state.lock().unwrap().sync_in_flight = None;
                });
                let flight = Arc::new(Mutex::new(Flight {
                    future: Some(future),
                    done: false,
                    wake_pending: false,
                    wakers: Vec::new(),
                }));
                state.sync_in_flight = Some(flight.clone());
                flight
            }
        };
        poll_fn(move |cx| Self::poll_sync(&flight, cx))
    }
    fn poll_sync(flight: &Arc<Mutex<Flight>>, cx: &mut Context<'_>) -> Poll<()> {
        let future = {
            let mut state = flight.lock().unwrap();
            if state.done {
                return Poll::Ready(());
            }
            if !state.wakers.iter().any(|w| w.will_wake(cx.waker())) {
                state.wakers.push(cx.waker().clone());
            }
            state.future.take()
        };
        let Some(mut future) = future else {
            return Poll::Pending;
        };
        let waker = Waker::from(Arc::new(FlightWake(Arc::downgrade(flight))));
        let result = future.as_mut().poll(&mut Context::from_waker(&waker));
        let wakers = {
            let mut state = flight.lock().unwrap();
            if result.is_ready() {
                state.done = true;
                std::mem::take(&mut state.wakers)
            } else {
                state.future = Some(future);
                if std::mem::take(&mut state.wake_pending) {
                    std::mem::take(&mut state.wakers)
                } else {
                    Vec::new()
                }
            }
        };
        for waker in wakers {
            waker.wake();
        }
        result
    }
    async fn sync_client_server_time_inner(&self) {
        if self.runtime == "native" {
            Self::clear_offset(&mut self.state.lock().unwrap());
            return;
        }
        if self.runtime == "worker" {
            return;
        }
        self.sync_from_server_time(self.fetch_server_time().await);
        self.notify_time_sync();
    }
    async fn fetch_server_time(&self) -> f64 {
        if let Some(time) = self.fetch_from_server_time_source().await {
            time
        } else if let Some(time) = self.fetch_venue_time().await {
            time
        } else {
            date_ms((self.now)())
        }
    }
    pub async fn sync_bybit_client_server_time(&self, time: Option<f64>) {
        if self.runtime == "native" {
            Self::clear_offset(&mut self.state.lock().unwrap());
            return;
        }
        let time = time.filter(|v| truthy(*v));
        if time.is_none() && self.runtime == "worker" {
            return;
        }
        // TS samples the local clock after awaiting the Bybit-first fallback.
        let server = if let Some(time) = time {
            date_ms(time)
        } else {
            self.fetch_bybit_time().await
        };
        self.sync_from_server_time(server);
    }
    async fn fetch_bybit_time(&self) -> f64 {
        if let Some(time) = self.fetch_venue_time().await {
            time
        } else if let Some(time) = self.fetch_from_server_time_source().await {
            time
        } else {
            date_ms((self.now)())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    struct ThreadWake(std::thread::Thread);
    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
        let mut future = std::pin::pin!(future);
        loop {
            match future.as_mut().poll(&mut Context::from_waker(&waker)) {
                Poll::Ready(value) => return value,
                Poll::Pending => std::thread::park(),
            }
        }
    }
    fn clock(runtime: &str, now: f64) -> VirtualClock {
        VirtualClock {
            runtime: runtime.into(),
            now: Arc::new(move || now),
            ..VirtualClock::default()
        }
    }
    fn source(value: Result<Option<f64>, String>) -> ServerTimeSource {
        Arc::new(move || {
            let value = value.clone();
            Box::pin(async move { value })
        })
    }
    #[test]
    fn drift_boundaries_truthiness_and_runtime_rules() {
        let clock = clock("worker", 10_000.0);
        for delta in [999.0, -999.0, 0.0] {
            clock.sync_from_server_time(10_000.0 + delta);
            assert_eq!(clock.get_current_time_ms(), 10_000.0);
        }
        for delta in [1000.0, -1000.0] {
            clock.sync_from_server_time(10_000.0 + delta);
            assert_eq!(clock.get_current_time_ms(), 10_000.0 + delta);
        }
        block_on(clock.sync_client_server_time());
        assert_eq!(clock.get_current_time_ms(), 9000.0);
        for time in [None, Some(0.0), Some(-0.0), Some(f64::NAN)] {
            block_on(clock.sync_bybit_client_server_time(time));
            assert_eq!(clock.get_current_time_ms(), 9000.0);
        }
        block_on(clock.sync_bybit_client_server_time(Some(20_000.0)));
        assert_eq!(clock.get_current_time_ms(), 20_000.0);
        clock.stop();
        assert_eq!(clock.get_current_time_ms(), 20_000.0);
        let mut native = clock.clone();
        native.runtime = "native".into();
        block_on(native.sync_bybit_client_server_time(Some(30_000.0)));
        assert_eq!(clock.get_current_time_ms(), 10_000.0);
        assert!(std::ptr::eq(
            <&'static VirtualClock>::default(),
            <&'static VirtualClock>::default()
        ));
    }
    #[test]
    fn source_order_errors_and_invalid_venue_time() {
        let mut clock = clock("browser", 10_000.0);
        clock.set_server_time_source(Some(source(Ok(Some(30_000.0)))));
        clock.venue_time_source = Some(source(Ok(Some(20_000.0))));
        block_on(clock.sync_client_server_time());
        assert_eq!(clock.get_current_time_ms(), 30_000.0);
        block_on(clock.sync_bybit_client_server_time(None));
        assert_eq!(clock.get_current_time_ms(), 20_000.0);
        for value in [
            Err("offline".into()),
            Ok(None),
            Ok(Some(f64::NAN)),
            Ok(Some(f64::INFINITY)),
        ] {
            clock.set_server_time_source(Some(source(value)));
            block_on(clock.sync_client_server_time());
            assert_eq!(clock.get_current_time_ms(), 20_000.0);
        }
        clock.venue_time_source = Some(source(Err("offline".into())));
        block_on(clock.sync_client_server_time());
        assert_eq!(clock.get_current_time_ms(), 10_000.0);
        clock.set_server_time_source(Some(source(Ok(Some(30_000.0)))));
        block_on(clock.sync_bybit_client_server_time(None));
        assert_eq!(clock.get_current_time_ms(), 30_000.0);
        clock.venue_time_source = Some(source(Ok(Some(f64::NAN))));
        block_on(clock.sync_bybit_client_server_time(None));
        assert!(clock.get_current_time_ms().is_nan());
    }
    #[test]
    fn date_clipping_keeps_hot_path_allocation_free() {
        let clock = clock("worker", 10_000.9);
        clock.sync_from_server_time(20_000.9);
        assert_eq!(clock.get_current_time(), 20_000.0);
        assert_eq!(clock.get_current_time_ms(), 20_000.9);
        clock.sync_from_server_time(f64::INFINITY);
        assert!(clock.get_current_time().is_nan());
    }
    #[test]
    fn listeners_dedupe_and_mutate_live_set_without_locking_callbacks() {
        let clock = clock("browser", 10_000.0);
        let count = Arc::new(AtomicU64::new(0));
        let calls = count.clone();
        let callback: Listener = Arc::new(move |_| {
            calls.fetch_add(1, Ordering::SeqCst);
        });
        let remove = clock.subscribe_to_time_sync(callback.clone());
        let remove_again = clock.subscribe_to_time_sync(callback);
        block_on(clock.sync_client_server_time());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(remove());
        assert!(!remove_again());
        let later = count.clone();
        let later: Listener = Arc::new(move |_| {
            later.fetch_add(10, Ordering::SeqCst);
        });
        let reentrant = clock.clone();
        let _remove = clock.subscribe_to_time_sync(Arc::new(move |_| {
            let _ = reentrant.subscribe_to_time_sync(later.clone());
        }));
        block_on(clock.sync_client_server_time());
        assert_eq!(count.load(Ordering::SeqCst), 11);
    }
    #[test]
    fn single_flight_survives_first_caller_drop_and_stop_during_start() {
        let clock = clock("browser", 10_000.0);
        let calls = Arc::new(AtomicU64::new(0));
        let ready = Arc::new(AtomicBool::new(false));
        let gate = ready.clone();
        let counter = calls.clone();
        clock.set_server_time_source(Some(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
            let ready = gate.clone();
            Box::pin(poll_fn(move |_| {
                if ready.load(Ordering::SeqCst) {
                    Poll::Ready(Ok(Some(20_000.0)))
                } else {
                    Poll::Pending
                }
            }))
        })));
        let waker = Waker::from(Arc::new(ThreadWake(std::thread::current())));
        let mut cx = Context::from_waker(&waker);
        let mut first = Box::pin(clock.sync_client_server_time());
        let mut second = Box::pin(clock.sync_client_server_time());
        assert!(first.as_mut().poll(&mut cx).is_pending());
        assert!(second.as_mut().poll(&mut cx).is_pending());
        drop(first);
        ready.store(true, Ordering::SeqCst);
        assert!(second.as_mut().poll(&mut cx).is_ready());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        ready.store(false, Ordering::SeqCst);
        let mut start = Box::pin(clock.start());
        assert!(start.as_mut().poll(&mut cx).is_pending());
        clock.stop();
        ready.store(true, Ordering::SeqCst);
        assert!(start.as_mut().poll(&mut cx).is_ready());
        assert!(!clock.state.lock().unwrap().started);
        assert!(clock.state.lock().unwrap().timeout_id.is_none());
    }
    #[test]
    fn periodic_timer_preserves_the_ts_sleep_detection_bug() {
        let now = Arc::new(AtomicU64::new(10_000));
        let time = now.clone();
        let timers: Arc<Mutex<Vec<SyncFuture>>> = Arc::new(Mutex::new(Vec::new()));
        let timer_queue = timers.clone();
        let canceled = Arc::new(AtomicU64::new(0));
        let cancel = canceled.clone();
        let mut clock = clock("worker", 10_000.0);
        clock.now = Arc::new(move || time.load(Ordering::SeqCst) as f64);
        clock.schedule = Arc::new(move |delay, future| {
            assert_eq!(delay, 300_000);
            timer_queue.lock().unwrap().push(future);
            7
        });
        clock.cancel = Arc::new(move |id| {
            cancel.store(id, Ordering::SeqCst);
        });
        clock.sync_from_server_time(20_000.0);
        block_on(clock.start());
        block_on(clock.start());
        assert_eq!(timers.lock().unwrap().len(), 1);
        now.store(310_000, Ordering::SeqCst);
        let timer = timers.lock().unwrap().remove(0);
        block_on(timer);
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/clock_oracle.json")).unwrap();
        let oracle = cases
            .iter()
            .find(|case| case["runtime"] == "worker-timer")
            .unwrap();
        assert_eq!(
            clock.get_current_time_ms(),
            oracle["after"].as_f64().unwrap()
        );
        assert_eq!(timers.lock().unwrap().len(), 1);
        clock.stop();
        assert_eq!(canceled.load(Ordering::SeqCst), 7);
    }
    #[test]
    fn old_unsubscribe_removes_readded_listener_identity() {
        let clock = clock("browser", 0.0);
        let callback: Listener = Arc::new(|_| {});
        let old_remove = clock.subscribe_to_time_sync(callback.clone());
        assert!(old_remove());
        let _new_remove = clock.subscribe_to_time_sync(callback);
        assert!(old_remove());
        assert!(clock.state.lock().unwrap().listeners.is_empty());
    }
    #[test]
    fn scheduling_hook_stop_cancels_newly_returned_timer() {
        let mut clock = clock("worker", 0.0);
        let reentrant = clock.clone();
        let canceled = Arc::new(AtomicU64::new(0));
        let cancel = canceled.clone();
        clock.schedule = Arc::new(move |_, _| {
            reentrant.stop();
            9
        });
        clock.cancel = Arc::new(move |id| {
            cancel.store(id, Ordering::SeqCst);
        });
        block_on(clock.start());
        assert_eq!(canceled.load(Ordering::SeqCst), 9);
        assert!(clock.state.lock().unwrap().timeout_id.is_none());
    }
    #[test]
    fn wake_while_polling_is_reissued_after_future_restoration() {
        struct CountWake(AtomicU64);
        impl Wake for CountWake {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        let mut fired = false;
        let future = Box::pin(poll_fn(move |cx| {
            if fired {
                Poll::Ready(())
            } else {
                fired = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }));
        let flight = Arc::new(Mutex::new(Flight {
            future: Some(future),
            done: false,
            wake_pending: false,
            wakers: Vec::new(),
        }));
        let count = Arc::new(CountWake(AtomicU64::new(0)));
        let waker = Waker::from(count.clone());
        let mut cx = Context::from_waker(&waker);
        assert!(VirtualClock::poll_sync(&flight, &mut cx).is_pending());
        assert_eq!(count.0.load(Ordering::SeqCst), 1);
        assert!(VirtualClock::poll_sync(&flight, &mut cx).is_ready());
    }
    #[test]
    fn checked_in_typescript_clock_oracle() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../tests/clock_oracle.json")).unwrap();
        for case in cases {
            if case["runtime"] == "worker-timer" {
                continue;
            }
            let clock = clock(case["runtime"].as_str().unwrap(), 10000.0);
            clock.sync_from_server_time(10000.0 + case["delta"].as_f64().unwrap());
            assert_eq!(
                clock.get_current_time_ms(),
                case["pushed"].as_f64().unwrap(),
                "{case}"
            );
            clock.set_server_time_source(Some(source(Ok(Some(30000.0)))));
            block_on(clock.sync_client_server_time());
            assert_eq!(
                clock.get_current_time_ms(),
                case["synced"].as_f64().unwrap(),
                "{case}"
            );
            block_on(clock.sync_bybit_client_server_time(Some(20000.0)));
            assert_eq!(
                clock.get_current_time_ms(),
                case["bybit"].as_f64().unwrap(),
                "{case}"
            );
        }
    }
}
