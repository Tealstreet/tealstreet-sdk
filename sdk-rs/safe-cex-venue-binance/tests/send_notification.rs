//! Real core/Store native assertions precede the actual-source comparison.
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{exchange::BaseExchange, runtime::*, websocket::*};
use safe_cex_store::Store;
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::{ExchangeName, ExchangeOptions, StoreSubscriberError};
use safe_cex_venue_binance::{private_open::*, private_ping::*, public_open::*, spot_open::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};

type Work = Pin<Box<dyn Future<Output = ()>>>;
type Job = Rc<RefCell<Option<Work>>>;
struct Task(Job);
impl ExchangeTask for Task {
    fn cancel(&self) {
        let future = self.0.borrow_mut().take();
        drop(future);
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel();
    }
}
#[derive(Default)]
struct Runtime {
    jobs: RefCell<Vec<Job>>,
    errors: RefCell<Vec<ExchangeError>>,
    inline: Cell<bool>,
}
impl Runtime {
    fn pump(&self) {
        let jobs = self.jobs.borrow().clone();
        for job in jobs {
            let future = job.borrow_mut().take();
            if let Some(mut future) = future {
                if future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    *job.borrow_mut() = Some(future);
                }
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        123.0
    }
    fn now_ms(&self) -> f64 {
        1000.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: Work) -> Box<dyn ExchangeTask> {
        let job = Rc::new(RefCell::new(Some(future)));
        self.jobs.borrow_mut().push(job.clone());
        if self.inline.get() {
            let future = job.borrow_mut().take();
            if let Some(mut future) = future {
                if future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    *job.borrow_mut() = Some(future);
                }
            }
        }
        Box::new(Task(job))
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "native-notification".into()
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
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        error: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.errors.borrow_mut().push(error.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker in fixture")
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("no tunnel in fixture")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        panic!("a reconnect timer must remain pending")
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        true
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    base: Rc<BaseExchange>,
    options: ExchangeOptions,
    trace: Rc<RefCell<Vec<Value>>>,
    resync: Rc<ReopenResync>,
    fail_warning: Cell<bool>,
    fail_reconnect_info: Cell<bool>,
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "fixture"
    }
    fn is_disposed(&self) -> bool {
        self.base.is_disposed()
    }
    fn options(&self) -> ExchangeOptions {
        self.options.clone()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.base.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        true
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        self.resync.clone()
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("no snapshot in fixture")
    }
    fn log(&self, _: &str, severity: LogSeverity) -> Result<(), ExchangeError> {
        if self.fail_warning.get() && severity == LogSeverity::Warning {
            return Err(ExchangeError::new("direct warning failed"));
        }
        if self.fail_reconnect_info.get() && severity == LogSeverity::Info {
            return Err(ExchangeError::new("reconnect info failed"));
        }
        Ok(())
    }
    fn emit(&self, event: &str, _: &str) -> Result<(), ExchangeError> {
        if event == "grafana-pws-sub" {
            self.trace.borrow_mut().push(json!("diagnostic"));
        }
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        self.base.sync_socket_health_to_store()
    }
    fn update_latency(&self, _: f64) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push(json!("emit-error"));
        Ok(())
    }
}
struct Protocol;
impl WsProtocol for Protocol {}
struct Fixture {
    store: Rc<DefaultStore>,
    parent: Rc<Parent>,
    runtime: Rc<Runtime>,
    socket: Rc<BaseWebSocket>,
    cause: Rc<dyn Error>,
    first: Rc<Cell<usize>>,
    second: Rc<Cell<usize>>,
    unsubscribes: Vec<Box<dyn Fn()>>,
}
impl Fixture {
    fn new(public_only: bool, disposed: bool, throws: bool, inline: bool) -> Self {
        let store = Rc::new(DefaultStore::new());
        let runtime = Rc::new(Runtime::default());
        runtime.inline.set(inline);
        let options: ExchangeOptions = serde_json::from_value(
            json!({"key":"fixture","secret":"fixture","publicDataOnly":public_only}),
        )
        .unwrap();
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Binance,
            options.clone(),
            store.clone(),
            runtime.clone(),
            Default::default(),
        ));
        if disposed {
            base.dispose().unwrap();
        }
        let trace = Rc::new(RefCell::new(vec![]));
        let parent = Rc::new(Parent {
            base,
            options,
            trace: trace.clone(),
            resync: Default::default(),
            fail_warning: Cell::new(false),
            fail_reconnect_info: Cell::new(false),
        });
        let socket = BaseWebSocket::new(1, parent.clone(), runtime.clone(), Rc::new(Protocol));
        socket.order_book_inner_state.borrow_mut().insert(
            "fixture".into(),
            OrderBookInnerState {
                updates: vec![json!(1)],
                is_snapshot_loaded: true,
            },
        );
        let mut original = ExchangeError::new("socket health subscriber failed");
        original.name = "RangeError".into();
        original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "native-health"
        )));
        original.store_subscriber_ids = vec![41, 73];
        let cause: Rc<dyn Error> = Rc::new(original);
        let failure = StoreSubscriberError::new("subscriber wrapper").with_cause(cause.clone());
        let first = Rc::new(Cell::new(0));
        let second = Rc::new(Cell::new(0));
        let count = first.clone();
        let events = trace.clone();
        let a = Store::subscribe(
            store.as_ref(),
            Box::new(move |data, _| {
                count.set(count.get() + 1);
                let healthy = data.socket_health.as_ref().unwrap()["publicHealthy"]
                    .as_bool()
                    .unwrap();
                assert!(
                    healthy,
                    "native health is committed before subscriber callback"
                );
                events.borrow_mut().push(json!(["first", healthy]));
                if throws { Err(failure.clone()) } else { Ok(()) }
            }),
        );
        let count = second.clone();
        let events = trace;
        let b = Store::subscribe(
            store.as_ref(),
            Box::new(move |data, _| {
                count.set(count.get() + 1);
                events.borrow_mut().push(json!([
                    "second",
                    data.socket_health.as_ref().unwrap()["publicHealthy"]
                ]));
                Ok(())
            }),
        );
        Self {
            store,
            parent,
            runtime,
            socket,
            cause,
            first,
            second,
            unsubscribes: vec![a, b],
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for cleanup in &self.unsubscribes {
            cleanup();
        }
        self.socket.dispose();
        self.parent.base.dispose().unwrap();
    }
}
#[derive(Clone, Copy)]
struct Input {
    variant: &'static str,
    scope: &'static str,
    class_name: Option<&'static str>,
    market: bool,
    public_only: bool,
    disposed: bool,
    throws: bool,
    method: &'static str,
}
fn inputs() -> Vec<Input> {
    let entries = [
        ("binance", "private", None, false),
        ("binancepm", "private", None, false),
        ("binancespot", "private", None, false),
        ("binance", "public", Some("BinanceMarketWebsocket"), true),
        ("binance", "public", Some("BinancePublicWebsocket"), false),
        (
            "binancepm",
            "public",
            Some("BinancePMMarketWebsocket"),
            true,
        ),
        (
            "binancepm",
            "public",
            Some("BinancePMPublicWebsocket"),
            false,
        ),
        (
            "binancespot",
            "public",
            Some("BinancePublicWebsocket"),
            false,
        ),
    ];
    let mut result = vec![];
    for (variant, scope, class_name, market) in entries {
        for method in if scope == "private" {
            vec!["ping", "onOpen"]
        } else {
            vec!["onOpen"]
        } {
            for public_only in if scope == "public" {
                vec![false, true]
            } else {
                vec![false]
            } {
                for disposed in [false, true] {
                    for throws in [false, true] {
                        result.push(Input {
                            variant,
                            scope,
                            class_name,
                            market,
                            public_only,
                            disposed,
                            throws,
                            method,
                        });
                    }
                }
            }
        }
    }
    assert_eq!(
        result.len(),
        64,
        "independent eight-owner/method fixture table"
    );
    result
}
// JSON wire numbers have one ECMAScript Number domain, while serde_json
// distinguishes integer and floating storage. Native assertions run before this.
fn wire_numbers(value: &Value) -> Value {
    match value {
        Value::Number(value) => json!(value.as_f64().expect("finite authored wire number")),
        Value::Array(values) => Value::Array(values.iter().map(wire_numbers).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), wire_numbers(value)))
                .collect(),
        ),
        value => value.clone(),
    }
}
fn native_case(input: Input) -> Value {
    let h = Fixture::new(input.public_only, input.disposed, input.throws, true);
    let resync = Cell::new(true);
    let opened = Cell::new(false);
    let events = h.parent.trace.clone();
    let fetch = || {
        events.borrow_mut().push(json!("fetch-orders"));
        Ok(())
    };
    let events = h.parent.trace.clone();
    let subscribe = || {
        events.borrow_mut().push(json!("subscribe"));
        Ok(())
    };
    let spot_open =
        input.scope == "private" && input.variant == "binancespot" && input.method == "onOpen";
    if spot_open {
        let attempt = Cell::new(0.0);
        let mut yielded = false;
        let params = std::future::poll_fn(move |cx| {
            if yielded {
                Poll::Ready(Ok(safe_cex_num::JsJsonValue::Object(vec![(
                    "apiKey".into(),
                    safe_cex_num::JsJsonValue::String("fixture".into()),
                )])))
            } else {
                yielded = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        });
        let mut future = Box::pin(open_binance_spot_private(&h.socket, &attempt, || params));
        let first = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert_eq!(
            first.is_pending(),
            !input.disposed,
            "native Spot signing has an await boundary"
        );
        h.parent.trace.borrow_mut().push(json!("caller-returned"));
        let result = match first {
            Poll::Ready(r) => r,
            Poll::Pending => match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(r) => r,
                Poll::Pending => panic!("native signing resolves on second poll"),
            },
        };
        result.expect("native Spot open resolves despite discarded reconnect rejection");
        assert!(
            !h.socket.is_force_closed(),
            "native Spot open does not force close after background rejection"
        );
        h.parent.trace.borrow_mut().push(json!("caller-resolved"));
    } else {
        let result = match (input.scope, input.method, input.variant, input.market) {
            ("private", "ping", "binance", _) => ping_binance_private(&h.socket),
            ("private", "ping", "binancepm", _) => ping_binance_pm_private(&h.socket),
            ("private", "ping", "binancespot", _) => ping_binance_spot_private(&h.socket),
            ("private", "onOpen", "binance", _) => {
                open_binance_private(&h.socket, &resync, &opened, fetch)
            }
            ("private", "onOpen", "binancepm", _) => {
                open_binance_pm_private(&h.socket, &resync, &opened, fetch)
            }
            ("public", "onOpen", "binance", true) => open_binance_market(&h.socket, subscribe),
            ("public", "onOpen", "binancepm", true) => open_binance_pm_market(&h.socket, subscribe),
            ("public", "onOpen", "binance", false) => {
                open_binance_public(&h.socket, input.public_only, subscribe)
            }
            ("public", "onOpen", "binancepm", false) => {
                open_binance_pm_public(&h.socket, input.public_only, subscribe)
            }
            ("public", "onOpen", "binancespot", false) => {
                open_binance_spot_public(&h.socket, subscribe)
            }
            _ => unreachable!(),
        };
        result.expect("native venue operation continues after discarded reconnect rejection");
        h.parent.trace.borrow_mut().push(json!("caller-returned"));
    }
    let sends = !input.disposed
        && (input.scope == "private"
            || input.market
            || input.variant == "binancespot"
            || !input.public_only);
    assert_eq!(
        h.first.get(),
        usize::from(sends),
        "native first subscriber count"
    );
    assert_eq!(
        h.second.get(),
        usize::from(sends && !input.throws),
        "native later subscriber is skipped after throw"
    );
    assert_eq!(
        h.socket.diagnostics().on_close_count,
        usize::from(sends && !input.throws),
        "native close continuation is skipped after health failure"
    );
    assert!(
        h.runtime.errors.borrow().is_empty(),
        "native discarded rejection is deferred beyond caller return"
    );
    if sends {
        let health = Store::socket_health(h.store.as_ref()).unwrap().unwrap();
        // Source publishes four health flags and three defined diagnostics per
        // socket family; absent timestamps/close codes are omitted by this carrier.
        assert_eq!(
            health
                .as_object()
                .expect("native SocketHealth is a record")
                .len(),
            10,
            "native independent SocketHealth field count"
        );
        for (key, expected) in [
            ("publicHealthy", true),
            ("privateHealthy", true),
            ("publicInitialized", false),
            ("privateInitialized", false),
            ("publicReconnectPaused", false),
            ("privateReconnectPaused", false),
        ] {
            assert_eq!(
                health.get(key).and_then(Value::as_bool),
                Some(expected),
                "native SocketHealth boolean {key}"
            );
        }
        for key in [
            "publicCloseCountTotal",
            "publicCloseStreak",
            "privateCloseCountTotal",
            "privateCloseStreak",
        ] {
            assert_eq!(
                health.get(key).and_then(Value::as_f64),
                Some(0.0),
                "native SocketHealth counter {key}"
            );
        }
        assert_eq!(
            Store::socket_health(h.store.as_ref()).unwrap().unwrap()["publicHealthy"],
            true,
            "native health commits before failure"
        );
    }
    if input.scope == "private" && input.method == "onOpen" && !spot_open {
        assert_eq!(
            opened.get(),
            !input.disposed,
            "native private open sets hasOpened despite background rejection"
        );
        assert_eq!(
            resync.get(),
            input.disposed,
            "native private open clears resync despite background rejection"
        );
    }
    let book = h.socket.order_book_inner_state.borrow();
    let book = book.get("fixture").unwrap();
    let reset = input.scope == "public" && !input.market && !input.disposed;
    assert_eq!(
        book.updates.len(),
        usize::from(!reset),
        "native public open resets buffered updates despite background rejection"
    );
    assert_eq!(
        book.is_snapshot_loaded, !reset,
        "native public open clears snapshot flag despite background rejection"
    );
    let mut expected = vec![];
    if spot_open {
        expected.push(json!("caller-returned"));
    }
    if sends {
        expected.push(json!(["first", true]));
        if !input.throws {
            expected.push(json!(["second", true]));
        }
    }
    if !input.disposed && input.scope == "private" {
        expected.push(json!("diagnostic"));
    }
    if !input.disposed && input.scope == "private" && input.method == "onOpen" && !spot_open {
        expected.push(json!("fetch-orders"));
    }
    if !input.disposed && input.scope == "public" {
        expected.push(json!("subscribe"));
    }
    expected.push(json!(if spot_open {
        "caller-resolved"
    } else {
        "caller-returned"
    }));
    assert_eq!(
        *h.parent.trace.borrow(),
        expected,
        "native background and local continuation order"
    );
    h.runtime.pump();
    h.runtime.pump();
    let errors = h.runtime.errors.borrow();
    assert_eq!(
        errors.len(),
        usize::from(sends && input.throws),
        "native rejected reconnect is reported exactly once"
    );
    for error in errors.iter() {
        assert_eq!(
            error.name, "RangeError",
            "native report preserves original error name"
        );
        assert_eq!(
            error.message, "socket health subscriber failed",
            "native report preserves original error message"
        );
        assert_eq!(
            observed_error_code(&error.code),
            Some(json!("native-health")),
            "native report preserves original metadata"
        );
        assert_eq!(
            error.store_subscriber_ids,
            vec![41, 73, 1],
            "native report preserves subscriber boundary sequence"
        );
        assert!(
            std::ptr::eq(
                error
                    .source()
                    .expect("native report retains original opaque cause pointer"),
                h.cause.as_ref()
            ),
            "native report retains original opaque cause pointer"
        );
    }
    json!({"trace":*h.parent.trace.borrow(),"closeDelta":h.socket.diagnostics().on_close_count,"health":Store::socket_health(h.store.as_ref()).unwrap(),"book":{"updates":book.updates,"isSnapshotLoaded":book.is_snapshot_loaded},"hasOpened":opened.get(),"resyncOrdersOnOpen":resync.get(),"failure":errors.first().map(|e|json!({"name":e.name,"message":e.message,"code":observed_error_code(&e.code)})),"settlement":if !sends {None} else if errors.is_empty() {Some("resolved")} else {Some("rejected")}})
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn send_notification_native_continuations_match_actual_source() {
    let authored = inputs();
    nested_store_cause_native();
    // Complete every native assertion before starting the source/wire phase.
    let native: Vec<_> = authored.iter().map(|input| native_case(*input)).collect();
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/send-store.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 64, "authored source case floor");
    for (index, ((input, native), source)) in authored.iter().zip(&native).zip(&source).enumerate()
    {
        assert_eq!(
            source["variant"], input.variant,
            "source fixture identity case{index}"
        );
        assert_eq!(source["scope"], input.scope, "source scope case{index}");
        assert_eq!(
            source["className"].as_str(),
            input.class_name,
            "source class case{index}"
        );
        assert_eq!(source["method"], input.method, "source method case{index}");
        assert_eq!(
            source["market"], input.market,
            "source market identity case{index}"
        );
        assert_eq!(
            source["publicDataOnly"], input.public_only,
            "source public-only identity case{index}"
        );
        assert_eq!(
            source["disposed"], input.disposed,
            "source disposal identity case{index}"
        );
        assert_eq!(
            source["listenerThrows"], input.throws,
            "source failure identity case{index}"
        );
        for key in [
            "trace",
            "health",
            "book",
            "hasOpened",
            "resyncOrdersOnOpen",
            "settlement",
            "failure",
        ] {
            assert_eq!(
                wire_numbers(&native[key]),
                wire_numbers(&source[key]),
                "source {key} case{index}"
            );
        }
        // Source starts at100 to stop after its existing >100 guard; native
        // starts at0 and leaves the real reconnect timer pending. Compare the
        // independently asserted increments, not absolute initial counters.
        assert_eq!(
            native["closeDelta"].as_u64().unwrap(),
            source["onCloseCount"].as_u64().unwrap() - 100,
            "source close counter increment case{index}"
        );
    }
}

