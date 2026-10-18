use super::*;
#[path = "../tests/support/unit_tasks.rs"]
mod unit_tasks;
use crate::utils::normalize_market;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::ExchangeSettings,
    runtime::{ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::Store;
use safe_cex_transport::{RestFuture, RestResponse};
use safe_cex_types::StoreSubscriberError;
use safe_cex_types::{ExchangeName, ExchangeOptions, ExecutionsOptions};
use std::{cell::Cell, collections::VecDeque, future::Future, pin::Pin};
#[path = "reporting_tests.rs"]
mod reporting_values;
struct Runtime(f64, RefCell<Vec<Value>>, Rc<Cell<f64>>);
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.0
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.2.get()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        let clock = self.2.clone();
        Box::pin(async move {
            clock.set(clock.get() + delay_ms as f64);
            Ok(())
        })
    }
    fn spawn_local(&self, work: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        unit_tasks::spawn(work)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = if !matches!(severity, "info" | "warning" | "debug") {
            safe_cex_exchange::runtime::console_text(message)
        } else {
            let [message, observed_severity] = message else {
                panic!(
                    "BaseExchange.log console observation requires message and severity: {severity}: {message:?}"
                );
            };
            assert!(
                observed_severity
                    .strict_equals(&safe_cex_exchange::js_runtime::value::Js::str(severity)),
                "BaseExchange.log console severity argument must match its recorded channel"
            );
            safe_cex_exchange::runtime::console_text(std::slice::from_ref(message))
        };
        let message = rendered.as_str();
        self.1
            .borrow_mut()
            .push(json!({"message": message, "severity": severity}));
    }
    fn unique_id(&self) -> String {
        "id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        _: &ExchangeError,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct NoRequests;
impl RestAdapter for NoRequests {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("unexpected request")
    }
}
struct LeverageResponse(RefCell<Vec<RestRequest>>);
impl RestAdapter for LeverageResponse {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.borrow_mut().push(request);
        Box::pin(async {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(br#"[{"positionType":1,"leverage":"31","maxLeverageView":"80","openType":1},{"positionType":2,"leverage":"12","maxLeverage":"60","openType":2}]"#.to_vec()),
            })
        })
    }
}
struct LeverageFailure;
impl RestAdapter for LeverageFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move { Err(RestFailure::pre_transmit("request failed".into(), request)) })
    }
}
struct AccountFailure {
    message: String,
    response_data: Option<Value>,
}
impl RestAdapter for AccountFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let mut failure = RestFailure::pre_transmit(self.message.clone(), request);
        failure.response = self.response_data.as_ref().map(|data| {
            Box::new(RestResponse {
                status: 401,
                reason: Some("Unauthorized".into()),
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone())
                        .unwrap(),
                ),
            })
        });
        Box::pin(async move { Err(failure) })
    }
}
struct StaticResponse(Vec<u8>, RefCell<Vec<RestRequest>>);
impl RestAdapter for StaticResponse {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.1.borrow_mut().push(request);
        let body = self.0.clone();
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body),
            })
        })
    }
}
struct ScriptedResponse {
    responses: RefCell<VecDeque<Value>>,
    requests: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for ScriptedResponse {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(request);
        let body = serde_json::to_vec(
            &self
                .responses
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| json!({"code":0})),
        )
        .unwrap();
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(body),
            })
        })
    }
}
fn order(raw: &Value) -> Order {
    let mut order = normalize_order(
        json!({"id":"fixture","symbol":"BTC_USDT","side":1})
            .as_object()
            .unwrap(),
        None,
        &|v| v,
    )
    .unwrap()
    .unwrap();
    order.id = raw["id"].as_str().unwrap().into();
    order.symbol = raw["symbol"].as_str().unwrap().into();
    order.status = serde_json::from_value(raw["status"].clone()).unwrap();
    order.r#type = serde_json::from_value(raw["type"].clone()).unwrap();
    order.last_update = raw["lastUpdate"].as_f64().unwrap();
    order
}
fn view(orders: &[Order]) -> Value {
    Value::Array(orders.iter().map(|o|json!({"id":o.id,"symbol":o.symbol,"status":o.status,"type":o.r#type,"lastUpdate":o.last_update})).collect())
}
fn cancel_order(raw: &Value) -> Order {
    let mut order = order(&json!({
        "id":raw["id"],"symbol":raw["symbol"],"status":"open",
        "type":raw["type"],"lastUpdate":123456.0
    }));
    order.amount = raw["amount"].as_f64().unwrap();
    order.remaining = raw["remaining"].as_f64().unwrap();
    order.info = raw.get("info").cloned().map(Into::into).into();
    order
}
fn pairs_object(pairs: &[(String, f64)]) -> Value {
    Value::Object(
        pairs
            .iter()
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect(),
    )
}
fn venue(store: Rc<DefaultStore>, time: f64, public_only: bool) -> MexcExchange {
    venue_with_runtime(store, time, public_only).0
}
fn venue_with_runtime(
    store: Rc<DefaultStore>,
    time: f64,
    public_only: bool,
) -> (MexcExchange, Rc<Runtime>) {
    venue_with_runtime_and_key(store, time, public_only, "key")
}
fn venue_with_runtime_and_key(
    store: Rc<DefaultStore>,
    time: f64,
    public_only: bool,
    key: &str,
) -> (MexcExchange, Rc<Runtime>) {
    let runtime = Rc::new(Runtime(time, RefCell::default(), Rc::new(Cell::new(0.0))));
    let exchange = MexcExchange {
        external: Rc::new(|_, _| panic!("no externalPublicData configured")),
        base: Rc::new(BaseExchange::new(
            ExchangeName::Mexc,
            serde_json::from_value::<ExchangeOptions>(
                json!({"key":key,"secret":"secret","publicDataOnly":public_only}),
            )
            .unwrap(),
            store,
            runtime.clone(),
            ExchangeSettings::default(),
        )),
        capabilities: ExchangeCapabilities::default(),
        xhr: Rc::new(NoRequests),
        local_to_utc: Rc::new(|v| v),
        current_time_ms: Rc::new(move || time),
        placed: RefCell::new(BTreeMap::from([
            ("placed".into(), 1.0),
            ("seen".into(), 1.0),
        ])),
        canceled: RefCell::new(BTreeMap::from([("canceled".into(), 1.0)])),
        rejected: RefCell::new(BTreeMap::from([("rejected".into(), 1.0)])),
        symbol_settings: Rc::new(RefCell::new(BTreeMap::new())),
        account_position_mode: RefCell::new(safe_cex_types::PositionMode::Hedged),
        public_socket: RefCell::default(),
        binding: Default::default(),
    };
    (exchange, runtime)
}

#[test]
fn mexc_test_runtime_monotonic_clock_tracks_sleep() {
    let runtime = Runtime(1_000.0, RefCell::default(), Rc::new(Cell::new(0.0)));
    assert_eq!(runtime.monotonic_now_ms(), 0.0);
    unit_tasks::drive(runtime.sleep(17)).unwrap();
    assert_eq!(runtime.monotonic_now_ms(), 17.0);
}

#[test]
fn mexc_account_identity_matches_live_source() {
    let response = Rc::new(StaticResponse(
        br#"[{"currency":"USDT","availableBalance":"10","equity":"12","unrealized":"2"}]"#.to_vec(),
        RefCell::new(Vec::new()),
    ));
    let mut exchange = venue(Rc::new(DefaultStore::new()), 100.0, false);
    exchange.xhr = response.clone();
    let validation = unit_tasks::drive(Exchange::validate_account(&exchange)).unwrap();
    assert_eq!(
        validation.error.as_value(),
        &json!(""),
        "MEXC account validation result"
    );
    assert!(
        validation.balance.is_some(),
        "MEXC account validation balance presence"
    );
    let native_balance = validation.balance.as_ref().unwrap();
    for (label, actual, expected) in [
        ("free", native_balance.free, 10.0_f64),
        ("used", native_balance.used, 2.0_f64),
        ("total", native_balance.total, 12.0_f64),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "MEXC validation native balance:{label}"
        );
    }
    let account = unit_tasks::drive(Exchange::get_account(&exchange)).unwrap();
    assert_eq!(
        account.user_id, "mexc:2c70e12b7a0646f92279f427",
        "MEXC account identity"
    );

    let (mut empty_key, _) =
        venue_with_runtime_and_key(Rc::new(DefaultStore::new()), 100.0, false, "");
    empty_key.xhr = response;
    let empty_account = unit_tasks::drive(Exchange::get_account(&empty_key)).unwrap();
    assert_eq!(
        empty_account.user_id, "mexc:e4acf8526c3684f3f7572ff1",
        "MEXC empty-key identity fallback"
    );

    let mut failure_venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
    failure_venue.xhr = Rc::new(AccountFailure {
        message: "transport failure".into(),
        response_data: Some(json!({"msg":"response detail"})),
    });
    let validation = unit_tasks::drive(Exchange::validate_account(&failure_venue)).unwrap();
    assert_eq!(
        validation.error.as_value(),
        &json!("response detail"),
        "MEXC native response-msg precedence"
    );
    let rust_error = unit_tasks::drive(Exchange::get_account(&failure_venue)).unwrap_err();
    assert_eq!(
        rust_error.message, "response detail",
        "MEXC getAccount propagated validation error"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "account oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected["cases"].as_array().unwrap().len(), 5);
    assert_eq!(
        account.user_id, expected["cases"][1]["userId"],
        "MEXC account identity"
    );
    assert_eq!(
        empty_account.user_id, expected["cases"][2]["userId"],
        "MEXC empty-key identity fallback"
    );
    assert_eq!(
        validation.error.as_value(),
        &expected["cases"][3]["error"],
        "MEXC source response-message precedence"
    );
    assert_eq!(
        expected["cases"][4]["error"], "response detail",
        "MEXC source getAccount error message"
    );
}

#[test]
fn mexc_account_identity_mutations_fail_by_name() {
    for (variable, value, assertion) in [
        ("MEXC_ACCOUNT_RUST_CONTROL", "1", "MEXC account identity"),
        (
            "MEXC_ACCOUNT_TS_CONTROL",
            "empty-key-nullish",
            "MEXC empty-key identity fallback",
        ),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "exchange::tests::mexc_account_identity_matches_live_source",
                "--nocapture",
            ])
            .env(variable, value)
            .output()
            .unwrap();
        let output_text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{variable}: {output_text}");
        assert!(output_text.contains(assertion), "{variable}: {output_text}");
    }
}

#[test]
fn mexc_rejected_order_state_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rejected_order_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5, "rejected-order source scenario census");
    for index in 0..5 {
        let case = &cases[index];
        let store = Rc::new(DefaultStore::new());
        let initial = case["expected"]["orders"]
            .as_array()
            .and_then(|rows| rows.first())
            .map(|source_row| {
                order(&json!({
                    "id": source_row["id"],
                    "symbol":"BTCUSDT",
                    "status":source_row["status"],
                "type":"limit",
                    "lastUpdate":0.0
                }))
            });
        if let Some(order) = initial {
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    orders: Some(vec![order]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let (exchange, runtime) = venue_with_runtime(store.clone(), 123.25, false);
        exchange.rejected.borrow_mut().clear();
        exchange
            .note_rejected_regular_order(
                case["orderId"].as_str().unwrap(),
                case["errorCode"].as_f64().unwrap(),
            )
            .unwrap();
        let actual = json!({
            "orders":store.orders().unwrap().iter().map(|order|json!({"id":order.id,"status":order.status})).collect::<Vec<_>>(),
            "shield":exchange.rejected.borrow().iter().map(|(id,time)|json!([id,time])).collect::<Vec<_>>(),
            "logs":runtime.1.borrow().clone()
        });
        assert_eq!(actual["orders"], case["expected"]["orders"], "case {case}");
        assert_eq!(actual["shield"], case["expected"]["shield"], "case {case}");
        let expected_error = case["expected"]["errors"].as_array().unwrap();
        if let Some(expected_error) = expected_error.first().and_then(Value::as_str) {
            assert!(
                runtime.1.borrow().iter().any(|log| log["message"]
                    .as_str()
                    .is_some_and(|message| message.contains(expected_error))),
                "missing source reason text {expected_error:?} in Rust error logs"
            );
        } else {
            assert!(runtime.1.borrow().is_empty(), "empty id must not log");
        }
    }
}

#[test]
fn mexc_rejected_order_propagates_notification_after_commit() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rejected_order_throw_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "rejected-order subscriber oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        expected["rejected"], true,
        "source propagates subscriber throw"
    );
    assert_eq!(expected["message"], "subscriber failed");
    assert_eq!(expected["orders"][0]["status"], "canceled");
    assert_eq!(expected["subscriberCalls"], json!(["first"]));
    assert!(expected["errors"].as_array().unwrap().is_empty());

    let store = Rc::new(DefaultStore::new());
    let mut canceled = order(&json!({
        "id":"reject-id","symbol":"BTCUSDT","status":"open",
        "type":"limit","lastUpdate":1
    }));
    canceled.status = safe_cex_types::OrderStatus::Open.into();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![canceled]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = calls.clone();
    let _first = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            first_calls.borrow_mut().push("first");
            Err(StoreSubscriberError::new("subscriber failed"))
        }),
    );
    let second_calls = calls.clone();
    let _second = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            second_calls.borrow_mut().push("second");
            Ok(())
        }),
    );
    let (exchange, runtime) = venue_with_runtime(store.clone(), 123.25, false);
    exchange.rejected.borrow_mut().clear();
    let error = exchange
        .note_rejected_regular_order("reject-id__legacy", 6.0)
        .unwrap_err();
    assert_eq!(error.message, expected["message"]);
    assert_eq!(error.store_subscriber_ids, [0]);
    assert_eq!(
        store.orders().unwrap()[0].status,
        safe_cex_types::OrderStatus::Canceled,
        "Store status commits before notification failure"
    );
    assert_eq!(
        *exchange.rejected.borrow(),
        BTreeMap::from([("reject-id".into(), 123.25)])
    );
    assert_eq!(*calls.borrow(), ["first"]);
    assert!(
        runtime.1.borrow().is_empty(),
        "error emission follows the failed Store write"
    );
}

