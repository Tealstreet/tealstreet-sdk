#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::{js_from_value, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::{
    Balance, ExchangeName, ExchangeOptions, MarginType, Market, OHLCVOptions, Order, OrderSide,
    OrderStatus, OrderType, Position, PositionMode, PositionSide, StoreData, Ticker,
};
use safe_cex_venue_blofin::{
    api::{ApiRuntime, create_api},
    order_services::{ServiceError, ServiceHost},
    rest_lifecycle::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/rest_lifecycle_oracle.cjs"
            ))
            .env("TZ", "UTC")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "actual_source_oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            61,
            "independent REST/lifecycle floor"
        );
        assert_eq!(
            v["api"].as_array().unwrap().len(),
            30,
            "independent API floor"
        );
        assert_eq!(
            v["constructors"].as_array().unwrap().len(),
            5,
            "independent constructor floor"
        );
        assert_eq!(v["stoppedAfter"], 61);
        eprintln!("source header {}", v["header"]);
        v
    })
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
    }
    panic!("fixture poll floor1000 exhausted")
}
fn num(a: f64, e: &Value) {
    let s = e["number"]
        .as_str()
        .expect("source native numeric observation");
    if s == "nan" {
        assert!(a.is_nan(), "native NaN")
    } else {
        assert_eq!(
            a.to_bits(),
            u64::from_str_radix(s, 16).unwrap(),
            "native numeric bits"
        );
    }
}
fn plain(a: &Value, e: &Value) {
    if e.get("number").is_some() {
        num(js_value_as_f64(a).expect("native info number"), e);
        return;
    }
    match (a, e) {
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len());
            for (a, e) in a.iter().zip(e) {
                plain(a, e)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len());
            for (k, e) in e {
                plain(a.get(k).expect("native own field"), e);
            }
        }
        _ => assert_eq!(a, e, "native retained field"),
    }
}
fn wire(a: &Value, e: &Value) {
    match (a, e) {
        (Value::Number(a), Value::Number(e)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            e.as_f64().unwrap().to_bits(),
            "wire Number bits"
        ),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len());
            for (a, e) in a.iter().zip(e) {
                wire(a, e)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len());
            for (k, e) in e {
                wire(a.get(k).unwrap(), e)
            }
        }
        _ => assert_eq!(a, e, "wire field"),
    }
}
fn market(a: &Market, e: &Value) {
    assert_eq!(a.id, e["id"]);
    assert_eq!(a.symbol, e["symbol"]);
    assert_eq!(a.base, e["base"]);
    assert_eq!(a.quote, e["quote"]);
    assert_eq!(a.active, e["active"]);
    num(a.precision.amount, &e["precision"]["amount"]);
    num(a.precision.price, &e["precision"]["price"]);
    num(a.limits.amount.min, &e["limits"]["amount"]["min"]);
    num(a.limits.amount.max, &e["limits"]["amount"]["max"]);
    num(a.limits.leverage.min, &e["limits"]["leverage"]["min"]);
    num(a.limits.leverage.max, &e["limits"]["leverage"]["max"]);
    num(a.contract_size.unwrap(), &e["contractSize"]);
    retained_assert::assert_value(a.info.as_ref().unwrap(), &e["info"]);
    assert!(
        a.pretty_symbol.is_none()
            && a.is_expiring.is_none()
            && a.is_inverse.is_none()
            && a.is_spot.is_none()
            && a.is_perpetual.is_none()
            && a.is_read_only.is_none()
            && a.taker_fee.is_none()
            && a.maker_fee.is_none()
    );
}
fn balance(a: &Balance, e: &Value) {
    num(a.total, &e["total"]);
    num(a.free, &e["free"]);
    num(a.used, &e["used"]);
    num(a.upnl, &e["upnl"]);
    assert!(a.info.is_none() && a.currencies.is_none() && a.coins.is_none());
}
fn position(a: &Position, e: &Value) {
    assert_eq!(a.id, e["id"]);
    assert_eq!(a.symbol, e["symbol"]);
    assert_eq!(a.side, PositionSide::Long);
    assert_eq!(e["side"], "long");
    assert_eq!(a.margin_type, Some(MarginType::Cross));
    assert_eq!(e["marginType"], "cross");
    assert_eq!(a.position_mode, Some(PositionMode::OneWay));
    assert_eq!(e["positionMode"], "oneway");
    for (a, key) in [
        (a.entry_price, "entryPrice"),
        (a.notional, "notional"),
        (a.leverage, "leverage"),
        (a.unrealized_pnl, "unrealizedPnl"),
        (a.contracts, "contracts"),
        (a.liquidation_price, "liquidationPrice"),
    ] {
        num(a, &e[key])
    }
    if e.get("info").is_none() || e["info"].get("undefined") == Some(&Value::Bool(true)) {
        assert!(a.info.is_none())
    } else {
        retained_assert::assert_value(a.info.as_ref().unwrap(), &e["info"]);
    }
    if let Some(v) = e.get("marginSize") {
        num(a.margin_size.unwrap(), v)
    } else {
        assert!(a.margin_size.is_none())
    }
    assert!(a.realized_pnl.is_none() && a.extra.is_none() && a.last_update.is_none());
}
fn order(a: &Order, e: &Value) {
    assert_eq!(a.id, e["id"]);
    assert_eq!(a.symbol, e["symbol"]);
    assert_eq!(a.status, OrderStatus::Open);
    assert_eq!(e["status"], "open");
    assert_eq!(a.r#type, OrderType::Limit);
    assert_eq!(e["type"], "limit");
    assert_eq!(a.side, OrderSide::Buy);
    assert_eq!(e["side"], "buy");
    for (a, key) in [
        (a.price.to_number(), "price"),
        (a.amount, "amount"),
        (a.filled, "filled"),
        (a.remaining, "remaining"),
        (a.last_update, "lastUpdate"),
    ] {
        num(a, &e[key])
    }
    assert_eq!(a.reduce_only.value().copied(), e["reduceOnly"].as_bool());
    assert!(
        a.parent_id.is_none()
            && a.close.is_none()
            && a.info.is_none()
            && a.time_in_force.is_none()
            && a.trigger_type.is_none()
            && a.tp_trigger_type.is_none()
            && a.sl_trigger_type.is_none()
            && a.take_profit.is_none()
            && a.stop_loss.is_none()
            && a.extra.is_none()
    );
}
fn ticker(a: &Ticker, e: &Value) {
    assert_eq!(a.id, e["id"]);
    assert_eq!(a.symbol, e["symbol"]);
    for (a, key) in [
        (a.bid, "bid"),
        (a.ask, "ask"),
        (a.last, "last"),
        (a.mark, "mark"),
        (a.index, "index"),
        (a.percentage, "percentage"),
        (a.funding_rate, "fundingRate"),
        (a.volume, "volume"),
        (a.quote_volume, "quoteVolume"),
    ] {
        num(a, &e[key])
    }
    num(a.open_interest, &e["openInterest"]);
    num(a.funding_time, &e["fundingTime"]);
}
fn state(a: &StoreData, e: &Value) {
    assert_eq!(a.markets.len(), e["markets"].as_array().unwrap().len());
    for (a, e) in a.markets.iter().zip(e["markets"].as_array().unwrap()) {
        market(a, e)
    }
    assert_eq!(a.positions.len(), e["positions"].as_array().unwrap().len());
    for (a, e) in a.positions.iter().zip(e["positions"].as_array().unwrap()) {
        position(a, e)
    }
    assert_eq!(a.orders.len(), e["orders"].as_array().unwrap().len());
    for (a, e) in a.orders.iter().zip(e["orders"].as_array().unwrap()) {
        order(a, e)
    }
    assert_eq!(a.tickers.len(), e["tickers"].as_array().unwrap().len());
    for (a, e) in a.tickers.iter().zip(e["tickers"].as_array().unwrap()) {
        ticker(a, e)
    }
    balance(&a.balance, &e["balance"]);
    for (v, key) in [
        (a.loaded.markets, "markets"),
        (a.loaded.tickers, "tickers"),
        (a.loaded.balance, "balance"),
        (a.loaded.positions, "positions"),
        (a.loaded.orders, "orders"),
    ] {
        assert_eq!(v, e["loaded"][key]);
    }
}
fn snapshot(s: &DefaultStore) -> StoreData {
    Store::serialize(s).unwrap()
}
fn store(i: &Value) -> DefaultStore {
    let models = &corpus()["models"];
    let s = DefaultStore::new();
    Store::update(
        &s,
        StoreUpdate {
            markets: Some(js_from_value(json!([models["market"]])).unwrap()),
            positions: Some(js_from_value(json!([models["position"], models["closed"]])).unwrap()),
            orders: Some(
                js_from_value(if i["storedOrdersEmpty"] == true {
                    json!([])
                } else {
                    json!([models["order"]])
                })
                .unwrap(),
            ),
            balance: Some(js_from_value(json!({"total":77,"free":66,"used":11,"upnl":4})).unwrap()),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    s
}
fn response(value: Value, phase: u8) -> RestResponse {
    RestResponse {
        status: 200,
        reason: None,
        headers: BTreeMap::new(),
        body: match phase % 3 {
            0 => RestResponseBody::Encoded(value.to_string().into_bytes()),
            1 => RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
            ),
            _ => RestResponseBody::Transformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(value).unwrap(),
            ),
        },
    }
}
fn failure(message: &str, hint: &Value) -> ServiceError<String> {
    let mut e = RestFailure::pre_transmit(message.into(), RestRequest::default());
    e.response = Some(Box::new(response(json!({"msg":hint}), 1)));
    ServiceError::Rest(Box::new(e))
}
fn error(e: ServiceError<String>) -> String {
    match e {
        ServiceError::Host(e) | ServiceError::Input(e) => e,
        ServiceError::Rest(e) => e.message,
        ServiceError::Ingress(e) => e.to_string(),
        ServiceError::Coercion(e) => e.message.into(),
        ServiceError::Store(e) => e.to_string(),
    }
}
struct ReadAdapter<'a> {
    input: &'a Value,
    requests: RefCell<Vec<RestRequest>>,
    variant: u8,
}
impl RestAdapter for ReadAdapter<'_> {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(r.clone());
        let mut yielded = false;
        Box::pin(std::future::poll_fn(move |cx| {
            if self.variant < 3 && !yielded {
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if self.input.get("throw").is_some() {
                if let ServiceError::Rest(mut e) =
                    failure(self.input["throw"].as_str().unwrap(), &self.input["hint"])
                {
                    *e.request = r.clone();
                    return Poll::Ready(Err(*e));
                }
                unreachable!()
            }
            Poll::Ready(Ok(response(self.input["body"].clone(), self.variant)))
        }))
    }
}
fn read_cases(method: &str, expected_count: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == method)
        .collect();
    assert_eq!(cases.len(), expected_count);
    for c in &cases {
        for variant in 0..6 {
            let i = &c["input"];
            let s = store(i);
            let logs = RefCell::new(Vec::new());
            let h = ServiceHost {
                emit_and_log_error: Box::new(|v| {
                    logs.borrow_mut()
                        .push(v.cloned().unwrap_or_else(|| json!({"undefined":true})));
                    Ok::<_, String>(())
                }),
                log: Box::new(|_, _| Ok(())),
            };
            let adapter = ReadAdapter {
                input: i,
                requests: RefCell::new(Vec::new()),
                variant,
            };
            let expected = &c["result"]["ok"];
            match method {
                "validateAccount" => {
                    let mut actual = ready(validate_account(&adapter, &h)).unwrap();
                    plain(&actual.error, &expected["error"]);
                    if c["errorAlias"] == true {
                        actual.error["reason"] = json!("caller changed");
                        assert_eq!(
                            i["hint"]["reason"], "blocked",
                            "native validation error snapshot independent"
                        );
                    }
                    if expected.get("balance").is_none()
                        || expected["balance"].get("undefined") == Some(&Value::Bool(true))
                    {
                        assert!(actual.balance.is_none())
                    } else {
                        balance(actual.balance.as_ref().unwrap(), &expected["balance"])
                    }
                }
                "fetchMarkets" => {
                    let mut actual =
                        ready(fetch_markets(&s, &adapter, &h, || "cache-id".into())).unwrap();
                    assert_eq!(actual.len(), expected.as_array().unwrap().len());
                    for (a, e) in actual.iter().zip(expected.as_array().unwrap()) {
                        market(a, e)
                    }
                    if c["fallbackAlias"] == true {
                        *actual[0].info.as_mut().unwrap().get_mut("stored").unwrap() =
                            json!(false).into();
                        assert!(
                            s.markets().unwrap()[0]
                                .info
                                .as_ref()
                                .unwrap()
                                .get("stored")
                                .unwrap()
                                .as_bool()
                                .unwrap(),
                            "native fallback snapshots independent"
                        )
                    }
                }
                "fetchOHLCV" => {
                    let opts: OHLCVOptions = serde_json::from_value(i["options"].clone()).unwrap();
                    let actual =
                        ready(fetch_ohlcv(&opts, &s, &adapter, &h, || 1710000000123.0)).unwrap();
                    assert_eq!(actual.len(), expected.as_array().unwrap().len());
                    for (a, e) in actual.iter().zip(expected.as_array().unwrap()) {
                        for (a, key) in [
                            (a.timestamp, "timestamp"),
                            (a.open, "open"),
                            (a.high, "high"),
                            (a.low, "low"),
                            (a.close, "close"),
                            (a.volume, "volume"),
                        ] {
                            num(a, &e[key])
                        }
                        assert!(a.last_side.is_none());
                    }
                }
                _ => unreachable!(),
            }
            state(&snapshot(&s), &c["state"]);
            // Request and diagnostic assertions follow every decoded native output/state assertion.
            let gets: Vec<_> = c["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t[0] == "get")
                .collect();
            assert_eq!(adapter.requests.borrow().len(), gets.len());
            for (r, e) in adapter.requests.borrow().iter().zip(gets) {
                assert_eq!(r.method, "GET");
                assert_eq!(r.url, e[1]);
                wire(r.params.as_ref().unwrap_or(&Value::Null), &e[2]);
                assert!(r.body.is_none());
            }
            let emits: Vec<_> = c["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t[0] == "emit")
                .collect();
            assert_eq!(logs.borrow().len(), emits.len());
            for (a, e) in logs.borrow().iter().zip(emits) {
                plain(a, &e[1])
            }
        }
    }
}
#[test]
fn native_validate_account() {
    read_cases("validateAccount", 8)
}
#[test]
fn native_fetch_markets() {
    read_cases("fetchMarkets", 6)
}
#[test]
fn native_fetch_ohlcv() {
    read_cases("fetchOHLCV", 22)
}

struct Life<'a> {
    s: DefaultStore,
    input: &'a Value,
    trace: Rc<RefCell<Vec<Value>>>,
    disposed: Cell<bool>,
    markets_released: Cell<bool>,
    service: ServiceHost<'a, String>,
    variant: u8,
}
impl Life<'_> {
    fn fetched<'a, T: 'a>(&'a self, kind: &str, value: T) -> LifecycleFuture<'a, T, String> {
        self.trace.borrow_mut().push(json!([kind]));
        let failure =
            self.input["fail"] == kind || (kind == "orders" && self.input["failOrders"] == true);
        let dispose = self.input["disposeOn"] == kind;
        let kind = kind.to_owned();
        let mut yielded = false;
        Box::pin(std::future::poll_fn(move |cx| {
            if self.variant == 0 && !yielded {
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if kind == "markets"
                && self.input["deferMarkets"] == true
                && !self.markets_released.get()
            {
                return Poll::Pending;
            }
            if dispose {
                self.disposed.set(true)
            }
            if failure {
                return Poll::Ready(Err(crate::failure(
                    &format!("{kind} failure"),
                    &json!(format!("{kind} hint")),
                )));
            }
            Poll::Ready(Ok(()))
        }))
        .map_value(value)
    }
}
// A fixture future holds one native value without requiring T: Clone or Send.
trait WithValue: Future<Output = Result<(), ServiceError<String>>> + Sized {
    fn map_value<'a, T: 'a>(self, value: T) -> LifecycleFuture<'a, T, String>
    where
        Self: 'a,
    {
        Box::pin(async move {
            self.await?;
            Ok(value)
        })
    }
}
impl<F: Future<Output = Result<(), ServiceError<String>>>> WithValue for F {}
impl LifecycleHost for Life<'_> {
    type Error = String;
    fn store(&self) -> &dyn Store {
        &self.s
    }
    fn service(&self) -> &ServiceHost<'_, String> {
        &self.service
    }
    fn init_external_public_data(&self) -> LifecycleFuture<'_, bool, String> {
        self.fetched("external", self.input["external"] == true)
    }
    fn public_data_only(&self) -> bool {
        self.input["publicOnly"] == true
    }
    fn disposed(&self) -> bool {
        self.disposed.get()
    }
    fn auth_failure_streak(&self) -> f64 {
        self.input["auth"].as_f64().unwrap_or(0.0)
    }
    fn tick_interval(&self) -> Option<f64> {
        self.trace.borrow_mut().push(json!(["interval"]));
        self.input["interval"].as_f64()
    }
    fn connect_public(&self) -> Result<(), ServiceError<String>> {
        self.trace.borrow_mut().push(json!(["public"]));
        Ok(())
    }
    fn connect_private(&self) -> Result<(), ServiceError<String>> {
        self.trace.borrow_mut().push(json!(["private"]));
        Ok(())
    }
    fn fetch_markets(&self) -> LifecycleFuture<'_, Vec<Market>, String> {
        self.fetched(
            "markets",
            js_from_value(json!([corpus()["models"]["market"]])).unwrap(),
        )
    }
    fn fetch_tickers_raw(
        &self,
    ) -> LifecycleFuture<'_, Option<safe_cex_venue_blofin::tickers::TickerInput>, String> {
        self.fetched("raw", Some(js_from_value(json!(["raw"])).unwrap()))
    }
    fn format_tickers(
        &self,
        _: Option<safe_cex_venue_blofin::tickers::TickerInput>,
    ) -> Result<Vec<Ticker>, ServiceError<String>> {
        self.trace.borrow_mut().push(json!(["format"]));
        Ok(js_from_value(json!([corpus()["models"]["ticker"]])).unwrap())
    }
    fn refetch_margin_type(&self) -> LifecycleFuture<'_, (), String> {
        self.fetched("margin", ())
    }
    fn refetch_position_mode(&self) -> LifecycleFuture<'_, (), String> {
        self.fetched("mode", ())
    }
    fn fetch_balance_positions(&self) -> LifecycleFuture<'_, (Balance, Vec<Position>), String> {
        let mut p = corpus()["models"]["position"].clone();
        p["contracts"] = json!(5);
        p["leverage"] = json!(8);
        self.fetched(
            "positions",
            (
                js_from_value(corpus()["models"]["balance"].clone()).unwrap(),
                js_from_value(json!([p])).unwrap(),
            ),
        )
    }
    fn fetch_orders(&self) -> LifecycleFuture<'_, Vec<Order>, String> {
        let mut o = corpus()["models"]["order"].clone();
        o["price"] = json!(13);
        self.fetched(
            "orders",
            js_from_value(if self.input["ordersEmpty"] == true {
                json!([])
            } else {
                json!([o])
            })
            .unwrap(),
        )
    }
    fn schedule_tick(&self, next: Option<f64>, delay: f64) -> Result<(), ServiceError<String>> {
        self.trace
            .borrow_mut()
            .push(json!(["schedule", delay, next]));
        Ok(())
    }
}
fn life_cases(method: &str, floor: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == method)
        .collect();
    assert_eq!(cases.len(), floor);
    for c in &cases {
        for variant in 0..2 {
            let i = &c["input"];
            let trace = Rc::new(RefCell::new(Vec::new()));
            let t = trace.clone();
            let service = ServiceHost {
                emit_and_log_error: Box::new(move |v| {
                    t.borrow_mut().push(json!(["emit", v]));
                    Ok(())
                }),
                log: Box::new(|_, _| Ok(())),
            };
            let host = Life {
                s: store(i),
                input: i,
                trace: trace.clone(),
                disposed: Cell::new(i["disposed"] == true),
                markets_released: Cell::new(false),
                service,
                variant,
            };
            let snapshots = Rc::new(RefCell::new(Vec::new()));
            let snaps = snapshots.clone();
            let t = trace.clone();
            let unsub = Store::subscribe(
                &host.s,
                Box::new(move |d, _| {
                    snaps.borrow_mut().push(d.clone());
                    t.borrow_mut().push(json!(["store"]));
                    Ok(())
                }),
            );
            let result = ready(if method == "start" {
                Box::pin(start(&host)) as LifecycleFuture<'_, (), String>
            } else {
                Box::pin(tick(&host, i["count"].as_f64().unwrap_or(0.0)))
            });
            unsub();
            if c["result"].get("error").is_some() {
                assert_eq!(error(result.unwrap_err()), c["result"]["error"])
            } else {
                result.unwrap();
            }
            state(&snapshot(&host.s), &c["state"]);
            let source_snaps: Vec<_> = c["trace"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t[0] == "store")
                .collect();
            assert_eq!(
                snapshots.borrow().len(),
                source_snaps.len(),
                "native intermediate Store count"
            );
            for (a, e) in snapshots.borrow().iter().zip(source_snaps) {
                state(a, &e[1])
            }
            // Independently assert captured next_count, beyond source loop's delay trace.
            for t in trace.borrow().iter().filter(|t| t[0] == "schedule") {
                if method == "tick" {
                    assert_eq!(
                        t[2].as_f64(),
                        Some(i["count"].as_f64().unwrap_or(0.0) + 1.0)
                    )
                } else if i["auth"].as_f64().unwrap_or(0.0) > 0.0 {
                    assert!(t[2].is_null())
                } else {
                    assert_eq!(t[2].as_f64(), Some(1.0))
                }
            }
            let actual: Vec<_> = trace
                .borrow()
                .iter()
                .map(|t| match t[0].as_str() {
                    Some("store") => json!(["store"]),
                    Some("schedule") => json!(["schedule", t[1]]),
                    _ => t.clone(),
                })
                .collect();
            let expected: Vec<_> = c["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| {
                    if t[0] == "store" {
                        json!(["store"])
                    } else {
                        t.clone()
                    }
                })
                .collect();
            wire(&Value::Array(actual), &Value::Array(expected));
        }
    }
}
#[test]
fn native_start() {
    life_cases("start", 10)
}
#[test]
fn native_tick() {
    life_cases("tick", 11)
}

