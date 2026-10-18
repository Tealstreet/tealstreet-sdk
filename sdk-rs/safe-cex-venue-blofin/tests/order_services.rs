#[path = "support/retained_assert.rs"]
mod retained_assert;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{FetchedLeverage, MarginType, Order, PlaceOrderOpts, PositionMode};
use safe_cex_venue_blofin::{
    modes::AccountModes, order_formatters::*, order_services::*, utility::UtilityInput,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/order_services_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual_typescript_order_services: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["cases"].as_array().unwrap().len(),
            63,
            "independent_total_floor"
        );
        assert_eq!(value["allocationWitness"], 10001);
        let arch = match std::env::consts::ARCH {
            "aarch64" => "arm64",
            "x86_64" => "x64",
            other => other,
        };
        assert_eq!(value["header"]["architecture"], arch);
        eprintln!("order/services header: {}", value["header"]);
        value
    })
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture"),
    }
}
fn input(case: &Value) -> Value {
    let raw = case["inputRaw"]
        .as_str()
        .unwrap()
        .replace(r#"{"__nativeNumber":"Infinity"}"#, "1e400")
        .replace(r#"{"__nativeNumber":"-Infinity"}"#, "-1e400");
    serde_json::from_str(&raw).unwrap()
}
fn default_market() -> Value {
    json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.25},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":50}},"contractSize":0.001,"info":{"lotSize":"3","minSize":"0.2"}})
}
fn store(input: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let markets = vec![input.get("market").cloned().unwrap_or_else(default_market)];
    let mut leverage = StoreUpdate::default().fetched_leverage.unwrap_or_default();
    leverage.insert(
        "KEEP".into(),
        FetchedLeverage {
            buy_leverage: 3.0,
            sell_leverage: 5.0,
            margin_type: None,
            position_mode: Some(PositionMode::OneWay),
            is_fetching: None,
        },
    );
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(js_from_value(Value::Array(markets)).unwrap()),
            positions: Some(
                js_from_value(input.get("positions").cloned().unwrap_or_else(|| json!([])))
                    .unwrap(),
            ),
            orders: Some(
                js_from_value(input.get("orders").cloned().unwrap_or_else(|| json!([]))).unwrap(),
            ),
            fetched_leverage: Some(leverage),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
fn mode(value: &Value) -> Option<PositionMode> {
    value
        .as_str()
        .map(|_| js_from_value(value.clone()).unwrap())
}
fn margin(value: &Value) -> Option<MarginType> {
    value
        .as_str()
        .map(|_| js_from_value(value.clone()).unwrap())
}
fn error(error: ServiceError<String>) -> String {
    match error {
        ServiceError::Host(value) | ServiceError::Input(value) => value,
        ServiceError::Rest(value) => value.message,
        ServiceError::Ingress(value) => value.to_string(),
        ServiceError::Coercion(value) => value.message.into(),
        ServiceError::Store(value) => value.to_string(),
    }
}
fn assert_native_record(native: &serde_json::Map<String, Value>, expected: &Value, label: &str) {
    let expected = expected.as_object().unwrap();
    assert_eq!(
        native.len(),
        expected.len(),
        "{label} native_field_presence"
    );
    for (key, expected) in expected {
        let actual = native
            .get(key)
            .unwrap_or_else(|| panic!("{label} native field missing {key}"));
        match expected {
            Value::String(text) => {
                assert_eq!(actual.as_str(), Some(text.as_str()), "{label} native {key}")
            }
            Value::Bool(value) => {
                assert_eq!(actual.as_bool(), Some(*value), "{label} native bool {key}")
            }
            Value::Array(values) => {
                let actual = actual.as_array().unwrap();
                assert_eq!(actual.len(), values.len(), "{label} native array {key}");
                for (a, e) in actual.iter().zip(values) {
                    assert_native_record(a.as_object().unwrap(), e, label);
                }
            }
            _ => assert_eq!(actual, expected, "{label} native {key}"),
        }
    }
}
fn assert_forwarded_order(native: &Order, raw: &Value) {
    assert_eq!(
        native.id,
        raw["id"].as_str().unwrap(),
        "native_forwarded_order_id"
    );
    assert_eq!(
        native.symbol,
        raw["symbol"].as_str().unwrap(),
        "native_forwarded_order_symbol"
    );
    assert_eq!(
        native.price,
        raw["price"].as_f64().unwrap(),
        "native_forwarded_order_price"
    );
    assert_eq!(
        native.amount,
        raw["amount"].as_f64().unwrap(),
        "native_forwarded_order_amount"
    );
    assert_eq!(native.filled, 0.0);
    assert_eq!(native.remaining, raw["amount"].as_f64().unwrap());
    assert_eq!(native.reduce_only, false);
    assert_eq!(native.last_update, 123.0);
    retained_assert::assert_value(
        native.info.as_ref().unwrap().get("witness").unwrap(),
        &raw["id"],
    );
}
fn run_formatter(case: &Value) {
    let input = input(case);
    let store = store(&input);
    let opts: PlaceOrderOpts = js_from_value(input["opts"].clone()).unwrap();
    let source: Option<UtilityInput> = input
        .get("source")
        .map(|value| js_from_value(value.clone()).unwrap());
    let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
    let uuid_count = Cell::new(0);
    let margin_trace = trace.clone();
    let mode_trace = trace.clone();
    let uuid_trace = trace.clone();
    let client_trace = trace.clone();
    let split_trace = trace.clone();
    let cancel_trace = trace.clone();
    let host = FormatHost {
        get_margin_type: Box::new(|symbol| {
            margin_trace.borrow_mut().push(json!(["margin", symbol]));
            Box::pin(std::future::ready(Ok(
                margin(&input["margin"]).unwrap_or(MarginType::Cross)
            )))
        }),
        get_position_mode: Box::new(|symbol| {
            mode_trace.borrow_mut().push(json!(["mode", symbol]));
            Box::pin(std::future::ready(Ok(
                mode(&input["mode"]).or(Some(PositionMode::Hedged))
            )))
        }),
        uuid: Box::new(|| {
            uuid_count.set(uuid_count.get() + 1);
            let id = format!("u{:03}", uuid_count.get());
            uuid_trace.borrow_mut().push(json!(["uuid", id]));
            Ok(id)
        }),
        resolve_caller_client_order_id: Box::new(|opts| {
            client_trace
                .borrow_mut()
                .push(json!(["client", opts.client_order_id]));
            Ok(opts.client_order_id.clone())
        }),
        assert_single_native_client_order_id: Box::new(|id, count| {
            split_trace.borrow_mut().push(json!(["split", id, count]));
            if id.is_some() && count != 1 {
                Err(ServiceError::Host("caller ID split probe".into()))
            } else {
                Ok(())
            }
        }),
        cancel_algo_orders: Box::new(|orders| {
            for native in &orders {
                let raw = input["orders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|raw| raw["id"] == native.id)
                    .unwrap();
                assert_forwarded_order(native, raw);
            }
            cancel_trace.borrow_mut().push(json!([
                "cancelAlgo",
                orders.iter().map(|o| &o.id).collect::<Vec<_>>()
            ]));
            let ids = orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>();
            let result = Store::remove_orders(&store, &ids).map_err(ServiceError::Store);
            Box::pin(std::future::ready(result))
        }),
        broker_id_enabled: input["production"] != false || input["proxy"] == true,
    };
    let name = case["name"].as_str().unwrap();
    let result = match name {
        "formatTriggerOrder" => ready(format_trigger_order(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
            source
                .as_ref()
                .map(|s: &UtilityInput| s.json().clone().into())
                .as_ref(),
            &store,
            &host,
        ))
        .map(|value| vec![value]),
        "formatAlgoOrder" => ready(format_algo_order(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
            &store,
            &host,
        ))
        .map(|value| vec![value]),
        "formatNormalOrder" => ready(format_normal_order(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
            &store,
            &host,
        )),
        _ => panic!("unknown formatter"),
    };
    let expected = &case["result"];
    match result {
        Ok(payloads) => {
            assert!(
                expected.get("error").is_none(),
                "native_{name}_expected_error {case}"
            );
            let rows = if name == "formatNormalOrder" {
                expected["value"].as_array().unwrap().clone()
            } else {
                vec![expected["value"].clone()]
            };
            // Source case count is secondary to fixed independent splitting anchors.
            if name == "formatNormalOrder" {
                match opts.amount {
                    310.0 => assert_eq!(payloads.len(), 5, "native_split_310_floor"),
                    300.0 => assert_eq!(payloads.len(), 3, "native_split_300_floor"),
                    _ => {}
                }
            }
            assert_eq!(payloads.len(), rows.len(), "native_{name}_payload_count");
            for (native, row) in payloads.iter().zip(&rows) {
                assert_native_record(&native.fields, row, name);
            }
        }
        Err(err) => assert_eq!(
            error(err),
            expected["error"].as_str().unwrap(),
            "native_{name}_error"
        ),
    }
    assert_eq!(
        *trace.borrow(),
        case["trace"].as_array().unwrap().clone(),
        "native_{name}_call_order"
    );
}
struct Adapter {
    queue: RefCell<VecDeque<Value>>,
    trace: Rc<RefCell<Vec<Value>>>,
    path: u8,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let payload = match &request.body {
            Some(RestBody::Untransformed(value)) => value.clone().into_value().unwrap(),
            None => Value::Null,
            _ => panic!("native request origin"),
        };
        self.trace.borrow_mut().push(json!([
            "request",
            request.method,
            request.url,
            payload,
            request.params
        ]));
        let row = self
            .queue
            .borrow_mut()
            .pop_front()
            .expect("independent_response_queue");
        let result = if let Some(message) = row["throw"].as_str() {
            let mut err = RestFailure::pre_transmit(message.into(), request);
            if let Some(message) = row.get("responseMsg") {
                err.response = Some(Box::new(RestResponse {
                    status: 503,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            json!({"msg":message}),
                        )
                        .unwrap(),
                    ),
                }));
            }
            if let Some(text) = row["responseBodyText"].as_str() {
                err.response = Some(Box::new(RestResponse {
                    status: 503,
                    reason: None,
                    headers: Default::default(),
                    body: match self.path {
                        0 => RestResponseBody::Encoded(text.as_bytes().to_vec()),
                        1 => RestResponseBody::Untransformed(
                            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                                Value::String(text.into()),
                            )
                            .unwrap(),
                        ),
                        _ => RestResponseBody::Transformed(
                            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                                Value::String(text.into()),
                            )
                            .unwrap(),
                        ),
                    },
                }));
            }
            Err(err)
        } else {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: match self.path {
                    0 => RestResponseBody::Encoded(serde_json::to_vec(&row).unwrap()),
                    1 => RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(row).unwrap(),
                    ),
                    _ => RestResponseBody::Transformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(row).unwrap(),
                    ),
                },
            })
        };
        Box::pin(std::future::ready(result))
    }
}
fn assert_leverage(native: &FetchedLeverage, expected: &Value, label: &str) {
    assert_eq!(
        native.buy_leverage,
        expected["buyLeverage"].as_f64().unwrap(),
        "{label}_native_buy_leverage"
    );
    assert_eq!(
        native.sell_leverage,
        expected["sellLeverage"].as_f64().unwrap(),
        "{label}_native_sell_leverage"
    );
    assert_eq!(
        native.position_mode,
        mode(&expected["positionMode"]),
        "{label}_native_position_mode"
    );
    assert_eq!(
        native.margin_type,
        margin(&expected["marginType"]),
        "{label}_native_margin_type"
    );
    assert_eq!(native.is_fetching, None);
}
fn run_service(case: &Value, path: u8) {
    let input = input(case);
    let store = store(&input);
    let modes = AccountModes::default();
    modes.position_mode.set(mode(&input["mode"]));
    modes.margin_type.set(margin(&input["margin"]));
    let trace = Rc::new(RefCell::new(Vec::<Value>::new()));
    let emit_trace = trace.clone();
    let log_trace = trace.clone();
    let host = ServiceHost {
        emit_and_log_error: Box::new(|value| {
            emit_trace.borrow_mut().push(json!([
                "emit",
                value.cloned().unwrap_or_else(|| json!({"undefined":true}))
            ]));
            Ok(())
        }),
        log: Box::new(|text, severity: LogSeverity| {
            log_trace
                .borrow_mut()
                .push(json!(["log", text, severity.as_str()]));
            Ok(())
        }),
    };
    let xhr = Adapter {
        queue: RefCell::new(input["responses"].as_array().unwrap().clone().into()),
        trace: trace.clone(),
        path,
    };
    let expected = &case["result"];
    let args = &input["args"];
    let symbol = args[0].as_str();
    let refetch = args[1].as_bool().unwrap_or(false);
    let name = case["name"].as_str().unwrap();
    let failure: Result<(), String> = match name {
        "getOrFetchPositionMode" => ready(get_or_fetch_position_mode(
            symbol, refetch, &modes, &store, &xhr, &host,
        ))
        .map_err(error)
        .map(|value| {
            assert_eq!(
                value,
                js_from_value::<safe_cex_types::PositionMode>(expected["value"].clone()).unwrap(),
                "native_position_mode_result"
            );
        }),
        "getOrFetchMarginType" => ready(get_or_fetch_margin_type(
            symbol, refetch, &modes, &store, &xhr, &host,
        ))
        .map_err(error)
        .map(|value| {
            assert_eq!(
                value,
                js_from_value::<safe_cex_types::MarginType>(expected["value"].clone()).unwrap(),
                "native_margin_type_result"
            );
        }),
        "fetchLeverageForSymbol" => ready(fetch_leverage_for_symbol(
            symbol.unwrap(),
            &modes,
            &store,
            &xhr,
            &host,
        ))
        .map_err(error)
        .map(|value| {
            if let Some(value) = value {
                assert_leverage(&value, &expected["value"], name);
            } else {
                assert!(expected["value"].is_null(), "native_no_leverage");
            }
        }),
        "changePositionMode" => ready(change_position_mode(
            args[0].as_bool().unwrap(),
            &modes,
            &store,
            &xhr,
            &host,
        ))
        .map_err(error),
        "placeTriggerOrder" => {
            let payload = OrderPayload {
                fields: args[0].as_object().unwrap().clone(),
            };
            ready(place_trigger_order(&payload, &xhr, &host))
                .map_err(error)
                .map(|value| {
                    assert_eq!(
                        value,
                        expected["value"].as_str().unwrap(),
                        "native_trigger_id"
                    );
                })
        }
        "cancelSymbolOrders" => ready(cancel_symbol_orders(
            symbol.unwrap(),
            &store,
            |orders: Vec<Order>| {
                let expected = input["orders"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|raw| raw["symbol"] == symbol.unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    orders.len(),
                    expected.len(),
                    "native_cancel_symbol_argument_count"
                );
                for (native, raw) in orders.iter().zip(expected) {
                    assert_forwarded_order(native, raw);
                }
                trace.borrow_mut().push(json!([
                    "cancel",
                    orders.iter().map(|o| &o.id).collect::<Vec<_>>()
                ]));
                std::future::ready(if let Some(message) = input["cancelThrow"].as_str() {
                    Err(message.to_owned())
                } else {
                    Ok(())
                })
            },
        )),
        _ => panic!("unknown service"),
    };
    match failure {
        Ok(()) => assert!(
            expected.get("error").is_none(),
            "native_{name}_expected_failure {case}"
        ),
        Err(message) => {
            assert!(
                expected.get("error").is_some(),
                "native_{name}_unexpected_error {message}"
            );
            assert_eq!(
                message,
                expected["error"].as_str().unwrap(),
                "native_{name}_error"
            );
        }
    }
    assert_eq!(
        modes.position_mode.get(),
        mode(&case["cache"]["mode"]),
        "native_{name}_position_cache"
    );
    assert_eq!(
        modes.margin_type.get(),
        margin(&case["cache"]["margin"]),
        "native_{name}_margin_cache"
    );
    let positions = Store::positions(&store);
    let expected_positions = case["store"]["positions"].as_array().unwrap();
    assert_eq!(
        positions.len(),
        expected_positions.len(),
        "native_{name}_position_count"
    );
    for (native, expected) in positions.iter().zip(expected_positions) {
        assert_eq!(native.id, expected["id"].as_str().unwrap());
        assert_eq!(native.entry_price, expected["entryPrice"].as_f64().unwrap());
        assert_eq!(native.position_mode, mode(&expected["positionMode"]));
        assert_eq!(native.margin_type, margin(&expected["marginType"]));
        assert_eq!(native.contracts, expected["contracts"].as_f64().unwrap());
    }
    let leverage = Store::fetched_leverage(&store);
    let expected_leverage = case["store"]["leverage"].as_object().unwrap();
    assert_eq!(leverage.len(), expected_leverage.len());
    for (symbol, value) in leverage.iter() {
        assert_leverage(value, &expected_leverage[symbol], name);
    }
    // Native outputs/caches have all been asserted before inspecting request/diagnostic records.
    assert_eq!(
        *trace.borrow(),
        case["trace"].as_array().unwrap().clone(),
        "native_{name}_call_order_path{path}"
    );
    assert!(xhr.queue.borrow().is_empty());
}
fn suite(name: &str, count: usize) {
    let cases = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["name"] == name)
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), count, "independent_{name}_case_floor");
    for case in cases {
        if name.starts_with("format") {
            run_formatter(case);
        } else {
            for path in 0..3 {
                run_service(case, path);
            }
        }
    }
}
#[test]
fn native_format_trigger_order() {
    suite("formatTriggerOrder", 11)
}
#[test]
fn native_format_normal_order() {
    suite("formatNormalOrder", 12)
}
#[test]
fn native_format_algo_order() {
    suite("formatAlgoOrder", 6)
}
#[test]
fn native_get_or_fetch_position_mode() {
    suite("getOrFetchPositionMode", 6)
}
#[test]
fn native_get_or_fetch_margin_type() {
    suite("getOrFetchMarginType", 6)
}
#[test]
fn native_fetch_leverage_for_symbol() {
    suite("fetchLeverageForSymbol", 11)
}
#[test]
fn native_change_position_mode() {
    suite("changePositionMode", 3)
}
#[test]
fn native_place_trigger_order() {
    suite("placeTriggerOrder", 5)
}
#[test]
fn native_cancel_symbol_orders() {
    suite("cancelSymbolOrders", 3)
}
#[test]
fn native_large_split_profile_is_explicit_refusal() {
    assert_eq!(corpus()["allocationWitness"], 10001);
    assert_eq!(corpus()["allocationRemainderWitness"], 10001);
    for (amount, lot) in [(10001.0, "1"), (9999.5, "0.5")] {
        let input = json!({"market":{"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.25},"limits":{"amount":{"min":0.5,"max":1},"leverage":{"min":1,"max":50}},"info":{"lotSize":lot}}});
        let store = store(&input);
        let opts: PlaceOrderOpts = js_from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":101}),
        )
        .unwrap();
        let uuid_count = Cell::new(0);
        let host = FormatHost {
            get_margin_type: Box::new(|_| Box::pin(std::future::ready(Ok(MarginType::Cross)))),
            get_position_mode: Box::new(|_| {
                Box::pin(std::future::ready(Ok(Some(PositionMode::OneWay))))
            }),
            uuid: Box::new(|| {
                uuid_count.set(uuid_count.get() + 1);
                Ok("u001".into())
            }),
            resolve_caller_client_order_id: Box::new(|_| Ok(None)),
            assert_single_native_client_order_id: Box::new(|_, _| Ok(())),
            cancel_algo_orders: Box::new(|_| Box::pin(std::future::ready(Ok(())))),
            broker_id_enabled: true,
        };
        let result = ready(format_normal_order(
            &safe_cex_types::PreparedOrderOpts::from(&opts),
            &store,
            &host,
        ));
        assert!(
            result.is_err(),
            "native_cap_refuses_full_and_remainder_payloads"
        );
        assert_eq!(
            error(result.unwrap_err()),
            "BloFin split exceeds the native10000-payload allocation profile"
        );
        assert_eq!(
            uuid_count.get(),
            1,
            "native_cap_precedes_split_uuid_generation"
        );
    }
}