#[test]
fn mexc_cancel_orders_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cancel_orders_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC cancel oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 6, "MEXC cancellation source corpus count");
    let expected_source_counts = [0, 3, 2, 1, 1, 51];
    let expected_request_counts = [0, 3, 1, 1, 1, 2];
    let mut native_comparisons = 0;
    for index in 0..6 {
        let case = &cases[index];
        let name = case["name"].as_str().unwrap();
        let throw_subscriber = name == "notification failure follows committed status update";
        let raw_orders = case["sourceOrders"].as_array().unwrap();
        assert_eq!(
            raw_orders.len(),
            expected_source_counts[index],
            "cancelOrders source order count: {name}"
        );
        let ids = case["orders"].as_array().unwrap();
        assert_eq!(
            ids.len(),
            expected_source_counts[index],
            "cancelOrders source id count: {name}"
        );
        let orders: Vec<_> = raw_orders.iter().map(cancel_order).collect();
        let store = Rc::new(DefaultStore::new());
        if !orders.is_empty() {
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    orders: Some(orders.clone()),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let calls = Rc::new(RefCell::new(Vec::<&'static str>::new()));
        let mut unsubscribe = Vec::new();
        if throw_subscriber {
            for label in ["first", "second"] {
                let calls = calls.clone();
                unsubscribe.push(Store::subscribe(
                    store.as_ref(),
                    Box::new(move |_, _| {
                        calls.borrow_mut().push(label);
                        if label == "first" {
                            Err(StoreSubscriberError::new("subscriber failed"))
                        } else {
                            Ok(())
                        }
                    }),
                ));
            }
        }
        let adapter = Rc::new(ScriptedResponse {
            responses: RefCell::new(
                case["responses"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .cloned()
                    .collect(),
            ),
            requests: RefCell::default(),
        });
        let (mut exchange, _) = venue_with_runtime(store.clone(), 123_456.0, false);
        *exchange.placed.borrow_mut() =
            BTreeMap::from([("plan".into(), 1.0), ("stop".into(), 1.0)]);
        *exchange.canceled.borrow_mut() = BTreeMap::new();
        exchange.xhr = adapter.clone();
        let result = unit_tasks::drive(Exchange::cancel_orders(&exchange, orders));
        let expected_error = case.get("error").filter(|value| !value.is_null());
        if let Some(expected_error) = expected_error {
            let error = match result {
                Err(error) => error,
                Ok(()) => panic!("cancelOrders unexpectedly accepted source error: {name}"),
            };
            assert_eq!(
                error.name, expected_error["name"],
                "cancelOrders error type: {name}"
            );
            assert_eq!(
                error.message, expected_error["message"],
                "cancelOrders error identity: {name}"
            );
            if throw_subscriber {
                assert_eq!(
                    error.store_subscriber_ids,
                    [0],
                    "cancelOrders notification identity"
                );
            }
        } else {
            result.unwrap();
        }
        let requests = adapter.requests.borrow();
        assert_eq!(
            requests.len(),
            expected_request_counts[index],
            "cancelOrders native request count: {name}"
        );
        assert_eq!(
            requests.len(),
            case["calls"].as_array().unwrap().len(),
            "cancelOrders native/source request count: {name}"
        );
        for request_index in 0..expected_request_counts[index] {
            let expected = &case["calls"][request_index];
            assert_eq!(
                requests[request_index].url, expected["url"],
                "cancelOrders endpoint identity: {name}/{request_index}"
            );
            assert_eq!(
                requests[request_index].method, "post",
                "cancelOrders method identity: {name}/{request_index}"
            );
            assert_eq!(
                requests[request_index]
                    .body
                    .as_ref()
                    .unwrap()
                    .untransformed()
                    .unwrap(),
                &safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    expected["data"].clone()
                )
                .unwrap(),
                "cancelOrders payload identity: {name}/{request_index}"
            );
        }
        let native_orders = store.orders().unwrap();
        assert_eq!(
            native_orders.len(),
            expected_source_counts[index],
            "cancelOrders native status count:{name}"
        );
        let source_statuses = case["statuses"].as_array().unwrap();
        assert_eq!(
            source_statuses.len(),
            expected_source_counts[index],
            "cancelOrders source status count:{name}"
        );
        for order_index in 0..expected_source_counts[index] {
            let actual = native_orders
                .get(order_index)
                .expect("cancelOrders native status present");
            let expected = &source_statuses[order_index];
            let expected_status = match expected[1].as_str().unwrap() {
                "open" => OrderStatus::Open,
                "canceled" => OrderStatus::Canceled,
                other => panic!("unexpected cancellation source status:{other}"),
            };
            assert_eq!(
                actual.id,
                expected[0].as_str().unwrap(),
                "cancelOrders native status id:{name}"
            );
            assert_eq!(
                actual.status, expected_status,
                "cancelOrders native status:{name}"
            );
        }
        let actual_canceled = pairs_object(
            &exchange
                .canceled
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), *value))
                .collect::<Vec<_>>(),
        );
        let expected_canceled = pairs_object(
            &case["canceled"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_str().unwrap().to_owned(),
                        pair[1].as_f64().unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            actual_canceled, expected_canceled,
            "cancelOrders canceled shield identity: {name}"
        );
        let actual_placed = pairs_object(
            &exchange
                .placed
                .borrow()
                .iter()
                .map(|(key, value)| (key.clone(), *value))
                .collect::<Vec<_>>(),
        );
        let expected_placed = pairs_object(
            &case["placed"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_str().unwrap().to_owned(),
                        pair[1].as_f64().unwrap(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        assert_eq!(
            actual_placed, expected_placed,
            "cancelOrders placed shield identity: {name}"
        );
        assert_eq!(
            json!(*calls.borrow()),
            case["subscriberCalls"],
            "cancelOrders subscriber order: {name}"
        );
        native_comparisons += 1;
    }
    assert_eq!(
        native_comparisons, 6,
        "cancelOrders native case comparisons"
    );
}

#[test]
fn mexc_cancel_orders_typescript_mutation_fails_by_name() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cancel_orders_oracle.mjs"
        ))
        .env("MEXC_TS_CONTROL", "cancel-stop-kind")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("MEXC cancel stop-order payload identity"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn mexc_cancel_orders_rust_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_cancel_orders_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_CANCEL_ORDERS_RUST_CONTROL", "stop-kind")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(text.contains("cancelOrders payload identity"), "{text}");
}

#[test]
fn mexc_rejected_order_propagation_detects_typescript_mutation_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "exchange::tests::mexc_rejected_order_propagates_notification_after_commit",
            "--nocapture",
        ])
        .env("MEXC_REJECTED_THROW_TS_CONTROL", "suppress-update")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_rejected_order_propagates_notification_after_commit ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_fetch_markets_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_markets_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchMarkets oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 3, "fetchMarkets source scenario census");
    for case_index in 0..3 {
        let case = &cases[case_index];
        let response = Rc::new(StaticResponse(
            serde_json::to_vec(&case["payload"]).unwrap(),
            RefCell::new(Vec::new()),
        ));
        let mut venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
        venue.xhr = response.clone();
        let mut markets = unit_tasks::drive(venue.fetch_markets()).unwrap();
        if let Some(market) = markets.first_mut() {
            let raw_rows = extract_rows(Some(&case["payload"]), &[]);
            let raw = raw_rows
                .iter()
                .find(|row| row["symbol"] == market.id)
                .unwrap();
            let info = market.info.as_mut().unwrap();
            info.as_object_mut().unwrap().push((
                "__from_market".encode_utf16().collect(),
                json!("market").into(),
            ));
            assert_ne!(raw["__from_market"], json!("market"));
            let mut source_payload = case["payload"].clone();
            let source_rows = if let Some(rows) = source_payload.as_array_mut() {
                rows
            } else {
                source_payload["data"].as_array_mut().unwrap()
            };
            let source_raw = source_rows
                .iter_mut()
                .find(|row| row["symbol"] == market.id)
                .unwrap();
            source_raw["__from_input"] = json!("input");
            assert!(
                !info
                    .get("__from_input")
                    .is_some_and(|value| value.text_equals("input"))
            );
            assert_eq!(case["alias"]["returnMutatesInput"], true);
            assert_eq!(case["alias"]["inputMutatesReturn"], true);
        }
        let actual: Vec<Value> = markets
            .iter()
            .map(|market| {
                let bits = |value: f64| {
                    if value.is_nan() {
                        "7ff8000000000000".to_owned()
                    } else {
                        format!("{:016x}", value.to_bits())
                    }
                };
                json!({
                    "id": market.id,
                    "symbol": market.symbol,
                    "base": market.base,
                    "quote": market.quote,
                    "active": market.active,
                    "amount": bits(market.precision.amount),
                    "price": bits(market.precision.price),
                    "min": bits(market.limits.amount.min),
                    "max": bits(safe_cex_num::numeric_value_to_number(&market.limits.amount.max)),
                    "minLeverage": bits(market.limits.leverage.min),
                    "maxLeverage": bits(market.limits.leverage.max),
                })
            })
            .collect();
        assert_eq!(
            actual,
            case["markets"].as_array().unwrap().to_vec(),
            "fetchMarkets payload {}",
            case["payload"]
        );
        let requests = response.1.borrow();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "get");
        assert_eq!(requests[0].url, "/api/v1/contract/detail");
    }
}

#[test]
fn mexc_fetch_markets_live_source_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_markets_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_MARKETS_CONTROL", "accept-hidden")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_fetch_markets_matches_live_source ... FAILED"),
        "{output}"
    );
}

#[test]
fn mexc_fetch_ohlcv_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_ohlcv_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchOHLCV oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5, "fetchOHLCV source scenario census");
    let mut source_total_candles = 0;
    let mut native_candle_comparisons = 0;
    let mut native_request_field_comparisons = 0;
    let mut native_case_comparisons = 0;
    for case_index in 0..5 {
        let case = &cases[case_index];
        let options = &case["options"];
        if case["intervalType"] == "function" {
            assert_eq!(
                case["candles"].as_array().unwrap().len(),
                0,
                "fetchOHLCV source candle count: inherited interval"
            );
            assert_eq!(case["error"], Value::Null);
            assert_eq!(
                case["requests"][0]["params"]["interval"]["typeof"],
                "function"
            );
            assert!(
                serde_json::from_value::<OHLCVOptions>(options.clone()).is_err(),
                "closed Rust Timeframe unexpectedly accepts inherited constructor"
            );
            native_case_comparisons += 1;
            continue;
        }
        let interval: safe_cex_types::AnyTimeframe =
            serde_json::from_value(options["interval"].clone()).unwrap();
        let number = |key: &str| options[key].as_f64();
        let options = OHLCVOptions {
            symbol: options["symbol"].as_str().unwrap().into(),
            interval,
            from: number("from"),
            to: number("to"),
            limit: None,
        };
        let response = Rc::new(StaticResponse(
            serde_json::to_vec(&case["payload"]).unwrap(),
            RefCell::new(Vec::new()),
        ));
        let mut venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
        let local_offset = case["localOffsetMs"].as_f64().unwrap();
        venue.local_to_utc = Rc::new(move |milliseconds| milliseconds + local_offset);
        venue.xhr = response.clone();
        let result = unit_tasks::drive(venue.fetch_ohlcv(options));
        match case["error"].as_str() {
            Some(expected) => {
                assert_eq!(
                    case["candles"],
                    Value::Null,
                    "fetchOHLCV source candle count: unsupported interval"
                );
                assert_eq!(result.unwrap_err().message, expected);
                assert!(
                    response.1.borrow().is_empty(),
                    "unsupported interval sent a request"
                );
                native_case_comparisons += 1;
            }
            None => {
                let candles = result.unwrap();
                let expected_candle_count = [2, 2, 1, 0, 0][case_index];
                let expected_candles = case["candles"].as_array().unwrap();
                if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
                    assert_eq!(
                        expected_candles.len(),
                        expected_candle_count,
                        "fetchOHLCV source candle count: case {case_index}"
                    );
                }
                if std::env::var_os("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS").is_none() {
                    assert_eq!(
                        candles.len(),
                        expected_candle_count,
                        "fetchOHLCV native candle count: case {case_index}"
                    );
                }
                source_total_candles += expected_candles.len();
                let bits = |value: f64| {
                    if value.is_nan() {
                        "7ff8000000000000".to_owned()
                    } else {
                        format!("{:016x}", value.to_bits())
                    }
                };
                let actual: Vec<Value> = candles
                    .iter()
                    .map(|candle| {
                        json!({
                            "timestamp": bits(candle.timestamp), "open": bits(candle.open),
                            "high": bits(candle.high), "low": bits(candle.low),
                            "close": bits(candle.close), "volume": bits(candle.volume),
                            "lastSide": candle.last_side,
                        })
                    })
                    .collect();
                assert_eq!(
                    actual,
                    case["candles"].as_array().unwrap().to_vec(),
                    "fetchOHLCV payload {}",
                    case["payload"]
                );
                native_candle_comparisons += candles.len();
                let requests = response.1.borrow();
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].method, case["requests"][0]["method"]);
                assert_eq!(requests[0].url, case["requests"][0]["url"]);
                assert_eq!(
                    requests[0].timeout_ms,
                    case["requests"][0]["timeout"].as_i64()
                );
                let actual_params = requests[0].params.as_ref().unwrap().as_object().unwrap();
                let expected_params = case["requests"][0]["params"].as_object().unwrap();
                let expected_param_keys: &[&str] = match case_index {
                    0 => &["interval", "start", "end"],
                    1 | 2 => &["interval"],
                    _ => unreachable!(),
                };
                assert_eq!(
                    expected_params
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    expected_param_keys,
                    "fetchOHLCV source query field identity: case {case_index}"
                );
                assert_eq!(
                    actual_params.keys().collect::<Vec<_>>(),
                    expected_params.keys().collect::<Vec<_>>()
                );
                let expected_param_count = [3, 1, 1, 0, 0][case_index];
                assert_eq!(
                    expected_params.len(),
                    expected_param_count,
                    "fetchOHLCV source param count: case {case_index}"
                );
                assert_eq!(
                    actual_params.len(),
                    expected_param_count,
                    "fetchOHLCV native param count: case {case_index}"
                );
                for key in ["interval", "start", "end"] {
                    if let Some(expected) = expected_params.get(key) {
                        let actual = &actual_params[key];
                        if let Some(expected_number) = expected.as_f64() {
                            assert_eq!(
                                actual.as_f64().unwrap().to_bits(),
                                expected_number.to_bits(),
                                "query param {key}"
                            );
                        } else {
                            assert_eq!(actual, expected, "query param {key}");
                        }
                        native_request_field_comparisons += 1;
                    }
                }
                native_case_comparisons += 1;
            }
        }
    }
    if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
        assert_eq!(source_total_candles, 5, "fetchOHLCV source total candles");
    }
    assert_eq!(
        native_candle_comparisons, 5,
        "fetchOHLCV native candle comparisons"
    );
    assert_eq!(
        native_request_field_comparisons, 5,
        "fetchOHLCV native request field comparisons"
    );
    assert_eq!(
        native_case_comparisons, 5,
        "fetchOHLCV native case comparisons"
    );
}

#[test]
fn mexc_fetch_order_book_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_order_book_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchOrderBook oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 3, "fetchOrderBook source scenario census");
    let mut source_total_levels = 0;
    let mut native_comparisons = 0;
    let mut native_request_comparisons = 0;
    for case_index in 0..3 {
        let case = &cases[case_index];
        let store = Rc::new(DefaultStore::new());
        if case["name"] != "missing-market" {
            let market = normalize_market(
                &serde_json::Value::Object((case["marketRaw"].as_object().unwrap()).clone()).into(),
            )
            .unwrap();
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    markets: Some(vec![market]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let adapter = Rc::new(StaticResponse(
            serde_json::to_vec(&json!({
                "data": {
                    "version": 1700000000456_u64,
                    "bids": [[99,3],[100,2]],
                    "asks": [[102,5],[101,4]]
                }
            }))
            .unwrap(),
            RefCell::default(),
        ));
        let mut exchange = venue(store, 100.0, false);
        exchange.xhr = adapter.clone();
        let limit = case["limit"].as_f64();
        let result = unit_tasks::drive(
            exchange.fetch_order_book(case["symbol"].as_str().unwrap().into(), limit),
        );
        match case["error"].as_str() {
            Some(expected) => {
                assert_eq!(result.unwrap_err().message, expected);
                assert!(
                    adapter.1.borrow().is_empty(),
                    "missing market sent a request"
                );
            }
            None => {
                let (book, last_update_id) = result.unwrap();
                assert_eq!(
                    last_update_id,
                    case["lastUpdateId"].as_str().unwrap(),
                    "fetchOrderBook lastUpdateId field identity"
                );
                let bits = |value: f64| format!("{:016x}", value.to_bits());
                for (side_index, (side, actual, expected)) in [
                    ("bids", &book.bids, &case["book"]["bids"]),
                    ("asks", &book.asks, &case["book"]["asks"]),
                ]
                .into_iter()
                .enumerate()
                {
                    let source_count = expected.as_array().unwrap().len();
                    let expected_count = match (case_index, side_index) {
                        (0 | 1, 0 | 1) => 2,
                        _ => unreachable!(),
                    };
                    if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
                        assert_eq!(
                            source_count, expected_count,
                            "fetchOrderBook {side} source level count"
                        );
                    }
                    if std::env::var_os("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS").is_none() {
                        assert_eq!(
                            actual.len(),
                            source_count,
                            "fetchOrderBook {side} expected/native level count"
                        );
                    }
                    source_total_levels += source_count;
                    for level_index in 0..expected_count {
                        let Some(level) = actual.get(level_index) else {
                            panic!("fetchOrderBook native level missing: {side}/{level_index}");
                        };
                        let expected = &expected[level_index];
                        assert_eq!(
                            bits(level.price),
                            expected["price"],
                            "{side} price field identity"
                        );
                        assert_eq!(
                            bits(level.amount),
                            expected["amount"],
                            "{side} amount field identity"
                        );
                        assert_eq!(
                            bits(level.total),
                            expected["total"],
                            "{side} total field identity"
                        );
                        native_comparisons += 1;
                    }
                }
                let requests = adapter.1.borrow();
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].url, case["requests"][0]["url"]);
                let actual_params = requests[0].params.as_ref().unwrap().as_object().unwrap();
                let expected_params = case["requests"][0]["params"].as_object().unwrap();
                assert_eq!(
                    actual_params.keys().collect::<Vec<_>>(),
                    expected_params.keys().collect::<Vec<_>>()
                );
                assert_eq!(
                    expected_params.len(),
                    1,
                    "fetchOrderBook source query field count"
                );
                assert_eq!(
                    actual_params.len(),
                    1,
                    "fetchOrderBook native query field count"
                );
                assert_eq!(
                    actual_params["limit"].as_f64().unwrap().to_bits(),
                    expected_params["limit"].as_f64().unwrap().to_bits(),
                    "query limit field identity"
                );
                native_request_comparisons += 1;
            }
        }
    }
    if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
        assert_eq!(source_total_levels, 8, "fetchOrderBook source total levels");
    }
    assert_eq!(
        native_comparisons, 8,
        "fetchOrderBook native level comparisons"
    );
    assert_eq!(
        native_request_comparisons, 2,
        "fetchOrderBook native request comparisons"
    );
}

