// Native values and observers shared by individually reviewed Store/helper adapters.
// This fixture never invokes or redirects a production function under review.
use indexmap::IndexMap;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::{ExchangeError, ExchangeErrorOrigin, LogSeverity};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{
    Candle, FetchedLeverage, Market, Order, OrderBook, OrderBookOrders, PlaceOrderOpts, Position,
    StoreSubscriberError, Ticker,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

pub type Events = Rc<RefCell<Vec<Value>>>;

pub fn events() -> Events {
    Rc::new(RefCell::new(Vec::new()))
}
pub fn push(events: &Events, value: Value) {
    events.borrow_mut().push(value);
}
pub fn observed(events: &Events) -> Vec<Value> {
    events.borrow().clone()
}
pub fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("configured ready fixture unexpectedly pending"),
    }
}
pub fn bits(number: f64) -> String {
    if number.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", number.to_bits())
    }
}
pub fn market(id: &str, symbol: &str) -> Market {
    serde_json::from_value(json!({"id":id,"symbol":symbol,"base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.01,"price":0.2},"limits":{"amount":{"min":0.01,"max":999},"leverage":{"min":1,"max":25}},
        "info":{"ih":"0x11","bd":3,"qd":6},"isPerpetual":true,"isSpot":false})).unwrap()
}
pub fn order(id: &str) -> Order {
    serde_json::from_value(json!({"id":id,"status":"open","symbol":"BTC_USDT_PERP","type":"limit","side":"buy",
        "price":7,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":123,"timeInForce":"GoodTillCancel"})).unwrap()
}
pub fn position(id: &str) -> Position {
    serde_json::from_value(
        json!({"id":id,"symbol":"BTC_USDT_PERP","side":"long","entryPrice":7,"notional":8,
        "leverage":7,"unrealizedPnl":0,"contracts":2,"liquidationPrice":1}),
    )
    .unwrap()
}
pub fn leverage(buy: f64, sell: f64) -> FetchedLeverage {
    FetchedLeverage {
        buy_leverage: buy,
        sell_leverage: sell,
        margin_type: None,
        position_mode: None,
        is_fetching: None,
    }
}
pub fn store(markets: Vec<Market>) -> Rc<DefaultStore> {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            positions: Some(vec![position("p1"), position("p2")]),
            fetched_leverage: Some(IndexMap::from([("OLD".into(), leverage(7.0, 8.0))])),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
pub fn view_leverage(value: &FetchedLeverage) -> Value {
    json!({"buy":bits(value.buy_leverage),"sell":bits(value.sell_leverage),"margin":value.margin_type.is_some(),
        "mode":value.position_mode.is_some(),"fetching":value.is_fetching})
}
pub fn view_cache(store: &dyn Store) -> Value {
    Value::Array(
        store
            .fetched_leverage()
            .unwrap()
            .iter()
            .map(|(key, value)| json!([key, view_leverage(value)]))
            .collect(),
    )
}
pub fn retained(value: &safe_cex_types::retained_json::RetainedJsonInput) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as I;
    match value {
        I::Undefined => json!({"kind":"undefined"}),
        I::Null => json!({"kind":"null"}),
        I::Bool(v) => json!({"kind":"boolean","value":v}),
        I::Text(v) => json!({"kind":"text","units":v}),
        I::Float(_) | I::Lexeme(_) => json!({"kind":"number","bits":bits(value.as_f64().unwrap())}),
        I::Array(v) => json!({"kind":"array","items":v.iter().map(retained).collect::<Vec<_>>()}),
        I::Object(v) => {
            json!({"kind":"object","fields":v.iter().map(|(key,v)|(key,retained(v))).collect::<Vec<_>>()})
        }
    }
}
pub fn view_market(value: &Market) -> Value {
    json!({"id":value.id,"symbol":value.symbol,"base":value.base,"quote":value.quote,"active":value.active,
        "pretty":value.pretty_symbol,"precision":[bits(value.precision.amount),bits(value.precision.price)],
        "amount":[bits(value.limits.amount.min),bits(safe_cex_num::numeric_value_to_number(&value.limits.amount.max))],
        "leverage":[bits(value.limits.leverage.min),bits(value.limits.leverage.max)],"price_limits":value.limits.price.is_some(),
        "flags":[value.is_spot,value.is_perpetual,value.is_inverse,value.is_expiring,value.is_read_only],
        "contract_size":value.contract_size.map(bits),"option":value.option.is_some(),
        "taker":value.taker_fee.map(bits),"maker":value.maker_fee.map(bits),"info":value.info.as_ref().map(retained)})
}
pub fn view_markets(store: &dyn Store) -> Value {
    Value::Array(store.markets().unwrap().iter().map(view_market).collect())
}
pub fn error(name: &str, message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = name.into();
    error.origin = Some(ExchangeErrorOrigin::new(std::io::Error::other(
        "fixed native origin",
    )));
    error.code = Some(safe_cex_exchange::js_runtime::value::Js::Number(55.0));
    error = error.with_retained_response((json!({"native":true})).into());
    error.stack = Some("native stack".into());
    error
}
pub fn view_error(error: &ExchangeError) -> Value {
    let code = error.code.as_ref().map(|code| {
        let profile =
            safe_cex_exchange::js_runtime::value::to_js_json(code).expect("finite fixture code");
        let text = safe_cex_num::js_json_stringify(&profile)
            .expect("source JSON observation")
            .expect("defined fixture code");
        serde_json::from_str::<Value>(&text).expect("finite fixture JSON")
    });
    json!({"name":error.name,"message":error.message,"code":code,"response":error.response_json_view(),"stack":error.stack,
        "origin":error.origin.is_some(),"subscriber":error.store_subscriber_cause.is_some(),"subscriber_ids":error.store_subscriber_ids.len()})
}
pub fn log(
    events: Events,
    fail_at: Option<usize>,
) -> impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> {
    log_with(events, fail_at, error("LogError", "log failed"))
}
pub fn log_with(
    events: Events,
    fail_at: Option<usize>,
    cause: ExchangeError,
) -> impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError> {
    let mut calls = 0;
    move |text, severity| {
        calls += 1;
        push(&events, json!(["log", text, severity.as_str()]));
        if fail_at == Some(calls) {
            Err(cause.clone())
        } else {
            Ok(())
        }
    }
}

pub fn watch(
    store: &Rc<DefaultStore>,
    events: Events,
    fail: bool,
    replace_positions: bool,
    cause: ExchangeError,
) -> Box<dyn Fn()> {
    let weak = Rc::downgrade(store);
    let replaced = Cell::new(false);
    Store::subscribe(
        store.as_ref(),
        Box::new(move |data, worker| {
            let cache = Value::Array(
                data.fetched_leverage
                    .iter()
                    .map(|(key, value)| json!([key, view_leverage(value)]))
                    .collect(),
            );
            push(&events, json!(["notify", cache, worker]));
            if replace_positions && !replaced.replace(true) {
                let store = weak.upgrade().expect("live observed Store");
                let mut replacement = position("live");
                replacement.leverage = 3.0;
                // Retain the worker flag and permit one observed reentrant update.
                Store::update(
                    store.as_ref(),
                    StoreUpdate {
                        positions: Some(vec![replacement]),
                        ..Default::default()
                    },
                    true,
                )
                .unwrap();
            }
            if fail {
                Err(StoreSubscriberError::new("notify failed").with_cause(Rc::new(cause.clone())))
            } else {
                Ok(())
            }
        }),
    )
}
pub fn view_order(value: &Order) -> Value {
    json!({"id":value.id,"symbol":value.symbol,"status":format!("{:?}",value.status),"type":format!("{:?}",value.r#type),
        "side":format!("{:?}",value.side),"price":bits((value.price).to_number()),"amount":bits(value.amount),"filled":bits(value.filled),
        "remaining":bits(value.remaining),"reduce":value.reduce_only,"last":bits(value.last_update),
        "tif":value.time_in_force.map(|v|format!("{v:?}")),"parent":value.parent_id,"client":value.client_order_id})
}
pub fn place(symbol: &str) -> PlaceOrderOpts {
    serde_json::from_value(json!({"symbol":symbol,"type":"limit","side":"buy","amount":2})).unwrap()
}
pub fn view_ticker(value: &Ticker) -> Value {
    json!({"id":value.id,"symbol":value.symbol,"numbers":[bits(value.bid),bits(value.ask),bits(value.last),bits(value.mark),bits(value.index),
        bits(value.percentage),bits(value.open_interest),bits(value.funding_rate),bits(value.funding_time),bits(value.volume),bits(value.quote_volume)],
        "mid":value.mid_price.map(bits),"timestamp":value.timestamp.map(bits),"info":value.info.as_ref().map(retained)})
}
pub fn view_candle(value: &Candle) -> Value {
    json!({"timestamp":bits(value.timestamp),"open":bits(value.open),"high":bits(value.high),"low":bits(value.low),
        "close":bits(value.close),"volume":bits(value.volume),"side":value.last_side.map(|v|format!("{v:?}"))})
}
pub fn view_levels(values: &[OrderBookOrders]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|v| json!([bits(v.price), bits(v.amount), bits(v.total)]))
            .collect(),
    )
}
pub fn view_book(value: &OrderBook) -> Value {
    json!({"bids":view_levels(&value.bids),"asks":view_levels(&value.asks)})
}
pub fn view_positions(store: &dyn Store) -> Value {
    Value::Array(
        store
            .positions()
            .unwrap()
            .iter()
            .map(|v| json!([v.id, v.symbol, bits(v.leverage)]))
            .collect(),
    )
}
