#![cfg(not(target_arch = "wasm32"))]
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_venue_grvt::{
    public_subscriptions::*,
    types::{GrvtJsonRpcVersion, GrvtSubscriptionMethod},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    sync::OnceLock,
};

fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let result = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public-subscriptions-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "SUBSCRIPTIONS actual source oracle {}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&result.stdout).unwrap()
    })
}
fn scalar(actual: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(actual.is_nan(), "{label} NaN");
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
#[derive(Clone)]
struct Snapshot {
    request_id: f64,
    subscriptions: Vec<(NumberKey, SubscribeRequest)>,
    tickers: Vec<(String, f64)>,
    timers: Vec<u64>,
    generation: f64,
}
fn snapshot(state: &PublicSubscriptions<u64>) -> Snapshot {
    Snapshot {
        request_id: state.request_id.get(),
        subscriptions: state
            .subscriptions
            .borrow()
            .iter()
            .map(|(key, value)| (*key, value.clone()))
            .collect(),
        tickers: state
            .ticker_subscriptions
            .borrow()
            .iter()
            .map(|(key, value)| (key.clone(), *value))
            .collect(),
        timers: state.ticker_batch_timers.values(),
        generation: state.ticker_batch_generation.get(),
    }
}
fn native_sub(actual: &SubscribeRequest, source: &Value, label: &str) {
    assert_eq!(
        actual.jsonrpc,
        GrvtJsonRpcVersion::V2,
        "{label} native version"
    );
    assert_eq!(source["jsonrpc"], "2.0", "{label} source version");
    scalar(actual.id, &source["id"], &format!("{label} native id"));
    assert_eq!(
        actual.method,
        GrvtSubscriptionMethod::Subscribe,
        "{label} native method"
    );
    assert_eq!(source["method"], "subscribe", "{label} source method");
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
fn native_request(actual: &RpcRequest, source: &Value, label: &str) {
    match actual {
        RpcRequest::Subscribe(actual) => native_sub(actual, source, label),
        RpcRequest::Unsubscribe(actual) => {
            assert_eq!(
                actual.jsonrpc,
                GrvtJsonRpcVersion::V2,
                "{label} native version"
            );
            assert_eq!(source["jsonrpc"], "2.0", "{label} source version");
            assert_eq!(
                actual.method,
                GrvtSubscriptionMethod::Unsubscribe,
                "{label} native method"
            );
            assert_eq!(source["method"], "unsubscribe", "{label} source method");
            scalar(actual.id, &source["id"], &format!("{label} native id"));
            scalar(
                actual.params.subscription_id,
                &source["params"]["subscription_id"],
                &format!("{label} native target"),
            );
        }
    }
}
fn native_snapshot(
    actual: &Snapshot,
    source: &Value,
    expected_subscriptions: usize,
    expected_tickers: usize,
    expected_timers: usize,
    label: &str,
) {
    scalar(
        actual.request_id,
        &source["requestId"],
        &format!("{label} native next id"),
    );
    assert_eq!(
        actual.subscriptions.len(),
        expected_subscriptions,
        "{label} native subscription count"
    );
    assert_eq!(
        source["subscriptions"].as_array().unwrap().len(),
        expected_subscriptions,
        "{label} independent source subscription count"
    );
    for ((key, request), entry) in actual
        .subscriptions
        .iter()
        .zip(source["subscriptions"].as_array().unwrap())
    {
        scalar(
            key.value(),
            &entry[0],
            &format!("{label} native SameValueZero key"),
        );
        native_sub(request, &entry[1], label);
    }
    assert_eq!(
        actual.tickers.len(),
        expected_tickers,
        "{label} native ticker count"
    );
    assert_eq!(
        source["tickers"].as_array().unwrap().len(),
        expected_tickers,
        "{label} independent source ticker count"
    );
    for ((instrument, id), entry) in actual
        .tickers
        .iter()
        .zip(source["tickers"].as_array().unwrap())
    {
        assert_eq!(
            instrument,
            entry[0].as_str().unwrap(),
            "{label} native ticker instrument"
        );
        scalar(*id, &entry[1], &format!("{label} native ticker id"));
    }
    assert_eq!(
        actual.timers.len(),
        expected_timers,
        "{label} native timer count"
    );
    assert_eq!(
        source["timers"].as_array().unwrap().len(),
        expected_timers,
        "{label} independent source timer count"
    );
    for (timer, entry) in actual
        .timers
        .iter()
        .zip(source["timers"].as_array().unwrap())
    {
        scalar(*timer as f64, entry, &format!("{label} native timer"));
    }
    scalar(
        actual.generation,
        &source["generation"],
        &format!("{label} native generation"),
    );
}
enum Effect {
    Send(Snapshot),
    Clear(u64),
}
fn native_effects(
    effects: &[Effect],
    source: &Value,
    count: usize,
    send_timers: usize,
    label: &str,
) {
    assert_eq!(effects.len(), count, "{label} native effect count");
    assert_eq!(
        source.as_array().unwrap().len(),
        count,
        "{label} independent source effect count"
    );
    for (actual, expected) in effects.iter().zip(source.as_array().unwrap()) {
        match actual {
            Effect::Send(actual) => {
                assert_eq!(expected[0], "send", "{label} native send order");
                native_snapshot(
                    actual,
                    &expected[1],
                    1,
                    if label == "UNSUBSCRIBE" { 2 } else { 0 },
                    send_timers,
                    label,
                );
            }
            Effect::Clear(actual) => {
                assert_eq!(expected[0], "clear", "{label} native clear order");
                assert_eq!(
                    *actual,
                    expected[1].as_u64().unwrap(),
                    "{label} native clear handle"
                );
            }
        }
    }
}
fn state() -> PublicSubscriptions<u64> {
    let state = PublicSubscriptions::default();
    state.ticker_batch_timers.insert(101);
    state.ticker_batch_timers.insert(202);
    state
}
fn initial_id(name: &str) -> f64 {
    match name {
        "large-id" => 9007199254740992.0,
        "negative-zero" => -0.0,
        "nan-id" | "nan-key" => f64::NAN,
        "infinity-id" => f64::INFINITY,
        _ => 1.0,
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Kline,
    Book,
    Ticker,
}
impl Kind {
    fn subscribe(self) -> &'static str {
        match self {
            Self::Kline => "subscribeToKline",
            Self::Book => "subscribeToOrderbook",
            Self::Ticker => "subscribeToTicker",
        }
    }
    fn unsubscribe(self) -> &'static str {
        match self {
            Self::Kline => "unsubscribeFromKline",
            Self::Book => "unsubscribeFromOrderbook",
            Self::Ticker => "unsubscribeFromTicker",
        }
    }
}
fn subscriptions(kind: Kind) {
    let data = corpus();
    assert_eq!(
        data["subRows"].as_array().unwrap().len(),
        25,
        "SUBSCRIBE independent whole floor"
    );
    let names = [
        "default",
        "explicit",
        "send-error",
        "send-delete",
        "large-id",
        "negative-zero",
        "nan-id",
        "infinity-id",
        "inherited",
    ];
    let mut wires = Vec::new();
    let mut visited = 0;
    for (index, row) in data["subRows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["method"] == kind.subscribe())
        .enumerate()
    {
        let name = names[index];
        assert_eq!(row["name"], name, "SUBSCRIBE independent input order");
        visited += 1;
        let state = state();
        state.request_id.set(initial_id(name));
        let effects = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wire = RefCell::new(Vec::new());
        let send = |request: &RpcRequest, text: &str| {
            requests.borrow_mut().push(request.clone());
            wire.borrow_mut().push(text.to_owned());
            effects.borrow_mut().push(Effect::Send(snapshot(&state)));
            if name == "send-delete" {
                state.subscriptions.borrow_mut().clear();
            }
            if name == "send-error" {
                Err(ExchangeError::new("send-fail"))
            } else {
                Ok(())
            }
        };
        let actual = match kind {
            Kind::Kline => state.subscribe_to_kline(
                "BTC-ß",
                if name == "inherited" {
                    "constructor"
                } else {
                    "8h"
                },
                if name == "explicit" {
                    Some("MARK")
                } else {
                    None
                },
                send,
            ),
            Kind::Book => state.subscribe_to_orderbook(
                "BTC-ß",
                if name == "explicit" { Some(-0.0) } else { None },
                if name == "explicit" {
                    Some(f64::INFINITY)
                } else {
                    None
                },
                send,
            ),
            Kind::Ticker => state.subscribe_to_ticker(
                "BTC-ß",
                if name == "explicit" { Some(25.5) } else { None },
                if name == "explicit" {
                    Some("v1.ticker.d")
                } else {
                    None
                },
                send,
            ),
        };
        error(actual.as_ref().err(), &row["error"], "SUBSCRIBE");
        if let Ok(id) = actual {
            scalar(id, &row["result"], "SUBSCRIBE native returned id");
        }
        assert_eq!(
            requests.borrow().len(),
            1,
            "SUBSCRIBE independent request count"
        );
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            1,
            "SUBSCRIBE independent source request count"
        );
        native_request(&requests.borrow()[0], &row["requests"][0], "SUBSCRIBE");
        native_snapshot(
            &snapshot(&state),
            &row["state"],
            if name == "send-delete" { 0 } else { 1 },
            0,
            2,
            "SUBSCRIBE",
        );
        assert_eq!(
            state.ticker_batch_timers.values(),
            vec![101, 202],
            "SUBSCRIBE independent timer preservation"
        );
        native_effects(&effects.borrow(), &row["effects"], 1, 2, "SUBSCRIBE");
        wires.push((
            wire.into_inner(),
            row["wires"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(
        visited,
        if kind == Kind::Kline { 9 } else { 8 },
        "SUBSCRIBE independent method floor"
    );
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "SUBSCRIBE wire after native assertions");
    }
}
fn unsubscriptions(kind: Kind) {
    let data = corpus();
    assert_eq!(
        data["unsubRows"].as_array().unwrap().len(),
        21,
        "UNSUBSCRIBE independent whole floor"
    );
    let names = [
        "default",
        "missing",
        "send-error",
        "clear-error",
        "nan-key",
        "negative-zero",
        "send-delete",
    ];
    let mut wires = Vec::new();
    let mut visited = 0;
    for (index, row) in data["unsubRows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["method"] == kind.unsubscribe())
        .enumerate()
    {
        let name = names[index];
        assert_eq!(row["name"], name, "UNSUBSCRIBE independent input order");
        visited += 1;
        let state = state();
        let id = initial_id(name);
        state.request_id.set(9.0);
        state.subscriptions.borrow_mut().insert(
            NumberKey::new(id),
            SubscribeRequest {
                jsonrpc: GrvtJsonRpcVersion::V2,
                id,
                method: GrvtSubscriptionMethod::Subscribe,
                params: SubscribeParams {
                    stream: "v1.ticker.d".into(),
                    selectors: ["SEED@500".into()],
                },
            },
        );
        state
            .ticker_subscriptions
            .borrow_mut()
            .insert("FIRST".into(), id);
        state
            .ticker_subscriptions
            .borrow_mut()
            .insert("SECOND".into(), id);
        let effects = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let wire = RefCell::new(Vec::new());
        let send = |request: &RpcRequest, text: &str| {
            requests.borrow_mut().push(request.clone());
            wire.borrow_mut().push(text.to_owned());
            effects.borrow_mut().push(Effect::Send(snapshot(&state)));
            if name == "send-delete" {
                state.subscriptions.borrow_mut().clear();
            }
            if name == "send-error" {
                Err(ExchangeError::new("send-fail"))
            } else {
                Ok(())
            }
        };
        let clear = |timer: &u64| {
            effects.borrow_mut().push(Effect::Clear(*timer));
            if name == "clear-error" {
                Err(ExchangeError::new("clear-fail"))
            } else {
                Ok(())
            }
        };
        let target = if name == "missing" { 17.0 } else { id };
        let actual = match kind {
            Kind::Kline => state.unsubscribe_from_kline(target, send),
            Kind::Book => state.unsubscribe_from_orderbook(target, send),
            Kind::Ticker => state.unsubscribe_from_ticker(target, send, clear),
        };
        error(actual.as_ref().err(), &row["error"], "UNSUBSCRIBE");
        let has_send = name != "missing" && !(kind == Kind::Ticker && name == "clear-error");
        assert_eq!(
            requests.borrow().len(),
            usize::from(has_send),
            "UNSUBSCRIBE independent request count"
        );
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            usize::from(has_send),
            "UNSUBSCRIBE independent source request count"
        );
        if has_send {
            native_request(&requests.borrow()[0], &row["requests"][0], "UNSUBSCRIBE");
        }
        let deleted = has_send && name != "send-error";
        native_snapshot(
            &snapshot(&state),
            &row["state"],
            usize::from(!deleted),
            if kind == Kind::Ticker && deleted && name != "nan-key" {
                1
            } else {
                2
            },
            if kind == Kind::Ticker && name != "missing" && name != "clear-error" {
                0
            } else {
                2
            },
            "UNSUBSCRIBE",
        );
        let clear_count = if kind == Kind::Ticker && name != "missing" {
            if name == "clear-error" { 1 } else { 2 }
        } else {
            0
        };
        native_effects(
            &effects.borrow(),
            &row["effects"],
            clear_count + usize::from(has_send),
            if kind == Kind::Ticker { 0 } else { 2 },
            "UNSUBSCRIBE",
        );
        assert_eq!(
            state.ticker_batch_timers.values(),
            if kind == Kind::Ticker && name != "missing" && name != "clear-error" {
                vec![]
            } else {
                vec![101, 202]
            },
            "UNSUBSCRIBE independent timers"
        );
        wires.push((
            wire.into_inner(),
            row["wires"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
        ));
    }
    assert_eq!(visited, 7, "UNSUBSCRIBE independent method floor");
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "UNSUBSCRIBE wire after native assertions");
    }
}
#[test]
fn subscribe_kline_native() {
    subscriptions(Kind::Kline);
}
#[test]
fn subscribe_book_native() {
    subscriptions(Kind::Book);
}
#[test]
fn subscribe_ticker_native() {
    subscriptions(Kind::Ticker);
}
#[test]
fn unsubscribe_kline_native() {
    unsubscriptions(Kind::Kline);
}
#[test]
fn unsubscribe_book_native() {
    unsubscriptions(Kind::Book);
}
#[test]
fn unsubscribe_ticker_native() {
    unsubscriptions(Kind::Ticker);
}
#[test]
fn cancel_batches_native() {
    let names = [
        "default",
        "clear-error",
        "reentrant-add",
        "reentrant-remove",
        "nested-cancel-add",
        "generation-large",
    ];
    let rows = corpus()["cancelRows"].as_array().unwrap();
    assert_eq!(rows.len(), 6, "CANCEL BATCH independent floor");
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "CANCEL BATCH independent order");
        let state = state();
        if name == "generation-large" {
            state.ticker_batch_generation.set(9007199254740992.0);
        }
        let effects = RefCell::new(Vec::new());
        let nested = Cell::new(false);
        let actual = state.cancel_ticker_batches(|timer: &u64| {
            effects.borrow_mut().push(Effect::Clear(*timer));
            if name == "clear-error" {
                return Err(ExchangeError::new("clear-fail"));
            }
            if name == "reentrant-add" && *timer == 101 {
                state.ticker_batch_timers.insert(303);
            }
            if name == "reentrant-remove" && *timer == 101 {
                state.ticker_batch_timers.remove(&202);
            }
            if name == "nested-cancel-add" && *timer == 101 && !nested.replace(true) {
                state.cancel_ticker_batches(|timer| {
                    effects.borrow_mut().push(Effect::Clear(*timer));
                    Ok(())
                })?;
                state.ticker_batch_timers.insert(303);
            }
            Ok(())
        });
        error(actual.as_ref().err(), &row["error"], "CANCEL BATCH");
        native_snapshot(
            &snapshot(&state),
            &row["state"],
            0,
            0,
            if name == "clear-error" { 2 } else { 0 },
            "CANCEL BATCH",
        );
        let count = match name {
            "clear-error" | "reentrant-remove" => 1,
            "reentrant-add" => 3,
            "nested-cancel-add" => 4,
            _ => 2,
        };
        native_effects(&effects.borrow(), &row["effects"], count, 0, "CANCEL BATCH");
        assert_eq!(
            state.ticker_batch_timers.values(),
            if name == "clear-error" {
                vec![101, 202]
            } else {
                vec![]
            },
            "CANCEL BATCH independent timers"
        );
    }
}
#[test]
fn ensure_ticker_native() {
    let names = [
        "new",
        "existing",
        "disposed",
        "disconnected",
        "send-error",
        "send-delete",
        "disposed-short-circuit",
    ];
    let rows = corpus()["ensureRows"].as_array().unwrap();
    assert_eq!(rows.len(), 7, "ENSURE independent floor");
    let mut wires = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let name = names[index];
        assert_eq!(row["name"], name, "ENSURE independent order");
        let state = state();
        if name == "existing" {
            state
                .ticker_subscriptions
                .borrow_mut()
                .insert("BTC".into(), 77.0);
        }
        let requests = RefCell::new(Vec::new());
        let wire = RefCell::new(Vec::new());
        let effects = RefCell::new(Vec::new());
        let connection_reads = Cell::new(0);
        let actual = state.ensure_ticker_subscription(
            "BTC",
            || name.starts_with("disposed"),
            || {
                connection_reads.set(connection_reads.get() + 1);
                assert_ne!(name, "disposed-short-circuit", "ENSURE guard short circuit");
                name != "disconnected"
            },
            |request, text| {
                requests.borrow_mut().push(request.clone());
                wire.borrow_mut().push(text.to_owned());
                effects.borrow_mut().push(Effect::Send(snapshot(&state)));
                if name == "send-delete" {
                    state.subscriptions.borrow_mut().clear();
                }
                if name == "send-error" {
                    Err(ExchangeError::new("send-fail"))
                } else {
                    Ok(())
                }
            },
        );
        error(actual.as_ref().err(), &row["error"], "ENSURE");
        if let Ok(Some(id)) = actual {
            scalar(id, &row["result"], "ENSURE native returned id");
        } else if actual.is_ok() {
            assert_eq!(
                row["result"]["undefined"], true,
                "ENSURE native absent result"
            );
        }
        let sends = usize::from(matches!(name, "new" | "send-error" | "send-delete"));
        assert_eq!(
            requests.borrow().len(),
            sends,
            "ENSURE independent send count"
        );
        assert_eq!(
            row["requests"].as_array().unwrap().len(),
            sends,
            "ENSURE independent source send count"
        );
        if sends == 1 {
            native_request(&requests.borrow()[0], &row["requests"][0], "ENSURE");
        }
        native_snapshot(
            &snapshot(&state),
            &row["state"],
            usize::from(matches!(name, "new" | "send-error")),
            usize::from(matches!(name, "existing" | "new" | "send-delete")),
            2,
            "ENSURE",
        );
        assert_eq!(
            connection_reads.get(),
            usize::from(!name.starts_with("disposed")),
            "ENSURE independent guard reads"
        );
        native_effects(&effects.borrow(), &row["effects"], sends, 2, "ENSURE");
        assert_eq!(
            state.ticker_batch_timers.values(),
            vec![101, 202],
            "ENSURE independent timers"
        );
        wires.push((
            wire.into_inner(),
            row["wires"]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
        ));
    }
    for (actual, expected) in wires {
        assert_eq!(actual, expected, "ENSURE wire after native assertions");
    }
}
#[test]
fn interval_native() {
    let names = [
        "1m",
        "3m",
        "5m",
        "15m",
        "30m",
        "1h",
        "2h",
        "4h",
        "6h",
        "8h",
        "12h",
        "1d",
        "3d",
        "5d",
        "1w",
        "2w",
        "3w",
        "4w",
        "CI_5_M",
        "unknown",
        "",
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
    ];
    let members = [
        ProtoMember::Constructor,
        ProtoMember::DefineGetter,
        ProtoMember::DefineSetter,
        ProtoMember::HasOwnProperty,
        ProtoMember::LookupGetter,
        ProtoMember::LookupSetter,
        ProtoMember::IsPrototypeOf,
        ProtoMember::PropertyIsEnumerable,
        ProtoMember::ToString,
        ProtoMember::ValueOf,
        ProtoMember::Prototype,
        ProtoMember::ToLocaleString,
    ];
    let rows = corpus()["mapRows"].as_array().unwrap();
    assert_eq!(rows.len(), 33, "MAP independent floor");
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["input"], names[index], "MAP independent input order");
        match map_interval_to_grvt(names[index]) {
            TableHit::Mapped(actual) => {
                assert!(index < 21, "MAP native mapped branch");
                assert_eq!(
                    actual,
                    row["value"].as_str().unwrap(),
                    "MAP native interval"
                );
                assert_eq!(row["kind"], "string", "MAP source string kind");
            }
            TableHit::Inherited(actual) => {
                assert!(index >= 21, "MAP native inherited branch");
                assert_eq!(actual, members[index - 21], "MAP native inherited identity");
                assert_eq!(
                    actual.js_string(),
                    row["text"].as_str().unwrap(),
                    "MAP native inherited interpolation"
                );
                if actual == ProtoMember::Prototype {
                    assert_eq!(row["kind"], "object", "MAP source prototype object");
                    assert!(
                        actual.length().is_none(),
                        "MAP native prototype has no arity"
                    );
                } else {
                    assert_eq!(row["kind"], "function", "MAP declared source callable");
                    assert_eq!(
                        u64::from(actual.length().unwrap()),
                        row["value"]["arity"].as_u64().unwrap(),
                        "MAP native inherited arity"
                    );
                }
            }
            TableHit::Missing => panic!("MAP native fallback missing"),
        }
    }
    assert_eq!(
        corpus()["inheritedCall"],
        true,
        "MAP declared source builtin executes"
    );
}
