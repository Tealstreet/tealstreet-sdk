use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{ExchangeName, MarginType, Position, PositionMode};
use safe_cex_venue_kucoin::position_metadata::{
    get_or_fetch_margin_type, get_or_fetch_position_mode,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};

type TaskFuture = Pin<Box<dyn Future<Output = ()>>>;
#[derive(Default)]
struct Runtime {
    tasks: RefCell<Vec<TaskFuture>>,
}
struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
impl Runtime {
    fn pump(&self) {
        for mut task in std::mem::take(&mut *self.tasks.borrow_mut()) {
            assert!(
                task.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_ready()
            );
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, future: TaskFuture) -> Box<dyn ExchangeTask> {
        self.tasks.borrow_mut().push(future);
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
    fn report_raw_exception(&self, _: &safe_cex_exchange::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    fetched: Result<Vec<Position>, ExchangeError>,
    calls: Cell<usize>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn fetch_positions(&self, symbols: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Position>> {
        assert_eq!(
            symbols,
            Some(vec!["BTC/USDT".into()]),
            "native dependency symbol"
        );
        self.calls.set(self.calls.get() + 1);
        Box::pin(async { self.fetched.clone() })
    }
}
fn setup(
    positions: Vec<Position>,
    loaded: bool,
    fetched: Result<Vec<Position>, ExchangeError>,
) -> (Rc<Venue>, Rc<Runtime>) {
    let store = Rc::new(DefaultStore::new());
    Store::update(store.as_ref(), StoreUpdate {
        positions: Some(positions),
        loaded: Some(serde_json::from_value(json!({"positions":loaded,"markets":false,"tickers":false,"balance":false,"orders":false})).unwrap()),
        ..Default::default()
    }, false).unwrap();
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoin,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        store,
        runtime.clone(),
        ExchangeSettings::default(),
    );
    (
        Rc::new(Venue {
            base,
            capabilities: ExchangeCapabilities::default(),
            fetched,
            calls: Cell::new(0),
        }),
        runtime,
    )
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn position(id: &str, mode: Value, margin: Value) -> Value {
    let mut value = json!({"id":id,"symbol":"BTC/USDT","side":"long","entryPrice":11,"notional":12,
        "leverage":13,"unrealizedPnl":14,"contracts":15,"liquidationPrice":16});
    if !mode.is_null() {
        value["positionMode"] = mode;
    }
    if !margin.is_null() {
        value["marginType"] = margin;
    }
    value
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-metadata.cjs"
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
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(captures.len(), cases.len(), "metadata source case count");
    captures
}
fn wire<T: serde::Serialize>(result: Result<T, ExchangeError>) -> Value {
    match result {
        Ok(value) => json!({"ok":value}),
        Err(error) => json!({"message":error.message,"name":error.name,"sameFailure":false}),
    }
}
#[test]
fn live_metadata_selects_first_native_enum_and_preserves_missing_errors() {
    let hedged = position("first", json!("hedged"), json!("cross"));
    let oneway = position("second", json!("oneway"), json!("isolated"));
    let rows = [
        json!([hedged, oneway]),
        json!([oneway, hedged]),
        json!([position("missing", Value::Null, Value::Null), hedged]),
        json!([
            position("missing-mode", Value::Null, json!("cross")),
            hedged
        ]),
        json!([
            position("missing-margin", json!("hedged"), Value::Null),
            oneway
        ]),
        json!([]),
        json!([oneway]),
        json!([]),
    ];
    let modes = [
        Some(PositionMode::Hedged),
        Some(PositionMode::OneWay),
        None,
        None,
        Some(PositionMode::Hedged),
        None,
        Some(PositionMode::OneWay),
        None,
    ];
    let margins = [
        Some(MarginType::Cross),
        Some(MarginType::Isolated),
        None,
        Some(MarginType::Cross),
        None,
        None,
        Some(MarginType::Isolated),
        None,
    ];
    let mut cases = vec![];
    let mut native = vec![];
    for index in 0..8 {
        let symbol = if index == 7 { "" } else { "BTC/USDT" };
        let (venue, _) = setup(
            serde_json::from_value(rows[index].clone()).unwrap(),
            false,
            Ok(vec![]),
        );
        let mode = ready(get_or_fetch_position_mode(venue.clone(), symbol));
        let margin = ready(get_or_fetch_margin_type(venue.clone(), symbol));
        assert_eq!(
            mode.as_ref().ok().copied(),
            modes[index],
            "KUCOIN-METADATA native position mode {index}"
        );
        assert_eq!(
            margin.as_ref().ok().copied(),
            margins[index],
            "KUCOIN-METADATA native margin type {index}"
        );
        for (error, missing, empty) in [
            (
                mode.as_ref().err(),
                "position mode",
                index == 5 || index == 7,
            ),
            (
                margin.as_ref().err(),
                "margin type",
                index == 5 || index == 7,
            ),
        ] {
            if let Some(error) = error {
                let message = if empty {
                    format!("No positions found for symbol {symbol} needed to determine {missing}")
                } else {
                    format!("Position missing {missing} for symbol {symbol}")
                };
                assert_eq!(
                    error.message, message,
                    "native missing metadata message {index}"
                );
                assert_eq!(error.name, "Error", "native missing metadata error name");
            }
        }
        assert_eq!(
            venue.calls.get(),
            0,
            "native cached or unloaded dependency does not fetch"
        );
        cases.push(json!({"symbol":symbol,"positions":rows[index]}));
        native.push((mode, margin));
    }
    // All native enum/error assertions above precede every wire assertion below.
    let expected = source(&cases);
    assert_eq!(expected.len(), 8, "independent metadata corpus floor");
    for (index, (mode, margin)) in native.into_iter().enumerate() {
        assert_eq!(
            expected[index]["calls"],
            json!([cases[index]["symbol"], cases[index]["symbol"]])
        );
        assert_eq!(wire(mode), expected[index]["getOrFetchPositionMode"]);
        assert_eq!(wire(margin), expected[index]["getOrFetchMarginType"]);
    }
}
#[test]
fn actual_core_fetch_is_awaited_and_dependency_errors_are_retained() {
    let positions = serde_json::from_value(json!([position(
        "fetched",
        json!("hedged"),
        json!("isolated")
    )]))
    .unwrap();
    let (venue, runtime) = setup(vec![], true, Ok(positions));
    let mut future = Box::pin(get_or_fetch_position_mode(venue.clone(), "BTC/USDT"));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(runtime.tasks.borrow().len(), 1, "one shared fetch task");
    runtime.pump();
    assert_eq!(
        ready(future).unwrap(),
        PositionMode::Hedged,
        "native fetched position mode"
    );
    assert_eq!(
        ready(get_or_fetch_margin_type(venue.clone(), "BTC/USDT")).unwrap(),
        MarginType::Isolated,
        "native fetched margin type"
    );
    assert_eq!(venue.calls.get(), 1, "shared fetched snapshot reused");

    let mut failure = ExchangeError::new("position dependency failure");
    failure.name = "TypeError".into();
    failure.code = Some(safe_cex_exchange::js_runtime::value::Js::str("DEPENDENCY"));
    failure.response = Some(json!({"data":{"msg":"distinct response"}}));
    let (venue, runtime) = setup(vec![], true, Err(failure.clone()));
    let mut future = Box::pin(get_or_fetch_margin_type(venue.clone(), "BTC/USDT"));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    runtime.pump();
    assert_eq!(
        ready(future).unwrap_err(),
        failure,
        "native margin dependency error metadata"
    );
    assert_eq!(
        ready(get_or_fetch_position_mode(venue.clone(), "BTC/USDT")).unwrap_err(),
        failure,
        "native mode dependency error metadata"
    );
    assert_eq!(venue.calls.get(), 1, "core rejection cache reused");
    let expected = source(&[json!({"symbol":"BTC/USDT","positions":[],"failure":true})]);
    for name in ["getOrFetchPositionMode", "getOrFetchMarginType"] {
        assert_eq!(
            expected[0][name],
            json!({"message":"position dependency failure","name":"TypeError","sameFailure":true})
        );
    }
}

#[test]
fn metadata_error_symbol_retains_the_shared_utf8_boundary() {
    let raw = r#""\ud800""#;
    assert!(
        serde_json::from_str::<String>(raw).is_err(),
        "native lone-surrogate symbol refusal"
    );
    let expected = source(&[json!({"symbolRaw":raw,"positions":[]})]);
    assert_eq!(expected.len(), 1, "independent UTF-16 boundary corpus");
    assert_eq!(
        expected[0]["outJson"],
        r#"{"calls":["\ud800","\ud800"],"getOrFetchPositionMode":{"message":"No positions found for symbol \ud800 needed to determine position mode","name":"Error","sameFailure":false},"getOrFetchMarginType":{"message":"No positions found for symbol \ud800 needed to determine margin type","name":"Error","sameFailure":false}}"#
    );
}