struct Disposer<'a> {
    stop: &'a str,
    trace: RefCell<Vec<String>>,
}
impl Disposer<'_> {
    fn call(&self, kind: &str) -> Result<(), String> {
        self.trace.borrow_mut().push(kind.into());
        if self.stop == kind {
            Err(format!("{kind} failed"))
        } else {
            Ok(())
        }
    }
}
impl DisposeHost for Disposer<'_> {
    type Error = String;
    fn super_dispose(&self) -> Result<(), String> {
        self.call("super")
    }
    fn public_dispose(&self) -> Result<(), String> {
        self.call("public")
    }
    fn private_dispose(&self) -> Result<(), String> {
        self.call("private")
    }
}
#[test]
fn native_dispose() {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["method"] == "dispose")
        .collect();
    assert_eq!(cases.len(), 4);
    for c in cases {
        let h = Disposer {
            stop: c["input"]["stop"].as_str().unwrap(),
            trace: RefCell::new(Vec::new()),
        };
        match dispose(&h) {
            Ok(()) => assert!(c["result"]["error"].is_null()),
            Err(e) => assert_eq!(e, c["result"]["error"]),
        }
        let expected: Vec<_> = c["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t[0].as_str().unwrap())
            .collect();
        assert_eq!(*h.trace.borrow(), expected);
    }
}

