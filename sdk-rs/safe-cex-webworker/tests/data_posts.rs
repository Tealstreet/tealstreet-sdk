//! Inspect actual handler posts before any trace JSON serialization.
#[path = "support/error_post.rs"]
mod error_post_support;
#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
#[allow(dead_code)]
mod runtime;
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_webworker::data_post::WorkerDataPost;
use safe_cex_webworker::{
    call_method::CallMethodHandler, payload::*, subscriptions::OHLCVSubscriptionsHandler,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    sync::LazyLock,
    task::{Context, Poll, Waker},
};

static SOURCE: LazyLock<Value> = LazyLock::new(|| {
    let out = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data-posts-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source["records"].as_array().unwrap().len(),
        7,
        "WORKER-DATA actual source corpus census"
    );
    source
});
fn observe(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Number(n) => {
            json!({"kind":"number","bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
        }
        Js::Null => Value::Null,
        Js::Bool(v) => json!(v),
        Js::String(s) => json!(String::from_utf16(s).unwrap()),
        Js::Array(_) => value.array_values().unwrap().iter().map(observe).collect(),
        Js::Object(_) => value
            .own_entries()
            .unwrap()
            .iter()
            .map(|(k, v)| (String::from_utf16(k).unwrap(), observe(v)))
            .collect(),
        other => panic!("unexpected handler data {other:?}"),
    }
}
fn source_data(method: &str, data: &Js) {
    let rows = SOURCE["records"].as_array().unwrap();
    let matched: Vec<_> = rows.iter().filter(|r| r["method"] == method).collect();
    assert_eq!(
        matched.len(),
        1,
        "WORKER-DATA exact source case identity {method}"
    );
    assert_eq!(
        observe(data),
        matched[0]["expected"]["data"],
        "WORKER-DATA actual source clone {method}"
    );
}

type CandleListener = Rc<RefCell<Option<Rc<dyn Fn(Candle)>>>>;
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    listener: CandleListener,
}
fn candles() -> Vec<Candle> {
    vec![
        Candle {
            timestamp: 1.0,
            open: f64::NAN,
            high: f64::INFINITY,
            low: f64::NEG_INFINITY,
            close: -0.0,
            volume: 7.25,
            last_side: None,
        },
        Candle {
            timestamp: 2.0,
            open: 13.5,
            high: 27.75,
            low: 3.125,
            close: 17.875,
            volume: f64::NAN,
            last_side: Some(OrderSide::Sell),
        },
    ]
}
fn order() -> Order {
    let mut row: Order = serde_json::from_value(json!({"id":"retained","symbol":"BTC","status":"open","type":"limit","side":"buy","price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":1})).unwrap();
    row.price = ModelField::Undefined;
    row.time_in_force = ModelField::Undefined;
    row.amount = f64::NAN;
    row.filled = -0.0;
    row.remaining = f64::INFINITY;
    row
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn set_leverage(&self, _: String, _: f64, _: Option<PositionSide>) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn fetch_ohlcv(&self, _: OHLCVOptions) -> ExchangeFuture<'_, Vec<Candle>> {
        Box::pin(async { Ok(candles()) })
    }
    fn fetch_orders(&self, _: Option<Vec<String>>) -> ExchangeFuture<'_, Vec<Order>> {
        Box::pin(async { Ok(vec![order()]) })
    }
    fn listen_ohlcv(
        &self,
        _: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError> {
        *self.listener.borrow_mut() = Some(callback);
        let listener = self.listener.clone();
        Ok(Box::new(move || {
            listener.borrow_mut().take();
            Ok(())
        }))
    }
}
fn venue() -> Rc<Venue> {
    let store = Rc::new(DefaultStore::new());
    let market=serde_json::from_value(json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            serde_json::from_value(json!({"key":"k","secret":"s"})).unwrap(),
            store,
            Rc::new(runtime::Runtime::default()),
            Default::default(),
        ),
        capabilities: Default::default(),
        listener: Default::default(),
    })
}
fn ready(future: impl Future<Output = ()>) {
    assert!(matches!(
        std::pin::pin!(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(())
    ));
}
fn number(value: &Js, expected: f64, field: &str) {
    let actual = value
        .as_f64()
        .unwrap_or_else(|| panic!("WORKER-DATA native Number {field}, got {value:?}"));
    if expected.is_nan() {
        assert!(actual.is_nan(), "WORKER-DATA native NaN {field}");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "WORKER-DATA native bits {field}"
        );
    }
}
#[test]
fn call_method_candles_keep_native_numbers_and_both_rows() {
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue(),
        WorkerDataPost::new(Rc::new(move |p| {
            sink.borrow_mut().push(p);
            Ok(())
        })),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method(
        "fetchOHLCV".into(),
        "fetchOHLCV".into(),
        vec![json!({"symbol":"BTC","interval":"1m"})],
    ));
    assert_eq!(messages.borrow().len(), 1);
    let messages = messages.borrow();
    let data = &messages[0].data;
    assert!(matches!(data.get("success").unwrap(), Js::Bool(true)));
    let rows = data.get("result").unwrap().array_values().unwrap();
    assert_eq!(
        rows.len(),
        2,
        "WORKER-DATA candle output cardinality before field walk"
    );
    let mut compared = 0;
    for (row, expected) in rows.iter().zip(candles()) {
        for (field, n) in [
            ("timestamp", expected.timestamp),
            ("open", expected.open),
            ("high", expected.high),
            ("low", expected.low),
            ("close", expected.close),
            ("volume", expected.volume),
        ] {
            number(&row.get(field).unwrap(), n, field);
            compared += 1;
        }
    }
    assert_eq!(compared, 12);
    source_data("fetchOHLCV", data);
}
#[test]
fn call_method_orders_keep_own_undefined_and_native_numbers() {
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue(),
        WorkerDataPost::new(Rc::new(move |p| {
            sink.borrow_mut().push(p);
            Ok(())
        })),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method(
        "fetchOrders".into(),
        "fetchOrders".into(),
        vec![Value::Null],
    ));
    let messages = messages.borrow();
    let row = messages[0]
        .data
        .get("result")
        .unwrap()
        .array_values()
        .unwrap()
        .remove(0);
    assert!(
        row.has_own("timeInForce"),
        "WORKER-DATA own undefined timeInForce must survive post"
    );
    assert!(row.get("timeInForce").unwrap().is_undefined());
    assert!(row.has_own("price") && row.get("price").unwrap().is_undefined());
    assert!(
        !row.has_own("parentId"),
        "WORKER-DATA absent field must stay absent"
    );
    number(&row.get("amount").unwrap(), f64::NAN, "amount");
    number(&row.get("filled").unwrap(), -0.0, "filled");
    number(&row.get("remaining").unwrap(), f64::INFINITY, "remaining");
    source_data("fetchOrders", &messages[0].data);
}
#[test]
fn candle_subscription_keeps_native_numbers_at_the_real_post_sink() {
    let venue = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = OHLCVSubscriptionsHandler::new(
        venue.clone(),
        WorkerDataPost::new(Rc::new(move |p| {
            sink.borrow_mut().push(p);
            Ok(())
        })),
        error_post_support::unexpected(),
    );
    handler
        .handle(StoreWorkerPayload {
            op: StoreWorkerOp::SubscribeOHLCV,
            instance_id: None,
            data: Some(json!({"symbol":"BTC","interval":"1m","subId":"a","tag":"metadata"})),
        })
        .unwrap();
    let callback = venue.listener.borrow().clone().unwrap();
    callback(candles().remove(0));
    assert_eq!(messages.borrow().len(), 1);
    let messages = messages.borrow();
    let data = &messages[0].data;
    assert!(data.get("subId").unwrap().strict_equals(&Js::str("a")));
    assert!(data.get("tag").unwrap().strict_equals(&Js::str("metadata")));
    number(
        &data.get("candle").unwrap().get("open").unwrap(),
        f64::NAN,
        "subscription open",
    );
    number(
        &data.get("candle").unwrap().get("high").unwrap(),
        f64::INFINITY,
        "subscription high",
    );
    number(
        &data.get("candle").unwrap().get("low").unwrap(),
        f64::NEG_INFINITY,
        "subscription low",
    );
    number(
        &data.get("candle").unwrap().get("close").unwrap(),
        -0.0,
        "subscription close",
    );
    source_data("listenOHLCV", data);
}