#[test]
fn mexc_fetch_order_book_typescript_version_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_order_book_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_ORDER_BOOK_TS_CONTROL", "replace-version")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("MEXC TypeScript lastUpdateId field identity"),
        "{text}"
    );
}

#[test]
fn mexc_fetch_order_book_empty_collection_control_fails_by_name() {
    for (mutation, bypass_source_counts) in [
        ("empty", false),
        ("drop-one", false),
        ("extra", false),
        ("empty", true),
    ] {
        let name = "exchange::tests::mexc_fetch_order_book_matches_live_source";
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture"])
            .env("MEXC_FETCH_ORDER_BOOK_RUST_CONTROL", mutation);
        if bypass_source_counts {
            command.env("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS", "1");
        }
        let output = command.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{mutation}: {text}");
        assert!(
            text.contains("fetchOrderBook bids source level count")
                || text.contains("fetchOrderBook bids expected/native level count"),
            "{mutation}: {text}"
        );
    }
}

#[test]
fn mexc_fetch_order_book_empty_source_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_order_book_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_ORDER_BOOK_TS_CONTROL", "empty-source")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("fetchOrderBook source scenario census"),
        "{text}"
    );
}

#[test]
fn mexc_fetch_ohlcv_source_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_ohlcv_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_OHLCV_CONTROL", "reverse-sort")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_fetch_ohlcv_matches_live_source ... FAILED"),
        "{output}"
    );
    assert!(output.contains("fetchOHLCV payload"), "{output}");
}

#[test]
fn mexc_fetch_ohlcv_rust_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_ohlcv_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap(),
        )
        .env("MEXC_FETCH_OHLCV_RUST_CONTROL", "reverse-sort")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.contains("mexc_fetch_ohlcv_matches_live_source ... FAILED"),
        "{output}"
    );
    assert!(output.contains("fetchOHLCV payload"), "{output}");
}

#[test]
fn mexc_fetch_ohlcv_empty_source_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_ohlcv_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_OHLCV_CONTROL", "empty-source")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(text.contains("fetchOHLCV source scenario census"), "{text}");
}

#[test]
fn mexc_fetch_ohlcv_collection_controls_fail_by_name() {
    for (mutation, bypass_source_counts) in [
        ("empty", false),
        ("drop-one", false),
        ("extra", false),
        ("empty", true),
    ] {
        let name = "exchange::tests::mexc_fetch_ohlcv_matches_live_source";
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture"])
            .env("MEXC_FETCH_OHLCV_RUST_CONTROL", mutation);
        if bypass_source_counts {
            command.env("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS", "1");
        }
        let output = command.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{mutation}: {text}");
        assert!(
            text.contains("fetchOHLCV native candle count: case 0"),
            "{mutation}: {text}"
        );
    }
}

#[test]
fn mexc_fetch_tickers_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_tickers_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchTickers oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let case: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(case["tickers"].as_array().unwrap().len(), 2);
    assert_eq!(case["alias"], true);
    let store = Rc::new(DefaultStore::new());
    let markets = case["markets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|market| {
            let mut result = normalize_market(
                &serde_json::Value::Object((market.as_object().unwrap()).clone()).into(),
            )
            .unwrap();
            result.id = market["id"].as_str().unwrap().to_owned();
            result.symbol = market["symbol"].as_str().unwrap().to_owned();
            result
        })
        .collect();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let response = Rc::new(StaticResponse(
        serde_json::to_vec(&case["raw"]).unwrap(),
        RefCell::new(Vec::new()),
    ));
    let mut venue = venue(store, 100.0, false);
    venue.xhr = response.clone();
    let mut tickers = unit_tasks::drive(venue.fetch_tickers()).unwrap();
    let mut parsed_source = case["raw"].clone();
    if let Some(ticker) = tickers.first_mut() {
        let info = ticker.info.as_mut().unwrap();
        info.as_object_mut().unwrap().push((
            "__from_ticker".encode_utf16().collect(),
            json!("ticker").into(),
        ));
        assert_ne!(parsed_source[2]["__from_ticker"], json!("ticker"));
        parsed_source[2]["__from_input"] = json!("input");
        assert!(
            !info
                .get("__from_input")
                .is_some_and(|value| value.text_equals("input"))
        );
    }
    let actual: Vec<Value> = tickers
        .iter()
        .map(|ticker| {
            let bits = |value: f64| {
                if value.is_nan() {
                    "7ff8000000000000".to_owned()
                } else {
                    format!("{:016x}", value.to_bits())
                }
            };
            json!({
                "id": ticker.id,
                "symbol": ticker.symbol,
                "bid": bits(ticker.bid),
                "ask": bits(ticker.ask),
                "last": bits(ticker.last),
                "mark": bits(ticker.mark),
                "index": bits(ticker.index.expect("numeric ticker source profile")),
                "percentage": bits(ticker.percentage),
                "openInterest": bits(ticker.open_interest),
                "fundingRate": bits(ticker.funding_rate),
                "fundingTime": bits(ticker.funding_time),
                "volume": bits(ticker.volume),
                "quoteVolume": bits(ticker.quote_volume),
            })
        })
        .collect();
    assert_eq!(actual, case["tickers"].as_array().unwrap().to_vec());
    assert_eq!(response.1.borrow()[0].url, "/api/v1/contract/ticker");
}

#[test]
fn mexc_fetch_tickers_typescript_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_tickers_matches_live_source";
    let process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_TICKERS_CONTROL", "drop-last-duplicate")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_eq!(process.status.code(), Some(101), "{output}");
    assert!(output.contains(&format!("{name} ... FAILED")), "{output}");
}

#[test]
fn mexc_fetch_balance_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_balance_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchBalance oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let case: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(case["alias"], true);
    let mut raw = case["raw"].clone();
    let response = Rc::new(StaticResponse(
        serde_json::to_vec(&raw).unwrap(),
        RefCell::new(Vec::new()),
    ));
    let mut exchange_venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
    exchange_venue.xhr = response.clone();
    let mut balance = unit_tasks::drive(exchange_venue.fetch_balance()).unwrap();
    let bits = |value: f64| {
        if value.is_nan() {
            "7ff8000000000000".to_owned()
        } else {
            format!("{:016x}", value.to_bits())
        }
    };
    assert_eq!(bits(balance.free), case["expected"]["free"]);
    assert_eq!(bits(balance.used), case["expected"]["used"]);
    assert_eq!(bits(balance.total), case["expected"]["total"]);
    assert_eq!(bits(balance.upnl), case["expected"]["upnl"]);
    let currencies = balance.currencies.as_ref().unwrap();
    assert_eq!(
        currencies.keys().map(String::as_str).collect::<Vec<_>>(),
        case["expected"]["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .collect::<Option<Vec<_>>>()
            .unwrap()
    );
    let rust_currencies = Value::Object(
        currencies
            .iter()
            .map(|(key, row)| {
                let row = row.as_balance().unwrap();
                (
                    key.clone(),
                    json!({
                        "free": bits(row.free),
                        "used": bits(row.used),
                        "total": bits(row.total),
                        "upnl": bits(row.upnl),
                    }),
                )
            })
            .collect(),
    );
    assert_eq!(rust_currencies, case["expected"]["currencies"]);
    let info = balance.info.as_mut().unwrap();
    info.as_array_mut().unwrap()[0]
        .as_object_mut()
        .unwrap()
        .push((
            "__from_result".encode_utf16().collect(),
            json!("result").into(),
        ));
    assert_ne!(raw[0]["__from_result"], json!("result"));
    raw[0]["__from_input"] = json!("input");
    assert!(
        !info.as_array().unwrap()[0]
            .get("__from_input")
            .is_some_and(|value| value.text_equals("input"))
    );
    assert_eq!(response.1.borrow()[0].url, "/api/v1/private/account/assets");
    let timing_response = Rc::new(StaticResponse(
        serde_json::to_vec(&json!([raw[0].clone(), null])).unwrap(),
        RefCell::new(Vec::new()),
    ));
    let mut timing_venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
    timing_venue.xhr = timing_response;
    let error = unit_tasks::drive(timing_venue.fetch_balance()).unwrap_err();
    assert_eq!(error.name, case["timing"]["name"]);
    assert_eq!(error.message, case["timing"]["message"]);
}

#[test]
fn mexc_fetch_balance_typescript_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_balance_matches_live_source";
    let process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_BALANCE_CONTROL", "drop-rows")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_eq!(process.status.code(), Some(101), "{output}");
    assert!(output.contains(&format!("{name} ... FAILED")), "{output}");
}

#[test]
fn mexc_fetch_position_mode_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_position_mode_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchPositionMode oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 9, "fetchPositionMode source scenario census");
    let mut native_comparisons = 0;
    for case_index in 0..9 {
        let case = &cases[case_index];
        let response = Rc::new(StaticResponse(
            serde_json::to_vec(&case["payload"]).unwrap(),
            RefCell::new(Vec::new()),
        ));
        let mut exchange_venue = venue(Rc::new(DefaultStore::new()), 100.0, false);
        exchange_venue.xhr = response.clone();
        let mode = unit_tasks::drive(exchange_venue.fetch_position_mode()).unwrap();
        assert_eq!(serde_json::to_value(mode).unwrap(), case["mode"]);
        assert_eq!(
            response.1.borrow()[0].url,
            "/api/v1/private/position/position_mode"
        );
        native_comparisons += 1;
    }
    assert_eq!(
        native_comparisons, 9,
        "fetchPositionMode native case comparisons"
    );
}

#[test]
fn mexc_fetch_position_mode_typescript_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_position_mode_matches_live_source";
    let process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_POSITION_MODE_CONTROL", "force-hedged")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_eq!(process.status.code(), Some(101), "{output}");
    assert!(output.contains(&format!("{name} ... FAILED")), "{output}");
}

#[test]
fn mexc_fetch_historical_fills_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_historical_fills_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchHistoricalFills oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 5, "historical fill source scenario census");
    let mut compared_cases = 0;
    let mut compared_fills = 0;
    for index in 0..5 {
        let case = &cases[index];
        let name = case["name"].as_str().unwrap();
        let (options, public_only) = match name {
            "full-options" => (
                ExecutionsOptions {
                    symbol: Some("BTCUSDT".into()),
                    order_id: Some("order-22".into()),
                    from: Some(1_699_999_999_000.0),
                    to: Some(1_700_000_002_000.0),
                    limit: Some(2.5),
                },
                false,
            ),
            "default-limit" => (
                ExecutionsOptions {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from: None,
                    to: None,
                    limit: Some(0.0),
                },
                false,
            ),
            "missing-symbol" => (
                ExecutionsOptions {
                    symbol: None,
                    order_id: Some("order-22".into()),
                    from: None,
                    to: None,
                    limit: None,
                },
                false,
            ),
            "public-only" => (
                ExecutionsOptions {
                    symbol: Some("BTCUSDT".into()),
                    order_id: None,
                    from: None,
                    to: None,
                    limit: None,
                },
                true,
            ),
            "non-finite-from" => (
                ExecutionsOptions {
                    symbol: Some("BTCUSDT".into()),
                    from: Some(f64::NAN),
                    order_id: None,
                    to: None,
                    limit: None,
                },
                false,
            ),
            _ => panic!("unexpected historical-fill scenario {name}"),
        };
        let body = json!({"data":{"items":[
            {"tradeId":"fill-slow","symbol":"BTC_USDT","side":1,"price":123.25,"vol":5,"createTime":1700000000000_i64,"orderId":"order-22","fee":-0.25,"feeCurrency":"USDT","profit":3.5,"role":2},
            {"tradeId":"fill-fast","symbol":"BTC_USDT","side":2,"price":55.5,"vol":7,"createTime":1700000001000_i64,"orderId":"order-33","fee":0.5,"feeCurrency":"USDC","profit":-2.75,"role":1}
        ]}}).to_string().into_bytes();
        let store = Rc::new(DefaultStore::new());
        let market = normalize_market(&serde_json::Value::Object((json!({"symbol":"BTC_USDT","contractSize":0.1,"volUnit":3,"minVol":2,"maxVol":100000,"priceScale":5,"minLeverage":4,"maxLeverage":100})
                .as_object()
                .unwrap()).clone()).into())
        .unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let adapter = Rc::new(StaticResponse(body, RefCell::default()));
        let mut exchange = venue(store, 123.25, public_only);
        exchange.xhr = adapter.clone();
        let fills =
            unit_tasks::drive(Exchange::fetch_historical_fills(&exchange, options)).unwrap();
        let expected_fills = case["fills"].as_array().unwrap();
        let source_count = match name {
            "full-options" | "default-limit" | "non-finite-from" => 2,
            "missing-symbol" | "public-only" => 0,
            _ => panic!("unexpected historical-fill case {name}"),
        };
        if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
            assert_eq!(
                expected_fills.len(),
                source_count,
                "historical source fill count: {name}"
            );
        }
        if std::env::var_os("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS").is_none() {
            assert_eq!(fills.len(), source_count, "historical fill count: {name}");
            assert_eq!(
                fills.len(),
                expected_fills.len(),
                "historical fill expected/native row count: {name}"
            );
        }
        for index in 0..source_count {
            let Some(actual) = fills.get(index) else {
                panic!("historical fill native row missing: {name}/{index}");
            };
            let expected = &expected_fills[index];
            assert_eq!(
                actual.id.as_deref(),
                expected["id"].as_str(),
                "execution id field identity"
            );
            assert_eq!(
                actual.symbol,
                expected["symbol"].as_str().unwrap(),
                "execution symbol field identity"
            );
            assert_eq!(
                actual.order_id.as_deref(),
                expected["orderId"].as_str(),
                "execution orderId field identity"
            );
            for (field, actual_value, expected_name) in [
                ("timestamp", actual.timestamp, "timestamp"),
                ("price", actual.price, "price"),
                ("qty", actual.qty, "qty"),
                ("commission", actual.commission.unwrap(), "commission"),
                ("realizedPnl", actual.realized_pnl.unwrap(), "realizedPnl"),
            ] {
                assert_eq!(
                    actual_value.to_bits(),
                    expected[expected_name].as_f64().unwrap().to_bits(),
                    "execution {field} field identity"
                );
            }
            assert_eq!(
                serde_json::to_value(actual.side).unwrap(),
                expected["side"],
                "execution side field identity"
            );
            assert_eq!(
                actual.commission_asset.as_deref(),
                expected["commissionAsset"].as_str(),
                "execution commissionAsset field identity"
            );
            assert_eq!(
                serde_json::to_value(&actual.position_side).unwrap(),
                expected["positionSide"],
                "execution positionSide field identity"
            );
            assert_eq!(
                actual.maker,
                expected["maker"].as_bool(),
                "execution maker field identity"
            );
        }
        compared_fills += fills.len();
        compared_cases += 1;
        if name == "full-options" {
            assert_eq!(fills.len(), 2);
            assert_eq!(fills[0].id.as_deref(), Some("fill-fast"));
            assert_eq!(fills[0].order_id.as_deref(), Some("order-33"));
            assert_eq!(fills[0].timestamp.to_bits(), 1_700_000_001.0_f64.to_bits());
            assert_eq!(fills[0].price.to_bits(), 55.5_f64.to_bits());
            assert_eq!(fills[0].commission.unwrap().to_bits(), 0.5_f64.to_bits());
            assert_eq!(
                fills[0].realized_pnl.unwrap().to_bits(),
                (-2.75_f64).to_bits()
            );
            let request = &adapter.1.borrow()[0];
            assert_eq!(request.url, "/api/v1/private/order/list/order_deals");
            assert_eq!(request.method, "get");
            let Some(safe_cex_types::QueryValue::Object(params)) = &request.query_params else {
                panic!("historical native query object");
            };
            assert_eq!(
                params.get("symbol"),
                Some(&safe_cex_types::QueryValue::String("BTC_USDT".into()))
            );
            assert_eq!(
                params.get("order_id"),
                Some(&safe_cex_types::QueryValue::String("order-22".into()))
            );
            assert_eq!(
                params.get("start_time"),
                Some(&safe_cex_types::QueryValue::Number(1_699_999_999_000.0))
            );
            assert_eq!(
                params.get("end_time"),
                Some(&safe_cex_types::QueryValue::Number(1_700_000_002_000.0))
            );
            assert_eq!(
                params.get("page_num"),
                Some(&safe_cex_types::QueryValue::Number(1.0))
            );
            assert_eq!(
                params.get("page_size"),
                Some(&safe_cex_types::QueryValue::Number(2.5))
            );
        } else if name == "default-limit" {
            assert_eq!(
                adapter.1.borrow()[0]
                    .query_params
                    .as_ref()
                    .and_then(|value| match value {
                        safe_cex_types::QueryValue::Object(params) => params.get("page_size"),
                        _ => None,
                    }),
                Some(&safe_cex_types::QueryValue::Number(100.0))
            );
        } else if name == "missing-symbol" || name == "public-only" {
            assert!(adapter.1.borrow().is_empty());
            assert!(fills.is_empty());
        } else {
            assert_eq!(
                case["requests"][0]["params"]["start_time"]["$number"],
                "7ff8000000000000"
            );
            let requests = adapter.1.borrow();
            let Some(safe_cex_types::QueryValue::Object(params)) = &requests[0].query_params else {
                panic!("historical native query object");
            };
            assert!(
                matches!(params.get("start_time"),Some(safe_cex_types::QueryValue::Number(value)) if value.is_nan()),
                "historical native nonfinite query is retained"
            );
        }
    }
    assert_eq!(compared_cases, 5, "historical fill native case comparisons");
    assert_eq!(compared_fills, 6, "historical fill native row comparisons");
}

