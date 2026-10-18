use safe_cex_exchange::{
    access_token::AccessToken,
    how_facade::{SafeCexHOW, get_server_latency_for_exchange},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_transport::{
    TransportError, TransportFuture,
    how_config::{self, HOWConfigPatch},
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Waker},
};
type Job = (Pin<Box<dyn Future<Output = ()>>>, Rc<Cell<bool>>);
#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Job>>,
}
struct Task(Rc<Cell<bool>>);
impl ExchangeTask for Task {
    fn cancel(&self) {
        self.0.set(true)
    }
}
impl Runtime {
    fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        for (mut job, cancelled) in jobs {
            if !cancelled.get()
                && job
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            {
                self.jobs.borrow_mut().push((job, cancelled));
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1000.0
    }
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        let cancelled = Rc::new(Cell::new(false));
        self.jobs.borrow_mut().push((future, cancelled.clone()));
        Box::new(Task(cancelled))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "root".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.0
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        Box::pin(async { Ok(Value::Null) })
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ROOT expected ready future"),
    }
}
fn clock(runtime: &str) -> VirtualClock {
    let mut clock = VirtualClock::default();
    clock.runtime = runtime.into();
    clock.now = Arc::new(|| 1000.0);
    clock
}
fn source() {
    let output = std::process::Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../safe-cex-transport/scripts/safe-cex-how-wrappers.cjs"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ROOT actual HOW wrappers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("2 configurations, 8 clock forwards, 6 latency paths")
    );
}
struct Adapter;
impl safe_cex_transport::RestAdapter for Adapter {
    fn execute(
        &self,
        _: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        Box::pin(std::future::pending())
    }
}
const VALID: &str = "e30.eyJleHAiOjQxMDI0NDQ4MDB9.x";
#[test]
fn root_how_actual_bindings_identity_context_token_and_clock() {
    let r = Rc::new(Runtime::default());
    let clock = clock("native");
    clock.sync_from_server_time(3700.0);
    assert_eq!(
        clock.get_current_time_ms().to_bits(),
        3700f64.to_bits(),
        "ROOT HOW native reset seed is nonzero"
    );
    let access = AccessToken::new(None, clock.clone(), false);
    access.set_proxy_token("module session".into());
    let calls = Rc::new(Cell::new(0));
    let out = calls.clone();
    access.set_contextual_proxy_token_provider(Some(Rc::new(move || {
        out.set(out.get() + 1);
        Ok(Some(Rc::new(|| Box::pin(async { Ok(VALID.into()) }))))
    })));
    let adapter: Rc<dyn safe_cex_transport::RestAdapter> = Rc::new(Adapter);
    how_config::configure_how(HOWConfigPatch {
        direct_adapter: Some(Some(adapter.clone())),
        ..Default::default()
    });
    let facade = SafeCexHOW::new(access.clone(), clock.clone(), r, Some("1"));
    let first = how_config::get_how_config();
    assert!(
        Rc::ptr_eq(first.direct_adapter.as_ref().unwrap(), &adapter),
        "ROOT HOW preserves unrelated adapter"
    );
    assert!(
        first.simulate_http_over_websocket,
        "ROOT HOW native simulation flag"
    );
    let token = (first.get_proxy_token)();
    assert_eq!(calls.get(), 1, "ROOT HOW token prefix runs on invocation");
    assert_eq!(
        ready(token).unwrap(),
        VALID,
        "ROOT HOW contextual token wins"
    );
    assert!(!(first.is_token_expired)(VALID).unwrap());
    assert!((first.is_token_expired)("bad").unwrap());
    let domains = (first.get_unique_latency_domains.as_ref().unwrap())();
    assert_eq!(
        &domains[..3],
        &["fapi.asterdex.com", "api.bybit.com", "hyrotrader-simulator"]
    );
    assert_eq!(
        first.latency_domain_to_exchanges.as_ref().unwrap()["api.bybit.com"],
        vec!["bybit", "bybitdemo", "bybitspot", "bybitinverse"]
    );
    (first.sync_server_time.as_ref().unwrap())(0.0);
    assert_eq!(
        clock.get_current_time_ms().to_bits(),
        1000f64.to_bits(),
        "ROOT HOW native clock reset"
    );
    facade.initialize_how();
    let second = how_config::get_how_config();
    assert!(Rc::ptr_eq(
        second.direct_adapter.as_ref().unwrap(),
        &adapter
    ));
    assert!(Rc::ptr_eq(&first.get_proxy_token, &second.get_proxy_token));
    assert!(Rc::ptr_eq(
        &first.is_token_expired,
        &second.is_token_expired
    ));
    assert!(Rc::ptr_eq(
        first.get_unique_latency_domains.as_ref().unwrap(),
        second.get_unique_latency_domains.as_ref().unwrap()
    ));
    assert!(!Rc::ptr_eq(
        first.sync_server_time.as_ref().unwrap(),
        second.sync_server_time.as_ref().unwrap()
    ));
    access.set_contextual_proxy_token_provider(Some(Rc::new(|| {
        Ok(Some(Rc::new(|| Box::pin(async { Ok("".into()) }))))
    })));
    assert_eq!(
        ready(how_config::get_proxy_token()).unwrap_err(),
        TransportError::Host(
            "ProxyTokenUnavailableError: proxy token missing or expired for this request context"
                .into()
        )
    );
    assert_eq!(access.current_proxy_token(), "module session");
    source();
}
#[test]
fn root_how_retains_dropped_token_observer_and_browser_fallback() {
    let r = Rc::new(Runtime::default());
    let mut clock = clock("browser");
    let gate = Arc::new(AtomicBool::new(false));
    let fetches = Arc::new(AtomicUsize::new(0));
    let seen = fetches.clone();
    let flag = gate.clone();
    clock.venue_time_source = Some(Arc::new(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        let flag = flag.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if flag.load(Ordering::SeqCst) {
                Poll::Ready(Ok(Some(2777.0)))
            } else {
                Poll::Pending
            }
        }))
    }));
    let access = AccessToken::new(None, clock.clone(), false);
    let token_gate = Rc::new(Cell::new(false));
    let completed = Rc::new(Cell::new(0));
    let flag = token_gate.clone();
    let seen = completed.clone();
    access.set_contextual_proxy_token_provider(Some(Rc::new(move || {
        let flag = flag.clone();
        let seen = seen.clone();
        Ok(Some(Rc::new(move || {
            let flag = flag.clone();
            let seen = seen.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if flag.get() {
                    seen.set(seen.get() + 1);
                    Poll::Ready(Ok(VALID.into()))
                } else {
                    Poll::Pending
                }
            }))
        })))
    })));
    let facade = SafeCexHOW::new(access, clock.clone(), r.clone(), Some("true"));
    assert!(!how_config::should_simulate_http_over_websocket());
    drop(how_config::get_proxy_token());
    assert_eq!(completed.get(), 0);
    token_gate.set(true);
    r.pump();
    assert_eq!(
        completed.get(),
        1,
        "ROOT HOW dropped token observer retains operation"
    );
    how_config::sync_server_time(-0.0);
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        1,
        "ROOT HOW browser falsy fetch prefix"
    );
    assert_eq!(clock.get_current_time_ms().to_bits(), 1000f64.to_bits());
    gate.store(true, Ordering::SeqCst);
    r.pump();
    assert_eq!(
        clock.get_current_time_ms().to_bits(),
        2777f64.to_bits(),
        "ROOT HOW retained fallback native time"
    );
    how_config::sync_server_time(2929.0);
    assert_eq!(clock.get_current_time_ms().to_bits(), 2929f64.to_bits());
    assert_eq!(fetches.load(Ordering::SeqCst), 1);
    drop(facade);
}