#[test]
fn assigned_void_is_own_undefined_while_unknown_method_omits_result() {
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue(),
        WorkerDataPost::new(Rc::new(move |p| {
            sink.borrow_mut().push(p);
            Ok(())
        })),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method(
        "setLeverage".into(),
        "setLeverage".into(),
        vec![json!("BTC"), json!(3), Value::Null],
    ));
    ready(handler.handle_call_method("notAMethod".into(), "notAMethod".into(), vec![]));
    let messages = messages.borrow();
    assert_eq!(messages.len(), 2);
    assert!(
        messages[0].data.has_own("result")
            && messages[0].data.get("result").unwrap().is_undefined()
    );
    assert!(!messages[1].data.has_own("result"));
    source_data("setLeverage", &messages[0].data);
    source_data("notAMethod", &messages[1].data);
}

#[test]
fn data_post_rejection_reaches_the_original_reply_catch() {
    for method in ["fetchOrders", "notAMethod"] {
        let posts = Rc::new(RefCell::new(Vec::new()));
        let sink = posts.clone();
        let mut failure = ExchangeError::new("native post rejected");
        failure.name = "TypeError".into();
        failure.stack = Some("native stack".into());
        failure.code = Some(json!(42));
        let handler = CallMethodHandler::new(
            venue(),
            WorkerDataPost::new(Rc::new(move |payload| {
                let first = sink.borrow().is_empty();
                sink.borrow_mut().push(payload);
                if first { Err(failure.clone()) } else { Ok(()) }
            })),
            error_post_support::unexpected(),
        );
        ready(handler.handle_call_method("failed-post".into(), method.into(), vec![Value::Null]));
        let posts = posts.borrow();
        assert_eq!(
            posts.len(),
            2,
            "WORKER-DATA source catch retries the post as an error reply"
        );
        assert!(
            posts[1]
                .data
                .get("success")
                .unwrap()
                .strict_equals(&Js::Bool(false))
        );
        let error = posts[1].data.get("result").unwrap();
        assert!(
            error
                .get("message")
                .unwrap()
                .strict_equals(&Js::str("native post rejected"))
        );
        assert!(
            error
                .get("name")
                .unwrap()
                .strict_equals(&Js::str("TypeError"))
        );
        assert!(
            error
                .get("stack")
                .unwrap()
                .strict_equals(&Js::str("native stack"))
        );
        number(
            &error.get("code").unwrap(),
            42.0,
            "original post error code",
        );
        source_data(&format!("post-rejected:{method}"), &posts[1].data);
    }
}
