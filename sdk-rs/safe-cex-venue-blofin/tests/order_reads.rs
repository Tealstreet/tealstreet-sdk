#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{
    BothPositionSide, Execution, ExecutionPositionSide, ExecutionsOptions, HistoricalOrder, Order,
    OrderHistoryOptions, OrderSide, OrderStatus, OrderTimeInForce, OrderType, PositionSide,
};
use safe_cex_venue_blofin::{
    order_reads::*,
    order_services::{ServiceError, ServiceHost},
    utility::UtilityInput,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/order_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual_source_oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            39,
            "independent_case_floor"
        );
        assert_eq!(v["stoppedAfter"], 39);
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            v => v,
        };
        assert_eq!(v["header"]["architecture"], arch);
        eprintln!("order-read source header: {}", v["header"]);
        v
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..1000 {
        if let Poll::Ready(v) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return v;
        }
    }
    panic!("pending fixture iteration floor1000 exhausted")
}
fn error(e: ServiceError<String>) -> String {
    match e {
        ServiceError::Input(v) | ServiceError::Host(v) => v,
        ServiceError::Rest(v) => v.message,
        ServiceError::Ingress(v) => v.to_string(),
        ServiceError::Coercion(v) => v.message.into(),
        ServiceError::Store(v) => v.to_string(),
    }
}
fn cleaned(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| v.get("__undefined") != Some(&Value::Bool(true)))
                .map(|(k, v)| (k.clone(), cleaned(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(cleaned).collect()),
        v => v.clone(),
    }
}
fn input(case: &Value) -> Value {
    cleaned(&serde_json::from_str::<Value>(case["inputRaw"].as_str().unwrap()).unwrap())
}
fn store(input: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let market=input.get("market").cloned().unwrap_or_else(||json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.25},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":50}},"contractSize":0.01,"info":{"contractValue":"0.01"}}));
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(js_from_value(json!([market])).unwrap()),
            orders: Some(
                js_from_value(input.get("stored").cloned().unwrap_or_else(|| json!([]))).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
struct Adapter<'a> {
    queues: RefCell<BTreeMap<String, VecDeque<Value>>>,
    trace: &'a RefCell<Vec<Value>>,
    variant: u8,
}
impl RestAdapter for Adapter<'_> {
    fn execute(&self, req: RestRequest) -> RestFuture<'_, RestResponse> {
        assert_eq!(req.method, "GET", "native read method");
        assert!(req.body.is_none(), "native no write body");
        assert!(
            req.params.as_ref().unwrap().is_object(),
            "native own query record"
        );
        self.trace
            .borrow_mut()
            .push(json!(["get", req.url, req.params]));
        let value = self
            .queues
            .borrow_mut()
            .get_mut(&req.url)
            .expect("route exists")
            .pop_front()
            .expect("independent route queue");
        let mut yielded = false;
        Box::pin(std::future::poll_fn(move |cx| {
            if self.variant < 3 && !yielded {
                yielded = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            let value = value.clone();
            if let Some(message) = value.get("throw").and_then(Value::as_str) {
                let mut e = RestFailure::pre_transmit(message.into(), req.clone());
                if value.get("hint").is_some() || value.get("messageHint").is_some() {
                    let body = if let Some(v) = value.get("hint") {
                        json!({"msg":v})
                    } else {
                        json!({"message":value["messageHint"]})
                    };
                    e.response = Some(Box::new(RestResponse {
                        status: 503,
                        reason: None,
                        headers: Default::default(),
                        body: RestResponseBody::Untransformed(
                            safe_cex_types::retained_json::RetainedJsonInput::from_value(body)
                                .unwrap(),
                        ),
                    }));
                }
                return Poll::Ready(Err(e));
            }
            Poll::Ready(Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: match self.variant % 3 {
                    0 => RestResponseBody::Encoded(serde_json::to_vec(&value).unwrap()),
                    1 => RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(value)
                            .unwrap(),
                    ),
                    _ => RestResponseBody::Transformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(value)
                            .unwrap(),
                    ),
                },
            }))
        }))
    }
}
fn side(v: &Value) -> OrderSide {
    match v.as_str().unwrap() {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => panic!("source side floor"),
    }
}
fn kind(v: &Value) -> OrderType {
    match v.as_str().unwrap() {
        "limit" => OrderType::Limit,
        "market" => OrderType::Market,
        "stop_market" => OrderType::StopLoss,
        "take_profit_market" => OrderType::TakeProfit,
        "conditional" => OrderType::Conditional,
        _ => panic!("source type floor"),
    }
}
fn status(v: &Value) -> OrderStatus {
    match v.as_str().unwrap() {
        "open" => OrderStatus::Open,
        "closed" => OrderStatus::Closed,
        "canceled" => OrderStatus::Canceled,
        _ => panic!("source status floor"),
    }
}
fn position(v: Option<&Value>) -> Option<ExecutionPositionSide> {
    v.filter(|v| v.get("__undefined").is_none())
        .map(|v| match v.as_str().unwrap() {
            "long" => ExecutionPositionSide::Side(PositionSide::Long),
            "short" => ExecutionPositionSide::Side(PositionSide::Short),
            "both" => ExecutionPositionSide::Both(BothPositionSide::Both),
            _ => panic!("source position floor"),
        })
}
fn number(v: Option<&Value>) -> Option<f64> {
    v.filter(|v| v.get("__undefined").is_none())
        .map(|v| v.as_f64().unwrap())
}
fn text(v: Option<&Value>) -> Option<&str> {
    v.filter(|v| v.get("__undefined").is_none())
        .map(|v| v.as_str().unwrap())
}
fn native_orders(actual: &[Order], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native order count"
    );
    for (o, e) in actual.iter().zip(expected) {
        assert_eq!(o.id, e["id"].as_str().unwrap(), "native order id");
        assert_eq!(
            o.symbol,
            e["symbol"].as_str().unwrap(),
            "native order symbol"
        );
        assert_eq!(o.r#type, kind(&e["type"]), "native order type");
        assert_eq!(o.status, status(&e["status"]), "native order status");
        assert_eq!(o.side, side(&e["side"]), "native order side");
        assert_eq!(o.price, e["price"].as_f64().unwrap(), "native order price");
        assert_eq!(
            o.amount,
            e["amount"].as_f64().unwrap(),
            "native order amount"
        );
        assert_eq!(
            o.filled,
            e["filled"].as_f64().unwrap(),
            "native order filled"
        );
        assert_eq!(
            o.remaining,
            e["remaining"].as_f64().unwrap(),
            "native order remaining"
        );
        assert_eq!(o.reduce_only, e["reduceOnly"].as_bool().unwrap());
        assert_eq!(
            o.last_update,
            e["lastUpdate"].as_f64().unwrap(),
            "native order clock"
        );
        assert_eq!(
            o.extra.as_ref().and_then(|v| v.remaining_native),
            number(e.get("extra").and_then(|v| v.get("remainingNative"))),
            "native remaining quantity"
        );
        assert_eq!(
            o.time_in_force,
            e.get("timeInForce").map(|v| match v.as_str().unwrap() {
                "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
                "PostOnly" => OrderTimeInForce::PostOnly,
                _ => panic!("source tif floor"),
            })
        );
        match (o.info.as_ref(), e.get("info")) {
            (Some(a), Some(e)) => retained_assert::assert_value(a, e),
            (None, None) => (),
            _ => panic!("native retained info presence"),
        };
    }
}
fn native_history(actual: &[HistoricalOrder], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(actual.len(), expected.len(), "native history count");
    for (o, e) in actual.iter().zip(expected) {
        assert_eq!(o.id, e["id"].as_str().unwrap());
        assert_eq!(o.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(o.side, side(&e["side"]));
        assert_eq!(o.r#type, kind(&e["type"]));
        assert_eq!(o.status, status(&e["status"]));
        assert_eq!(
            o.created_time,
            e["createdTime"].as_f64().unwrap(),
            "native history created time"
        );
        assert_eq!(
            o.updated_time,
            number(e.get("updatedTime")),
            "native history updated time"
        );
        assert_eq!(o.price, number(e.get("price")), "native history price");
        assert_eq!(
            o.average_price,
            number(e.get("averagePrice")),
            "native history average price"
        );
        assert_eq!(
            o.amount,
            e["amount"].as_f64().unwrap(),
            "native history amount"
        );
        assert_eq!(
            o.filled,
            e["filled"].as_f64().unwrap(),
            "native history filled"
        );
        assert_eq!(o.position_side, position(e.get("positionSide")));
        assert_eq!(o.reduce_only, e.get("reduceOnly").and_then(Value::as_bool));
        assert!(
            o.fees.is_none()
                && o.realized_pnl.is_none()
                && o.fee_asset.is_none()
                && o.rebate.is_none()
                && o.rebate_asset.is_none()
        );
    }
}
fn native_fills(actual: &[Execution], expected: &Value) {
    let expected = expected.as_array().unwrap();
    assert_eq!(
        actual.len(),
        expected.len(),
        "independent native execution count"
    );
    for (o, e) in actual.iter().zip(expected) {
        assert_eq!(o.id.as_deref(), text(e.get("id")), "native per-fill id");
        assert_eq!(
            o.order_id.as_deref(),
            text(e.get("orderId")),
            "native execution order id"
        );
        assert_eq!(
            o.timestamp,
            e["timestamp"].as_f64().unwrap(),
            "native execution timestamp"
        );
        assert_eq!(o.symbol, e["symbol"].as_str().unwrap());
        assert_eq!(
            o.price,
            e["price"].as_f64().unwrap(),
            "native execution price"
        );
        assert_eq!(o.qty, e["qty"].as_f64().unwrap(), "native execution qty");
        assert_eq!(o.side, side(&e["side"]));
        assert_eq!(
            o.commission,
            number(e.get("commission")),
            "native execution commission"
        );
        assert_eq!(
            o.commission_asset.as_deref(),
            text(e.get("commissionAsset"))
        );
        assert_eq!(o.position_side, position(e.get("positionSide")));
        assert_eq!(
            o.realized_pnl,
            number(e.get("realizedPnl")),
            "native execution realized pnl"
        );
        assert!(
            o.maker.is_none()
                && o.venue_reported_revenue.is_none()
                && o.venue_reported_revenue_asset.is_none()
        );
    }
}
fn execution_opts(v: &Value) -> ExecutionsOptions {
    ExecutionsOptions {
        symbol: v.get("symbol").and_then(Value::as_str).map(str::to_owned),
        order_id: v.get("orderId").and_then(Value::as_str).map(str::to_owned),
        from: v.get("from").and_then(Value::as_f64),
        to: v.get("to").and_then(Value::as_f64),
        limit: v.get("limit").and_then(Value::as_f64),
    }
}
fn history_opts(v: &Value) -> OrderHistoryOptions {
    OrderHistoryOptions {
        symbol: v.get("symbol").and_then(Value::as_str).map(str::to_owned),
        order_id: v.get("orderId").and_then(Value::as_str).map(str::to_owned),
        from: v.get("from").and_then(Value::as_f64),
        to: v.get("to").and_then(Value::as_f64),
        limit: v.get("limit").and_then(Value::as_f64),
        status: None,
    }
}
enum Output {
    Orders(Vec<Order>),
    Rows(Vec<UtilityInput>),
    History(Vec<HistoricalOrder>),
    Fills(Vec<Execution>),
}
fn wire_values(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(e)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            e.as_f64().unwrap().to_bits(),
            "wire numeric bits"
        ),
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "wire independent count");
            for (a, e) in a.iter().zip(e) {
                wire_values(a, e)
            }
        }
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "wire own property count");
            for (k, e) in e {
                wire_values(a.get(k).expect("wire field present"), e)
            }
        }
        _ => assert_eq!(actual, expected, "wire field value"),
    }
}
fn run(case: &Value, variant: u8) {
    let input = input(case);
    let store = store(&input);
    let trace = RefCell::new(Vec::new());
    let clock = Cell::new(0);
    let now = || {
        clock.set(clock.get() + 1);
        let value = 1000.0 + clock.get() as f64;
        trace.borrow_mut().push(json!(["clock", value]));
        value
    };
    let service = ServiceHost {
        emit_and_log_error: Box::new(|v| {
            trace.borrow_mut().push(json!([
                "emit",
                v.cloned().unwrap_or_else(|| json!({"__undefined":true}))
            ]));
            Ok(())
        }),
        log: Box::new(|text, severity| {
            trace
                .borrow_mut()
                .push(json!(["log", text, severity.as_str()]));
            Ok(())
        }),
    };
    let host = OrderReadHost {
        service: &service,
        now: &now,
        sleep: Box::new(|ms| {
            trace.borrow_mut().push(json!(["sleep", ms]));
            Box::pin(std::future::ready(Ok(())))
        }),
    };
    let adapter = Adapter {
        queues: RefCell::new(
            input["queues"]
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .map(|(k, v)| (k.clone(), v.as_array().unwrap().iter().cloned().collect()))
                .collect(),
        ),
        trace: &trace,
        variant,
    };
    let args = &input["args"];
    let name = case["name"].as_str().unwrap();
    let families = || -> [safe_cex_venue_blofin::rest_lifecycle::LifecycleFuture<'_, Vec<Order>, String>; 3] {
        [Box::pin(fetch_normal_orders(&store, &adapter, &host)),
         Box::pin(fetch_algo_orders(&store, &adapter, &host)),
         Box::pin(fetch_trigger_orders(&store, &adapter, &host))]
    };
    let result = match name {
        "fetchNormalOrders" => {
            ready(fetch_normal_orders(&store, &adapter, &host)).map(Output::Orders)
        }
        "fetchAlgoOrders" => ready(fetch_algo_orders(&store, &adapter, &host)).map(Output::Orders),
        "fetchTriggerOrders" => {
            ready(fetch_trigger_orders(&store, &adapter, &host)).map(Output::Orders)
        }
        "fetchOrders" => {
            let symbols: Vec<String> = args[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect();
            ready(fetch_orders(Some(&symbols), &store, &host, &families)).map(Output::Orders)
        }
        "fetchOrdersWithRetry" => ready(fetch_orders_with_retry(
            args[0].as_f64().unwrap(),
            &store,
            &host,
            &families,
        ))
        .map(Output::Orders),
        "fetchPaginatedOrderRows" => ready(fetch_paginated_order_rows(
            args[0].as_str().unwrap(),
            args[1].as_str().unwrap(),
            args[2].as_str().unwrap(),
            args[3].as_object().unwrap(),
            &adapter,
            &service,
        ))
        .map(Output::Rows),
        "fetchHistoricalOrders" => ready(fetch_historical_orders(
            Some(&history_opts(&args[0])),
            input["publicOnly"] == true,
            &store,
            &adapter,
            &service,
        ))
        .map(Output::History),
        "fetchHistoricalFills" => ready(fetch_historical_fills(
            &execution_opts(&args[0]),
            input["publicOnly"] == true,
            &store,
            &adapter,
            &service,
        ))
        .map(Output::Fills),
        _ => panic!("outside family"),
    };
    if let Some(expected) = case["result"].get("error") {
        assert_eq!(
            result.map_err(error).err().unwrap(),
            expected.as_str().unwrap(),
            "native error {name}"
        );
    } else {
        match result.map_err(error).unwrap() {
            Output::Orders(o) => native_orders(&o, &case["result"]["value"]),
            Output::History(o) => native_history(&o, &case["result"]["value"]),
            Output::Fills(o) => native_fills(&o, &case["result"]["value"]),
            Output::Rows(o) => {
                let expected = case["result"]["value"].as_array().unwrap();
                assert_eq!(o.len(), expected.len(), "native paginated row count");
                for (a, e) in o.iter().zip(expected) {
                    assert_eq!(a.json(), e, "native retained row");
                }
            }
        }
    }
    native_orders(&store.orders().unwrap(), &case["stored"]);
    // Request/clock/log records are compared only AFTER every native output/store assertion.
    wire_values(&Value::Array(trace.borrow().clone()), &case["trace"]);
    assert!(
        adapter.queues.borrow().values().all(VecDeque::is_empty),
        "independent route queues fully used"
    );
}
fn family(name: &str, floor: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["name"] == name)
        .collect();
    assert_eq!(cases.len(), floor, "independent family floor");
    for case in cases {
        for variant in 0..6 {
            run(case, variant);
        }
    }
}
#[test]
fn native_fetch_normal_orders() {
    family("fetchNormalOrders", 2)
}
#[test]
fn native_fetch_algo_orders() {
    family("fetchAlgoOrders", 2)
}
#[test]
fn native_fetch_trigger_orders() {
    family("fetchTriggerOrders", 2)
}
#[test]
fn native_fetch_paginated_order_rows() {
    family("fetchPaginatedOrderRows", 8)
}
#[test]
fn native_fetch_orders() {
    family("fetchOrders", 1)
}
#[test]
fn native_fetch_orders_with_retry() {
    family("fetchOrdersWithRetry", 4)
}
#[test]
fn native_fetch_historical_orders() {
    family("fetchHistoricalOrders", 6)
}
#[test]
fn native_fetch_historical_fills() {
    family("fetchHistoricalFills", 14)
}
#[test]
fn native_fill_boundaries() {
    for index in 0..3 {
        let case = &corpus()["boundaries"][index];
        let input = input(case);
        let store = store(&input);
        let trace = RefCell::new(Vec::new());
        let service = ServiceHost {
            emit_and_log_error: Box::new(|v| {
                trace.borrow_mut().push(v.cloned().unwrap());
                Ok(())
            }),
            log: Box::new(|_, _| Ok(())),
        };
        let adapter = Adapter {
            queues: RefCell::new(
                input["queues"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.as_array().unwrap().iter().cloned().collect()))
                    .collect(),
            ),
            trace: &trace,
            variant: 0,
        };
        let mut opts = execution_opts(&input["args"][0]);
        if index == 0 {
            opts.limit = Some(f64::NAN);
            assert_eq!(
                error(
                    ready(fetch_historical_fills(
                        &opts, false, &store, &adapter, &service
                    ))
                    .unwrap_err()
                ),
                "Nonfinite fill query limit is outside native JSON"
            );
            assert!(trace.borrow().is_empty());
        } else if index == 2 {
            assert_eq!(
                error(
                    ready(fetch_historical_fills(
                        &opts, false, &store, &adapter, &service
                    ))
                    .unwrap_err()
                ),
                "Nonfinite fill query time is outside native JSON"
            );
            assert!(trace.borrow().is_empty());
        } else {
            let output = ready(fetch_historical_fills(
                &opts, false, &store, &adapter, &service,
            ))
            .map_err(error)
            .unwrap();
            assert!(
                output.is_empty(),
                "native currency refusal produces no output"
            );
            assert_eq!(
                trace.borrow().last().unwrap().as_str(),
                Some("Fill commission asset is outside the native string model")
            );
        }
    }
}