#[test]
fn mexc_historical_fills_typescript_field_swap_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_historical_fills_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_TS_CONTROL", "execution-side-role")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("MEXC TypeScript execution role field identity"),
        "{text}"
    );
}

#[test]
fn mexc_fetch_historical_fills_field_swap_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_historical_fills_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_FILLS_RUST_CONTROL", "swap")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_fetch_historical_fills_matches_live_source ... FAILED"),
        "{text}"
    );
    assert!(
        text.contains("execution commission field identity"),
        "{text}"
    );
}

#[test]
fn mexc_fetch_historical_fills_collection_controls_fail_by_name() {
    for (mutation, bypass_source_counts) in [
        ("empty", false),
        ("drop-one", false),
        ("extra", false),
        ("empty", true),
    ] {
        let name = "exchange::tests::mexc_fetch_historical_fills_matches_live_source";
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", name, "--nocapture"])
            .env("MEXC_FETCH_FILLS_RUST_CONTROL", mutation);
        if bypass_source_counts {
            command.env("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS", "1");
        }
        let output = command.output().unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{mutation}: {text}");
        assert!(
            text.contains("historical fill count: full-options"),
            "{mutation}: {text}"
        );
    }
}

#[test]
fn mexc_fetch_historical_fills_empty_source_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_historical_fills_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_FILLS_TS_CONTROL", "empty-source")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("historical fill source scenario census"),
        "{text}"
    );
}

#[test]
fn mexc_fetch_positions_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_positions_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fetchPositions oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4, "fetchPositions source scenario census");
    let expected_position_counts = [4, 2, 4, 0];
    let mut native_position_comparisons = 0;
    for case_index in 0..4 {
        let case = &cases[case_index];
        let store = Rc::new(DefaultStore::new());
        let markets = case["markets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|market| {
                let mut result = normalize_market(
                    &serde_json::Value::Object((market.as_object().unwrap()).clone()).into(),
                )
                .unwrap();
                result.id = market["id"].as_str().unwrap().to_owned();
                result.symbol = market["symbol"].as_str().unwrap().to_owned();
                result
            })
            .collect();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(markets),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let response = Rc::new(StaticResponse(
            serde_json::to_vec(&json!({"data":case["raw"]})).unwrap(),
            RefCell::new(Vec::new()),
        ));
        let public_only = case["publicOnly"] == true;
        let (mut exchange_venue, _) = venue_with_runtime(store, 100.0, public_only);
        exchange_venue.xhr = response.clone();
        let symbols = case["symbols"].as_array().map(|values| {
            values
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect()
        });
        let mut positions =
            unit_tasks::drive(Exchange::fetch_positions(&exchange_venue, symbols)).unwrap();
        if std::env::var_os("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS").is_none() {
            assert_eq!(
                case["positions"].as_array().unwrap().len(),
                expected_position_counts[case_index],
                "fetchPositions source output count case {case_index}"
            );
        }
        if std::env::var_os("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS").is_none() {
            assert_eq!(
                positions.len(),
                case["positions"].as_array().unwrap().len(),
                "fetchPositions native output count case {case_index}"
            );
        }
        native_position_comparisons += positions.len();
        if let Some(position) = positions.iter_mut().find(|p| p.id == "BTCUSDT:short") {
            let info = position.info.as_mut().unwrap();
            info.as_object_mut().unwrap().push((
                "__fetchPositionsAlias".encode_utf16().collect(),
                json!(true).into(),
            ));
            let mut response_payload = case["raw"].clone();
            assert_ne!(response_payload[0]["__fetchPositionsAlias"], json!(true));
            response_payload[0]["__responseAlias"] = json!(true);
            assert_ne!(
                info.get("__responseAlias")
                    .and_then(|value| value.as_bool()),
                Some(true)
            );
            assert_eq!(case["alias"], true);
        }
        let actual: Vec<Value> = positions
            .iter()
            .map(|position| {
                let bits = |value: f64| if value.is_nan() { "7ff8000000000000".to_owned() } else { format!("{:016x}", value.to_bits()) };
                json!({
                    "id": position.id,
                    "symbol": position.symbol,
                    "side": position.side,
                    "entryPrice": bits(position.entry_price),
                    "notional": bits(position.notional),
                    "leverage": bits(position.leverage),
                    "marginType": position.margin_type,
                    "positionMode": position.position_mode,
                    "unrealizedPnl": bits(position.unrealized_pnl),
                    "realizedPnl": bits(position.realized_pnl.unwrap_or_default()),
                    "contracts": bits(position.contracts),
                    "liquidationPrice": bits(position.liquidation_price),
                    "marginSize": bits(position.margin_size.unwrap_or_default()),
                    "lastUpdate": bits(position.last_update.unwrap_or_default()),
                    "extra": {
                        "contractsNative": bits(position.extra.as_ref().unwrap().contracts_native.unwrap()),
                        "unrealizedPnlNative": bits(position.extra.as_ref().unwrap().unrealized_pnl_native.unwrap()),
                    },
                })
            })
            .collect();
        let expected_positions = case["positions"].as_array().unwrap();
        let actual_ids: Vec<_> = positions.iter().map(|position| &position.id).collect();
        let expected_ids: Vec<_> = expected_positions
            .iter()
            .map(|position| position["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            actual_ids, expected_ids,
            "fetchPositions native id/order identity case {case_index}"
        );
        assert_eq!(actual, expected_positions.to_vec());
        assert_eq!(response.1.borrow().len(), case["calls"]);
        if case["calls"] == 1 {
            assert_eq!(
                response.1.borrow()[0].url,
                "/api/v1/private/position/open_positions"
            );
        }
    }
    assert_eq!(
        native_position_comparisons, 10,
        "fetchPositions native row comparisons"
    );
}

#[test]
fn mexc_fetch_positions_typescript_mutation_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_positions_matches_live_source";
    let process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_POSITIONS_CONTROL", "omit-synthetic")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_eq!(process.status.code(), Some(101), "{output}");
    assert!(
        output.contains("fetchPositions source output count case 0"),
        "{output}"
    );
}

#[test]
fn mexc_fetch_positions_empty_source_control_fails_by_name() {
    let name = "exchange::tests::mexc_fetch_positions_matches_live_source";
    let process = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_FETCH_POSITIONS_CONTROL", "empty-source")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_eq!(process.status.code(), Some(101), "{output}");
    assert!(
        output.contains("fetchPositions source scenario census"),
        "{output}"
    );
}

#[test]
fn mexc_fetch_positions_collection_controls_fail_by_name() {
    for mutation in ["empty", "drop-one", "extra"] {
        let name = "exchange::tests::mexc_fetch_positions_matches_live_source";
        let process = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("MEXC_FETCH_POSITIONS_RUST_CONTROL", mutation)
            .env("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS", "1")
            .output()
            .unwrap();
        let output = format!(
            "{}{}",
            String::from_utf8_lossy(&process.stdout),
            String::from_utf8_lossy(&process.stderr)
        );
        assert_eq!(process.status.code(), Some(101), "{mutation}: {output}");
        assert!(
            output.contains("fetchPositions native output count case 0"),
            "{mutation}: {output}"
        );
    }
}

#[test]
fn mexc_fetch_positions_isolated_lodash_resolution_fails_loudly_when_removed() {
    let process = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_positions_oracle.mjs"
        ))
        .env("MEXC_FETCH_POSITIONS_CONTROL", "missing-lodash")
        .output()
        .unwrap();
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&process.stdout),
        String::from_utf8_lossy(&process.stderr)
    );
    assert_ne!(process.status.code(), Some(0), "{output}");
    assert!(output.contains("MODULE_NOT_FOUND"), "{output}");
}

#[test]
fn mexc_rejected_order_oracle_detects_typescript_mutation() {
    let name = "exchange::tests::mexc_rejected_order_state_matches_live_source";
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_REJECTED_TS_CONTROL", "keep-suffix")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains(&format!("{name} ... FAILED")) && text.contains("case "),
        "{text}"
    );
}

#[test]
fn mexc_reconciliation_live_store_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reconciliation_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 112);
    let throwing_oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/subscriber_throws_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        throwing_oracle.status.success(),
        "subscriber-throw oracle failed: {}",
        String::from_utf8_lossy(&throwing_oracle.stderr)
    );
    let subscriber_throws: Vec<Value> = serde_json::from_slice(&throwing_oracle.stdout).unwrap();
    assert_eq!(subscriber_throws.len(), 2);
    for subscriber_index in 0..2 {
        let case = &subscriber_throws[subscriber_index];
        let method = case["method"].as_str().unwrap();
        assert_eq!(
            case["rejected"], true,
            "{method}: source subscriber throw rejects after snapshot commit"
        );
        assert_eq!(case["message"], "subscriber failed", "{method}");
        assert_eq!(
            case["subscriberCalls"],
            json!(["first"]),
            "{method}: first subscriber throw skips the second"
        );
        assert!(
            case["orderIds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|id| id == "fetched"),
            "{method}: source commits snapshot state before notification throws"
        );
        assert_eq!(
            case["loaded"], true,
            "{method}: source commits loaded state"
        );
    }
    let mut native_comparisons = 0;
    for index in 0..112 {
        let case = &cases[index];
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                orders: Some(
                    case["initial"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(order)
                        .collect(),
                ),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let venue = venue(store.clone(), case["time"].as_f64().unwrap(), false);
        let notifications = Rc::new(Cell::new(0));
        let count = notifications.clone();
        let _unsubscribe = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                count.set(count.get() + 1);
                Ok(())
            }),
        );
        let fetched: Vec<_> = case["fetched"]
            .as_array()
            .unwrap()
            .iter()
            .map(order)
            .collect();
        let symbols = case
            .get("symbols")
            .map(|v| serde_json::from_value(v.clone()).unwrap());
        let regular = case["method"] == "fetchAndSetRegularOrders";
        let fetch = Box::pin(async {
            if case["disposed"] == true {
                venue.base.dispose().unwrap();
            }
            Ok(fetched)
        });
        let mut result =
            unit_tasks::drive(venue.reconcile_orders(symbols, fetch, regular)).unwrap();
        // Compare source-observable fields, retaining order/duplicates; the TS
        // fetch boundary uses partial rows, which the typed model fills out.
        let expected: Vec<_> = case["result"]
            .as_array()
            .unwrap()
            .iter()
            .map(order)
            .collect();
        assert_eq!(view(&result), view(&expected), "returned {index}: {case}");
        let expected: Vec<_> = case["orders"]
            .as_array()
            .unwrap()
            .iter()
            .map(order)
            .collect();
        assert_eq!(
            view(&store.orders().unwrap()),
            view(&expected),
            "orders {index}: {case}"
        );
        assert_eq!(
            serde_json::to_value(store.loaded().unwrap().clone()).unwrap(),
            case["loaded"],
            "loaded {index}"
        );
        assert_eq!(
            json!(notifications.get()),
            case["notifications"],
            "notifications {index}"
        );
        assert_eq!(
            Store::update_ids(store.as_ref()).unwrap().clone(),
            serde_json::from_value::<safe_cex_types::StoreUpdateIds>(case["updateIds"].clone())
                .unwrap(),
            "update ids {index}"
        );
        // CHOSEN owned-snapshot-vs-live-alias: mutating the returned TS row
        // is visible through Store; Rust's returned row is an owned snapshot.
        result[0].last_update = 4242.0;
        assert_eq!(
            result[0].last_update,
            case["alias"]["returnedLastUpdate"].as_f64().unwrap()
        );
        let stored = store
            .orders()
            .unwrap()
            .iter()
            .find(|o| o.id == result[0].id)
            .map(|o| o.last_update);
        if let Some(last) = stored {
            assert_eq!(last, 1.0, "owned Rust snapshot {index}");
            assert_eq!(
                case["alias"]["storedLastUpdate"], 4242,
                "live TS alias {index}"
            );
        } else {
            assert!(case["alias"]["storedLastUpdate"].is_null());
        }
        for (name, state) in [
            ("placed", &venue.placed),
            ("canceled", &venue.canceled),
            ("rejected", &venue.rejected),
        ] {
            let expected: BTreeMap<String, f64> =
                serde_json::from_value(case[name].clone()).unwrap();
            assert_eq!(*state.borrow(), expected, "{name} {index}");
        }
        native_comparisons += 1;
    }
    assert_eq!(
        native_comparisons, 112,
        "MEXC reconciliation native case comparisons"
    );
}