struct Constructor<'a> {
    stop: &'a str,
    trace: Vec<Value>,
    fields: Option<ConstructorFields>,
    xhr: Option<usize>,
    public: Option<usize>,
    private: Option<usize>,
}
impl Constructor<'_> {
    fn call(&mut self, k: &str) -> Result<(), String> {
        self.trace.push(json!([k]));
        if self.stop == k {
            Err(format!("{k} failed"))
        } else {
            Ok(())
        }
    }
}
impl ConstructorHost for Constructor<'_> {
    type Error = String;
    type Base = usize;
    type Xhr = usize;
    type Socket = usize;
    fn super_constructor(
        &mut self,
        name: ExchangeName,
        o: &ExchangeOptions,
        s: &dyn Store,
    ) -> Result<usize, String> {
        assert_eq!(name, ExchangeName::Blofin);
        assert_eq!(o.key, "k");
        assert_eq!(s.markets().unwrap().len(), 1);
        self.trace.push(json!(["super", "blofin"]));
        if self.stop == "super" {
            Err("super failed".into())
        } else {
            Ok(7)
        }
    }
    fn uuid(&self) -> Rc<dyn Fn() -> String> {
        Rc::new(|| "0123456789abcdef0123456789abcdef".into())
    }
    fn initialize_fields(&mut self, b: &usize, f: ConstructorFields) -> Result<(), String> {
        assert_eq!(*b, 7);
        self.fields = Some(f);
        Ok(())
    }
    fn create_xhr_with_secondary_key(
        &mut self,
        b: &usize,
        o: &ExchangeOptions,
    ) -> Result<usize, String> {
        assert_eq!(*b, 7);
        assert_eq!(o.passphrase.as_deref(), Some("p"));
        self.call("xhr")?;
        Ok(11)
    }
    fn assign_xhr(&mut self, b: &usize, x: usize) -> Result<(), String> {
        assert_eq!(*b, 7);
        self.xhr = Some(x);
        Ok(())
    }
    fn create_public_socket(&mut self, b: &usize) -> Result<usize, String> {
        assert_eq!(*b, 7);
        self.call("public")?;
        Ok(13)
    }
    fn assign_public_socket(&mut self, b: &usize, s: usize) -> Result<(), String> {
        assert_eq!(*b, 7);
        assert!(self.xhr.is_some());
        self.public = Some(s);
        Ok(())
    }
    fn create_private_socket(&mut self, b: &usize) -> Result<usize, String> {
        assert_eq!(*b, 7);
        self.call("private")?;
        Ok(17)
    }
    fn assign_private_socket(&mut self, b: &usize, s: usize) -> Result<(), String> {
        assert_eq!(*b, 7);
        assert!(self.public.is_some());
        self.private = Some(s);
        Ok(())
    }
}
#[test]
fn native_constructor() {
    for c in corpus()["constructors"].as_array().unwrap() {
        let mut h = Constructor {
            stop: c["stop"].as_str().unwrap(),
            trace: vec![],
            fields: None,
            xhr: None,
            public: None,
            private: None,
        };
        let opts: ExchangeOptions =
            js_from_value(json!({"key":"k","secret":"s","passphrase":"p"})).unwrap();
        let s = store(&Value::Null);
        let result = construct_exchange(&mut h, ExchangeName::Blofin, &opts, &s);
        if c["result"].get("error").is_some() {
            assert_eq!(result.unwrap_err(), c["result"]["error"])
        } else {
            assert_eq!(result.unwrap(), 7);
            let e = &c["result"]["ok"];
            assert_eq!(h.xhr, Some(11));
            assert_eq!(h.public, Some(13));
            assert_eq!(h.private, Some(17));
            assert_eq!(e["xhr"], "xhr");
            assert_eq!(e["public"], "public");
            assert_eq!(e["private"], "private");
            let f = h.fields.as_ref().unwrap();
            assert!(
                f.account_modes.margin_type.get().is_none()
                    && f.account_modes.position_mode.get().is_none()
            );
            assert!(e["margin"].is_null() && e["mode"].is_null());
            assert_eq!(f.client_order_id_rule.label, e["clientLabel"]);
            assert_eq!(
                (f.client_order_id_rule.mint.as_ref().unwrap())(),
                e["clientMint"]
            );
            assert!((f.client_order_id_rule.is_valid)("a1Z"));
            assert!(!(f.client_order_id_rule.is_valid)("with-hyphen"));
            assert_eq!(f.every, &e["every"]);
            assert_eq!(f.periodic, &e["periodic"]);
        }
        assert_eq!(h.trace, *c["trace"].as_array().unwrap());
    }
}