#[test]
fn discarded_report_survives_socket_disposal_once() {
    for inline in [false, true] {
        let h = Fixture::new(false, false, true, inline);
        ping_binance_private(&h.socket).expect("native ping succeeds before disposal");
        assert!(
            h.runtime.errors.borrow().is_empty(),
            "native report is deferred even with inline task polling"
        );
        h.socket.dispose();
        h.runtime.pump();
        h.runtime.pump();
        h.runtime.pump();
        assert_eq!(
            h.runtime.errors.borrow().len(),
            1,
            "native detached rejection survives socket disposal exactly once"
        );
        assert!(
            std::ptr::eq(
                h.runtime.errors.borrow()[0].source().unwrap(),
                h.cause.as_ref()
            ),
            "native detached original cause survives socket disposal"
        );
    }
}

#[test]
fn failed_health_does_not_set_reconnecting_before_the_next_send() {
    let h = Fixture::new(false, false, true, true);
    ping_binance_private(&h.socket).unwrap();
    ping_binance_private(&h.socket).unwrap();
    assert_eq!(
        h.first.get(),
        2,
        "native failed health leaves reconnect retryable on second send"
    );
    assert_eq!(
        h.second.get(),
        0,
        "native two failed notifications both skip later subscriber"
    );
    assert_eq!(
        h.socket.diagnostics().on_close_count,
        0,
        "native both failed reconnects skip close counter"
    );
    h.runtime.pump();
    h.runtime.pump();
    assert_eq!(
        h.runtime.errors.borrow().len(),
        2,
        "native each discarded rejection is independently reported"
    );
}

