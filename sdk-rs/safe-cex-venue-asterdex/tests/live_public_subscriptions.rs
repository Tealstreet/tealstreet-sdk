#[path = "support/subscription_runtime.rs"]
mod host;
use host::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::ExchangeError,
};
use safe_cex_types::{ExchangeName, OrderBook};
use safe_cex_venue_asterdex::{
    public_subscriptions::{
        BookCleanup, PublicSubscriptionIo, PublicSubscriptionState, PublicSubscriptions,
    },
    ws_helpers::{self, PublicKlineCandle},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

#[derive(Debug, Clone, Deserialize, PartialEq)]
struct Frame {
    method: String,
    params: Vec<String>,
    id: f64,
}
#[derive(Default)]
struct Io {
    connected: Cell<bool>,
    disposed: Cell<bool>,
    reconnecting: Cell<bool>,
    fail: Cell<usize>,
    pings: Cell<usize>,
    emits: Cell<usize>,
    disposes: Cell<usize>,
    attempts: RefCell<Vec<Frame>>,
    wire: RefCell<Vec<String>>,
    cleanups: RefCell<Vec<String>>,
    events: RefCell<Vec<&'static str>>,
}
impl PublicSubscriptionIo for Io {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn enable_reconnecting(&self) -> bool {
        self.reconnecting.get()
    }
    fn send(&self, message: &str) -> Result<(), ExchangeError> {
        self.attempts
            .borrow_mut()
            .push(serde_json::from_str(message).expect("decoded native subscription frame"));
        self.wire.borrow_mut().push(message.into());
        self.events.borrow_mut().push("send");
        if self.fail.get() > 0 {
            self.fail.set(self.fail.get() - 1);
            Err(ExchangeError::new("send-down"))
        } else {
            Ok(())
        }
    }
    fn ping(&self) -> Result<(), ExchangeError> {
        self.pings.set(self.pings.get() + 1);
        self.events.borrow_mut().push("ping");
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), ExchangeError> {
        self.emits.set(self.emits.get() + 1);
        self.events.borrow_mut().push("emit");
        Ok(())
    }
    fn cleanup_message_handler_stats(&self, stream: &str) -> Result<(), ExchangeError> {
        self.cleanups.borrow_mut().push(stream.into());
        Ok(())
    }
    fn dispose(&self) {
        self.disposes.set(self.disposes.get() + 1);
        self.connected.set(false);
    }
}
#[derive(Debug, Clone)]
enum Callback {
    Book(String, OrderBook),
    Candle(String, PublicKlineCandle),
}
struct Fixture {
    runtime: Rc<Runtime>,
    base: Rc<BaseExchange>,
    io: Rc<Io>,
    subscriptions: Rc<PublicSubscriptions>,
    callbacks: Rc<RefCell<Vec<Callback>>>,
}
fn fixture(case: &Value) -> Fixture {
    let runtime = Rc::new(Runtime::default());
    let io = Rc::new(Io::default());
    io.connected.set(case["connected"].as_bool().unwrap());
    io.reconnecting
        .set(case["reconnecting"].as_bool().unwrap_or(false));
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
        Rc::new(DefaultStore::new()),
        runtime.clone(),
        ExchangeSettings::default(),
    ));
    let subscriptions = PublicSubscriptions::new(base.clone(), io.clone());
    Fixture {
        runtime,
        base,
        io,
        subscriptions,
        callbacks: Rc::new(RefCell::new(Vec::new())),
    }
}
struct Snapshot {
    state: PublicSubscriptionState,
    deadlines: Vec<f64>,
    attempts: usize,
    callbacks: usize,
    pings: usize,
    emits: usize,
    disposes: usize,
}
struct Trace {
    snapshots: Vec<Snapshot>,
    attempts: Vec<Frame>,
    wire: Vec<String>,
    callbacks: Vec<Callback>,
    errors: Vec<String>,
    cleanups: Vec<String>,
    events: Vec<&'static str>,
}
fn run(case: &Value) -> Trace {
    let f = fixture(case);
    let mut handles: HashMap<String, BookCleanup> = HashMap::new();
    let mut snapshots = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        let mut pump = true;
        match step["op"].as_str().unwrap() {
            "book" => {
                let name = step["callback"].as_str().unwrap().to_owned();
                let callbacks = f.callbacks.clone();
                let cb = Rc::new(move |mut book: OrderBook| {
                    callbacks
                        .borrow_mut()
                        .push(Callback::Book(name.clone(), book.clone()));
                    if name == "A" {
                        book.bids[0].amount = 999.0;
                    }
                    Ok(())
                });
                let cleanup = f
                    .subscriptions
                    .listen_order_book(step["symbol"].as_str().unwrap(), cb)
                    .unwrap();
                handles.insert(step["handle"].as_str().unwrap().into(), cleanup);
            }
            "remove" => handles[step["handle"].as_str().unwrap()]().unwrap(),
            "clear-books" => f.subscriptions.clear_order_book_subscription_retries(),
            "kline" => {
                let name = step["callback"].as_str().unwrap().to_owned();
                let callbacks = f.callbacks.clone();
                f.subscriptions
                    .subscribe_to_kline(
                        step["stream"].as_str().unwrap(),
                        Rc::new(move |candle| {
                            callbacks
                                .borrow_mut()
                                .push(Callback::Candle(name.clone(), candle));
                            Ok(())
                        }),
                    )
                    .unwrap();
            }
            "remove-kline" => f
                .subscriptions
                .unsubscribe_from_kline(step["stream"].as_str().unwrap())
                .unwrap(),
            "book-data" => {
                let data = &step["data"];
                let book = ws_helpers::public_depth_snapshot(data).unwrap();
                f.subscriptions
                    .deliver_book(
                        &format!(
                            "{}@depth20@100ms",
                            data["s"].as_str().unwrap().to_lowercase()
                        ),
                        &book,
                    )
                    .unwrap();
            }
            "kline-data" => {
                let keys = f.subscriptions.state().klines;
                let streams: Vec<_> = keys.iter().map(String::as_str).collect();
                if let Some(update) =
                    ws_helpers::public_kline_update(&step["data"], &streams).unwrap()
                {
                    f.subscriptions
                        .deliver_kline(&update.stream, update.candle)
                        .unwrap();
                }
            }
            "connected" => f.io.connected.set(step["value"].as_bool().unwrap()),
            "disposed" => f.io.disposed.set(step["value"].as_bool().unwrap()),
            "fail-send" => f.io.fail.set(step["count"].as_u64().unwrap() as usize),
            "open" => f.subscriptions.on_open().unwrap(),
            "open-unflushed" => {
                f.subscriptions.on_open().unwrap();
                pump = false;
            }
            "dispose" => f.subscriptions.dispose(),
            "tick" => f.runtime.mono.set(step["value"].as_f64().unwrap()),
            "snapshot" => snapshots.push(Snapshot {
                state: f.subscriptions.state(),
                deadlines: f.runtime.deadlines(),
                attempts: f.io.attempts.borrow().len(),
                callbacks: f.callbacks.borrow().len(),
                pings: f.io.pings.get(),
                emits: f.io.emits.get(),
                disposes: f.io.disposes.get(),
            }),
            _ => panic!("unknown fixture step"),
        }
        if pump {
            f.runtime.pump();
        }
    }
    let trace = Trace {
        snapshots,
        attempts: f.io.attempts.borrow().clone(),
        wire: f.io.wire.borrow().clone(),
        callbacks: f.callbacks.borrow().clone(),
        errors: f.runtime.errors.borrow().clone(),
        cleanups: f.io.cleanups.borrow().clone(),
        events: f.io.events.borrow().clone(),
    };
    // Cleanup is outside observations; only this controller's lifecycle runs.
    f.subscriptions.dispose();
    assert_eq!(
        f.runtime.timers(),
        0,
        "native controller cleanup cancels its timers"
    );
    f.base.dispose().unwrap();
    trace
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedSnapshot {
    topics: Vec<(String, Vec<String>)>,
    books: Vec<(String, Vec<f64>, bool)>,
    klines: Vec<String>,
    pending: usize,
    ready: bool,
    subscription_id: f64,
    listener_id: f64,
    deadlines: Vec<f64>,
    attempts: usize,
    callbacks: usize,
    pings: usize,
    emits: usize,
    disposes: usize,
}
fn native_assertions(
    name: &str,
    trace: &Trace,
    source: &Value,
    snapshots: usize,
    attempts: usize,
    callbacks: usize,
) {
    assert_eq!(
        trace.snapshots.len(),
        snapshots,
        "{name} native snapshot floor"
    );
    assert_eq!(
        source["snapshots"].as_array().unwrap().len(),
        snapshots,
        "{name} source snapshot floor"
    );
    assert_eq!(
        trace.attempts.len(),
        attempts,
        "{name} native attempt floor"
    );
    assert_eq!(
        source["attempts"].as_array().unwrap().len(),
        attempts,
        "{name} source attempt floor"
    );
    assert_eq!(
        trace.callbacks.len(),
        callbacks,
        "{name} native callback floor"
    );
    assert_eq!(
        source["callbacks"].as_array().unwrap().len(),
        callbacks,
        "{name} source callback floor"
    );
    for (actual, expected) in trace
        .snapshots
        .iter()
        .zip(source["snapshots"].as_array().unwrap())
    {
        let expected: ExpectedSnapshot = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(
            actual.state.topics, expected.topics,
            "{name} native ordered topics"
        );
        assert_eq!(
            actual.state.books, expected.books,
            "{name} native book listener ids/retry slots"
        );
        assert_eq!(
            actual.state.klines, expected.klines,
            "{name} native kline registrations"
        );
        assert_eq!(
            actual.state.pending.len(),
            expected.pending,
            "{name} native pending"
        );
        if expected.pending > 0 {
            assert_eq!(
                actual.state.pending,
                vec!["btcusdt@kline_1m"],
                "{name} native captured pending stream"
            );
        }
        assert_eq!(actual.state.ready, expected.ready, "{name} native ready");
        assert_eq!(
            actual.state.subscription_id, expected.subscription_id,
            "{name} native next subscription id"
        );
        assert_eq!(
            actual.state.listener_id, expected.listener_id,
            "{name} native next listener id"
        );
        assert_eq!(
            actual.deadlines, expected.deadlines,
            "{name} native deadlines"
        );
        for (key, native, reference) in [
            ("attempts", actual.attempts, expected.attempts),
            ("callbacks", actual.callbacks, expected.callbacks),
            ("pings", actual.pings, expected.pings),
            ("emits", actual.emits, expected.emits),
            ("disposes", actual.disposes, expected.disposes),
        ] {
            assert_eq!(native, reference, "{name} native {key}");
        }
    }
    for (actual, expected) in trace
        .attempts
        .iter()
        .zip(source["attempts"].as_array().unwrap())
    {
        let expected: Frame = serde_json::from_value(expected.clone()).unwrap();
        assert_eq!(actual.method, expected.method, "{name} native method");
        assert_eq!(actual.params, expected.params, "{name} native params");
        assert_eq!(actual.id, expected.id, "{name} native emitted id");
    }
    for (actual, expected) in trace
        .callbacks
        .iter()
        .zip(source["callbacks"].as_array().unwrap())
    {
        match actual {
            Callback::Book(label, book) => {
                assert_eq!(expected["kind"], "book");
                assert_eq!(label, expected["name"].as_str().unwrap());
                let reference: OrderBook =
                    serde_json::from_value(expected["book"].clone()).unwrap();
                assert_eq!(book.bids.len(), 1);
                assert_eq!(book.asks.len(), 1);
                assert_eq!(reference.bids.len(), 1);
                assert_eq!(reference.asks.len(), 1);
                for (native, reference) in book
                    .bids
                    .iter()
                    .chain(&book.asks)
                    .zip(reference.bids.iter().chain(&reference.asks))
                {
                    assert_eq!(native.price, reference.price, "{name} native book price");
                    assert_eq!(native.amount, reference.amount, "{name} native book amount");
                    assert_eq!(native.total, reference.total, "{name} native book total");
                }
            }
            Callback::Candle(label, candle) => {
                assert_eq!(expected["kind"], "candle");
                assert_eq!(label, expected["name"].as_str().unwrap());
                let reference = &expected["candle"];
                assert_eq!(
                    candle.timestamp.as_ref(),
                    reference.get("timestamp"),
                    "{name} native candle timestamp"
                );
                for (key, value) in [
                    ("open", candle.open),
                    ("high", candle.high),
                    ("low", candle.low),
                    ("close", candle.close),
                    ("volume", candle.volume),
                ] {
                    assert_eq!(
                        value,
                        reference[key].as_f64().unwrap(),
                        "{name} native candle {key}"
                    );
                }
            }
        }
    }
    let errors: Vec<_> = source["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(trace.errors, errors, "{name} native rejection channel");
    let cleanups: Vec<_> = source["cleanups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(trace.cleanups, cleanups, "{name} native cleanup calls");
    let events: Vec<_> = source["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(trace.events, events, "{name} native call order");
}
fn source() -> (Vec<Value>, Vec<Value>) {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-subscriptions-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    (
        serde_json::from_str(include_str!("public-subscriptions-cases.json")).unwrap(),
        serde_json::from_slice(&result.stdout).unwrap(),
    )
}
fn check_family(family: &str) {
    let (cases, sources) = source();
    assert_eq!(cases.len(), 9);
    assert_eq!(sources.len(), 9);
    const NAMES: [&str; 9] = [
        "book-delay-multi-listener",
        "book-failure-removed-generation",
        "book-retry-ceiling",
        "kline-replacement-and-orphan-unsubscribe",
        "kline-initialization-queue",
        "reopen-insertion-order",
        "send-errors-and-disposal",
        "disposal-orphan-wait-finding",
        "disposal-open-completion-finding",
    ];
    const SNAPSHOTS: [usize; 9] = [7, 5, 5, 3, 6, 4, 4, 2, 1];
    const ATTEMPTS: [usize; 9] = [2, 3, 2, 5, 2, 4, 5, 0, 1];
    const CALLBACKS: [usize; 9] = [3, 1, 0, 1, 1, 0, 0, 0, 0];
    let mut checked = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(case["name"], NAMES[i]);
        assert_eq!(sources[i]["name"], NAMES[i]);
        if case["family"] != family {
            continue;
        }
        let trace = run(case);
        native_assertions(
            NAMES[i],
            &trace,
            &sources[i],
            SNAPSHOTS[i],
            ATTEMPTS[i],
            CALLBACKS[i],
        );
        checked.push((i, trace));
    }
    assert_eq!(
        checked.len(),
        match family {
            "book" => 3,
            "kline" => 2,
            "lifecycle" => 2,
            _ => panic!("family"),
        }
    );
    // All native assertions for every case in this family run before any wire assertion.
    for (i, trace) in checked {
        let wire: Vec<_> = sources[i]["wire"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(trace.wire, wire, "{} wire bytes", NAMES[i]);
    }
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn public_book_subscriptions_match_source() {
    check_family("book");
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn public_kline_subscriptions_match_source() {
    check_family("kline");
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn public_reopen_dispose_match_source() {
    check_family("lifecycle");
}
#[test]
#[ignore = "requires live TypeScript compiler"]
fn public_disposal_differences_are_observed() {
    let (cases, sources) = source();
    assert_eq!(cases.len(), 9);
    assert_eq!(sources.len(), 9);
    assert_eq!(cases[7]["name"], "disposal-orphan-wait-finding");
    assert_eq!(cases[8]["name"], "disposal-open-completion-finding");
    let orphan = run(&cases[7]);
    assert_eq!(orphan.snapshots.len(), 2);
    assert_eq!(orphan.attempts.len(), 0);
    assert_eq!(orphan.callbacks.len(), 0);
    for snapshot in &orphan.snapshots {
        assert!(
            snapshot.deadlines.is_empty(),
            "native disposal cancels orphan kline wait"
        );
        assert!(!snapshot.state.ready);
        assert!(snapshot.state.klines.is_empty());
        assert_eq!(snapshot.disposes, 1);
    }
    assert_eq!(sources[7]["snapshots"].as_array().unwrap().len(), 2);
    for snapshot in sources[7]["snapshots"].as_array().unwrap() {
        assert_eq!(
            snapshot["deadlines"].as_array().unwrap().len(),
            1,
            "source disposal leaves orphan kline wait live"
        );
    }
    let opening = run(&cases[8]);
    assert_eq!(opening.snapshots.len(), 1);
    assert_eq!(opening.attempts.len(), 1);
    assert_eq!(opening.callbacks.len(), 0);
    let snapshot = &opening.snapshots[0];
    assert!(
        !snapshot.state.ready,
        "native disposal cancels pending open completion"
    );
    assert_eq!(snapshot.emits, 0);
    assert_eq!(snapshot.disposes, 1);
    assert!(snapshot.state.topics.is_empty());
    assert_eq!(sources[8]["snapshots"].as_array().unwrap().len(), 1);
    assert_eq!(
        sources[8]["snapshots"][0]["ready"], true,
        "source finally reinitializes after disposal"
    );
    assert_eq!(sources[8]["snapshots"][0]["emits"], 1);
    // No wire equality for intentional lifecycle differences.
}