#[test]
fn mexc_leverage_override_dispatches_through_dyn_exchange() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_store::StoreUpdate;

    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/leverage_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "live leverage oracle failed: {}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(
        expected["success"]["fetched"]["buyLeverage"], 31.0,
        "TypeScript leverage buy"
    );
    assert_eq!(
        expected["success"]["fetched"]["sellLeverage"], 12.0,
        "TypeScript leverage sell"
    );

    let store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":1,"maxLeverage":20})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut exchange = venue(store.clone(), 0.0, false);
    let adapter = Rc::new(LeverageResponse(RefCell::default()));
    exchange.xhr = adapter.clone();

    let object: &dyn Exchange = &exchange;
    let fetched = unit_tasks::drive(object.fetch_leverage_for_symbol("BTCUSDT".into()))
        .unwrap()
        .expect("MEXC override returns a fetched leverage record");
    assert_eq!(
        fetched.buy_leverage,
        expected["success"]["fetched"]["buyLeverage"]
            .as_f64()
            .unwrap(),
        "override buy leverage"
    );
    assert_eq!(
        fetched.sell_leverage,
        expected["success"]["fetched"]["sellLeverage"]
            .as_f64()
            .unwrap(),
        "override sell leverage"
    );
    assert_eq!(
        fetched.margin_type,
        Some(safe_cex_types::MarginType::Isolated)
    );
    assert_eq!(
        adapter.0.borrow().len(),
        1,
        "override issued the venue request"
    );
    assert_eq!(
        adapter.0.borrow()[0].params,
        Some(json!({"symbol":"BTC_USDT"}))
    );
    assert_eq!(
        store.fetched_leverage().unwrap()["BTCUSDT"].buy_leverage,
        expected["success"]["store"]["BTCUSDT"]["buyLeverage"]
            .as_f64()
            .unwrap()
    );
    assert_eq!(
        store.markets_by_symbol().unwrap()["BTCUSDT"]
            .limits
            .leverage
            .max,
        expected["success"]["market"]["limits"]["leverage"]["max"]
            .as_f64()
            .unwrap()
    );
    assert_eq!(
        expected["metadataCalls"],
        json!(["BTCUSDT"]),
        "TypeScript metadata delegation"
    );
    assert_eq!(
        expected["subscriberThrow"]["rejected"], true,
        "source Store subscriber throw rejects leverage fetch after commit"
    );
    assert_eq!(
        expected["subscriberThrow"]["marketMax"], 60,
        "source Store commits market update before subscriber throw"
    );
    assert_eq!(
        expected["subscriberThrow"]["fetchedBuy"], 21,
        "source fallback commits fetched leverage before subscriber throw"
    );
    assert_eq!(
        expected["subscriberThrow"]["subscriberCalls"],
        json!(["first", "first"]),
        "source subscriber throw skips the second listener on both notifications"
    );
    use safe_cex_types::StoreSubscriberError;
    let throwing_store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":1,"maxLeverage":20})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        throwing_store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let first_calls = calls.clone();
    let _first_unsubscribe = Store::subscribe(
        throwing_store.as_ref(),
        Box::new(move |_, _| {
            first_calls.borrow_mut().push("first");
            Err(StoreSubscriberError::new("subscriber failed"))
        }),
    );
    let second_calls = calls.clone();
    let _second_unsubscribe = Store::subscribe(
        throwing_store.as_ref(),
        Box::new(move |_, _| {
            second_calls.borrow_mut().push("second");
            Ok(())
        }),
    );
    let mut throwing = venue(throwing_store.clone(), 0.0, false);
    throwing.xhr = Rc::new(LeverageResponse(RefCell::default()));
    throwing.symbol_settings.borrow_mut().insert(
        "BTCUSDT".into(),
        MexcSymbolConfig {
            buy_leverage: 21.0,
            sell_leverage: 7.0,
            margin_type: safe_cex_types::MarginType::Cross,
            position_mode: safe_cex_types::PositionMode::Hedged,
        },
    );
    let error =
        unit_tasks::drive((&throwing as &dyn Exchange).fetch_leverage_for_symbol("BTCUSDT".into()))
            .unwrap_err();
    assert_eq!(error.message, "subscriber failed");
    assert_eq!(
        throwing_store.markets_by_symbol().unwrap()["BTCUSDT"]
            .limits
            .leverage
            .max,
        60.0,
        "market update commits before its notification failure"
    );
    assert_eq!(
        throwing_store.fetched_leverage().unwrap()["BTCUSDT"].buy_leverage,
        21.0,
        "caught market notification falls back to the configured leverage"
    );
    assert_eq!(
        *calls.borrow(),
        ["first", "first"],
        "notification failure skips later subscribers on both writes"
    );
    unit_tasks::drive(object.update_symbol_metadata("BTCUSDT".into())).unwrap();
    assert_eq!(
        adapter.0.borrow().len(),
        2,
        "metadata update delegates to leverage fetch"
    );

    let fallback_store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":1,"maxLeverage":20})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        fallback_store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let fallback = venue(fallback_store.clone(), 0.0, true);
    fallback.symbol_settings.borrow_mut().insert(
        "BTCUSDT".into(),
        MexcSymbolConfig {
            buy_leverage: 21.0,
            sell_leverage: 7.0,
            margin_type: safe_cex_types::MarginType::Cross,
            position_mode: safe_cex_types::PositionMode::Hedged,
        },
    );
    let fallback_result =
        unit_tasks::drive((&fallback as &dyn Exchange).fetch_leverage_for_symbol("BTCUSDT".into()))
            .unwrap()
            .unwrap();
    assert_eq!(
        fallback_result.buy_leverage,
        expected["publicFallback"]["fetched"]["buyLeverage"]
            .as_f64()
            .unwrap(),
        "public-only fallback buy leverage"
    );
    assert_eq!(
        fallback_store.fetched_leverage().unwrap()["BTCUSDT"].sell_leverage,
        expected["publicFallback"]["store"]["BTCUSDT"]["sellLeverage"]
            .as_f64()
            .unwrap()
    );

    let empty_store = Rc::new(DefaultStore::new());
    let missing = venue(empty_store.clone(), 0.0, false);
    missing.symbol_settings.borrow_mut().insert(
        "BTCUSDT".into(),
        MexcSymbolConfig {
            buy_leverage: 21.0,
            sell_leverage: 7.0,
            margin_type: safe_cex_types::MarginType::Cross,
            position_mode: safe_cex_types::PositionMode::Hedged,
        },
    );
    let missing_result =
        unit_tasks::drive((&missing as &dyn Exchange).fetch_leverage_for_symbol("BTCUSDT".into()))
            .unwrap()
            .unwrap();
    assert_eq!(
        missing_result.buy_leverage,
        expected["missingMarket"]["fetched"]["buyLeverage"]
            .as_f64()
            .unwrap(),
        "missing-market fallback buy leverage"
    );

    let failed_store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":1,"maxLeverage":20})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        failed_store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut failed = venue(failed_store.clone(), 0.0, false);
    failed.symbol_settings.borrow_mut().insert(
        "BTCUSDT".into(),
        MexcSymbolConfig {
            buy_leverage: 21.0,
            sell_leverage: 7.0,
            margin_type: safe_cex_types::MarginType::Cross,
            position_mode: safe_cex_types::PositionMode::Hedged,
        },
    );
    failed.xhr = Rc::new(LeverageFailure);
    let failed_result =
        unit_tasks::drive((&failed as &dyn Exchange).fetch_leverage_for_symbol("BTCUSDT".into()))
            .unwrap()
            .unwrap();
    assert_eq!(
        failed_result.buy_leverage,
        expected["requestFailure"]["fetched"]["buyLeverage"]
            .as_f64()
            .unwrap(),
        "request failure fallback buy leverage"
    );
    assert_eq!(
        failed_store.fetched_leverage().unwrap()["BTCUSDT"].sell_leverage,
        expected["requestFailure"]["store"]["BTCUSDT"]["sellLeverage"]
            .as_f64()
            .unwrap()
    );
}

#[path = "../tests/support/mod.rs"]
mod support;
struct OrderWire {
    failure: Option<String>,
    calls: RefCell<Vec<Value>>,
}
fn raw_order(id: &str, symbol: &str) -> Value {
    json!({"id":id,"orderId":id,"symbol":symbol,"side":1,"positionType":2,"vol":5,"price":123,"createTime":1700000000000u64})
}
impl RestAdapter for OrderWire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls
                .borrow_mut()
                .push(json!({"url":request.url,"params":request.params}));
            let kind = if request.url.contains("open_orders") {
                "open"
            } else if request.url.contains("planorder") {
                "plan"
            } else {
                "stop"
            };
            if self.failure.as_deref() == Some(kind)
                || self.failure.as_deref() == Some(format!("error-{kind}").as_str())
            {
                let mut error = RestFailure::pre_transmit(format!("failed {kind}"), request);
                error.response = Some(Box::new(RestResponse {
                    status: 400,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(json!({"message": if self.failure.as_deref() == Some(kind) { json!(format!("venue {kind}")) } else {json!({"toString":null})}})
                        .to_string()
                        .into_bytes()),
                }));
                return Err(error);
            }
            let mut rows = match kind {
                "open" => {
                    let mut duplicate = raw_order("101", "BTC_USDT");
                    duplicate["price"] = json!(999);
                    vec![
                        raw_order("101", "BTC_USDT"),
                        duplicate,
                        raw_order("102", "ETH_USDT"),
                    ]
                }
                "plan" => vec![raw_order("201", "BTC_USDT"), raw_order("202", "BTC_USDT")],
                "stop" => vec![raw_order("301", "BTC_USDT")],
                _ => unreachable!(),
            };
            if kind == "open" && self.failure.as_deref() == Some("identity") {
                rows[0]["orderId"] = json!("wire-101");
            }
            if kind == "open" {
                match self.failure.as_deref() {
                    Some("row-null") => rows.push(Value::Null),
                    Some("row-number") => rows.push(json!(7)),
                    Some("row-array") => rows.push(json!([])),
                    Some("row-coercion") => {
                        let mut row = raw_order("bad", "BTC_USDT");
                        row["symbol"] = json!({"toString":null});
                        rows.push(row);
                    }
                    _ => (),
                }
            }
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    json!({"data":{"items":rows}}).to_string().into_bytes(),
                ),
            })
        })
    }
}
#[test]
fn mexc_fetch_orders_live_requests_and_results() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fetch_orders_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 240, "fetchOrders source scenario census");
    let mut native_comparisons = 0;
    for index in 0..240 {
        let case = &cases[index];
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![
                    normalize_market(
                        &serde_json::Value::Object(
                            (case["marketRaw"].as_object().unwrap()).clone(),
                        )
                        .into(),
                    )
                    .unwrap(),
                ]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let now = case["time"].as_f64().unwrap();
        let runtime = Rc::new(Runtime(now, RefCell::default(), Rc::new(Cell::new(0.0))));
        let mut venue = venue(store, now, case["publicOnly"].as_bool().unwrap());
        Rc::get_mut(&mut venue.base).unwrap().runtime = runtime.clone();
        *venue.canceled.borrow_mut() = BTreeMap::from([("201".into(), now)]);
        let wire = Rc::new(OrderWire {
            failure: case["failure"].as_str().map(str::to_owned),
            calls: RefCell::default(),
        });
        venue.xhr = wire.clone();
        let symbols = case
            .get("symbols")
            .map(|s| serde_json::from_value(s.clone()).unwrap());
        let result = unit_tasks::drive(if case["method"] == "fetchOrders" {
            venue.fetch_orders(symbols)
        } else {
            venue.fetch_regular_orders(symbols)
        });
        match result {
            Ok(result) => {
                assert!(case.get("error").is_none());
                if std::env::var_os("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS").is_none() {
                    assert_eq!(
                        result.len(),
                        case["result"].as_array().unwrap().len(),
                        "fetchOrders native row count {index}"
                    );
                }
                if case["failure"] == "identity"
                    && case["result"].as_array().is_some_and(|orders| {
                        orders
                            .first()
                            .is_some_and(|order| order["id"] == "wire-101")
                    })
                {
                    assert_eq!(
                        result.first().map(|order| order.id.as_str()),
                        Some("wire-101"),
                        "MexcOrder.id prefers the distinct raw orderId field"
                    );
                }
                assert_eq!(
                    support::encode(&result),
                    case["result"],
                    "fetched {index}: {case}"
                );
            }
            Err(error) => assert_eq!(
                json!({"name":error.name,"message":error.message}),
                case["error"],
                "thrown {index}: {case}"
            ),
        }
        assert_eq!(
            support::encode(&*wire.calls.borrow()),
            case["calls"],
            "requests {index}"
        );
        assert_eq!(json!(*runtime.1.borrow()), case["logs"], "logs {index}");
        let expected: BTreeMap<String, f64> =
            serde_json::from_value(case["canceled"].clone()).unwrap();
        assert_eq!(*venue.canceled.borrow(), expected, "canceled {index}");
        assert_eq!(support::decode_number(&support::encode(&now)), now);
        native_comparisons += 1;
    }
    assert_eq!(
        native_comparisons, 240,
        "fetchOrders native case comparisons"
    );
}

#[test]
fn mexc_consuming_empty_array_deserializer_control_fails_by_name() {
    let targets = [
        (
            "exchange::tests::mexc_fetch_ohlcv_matches_live_source",
            "fetchOHLCV native candle count: case 0",
            "fetchOHLCV payload",
        ),
        (
            "exchange::tests::mexc_fetch_order_book_matches_live_source",
            "fetchOrderBook bids expected/native level count",
            "fetchOrderBook native level missing",
        ),
        (
            "exchange::tests::mexc_fetch_historical_fills_matches_live_source",
            "historical fill count: full-options",
            "historical fill native row missing",
        ),
        (
            "exchange::tests::mexc_fetch_positions_matches_live_source",
            "fetchPositions native id/order identity case 0",
            "fetchPositions native id/order identity case 0",
        ),
        (
            "exchange::tests::mexc_fetch_orders_live_requests_and_results",
            "fetchOrders native row count",
            "fetched ",
        ),
    ];
    for (target, guarded_assertion, bypassed_assertion) in targets {
        for bypass_lengths in [false, true] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", target, "--nocapture"])
                .env("MEXC_CONSUME_ARRAYS_RUST_CONTROL", "consume-all-then-empty");
            if bypass_lengths {
                command
                    .env("MEXC_CARDINALITY_BYPASS_SOURCE_COUNTS", "1")
                    .env("MEXC_CARDINALITY_BYPASS_NATIVE_LENGTH_CHECKS", "1");
            }
            let output = command.output().unwrap();
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                output.status.code(),
                Some(101),
                "consuming array control target={target} bypass={bypass_lengths}: {text}"
            );
            assert!(
                text.contains(if bypass_lengths {
                    bypassed_assertion
                } else {
                    guarded_assertion
                }),
                "consuming array control target={target} bypass={bypass_lengths}: {text}"
            );
        }
    }
}

#[test]
fn mexc_orders_override_reaches_concrete_reconciliation() {
    for captured in [false, true] {
        let store = Rc::new(DefaultStore::new());
        let preserved = order(
            &json!({"id":"placed","symbol":"BTCUSDT","status":"open","type":"stop_market","lastUpdate":1}),
        );
        Store::update(
            store.as_ref(),
            StoreUpdate {
                orders: Some(vec![preserved]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let mut venue = venue(store.clone(), 1000.0, false);
        venue.xhr = Rc::new(OrderWire {
            failure: None,
            calls: RefCell::default(),
        });
        let response = if captured {
            let method = <MexcExchange as Exchange>::fetch_and_set_orders;
            unit_tasks::drive(method(&venue, Some(vec![])))
        } else {
            let erased: &dyn Exchange = &venue;
            unit_tasks::drive(erased.fetch_and_set_orders(None))
        }
        .unwrap();
        assert!(!response.iter().any(|o| o.id == "placed"));
        assert!(
            store.orders().unwrap().iter().any(|o| o.id == "placed"),
            "concrete MEXC placement shield override must run, captured={captured}"
        );
        assert!(
            store.loaded().unwrap().orders,
            "empty scope must dispatch full MEXC reconciliation"
        );
    }
}

#[test]
fn mexc_reconciliation_drop_leaves_state_recoverable() {
    use std::task::{Context, Poll};
    let store = Rc::new(DefaultStore::new());
    let existing = order(
        &json!({"id":"placed","symbol":"BTCUSDT","status":"open","type":"stop_market","lastUpdate":1}),
    );
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![existing]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let venue = venue(store.clone(), 1000.0, false);
    let before = store.serialize().unwrap();
    for poll in [false, true] {
        let fetch = Box::pin(futures::future::pending());
        let mut future = venue.reconcile_orders(None, fetch, false);
        if poll {
            assert!(matches!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(futures::task::noop_waker_ref())),
                Poll::Pending
            ));
        }
        drop(future);
        assert_eq!(store.serialize().unwrap(), before);
        assert_eq!(venue.placed.borrow().len(), 2);
    }
    // A canceled caller must not leave a gate that blocks the next snapshot.
    let next = Box::pin(async { Ok(vec![]) });
    unit_tasks::drive(venue.reconcile_orders(None, next, false)).unwrap();
    assert!(store.loaded().unwrap().orders);
    assert_eq!(store.orders().unwrap()[0].id, "placed");
}

#[test]
fn mexc_reconciliation_typescript_control_is_observed() {
    child_control(
        "exchange::tests::mexc_reconciliation_live_store_transitions",
        "snapshot-disposal",
    );
    child_control(
        "exchange::tests::mexc_fetch_orders_live_requests_and_results",
        "fetch-dedupe",
    );
}
fn child_control(name: &str, control: &str) {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("MEXC_TS_CONTROL", control)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!("{name} ... FAILED")), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("assertion `left == right` failed"),
        "{stderr}"
    );
}