struct ApiClock {
    n: Cell<usize>,
    now: Cell<u64>,
    auth: Cell<i32>,
    auth_calls: Cell<usize>,
    remote: bool,
    events: RefCell<Vec<Value>>,
}
impl RateLimitClock for ApiClock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
impl ApiRuntime for ApiClock {
    fn timestamp(&self) -> f64 {
        1710000000123.0
    }
    fn nonce(&self) -> String {
        let n = self.n.get();
        self.n.set(n + 1);
        format!("nonce-{n}")
    }
    fn remote_signer_present(&self) -> bool {
        self.remote
    }
    fn record_auth_failure(&self) {
        self.auth_calls.set(self.auth_calls.get() + 1);
        self.auth.set(self.auth.get() + 1);
        self.events.borrow_mut().push(json!(["failure"]));
    }
    fn record_auth_success(&self) {
        self.auth_calls.set(self.auth_calls.get() + 1);
        self.auth.set(0);
        self.events.borrow_mut().push(json!(["success"]));
    }
}
struct ApiAdapter {
    failures: usize,
    elapsed: u64,
    code: Value,
    attempts: RefCell<Vec<RestRequest>>,
    events: Rc<ApiClock>,
}
impl RestAdapter for ApiAdapter {
    fn execute(&self, r: RestRequest) -> RestFuture<'_, RestResponse> {
        let index = self.attempts.borrow().len();
        self.events.now.set(self.events.now.get() + self.elapsed);
        self.attempts.borrow_mut().push(r.clone());
        self.events.events.borrow_mut().push(json!([
            "attempt",
            r.method,
            r.url,
            r.headers,
            r.timeout_ms
        ]));
        Box::pin(async move {
            if index < self.failures {
                Err(RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "network".into(),
                        Some("ECONNRESET".into()),
                    )),
                    r,
                ))
            } else {
                Ok(response(json!({"code":self.code}), 1))
            }
        })
    }
}
#[test]
fn native_create_api() {
    for (case_index, c) in corpus()["api"].as_array().unwrap().iter().enumerate() {
        let i = &c["input"];
        let mut opts = i["options"].clone();
        let remote = opts
            .as_object_mut()
            .unwrap()
            .remove("remoteSigner")
            .is_some();
        let mut opts: ExchangeOptions = js_from_value(opts).unwrap();
        let runtime = Rc::new(ApiClock {
            n: Cell::new(0),
            now: Cell::new(1710000000123),
            auth: Cell::new(0),
            auth_calls: Cell::new(0),
            remote,
            events: RefCell::new(Vec::new()),
        });
        let xhr = Rc::new(ApiAdapter {
            failures: i["retry"]["failures"].as_u64().unwrap_or(0) as usize,
            elapsed: i["retry"]["elapsed"].as_u64().unwrap_or(0),
            code: i["retry"]
                .get("code")
                .cloned()
                .unwrap_or_else(|| json!("0")),
            attempts: RefCell::new(Vec::new()),
            events: runtime.clone(),
        });
        let defaults = RefCell::new(None);
        let api = create_api(&opts, runtime.clone(), |o, base, timeout| {
            assert_eq!(o.key, opts.key);
            *defaults.borrow_mut() = Some((base.to_owned(), timeout));
            Ok(xhr.clone())
        });
        if let Err(e) = api {
            assert_eq!(e, c["result"]["error"]);
            assert!(defaults.borrow().is_none());
            continue;
        }
        let api = api.unwrap();
        if i.get("mutate").is_some() {
            opts.key = "changed".into();
            opts.secret = "changed".into();
            opts.passphrase = Some("changed".into());
        }
        let d = defaults.borrow();
        assert_eq!(d.as_ref().unwrap().0, c["defaults"]["baseURL"]);
        assert_eq!(d.as_ref().unwrap().1, 20000);
        drop(d);
        if !i["retry"].is_null() {
            let r = RestRequest {
                method: i["retry"]["method"].as_str().unwrap().into(),
                timeout_ms: i["retry"]["timeout"].as_i64(),
                url: "/account".into(),
                headers: if i["retry"]["method"] == "post" {
                    serde_json::from_value(json!({"Accept":"application/json, text/plain, */*","Content-Type":"application/x-www-form-urlencoded"})).unwrap()
                } else {
                    serde_json::from_value(json!({"Accept":"application/json, text/plain, */*"}))
                        .unwrap()
                },
                ..Default::default()
            };
            let result = ready(api.execute(r));
            if c["result"].get("error").is_some() {
                assert_eq!(result.unwrap_err().message, c["result"]["error"])
            } else {
                assert_eq!(result.unwrap().status, 200)
            }
            assert_eq!(
                xhr.attempts.borrow().len(),
                if i["retry"]["method"] == "post" {
                    1
                } else if i["retry"]["elapsed"] == 7000 {
                    match i["retry"]["timeout"].as_i64() {
                        None => 3,
                        Some(0) => 4,
                        Some(5000) => 1,
                        _ => panic!("independent timeout fixture floor"),
                    }
                } else {
                    4
                }
            );
            for r in xhr.attempts.borrow().iter() {
                assert!(
                    r.headers.contains_key("ACCESS-SIGN"),
                    "native retry signed headers"
                );
            }
            assert_eq!(
                runtime.auth.get(),
                c["auth"].as_i64().unwrap() as i32,
                "native auth state"
            );
            assert_eq!(
                runtime.n.get(),
                c["nonceCalls"].as_u64().unwrap() as usize,
                "native nonce count"
            );
            assert_eq!(
                runtime.auth_calls.get(),
                if c["result"].get("error").is_some() {
                    0
                } else {
                    4
                },
                "native retry auth callback count"
            );
            if i["retry"]["elapsed"] == 7000 {
                let expected: Vec<i64> = match i["retry"]["timeout"].as_i64() {
                    None => vec![20000, 13000, 6000],
                    Some(0) => vec![0, 0, 0, 0],
                    Some(5000) => vec![5000],
                    _ => unreachable!(),
                };
                for (r, t) in xhr.attempts.borrow().iter().zip(expected) {
                    assert_eq!(r.timeout_ms, Some(t), "native remaining retry budget");
                }
            }
            let actual = runtime.events.borrow().clone();
            wire(&Value::Array(actual), &c["trace"]);
        } else {
            let config = &i["config"];
            let r = RestRequest {
                method: config
                    .get("method")
                    .and_then(Value::as_str)
                    .unwrap_or("get")
                    .into(),
                url: config
                    .get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("/account")
                    .into(),
                params: config.get("params").cloned(),
                body: config.get("data").cloned().map(|value| {
                    RestBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(value)
                            .unwrap(),
                    )
                }),
                headers: config
                    .get("headers")
                    .map(|v| serde_json::from_value(v.clone()).unwrap())
                    .unwrap_or_default(),
                ..Default::default()
            };
            let signed = api.intercept_request(r).unwrap();
            let e = if i.get("mutate").is_some() {
                assert_eq!(c["result"]["ok"]["headers"]["ACCESS-KEY"], "changed");
                &corpus()["api"][2]["result"]["ok"]
            } else {
                &c["result"]["ok"]
            };
            assert_eq!(signed.url, e["url"]);
            assert_eq!(signed.method, e["method"]);
            if e["params"].get("undefined") == Some(&Value::Bool(true)) {
                assert!(signed.params.is_none())
            } else {
                plain(signed.params.as_ref().unwrap(), &e["params"])
            }
            if e["data"].get("undefined") == Some(&Value::Bool(true)) {
                assert!(signed.body.is_none())
            } else {
                plain(
                    &signed
                        .body
                        .as_ref()
                        .unwrap()
                        .untransformed()
                        .unwrap()
                        .clone()
                        .into_value()
                        .unwrap(),
                    &e["data"],
                )
            }
            assert_eq!(
                signed.headers.len(),
                e["headers"].as_object().unwrap().len()
            );
            for (k, e) in e["headers"].as_object().unwrap() {
                assert_eq!(
                    signed.headers.get(k).unwrap(),
                    e.as_str().unwrap(),
                    "native signed header {k} case{case_index}"
                )
            }
            let code = i.get("code").cloned().unwrap_or_else(|| json!("0"));
            api.intercept_response(&response(json!({"code":code}), 1), &signed)
                .unwrap();
            assert_eq!(
                runtime.auth.get(),
                c["auth"].as_i64().unwrap() as i32,
                "native auth state"
            );
            assert_eq!(
                runtime.n.get(),
                c["nonceCalls"].as_u64().unwrap() as usize,
                "native nonce count"
            );
            assert_eq!(*runtime.events.borrow(), *c["trace"].as_array().unwrap());
        }
    }
}