#[test]
fn public_subscribe_disposal_does_not_cancel_prior_rejection() {
    for owner in 0..5 {
        let h = Fixture::new(false, false, true, true);
        let subscribe = || {
            assert!(
                h.runtime.errors.borrow().is_empty(),
                "native public subscribe precedes rejection reporting"
            );
            h.socket.dispose();
            Ok(())
        };
        let result = match owner {
            0 => open_binance_market(&h.socket, subscribe),
            1 => open_binance_public(&h.socket, false, subscribe),
            2 => open_binance_pm_market(&h.socket, subscribe),
            3 => open_binance_pm_public(&h.socket, false, subscribe),
            4 => open_binance_spot_public(&h.socket, subscribe),
            _ => unreachable!(),
        };
        result.expect("native public subscribe can dispose after background rejection");
        h.runtime.pump();
        h.runtime.pump();
        h.runtime.pump();
        assert_eq!(
            h.runtime.errors.borrow().len(),
            1,
            "native all five public owners retain rejected promise past subscribe disposal"
        );
        assert!(
            std::ptr::eq(
                h.runtime.errors.borrow()[0]
                    .source()
                    .expect("native disposed public owner retains original cause"),
                h.cause.as_ref()
            ),
            "native disposed public owner retains original cause pointer"
        );
    }
}