struct CancelTransportFailure(Rc<Cell<usize>>);
impl RestAdapter for CancelTransportFailure {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.0.set(self.0.get() + 1);
        let mut failure = RestFailure::pre_transmit("transport failure".into(), request);
        failure.code = Some("ERR_NETWORK".into());
        failure.response = Some(Box::new(RestResponse {
            status: 401,
            reason: Some("Unauthorized".into()),
            headers: Default::default(),
            body: RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    json!({"message":"response detail"}),
                )
                .unwrap(),
            ),
        }));
        Box::pin(async move { Err(failure) })
    }
}

#[test]
fn mexc_cancel_transport_projection_finding() {
    let store = Rc::new(DefaultStore::new());
    let cancel = cancel_order(
        &json!({"id":"regular__buy","symbol":"BTCUSDT","type":"limit","amount":1,"remaining":1}),
    );
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![cancel.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let mut exchange = venue(store.clone(), 123456.0, false);
    let calls = Rc::new(Cell::new(0));
    exchange.xhr = Rc::new(CancelTransportFailure(calls.clone()));
    let pending = Exchange::cancel_orders(&exchange, vec![cancel]);
    assert_eq!(
        calls.get(),
        0,
        "CHOSEN native cancellation lazy POST prefix"
    );
    let error = unit_tasks::drive(pending).unwrap_err();
    assert_eq!(
        calls.get(),
        1,
        "MEXC cancellation native request-call floor"
    );
    assert_eq!(
        error.message, "transport failure",
        "MEXC cancellation native original transport message"
    );

    let origin = std::error::Error::source(&error)
        .unwrap()
        .downcast_ref::<RestFailure>()
        .expect("MEXC cancellation typed REST origin");
    assert_eq!(
        origin.request.method, "post",
        "MEXC cancellation native original method"
    );
    assert_eq!(
        origin.request.url, "/api/v1/private/order/cancel",
        "MEXC cancellation native original URL"
    );
    let Some(safe_cex_transport::RestBody::Untransformed(body)) = &origin.request.body else {
        panic!("MEXC cancellation native untransformed request")
    };
    assert_eq!(
        body,
        &safe_cex_types::retained_json::RetainedJsonInput::from_value(json!(["regular"])).unwrap(),
        "MEXC cancellation native original IDs"
    );
    assert_eq!(
        origin.response.as_ref().unwrap().status,
        401,
        "MEXC cancellation native original response"
    );
    assert_eq!(
        store.orders().unwrap()[0].status,
        OrderStatus::Open,
        "MEXC transport rejection leaves native order open"
    );
    assert_eq!(
        observed_error_code(&error.code),
        Some(json!("ERR_NETWORK")),
        "MEXC cancellation native code retained"
    );
    assert_eq!(
        error.response_json_view().as_ref().unwrap()["data"]["message"],
        "response detail",
        "MEXC cancellation native response retained"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/cancel_orders_oracle.mjs"
        ))
        .arg("--transport-finding")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC transport finding oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["prefixCalls"], 1,
        "SOURCE cancellation eager POST prefix"
    );
    assert_eq!(
        source["message"], "transport failure",
        "MEXC cancellation source original message"
    );
    assert_eq!(
        source["response"]["data"]["message"], "response detail",
        "MEXC cancellation source response retained"
    );
    assert_eq!(
        source["code"], "ERR_NETWORK",
        "MEXC cancellation source code retained"
    );

    assert_eq!(
        source["status"], "open",
        "MEXC transport rejection leaves source order open"
    );
    assert_eq!(
        source["calls"].as_array().unwrap().len(),
        1,
        "MEXC cancellation source transport call count"
    );
}

struct RetainedRestRejection {
    producer: Rc<RefCell<RestFailure>>,
    calls: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for RetainedRestRejection {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls.borrow_mut().push(request.clone());
        let mut failure = self.producer.borrow().clone();
        failure.request = Box::new(request);
        Box::pin(async move { Err(failure) })
    }
}
#[test]
fn mexc_uncaught_rest_rejections_retain_native_origins() {
    use std::error::Error;
    for (name, url) in [
        ("fetchBalance", "/api/v1/private/account/assets"),
        (
            "fetchPositionMode",
            "/api/v1/private/position/position_mode",
        ),
    ] {
        let mut failure =
            RestFailure::pre_transmit("transport failure".into(), RestRequest::default());
        failure.code = Some("ERR_NETWORK".into());
        failure.context = Some(Box::new(json!({"phase":"native-origin"})));
        failure.response = Some(Box::new(RestResponse {
            status: 401,
            reason: Some("Unauthorized".into()),
            headers: Default::default(),
            body: RestResponseBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    json!({"message":"response detail","marker":7}),
                )
                .unwrap(),
            ),
        }));
        let producer = Rc::new(RefCell::new(failure));
        let adapter = Rc::new(RetainedRestRejection {
            producer: producer.clone(),
            calls: RefCell::new(Vec::new()),
        });
        let mut exchange = venue(Rc::new(DefaultStore::new()), 123456.0, false);
        exchange.xhr = adapter.clone();
        let error = if name == "fetchBalance" {
            let pending = exchange.fetch_balance();
            assert_eq!(
                adapter.calls.borrow().len(),
                0,
                "CHOSEN native balance lazy GET prefix"
            );
            unit_tasks::drive(pending).unwrap_err()
        } else {
            let pending = exchange.fetch_position_mode();
            assert_eq!(
                adapter.calls.borrow().len(),
                0,
                "CHOSEN native mode lazy GET prefix"
            );
            unit_tasks::drive(pending).unwrap_err()
        };
        assert_eq!(
            error.message, "transport failure",
            "MEXC REST native original message {name}"
        );
        let original = error
            .source()
            .unwrap()
            .downcast_ref::<RestFailure>()
            .expect("MEXC REST native typed failure");
        assert_eq!(
            original.request.method, "get",
            "MEXC REST native request method {name}"
        );
        assert_eq!(
            original.request.url, url,
            "MEXC REST native request URL {name}"
        );
        let response = original.response.as_ref().unwrap();
        assert_eq!(
            response.status, 401,
            "MEXC REST native response status {name}"
        );
        assert_eq!(
            response.reason.as_deref(),
            Some("Unauthorized"),
            "MEXC REST native response reason {name}"
        );
        let RestResponseBody::Untransformed(data) = &response.body else {
            panic!("MEXC REST native phase")
        };
        assert_eq!(
            data.clone().into_value().unwrap()["message"],
            "response detail",
            "MEXC REST native response payload {name}"
        );
        assert_eq!(
            data.clone().into_value().unwrap()["marker"],
            7,
            "MEXC REST native original marker {name}"
        );
        assert_eq!(
            adapter.calls.borrow().len(),
            1,
            "MEXC REST independent request-call floor {name}"
        );
        assert_eq!(
            original.code.as_deref(),
            Some("ERR_NETWORK"),
            "MEXC REST retained native code {name}"
        );
        assert_eq!(
            original.context.as_deref().unwrap()["phase"],
            "native-origin",
            "MEXC REST retained native context {name}"
        );
        // Existing adapter ownership is retained, not relabelled as a live JS alias.
        producer.borrow_mut().response.as_mut().unwrap().body = RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(
                json!({"message":"response detail","marker":42}),
            )
            .unwrap(),
        );
        assert_eq!(
            data.clone().into_value().unwrap()["marker"],
            7,
            "MEXC REST chosen native response snapshot {name}"
        );
        assert_eq!(
            producer
                .borrow()
                .response
                .as_ref()
                .unwrap()
                .body
                .json_value()
                .unwrap()["marker"],
            42,
            "MEXC REST producer mutation witness {name}"
        );
        assert_eq!(
            observed_error_code(&error.code),
            Some(json!("ERR_NETWORK")),
            "MEXC REST native code {name}"
        );
        assert_eq!(
            error.context.as_ref().unwrap()["phase"],
            "native-origin",
            "MEXC REST native context {name}"
        );
        assert_eq!(
            error.response_json_view().as_ref().unwrap()["data"]["marker"],
            7,
            "MEXC REST metadata snapshot view {name}"
        );
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest_rejection_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC REST actual source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 2, "MEXC REST source method floor");
    for (row, (name, url)) in source.iter().zip([
        ("fetchBalance", "/api/v1/private/account/assets"),
        (
            "fetchPositionMode",
            "/api/v1/private/position/position_mode",
        ),
    ]) {
        assert_eq!(row["method"], name, "MEXC REST source method");
        assert_eq!(row["url"], url, "MEXC REST source URL");
        assert_eq!(
            row["message"], "transport failure",
            "MEXC REST source original message"
        );
        assert_eq!(row["code"], "ERR_NETWORK", "MEXC REST source original code");
        assert_eq!(row["status"], 401, "MEXC REST source original status");
        assert_eq!(
            row["marker"], 42,
            "MEXC REST source live response alias finding"
        );
        assert_eq!(
            row["calls"], 1,
            "MEXC REST source independent request calls"
        );
        assert_eq!(row["prefixCalls"], 1, "SOURCE REST eager GET prefix");
    }
}

#[derive(Debug)]
struct MetadataSubscriberOrigin(u32);
impl std::fmt::Display for MetadataSubscriberOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(if self.0 == 401 {
            "metadata first"
        } else {
            "metadata second"
        })
    }
}
impl std::error::Error for MetadataSubscriberOrigin {}
#[test]
fn mexc_metadata_wrapper_keeps_second_subscriber_origin_and_eager_prefix() {
    use std::error::Error;
    for public_only in [false, true] {
        let store = Rc::new(DefaultStore::new());
        let market = normalize_market(
            &serde_json::Value::Object(
                (json!({"symbol":"BTC_USDT","contractSize":1,"maxLeverage":20})
                    .as_object()
                    .unwrap())
                .clone(),
            )
            .into(),
        )
        .unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let originals = [
            Rc::new(MetadataSubscriberOrigin(401)),
            Rc::new(MetadataSubscriberOrigin(402)),
        ];
        let first = Rc::new(Cell::new(0));
        let later = Rc::new(Cell::new(0));
        let calls = first.clone();
        let causes = originals.clone();
        let _first = Store::subscribe(
            store.as_ref(),
            Box::new(move |data, _| {
                let index = calls.get();
                calls.set(index + 1);
                if index == 0 && !public_only {
                    assert_eq!(
                        data.markets[0].limits.leverage.max.to_bits(),
                        60_f64.to_bits(),
                        "metadata first native callback sees committed cap"
                    );
                    assert!(
                        !data.fetched_leverage.contains_key("BTCUSDT"),
                        "metadata first native callback precedes fallback write"
                    );
                } else {
                    assert_eq!(
                        data.fetched_leverage["BTCUSDT"].buy_leverage.to_bits(),
                        21_f64.to_bits(),
                        "metadata final native callback sees committed fallback"
                    );
                }
                Err(StoreSubscriberError::new(causes[index].to_string())
                    .with_cause(causes[index].clone()))
            }),
        );
        let calls = later.clone();
        let _later = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                calls.set(calls.get() + 1);
                Ok(())
            }),
        );
        let (mut exchange, runtime) = venue_with_runtime(store.clone(), 0.0, public_only);
        let adapter = Rc::new(LeverageResponse(RefCell::new(Vec::new())));
        exchange.xhr = adapter.clone();
        exchange.symbol_settings.borrow_mut().insert(
            "BTCUSDT".into(),
            MexcSymbolConfig {
                buy_leverage: 21.0,
                sell_leverage: 7.0,
                margin_type: MarginType::Cross,
                position_mode: PositionMode::Hedged,
            },
        );
        let pending = Exchange::update_symbol_metadata(&exchange, "BTCUSDT".into());
        assert_eq!(
            first.get(),
            if public_only { 1 } else { 0 },
            "metadata source eager subscriber prefix"
        );
        assert_eq!(later.get(), 0, "metadata native no later prefix subscriber");
        assert_eq!(
            adapter.0.borrow().len(),
            if public_only { 0 } else { 1 },
            "metadata source eager request prefix"
        );
        assert!(
            store.fetched_leverage().unwrap().contains_key("BTCUSDT") == public_only,
            "metadata source only public fallback commits before awaiting HTTP"
        );
        assert!(
            runtime.1.borrow().is_empty(),
            "metadata native no prefix log"
        );
        let error = unit_tasks::drive(pending)
            .expect_err("metadata native wrapper propagates original final failure");
        let expected_calls = if public_only { 1 } else { 2 };
        assert_eq!(
            first.get(),
            expected_calls,
            "metadata independent native subscriber calls"
        );
        assert_eq!(
            later.get(),
            0,
            "metadata native aborts later subscriber on every write"
        );
        assert_eq!(
            adapter.0.borrow().len(),
            if public_only { 0 } else { 1 },
            "metadata independent native request calls"
        );
        assert_eq!(
            store.markets_by_symbol().unwrap()["BTCUSDT"]
                .limits
                .leverage
                .max
                .to_bits(),
            if public_only { 20_f64 } else { 60_f64 }.to_bits(),
            "metadata native committed market cap"
        );
        let fetched = &store.fetched_leverage().unwrap()["BTCUSDT"];
        assert_eq!(
            fetched.buy_leverage.to_bits(),
            21_f64.to_bits(),
            "metadata native committed fallback buy"
        );
        assert_eq!(
            fetched.sell_leverage.to_bits(),
            7_f64.to_bits(),
            "metadata native committed fallback sell"
        );
        assert_eq!(
            fetched.margin_type,
            Some(MarginType::Cross),
            "metadata native fallback margin enum"
        );
        assert_eq!(
            fetched.position_mode,
            Some(PositionMode::Hedged),
            "metadata native fallback mode enum"
        );
        assert_eq!(
            error.message,
            if public_only {
                "metadata first"
            } else {
                "metadata second"
            },
            "metadata native final error message"
        );
        assert_eq!(
            error.store_subscriber_ids,
            [0],
            "metadata native final subscriber identity"
        );
        assert!(
            error.code.is_none(),
            "CHOSEN native opaque fields live in the source chain"
        );
        let logs = runtime.1.borrow();
        assert_eq!(
            logs.len(),
            if public_only { 0 } else { 1 },
            "metadata independent native warning-log calls"
        );
        if !public_only {
            assert_eq!(
                logs[0]["message"], "Failed to fetch MEXC leverage cap for BTCUSDT: metadata first",
                "metadata native warning-log message"
            );
            assert_eq!(
                logs[0]["severity"], "warning",
                "metadata native warning-log severity"
            );
        }
        let opaque = error
            .source()
            .unwrap()
            .downcast_ref::<MetadataSubscriberOrigin>()
            .expect("metadata native source is the original subscriber cause");
        assert_eq!(
            opaque.0,
            if public_only { 401 } else { 402 },
            "metadata native original final cause code"
        );
        assert!(
            std::ptr::eq(opaque, originals[expected_calls - 1].as_ref()),
            "metadata native original final cause identity"
        );
        if !public_only {
            assert!(
                !std::ptr::eq(opaque, originals[0].as_ref()),
                "metadata first cause must not escape instead of second"
            );
        }
    }
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/metadata_rejection_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "metadata actual wrapper/child Source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 2, "metadata Source two root floor");
    for (index, public_only) in [false, true].into_iter().enumerate() {
        let row = &rows[index];
        assert_eq!(row["publicOnly"], public_only, "metadata Source root order");
        assert_eq!(
            row["prefix"]["first"],
            if public_only { 1 } else { 0 },
            "CHOSEN metadata Source eager subscriber prefix"
        );
        assert_eq!(
            row["prefix"]["requests"],
            if public_only { 0 } else { 1 },
            "CHOSEN metadata Source eager request prefix"
        );
        assert_eq!(
            row["first"],
            if public_only { 1 } else { 2 },
            "metadata Source independent first subscriber calls"
        );
        assert_eq!(row["later"], 0, "metadata Source aborts later subscriber");
        assert_eq!(
            row["requests"],
            if public_only { 0 } else { 1 },
            "metadata Source independent final request calls"
        );
        assert_eq!(
            row["logs"].as_array().unwrap().len(),
            if public_only { 0 } else { 1 },
            "metadata Source independent warning-log calls"
        );
        if !public_only {
            assert_eq!(
                row["logs"][0][0], "Failed to fetch MEXC leverage cap for BTCUSDT: metadata first",
                "metadata Source warning-log message"
            );
            assert_eq!(
                row["logs"][0][1], "warning",
                "metadata Source warning-log severity"
            );
        }
        assert_eq!(
            row["code"],
            if public_only { 401 } else { 402 },
            "metadata Source original final code"
        );
        assert_eq!(
            row["max"],
            if public_only { 20 } else { 60 },
            "metadata Source committed market cap"
        );
        assert_eq!(row["buy"], 21, "metadata Source committed fallback buy");
        assert_eq!(row["sell"], 7, "metadata Source committed fallback sell");
    }
}

