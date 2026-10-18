use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{
    ExchangeName, Execution, ExecutionsOptions, HistoricalTrade, HistoricalTradeOptions,
};
use safe_cex_venue_woo::historical_fills::{
    fetch_historical_fills, fetch_historical_trades_internal,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Runtime {
    logs: Rc<RefCell<Vec<Value>>>,
    serial: Cell<u64>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        Box::pin(async { Ok(()) })
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("no history tasks")
    }
    fn console_values(&self, level: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if level == "error" {
            self.logs.borrow_mut().push(json!(
                message
                    .strip_prefix("exchange error ")
                    .expect("caller diagnostic prefix")
            ));
        }
    }
    fn unique_id(&self) -> String {
        self.serial.set(self.serial.get() + 1);
        self.serial.get().to_string()
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
        Box::pin(async { Err(ExchangeError::new("no worker")) })
    }
}
struct Venue {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
    public: bool,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
    fn is_public_data_only(&self) -> bool {
        self.public
    }
}
fn venue(public: bool, throw: bool, logs: Rc<RefCell<Vec<Value>>>) -> Venue {
    let runtime = Rc::new(Runtime {
        logs: logs.clone(),
        serial: Cell::new(0),
    });
    let base = BaseExchange::new(
        ExchangeName::Woo,
        serde_json::from_value(json!({"key":"test-key","secret":"test-secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        runtime,
        ExchangeSettings::default(),
    );
    if throw {
        base.on(
            "error",
            Rc::new(move |_, _| {
                let mut e =
                    ExchangeError::new(format!("subscriber failure {}", logs.borrow().len()));
                e.name = "TypeError".into();
                Err(e)
            }),
        );
    }
    Venue {
        base,
        capabilities: ExchangeCapabilities::default(),
        public,
    }
}
struct Adapter {
    label: String,
    rows: Vec<Value>,
    groups: RefCell<Vec<Value>>,
    attempts: Cell<usize>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.attempts.set(self.attempts.get() + 1);
            let params = request.params.as_ref().unwrap();
            if request.url == "/v3/trade/transactionHistory"
                && (params.get("symbol") == Some(&json!("PERP_BTC_USDT"))
                    || !params.as_object().unwrap().contains_key("symbol")
                        && self.groups.borrow().is_empty())
            {
                self.groups.borrow_mut().push(json!({"symbol":if params.get("symbol").is_some(){json!("BTCUSDT")}else{json!({"$undefined":true})},"limit":bits(safe_cex_num::js_number(params.get("limit")).unwrap()),"fromMilliseconds":params.get("startTime").map(observe).unwrap_or(json!({"$undefined":true})),"toMilliseconds":params.get("endTime").map(observe).unwrap_or(json!({"$undefined":true}))}));
            }
            let group = self.groups.borrow().len();
            if matches!(self.label.as_str(), "request-error" | "logger-error")
                || (group == 2
                    && matches!(
                        self.label.as_str(),
                        "fallback-error" | "fallback-logger-error"
                    ))
            {
                let mut error = RestFailure::pre_transmit("outer-message".into(), request);
                error.context = Some(Box::new(
                    json!({"response":{"data":{"message":"response-message"}}}),
                ));
                return Err(error);
            }
            let rows =
                if self.label == "empty" || (group == 1 && self.label.starts_with("fallback")) {
                    Vec::new()
                } else {
                    self.rows.clone()
                };
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"data":{"rows":rows}}),
                    )
                    .unwrap(),
                ),
            })
        })
    }
}
fn bits(value: f64) -> Value {
    json!({"$bits":format!("{:016x}",if value.is_nan(){0x7ff8000000000000}else{value.to_bits()})})
}
fn observe(value: &Value) -> Value {
    match value {
        Value::Number(_) => bits(safe_cex_num::js_number(Some(value)).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        other => other.clone(),
    }
}
fn unpack(value: &Value) -> Value {
    if let Some(raw) = value.get("$bits").and_then(Value::as_str) {
        return Value::from(f64::from_bits(u64::from_str_radix(raw, 16).unwrap()));
    }
    match value {
        Value::Array(v) => Value::Array(v.iter().map(unpack).collect()),
        Value::Object(v) => Value::Object(
            v.iter()
                .filter(|(_, v)| v.get("$undefined") != Some(&Value::Bool(true)))
                .map(|(k, v)| (k.clone(), unpack(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}
fn execution(row: &Execution) -> Value {
    let mut out = json!({"timestamp":bits(row.timestamp),"symbol":row.symbol,"price":bits(row.price),"qty":bits(row.qty),"side":match row.side{safe_cex_types::OrderSide::Buy=>"buy",safe_cex_types::OrderSide::Sell=>"sell"}});
    for (key, value) in [
        ("id", row.id.as_ref()),
        ("orderId", row.order_id.as_ref()),
        ("commissionAsset", row.commission_asset.as_ref()),
    ] {
        if let Some(value) = value {
            out[key] = json!(value);
        }
    }
    for (key, value) in [
        ("commission", row.commission),
        ("realizedPnl", row.realized_pnl),
    ] {
        if let Some(value) = value {
            out[key] = bits(value);
        }
    }
    if let Some(value) = row.maker {
        out["maker"] = json!(value);
    }
    if let Some(value) = &row.position_side {
        out["positionSide"] = serde_json::to_value(value).unwrap();
    }
    out
}
fn trade(row: &HistoricalTrade) -> Value {
    let mut out = json!({"time":bits(row.time),"symbol":row.symbol,"orderId":row.order_id,"price":bits(row.price),"qty":bits(row.qty),"side":match row.side{safe_cex_types::OrderSide::Buy=>"buy",safe_cex_types::OrderSide::Sell=>"sell"},"positionSide":serde_json::to_value(&row.position_side).unwrap()});
    for (key, value) in [
        ("id", row.id.as_ref()),
        ("commissionAsset", row.commission_asset.as_ref()),
    ] {
        if let Some(value) = value {
            out[key] = json!(value);
        }
    }
    for (key, value) in [
        ("commission", row.commission),
        ("realizedPnl", row.realized_pnl),
    ] {
        if let Some(value) = value {
            out[key] = bits(value);
        }
    }
    if let Some(value) = row.maker {
        out["maker"] = json!(value);
    }
    out
}
fn ready<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("history fixture yielded"),
    }
}
#[test]
fn historical_callers_match_source_fields_effects_and_errors() {
    let run = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/historical-fills-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let source: Value = serde_json::from_slice(&run.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 48, "FILLS source case count");
    let (
        mut compared,
        mut excluded,
        mut output_count,
        mut request_count,
        mut attempt_count,
        mut log_count,
    ) = (0, 0, 0, 0, 0, 0);
    for index in 0..48 {
        let case = cases.get(index).unwrap();
        let label = case["label"].as_str().unwrap();
        if matches!(label, "normalize-error" | "limit-nan") {
            excluded += 1;
            continue;
        }
        let logs = Rc::new(RefCell::new(Vec::new()));
        let venue = venue(
            label == "public",
            matches!(label, "logger-error" | "fallback-logger-error"),
            logs.clone(),
        );
        let adapter = Adapter {
            label: label.into(),
            rows: unpack(&case["input"]).as_array().unwrap().clone(),
            groups: RefCell::new(Vec::new()),
            attempts: Cell::new(0),
        };
        let result = if index < 24 {
            let opts: ExecutionsOptions = serde_json::from_value(unpack(&case["opts"])).unwrap();
            ready(fetch_historical_fills(&venue, &adapter, &opts))
                .map(|rows| rows.iter().map(execution).collect::<Vec<_>>())
        } else {
            let mut opts = unpack(&case["opts"]);
            if opts.get("symbol").is_none() {
                opts["symbol"] = json!("");
            }
            let opts: HistoricalTradeOptions = serde_json::from_value(opts).unwrap();
            ready(fetch_historical_trades_internal(&venue, &adapter, &opts))
                .map(|rows| rows.iter().map(trade).collect::<Vec<_>>())
        };
        assert_eq!(
            json!(*adapter.groups.borrow()),
            case["requests"],
            "FILLS native query sequence {index}"
        );
        assert_eq!(
            json!(*logs.borrow()),
            case["logs"],
            "FILLS native reporting order {index}"
        );
        if let Some(expected) = case["outcome"].get("error") {
            let error = result.expect_err("FILLS logger error propagates");
            assert_eq!(
                json!({"name":error.name,"message":error.message}),
                *expected,
                "FILLS native error identity"
            );
        } else {
            let rows = result.unwrap();
            assert_eq!(
                rows.len(),
                case["outcome"]["rows"].as_array().unwrap().len(),
                "FILLS native output cardinality"
            );
            output_count += rows.len();
            assert_eq!(
                json!(rows),
                case["outcome"]["rows"],
                "FILLS native field identities {index}"
            );
        }
        compared += 1;
        request_count += adapter.groups.borrow().len();
        attempt_count += adapter.attempts.get();
        log_count += logs.borrow().len();
    }
    assert_eq!(
        (compared, excluded),
        (44, 4),
        "FILLS native comparison inventory"
    );
    assert_eq!(
        (output_count, request_count, attempt_count, log_count),
        (61, 44, 179, 9),
        "FILLS native independent totals"
    );
}

#[test]
fn historical_output_refusal_timing_and_nonfinite_bounds() {
    let run = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/historical-fills-boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let source: Value = serde_json::from_slice(&run.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18, "FILLS boundary source total");
    let (mut outputs, mut refusals, mut requests, mut reports) = (0, 0, 0, 0);
    for index in 0..18 {
        let case = cases.get(index).unwrap();
        let label = case["label"].as_str().unwrap();
        let logs = Rc::new(RefCell::new(Vec::new()));
        let venue = venue(false, false, logs.clone());
        let adapter = Adapter {
            label: label.into(),
            rows: unpack(&case["input"]).as_array().unwrap().clone(),
            groups: RefCell::new(Vec::new()),
            attempts: Cell::new(0),
        };
        let mut raw = unpack(&case["opts"]);
        raw.as_object_mut().unwrap().remove("from");
        raw.as_object_mut().unwrap().remove("to");
        let mut opts: ExecutionsOptions = serde_json::from_value(raw).unwrap();
        let number = if label.ends_with("nan") {
            f64::NAN
        } else if label.ends_with("positive") {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        };
        if label.starts_with("from-") {
            opts.from = Some(number);
        }
        if label.starts_with("to-") {
            opts.to = Some(number);
        }
        let result = if index < 9 {
            ready(fetch_historical_fills(&venue, &adapter, &opts))
                .map(|rows| rows.iter().map(execution).collect::<Vec<_>>())
        } else {
            let opts = HistoricalTradeOptions {
                symbol: opts.symbol.unwrap(),
                order_id: opts.order_id,
                from: opts.from,
                to: opts.to,
                limit: opts.limit,
                from_id: None,
                account_name: None,
            };
            ready(fetch_historical_trades_internal(&venue, &adapter, &opts))
                .map(|rows| rows.iter().map(trade).collect::<Vec<_>>())
        };
        assert_eq!(
            json!(*adapter.groups.borrow()),
            case["requests"],
            "FILLS boundary native queries"
        );
        assert_eq!(
            json!(*logs.borrow()),
            case["logs"],
            "FILLS boundary report precedence"
        );
        if label == "retained-asset" {
            assert_eq!(
                result.unwrap_err().message,
                "history commissionAsset cannot cross the typed String output",
                "FILLS retained asset refusal"
            );
            assert!(
                logs.borrow().is_empty(),
                "FILLS refusal is not a source error"
            );
            refusals += 1;
        } else {
            let rows = result.expect("FILLS discarded asset or later source error must not refuse");
            assert_eq!(
                rows.len(),
                case["rows"].as_array().unwrap().len(),
                "FILLS boundary native row count"
            );
            outputs += rows.len();
            assert_eq!(json!(rows), case["rows"], "FILLS boundary native values");
        }
        requests += adapter.attempts.get();
        reports += logs.borrow().len();
    }
    assert_eq!(
        (outputs, refusals, requests, reports),
        (38, 2, 18, 2),
        "FILLS boundary native totals"
    );
}
