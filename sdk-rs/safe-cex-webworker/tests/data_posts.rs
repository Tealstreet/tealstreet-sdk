//! Inspect actual handler posts before any trace JSON serialization.
#[path = "support/error_post.rs"]
mod error_post_support;
#[path = "support/runtime.rs"]
#[allow(dead_code)]
mod runtime;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture, ListenerCleanup},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_webworker::{
    call_method::CallMethodHandler, payload::*, subscriptions::OHLCVSubscriptionsHandler,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

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
fn number(value: &Value, expected: f64, field: &str) {
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
        Rc::new(move |p| sink.borrow_mut().push(p)),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method(
        "candles".into(),
        "fetchOHLCV".into(),
        vec![json!({"symbol":"BTC","interval":"1m"})],
    ));
    assert_eq!(messages.borrow().len(), 1);
    let messages = messages.borrow();
    let data = messages[0].data.as_ref().unwrap();
    assert_eq!(data["success"], true);
    let rows = data["result"].as_array().unwrap();
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
            number(&row[field], n, field);
            compared += 1;
        }
    }
    assert_eq!(compared, 12);
}
#[test]
fn call_method_orders_keep_own_undefined_and_native_numbers() {
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = CallMethodHandler::new(
        venue(),
        Rc::new(move |p| sink.borrow_mut().push(p)),
        error_post_support::unexpected(),
    );
    ready(handler.handle_call_method("orders".into(), "fetchOrders".into(), vec![Value::Null]));
    let messages = messages.borrow();
    let row = &messages[0].data.as_ref().unwrap()["result"][0];
    assert!(
        row.as_object().unwrap().contains_key("timeInForce"),
        "WORKER-DATA own undefined timeInForce must survive post"
    );
    number(&row["amount"], f64::NAN, "amount");
    number(&row["filled"], -0.0, "filled");
    number(&row["remaining"], f64::INFINITY, "remaining");
}
#[test]
fn candle_subscription_keeps_native_numbers_at_the_real_post_sink() {
    let venue = venue();
    let messages = Rc::new(RefCell::new(Vec::new()));
    let sink = messages.clone();
    let handler = OHLCVSubscriptionsHandler::new(
        venue.clone(),
        Rc::new(move |p| sink.borrow_mut().push(p)),
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
    let data = messages[0].data.as_ref().unwrap();
    assert_eq!(data["subId"], "a");
    assert_eq!(data["tag"], "metadata");
    number(&data["candle"]["open"], f64::NAN, "subscription open");
    number(&data["candle"]["high"], f64::INFINITY, "subscription high");
    number(
        &data["candle"]["low"],
        f64::NEG_INFINITY,
        "subscription low",
    );
    number(&data["candle"]["close"], -0.0, "subscription close");
}