#[test]
fn mexc_history_keeps_native_query_numbers_before_rendering() {
    use safe_cex_types::QueryValue;
    let adapter = Rc::new(StaticResponse(
        br#"{"data":[]}"#.to_vec(),
        RefCell::new(Vec::new()),
    ));
    let mut exchange = venue(Rc::new(DefaultStore::new()), 123.25, false);
    exchange.xhr = adapter.clone();
    let future = exchange.fetch_historical_fills(ExecutionsOptions {
        symbol: Some("BTCUSDT".into()),
        from: Some(f64::NAN),
        to: Some(f64::NEG_INFINITY),
        order_id: None,
        limit: None,
    });
    assert_eq!(
        adapter.1.borrow().len(),
        0,
        "history native request prefix before polling"
    );
    let fills = unit_tasks::drive(future).unwrap();
    assert_eq!(
        fills.len(),
        0,
        "history native independent empty result count"
    );
    let requests = adapter.1.borrow();
    assert_eq!(
        requests.len(),
        1,
        "history native independent request count"
    );
    let QueryValue::Object(params) = requests[0]
        .query_params
        .as_ref()
        .expect("history native rich query carrier before rendering")
    else {
        panic!("history native query object before rendering");
    };
    assert_eq!(
        params
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["symbol", "start_time", "end_time", "page_num", "page_size"],
        "history native original parameter order before rendering"
    );
    let QueryValue::Number(from) = params.get("start_time").unwrap() else {
        panic!("history native Number start_time");
    };
    assert!(from.is_nan(), "history native NaN before rendering");
    let QueryValue::Number(to) = params.get("end_time").unwrap() else {
        panic!("history native Number end_time");
    };
    assert_eq!(
        *to,
        f64::NEG_INFINITY,
        "history native negative infinity before rendering"
    );
    assert!(
        requests[0].params.is_none(),
        "history native absence of legacy JSON parameter projection"
    );
}

#[path = "../tests/support/history_store.rs"]
#[allow(dead_code)]
mod history_store;

#[test]
fn mexc_history_reads_markets_per_row_and_preserves_error_precedence() {
    use history_store::ObservedStore;
    let row =
        json!({"symbol":"BTC_USDT","vol":10,"price":3,"createTime":1700000000000_i64,"role":1});
    let mut bad_asset = row.clone();
    bad_asset["feeCurrency"] = json!(42);
    let mut bad_profit = bad_asset.clone();
    bad_profit["profit"] = json!({"toString":0,"valueOf":0});
    let mut bad_role = bad_asset.clone();
    bad_role["role"] = json!({"toString":0,"valueOf":0});
    let specs = [
        (
            "fresh-markets",
            vec![
                row.clone(),
                json!({"symbol":"BTC_USDT","vol":20,"price":3,"createTime":1700000000000_i64,"role":1}),
            ],
            2,
        ),
        ("empty", vec![], 0),
        ("null", vec![Value::Null], 1),
        (
            "coercion",
            vec![json!({"symbol":{"toString":0,"valueOf":0}})],
            1,
        ),
        (
            "primitives",
            vec![json!(false), json!(0), json!("row"), json!([])],
            4,
        ),
        ("commission-number", vec![bad_asset.clone()], 1),
        (
            "commission-then-null",
            vec![bad_asset.clone(), Value::Null],
            2,
        ),
        (
            "commission-then-coercion",
            vec![
                bad_asset.clone(),
                json!({"symbol":{"toString":0,"valueOf":0}}),
            ],
            2,
        ),
        ("commission-profit-coercion", vec![bad_profit], 1),
        ("commission-role-coercion", vec![bad_role], 1),
        ("unicode", vec![], 0),
        (
            "scalar-bmp",
            vec![{
                let mut v = row.clone();
                v["symbol"] = json!("é");
                v
            }],
            1,
        ),
        (
            "scalar-astral",
            vec![{
                let mut v = row.clone();
                v["symbol"] = json!("😃");
                v
            }],
            1,
        ),
        (
            "scalar-space",
            vec![{
                let mut v = row.clone();
                v["symbol"] = json!("a\u{FEFF}\u{3000}b");
                v
            }],
            1,
        ),
    ];
    let mut native = Vec::new();
    let mut total_getters = 0;
    for (label, rows, wanted_reads) in specs {
        let store = Rc::new(ObservedStore::new());
        let market = normalize_market(
            &serde_json::Value::Object(
                (json!({"symbol":"BTC_USDT","contractSize":0.1})
                    .as_object()
                    .unwrap())
                .clone(),
            )
            .into(),
        )
        .unwrap();
        Store::update(
            &store.inner,
            StoreUpdate {
                markets: Some(vec![market.clone()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        if label == "fresh-markets" {
            let mut changed = market;
            changed.contract_size = Some(0.3);
            *store.second_market.borrow_mut() = Some(changed);
        }
        let body = if label == "unicode" {
            br#"{"data":[{"symbol":"\ud800","vol":10,"price":3,"createTime":1700000000000,"role":1}]}"#.to_vec()
        } else {
            serde_json::to_vec(&json!({"data":rows})).unwrap()
        };
        let adapter = Rc::new(StaticResponse(body, RefCell::new(vec![])));
        let mut exchange = venue(Rc::new(DefaultStore::new()), 123.25, false);
        Rc::get_mut(&mut exchange.base).unwrap().store = store.clone();
        exchange.xhr = adapter.clone();
        let result = unit_tasks::drive(exchange.fetch_historical_fills(ExecutionsOptions {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from: None,
            to: None,
            limit: None,
        }));
        assert_eq!(
            adapter.1.borrow().len(),
            1,
            "history native independent row-method GET count {label}"
        );
        assert_eq!(
            store.market_reads.get(),
            wanted_reads,
            "history native row-local getter count {label}"
        );
        total_getters += store.market_reads.get();
        match label {
            "fresh-markets" => {
                let fills = result.as_ref().unwrap();
                assert_eq!(
                    fills.len(),
                    2,
                    "history native independent fresh fill count"
                );
                assert_eq!(
                    fills.iter().map(|fill| fill.qty).collect::<Vec<_>>(),
                    [1.0, 6.0],
                    "history native fresh row-local quantities"
                );
            }
            "empty" | "primitives" => assert_eq!(
                result.as_ref().unwrap().len(),
                0,
                "history native independent no-fill count {label}"
            ),
            "null" | "commission-then-null" => {
                let error = result.as_ref().unwrap_err();
                assert_eq!(
                    error.name, "TypeError",
                    "history native null error name {label}"
                );
                assert_eq!(
                    error.message, "Cannot read properties of null (reading 'symbol')",
                    "history native null error precedence {label}"
                );
            }
            "coercion"
            | "commission-then-coercion"
            | "commission-profit-coercion"
            | "commission-role-coercion" => {
                let error = result.as_ref().unwrap_err();
                assert_eq!(
                    error.name, "TypeError",
                    "history native coercion name {label}"
                );
                assert_eq!(
                    error.message, "Cannot convert object to primitive value",
                    "history native coercion precedence {label}"
                );
            }
            "commission-number" => {
                let error = result.as_ref().unwrap_err();
                assert_eq!(
                    error.name, "TypeError",
                    "history native chosen asset refusal name"
                );
                assert_eq!(
                    error.message,
                    "MEXC commissionAsset is outside the shared Execution string boundary",
                    "history native chosen asset refusal"
                );
            }
            "unicode" => assert_eq!(
                result.as_ref().unwrap_err().name,
                "SyntaxError",
                "history native whole-response UTF16 refusal after one GET, before any Store getter"
            ),
            "scalar-bmp" | "scalar-astral" | "scalar-space" => {
                let fills = result.as_ref().unwrap();
                assert_eq!(fills.len(), 1, "history native scalar output count");
                assert_eq!(fills[0].qty, 10.0, "history native scalar output quantity");
                let wanted: &[u16] = match label {
                    "scalar-bmp" => &[233],
                    "scalar-astral" => &[55357, 56835],
                    _ => &[97, 65279, 12288, 98],
                };
                assert_eq!(
                    fills[0].symbol.encode_utf16().collect::<Vec<_>>(),
                    wanted,
                    "history native decoded scalar Execution symbol units"
                );
            }
            _ => unreachable!(),
        }
        native.push((label, result));
    }
    assert_eq!(
        native.len(),
        14,
        "history native independent method root floor"
    );
    assert_eq!(total_getters, 18, "history native independent getter floor");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/history_rows_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        14,
        "history independent Source method root floor"
    );
    for ((label, result), source) in native.iter().zip(&source) {
        assert_eq!(source["label"], *label, "history Source label");
        assert_eq!(source["gets"], 1, "history Source independent GET count");
        if *label == "unicode" {
            assert_eq!(
                source["symbolUnits"],
                json!([55296]),
                "history Source actual Execution lone-surrogate symbol versus native whole-response refusal"
            );
            assert_eq!(
                source["reads"], 1,
                "history Source getter before UTF16 output"
            );
            continue;
        }
        if *label == "commission-number" {
            assert_eq!(
                source["commission"], 42,
                "history Source actual non-string asset, chosen native whole-record refusal"
            );
            continue;
        }
        if label.starts_with("scalar-") {
            let symbol = &result.as_ref().unwrap()[0].symbol;
            assert_eq!(
                source["symbolUnits"],
                json!(symbol.encode_utf16().collect::<Vec<_>>()),
                "history Source/native scalar Execution symbol units {label}"
            );
        }
        match result {
            Ok(fills) => assert_eq!(
                fills.iter().map(|fill| fill.qty).collect::<Vec<_>>(),
                source["qty"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|value| value.as_f64().unwrap())
                    .collect::<Vec<_>>(),
                "history Source quantity {label}"
            ),
            Err(error) => {
                assert_eq!(
                    source["error"]["name"], error.name,
                    "history Source error name {label}"
                );
                assert_eq!(
                    source["error"]["message"], error.message,
                    "history Source error message {label}"
                );
            }
        }
    }
}

#[test]
fn mexc_history_rejection_keeps_native_request_and_typed_origin() {
    use safe_cex_types::QueryValue as Q;
    use std::error::Error;
    let mut failure = RestFailure::pre_transmit("history down".into(), RestRequest::default());
    failure.code = Some("HISTORY_FAILED".into());
    failure.response = Some(Box::new(RestResponse {
        status: 418,
        reason: None,
        headers: Default::default(),
        body: RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"marker":7}))
                .unwrap(),
        ),
    }));
    let producer = Rc::new(RefCell::new(failure));
    let adapter = Rc::new(RetainedRestRejection {
        producer: producer.clone(),
        calls: RefCell::new(vec![]),
    });
    let mut exchange = venue(Rc::new(DefaultStore::new()), 123.25, false);
    exchange.xhr = adapter.clone();
    let pending = exchange.fetch_historical_fills(ExecutionsOptions {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from: Some(f64::NAN),
        to: Some(-0.0),
        limit: None,
    });
    assert_eq!(
        adapter.calls.borrow().len(),
        0,
        "history rejected native lazy request prefix"
    );
    let error = unit_tasks::drive(pending).unwrap_err();
    assert_eq!(
        error.name, "Error",
        "history rejected native original error name"
    );
    assert_eq!(
        error.message, "history down",
        "history rejected native original error message"
    );
    assert_eq!(
        adapter.calls.borrow().len(),
        1,
        "history rejected independent native request floor"
    );
    let original = error
        .source()
        .expect("history rejected retained native error origin")
        .downcast_ref::<RestFailure>()
        .expect("history rejected retained native RestFailure");
    assert_eq!(
        original.code.as_deref(),
        Some("HISTORY_FAILED"),
        "history rejected native original code"
    );
    let response = original.response.as_ref().unwrap();
    assert_eq!(
        response.status, 418,
        "history rejected native original response status"
    );
    let RestResponseBody::Untransformed(data) = &response.body else {
        panic!("history rejected native original body phase");
    };
    assert_eq!(
        data.clone().into_value().unwrap()["marker"],
        7,
        "history rejected native original body field"
    );
    let Some(Q::Object(params)) = &original.request.query_params else {
        panic!("history rejected native retained rich query");
    };
    assert_eq!(
        params
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["symbol", "start_time", "end_time", "page_num", "page_size"],
        "history rejected native original query order"
    );
    assert!(
        matches!(params.get("start_time"),Some(Q::Number(value)) if value.is_nan()),
        "history rejected native original NaN"
    );
    assert!(
        matches!(params.get("end_time"),Some(Q::Number(value)) if value.to_bits()==(-0.0f64).to_bits()),
        "history rejected native original signed-zero bits"
    );
    assert!(
        original.request.params.is_none(),
        "history rejected native absent legacy projection"
    );
    producer.borrow_mut().response.as_mut().unwrap().body = RestResponseBody::Untransformed(
        safe_cex_types::retained_json::RetainedJsonInput::from_value(json!({"marker":42})).unwrap(),
    );
    assert_eq!(
        data.clone().into_value().unwrap()["marker"],
        7,
        "history rejected chosen owned native error response snapshot"
    );
    assert_eq!(
        producer
            .borrow()
            .response
            .as_ref()
            .unwrap()
            .body
            .json_value()
            .unwrap()["marker"],
        42,
        "history rejected native producer mutation committed"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/history_rows_oracle.mjs"
        ))
        .arg("--rejection")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        1,
        "history rejected independent Source method root floor"
    );
    let source = &source[0];
    assert_eq!(source["gets"], 1, "history rejected Source GET floor");
    assert_eq!(source["reads"], 0, "history rejected Source getter floor");
    assert_eq!(
        source["error"]["name"], error.name,
        "history rejected Source name"
    );
    assert_eq!(
        source["error"]["message"], error.message,
        "history rejected Source message"
    );
    assert_eq!(
        source["rejection"]["same"], true,
        "history rejected Source original error identity"
    );
    assert_eq!(
        source["rejection"]["code"], "HISTORY_FAILED",
        "history rejected Source original code"
    );
    assert_eq!(
        source["rejection"]["status"], 418,
        "history rejected Source original status"
    );
    assert_eq!(
        source["rejection"]["marker"], 42,
        "history rejected Source live response alias versus native owned snapshot"
    );
}

