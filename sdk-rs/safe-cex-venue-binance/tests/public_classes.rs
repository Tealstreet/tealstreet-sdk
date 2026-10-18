#![allow(dead_code)]
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{runtime::*, websocket::*};
use safe_cex_store::Store;
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeOptions;

use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
fn run<T>(f: impl Future<Output = T>) -> T {
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
type SendHook = Box<dyn Fn(&str)>;
struct Socket {
    hook: RefCell<Option<SendHook>>,
    sent: RefCell<Vec<String>>,
    trace: Rc<RefCell<Vec<&'static str>>>,
    fail: Cell<bool>,
}
impl WsSocket for Socket {
    fn ready_state(&self) -> u8 {
        1
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push("send");
        self.sent.borrow_mut().push(message.into());
        if let Some(hook) = self.hook.borrow().as_ref() {
            hook(message);
        }
        if self.fail.get() {
            let mut e = ExchangeError::new("fixture send");
            e.metadata.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
                "fixture-code"
            )));
            Err(e)
        } else {
            Ok(())
        }
    }
    fn close(&self) {}
    fn terminate(&self) {}
    fn reconnect(&self) {}
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct Runtime {
    diagnostics: bool,
    now: Cell<f64>,
    socket: Rc<Socket>,
    trace: Rc<RefCell<Vec<&'static str>>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.trace.borrow_mut().push("clock");
        self.now.get()
    }
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
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
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
impl WsRuntime for Runtime {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        panic!("unexpected tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let s: Rc<dyn WsSocket> = self.socket.clone();
        Box::pin(async move { Ok(s) })
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
        self.diagnostics
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
struct Parent {
    trace: Rc<RefCell<Vec<&'static str>>>,
    diagnostic_frames: RefCell<Vec<String>>,
    disposed: Cell<bool>,
    options: ExchangeOptions,
    store: Rc<DefaultStore>,
    latency: RefCell<Option<f64>>,
}
impl WsParent for Parent {
    fn name(&self) -> &str {
        "binance"
    }
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> ExchangeOptions {
        self.options.clone()
    }
    fn store(&self) -> Rc<dyn Store> {
        self.store.clone()
    }
    fn is_private_socket(&self, _: &BaseWebSocket) -> bool {
        true
    }
    fn resync_coordinator(&self) -> Rc<ReopenResync> {
        Rc::new(ReopenResync::default())
    }
    fn fetch_and_set_private_snapshot(&self, _: usize) -> ExchangeFuture<'static, ()> {
        panic!("unexpected snapshot")
    }
    fn log(&self, _: &str, _: LogSeverity) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn emit(&self, event: &str, message: &str) -> Result<(), ExchangeError> {
        if event == "grafana-pws-sub" {
            self.trace.borrow_mut().push("diagnostic");
            self.diagnostic_frames.borrow_mut().push(message.into());
        }
        Ok(())
    }
    fn sync_socket_health_to_store(&self) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn update_latency(&self, n: f64) -> Result<(), ExchangeError> {
        *self.latency.borrow_mut() = Some(n);
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
        Ok(())
    }
}

#[path = "support/class_listener_host.rs"]
mod listener_operations;
use safe_cex_venue_binance::{
    private_event_value::*,
    public_classes::*,
    public_listeners::{InnerState, ListenerHost, Parser},
};
fn fixture(kind: PublicKind) -> (Rc<PublicSocket>, Rc<Parent>, Rc<Socket>) {
    let trace = Rc::new(RefCell::new(vec![]));
    let socket = Rc::new(Socket {
        hook: RefCell::new(None),
        sent: RefCell::new(vec![]),
        trace: trace.clone(),
        fail: Cell::new(false),
    });
    let runtime = Rc::new(Runtime {
        diagnostics: false,
        now: Cell::new(1000.0),
        socket: socket.clone(),
        trace: trace.clone(),
    });
    let parent = Rc::new(Parent {
        trace,
        diagnostic_frames: RefCell::new(vec![]),
        disposed: Cell::new(false),
        options: serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
        store: Rc::new(DefaultStore::new()),
        latency: RefCell::new(None),
    });
    let owner = PublicSocket::new(
        kind,
        1,
        PublicDependencies {
            parent: parent.clone(),
            runtime,
            operations: listener_operations::Host::new("normal"),
        },
    );
    run(owner
        .base()
        .create_websocket("wss://fixture.invalid".into(), vec![], None))
    .unwrap();
    (owner, parent, socket)
}
#[test]
fn public_classes_bind_defaults_parent_sibling_and_open() {
    let mut wire = vec![];
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::FuturesPublic,
        PublicKind::PMMarket,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let (owner, parent, socket) = fixture(kind);
        assert!(
            Rc::ptr_eq(&owner.parent, &(parent.clone() as Rc<dyn WsParent>)),
            "native public original parent"
        );
        let expected: &[&str] = match kind {
            PublicKind::FuturesMarket | PublicKind::PMMarket => &["24hrTicker", "markPriceUpdate"],
            PublicKind::SpotPublic => &["24hrTicker", "bookTicker"],
            _ => &["bookTicker"],
        };
        assert_eq!(
            owner
                .base()
                .message_handlers
                .borrow()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            expected,
            "native public class default keys"
        );
        for name in expected {
            let callback = owner.handlers(name).unwrap();
            assert_eq!(callback.len(), 1, "independent default handler count");
            assert!(
                Rc::ptr_eq(&callback[0], &owner.handlers(name).unwrap()[0]),
                "native stable arrow identity"
            );
        }
        assert!(
            owner.listeners.handlers.borrow().is_empty(),
            "native class uses only core registry"
        );
        assert!(owner.listeners.topics.borrow().is_empty());
        assert!(owner.listeners.inner.borrow().is_empty());
        assert_eq!(
            owner.listeners.next_subscription_id.get(),
            2.0,
            "native Futures market ID initial2"
        );
        if matches!(kind, PublicKind::FuturesPublic | PublicKind::PMPublic) {
            let sibling = owner.market_websocket.get().unwrap();
            assert!(
                Rc::ptr_eq(&owner.parent, &sibling.parent),
                "native sibling same parent"
            );
            assert!(
                !Rc::ptr_eq(owner.base(), sibling.base()),
                "native sibling separate socket"
            );
            assert_eq!(
                sibling.kind,
                if kind == PublicKind::FuturesPublic {
                    PublicKind::FuturesMarket
                } else {
                    PublicKind::PMMarket
                },
                "native actual sibling kind"
            );
        } else {
            assert!(owner.market_websocket.get().is_none(), "native no sibling");
        }
        owner
            .listeners
            .topics
            .borrow_mut()
            .insert("eth@depth".into(), 2.0);
        let inner = Rc::new(InnerState::default());
        inner.updates.borrow_mut().push(number(4.2));
        inner.snapshot_loaded.set(true);
        owner
            .listeners
            .inner
            .borrow_mut()
            .insert("book".into(), inner.clone());
        owner.on_open().unwrap();
        assert_eq!(
            socket.sent.borrow().len(),
            2,
            "native open default plus one retained topic"
        );
        let first: &str = match kind {
            PublicKind::FuturesMarket | PublicKind::PMMarket => {
                r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!markPrice@arr@1s"]}"#
            }
            PublicKind::SpotPublic => {
                r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!bookTicker"]}"#
            }
            _ => r#"{"id":1,"method":"SUBSCRIBE","params":["!bookTicker"]}"#,
        };
        let decoded: serde_json::Value = serde_json::from_str(&socket.sent.borrow()[0]).unwrap();
        assert_eq!(
            decoded["id"].as_f64(),
            Some(1.0),
            "native decoded initial subscription ID"
        );
        assert_eq!(decoded["method"].as_str(), Some("SUBSCRIBE"));
        let n = if matches!(
            kind,
            PublicKind::FuturesMarket | PublicKind::PMMarket | PublicKind::SpotPublic
        ) {
            2
        } else {
            1
        };
        assert_eq!(
            decoded["params"].as_array().unwrap().len(),
            n,
            "independent decoded initial topic count"
        );
        let topics: &[&str] = match kind {
            PublicKind::FuturesMarket | PublicKind::PMMarket => {
                &["!ticker@arr", "!markPrice@arr@1s"]
            }
            PublicKind::SpotPublic => &["!ticker@arr", "!bookTicker"],
            _ => &["!bookTicker"],
        };
        assert_eq!(
            decoded["params"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            topics,
            "native decoded initial subscription topic values"
        );
        let second: Value = serde_json::from_str(&socket.sent.borrow()[1]).unwrap();
        assert_eq!(
            second["id"].as_f64(),
            Some(10.0),
            "native decoded retained-topic ID"
        );
        assert_eq!(second["method"].as_str(), Some("SUBSCRIBE"));
        assert_eq!(
            second["params"].as_array().unwrap().len(),
            1,
            "independent retained-topic count"
        );
        assert_eq!(
            second["params"][0].as_str(),
            Some("eth@depth"),
            "native decoded retained topic value"
        );
        wire.push((socket.sent.borrow()[0].clone(), first));
        assert_eq!(
            inner.snapshot_loaded.get(),
            matches!(kind, PublicKind::FuturesMarket | PublicKind::PMMarket),
            "native reset authoritative inner object"
        );
        assert_eq!(
            inner.updates.borrow().len(),
            if matches!(kind, PublicKind::FuturesMarket | PublicKind::PMMarket) {
                1
            } else {
                0
            },
            "independent reset count"
        );
        parent.disposed.set(true);
        let before = socket.sent.borrow().len();
        owner.on_open().unwrap();
        assert_eq!(
            socket.sent.borrow().len(),
            before,
            "native disposed skips open"
        );
    }
    for (actual, expected) in wire {
        assert_eq!(
            actual, expected,
            "wire payload after all native construction/open assertions"
        )
    }
}
#[test]
fn public_classes_share_native_payload_callbacks_reentrancy_and_failure() {
    let (owner, _, _) = fixture(PublicKind::FuturesMarket);
    let event = Rc::new(RefCell::new(None));
    let captured = event.clone();
    let owner_copy = owner.clone();
    let nested = Rc::new(Cell::new(false));
    let nesting = nested.clone();
    let callback: Parser = Rc::new(move |events| {
        if !nesting.get() {
            *captured.borrow_mut() = Some(events.read("0")?);
            events.read("0")?.write("native", number(f64::INFINITY));
            nesting.set(true);
            owner_copy.on_message(r#"{"e":"nested","s":"BTC"}"#)?;
        }
        Ok(())
    });
    owner.replace_handlers("kline".into(), vec![callback]);
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    let original = event.clone();
    let exact: Parser = Rc::new(move |events| {
        let first = events.read("0")?;
        assert!(
            first.same(original.borrow().as_ref().unwrap()),
            "native exact retains original parsed object"
        );
        assert!(
            matches!(first.read("native")?,EventValue::Number(v)if v==f64::INFINITY),
            "native exact sees nonfinite mutation without JSON projection"
        );
        counted.set(counted.get() + 1);
        Ok(())
    });
    owner.replace_handlers("kline.BTC".into(), vec![exact]);
    owner.on_message(r#"{"e":"kline","s":"BTC"}"#).unwrap();
    assert_eq!(calls.get(), 1, "independent exact callback count");
    let retained = safe_cex_types::retained_json::RetainedJsonInput::Object(vec![
        (
            "data".encode_utf16().collect(),
            safe_cex_types::retained_json::RetainedJsonInput::Text(vec![0xd800]),
        ),
        (
            "zero".encode_utf16().collect(),
            safe_cex_types::retained_json::RetainedJsonInput::Float(-0.),
        ),
    ]);
    let primary = Rc::new(
        ExchangeError::new("class callback fixture").with_retained_response(retained.clone()),
    );
    let e = primary.clone();
    owner.replace_handlers("error".into(), vec![Rc::new(move |_| Err(e.clone()))]);
    let actual = owner.on_message(r#"{"e":"error"}"#).unwrap_err();
    assert!(
        Rc::ptr_eq(&actual, &primary),
        "native bridged original failure identity"
    );
    let core = owner.base().message_handlers.borrow()["error"].borrow()[0].clone();
    let bridged = core(&EventValue::Null).unwrap_err();
    assert_eq!(
        bridged.retained_response_input(),
        Some(retained),
        "direct core callback preserves retained failure fields"
    );
    let safe_cex_types::retained_json::RetainedJsonInput::Object(fields) =
        bridged.retained_response_input().unwrap()
    else {
        panic!("retained response object");
    };
    assert!(
        matches!(fields[1].1, safe_cex_types::retained_json::RetainedJsonInput::Float(value) if value.to_bits() == 0x8000_0000_0000_0000)
    );
    owner.replace_handlers("good".into(), vec![Rc::new(|_| Ok(()))]);
    owner.on_message(r#"{"e":"good"}"#).unwrap();
}
#[test]
fn public_classes_native_ticker_bindings_before_actual_source() {
    let mut input = vec![];
    let mut native = vec![];
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::FuturesPublic,
        PublicKind::PMMarket,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let (owner, parent, socket) = fixture(kind);
        let ticker:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
        Store::update(
            parent.store.as_ref(),
            safe_cex_store::StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let keys = owner
            .base()
            .message_handlers
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let sibling=owner.market_websocket.get().map(|s|json!({"sameParent":Rc::ptr_eq(&owner.parent,&s.parent),"separate":!Rc::ptr_eq(&owner,&s),"name":if s.kind==PublicKind::FuturesMarket{"BinanceMarketWebsocket"}else{"BinancePMMarketWebsocket"}}));
        owner
            .listeners
            .topics
            .borrow_mut()
            .insert("eth@depth".into(), 2.0);
        let inner = Rc::new(InnerState::default());
        inner.updates.borrow_mut().push(number(4.2));
        inner.snapshot_loaded.set(true);
        owner
            .listeners
            .inner
            .borrow_mut()
            .insert("book".into(), inner.clone());
        owner.on_open().unwrap();
        let event = if matches!(
            kind,
            PublicKind::FuturesMarket | PublicKind::PMMarket | PublicKind::SpotPublic
        ) {
            "24hrTicker"
        } else {
            "bookTicker"
        };
        owner.on_message(&format!(r#"{{"e":"{event}","s":"BTC","c":"4.2","P":"1.2","v":"3.6","q":"9.1","b":"2.5","a":"12.3"}}"#)).unwrap();
        let ticker = parent.store.tickers().unwrap()[0].clone();
        let regular = event == "24hrTicker";
        assert_eq!(
            ticker.last,
            if regular { 4.2 } else { 0.0 },
            "native concrete ticker last"
        );
        assert_eq!(ticker.percentage, if regular { 1.2 } else { 0.0 });
        assert_eq!(ticker.volume, if regular { 3.6 } else { 0.0 });
        assert_eq!(ticker.quote_volume, if regular { 9.1 } else { 0.0 });
        assert_eq!(
            ticker.bid,
            if regular { 0.0 } else { 2.5 },
            "native concrete bid"
        );
        assert_eq!(
            ticker.ask,
            if regular { 0.0 } else { 12.3 },
            "native concrete ask"
        );
        assert_eq!(
            ticker.mark,
            if kind == PublicKind::SpotPublic {
                4.2
            } else {
                0.0
            },
            "native Spot mark bound to Spot body"
        );
        assert_eq!(
            ticker.index.unwrap(),
            if kind == PublicKind::SpotPublic {
                4.2
            } else {
                0.0
            }
        );
        assert_eq!(ticker.funding_rate, 0.0);
        assert_eq!(ticker.funding_time, 0.0);
        assert_eq!(ticker.id, "btc");
        assert_eq!(ticker.symbol, "BTC");
        let stats = owner.base().message_handlers_stats.borrow();
        assert_eq!(stats.len(), 1, "independent event statistics count");
        assert_eq!(
            stats[event].last_callbacks_count,
            safe_cex_num::PrototypeScalar::from(Some(1)),
            "native class actual callbacks count"
        );
        assert_eq!(stats[event].last_message_timestamp, 1000.0);
        assert_eq!(stats[event].had_handlers, Some(true));
        drop(stats);
        input.push(json!({"kind":format!("{kind:?}")}));
        native.push(json!({"kind":format!("{kind:?}"),"keys":keys,"sibling":sibling,"frames":*socket.sent.borrow(),"book":{"updates":inner.updates.borrow().len(),"loaded":inner.snapshot_loaded.get()},"ticker":{"id":ticker.id,"symbol":ticker.symbol,"last":ticker.last,"mark":ticker.mark,"index":ticker.index.unwrap(),"percentage":ticker.percentage,"volume":ticker.volume,"quoteVolume":ticker.quote_volume,"fundingRate":ticker.funding_rate,"fundingTime":ticker.funding_time,"bid":ticker.bid,"ask":ticker.ask},"stats":[event]}));
    }
    assert_eq!(input.len(), 5, "independent public class floor");
    use std::io::Write;
    let mut child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-classes.mjs"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&input).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "actual class oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 5, "independent actual source floor");
    fn numeric(v: Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().expect("finite fixture wire number")),
            Value::Array(v) => Value::Array(v.into_iter().map(numeric).collect()),
            Value::Object(v) => {
                Value::Object(v.into_iter().map(|(k, v)| (k, numeric(v))).collect())
            }
            v => v,
        }
    }
    assert_eq!(
        numeric(json!(native)),
        numeric(json!(source)),
        "actual class bodies after all native field/binding assertions"
    );
}

#[test]
fn public_classes_preserve_foreign_core_callback_on_listener_registration() {
    let (owner, _, _) = fixture(PublicKind::FuturesMarket);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let original: MessageHandler = Rc::new(move |events| {
        assert_eq!(
            events
                .get("0")?
                .get("k")?
                .get("s")?
                .strict_equals(&EventValue::str("BTC")),
            true,
            "native foreign callback receives parsed envelope"
        );
        count.set(count.get() + 1);
        Ok(())
    });
    owner.base().message_handlers.borrow_mut().insert(
        "kline.1m.BTC".into(),
        Rc::new(RefCell::new(vec![original.clone()])),
    );
    let unsubscribe = owner
        .listen_ohlcv(
            EventValue::from_json(json!({"symbol":"BTC","interval":"1m"})),
            Rc::new(|candle| {
                assert_eq!(candle.close, 4.2, "native new candle callback");
                Ok(())
            }),
        )
        .unwrap();
    let retained = owner.base().message_handlers.borrow()["kline.1m.BTC"].borrow()[0].clone();
    assert!(
        Rc::ptr_eq(&retained, &original),
        "native registration preserves original foreign callback identity"
    );
    assert_eq!(
        owner.base().message_handlers.borrow()["kline.1m.BTC"]
            .borrow()
            .len(),
        2,
        "independent foreign plus candle count"
    );
    owner.on_message(r#"{"e":"kline","s":"BTC","k":{"s":"BTC","i":"1m","t":1200,"o":"1.2","h":"9.1","l":"0.5","c":"4.2","v":"3.6"}}"#).unwrap();
    assert_eq!(calls.get(), 1, "independent foreign invocation count");
    unsubscribe().unwrap();
    assert_eq!(
        owner.base().message_handlers.borrow()["kline.1m.BTC"]
            .borrow()
            .len(),
        1,
        "native unsubscribe preserves foreign callback"
    );
    owner
        .on_message(r#"{"e":"kline","s":"BTC","k":{"s":"BTC","i":"1m"},"native":1e400}"#)
        .unwrap();
    assert_eq!(
        calls.get(),
        2,
        "nonfinite unrelated data retains the foreign callback frame"
    );
}

#[test]
fn public_bound_callback_and_core_protocol_retain_receiver_without_cycle() {
    let (owner, parent, _) = fixture(PublicKind::SpotPublic);
    let ticker:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    Store::update(
        parent.store.as_ref(),
        safe_cex_store::StoreUpdate {
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let state = Rc::downgrade(&**owner);
    let arrow = owner.handlers("24hrTicker").unwrap()[0].clone();
    let base = owner.base().clone();
    drop(owner);
    assert!(
        state.upgrade().is_some(),
        "native saved arrow and Base retain receiver state"
    );
    arrow(EventValue::from_json(
        json!([{"s":"BTC","c":"4.2","P":"1.2","v":"3.6","q":"9.1"}]),
    ))
    .unwrap();
    assert_eq!(
        parent.store.tickers().unwrap()[0].last,
        4.2,
        "native saved arrow works after public handle drop"
    );
    base.on_message(&safe_cex_transport::socket::SocketFrame::Text(
        r#"{"e":"bookTicker","s":"BTC","b":"2.5","a":"12.3"}"#.into(),
    ))
    .unwrap();
    assert_eq!(
        parent.store.tickers().unwrap()[0].ask,
        12.3,
        "native retained Base protocol works after public handle drop"
    );
    drop(base);
    assert!(
        state.upgrade().is_some(),
        "native saved arrow independently retains receiver"
    );
    drop(arrow);
    assert!(
        state.upgrade().is_none(),
        "native no receiver Base callback cycle after last lease drops"
    );
}

#[test]
fn public_depth_registry_does_not_retain_its_own_socket_lease() {
    for kind in [
        PublicKind::FuturesPublic,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let (owner, _, _) = fixture(kind);
        let state = Rc::downgrade(&**owner);
        let base = Rc::downgrade(owner.base());
        let unsubscribe = owner
            .listen_order_book("BTC".into(), Rc::new(|_| Ok(())))
            .unwrap();
        assert_eq!(
            owner.base().message_handlers.borrow()["depthUpdate.BTC"]
                .borrow()
                .len(),
            1,
            "independent native depth callback count"
        );
        drop(owner);
        assert!(
            base.upgrade().is_some(),
            "native exported depth unsubscribe retains socket lease"
        );
        drop(unsubscribe);
        assert!(
            base.upgrade().is_none(),
            "native dropping depth handle does not leave Base registry cycle"
        );
        assert!(
            state.upgrade().is_none(),
            "native dropping depth handle releases public receiver"
        );
    }
    let (owner, _, socket) = fixture(PublicKind::SpotPublic);
    let unsubscribe = owner
        .listen_order_book("BTC".into(), Rc::new(|_| Ok(())))
        .unwrap();
    unsubscribe().unwrap();
    assert_eq!(
        owner.base().message_handlers.borrow()["depthUpdate.BTC"]
            .borrow()
            .len(),
        0,
        "native depth unsubscribe reads authoritative handler map"
    );
    assert_eq!(
        socket.sent.borrow().len(),
        2,
        "independent depth subscribe and unsubscribe payload count"
    );
}
#[test]
fn saved_core_bridge_decodes_numeric_lexeme_without_frame() {
    let (owner, _, _) = fixture(PublicKind::FuturesMarket);
    owner.replace_handlers(
        "saved".into(),
        vec![Rc::new(|events| {
            assert!(
                matches!(events.read("0")?.read("native")?,EventValue::Number(v)if v==f64::INFINITY),
                "native saved core bridge preserves overflow Number"
            );
            Ok(())
        })],
    );
    let callback = owner.base().message_handlers.borrow()["saved"].borrow()[0].clone();
    let data: Value = serde_json::from_str(r#"[{"native":1e400}]"#).unwrap();
    callback(&safe_cex_exchange::js_runtime::value::from_json(&data)).unwrap();
}

#[test]
fn public_captured_ticker_reference_observes_id_rename_before_dictionary_replacement() {
    for kind in [
        PublicKind::FuturesPublic,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let (owner, parent, _) = fixture(kind);
        let ticker:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
        Store::update(
            parent.store.as_ref(),
            safe_cex_store::StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let renamed = Rc::new(Cell::new(false));
        let once = renamed.clone();
        let store = parent.store.clone();
        let stop = Store::subscribe(
            parent.store.as_ref(),
            Box::new(move |_, _| {
                if !once.replace(true) {
                    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
                    patch.set_json("id", json!("renamed"));
                    Store::update_ticker(store.as_ref(), "btc", &patch).unwrap();
                    let mut next = store.tickers().unwrap()[0].clone();
                    next.bid = 0.0;
                    next.ask = 0.0;
                    Store::update(
                        store.as_ref(),
                        safe_cex_store::StoreUpdate {
                            tickers: Some(vec![next]),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                }
                Ok(())
            }),
        );
        owner
            .handle_stream(
                "bookTicker",
                EventValue::from_json(
                    json!([{"s":"BTC","b":"2.5","a":"12.3"},{"s":"BTC","b":"3.6","a":"4.2"}]),
                ),
            )
            .unwrap();
        assert!(renamed.get());
        assert_eq!(
            parent.store.tickers().unwrap()[0].id,
            "renamed",
            "native current row retains renamed ID"
        );
        assert_eq!(
            parent.store.tickers().unwrap()[0].bid,
            3.6,
            "native captured ticker ID observes mutation before replacement"
        );
        assert_eq!(
            parent.store.tickers().unwrap()[0].ask,
            4.2,
            "native captured ticker reference updates replacement row through renamed ID"
        );
        stop();
    }
}

#[test]
fn public_stock_inherited_ticker_target_still_parses_fields() {
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::PMMarket,
        PublicKind::SpotPublic,
    ] {
        let (owner, _, _) = fixture(kind);
        let failure=owner.on_message(r#"{"e":"24hrTicker","s":"constructor","c":{"toString":7,"valueOf":8},"P":"1.2","v":"3.6","q":"9.1"}"#).unwrap_err();
        assert_eq!(
            failure.name, "TypeError",
            "native inherited dictionary target reaches numeric field coercion"
        );
        assert_eq!(failure.message, "Cannot convert object to primitive value");
    }
}

#[test]
fn public_same_generation_symbol_alias_receives_second_tick() {
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::PMMarket,
        PublicKind::SpotPublic,
    ] {
        let (owner, parent, _) = fixture(kind);
        let ticker: safe_cex_types::Ticker = serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
        Store::update(
            parent.store.as_ref(),
            safe_cex_store::StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let once = Rc::new(Cell::new(false));
        let renamed = once.clone();
        let store = parent.store.clone();
        let stop = Store::subscribe(
            parent.store.as_ref(),
            Box::new(move |_, _| {
                if !renamed.replace(true) {
                    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
                    patch.set_json("symbol", json!("ETH"));
                    Store::update_ticker(store.as_ref(), "btc", &patch).unwrap();
                }
                Ok(())
            }),
        );
        owner
            .handle_stream(
                "24hrTicker",
                EventValue::from_json(json!([
                    {"s":"BTC","c":"4.2","P":"1.2","v":"3.6","q":"9.1"},
                    {"s":"ETH","c":"12.3","P":"2.1","v":"5.6","q":"8.1"}
                ])),
            )
            .unwrap();
        assert_eq!(
            parent.store.tickers().unwrap()[0].symbol,
            "ETH",
            "native subscriber renames ticker symbol"
        );
        assert_eq!(
            parent.store.tickers().unwrap()[0].last,
            12.3,
            "native same-generation alias receives second price 12.3 rather than 4.2"
        );
        stop();
    }
}

#[test]
fn public_surrogate_frames_retain_raw_units_and_reach_callbacks() {
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::FuturesPublic,
        PublicKind::PMMarket,
        PublicKind::PMPublic,
        PublicKind::SpotPublic,
    ] {
        let (owner, _, _) = fixture(kind);
        let count = Rc::new(Cell::new(0));
        let seen = count.clone();
        let raw = Rc::new(RefCell::new(Vec::<Vec<u16>>::new()));
        let observed = raw.clone();
        owner.replace_handlers(
            "kline".into(),
            vec![Rc::new(move |events| {
                let EventValue::String(units) = events.get("0")?.get("x")? else {
                    panic!("native source string");
                };
                observed.borrow_mut().push(units.to_vec());
                seen.set(seen.get() + 1);
                Ok(())
            })],
        );
        owner
            .on_message(r#"{"e":"kline","s":"BTC","x":"ok"}"#)
            .unwrap();
        assert_eq!(
            count.replace(0),
            1,
            "native ordinary scalar frame invokes public callback"
        );
        owner
            .on_message(r#"{"e":"kline","s":"BTC","x":"\ud800"}"#)
            .unwrap();
        assert_eq!(
            count.get(),
            1,
            "native escaped surrogate frame reaches public callback"
        );
        assert_eq!(
            *raw.borrow(),
            [vec![111, 107], vec![0xd800]],
            "native callbacks retain exact ordinary and surrogate units"
        );
    }
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-boundary-findings.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "source boundary assertions: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source["dispatch"]["callbacks"], 1,
        "FORCED actual source delivers surrogate frame"
    );
    assert_eq!(
        source["dispatch"]["unit"], 55296,
        "FORCED actual source callback observes lone code unit"
    );
}

#[test]
fn public_symbol_collision_uses_latest_store_alias_owner() {
    for kind in [
        PublicKind::FuturesMarket,
        PublicKind::PMMarket,
        PublicKind::SpotPublic,
    ] {
        let (owner, parent, _) = fixture(kind);
        let one:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
        let mut two = one.clone();
        two.id = "eth".into();
        two.symbol = "ETH".into();
        Store::update(
            parent.store.as_ref(),
            safe_cex_store::StoreUpdate {
                tickers: Some(vec![one, two]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let once = Rc::new(Cell::new(false));
        let seen = once.clone();
        let store = parent.store.clone();
        let stop = Store::subscribe(
            parent.store.as_ref(),
            Box::new(move |_, _| {
                if !seen.replace(true) {
                    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
                    patch.set_json("symbol", json!("ETH"));
                    Store::update_ticker(store.as_ref(), "btc", &patch).unwrap();
                }
                Ok(())
            }),
        );
        owner
            .handle_stream(
                "24hrTicker",
                EventValue::from_json(json!([
                    {"s":"BTC","c":"4.2","P":"1.2","v":"3.6","q":"9.1"},
                    {"s":"ETH","c":"12.3","P":"2.1","v":"5.6","q":"8.1"}
                ])),
            )
            .unwrap();
        assert_eq!(
            parent.store.tickers().unwrap()[0].last,
            12.3,
            "native symbol collision updates latest alias owner btc to 12.3"
        );
        assert_eq!(
            parent.store.tickers().unwrap()[1].last,
            0.0,
            "native symbol collision leaves displaced eth ticker unchanged"
        );
        stop();
    }
}

#[test]
fn retained_handler_frames_cross_the_production_core_registry() {
    let (owner, _, _) = fixture(PublicKind::FuturesMarket);
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let callback: MessageHandler = Rc::new(move |_| {
        seen.set(seen.get() + 1);
        Ok(())
    });
    owner.base().message_handlers.borrow_mut().insert(
        "retainedProbe".into(),
        Rc::new(RefCell::new(vec![callback])),
    );
    for marker in [r"\ud800", r"\udfff", r"\ud83e\udd80"] {
        let frame = format!(
            r#"{{"e":"retainedProbe","unused":{{"{marker}":["{marker}"]}},"overflow":1e400,"zero":-0}}"#
        );
        owner
            .on_message(&frame)
            .expect("source-valid native graph reaches core callbacks");
    }
    owner
        .on_message(r#"{"e":"retainedProbe","neighbor":true}"#)
        .unwrap();
    assert_eq!(
        calls.get(),
        4,
        "every source-valid frame reaches the real class/core registry"
    );
}

#[cfg(test)]
mod retained_handler_support {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-exchange/tests/support/retained_handlers.rs"
    ));
}
#[test]
fn shared_core_handlers_retain_the_source_graph_and_native_fields() {
    let (owner, _, _) = fixture(PublicKind::FuturesMarket);
    let cases = retained_handler_support::source_frames("binance", "publicWebsocket");
    let (handlers, calls) = retained_handler_support::pair(&cases, true);
    owner
        .base()
        .message_handlers
        .borrow_mut()
        .insert("retainedProbe".into(), handlers);
    for case in cases {
        owner.on_message(case["frame"].as_str().unwrap()).unwrap();
    }
    assert_eq!(calls.get(), 8);
}