#[test]
fn direct_warning_failure_remains_synchronous() {
    let h = Fixture::new(false, false, true, true);
    h.parent.fail_warning.set(true);
    let error =
        ping_binance_private(&h.socket).expect_err("native direct warning error must propagate");
    assert_eq!(
        error.message, "direct warning failed",
        "native direct warning preserves its own failure"
    );
    assert_eq!(
        h.first.get(),
        0,
        "native direct warning precedes reconnect health publication"
    );
    h.runtime.pump();
    h.runtime.pump();
    assert!(
        h.runtime.errors.borrow().is_empty(),
        "native direct warning is not a discarded reconnect rejection"
    );
}

#[test]
fn discarded_reconnect_prefix_error_is_queued_while_explicit_reconnect_remains_fallible() {
    let h = Fixture::new(false, false, true, true);
    h.parent.fail_reconnect_info.set(true);
    ping_binance_private(&h.socket)
        .expect("native async reconnect prefix error cannot throw from ping");
    assert_eq!(
        *h.parent.trace.borrow(),
        vec![json!("diagnostic")],
        "native ping diagnostic follows rejected reconnect prefix"
    );
    assert_eq!(
        h.first.get(),
        0,
        "native reconnect log failure precedes health publication"
    );
    assert!(
        h.runtime.errors.borrow().is_empty(),
        "native reconnect prefix log rejection is deferred"
    );
    h.runtime.pump();
    h.runtime.pump();
    assert_eq!(
        h.runtime.errors.borrow().len(),
        1,
        "native discarded reconnect prefix rejection reports once"
    );
    assert_eq!(
        h.runtime.errors.borrow()[0].message,
        "reconnect info failed",
        "native reconnect prefix preserves original log error"
    );
    let error = h
        .socket
        .reconnect()
        .expect_err("native explicit reconnect caller retains its failure channel");
    assert_eq!(
        error.message, "reconnect info failed",
        "native explicit reconnect failure value"
    );
    h.runtime.pump();
    assert_eq!(
        h.runtime.errors.borrow().len(),
        1,
        "native observed explicit reconnect adds no detached report"
    );
}