#[test]
fn native_rest_boundaries() {
    assert_eq!(corpus()["boundaries"].as_array().unwrap().len(), 6);
    let models = &corpus()["models"];
    let s = store(&Value::Null);
    let logs = RefCell::new(Vec::new());
    let h = ServiceHost {
        emit_and_log_error: Box::new(|v| {
            logs.borrow_mut().push(v.cloned());
            Ok::<_, String>(())
        }),
        log: Box::new(|_, _| Ok(())),
    };
    for (index, c) in corpus()["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .take(3)
        .enumerate()
    {
        let mut i = c["input"].clone();
        if index < 2 {
            let mut row = json!({"instType":"SWAP","contractType":"linear","instId":"BTC-USDT","quoteCurrency":"USDT","minSize":"2","lotSize":"3","contractValue":"0.01"});
            if index == 0 {
                row["baseCurrency"] = json!("BTC");
                row["contractValue"] = json!("bad");
            }
            i = json!({"body":{"data":[row]}});
        }
        let adapter = ReadAdapter {
            input: &i,
            requests: RefCell::new(Vec::new()),
            variant: 1,
        };
        if index < 2 {
            let actual = ready(fetch_markets(&s, &adapter, &h, || "cache-id".into())).unwrap();
            assert_eq!(actual.len(), 1);
            assert_eq!(actual[0].id, models["market"]["id"]);
            if index == 0 {
                // The source parseFloat("bad") result is a usable native NaN.
                assert!(actual[0].contract_size.unwrap().is_nan());
                let info = actual[0].info.as_ref().unwrap();
                assert!(
                    info.get("contractValue")
                        .unwrap()
                        .as_f64()
                        .unwrap()
                        .is_nan()
                );
                assert!(
                    info.get("stored").is_none(),
                    "fresh row, no cached fallback"
                );
                assert!(
                    logs.borrow().is_empty(),
                    "no error for retained native info"
                );
            } else {
                assert!(
                    actual[0]
                        .info
                        .as_ref()
                        .unwrap()
                        .get("stored")
                        .unwrap()
                        .as_bool()
                        .unwrap()
                );
            }
        } else {
            let opts: OHLCVOptions =
                serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","to":9e15}))
                    .unwrap();
            let actual = ready(fetch_ohlcv(&opts, &s, &adapter, &h, || 1710000000123.0)).unwrap();
            assert!(actual.is_empty());
            assert!(adapter.requests.borrow().is_empty());
        }
    }
    assert_eq!(
        logs.borrow().len(),
        2,
        "strict base field and invalid time report; native NaN info remains usable"
    );
    let sort_input = json!({"body":{"data":[["3000","1","2","0","1","x","y","3"],["2000","1","2","0","1","x","y","3"],["bad","1","2","0","1","x","y","3"],["1000","1","2","0","1","x","y","3"]]}});
    let adapter = ReadAdapter {
        input: &sort_input,
        requests: RefCell::new(Vec::new()),
        variant: 1,
    };
    let opts = OHLCVOptions {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
        to: None,
        from: None,
        limit: None,
    };
    let sorted = ready(fetch_ohlcv(&opts, &s, &adapter, &h, || 1710000000123.0)).unwrap();
    assert_eq!(sorted.len(), 4);
    for (a, t) in sorted.iter().zip([2.0, 3.0, f64::NAN, 1.0]) {
        if t.is_nan() {
            assert!(a.timestamp.is_nan())
        } else {
            assert_eq!(a.timestamp.to_bits(), t.to_bits())
        }
        assert_eq!(a.volume.to_bits(), 3.0f64.to_bits());
    }
    num(
        1.0,
        &corpus()["boundaries"][3]["result"]["ok"][0]["timestamp"],
    );
    let dst = &corpus()["boundaries"][4];
    let i = json!({"body":{"data":[]}});
    let adapter = ReadAdapter {
        input: &i,
        requests: RefCell::new(Vec::new()),
        variant: 1,
    };
    let opts = OHLCVOptions {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::D1),
        to: Some(1710129600000.0),
        from: None,
        limit: Some(1.0),
    };
    assert!(
        ready(fetch_ohlcv(&opts, &s, &adapter, &h, || 1710000000123.0))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        adapter.requests.borrow()[0].params.as_ref().unwrap()["before"].as_f64(),
        Some(1710043200000.0),
        "native fixed-day24h profile"
    );
    let source_get = dst["trace"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t[0] == "get")
        .unwrap();
    num(1710046800000.0, &source_get[2]["before"]);
    let c = &corpus()["boundaries"][5];
    let i = c["input"].clone();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let service = ServiceHost {
        emit_and_log_error: Box::new(|_| Ok::<_, String>(())),
        log: Box::new(|_, _| Ok(())),
    };
    let host = Life {
        s: store(&i),
        input: &i,
        trace,
        disposed: Cell::new(false),
        markets_released: Cell::new(false),
        service,
        variant: 1,
    };
    assert_eq!(error(ready(start(&host)).unwrap_err()), "raw failure");
    host.markets_released.set(true);
    assert!(
        !host.s.loaded().unwrap().markets,
        "native pending peer dropped after Promise.all rejection"
    );
    assert_eq!(
        c["state"]["loaded"]["markets"], true,
        "source independent promise continued after peer rejection"
    );
}

