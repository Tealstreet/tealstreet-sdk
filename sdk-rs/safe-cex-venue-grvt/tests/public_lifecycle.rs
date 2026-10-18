#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Market;
use safe_cex_venue_grvt::{
    public_lifecycle::*,
    public_subscriptions::*,
    types::{GrvtJsonRpcVersion, GrvtSubscriptionMethod},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::OnceLock,
};
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let result = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-lifecycle-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "LIFECYCLE source oracle {}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    })
}
fn scalar(actual: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(actual.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            format!("{:016x}", actual.to_bits()),
            source["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn error(actual: Option<&ExchangeError>, source: &Value, label: &str) {
    if let Some(actual) = actual {
        assert_eq!(
            actual.name,
            source["name"].as_str().unwrap(),
            "{label} native error name"
        );
        assert_eq!(
            actual.message,
            source["message"].as_str().unwrap(),
            "{label} native error message"
        );
    } else {
        assert!(source.is_null(), "{label} unexpected success");
    }
}
fn market(index: usize) -> Market {
    serde_json::from_value(json!({"id":format!("m{index}"),"symbol":format!("SYM{index}"),"base":"BASE","quote":"QUOTE","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":9},"leverage":{"min":1,"max":9}}})).unwrap()
}
fn roster(count: usize) -> Rc<AddedMarkets> {
    Rc::new(RefCell::new(
        (0..count)
            .map(|index| Rc::new(RefCell::new(market(index))))
            .collect(),
    ))
}
fn seed(id: f64) -> SubscribeRequest {
    SubscribeRequest {
        jsonrpc: GrvtJsonRpcVersion::V2,
        id,
        method: GrvtSubscriptionMethod::Subscribe,
        params: SubscribeParams {
            stream: "v1.ticker.d".into(),
            selectors: [format!("SEED{}@500", id as u64)],
        },
    }
}
fn request(actual: &RpcRequest, source: &Value, label: &str) {
    let RpcRequest::Subscribe(actual) = actual else {
        panic!("{label} native request variant")
    };
    assert_eq!(
        actual.jsonrpc,
        GrvtJsonRpcVersion::V2,
        "{label} native version"
    );
    assert_eq!(source["jsonrpc"], "2.0", "{label} source version");
    assert_eq!(
        actual.method,
        GrvtSubscriptionMethod::Subscribe,
        "{label} native method"
    );
    assert_eq!(source["method"], "subscribe", "{label} source method");
    scalar(actual.id, &source["id"], &format!("{label} native id"));
    assert_eq!(
        actual.params.stream,
        source["params"]["stream"].as_str().unwrap(),
        "{label} native stream"
    );
    assert_eq!(
        actual.params.selectors.len(),
        1,
        "{label} native selector count"
    );
    assert_eq!(
        source["params"]["selectors"].as_array().unwrap().len(),
        1,
        "{label} independent source selector count"
    );
    assert_eq!(
        actual.params.selectors[0],
        source["params"]["selectors"][0].as_str().unwrap(),
        "{label} native selector"
    );
}
fn snapshot(
    state: &PublicSubscriptions<u64>,
    source: &Value,
    counts: (usize, usize, usize),
    disposed: bool,
    label: &str,
) {
    scalar(
        state.request_id.get(),
        &source["requestId"],
        &format!("{label} native counter"),
    );
    scalar(
        state.ticker_batch_generation.get(),
        &source["generation"],
        &format!("{label} native generation"),
    );
    let subs = state.subscriptions.borrow();
    assert_eq!(subs.len(), counts.0, "{label} native subscription count");
    assert_eq!(
        source["subscriptions"].as_array().unwrap().len(),
        counts.0,
        "{label} independent source subscription count"
    );
    for ((key, sub), entry) in subs.iter().zip(source["subscriptions"].as_array().unwrap()) {
        scalar(key.value(), &entry[0], &format!("{label} native Map key"));
        request(&RpcRequest::Subscribe(sub.clone()), &entry[1], label);
    }
    drop(subs);
    let tickers = state.ticker_subscriptions.borrow();
    assert_eq!(tickers.len(), counts.1, "{label} native ticker count");
    assert_eq!(
        source["tickers"].as_array().unwrap().len(),
        counts.1,
        "{label} independent source ticker count"
    );
    for ((instrument, id), entry) in tickers.iter().zip(source["tickers"].as_array().unwrap()) {
        assert_eq!(
            instrument,
            entry[0].as_str().unwrap(),
            "{label} native instrument"
        );
        scalar(*id, &entry[1], &format!("{label} native ticker id"));
    }
    let timers = state.ticker_batch_timers.values();
    assert_eq!(timers.len(), counts.2, "{label} native timer count");
    assert_eq!(
        source["timers"].as_array().unwrap().len(),
        counts.2,
        "{label} independent source timer count"
    );
    for (timer, entry) in timers.iter().zip(source["timers"].as_array().unwrap()) {
        scalar(
            *timer as f64,
            entry,
            &format!("{label} native timer handle"),
        );
    }
    assert_eq!(
        disposed,
        source["disposed"].as_bool().unwrap(),
        "{label} native disposed"
    );
}
fn native_event_value(actual: &Value, source: &Value, label: &str) {
    match (actual, source) {
        (Value::Number(actual), Value::Number(source)) => assert_eq!(
            actual.as_f64().unwrap().to_bits(),
            source.as_f64().unwrap().to_bits(),
            "{label} native numeric operation value"
        ),
        (Value::Array(actual), Value::Array(source)) => {
            assert_eq!(
                actual.len(),
                source.len(),
                "{label} operation argument width"
            );
            for (actual, source) in actual.iter().zip(source) {
                native_event_value(actual, source, label);
            }
        }
        _ => assert_eq!(actual, source, "{label} native operation value"),
    }
}
fn events(actual: &[Value], source: &Value, count: usize, label: &str) {
    assert_eq!(
        actual.len(),
        count,
        "{label} independent native event count"
    );
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "{label} independent source event count"
    );
    for (actual, source) in actual.iter().zip(source.as_array().unwrap()) {
        native_event_value(actual, source, label);
    }
}
fn frames(actual: &[RpcRequest], source: &Value, count: usize, label: &str) {
    assert_eq!(actual.len(), count, "{label} independent request count");
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "{label} independent source request count"
    );
    for (actual, source) in actual.iter().zip(source.as_array().unwrap()) {
        request(actual, source, label);
    }
}
fn wire_rows(actual: Vec<String>, source: &Value) -> (Vec<String>, Vec<String>) {
    (
        actual,
        source
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect(),
    )
}
#[test]
fn connect_native() {
    let names = [
        "production",
        "testnet",
        "disposed",
        "create-error",
        "open-error",
        "message-error",
        "swap-socket",
        "unset-socket",
        "replace-handler",
    ];
    let counts = [4, 4, 0, 1, 2, 3, 4, 2, 4];
    let rows = corpus()["connectRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "CONNECT independent floor");
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "CONNECT independent order");
        let socket = RefCell::new(Some(Rc::new("old".to_owned())));
        let events_log = RefCell::new(Vec::new());
        let handles = RefCell::new([Rc::new(11u64), Rc::new(22), Rc::new(33)]);
        let visited = Cell::new(0);
        let actual = connect_and_subscribe(
            || name == "disposed",
            || name == "testnet",
            &socket,
            |event| {
                handles.borrow()[match event {
                    SocketEvent::Open => 0,
                    SocketEvent::Message => 1,
                    SocketEvent::Close => 2,
                }]
                .clone()
            },
            |url| {
                assert_eq!(
                    url,
                    if name == "testnet" {
                        "wss://market-data.testnet.grvt.io/ws/full"
                    } else {
                        "wss://market-data.grvt.io/ws/full"
                    },
                    "CONNECT native URL"
                );
                events_log.borrow_mut().push(json!(["create", url]));
                if name == "create-error" {
                    Err(ExchangeError::new("create-fail"))
                } else {
                    Ok(Rc::new("created".into()))
                }
            },
            |receiver, event, handler| {
                let step = visited.get();
                visited.set(step + 1);
                let expected = [SocketEvent::Open, SocketEvent::Message, SocketEvent::Close][step];
                assert_eq!(event, expected, "CONNECT native listener order");
                assert!(
                    std::ptr::eq(handler, handles.borrow()[step].as_ref()),
                    "CONNECT native handler identity"
                );
                assert_eq!(
                    receiver,
                    if name == "swap-socket" && step > 0 {
                        "replacement"
                    } else {
                        "created"
                    },
                    "CONNECT native live socket receiver"
                );
                let event_text = match event {
                    SocketEvent::Open => "open",
                    SocketEvent::Message => "message",
                    SocketEvent::Close => "close",
                };
                events_log
                    .borrow_mut()
                    .push(json!(["listen", receiver, event_text, true]));
                if name == format!("{event_text}-error") {
                    return Err(ExchangeError::new(format!("{event_text}-fail")));
                }
                if event == SocketEvent::Open && name == "swap-socket" {
                    *socket.borrow_mut() = Some(Rc::new("replacement".into()));
                }
                if event == SocketEvent::Open && name == "unset-socket" {
                    *socket.borrow_mut() = None;
                }
                if event == SocketEvent::Open && name == "replace-handler" {
                    handles.borrow_mut()[1] = Rc::new(44);
                }
                Ok(())
            },
        );
        error(actual.as_ref().err(), &row["error"], "CONNECT");
        assert_eq!(
            socket.borrow().as_ref().map(|v| v.as_str()),
            row["socket"].as_str(),
            "CONNECT native assigned socket"
        );
        events(
            &events_log.borrow(),
            &row["events"],
            counts[index],
            "CONNECT",
        );
    }
}
#[test]
fn open_native() {
    let names = [
        "ok",
        "disposed",
        "track-error",
        "replay-error",
        "all-error",
        "off-error",
        "on-error",
        "dispose-during-track",
        "replace-handler",
    ];
    let counts = [5, 0, 1, 2, 3, 4, 5, 5, 5];
    let rows = corpus()["openRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "OPEN independent floor");
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "OPEN independent order");
        let events_log = RefCell::new(Vec::new());
        let disposed = Cell::new(name == "disposed");
        let handler = RefCell::new(Rc::new(37u64));
        let step = |which: &str| {
            events_log.borrow_mut().push(json!([which]));
            if which == "track" && name == "dispose-during-track" {
                disposed.set(true);
            }
            if name == format!("{which}-error") {
                Err(ExchangeError::new(format!("{which}-fail")))
            } else {
                Ok(())
            }
        };
        let event = |which: &str, event: &str, actual_handler: &u64| {
            assert_eq!(event, "markets:changed", "OPEN native emitter event");
            assert!(
                std::ptr::eq(actual_handler, handler.borrow().as_ref()),
                "OPEN native emitter handler"
            );
            events_log.borrow_mut().push(json!([which, event, true]));
            if which == "off" && name == "replace-handler" {
                *handler.borrow_mut() = Rc::new(47);
            }
            if name == format!("{which}-error") {
                Err(ExchangeError::new(format!("{which}-fail")))
            } else {
                Ok(())
            }
        };
        let actual = on_open(
            || handler.borrow().clone(),
            || disposed.get(),
            || step("track"),
            || step("replay"),
            || step("all"),
            |evt, h| event("off", evt, h),
            |evt, h| event("on", evt, h),
        );
        error(actual.as_ref().err(), &row["error"], "OPEN");
        assert_eq!(
            disposed.get(),
            row["disposed"].as_bool().unwrap(),
            "OPEN native disposal during track"
        );
        events(&events_log.borrow(), &row["events"], counts[index], "OPEN");
    }
}
#[test]
fn markets_changed_native() {
    let names = [
        "ok",
        "empty",
        "no-socket",
        "disposed",
        "disconnected",
        "send-error",
        "emit-error",
        "append",
        "remove-first",
        "live-symbol",
        "dispose-after-first",
    ];
    let sent = [2, 0, 0, 0, 0, 2, 1, 3, 1, 2, 1];
    let cached = [2, 0, 0, 0, 0, 1, 0, 3, 1, 1, 1];
    let event_counts = [2, 0, 0, 0, 0, 3, 2, 3, 1, 3, 1];
    let rows = corpus()["marketRows"].as_array().unwrap();
    assert_eq!(rows.len(), 11, "MARKETS independent floor");
    let mut wires = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "MARKETS independent order");
        let state = PublicSubscriptions::<u64>::default();
        let added = roster(if name == "empty" { 0 } else { 2 });
        let disposed = Cell::new(name == "disposed");
        let events_log = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wire = RefCell::new(Vec::new());
        let actual = on_markets_changed(
            &state,
            &added,
            || name != "no-socket",
            || disposed.get(),
            || name != "disconnected",
            |req, text| {
                requests.borrow_mut().push(req.clone());
                wire.borrow_mut().push(text.to_owned());
                events_log.borrow_mut().push(json!(["send"]));
                let first = wire.borrow().len() == 1;
                if name == "append" && first {
                    added.borrow_mut().push(Rc::new(RefCell::new(market(2))));
                }
                if name == "remove-first" && first {
                    added.borrow_mut().remove(0);
                }
                if name == "dispose-after-first" {
                    disposed.set(true);
                }
                if name == "live-symbol" && first {
                    added.borrow()[0].borrow_mut().symbol = "CHANGED".into();
                }
                if matches!(name, "send-error" | "emit-error" | "live-symbol") && first {
                    Err(ExchangeError::new("send-fail"))
                } else {
                    Ok(())
                }
            },
            |message| {
                events_log.borrow_mut().push(json!(["emit", message]));
                if name == "emit-error" {
                    Err(ExchangeError::new("emit-fail"))
                } else {
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "MARKETS");
        frames(&requests.borrow(), &row["requests"], sent[index], "MARKETS");
        snapshot(
            &state,
            &row["state"],
            (sent[index], cached[index], 0),
            disposed.get(),
            "MARKETS",
        );
        events(
            &events_log.borrow(),
            &row["events"],
            event_counts[index],
            "MARKETS",
        );
        wires.push(wire_rows(wire.into_inner(), &row["wires"]));
    }
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "MARKETS wire after native assertions");
    }
}
fn replay_fixture(
    state: &PublicSubscriptions<u64>,
    name: &str,
    requests: &RefCell<Vec<RpcRequest>>,
    wires: &RefCell<Vec<String>>,
    events_log: &RefCell<Vec<Value>>,
    mutated: &Cell<bool>,
) -> Result<(), ExchangeError> {
    resubscribe(state, |request, text| {
        requests.borrow_mut().push(request.clone());
        wires.borrow_mut().push(text.to_owned());
        events_log.borrow_mut().push(json!(["send"]));
        if name == "send-error" {
            return Err(ExchangeError::new("send-fail"));
        }
        if !mutated.replace(true) {
            match name {
                "append" => {
                    state
                        .subscriptions
                        .borrow_mut()
                        .insert(NumberKey::new(3.0), seed(3.0));
                }
                "delete-next" => {
                    state
                        .subscriptions
                        .borrow_mut()
                        .shift_remove(&NumberKey::new(2.0));
                }
                "clear-add" => {
                    let mut subs = state.subscriptions.borrow_mut();
                    subs.clear();
                    subs.insert(NumberKey::new(3.0), seed(3.0));
                }
                "delete-reinsert" => {
                    let mut subs = state.subscriptions.borrow_mut();
                    subs.shift_remove(&NumberKey::new(1.0));
                    subs.insert(NumberKey::new(1.0), seed(1.0));
                }
                "replace-next" => {
                    let mut replacement = seed(2.0);
                    replacement.params.stream = "v1.ticker.s".into();
                    replacement.params.selectors = ["REPLACED@500".into()];
                    state
                        .subscriptions
                        .borrow_mut()
                        .insert(NumberKey::new(2.0), replacement);
                }
                "nested-clear-add" => {
                    replay_fixture(state, name, requests, wires, events_log, mutated)?;
                    let mut subs = state.subscriptions.borrow_mut();
                    subs.clear();
                    subs.insert(NumberKey::new(3.0), seed(3.0));
                }
                _ => {}
            }
        }
        Ok(())
    })
}
#[test]
fn replay_native() {
    let names = [
        "ok",
        "empty",
        "send-error",
        "append",
        "delete-next",
        "clear-add",
        "delete-reinsert",
        "replace-next",
        "nested-clear-add",
    ];
    let sent = [2, 0, 1, 3, 1, 2, 3, 2, 4];
    let remaining = [2, 0, 2, 3, 1, 1, 2, 2, 1];
    let rows = corpus()["replayRows"].as_array().unwrap();
    assert_eq!(rows.len(), 9, "REPLAY independent floor");
    let mut wire_rows_out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "REPLAY independent order");
        let state = PublicSubscriptions::<u64>::default();
        if name != "empty" {
            state
                .subscriptions
                .borrow_mut()
                .insert(NumberKey::new(1.0), seed(1.0));
            state
                .subscriptions
                .borrow_mut()
                .insert(NumberKey::new(2.0), seed(2.0));
        }
        let requests = RefCell::new(Vec::new());
        let wire = RefCell::new(Vec::new());
        let events_log = RefCell::new(Vec::new());
        let mutated = Cell::new(false);
        let actual = replay_fixture(&state, name, &requests, &wire, &events_log, &mutated);
        error(actual.as_ref().err(), &row["error"], "REPLAY");
        frames(&requests.borrow(), &row["requests"], sent[index], "REPLAY");
        snapshot(
            &state,
            &row["state"],
            (remaining[index], 0, 0),
            false,
            "REPLAY",
        );
        events(&events_log.borrow(), &row["events"], sent[index], "REPLAY");
        wire_rows_out.push(wire_rows(wire.into_inner(), &row["wires"]));
    }
    for (actual, expected) in wire_rows_out {
        assert_eq!(actual, expected, "REPLAY wire after native assertions");
    }
}
struct BatchHost {
    name: &'static str,
    disposed: Cell<bool>,
    connected: Cell<bool>,
    markets: Rc<AddedMarkets>,
    events: RefCell<Vec<Value>>,
    requests: RefCell<Vec<RpcRequest>>,
    wires: RefCell<Vec<String>>,
    callbacks: RefCell<Vec<(u64, f64, TimerCallback)>>,
}
impl TickerBatchHost<u64> for BatchHost {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_connected(&self) -> bool {
        self.connected.get()
    }
    fn markets(
        &self,
    ) -> Result<Rc<RefCell<Vec<Rc<RefCell<safe_cex_types::Market>>>>>, ExchangeError> {
        Ok(self.markets.clone())
    }
    fn clear(&self, timer: &u64) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(json!(["clear", timer]));
        if self.name == "clear-error" {
            Err(ExchangeError::new("clear-fail"))
        } else {
            Ok(())
        }
    }
    fn schedule(&self, delay: f64, mut callback: TimerCallback) -> Result<u64, ExchangeError> {
        let id = 100 + self.callbacks.borrow().len() as u64;
        self.events
            .borrow_mut()
            .push(json!(["schedule", delay, id]));
        if self.name == "schedule-error" {
            return Err(ExchangeError::new("schedule-fail"));
        }
        if self.name == "inline-callback" {
            callback()?;
        }
        self.callbacks.borrow_mut().push((id, delay, callback));
        if self.name == "schedule-append" && id == 100 {
            self.markets
                .borrow_mut()
                .push(Rc::new(RefCell::new(market(20))));
        }
        Ok(id)
    }
    fn send(&self, request: &RpcRequest, text: &str) -> Result<(), ExchangeError> {
        self.requests.borrow_mut().push(request.clone());
        self.wires.borrow_mut().push(text.to_owned());
        self.events.borrow_mut().push(json!(["send"]));
        if matches!(self.name, "send-error" | "emit-error") && self.wires.borrow().len() == 1 {
            Err(ExchangeError::new("send-fail"))
        } else {
            Ok(())
        }
    }
    fn emit(&self, message: &str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(json!(["emit", message]));
        if self.name == "emit-error" {
            Err(ExchangeError::new("emit-fail"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn ticker_batches_native() {
    let names = [
        "forty-one",
        "empty",
        "disposed",
        "disconnected",
        "clear-error",
        "schedule-error",
        "send-error",
        "emit-error",
        "callback-disposed",
        "callback-disconnected",
        "stale-generation",
        "generation-nan",
        "inline-callback",
        "live-market",
        "replace-market",
        "schedule-append",
    ];
    let queue_counts = [3, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 2];
    let sent = [41, 0, 0, 0, 0, 0, 2, 1, 0, 0, 0, 0, 0, 2, 2, 21];
    let cached = [41, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 2, 2, 21];
    let event_counts = [45, 1, 0, 0, 1, 2, 5, 4, 2, 2, 2, 2, 2, 4, 4, 24];
    let eager_counts = [4, 1, 0, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 3];
    let rows = corpus()["batchRows"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "BATCH independent floor");
    let mut wire_rows_out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "BATCH independent order");
        let state = Rc::new(PublicSubscriptions::<u64>::default());
        state.ticker_batch_timers.insert(90);
        if name == "generation-nan" {
            state.ticker_batch_generation.set(f64::NAN);
        }
        let count = if name == "empty" {
            0
        } else if name == "forty-one" {
            41
        } else if name == "schedule-append" {
            20
        } else {
            2
        };
        let host = Rc::new(BatchHost {
            name,
            disposed: Cell::new(name == "disposed"),
            connected: Cell::new(name != "disconnected"),
            markets: roster(count),
            events: RefCell::new(Vec::new()),
            requests: RefCell::new(Vec::new()),
            wires: RefCell::new(Vec::new()),
            callbacks: RefCell::new(Vec::new()),
        });
        let actual = subscribe_to_all_tickers(state.clone(), host.clone());
        error(actual.as_ref().err(), &row["error"], "BATCH");
        let retained_old = usize::from(matches!(name, "disposed" | "disconnected" | "clear-error"));
        snapshot(
            &state,
            &row["before"],
            (0, 0, retained_old + queue_counts[index]),
            host.disposed.get(),
            "BATCH BEFORE",
        );
        events(
            &host.events.borrow(),
            &row["eager"],
            eager_counts[index],
            "BATCH eager",
        );
        let queued = host.callbacks.borrow();
        assert_eq!(
            queued.len(),
            queue_counts[index],
            "BATCH native scheduled count"
        );
        assert_eq!(
            row["queued"].as_array().unwrap().len(),
            queue_counts[index],
            "BATCH independent source queue count"
        );
        for (i, (timer, delay, _)) in queued.iter().enumerate() {
            assert_eq!(*timer, 100 + i as u64, "BATCH native scheduled handle");
            assert_eq!(
                delay.to_bits(),
                (i as f64 * 100.0).to_bits(),
                "BATCH native stagger delay"
            );
            assert_eq!(
                *timer,
                row["queued"][i]["id"].as_u64().unwrap(),
                "BATCH source handle"
            );
            assert_eq!(
                delay.to_bits(),
                row["queued"][i]["delay"].as_f64().unwrap().to_bits(),
                "BATCH source delay"
            );
        }
        drop(queued);
        if name == "callback-disposed" {
            host.disposed.set(true);
        }
        if name == "callback-disconnected" {
            host.connected.set(false);
        }
        if name == "stale-generation" {
            state
                .ticker_batch_generation
                .set(state.ticker_batch_generation.get() + 1.0);
        }
        if name == "live-market" {
            host.markets.borrow()[0].borrow_mut().id = "CHANGED".into();
        }
        if name == "replace-market" {
            host.markets.borrow_mut()[0] = Rc::new(RefCell::new(market(99)));
        }
        let callbacks = host.callbacks.take();
        assert_eq!(
            row["callbackErrors"].as_array().unwrap().len(),
            queue_counts[index],
            "BATCH independent callback count"
        );
        for (i, (_, _, mut callback)) in callbacks.into_iter().enumerate() {
            let result = callback();
            error(
                result.as_ref().err(),
                &row["callbackErrors"][i],
                "BATCH CALLBACK",
            );
        }
        frames(
            &host.requests.borrow(),
            &row["requests"],
            sent[index],
            "BATCH",
        );
        snapshot(
            &state,
            &row["state"],
            (sent[index], cached[index], retained_old),
            host.disposed.get(),
            "BATCH",
        );
        events(
            &host.events.borrow(),
            &row["events"],
            event_counts[index],
            "BATCH",
        );
        wire_rows_out.push(wire_rows(host.wires.take(), &row["wires"]));
    }
    for (actual, expected) in wire_rows_out {
        assert_eq!(actual, expected, "BATCH wire after native assertions");
    }
}
#[test]
fn dispose_native() {
    let names = [
        "ok",
        "clear-error",
        "off-error",
        "base-error",
        "clear-replaces-handler",
    ];
    let counts = [3, 1, 2, 3, 3];
    let rows = corpus()["disposeRows"].as_array().unwrap();
    assert_eq!(rows.len(), 5, "DISPOSE independent floor");
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "DISPOSE independent order");
        let state = PublicSubscriptions::<u64>::default();
        state
            .subscriptions
            .borrow_mut()
            .insert(NumberKey::new(1.0), seed(1.0));
        state
            .ticker_subscriptions
            .borrow_mut()
            .insert("m0".into(), 1.0);
        state.ticker_batch_timers.insert(90);
        let disposed = Cell::new(false);
        let handler = RefCell::new(Rc::new(73u64));
        let events_log = RefCell::new(Vec::new());
        let actual = dispose(
            || handler.borrow().clone(),
            &state,
            |timer| {
                assert_eq!(*timer, 90, "DISPOSE native clear handle");
                events_log.borrow_mut().push(json!(["clear", timer]));
                if name == "clear-replaces-handler" {
                    *handler.borrow_mut() = Rc::new(74);
                }
                if name == "clear-error" {
                    Err(ExchangeError::new("clear-fail"))
                } else {
                    Ok(())
                }
            },
            |event, h| {
                assert_eq!(event, "markets:changed", "DISPOSE native emitter event");
                assert!(
                    std::ptr::eq(h, handler.borrow().as_ref()),
                    "DISPOSE native emitter handler"
                );
                assert_eq!(
                    state.subscriptions.borrow().len(),
                    0,
                    "DISPOSE native cleared before emitter"
                );
                assert_eq!(
                    state.ticker_subscriptions.borrow().len(),
                    0,
                    "DISPOSE native ticker cleared before emitter"
                );
                events_log.borrow_mut().push(json!(["off", event, true]));
                if name == "off-error" {
                    Err(ExchangeError::new("off-fail"))
                } else {
                    Ok(())
                }
            },
            || {
                events_log.borrow_mut().push(json!(["base"]));
                if name == "base-error" {
                    Err(ExchangeError::new("base-fail"))
                } else {
                    disposed.set(true);
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "DISPOSE");
        let retained = usize::from(name == "clear-error");
        snapshot(
            &state,
            &row["state"],
            (retained, retained, retained),
            disposed.get(),
            "DISPOSE",
        );
        events(
            &events_log.borrow(),
            &row["events"],
            counts[index],
            "DISPOSE",
        );
    }
}

#[test]
fn minimal_market_event_native() {
    let source = &corpus()["minimalMarketRow"];
    let state = PublicSubscriptions::<u64>::default();
    let added: AddedMarkets<AddedMarket> = RefCell::new(vec![
        Rc::new(RefCell::new(AddedMarket {
            id: "m0".into(),
            symbol: "SYM0".into(),
        })),
        Rc::new(RefCell::new(AddedMarket {
            id: "m1".into(),
            symbol: "SYM1".into(),
        })),
    ]);
    let requests = RefCell::new(Vec::new());
    let wires = RefCell::new(Vec::new());
    on_markets_changed(
        &state,
        &added,
        || true,
        || false,
        || true,
        |request, wire| {
            requests.borrow_mut().push(request.clone());
            wires.borrow_mut().push(wire.to_owned());
            Ok(())
        },
        |message| Err(ExchangeError::new(message)),
    )
    .unwrap();
    frames(&requests.borrow(), &source["requests"], 2, "MINIMAL EVENT");
    snapshot(&state, &source["state"], (2, 2, 0), false, "MINIMAL EVENT");
    let (actual, expected) = wire_rows(wires.into_inner(), &source["wires"]);
    assert_eq!(
        actual, expected,
        "MINIMAL EVENT wire after native assertions"
    );
}