#[test]
fn root_how_host_discard_resolves_pending_token_observer() {
    struct WakeCount(AtomicUsize);
    impl std::task::Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Rc::new(Runtime::default());
    let clock = clock("browser");
    let access = AccessToken::new(None, clock.clone(), false);
    access.set_contextual_proxy_token_provider(Some(Rc::new(|| {
        Ok(Some(Rc::new(|| Box::pin(std::future::pending()))))
    })));
    let _facade = SafeCexHOW::new(access, clock, runtime.clone(), None);
    let mut observer = how_config::get_proxy_token();
    let wake = Arc::new(WakeCount(AtomicUsize::new(0)));
    let waker = Waker::from(wake.clone());
    assert!(
        observer
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    assert_eq!(
        runtime.jobs.borrow().len(),
        1,
        "HOW-CANCEL one pending token task"
    );
    drop(std::mem::take(&mut *runtime.jobs.borrow_mut()));
    assert_eq!(
        wake.0.load(Ordering::SeqCst),
        1,
        "HOW-CANCEL host discard wakes token observer exactly once"
    );
    assert!(
        matches!(ready(observer), Err(TransportError::Host(message)) if message == "HOW token operation cancelled"),
        "HOW-CANCEL pending observer receives cancellation error"
    );
}

#[test]
fn root_how_dropped_polled_observer_releases_waker_but_keeps_token_task() {
    struct Sentinel(Arc<AtomicUsize>);
    impl std::task::Wake for Sentinel {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let runtime = Rc::new(Runtime::default());
    let clock = clock("browser");
    let access = AccessToken::new(None, clock.clone(), false);
    let completed = Rc::new(Cell::new(0));
    let gate = Rc::new(Cell::new(false));
    let flag = gate.clone();
    let count = completed.clone();
    access.set_contextual_proxy_token_provider(Some(Rc::new(move || {
        let flag = flag.clone();
        let count = count.clone();
        Ok(Some(Rc::new(move || {
            let flag = flag.clone();
            let count = count.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if flag.get() {
                    count.set(count.get() + 1);
                    Poll::Ready(Ok(VALID.into()))
                } else {
                    Poll::Pending
                }
            }))
        })))
    })));
    let _facade = SafeCexHOW::new(access, clock, runtime.clone(), None);
    let wakes = Arc::new(AtomicUsize::new(0));
    let sentinel = Arc::new(Sentinel(wakes.clone()));
    let weak = Arc::downgrade(&sentinel);
    let waker = Waker::from(sentinel);
    let mut observer = how_config::get_proxy_token();
    assert!(
        observer
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(observer);
    drop(waker);
    assert!(
        weak.upgrade().is_none(),
        "HOW-OBSERVER cancelled observer immediately releases its waker"
    );
    assert_eq!(
        runtime.jobs.borrow().len(),
        1,
        "HOW-OBSERVER observer drop retains owned token task"
    );
    gate.set(true);
    runtime.pump();
    assert_eq!(
        completed.get(),
        1,
        "HOW-OBSERVER token work completes after observer drop"
    );
    assert_eq!(
        wakes.load(Ordering::SeqCst),
        0,
        "HOW-OBSERVER abandoned observer is never woken"
    );
}
#[derive(Default)]
struct Host {
    calls: Cell<usize>,
    bad: Cell<bool>,
}
impl TunnelHost for Host {
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        panic!("ROOT no dialing")
    }
    fn capabilities(&self) -> SocketCapabilities {
        self.calls.set(self.calls.get() + 1);
        SocketCapabilities {
            runtime: if self.bad.get() {
                SocketRuntime::Native
            } else {
                SocketRuntime::NodeWs
            },
            upgrade_headers: !self.bad.get(),
            alpn: true,
        }
    }
    fn now_ms(&self) -> u64 {
        4242
    }
    fn sleep_until(&self, _: u64) -> TransportFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn connect(
        &self,
        _: String,
        _: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        panic!("ROOT no connect")
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        panic!("ROOT no token")
    }
    fn is_token_expired(&self, _: &str) -> bool {
        panic!("ROOT no expiry")
    }
    fn sync_server_time(&self, _: u64) {
        panic!("ROOT no clock")
    }
}
#[test]
fn root_how_latency_uses_canonical_metadata_and_native_entry_fields() {
    how_config::configure_how(HOWConfigPatch {
        simulate_http_over_websocket: Some(false),
        ..Default::default()
    });
    let host = Rc::new(Host::default());
    host.bad.set(true);
    for exchange in ["backpack", "cryptocom", "lighter"] {
        assert_eq!(
            get_server_latency_for_exchange(host.clone(), exchange).unwrap(),
            None,
            "ROOT HOW no-domain native result"
        );
    }
    assert_eq!(
        host.calls.get(),
        0,
        "ROOT HOW no-domain precedes socket selector"
    );
    assert_eq!(
        get_server_latency_for_exchange(host.clone(), "missing").unwrap_err(),
        TransportError::Host("Unknown exchange metadata: missing".into())
    );
    assert_eq!(host.calls.get(), 0);
    assert!(get_server_latency_for_exchange(host.clone(), "binance").is_err());
    host.bad.set(false);
    let sg =
        safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "binance").unwrap();
    let jp =
        safe_cex_transport::how::get_how_websocket_for_exchange(host.clone(), "bybit").unwrap();
    for (socket, domain, latency) in [
        (&sg, "fapi.binance.com", 17.25),
        (&jp, "api.bybit.com", 29.5),
    ] {
        socket.on_message(SocketFrame::Text(json!({"type":"rpc.latency","domain":domain,"latency_ms":latency,"source":"origin","cached":true,"sample_count":7,"measured_at":1234}).to_string())).unwrap();
    }
    for (exchange, expected) in [("binance", 17.25f64), ("bybit", 29.5f64)] {
        let entry = get_server_latency_for_exchange(host.clone(), exchange).unwrap();
        assert!(
            entry.is_some(),
            "ROOT HOW native selected domain latency exists"
        );
        let entry = entry.unwrap();
        assert_eq!(
            entry.latency_ms.as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "ROOT HOW native selected domain latency"
        );
        assert_eq!(entry.source, json!("origin"));
        assert_eq!(entry.cached, json!(true));
        assert_eq!(entry.sample_count, json!(7));
        assert_eq!(entry.measured_at, json!(1234));
        assert_eq!(entry.updated_at, 4242);
    }
    assert_eq!(get_server_latency_for_exchange(host, "okx").unwrap(), None);
    source();
}