#[test]
fn audit_validation_interleaves_sum_coercion() {
    let cases = corpus()["audit"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "BLOFIN-AUDIT validation independent count");
    let case = &cases[0];
    assert_eq!(
        case["input"],
        json!({"body":{"code":"0","data":[{"available":"1","frozen":{"toString":null}},null]}}),
        "BLOFIN-AUDIT validation frozen input"
    );
    assert_eq!(
        case["result"]["ok"]["error"], "Cannot read properties of null (reading 'available')",
        "BLOFIN-AUDIT validation source precedence"
    );
    let logs = RefCell::new(Vec::new());
    let host = ServiceHost {
        emit_and_log_error: Box::new(|value| {
            logs.borrow_mut().push(value.cloned());
            Ok::<_, String>(())
        }),
        log: Box::new(|_, _| Ok(())),
    };
    let adapter = ReadAdapter {
        input: &case["input"],
        requests: RefCell::new(Vec::new()),
        variant: 0,
    };
    let actual = ready(validate_account(&adapter, &host)).unwrap();
    assert_eq!(
        actual.error,
        json!("Cannot convert object to primitive value"),
        "BLOFIN-AUDIT validation native precedence"
    );
    assert!(
        actual.balance.is_none(),
        "BLOFIN-AUDIT validation native no balance"
    );
    assert_eq!(
        *logs.borrow(),
        vec![Some(json!("Cannot convert object to primitive value"))],
        "BLOFIN-AUDIT validation native report"
    );
}

#[path = "support/retained_market_info.rs"]
mod retained_market_info;
