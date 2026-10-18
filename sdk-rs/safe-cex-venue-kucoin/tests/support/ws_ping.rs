use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{
        BaseWebSocket, WebSocketConnectionOptions, WsEvent, WsProtocol, WsRuntime, WsSocket,
    },
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::{TransportProvider, socket::TunnelHost};
use safe_cex_types::ExchangeName;
use safe_cex_venue_kucoin::ws_ping::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    sync::Arc,
    task::{Context, Poll, Waker},
};
#[path = "private_socket.rs"]
mod private_socket;

type Job = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    monotonic: Cell<f64>,
    date: Cell<f64>,
    trace: Rc<RefCell<Vec<String>>>,
    io: Rc<Io>,
    jobs: RefCell<Vec<Job>>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        self.trace.borrow_mut().push("performance".into());
        self.monotonic.get()
    }
    fn now_ms(&self) -> f64 {
        self.date.get()
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(std::future::pending())
    }
    fn spawn_local(&self, future: Job) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(future);
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
        0.5
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
        panic!("unexpected background error")
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
        panic!("unused tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        let io = self.io.clone();
        Box::pin(async move { Ok(io as Rc<dyn WsSocket>) })
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
        3000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        true
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
#[derive(Default)]
struct Io {
    ready: Cell<u8>,
    frames: RefCell<Vec<String>>,
    trace: Rc<RefCell<Vec<String>>>,
    failure: RefCell<Option<ExchangeError>>,
}
impl WsSocket for Io {
    fn ready_state(&self) -> u8 {
        self.ready.get()
    }
    fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
        Box::pin(std::future::pending())
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.trace.borrow_mut().push("send".into());
        self.frames.borrow_mut().push(message.into());
        match self.failure.borrow().clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn close(&self) {
        self.ready.set(3);
    }
    fn terminate(&self) {
        self.ready.set(3);
    }
    fn reconnect(&self) {
        panic!("heartbeat must not reconnect")
    }
    fn is_reconnecting(&self) -> bool {
        false
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
struct Protocol;
impl WsProtocol for Protocol {}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
struct Fixture {
    venue: Rc<dyn Exchange>,
    runtime: Rc<Runtime>,
    store: Rc<DefaultStore>,
    socket: Rc<BaseWebSocket>,
}
fn setup(date: f64, monotonic: f64) -> Fixture {
    let trace = Rc::new(RefCell::new(vec![]));
    let io = Rc::new(Io {
        trace: trace.clone(),
        ..Default::default()
    });
    let runtime = Rc::new(Runtime {
        trace,
        io,
        ..Default::default()
    });
    runtime.date.set(date);
    runtime.monotonic.set(monotonic);
    runtime.io.ready.set(1);
    let store = Rc::new(DefaultStore::new());
    let venue: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    });
    let socket = BaseWebSocket::new(
        1,
        WsExchangeParent::new(&venue),
        runtime.clone(),
        Rc::new(Protocol),
    );
    ready(socket.create_websocket("wss://fixture".into(), vec![], None)).unwrap();
    runtime.trace.borrow_mut().clear();
    Fixture {
        venue,
        runtime,
        store,
        socket,
    }
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/ws-ping.cjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(captures.len(), cases.len(), "ping source case cardinality");
    captures
}
#[test]
fn four_live_pings_preserve_native_clock_identity_disposal_and_failure() {
    let mut cases = vec![];
    let mut native = vec![];
    let mut frame_total = 0;
    for method in [
        "kucoin-public",
        "kucoin-private",
        "kucoinspot-public",
        "kucoinspot-private",
    ] {
        for outcome in 0..3 {
            let index = cases.len();
            let date = 1_710_000_123_000.0 + index as f64 * 19.0;
            let monotonic = 27.125 + index as f64 * 3.5;
            let fixture = setup(date, monotonic);
            let disposed = outcome == 2;
            let fail = outcome == 1;
            let mut failure = ExchangeError::new("native send failure");
            failure.name = "TypeError".into();
            failure.code = Some(safe_cex_exchange::js_runtime::value::Js::str("WS_SEND"));
            failure.context = Some(json!({"retained":17}));
            if fail {
                *fixture.runtime.io.failure.borrow_mut() = Some(failure.clone());
            }
            if disposed {
                fixture.venue.base().dispose().unwrap();
            }
            fixture.runtime.trace.borrow_mut().clear();
            let date_now = || {
                fixture.runtime.trace.borrow_mut().push("date".into());
                fixture.runtime.now_ms()
            };
            let result = match method {
                "kucoin-public" => ping_futures_public(&fixture.socket, date_now),
                "kucoin-private" => ping_futures_private(&fixture.socket, date_now),
                "kucoinspot-public" => ping_spot_public(&fixture.socket, date_now),
                "kucoinspot-private" => ping_spot_private(&fixture.socket, date_now),
                _ => unreachable!(),
            };
            let trace = fixture.runtime.trace.borrow().clone();
            let frames = fixture.runtime.io.frames.borrow().clone();
            assert_eq!(
                frames.len(),
                usize::from(!disposed),
                "KUCOIN-PING native frame count {method}/{outcome}"
            );
            assert_eq!(
                trace,
                if disposed {
                    vec![]
                } else {
                    vec!["performance", "date", "send"]
                },
                "KUCOIN-PING native effect order {method}/{outcome}"
            );
            if fail {
                assert_eq!(
                    result.as_ref().unwrap_err(),
                    &failure,
                    "KUCOIN-PING native send error metadata {method}"
                );
            } else {
                assert!(
                    result.is_ok(),
                    "KUCOIN-PING native success {method}/{outcome}"
                );
            }
            for frame in &frames {
                let decoded: Value = serde_json::from_str(frame).unwrap();
                assert_eq!(
                    decoded.as_object().unwrap().len(),
                    2,
                    "native ping field count"
                );
                assert_eq!(
                    decoded["id"].as_f64().unwrap().to_bits(),
                    date.to_bits(),
                    "KUCOIN-PING native id {method}/{outcome}"
                );
                assert_eq!(
                    decoded["type"].as_str(),
                    Some("ping"),
                    "KUCOIN-PING native type {method}"
                );
                frame_total += 1;
            }
            if !disposed {
                // Read the actual core ping timestamp through its native pong-latency effect.
                fixture.runtime.monotonic.set(monotonic + 38.0);
                fixture.socket.handle_pong_event(5000).unwrap();
                assert_eq!(
                    fixture.store.latency().to_bits(),
                    19.0_f64.to_bits(),
                    "KUCOIN-PING native monotonic timestamp {method}/{outcome}"
                );
            }
            cases.push(json!({"method":method,"date":date,"monotonic":monotonic,"disposed":disposed,"fail":fail}));
            native.push((trace, frames, result));
        }
    }
    assert_eq!(
        cases.len(),
        12,
        "independent four-role three-outcome corpus"
    );
    assert_eq!(frame_total, 8, "independent eight native ping frames");
    // Every native field/effect/error assertion above precedes source wire assertions.
    let expected = source(&cases);
    assert_eq!(expected.len(), 12, "independent ping source corpus");
    for (index, (trace, frames, result)) in native.into_iter().enumerate() {
        let ping_at = if cases[index]["disposed"] == true {
            0.0
        } else {
            cases[index]["monotonic"].as_f64().unwrap()
        };
        assert_eq!(
            expected[index]["pingAtBits"],
            format!("{:016x}", ping_at.to_bits()),
            "source native monotonic identity {index}"
        );
        assert_eq!(
            json!(trace),
            expected[index]["events"],
            "live ping effects {index}"
        );
        assert_eq!(
            json!(frames),
            expected[index]["frames"],
            "live ping wire bytes {index}"
        );
        if let Err(error) = result {
            assert_eq!(
                expected[index]["rejected"],
                json!({"message":error.message,"name":error.name,"same":true})
            );
        } else {
            assert!(expected[index].get("rejected").is_none());
        }
    }
}

#[test]
fn spot_topics_native_values_before_wire() {
    use safe_cex_venue_kucoin::spot_topics::{subscribe_spot_topic, unsubscribe_spot_topic};
    let mut cases = vec![];
    let mut native = vec![];
    let mut frame_total = 0;
    for method in ["subscribeTopic", "unsubscribeTopic"] {
        for topic in ["", "/market/candles:BTC-USDT_1min", "\"\n☃/🙂"] {
            for connected in [false, true] {
                for disposed in [false, true] {
                    for fail in [false, true] {
                        let date = 1_710_000_123_000.0 + cases.len() as f64 * 19.0;
                        let fixture = setup(date, 123.0);
                        if disposed {
                            fixture.venue.base().dispose().unwrap();
                        }
                        fixture.runtime.io.ready.set(if connected { 1 } else { 3 });
                        let mut failure = ExchangeError::new("topic send failure");
                        failure.name = "TypeError".into();
                        failure.code =
                            Some(safe_cex_exchange::js_runtime::value::Js::str("TOPIC_SEND"));
                        failure.context = Some(json!({"retained":17}));
                        failure.store_subscriber_ids = vec![7, 3];
                        if fail {
                            *fixture.runtime.io.failure.borrow_mut() = Some(failure.clone());
                        }
                        fixture.runtime.trace.borrow_mut().clear();
                        let date_now = || {
                            fixture.runtime.trace.borrow_mut().push("date".into());
                            fixture.runtime.now_ms()
                        };
                        let result = if method == "subscribeTopic" {
                            subscribe_spot_topic(&fixture.socket, topic, date_now)
                        } else {
                            unsubscribe_spot_topic(&fixture.socket, topic, date_now)
                        };
                        let active = connected && !disposed;
                        let events: Vec<&str> = if active { vec!["date", "send"] } else { vec![] };
                        assert_eq!(
                            *fixture.runtime.trace.borrow(),
                            events,
                            "KUCOIN-TOPIC native ordered effects {method}/{connected}/{disposed}/{fail}"
                        );
                        let expected_error = (active && fail).then_some(&failure);
                        assert_eq!(
                            result.as_ref().err(),
                            expected_error,
                            "KUCOIN-TOPIC native complete error"
                        );
                        let frames = fixture.runtime.io.frames.borrow().clone();
                        assert_eq!(
                            frames.len(),
                            usize::from(active),
                            "KUCOIN-TOPIC native frame count"
                        );
                        if active {
                            frame_total += 1;
                            let decoded: Value = serde_json::from_str(&frames[0]).unwrap();
                            assert_eq!(
                                decoded["id"].as_f64(),
                                Some(date),
                                "KUCOIN-TOPIC native date"
                            );
                            assert_eq!(
                                decoded["type"],
                                if method == "subscribeTopic" {
                                    "subscribe"
                                } else {
                                    "unsubscribe"
                                },
                                "KUCOIN-TOPIC native operation {method}"
                            );
                            assert_eq!(
                                decoded["topic"], topic,
                                "KUCOIN-TOPIC native topic identity"
                            );
                            assert_eq!(
                                decoded["privateChannel"].as_bool(),
                                Some(false),
                                "KUCOIN-TOPIC native public flag"
                            );
                            assert_eq!(
                                decoded["response"].as_bool(),
                                Some(true),
                                "KUCOIN-TOPIC native response flag"
                            );
                            assert_eq!(
                                decoded.as_object().unwrap().len(),
                                5,
                                "KUCOIN-TOPIC native five fields"
                            );
                        }
                        assert_eq!(
                            fixture.runtime.io.ready.get(),
                            if connected { 1 } else { 3 },
                            "KUCOIN-TOPIC native readiness unchanged"
                        );
                        cases.push(json!({"method":method,"topic":topic,"date":date,"connected":connected,"disposed":disposed,"fail":fail}));
                        native.push(json!({"events":*fixture.runtime.trace.borrow(),"frames":frames,
                            "error":result.err().map(|e| json!({"name":e.name,"message":e.message,"same":true}))}));
                        fixture.runtime.jobs.borrow_mut().clear();
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 48, "declared spot topic profile floor");
    assert_eq!(
        frame_total, 12,
        "two methods times three topics times two send outcomes"
    );
    assert!(
        serde_json::from_str::<String>(r#""\ud800""#).is_err(),
        "KUCOIN-TOPIC native required string refuses lone D800"
    );
    assert_eq!(
        serde_json::from_str::<String>(r#""\\ud800""#).unwrap(),
        "\\ud800",
        "KUCOIN-TOPIC native literal ASCII positive"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-topics.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&cases).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(captures.len(), 48, "source spot topic profile floor");
    assert_eq!(
        native, captures,
        "live topic bytes after all decoded native assertions"
    );
    let witness = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-topics.cjs"
        ))
        .arg("--surrogate-witness")
        .output()
        .unwrap();
    assert!(
        witness.status.success(),
        "{}",
        String::from_utf8_lossy(&witness.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&witness.stdout).unwrap();
    assert_eq!(rows.len(), 2, "two independent topic unit observations");
    for (row, method) in rows.iter().zip(["subscribeTopic", "unsubscribeTopic"]) {
        assert_eq!(row["method"], method, "source topic witness method");
        assert_eq!(
            row["units"],
            json!([0xd800]),
            "source transmitted materialized topic units"
        );
        assert!(
            row["frames"][0].as_str().unwrap().contains("\\ud800"),
            "source transmission retains lone D800 escape"
        );
    }
}