#[test]
fn mexc_history_uses_configured_local_date_conversion() {
    let store = Rc::new(DefaultStore::new());
    let market = normalize_market(
        &serde_json::Value::Object(
            (json!({"symbol":"BTC_USDT","contractSize":0.1})
                .as_object()
                .unwrap())
            .clone(),
        )
        .into(),
    )
    .unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let adapter=Rc::new(StaticResponse(br#"{"data":[{"symbol":"BTC_USDT","vol":10,"price":3,"createTime":"2024-01-02T03:04:05","role":1}]}"#.to_vec(),RefCell::new(vec![])));
    let arguments = Rc::new(RefCell::new(Vec::new()));
    let recorded = arguments.clone();
    let mut exchange = venue(store, 123.25, false);
    exchange.xhr = adapter.clone();
    exchange.local_to_utc = Rc::new(move |value| {
        recorded.borrow_mut().push(value);
        value - 7_200_000.0
    });
    let pending = exchange.fetch_historical_fills(ExecutionsOptions {
        symbol: Some("BTCUSDT".into()),
        order_id: None,
        from: None,
        to: None,
        limit: None,
    });
    assert_eq!(
        adapter.1.borrow().len(),
        0,
        "history date native lazy GET prefix"
    );
    assert_eq!(
        arguments.borrow().len(),
        0,
        "history date native host prefix"
    );
    let fills = unit_tasks::drive(pending).unwrap();
    assert_eq!(fills.len(), 1, "history date independent native fill floor");
    assert_eq!(
        adapter.1.borrow().len(),
        1,
        "history date independent native GET floor"
    );
    assert_eq!(
        *arguments.borrow(),
        [1_704_164_645_000.0],
        "history native configured local-to-UTC call floor and civil-time argument"
    );
    assert_eq!(
        fills[0].timestamp.to_bits(),
        1_704_157_445.0f64.to_bits(),
        "history native fixed UTC+02 decoded timestamp"
    );
    assert_eq!(
        fills[0].qty.to_bits(),
        1.0f64.to_bits(),
        "history native date decoded quantity"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/history_rows_oracle.mjs"
        ))
        .arg("--local-date")
        .env("TZ", "Etc/GMT-2")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        1,
        "history date independent Source method root floor"
    );
    assert_eq!(source[0]["gets"], 1, "history date Source GET floor");
    assert_eq!(source[0]["reads"], 1, "history date Source getter floor");
    assert_eq!(
        source[0]["dateArguments"],
        json!(["2024-01-02T03:04:05"]),
        "history date Source actual Date.parse argument"
    );
    assert_eq!(
        source[0]["timestamp"].as_f64().unwrap().to_bits(),
        fills[0].timestamp.to_bits(),
        "history date Source/native decoded timestamp"
    );
}

#[test]
fn mexc_positions_reads_each_market_and_keeps_symbol_only_caller_lookup() {
    use history_store::ObservedStore;
    let base = json!({"symbol":"BTC_USDT","holdVol":10,"holdAvgPrice":11,
        "positionType":1,"leverage":3,"createTime":1700000000000_i64});
    let mut contract_only = base.clone();
    contract_only
        .as_object_mut()
        .unwrap()
        .shift_remove("symbol");
    contract_only["contract"] = json!("BTC_USDT");
    let mut symbol_null = contract_only.clone();
    symbol_null["symbol"] = Value::Null;
    let mut symbol_empty = base.clone();
    symbol_empty["symbol"] = json!("");
    let mut short = base.clone();
    short["positionType"] = json!(2);
    let cases: [(&str, Vec<Value>, usize, &[f64]); 6] = [
        ("symbol", vec![base.clone()], 2, &[1.0, 0.0]),
        ("contract-only", vec![contract_only], 2, &[10.0, 0.0]),
        ("symbol-null", vec![symbol_null], 2, &[10.0, 0.0]),
        ("symbol-empty", vec![symbol_empty], 2, &[0.0, 0.0]),
        ("null-row", vec![Value::Null], 1, &[]),
        ("fresh-market", vec![base, short], 3, &[1.0, 3.0]),
    ];
    let mut calls = 0;
    let mut getter_calls = 0;
    let mut observations = Vec::new();
    for (label, rows, expected_reads, expected_contracts) in cases {
        let store = Rc::new(ObservedStore::new());
        let market = normalize_market(
            &serde_json::Value::Object(
                (json!({"symbol":"BTC_USDT","contractSize":0.1,"priceScale":2})
                    .as_object()
                    .unwrap())
                .clone(),
            )
            .into(),
        )
        .unwrap();
        store
            .update(
                StoreUpdate {
                    markets: Some(vec![market]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        if label == "fresh-market" {
            *store.second_market.borrow_mut() = Some(
                normalize_market(
                    &serde_json::Value::Object(
                        (json!({"symbol":"BTC_USDT","contractSize":0.3,"priceScale":2})
                            .as_object()
                            .unwrap())
                        .clone(),
                    )
                    .into(),
                )
                .unwrap(),
            );
        }
        let mut exchange = venue(Rc::new(DefaultStore::new()), 100.0, false);
        Rc::get_mut(&mut exchange.base).unwrap().store = store.clone();
        let adapter = Rc::new(StaticResponse(
            serde_json::to_vec(&json!({"data":rows})).unwrap(),
            RefCell::default(),
        ));
        exchange.xhr = adapter.clone();
        let future = Exchange::fetch_positions(&exchange, Some(vec!["BTCUSDT".into()]));
        assert_eq!(
            adapter.1.borrow().len(),
            0,
            "positions native unpolled GET floor {label}"
        );
        assert_eq!(
            store.market_reads.get(),
            0,
            "positions native unpolled market getter floor {label}"
        );
        let positions = unit_tasks::drive(future).unwrap();
        calls += 1;
        assert_eq!(
            positions.len(),
            expected_contracts.len(),
            "positions native independently fixed result floor {label}"
        );
        for (position, expected) in positions.iter().zip(expected_contracts) {
            assert_eq!(
                position.symbol, "BTCUSDT",
                "positions native decoded symbol {label}"
            );
            assert_eq!(
                position.contracts.to_bits(),
                expected.to_bits(),
                "positions native source symbol-only contracts {label}"
            );
        }
        assert_eq!(
            store.market_reads.get(),
            expected_reads,
            "positions native fresh market getter calls {label}"
        );
        assert_eq!(
            adapter.1.borrow().len(),
            1,
            "positions native polled GET floor {label}"
        );
        assert_eq!(
            adapter.1.borrow()[0].url,
            "/api/v1/private/position/open_positions",
            "positions native decoded GET URL {label}"
        );
        getter_calls += store.market_reads.get();
        observations.push((
            label,
            positions.iter().map(|p| p.contracts).collect::<Vec<_>>(),
        ));
    }
    assert_eq!(calls, 6, "positions native independent method call floor");
    assert_eq!(
        getter_calls, 12,
        "positions native independent market getter floor"
    );
    assert_eq!(
        observations.len(),
        6,
        "positions native independent observation floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/position_market_reads_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "positions actual Source market oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        6,
        "positions Source independent method call floor"
    );
    for ((label, contracts), row) in observations.iter().zip(&source) {
        assert_eq!(row["label"], *label, "positions secondary Source identity");
        let source_contracts = row["contracts"].as_array().unwrap();
        assert_eq!(source_contracts.len(), contracts.len());
        for (source_contract, native_contract) in source_contracts.iter().zip(contracts) {
            assert_eq!(
                source_contract.as_f64().unwrap().to_bits(),
                native_contract.to_bits(),
                "positions secondary Source decoded contracts {label}"
            );
        }
    }
}

#[test]
fn mexc_validation_keeps_raw_detail_and_error_precedence() {
    let cases = [
        (
            json!({"message":"first", "msg":"second"}),
            "transport",
            json!("first"),
        ),
        (
            json!({"message":"", "msg":"second"}),
            "transport",
            json!("second"),
        ),
        (
            json!({"message":false, "msg":0}),
            "transport",
            json!("transport"),
        ),
        (json!({}), "", json!("Invalid MEXC key")),
        (json!({"message":42}), "transport", json!(42)),
        (
            json!({"message":{"message":"object detail", "code":313}}),
            "transport",
            json!({"message":"object detail", "code":313}),
        ),
    ];
    let mut native = Vec::new();
    for (index, (data, message, wanted)) in cases.into_iter().enumerate() {
        let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
        exchange.base = Rc::new(reporting_values::raw_reporting_base());
        exchange.xhr = Rc::new(AccountFailure {
            message: message.into(),
            response_data: Some(data),
        });
        let emitted = Rc::new(RefCell::new(Vec::new()));
        let events = emitted.clone();
        exchange.base.on_args(
            "error",
            Rc::new(move |_, event| {
                let event: Vec<serde_json::Value> = event
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
                events.borrow_mut().push(serde_json::json!(event));
                Ok(())
            }),
        );
        let result = unit_tasks::drive(exchange.validate_account()).unwrap();
        native.push(result.error.as_value().clone());
        assert_eq!(
            result.error.as_value(),
            &wanted,
            "MEXC raw validation detail case {index}"
        );
        assert!(
            result.balance.is_none(),
            "MEXC failed validation has no balance case {index}"
        );
        let expected = match index {
            0 => "Exchange Error: first",
            1 => "Exchange Error: second",
            2 => "Exchange Error: transport",
            3 => "Exchange Error: Invalid MEXC key",
            4 => "42",
            5 => "object detail",
            _ => unreachable!(),
        };
        assert_eq!(
            emitted.borrow().len(),
            1,
            "MEXC validation reports once case {index}"
        );
        assert_eq!(
            emitted.borrow()[0][0],
            expected,
            "MEXC validation raw reporting case {index}"
        );
    }
    assert_eq!(native.len(), 6, "MEXC independent raw validation floor");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC raw account oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["rawCases"].as_array().unwrap().len(),
        6,
        "MEXC source raw validation floor"
    );
    for (index, value) in native.into_iter().enumerate() {
        assert_eq!(
            value, source["rawCases"][index]["error"],
            "MEXC raw account source detail case {index}"
        );
        assert_eq!(
            source["rawCases"][index]["emitted"],
            json!([value]),
            "MEXC source raw report input case {index}"
        );
    }
}

#[test]
fn mexc_ohlcv_retains_rejected_transport_origin() {
    let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
    exchange.xhr = Rc::new(AccountFailure {
        message: "original transport".into(),
        response_data: Some(json!({"message":"server detail", "code":611})),
    });
    let options = OHLCVOptions {
        symbol: "BTCUSDT".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
        from: None,
        to: None,
        limit: None,
    };
    let error = unit_tasks::drive(exchange.fetch_ohlcv(options)).unwrap_err();
    assert_eq!(
        error.message, "original transport",
        "MEXC OHLCV original transport message"
    );
    assert_eq!(
        error.response_json_view().as_ref().unwrap()["data"]["message"],
        "server detail",
        "MEXC OHLCV retained server response"
    );
    let original = error
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<RestFailure>()
        .expect("MEXC OHLCV typed RestFailure origin");
    assert_eq!(
        original.request.url, "/api/v1/contract/kline/BTC_USDT",
        "MEXC OHLCV rejected original request"
    );
}

#[test]
fn mexc_snapshot_reporters_preserve_object_detail_and_listener_failure() {
    let mut native = Vec::new();
    for method in ["fetchMarkets", "fetchTickers", "fetchPositions"] {
        for throws in [false, true] {
            let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
            exchange.base = Rc::new(reporting_values::raw_reporting_base());
            exchange.xhr = Rc::new(AccountFailure {
                message: "transport".into(),
                response_data: Some(json!({"message":{"message":"inner detail","code":313}})),
            });
            let emitted = Rc::new(RefCell::new(Vec::new()));
            let events = emitted.clone();
            let mut original = ExchangeError::new("listener stopped reporting");
            original.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(777)));
            let failure = original.clone();
            exchange.base.on_args(
                "error",
                Rc::new(move |_, value| {
                    let value: Vec<serde_json::Value> = value
                        .iter()
                        .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                        .collect();
                    events.borrow_mut().push(serde_json::json!(value));
                    if throws { Err(failure.clone()) } else { Ok(()) }
                }),
            );
            let result = match method {
                "fetchMarkets" => unit_tasks::drive(exchange.fetch_markets()).map(|v| v.len()),
                "fetchTickers" => unit_tasks::drive(exchange.fetch_tickers()).map(|v| v.len()),
                "fetchPositions" => {
                    unit_tasks::drive(exchange.fetch_positions(None)).map(|v| v.len())
                }
                _ => unreachable!(),
            };
            assert_eq!(
                emitted.borrow().len(),
                1,
                "MEXC snapshot reporting-call floor {method}/{throws}"
            );
            assert_eq!(
                emitted.borrow()[0][0],
                "inner detail",
                "MEXC snapshot raw object report {method}/{throws}"
            );
            if throws {
                assert_eq!(
                    result.unwrap_err(),
                    original,
                    "MEXC snapshot original reporting failure {method}"
                );
            } else {
                assert_eq!(result.unwrap(), 0, "MEXC snapshot fallback length {method}");
                native.push(method);
            }
        }
    }
    assert_eq!(
        native.len(),
        3,
        "MEXC snapshot distinct native method floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/account_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "MEXC snapshot source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["snapshotCases"].as_array().unwrap().len(),
        3,
        "MEXC snapshot source method floor"
    );
    for (index, method) in native.into_iter().enumerate() {
        assert_eq!(
            source["snapshotCases"][index]["method"], method,
            "MEXC snapshot source method identity"
        );
        assert_eq!(
            source["snapshotCases"][index]["emitted"],
            json!([{"message":"inner detail","code":313}]),
            "MEXC snapshot source raw reporting argument"
        );
        assert_eq!(
            source["snapshotCases"][index]["fallbackLength"], 0,
            "MEXC snapshot source empty fallback"
        );
    }
}

#[test]
fn mexc_four_borrowed_methods_price_unpolled_drop_and_first_poll() {
    use futures::TryFutureExt;
    let mut completed = 0;
    for method in [
        "fetchMarkets",
        "fetchTickers",
        "fetchAndSetOrders",
        "fetchAndSetRegularOrders",
    ] {
        let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
        let response = Rc::new(StaticResponse(b"[]".to_vec(), RefCell::default()));
        exchange.xhr = response.clone();
        let make = || -> ExchangeFuture<'_, usize> {
            match method {
                "fetchMarkets" => Box::pin(exchange.fetch_markets().map_ok(|v| v.len())),
                "fetchTickers" => Box::pin(exchange.fetch_tickers().map_ok(|v| v.len())),
                "fetchAndSetOrders" => {
                    Box::pin(exchange.fetch_and_set_orders(None).map_ok(|v| v.len()))
                }
                "fetchAndSetRegularOrders" => Box::pin(
                    exchange
                        .fetch_and_set_regular_orders(None)
                        .map_ok(|v| v.len()),
                ),
                _ => unreachable!(),
            }
        };
        let future = make();
        assert_eq!(
            response.1.borrow().len(),
            0,
            "MEXC borrowed unpolled request count {method}"
        );
        drop(future);
        assert_eq!(
            response.1.borrow().len(),
            0,
            "MEXC borrowed dropped observer omits request {method}"
        );
        assert_eq!(
            unit_tasks::drive(make()).unwrap(),
            0,
            "MEXC borrowed empty response output {method}"
        );
        let expected = if method == "fetchAndSetOrders" { 3 } else { 1 };
        assert_eq!(
            response.1.borrow().len(),
            expected,
            "MEXC borrowed polled request count {method}"
        );
        completed += 1;
    }
    assert_eq!(completed, 4, "MEXC borrowed independent method floor");
}

#[test]
fn mexc_account_reporting_checks_enhancement_before_message_coercion() {
    let mut exchange = venue(Rc::new(DefaultStore::new()), 100., false);
    exchange.base = Rc::new(reporting_values::raw_reporting_base());
    exchange.xhr = Rc::new(AccountFailure {
        message: "transport".into(),
        response_data: Some(
            json!({"message":{"context":{"x":1},"enhancedMessage":"enhanced","message":{"toString":null}}}),
        ),
    });
    let events = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    exchange.base.on_args(
        "error",
        Rc::new(move |_, value| {
            let value: Vec<serde_json::Value> = value
                .iter()
                .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                .collect();
            sink.borrow_mut().push(serde_json::json!(value));
            Ok(())
        }),
    );
    let validation = unit_tasks::drive(exchange.validate_account()).unwrap();
    assert_eq!(
        validation.error.as_value()["enhancedMessage"],
        "enhanced",
        "MEXC validation retains enhanced original detail"
    );
    assert_eq!(
        events.borrow().len(),
        1,
        "MEXC enhanced reporter call floor"
    );
    assert_eq!(
        events.borrow()[0][0],
        "enhanced",
        "MEXC enhanced reporting skips unusable message coercion"
    );
    exchange.xhr = Rc::new(AccountFailure {
        message: "transport".into(),
        response_data: Some(json!({"message":{"message":"visible","toString":null}})),
    });
    let error = unit_tasks::drive(exchange.get_account()).unwrap_err();
    assert_eq!(
        error.name, "TypeError",
        "MEXC getAccount preserves original coercion TypeError"
    );
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture")
    })
}