#[test]
fn private_ping_family_continues_after_background_notification_failure() {
    let selected: Vec<_> = inputs()
        .into_iter()
        .filter(|input| input.method == "ping")
        .collect();
    assert_eq!(
        selected.len(),
        12,
        "authored three ping owners by disposal and subscriber failure"
    );
    for input in selected {
        native_case(input);
    }
}
#[test]
fn private_open_family_continues_after_background_notification_failure() {
    let selected: Vec<_> = inputs()
        .into_iter()
        .filter(|input| {
            input.scope == "private" && input.method == "onOpen" && input.variant != "binancespot"
        })
        .collect();
    assert_eq!(
        selected.len(),
        8,
        "authored two private open owners by disposal and subscriber failure"
    );
    for input in selected {
        native_case(input);
    }
}
#[test]
fn spot_open_family_does_not_catch_discarded_background_notification_failure() {
    let selected: Vec<_> = inputs()
        .into_iter()
        .filter(|input| {
            input.scope == "private" && input.method == "onOpen" && input.variant == "binancespot"
        })
        .collect();
    assert_eq!(
        selected.len(),
        4,
        "authored Spot open disposal and subscriber failure cases"
    );
    for input in selected {
        native_case(input);
    }
}
#[test]
fn public_open_family_continues_after_background_notification_failure() {
    let selected: Vec<_> = inputs()
        .into_iter()
        .filter(|input| input.scope == "public")
        .collect();
    assert_eq!(
        selected.len(),
        40,
        "authored five public owners by public-only, disposal and subscriber failure"
    );
    for input in selected {
        native_case(input);
    }
}

