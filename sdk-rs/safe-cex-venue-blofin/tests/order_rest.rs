#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{FetchedLeverage, Order, PreparedOrderOpts as PlaceOrderOpts};
use safe_cex_venue_blofin::{
    modes::AccountModes,
    order_formatters::OrderPayload,
    order_rest::*,
    order_services::{ServiceError, ServiceHost},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
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
                "/tests/order_rest_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual_source_oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            data["cases"].as_array().unwrap().len(),
            53,
            "independent_case_floor"
        );
        assert_eq!(data["stoppedAfter"], 53);
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            a => a,
        };
        assert_eq!(data["header"]["architecture"], arch);
        eprintln!("order REST oracle header: {}", data["header"]);
        data
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready fixture"),
    }
}
fn error(error: ServiceError<String>) -> String {
    match error {
        ServiceError::Host(v) | ServiceError::Input(v) => v,
        ServiceError::Rest(v) => v.message,
        ServiceError::Ingress(v) => v.to_string(),
        ServiceError::Coercion(v) => v.message.into(),
        ServiceError::Store(v) => v.to_string(),
    }
}
fn is_absent(value: Option<&Value>) -> bool {
    value.is_none_or(|v| v.get("__undefined") == Some(&Value::Bool(true)))
}
fn numeric(actual: Option<f64>, raw: Option<&Value>, field: &str) {
    if is_absent(raw) {
        assert!(actual.is_none(), "native absent {field}")
    } else {
        assert_eq!(actual, raw.and_then(Value::as_f64), "native number {field}");
    }
}
fn native_opts(opts: &PlaceOrderOpts, raw: &Value) {
    assert_eq!(
        opts.symbol,
        raw["symbol"].as_str().unwrap(),
        "native opts symbol"
    );
    // Enum equality is performed against an independently decoded source literal.
    assert_eq!(
        opts.r#type,
        js_from_value::<safe_cex_types::OrderType>(raw["type"].clone()).unwrap(),
        "native opts type"
    );
    assert_eq!(
        opts.side,
        js_from_value::<safe_cex_types::OrderSide>(raw["side"].clone()).unwrap(),
        "native opts side"
    );
    assert_eq!(
        opts.amount,
        raw["amount"].as_f64().unwrap(),
        "native opts amount"
    );
    numeric(opts.price, raw.get("price"), "price");
    numeric(opts.trigger_price, raw.get("triggerPrice"), "triggerPrice");
    numeric(opts.stop_loss, raw.get("stopLoss"), "stopLoss");
    numeric(opts.take_profit, raw.get("takeProfit"), "takeProfit");
    if !is_absent(raw.get("reduceOnly")) {
        assert_eq!(
            opts.reduce_only,
            raw["reduceOnly"].as_bool(),
            "native opts reduceOnly"
        );
    } else {
        assert!(opts.reduce_only.is_none());
    }
    // Compare closed native enum values, never serialize them to judge parity.
    assert_eq!(
        opts.time_in_force,
        raw.get("timeInForce")
            .filter(|v| !is_absent(Some(v)))
            .map(|v| js_from_value(v.clone()).unwrap()),
        "native timeInForce"
    );
    assert_eq!(
        opts.sl_trigger_type,
        raw.get("slTriggerType")
            .filter(|v| !is_absent(Some(v)))
            .map(|v| js_from_value(v.clone()).unwrap()),
        "native slTriggerType"
    );
    assert_eq!(
        opts.tp_trigger_type,
        raw.get("tpTriggerType")
            .filter(|v| !is_absent(Some(v)))
            .map(|v| js_from_value(v.clone()).unwrap()),
        "native tpTriggerType"
    );
}
fn field_map(native: &serde_json::Map<String, Value>, raw: &Value) {
    let expected = raw.as_object().unwrap();
    assert_eq!(native.len(), expected.len(), "native map field presence");
    for (key, value) in expected {
        let actual = &native[key];
        match value {
            Value::String(v) => assert_eq!(actual.as_str(), Some(v.as_str()), "native map {key}"),
            Value::Bool(v) => assert_eq!(actual.as_bool(), Some(*v), "native map {key}"),
            _ => assert_eq!(actual, value, "native map {key}"),
        }
    }
}
struct Capture<'a> {
    source: &'a Value,
    trace: RefCell<Vec<Value>>,
}
impl Capture<'_> {
    fn next(&self) -> Value {
        self.source["trace"][self.trace.borrow().len()].clone()
    }
    fn event(&self, event: Value) {
        self.trace.borrow_mut().push(event);
    }
    fn opts(
        &self,
        kind: &str,
        opts: &PlaceOrderOpts,
        source: Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) {
        let expected = self.next();
        assert_eq!(expected[0], kind, "native callback order");
        native_opts(opts, &expected[1]);
        if kind == "trigger" {
            match (source, expected.get(2).filter(|v| !v.is_null())) {
                (Some(a), Some(e)) => retained_assert::assert_value(a, e),
                (None, None) => (),
                _ => panic!("native trigger provenance presence"),
            }
        }
        // Only record a source trace AFTER all native argument assertions.
        self.event(expected);
    }
}
struct Xhr<'a> {
    responses: RefCell<VecDeque<Value>>,
    capture: &'a Capture<'a>,
    variant: u8,
}
impl RestAdapter for Xhr<'_> {
    fn execute(&self, req: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert_eq!(req.method, "POST", "native method");
            assert!(req.params.is_none(), "native absent params");
            let expected = self.capture.next();
            assert_eq!(expected[0], "request", "native request ordering");
            assert_eq!(req.url, expected[1].as_str().unwrap(), "native endpoint");
            let Some(RestBody::Untransformed(payload)) = req.body.clone() else {
                panic!("native body provenance")
            };
            let payload = payload.into_value().unwrap();
            if let Some(native) = payload.as_array() {
                let raw = expected[2].as_array().unwrap();
                assert_eq!(native.len(), raw.len(), "independent native batch size");
                for (a, e) in native.iter().zip(raw) {
                    field_map(a.as_object().unwrap(), e);
                }
            } else {
                field_map(payload.as_object().unwrap(), &expected[2]);
            }
            self.capture.event(json!(["request", req.url, payload]));
            let value = self
                .responses
                .borrow_mut()
                .pop_front()
                .expect("independent response queue");
            if let Some(message) = value.get("throw").and_then(Value::as_str) {
                let mut failure = RestFailure::pre_transmit(message.into(), req.clone());
                if let Some(hint) = value.get("hint") {
                    failure.response = Some(Box::new(RestResponse {
                        status: 503,
                        reason: None,
                        headers: Default::default(),
                        body: RestResponseBody::Untransformed(
                            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                                json!({"msg":hint}),
                            )
                            .unwrap(),
                        ),
                    }));
                }
                return Err(failure);
            }
            Ok(RestResponse {
                body: match self.variant {
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
                status: 200,
                reason: None,
                headers: Default::default(),
            })
        })
    }
}
fn store(input: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let market = json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.25},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":2,"max":50}},"contractSize":0.001,"info":{"lotSize":"1"}});
    let mut fetched = serde_json::Map::new();
    fetched.insert("KEEP".into(), json!({"buyLeverage":3,"sellLeverage":5}));
    if let Some(value) = input.get("leverage") {
        fetched.insert("BTCUSDT".into(), value.clone());
    }
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(js_from_value(json!([market])).unwrap()),
            orders: Some(
                js_from_value(input.get("orders").cloned().unwrap_or_else(|| json!([]))).unwrap(),
            ),
            positions: Some(
                js_from_value(input.get("positions").cloned().unwrap_or_else(|| json!([])))
                    .unwrap(),
            ),
            fetched_leverage: Some(
                fetched
                    .into_iter()
                    .map(|(key, value)| {
                        (
                            key,
                            FetchedLeverage {
                                buy_leverage: value["buyLeverage"].as_f64().unwrap(),
                                sell_leverage: value["sellLeverage"].as_f64().unwrap(),
                                margin_type: None,
                                position_mode: None,
                                is_fetching: value.get("isFetching").and_then(Value::as_bool),
                            },
                        )
                    })
                    .collect(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
fn host<'a>(
    capture: &'a Capture<'a>,
    input: &'a Value,
    service: &'a ServiceHost<'a, String>,
) -> WriteHost<'a, String> {
    let payload = |size: &str| OrderPayload {
        fields: json!({"instId":"BTC-USDT","size":size,"price":"101.5"})
            .as_object()
            .unwrap()
            .clone(),
    };
    WriteHost {
        service,
        format_normal_order: Box::new(move |opts| {
            capture.opts("normal", &opts, None);
            Box::pin(std::future::ready(Ok(vec![payload(
                input["normalSize"].as_str().unwrap_or("12"),
            )])))
        }),
        format_algo_order: Box::new(move |opts| {
            capture.opts("algo", &opts, None);
            Box::pin(std::future::ready(Ok(payload("12"))))
        }),
        format_trigger_order: Box::new(move |opts, source| {
            capture.opts("trigger", &opts, source.as_ref());
            Box::pin(std::future::ready(Ok(payload("12"))))
        }),
        place_order: Box::new(move |opts| {
            capture.opts("place", &opts, None);
            Box::pin(std::future::ready(Ok(vec![Some(json!("replacement"))])))
        }),
        get_margin_type: Box::new(move |symbol, refetch| {
            assert_eq!(
                symbol.as_deref(),
                capture.next()[1].as_str(),
                "native margin symbol"
            );
            assert_eq!(
                refetch,
                capture.next()[2].as_bool().unwrap(),
                "native margin refetch"
            );
            capture.event(json!(["margin", symbol, refetch]));
            Box::pin(std::future::ready(Ok(js_from_value(
                input
                    .get("margin")
                    .cloned()
                    .unwrap_or_else(|| json!("cross")),
            )
            .unwrap())))
        }),
        get_position_mode: Box::new(move || {
            capture.event(json!(["mode"]));
            Box::pin(std::future::ready(Ok(js_from_value(
                input
                    .get("mode")
                    .cloned()
                    .unwrap_or_else(|| json!("oneway")),
            )
            .unwrap())))
        }),
        fetch_positions: Box::new(move |symbols| {
            assert_eq!(
                symbols,
                capture.next()[1]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "native fetched symbol arguments"
            );
            capture.event(json!(["fetchPositions", symbols]));
            Box::pin(std::future::ready(Ok(js_from_value(
                input
                    .get("fetchedPositions")
                    .cloned()
                    .unwrap_or_else(|| json!([])),
            )
            .unwrap())))
        }),
        sleep: Box::new(move |ms| {
            assert_eq!(
                ms,
                capture.next()[1].as_u64().unwrap(),
                "native scheduler milliseconds"
            );
            capture.event(json!(["sleep", ms]));
            Box::pin(std::future::ready(Ok(())))
        }),
        resolve_caller_client_order_id: Box::new(move |opts| {
            capture.event(json!(["guard", opts.client_order_id]));
            if input["guardThrow"] == true {
                Err(ServiceError::Host("caller ID rejection".into()))
            } else {
                Ok(())
            }
        }),
        assert_caller_client_order_ids: Box::new(move |opts| {
            capture.event(json!(["batchGuard", opts.len()]));
            if input["guardThrow"] == true {
                Err(ServiceError::Host("caller ID rejection".into()))
            } else {
                Ok(())
            }
        }),
    }
}
fn native_store(store: &DefaultStore, case: &Value) {
    let positions = store.positions();
    let expected = case["positions"].as_array().unwrap();
    assert_eq!(positions.len(), expected.len(), "native position count");
    for (p, e) in positions.iter().zip(expected) {
        assert_eq!(p.id, e["id"].as_str().unwrap(), "native position id");
        assert_eq!(
            p.leverage,
            e["leverage"].as_f64().unwrap(),
            "native position leverage"
        );
        assert_eq!(
            p.contracts,
            e["contracts"].as_f64().unwrap(),
            "native contracts"
        );
        assert_eq!(
            p.entry_price,
            e["entryPrice"].as_f64().unwrap(),
            "native entryPrice"
        );
        assert_eq!(
            p.margin_type,
            e.get("marginType")
                .map(|v| js_from_value(v.clone()).unwrap()),
            "native margin type"
        );
    }
    let fetched = store.fetched_leverage();
    let expected = case["leverage"].as_object().unwrap();
    assert_eq!(
        fetched.len(),
        expected.len(),
        "native leverage count: {fetched:?} expected {expected:?}"
    );
    for (key, e) in expected {
        let actual = &fetched[key];
        assert_eq!(
            actual.buy_leverage,
            e["buyLeverage"].as_f64().unwrap(),
            "native buy leverage"
        );
        assert_eq!(
            actual.sell_leverage,
            e["sellLeverage"].as_f64().unwrap(),
            "native sell leverage"
        );
        assert_eq!(
            actual.margin_type,
            e.get("marginType")
                .map(|v| js_from_value(v.clone()).unwrap()),
            "native marginType"
        );
        assert_eq!(
            actual.position_mode,
            e.get("positionMode")
                .map(|v| js_from_value(v.clone()).unwrap()),
            "native positionMode"
        );
        assert_eq!(
            actual.is_fetching,
            e.get("isFetching").and_then(Value::as_bool),
            "native isFetching"
        );
    }
    let orders = store.orders();
    let expected = case["orders"].as_array().unwrap();
    assert_eq!(orders.len(), expected.len(), "native remaining order count");
    for (o, e) in orders.iter().zip(expected) {
        assert_eq!(
            o.id,
            e["id"].as_str().unwrap(),
            "native reconciled order ID"
        );
        assert_eq!(
            o.price,
            e["price"].as_f64().unwrap(),
            "native remaining price"
        );
        assert_eq!(
            o.amount,
            e["amount"].as_f64().unwrap(),
            "native remaining amount"
        );
    }
}
fn run(case: &Value, variant: u8) {
    let input: Value = serde_json::from_str(case["inputRaw"].as_str().unwrap()).unwrap();
    let store = store(&input);
    let capture = Capture {
        source: case,
        trace: RefCell::new(Vec::new()),
    };
    let xhr = Xhr {
        responses: RefCell::new(
            input
                .get("responses")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into(),
        ),
        capture: &capture,
        variant,
    };
    let service = ServiceHost {
        emit_and_log_error: Box::new(|v| {
            capture.event(json!([
                "emit",
                v.cloned().unwrap_or_else(|| json!({"__undefined":true}))
            ]));
            Ok(())
        }),
        log: Box::new(|_, _: LogSeverity| panic!("no source log callback")),
    };
    let host = host(&capture, &input, &service);
    let args = &input["args"];
    let name = case["name"].as_str().unwrap();
    let result: Result<Vec<Option<Value>>, ServiceError<String>> = match name {
        "placeOrderBatch" | "placeAlgoOrderBatch" => {
            let payloads = args[0]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| OrderPayload {
                    fields: v.as_object().unwrap().clone(),
                })
                .collect::<Vec<_>>();
            if name == "placeOrderBatch" {
                ready(place_order_batch(&payloads, &xhr, &service))
                    .map(|ids| ids.into_iter().map(|id| Some(json!(id))).collect())
            } else {
                ready(place_algo_order_batch(&payloads, &xhr, &service))
            }
        }
        "placeOrder" => ready(place_order(
            &js_from_value(args[0].clone()).unwrap(),
            &xhr,
            &host,
        )),
        "placeOrders" => ready(place_orders(
            &js_from_value::<Vec<PlaceOrderOpts>>(args[0].clone()).unwrap(),
            &xhr,
            &host,
        )),
        "cancelNormalOrders" | "cancelAlgoOrders" | "cancelTriggerOrders" | "cancelOrders" => {
            let orders: Vec<Order> = js_from_value(args[0].clone()).unwrap();
            match name {
                "cancelNormalOrders" => ready(cancel_normal_orders(&orders, &store, &xhr, &host)),
                "cancelAlgoOrders" => ready(cancel_algo_orders(&orders, &store, &xhr, &host)),
                "cancelTriggerOrders" => ready(cancel_trigger_orders(&orders, &store, &xhr, &host)),
                _ => ready(cancel_orders(&orders, &store, &xhr, &host)),
            }
            .map(|()| Vec::new())
        }
        "changeMarginType" => ready(change_margin_type(
            args[0].as_bool().unwrap(),
            args[1].as_str(),
            &AccountModes::default(),
            &store,
            &xhr,
            &host,
        ))
        .map(|()| Vec::new()),
        "setLeverage" => ready(set_leverage(
            args[0].as_str().unwrap(),
            args[1].as_f64().unwrap(),
            &store,
            &xhr,
            &host,
        ))
        .map(|()| Vec::new()),
        "updateOrder" => ready(update_order(
            &js_from_value(args[0]["order"].clone()).unwrap(),
            &js_from_value(args[0]["update"].clone()).unwrap(),
            &store,
            &xhr,
            &host,
        )),
        _ => panic!("unowned method"),
    };
    if let Some(expected) = case["result"].get("error") {
        assert_eq!(
            result.map_err(error).unwrap_err(),
            expected.as_str().unwrap(),
            "native thrown error {name}"
        );
    } else {
        let actual = result.map_err(error).unwrap();
        if let Some(expected) = case["result"]["value"].as_array() {
            assert_eq!(actual.len(), expected.len(), "independent native ID count");
            for (a, e) in actual.iter().zip(expected) {
                if is_absent(Some(e)) {
                    assert!(a.is_none(), "native undefined placement ID");
                } else {
                    assert_eq!(a.as_ref(), Some(e), "native placement ID");
                }
            }
        } else {
            assert!(actual.is_empty(), "native void result");
        }
    }
    native_store(&store, case);
    assert_eq!(
        *capture.trace.borrow(),
        case["trace"].as_array().unwrap().clone(),
        "wire trace AFTER native assertions {name}"
    );
    assert!(
        xhr.responses.borrow().is_empty(),
        "independent queue fully used {name}"
    );
}
fn family(name: &str, floor: usize) {
    let selected: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["name"] == name)
        .collect();
    assert_eq!(selected.len(), floor, "independent family floor {name}");
    for case in selected {
        for variant in 0..3 {
            run(case, variant);
        }
    }
}
#[test]
fn native_place_order_batch() {
    family("placeOrderBatch", 6)
}
#[test]
fn native_place_algo_order_batch() {
    family("placeAlgoOrderBatch", 4)
}
#[test]
fn native_place_order() {
    family("placeOrder", 5)
}
#[test]
fn native_place_orders() {
    family("placeOrders", 4)
}
#[test]
fn native_cancel_normal_orders() {
    family("cancelNormalOrders", 6)
}
#[test]
fn native_cancel_algo_orders() {
    family("cancelAlgoOrders", 4)
}
#[test]
fn native_cancel_trigger_orders() {
    family("cancelTriggerOrders", 4)
}
#[test]
fn native_cancel_orders() {
    family("cancelOrders", 4)
}
#[test]
fn native_change_margin_type() {
    family("changeMarginType", 4)
}
#[test]
fn native_set_leverage() {
    family("setLeverage", 7)
}
#[test]
fn native_update_order() {
    family("updateOrder", 5)
}
#[test]
fn native_null_update_boundary() {
    let case = &corpus()["boundaries"][0];
    let input: Value = serde_json::from_str(case["inputRaw"].as_str().unwrap()).unwrap();
    let store = store(&input);
    let capture = Capture {
        source: case,
        trace: RefCell::new(Vec::new()),
    };
    let xhr = Xhr {
        responses: RefCell::new(VecDeque::new()),
        capture: &capture,
        variant: 0,
    };
    let service = ServiceHost {
        emit_and_log_error: Box::new(|_| panic!("boundary before emit")),
        log: Box::new(|_, _| panic!("boundary before log")),
    };
    let host = host(&capture, &input, &service);
    let order = js_from_value(input["args"][0]["order"].clone()).unwrap();
    let update = js_from_value(input["args"][0]["update"].clone()).unwrap();
    assert_eq!(
        error(ready(update_order(&order, &update, &store, &xhr, &host)).unwrap_err()),
        "Update price must be a native number"
    );
    assert!(capture.trace.borrow().is_empty());
}
struct HoldingAdapter {
    starts: RefCell<Vec<String>>,
    release: std::cell::Cell<bool>,
    reject_long: bool,
}
impl RestAdapter for HoldingAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        assert_eq!(request.method, "POST");
        assert_eq!(request.url, "/api/v1/account/set-leverage");
        let Some(RestBody::Untransformed(payload)) = request.body.as_ref() else {
            panic!("native parallel payload origin")
        };
        let payload = payload.clone().into_value().unwrap();
        assert_eq!(payload["leverage"].as_str(), Some("9"));
        assert_eq!(payload["marginMode"].as_str(), Some("isolated"));
        let side = payload["positionSide"].as_str().unwrap().to_owned();
        self.starts.borrow_mut().push(side.clone());
        Box::pin(std::future::poll_fn(move |_| {
            if self.reject_long && side == "long" {
                return Poll::Ready(Err(RestFailure::pre_transmit(
                    "early transport rejection".into(),
                    request.clone(),
                )));
            }
            if !self.release.get() {
                return Poll::Pending;
            }
            Poll::Ready(Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        if side == "long" {
                            json!({"code":"0"})
                        } else {
                            json!({"code":"5","msg":"short denied"})
                        },
                    )
                    .unwrap(),
                ),
            }))
        }))
    }
}
#[test]
fn native_parallel_leverage_writes_start_before_settlement() {
    assert_eq!(
        corpus()["parallelWitness"]["starts"],
        json!(["long", "short"])
    );
    assert_eq!(
        corpus()["parallelWitness"]["immediateStarts"],
        json!(["long", "short"])
    );
    let case = &corpus()["cases"][41];
    let input: Value = serde_json::from_str(case["inputRaw"].as_str().unwrap()).unwrap();
    for reject_long in [false, true] {
        let store = store(&input);
        let capture = Capture {
            source: case,
            trace: RefCell::new(Vec::new()),
        };
        let service = ServiceHost {
            emit_and_log_error: Box::new(|_| Ok(())),
            log: Box::new(|_, _| panic!("no log")),
        };
        let host = host(&capture, &input, &service);
        let xhr = HoldingAdapter {
            starts: RefCell::new(Vec::new()),
            release: std::cell::Cell::new(false),
            reject_long,
        };
        let future = set_leverage("BTCUSDT", 9.0, &store, &xhr, &host);
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut cx);
        assert_eq!(
            *xhr.starts.borrow(),
            ["long", "short"],
            "native parallel independent two writes before settlement"
        );
        assert_eq!(
            store.positions()[0].leverage,
            3.0,
            "native no optimistic mutation before response"
        );
        assert_eq!(
            store.fetched_leverage().len(),
            1,
            "native no cache mutation before response"
        );
        if reject_long {
            assert_eq!(
                error(match first {
                    Poll::Ready(result) => result.unwrap_err(),
                    Poll::Pending => panic!("ready rejection"),
                }),
                "early transport rejection"
            );
        } else {
            assert!(matches!(first, Poll::Pending));
            xhr.release.set(true);
            assert_eq!(
                error(match future.as_mut().poll(&mut cx) {
                    Poll::Ready(result) => result.unwrap_err(),
                    Poll::Pending => panic!("released responses"),
                }),
                "short denied"
            );
        }
        assert_eq!(
            store.positions()[0].leverage,
            3.0,
            "native failed response keeps leverage"
        );
        assert_eq!(
            store.fetched_leverage().len(),
            1,
            "native failed response keeps cache"
        );
    }
}
#[test]
fn native_signed_zero_leverage_clamp() {
    let witnesses = corpus()["zeroWitnesses"].as_array().unwrap();
    assert_eq!(witnesses.len(), 4, "independent four zero witnesses");
    for (index, witness) in witnesses.iter().enumerate() {
        let zero = |v: &Value| {
            if v.as_str() == Some("-0") {
                -0.0_f64
            } else {
                0.0_f64
            }
        };
        let input: Value =
            serde_json::from_str(corpus()["cases"][40]["inputRaw"].as_str().unwrap()).unwrap();
        let store = store(&input);
        let mut market = store.markets()[0].clone();
        market.limits.leverage.min = zero(&witness["min"]);
        market.limits.leverage.max = zero(&witness["max"]);
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let capture = Capture {
            source: &corpus()["cases"][40],
            trace: RefCell::new(Vec::new()),
        };
        let service = ServiceHost {
            emit_and_log_error: Box::new(|_| panic!("no zero error")),
            log: Box::new(|_, _| panic!("no zero log")),
        };
        let host = host(&capture, &input, &service);
        // Separate direct-native REST recorder; source witness independently pins the wire literal to "0".
        struct ZeroAdapter {
            requests: RefCell<Vec<RestRequest>>,
        }
        impl RestAdapter for ZeroAdapter {
            fn execute(&self, req: RestRequest) -> RestFuture<'_, RestResponse> {
                self.requests.borrow_mut().push(req);
                Box::pin(std::future::ready(Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            json!({"code":"0"}),
                        )
                        .unwrap(),
                    ),
                })))
            }
        }
        let xhr = ZeroAdapter {
            requests: RefCell::new(Vec::new()),
        };
        ready(set_leverage(
            "BTCUSDT",
            zero(&witness["input"]),
            &store,
            &xhr,
            &host,
        ))
        .map_err(error)
        .unwrap();
        let leverage = store.fetched_leverage();
        let expected = if [false, true, true, false][index] {
            (-0.0_f64).to_bits()
        } else {
            0.0_f64.to_bits()
        };
        assert_eq!(
            leverage["BTCUSDT"].buy_leverage.to_bits(),
            expected,
            "native signed-zero buy clamp bits"
        );
        assert_eq!(
            leverage["BTCUSDT"].sell_leverage.to_bits(),
            expected,
            "native signed-zero sell clamp bits"
        );
        let requests = xhr.requests.borrow();
        assert_eq!(requests.len(), 1, "independent one zero write");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].url, "/api/v1/account/set-leverage");
        let Some(RestBody::Untransformed(body)) = requests[0].body.as_ref() else {
            panic!("native zero origin")
        };
        assert_eq!(
            body.clone().into_value().unwrap()["leverage"].as_str(),
            Some("0"),
            "wire zero AFTER cached native bits"
        );
    }
}