fn nested_store_cause_native() {
    let mut original = ExchangeError::new("nested health failure");
    original.name = "RangeError".into();
    original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "nested-health"
    )));
    let original: Rc<dyn Error> = Rc::new(original);
    let first_store = DefaultStore::new();
    let first_cause = StoreSubscriberError::new("first wrapper").with_cause(original.clone());
    let _first_subscription =
        Store::subscribe(&first_store, Box::new(move |_, _| Err(first_cause.clone())));
    let health = json!({"publicHealthy":true,"updatedAt":7});
    let first: ExchangeError = first_store
        .update_socket_health(health.as_object().unwrap())
        .unwrap_err()
        .into();
    assert!(
        std::ptr::eq(
            first
                .source()
                .expect("native first Store retains original cause"),
            original.as_ref()
        ),
        "native first Store retains original cause"
    );
    let second_store = DefaultStore::new();
    let _second_subscription = Store::subscribe(
        &second_store,
        Box::new(move |_, _| Err(first.clone().into())),
    );
    let second: ExchangeError = second_store
        .update_socket_health(health.as_object().unwrap())
        .unwrap_err()
        .into();
    assert_eq!(
        second.name, "RangeError",
        "native nested Store preserves original error name"
    );
    assert_eq!(
        observed_error_code(&second.code),
        Some(json!("nested-health")),
        "native nested Store preserves original metadata"
    );
    assert_eq!(
        second.store_subscriber_ids,
        vec![0, 0],
        "native independent Store boundary IDs retain both local zero subscribers"
    );
    assert!(
        std::ptr::eq(
            second
                .source()
                .expect("native nested Store retains original cause"),
            original.as_ref()
        ),
        "native nested Store retains original cause"
    );
}
#[test]
fn nested_store_boundaries_retain_the_original_opaque_cause() {
    nested_store_cause_native();
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
