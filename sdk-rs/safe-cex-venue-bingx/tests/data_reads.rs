use safe_cex_types::retained_json::RetainedJsonInput as Input;
#[path = "support/retained_observe.rs"]
mod retained_observe;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_num::numeric_ingress::{JsNumberInput, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::*;
use safe_cex_venue_bingx::data_reads::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..100 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("fixture must resolve within 100 local polls");
}
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/data_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "BINGX-DATA source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn number(n: f64) -> Value {
    json!({"number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
}
fn observe(v: &Value) -> Value {
    match v {
        Value::Number(_) => number(js_value_as_f64(v).unwrap()),
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        _ => v.clone(),
    }
}
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            let mut keys = fields.keys().collect::<Vec<_>>();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|key| (key.clone(), sorted(&fields[key])))
                    .collect(),
            )
        }
        Value::Array(rows) => Value::Array(rows.iter().map(sorted).collect()),
        _ => value.clone(),
    }
}
fn balance(v: &Balance) -> Value {
    let mut out = json!({"total":number(v.total),"free":number(v.free),"used":number(v.used),"upnl":number(v.upnl)});
    if let Some(info) = &v.info {
        out["info"] = retained_observe::observe(info);
    }
    assert!(
        v.coins.is_none() && v.currencies.is_none(),
        "BINGX-DATA native balance shape"
    );
    out
}
fn market(v: &Market) -> Value {
    json!({"id":v.id,"symbol":v.symbol,"base":v.base,"quote":v.quote,"active":v.active,"precision":{"amount":number(v.precision.amount),"price":number(v.precision.price)},"limits":{"amount":{"min":number(v.limits.amount.min),"max":number(v.limits.amount.max)},"leverage":{"min":number(v.limits.leverage.min),"max":number(v.limits.leverage.max)}}})
}
fn ticker(v: &Ticker) -> Value {
    json!({"id":v.id,"symbol":v.symbol,"bid":number(v.bid),"ask":number(v.ask),"last":number(v.last),"mark":number(v.mark),"index":number(v.index),"percentage":number(v.percentage),"fundingRate":number(v.funding_rate),"fundingTime":number(v.funding_time),"volume":number(v.volume),"quoteVolume":number(v.quote_volume),"openInterest":number(v.open_interest)})
}
#[derive(Default)]
struct Host {
    logs: RefCell<Vec<String>>,
    events: Rc<RefCell<Vec<String>>>,
}
impl ExchangeRuntime for Host {
    fn monotonic_now_ms(&self) -> f64 {
        1234.0
    }
    fn now_ms(&self) -> f64 {
        1234.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("reads do not schedule")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("reads do not spawn")
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(message);
        let message = rendered.as_str();
        if severity == "error" {
            self.logs
                .borrow_mut()
                .push(message.strip_prefix("exchange error ").unwrap().into());
        }
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
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("reads do not use workers")
    }
}
struct Transport {
    input: Value,
    calls: RefCell<Vec<safe_cex_transport::RestRequest>>,
}
impl ReadTransport for Transport {
    fn send(&self, request: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
        self.calls.borrow_mut().push(request);
        let index = self.calls.borrow().len() - 1;
        Box::pin(async move {
            if !self.input["error"].is_null() {
                let mut error =
                    ExchangeError::new(self.input["error"]["message"].as_str().unwrap());
                error.response = Some(self.input["error"]["response"].clone());
                error.store_subscriber_ids = vec![71, 83];
                return Err(error);
            }
            if let Some(raw) = self.input.get("rawResponse").and_then(Value::as_str) {
                return Ok(serde_json::from_str(raw).unwrap());
            }
            Ok(serde_json::from_value(self.input["responses"][index].clone()).unwrap())
        })
    }
}
fn fixture(input: Value) -> (BingxDataReads, Rc<DefaultStore>, Rc<Transport>, Rc<Host>) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![
                serde_json::from_value(corpus()["market"].clone()).unwrap(),
            ]),
            balance: Some(serde_json::from_value(corpus()["balance"].clone()).unwrap()),
            fetched_leverage: Some(
                serde_json::from_value(json!({"OLD":{"buyLeverage":2,"sellLeverage":3}})).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let host = Rc::new(Host::default());
    let base = Rc::new(BaseExchange::new(
        ExchangeName::Bingx,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store.clone(),
        host.clone(),
        ExchangeSettings::default(),
    ));
    let events = host.events.clone();
    base.on(
        "error",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            events.borrow_mut().push(value.as_str().unwrap().into());
            Ok(())
        }),
    );
    if input["disposed"] == true {
        base.dispose().unwrap();
    }
    let transport = Rc::new(Transport {
        input,
        calls: RefCell::new(Vec::new()),
    });
    (
        BingxDataReads::new(base, transport.clone()),
        store,
        transport,
        host,
    )
}
#[test]
fn data_reads_match_actual_typescript() {
    let asset_rows = corpus()["assetRows"].as_array().unwrap();
    assert_eq!(asset_rows.len(), 10, "independent source asset cases");
    for (index, row) in asset_rows.iter().enumerate() {
        assert_eq!(row["field"], if index < 5 { "base" } else { "quote" });
        assert_eq!(
            row["type"],
            ["object", "boolean", "null", "number", "undefined"][index % 5]
        );
        let markets = row["result"].as_array().unwrap();
        assert_eq!(markets.len(), 3);
        assert_eq!(markets[0]["base"], "BTC");
        assert_eq!(markets[1]["symbol"], "BADUSDT");
        assert_eq!(markets[2]["base"], "ETH");
    }
    let leverage_rows = corpus()["leverageRows"].as_array().unwrap();
    assert_eq!(leverage_rows.len(), 4, "source raw market leverage cases");
    for row in leverage_rows {
        assert_eq!(row.as_array().unwrap().len(), 3);
    }
    let names = [
        "validateAccount",
        "fetchBalance",
        "fetchMarkets",
        "fetchTickersRaw",
        "formatTickers",
        "fetchTickers",
        "fetchLeverageForSymbol",
        "fetchMarginModeForSymbol",
        "getOrFetchPositionMode",
    ];
    assert_eq!(
        corpus()["names"],
        json!(names),
        "BINGX-DATA source method census"
    );
    let cases = corpus()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36, "BINGX-DATA source case census");
    let inputs = json!(
        cases
            .iter()
            .map(|case| json!([case["method"], case["variant"], case["input"]]))
            .collect::<Vec<_>>()
    );
    let encoded = serde_json::to_string(&sorted(&inputs)).unwrap();
    let fingerprint = encoded.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    });
    assert_eq!(
        fingerprint, 0x451097675e13b5cf,
        "BINGX-DATA independently pinned input content"
    );
    println!("BINGX-DATA runtime {}", corpus()["runtime"]);
    let mut counts = [0; 9];
    for (index, case) in cases.iter().enumerate() {
        let method = case["method"].as_str().unwrap();
        let family = names.iter().position(|n| *n == method).unwrap();
        assert_eq!(family, index / 4, "BINGX-DATA method order");
        assert_eq!(
            case["variant"],
            json!(index % 4),
            "BINGX-DATA variant order"
        );
        counts[family] += 1;
        let (venue, store, transport, host) = fixture(case["input"].clone());
        let cached = Store::balance_handle(store.as_ref());
        let result:Result<Value,ExchangeError>=match method {
            "validateAccount"=>ready(venue.validate_account()).map(|v|{let mut out=json!({"error":v.error});if let Some(b)=v.balance{out["balance"]=balance(&b);}out}),
            "fetchBalance"=>ready(venue.fetch_balance()).map(|v|{if index%4>=2{assert!(std::sync::Arc::ptr_eq(&v,&cached),"BINGX-DATA retained balance fallback");}balance(&v)}),
            "fetchMarkets"=>ready(venue.fetch_markets()).map(|v|Value::Array(v.iter().map(market).collect())),
            "fetchTickersRaw"=>ready(venue.fetch_tickers_raw()).map(|v|observe(&serde_json::to_value(v).unwrap())),
            "formatTickers"=>venue.format_tickers(&RawTickers { dailys:case["input"]["raw"].get("dailys").cloned().map(Into::into),prices:case["input"]["raw"].get("prices").cloned().map(Into::into) }).map(|v|Value::Array(v.iter().map(ticker).collect())),
            "fetchTickers"=>ready(venue.fetch_tickers()).map(|v|{assert_eq!(Store::tickers(store.as_ref()).len(),v.len(),"BINGX-DATA native published tickers");Value::Array(v.iter().map(ticker).collect())}),
            "fetchLeverageForSymbol"=>ready(venue.fetch_leverage_for_symbol("BTCUSDT")).map(|v|v.map_or(Value::Null,|v|json!({"buyLeverage":number(v.buy_leverage),"sellLeverage":number(v.sell_leverage)}))),
            "fetchMarginModeForSymbol"=>ready(venue.fetch_margin_mode_for_symbol("BTCUSDT")).map(|v|json!(v)),
            "getOrFetchPositionMode"=>ready(async {let a=venue.get_or_fetch_position_mode(false).await?;let b=venue.get_or_fetch_position_mode(false).await?;let c=venue.get_or_fetch_position_mode(true).await?;Ok(json!([a,b,c]))}),
            _=>unreachable!(),
        };
        let output = match result {
            Ok(v) => json!({"ok":v}),
            Err(e) => {
                if index % 4 == 2
                    && ["fetchMarginModeForSymbol", "getOrFetchPositionMode"].contains(&method)
                {
                    assert_eq!(
                        e.store_subscriber_ids,
                        vec![71, 83],
                        "BINGX-DATA propagated transport identity"
                    );
                }
                json!({"error":{"name":e.name,"message":e.message}})
            }
        };
        let mut expected = case["output"].clone();
        if method == "validateAccount" && index % 4 == 1 {
            assert_eq!(
                expected["ok"]["balance"],
                json!({"undefined":true}),
                "BINGX-DATA source own undefined balance"
            );
            assert!(
                output["ok"].get("balance").is_none(),
                "BINGX-DATA CHOSEN absent optional balance"
            );
            expected["ok"].as_object_mut().unwrap().remove("balance");
        }
        assert_eq!(
            output,
            expected,
            "BINGX-DATA native {method} variant {}",
            index % 4
        );
        let calls = transport.calls.borrow();
        let configs: Vec<Value> = calls
            .iter()
            .map(|r| {
                assert_eq!(r.method, "GET", "BINGX-DATA native method");
                assert!(r.body.is_none(), "BINGX-DATA GET body");
                let mut args = vec![json!(r.url)];
                if let Some(p) = &r.params {
                    args.push(json!({"params":p}));
                }
                observe(&json!(args))
            })
            .collect();
        assert_eq!(
            json!(configs),
            case["calls"],
            "BINGX-DATA native requests {method}"
        );
        let reports = case["reports"].as_array().unwrap();
        assert_eq!(
            *host.logs.borrow(),
            reports
                .iter()
                .map(|r| r[0].as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "BINGX-DATA native reports {method}"
        );
        let formatted = reports
            .iter()
            .map(|r| {
                let msg = r[0].as_str().unwrap();
                let label = r.get(1).and_then(Value::as_str).unwrap_or("Exchange Error");
                if msg.is_empty() {
                    r.get(1)
                        .and_then(Value::as_str)
                        .unwrap_or("Unknown Exchange Error")
                        .to_owned()
                } else {
                    format!("{label}: {msg}")
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(
            *host.events.borrow(),
            formatted,
            "BINGX-DATA native report labels {method}"
        );
        if method == "fetchLeverageForSymbol" {
            let leverage = Store::fetched_leverage(store.as_ref());
            assert_eq!(
                leverage["OLD"].buy_leverage, 2.0,
                "BINGX-DATA retain unrelated leverage"
            );
            assert_eq!(
                leverage.contains_key("BTCUSDT"),
                index % 4 < 2,
                "BINGX-DATA leverage publication"
            );
        }
    }
    assert_eq!(counts, [4; 9], "BINGX-DATA native executed rows");
    assert_eq!(
        corpus()["boundary"]["units"],
        json!([97, 55296, 98]),
        "BINGX-DATA source lone surrogate"
    );
    let (venue, _, _, _) = fixture(json!({"rawResponse":corpus()["boundary"]["raw"]}));
    let balance = ready(venue.fetch_balance()).unwrap();
    assert_eq!(
        json!(
            balance
                .info
                .as_ref()
                .unwrap()
                .get("userId")
                .and_then(Input::as_text)
                .unwrap()
        ),
        corpus()["boundary"]["units"],
        "BINGX-DATA native balance info retains lone UTF16"
    );
}
#[test]
fn store_notification_identity_is_retained() {
    let case = &corpus()["cases"][20];
    let (venue, store, _, _) = fixture(case["input"].clone());
    let listener = Store::subscribe(
        store.as_ref(),
        Box::new(|_, _| {
            let mut cause = ExchangeError::new("subscriber cause");
            cause.name = "ListenerError".into();
            cause.store_subscriber_ids = vec![71, 83];
            Err(StoreSubscriberError::new("subscriber cause").with_cause(Rc::new(cause)))
        }),
    );
    let error = ready(venue.fetch_tickers()).unwrap_err();
    assert_eq!(
        error.name, "ListenerError",
        "BINGX-DATA subscriber error name"
    );
    assert_eq!(
        error.message, "subscriber cause",
        "BINGX-DATA subscriber error cause"
    );
    assert_eq!(
        &error.store_subscriber_ids[..2],
        &[71, 83],
        "BINGX-DATA subscriber inner boundaries"
    );
    assert_eq!(
        error.store_subscriber_ids.len(),
        3,
        "BINGX-DATA subscriber outer boundary appended"
    );
    assert_eq!(
        Store::tickers(store.as_ref()).len(),
        1,
        "BINGX-DATA write precedes failing notification"
    );
    listener();
}
#[test]
fn typed_market_and_leverage_fields_refuse_source_dynamic_values() {
    assert_eq!(
        json!(&corpus()["chosen"].as_array().unwrap()[..2]),
        json!([{ "method":"fetchMarkets","value":"75" },{ "method":"fetchLeverageForSymbol","value":"17" }]),
        "BINGX-DATA source retains dynamic numeric strings"
    );
    let mut input = corpus()["cases"][8]["input"].clone();
    input["responses"][0]["data"][0]["maxLongLeverage"] = json!("75");
    let (venue, _, _, host) = fixture(input);
    let markets = ready(venue.fetch_markets()).unwrap();
    assert_eq!(
        markets
            .iter()
            .map(|m| m.symbol.as_str())
            .collect::<Vec<_>>(),
        ["XAUUSDUSDT"],
        "BINGX-DATA valid fresh sibling survives typed leverage refusal"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec![
            r#"Refused BingX market row 0 symbol=BTC-USDT: limits.leverage.max="75": typed max leverage requires a number"#
        ],
        "BINGX-DATA typed market refusal report"
    );
    let mut input = corpus()["cases"][24]["input"].clone();
    input["responses"][0]["data"]["longLeverage"] = json!("17");
    let (venue, store, _, host) = fixture(input);
    assert!(
        ready(venue.fetch_leverage_for_symbol("BTCUSDT"))
            .unwrap()
            .is_none(),
        "BINGX-DATA typed leverage refusal"
    );
    assert_eq!(
        Store::fetched_leverage(store.as_ref()).len(),
        1,
        "BINGX-DATA refused leverage is not published"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["typed buy leverage requires a number"],
        "BINGX-DATA typed leverage refusal report"
    );
}
#[test]
fn disposed_after_response_does_not_publish_tickers() {
    struct Disposing {
        base: Rc<BaseExchange>,
        responses: Vec<Value>,
        calls: std::cell::Cell<usize>,
    }
    impl ReadTransport for Disposing {
        fn send(&self, _: safe_cex_transport::RestRequest) -> ExchangeFuture<'_, JsNumberInput> {
            let index = self.calls.get();
            self.calls.set(index + 1);
            Box::pin(async move {
                if index == 1 {
                    self.base.dispose()?;
                }
                Ok(serde_json::from_value(self.responses[index].clone()).unwrap())
            })
        }
    }
    let (venue, store, _, _) = fixture(corpus()["cases"][20]["input"].clone());
    let transport = Rc::new(Disposing {
        base: venue.base.clone(),
        responses: corpus()["cases"][20]["input"]["responses"]
            .as_array()
            .unwrap()
            .clone(),
        calls: std::cell::Cell::new(0),
    });
    let venue = BingxDataReads::new(venue.base, transport.clone());
    assert!(
        ready(venue.fetch_tickers()).unwrap().is_empty(),
        "BINGX-DATA in-flight disposal result"
    );
    assert_eq!(
        transport.calls.get(),
        2,
        "BINGX-DATA sequential raw reads complete"
    );
    assert!(
        Store::tickers(store.as_ref()).is_empty(),
        "BINGX-DATA in-flight disposal suppresses Store write"
    );
}
#[test]
fn source_market_error_precedes_typed_output_refusal() {
    assert_eq!(
        corpus()["chosen"][2],
        json!({"method":"market-source-prefix","reports":["Cannot read properties of null (reading 'replace')"]}),
        "BINGX-DATA actual source later error precedes typed refusal"
    );
    let mut input = corpus()["cases"][8]["input"].clone();
    input["responses"][0]["data"][0]["maxLongLeverage"] = json!("75");
    input["responses"][0]["data"][1]["symbol"] = Value::Null;
    let (venue, _, _, host) = fixture(input);
    assert_eq!(
        ready(venue.fetch_markets()).unwrap().len(),
        1,
        "BINGX-DATA source failure returns cached markets"
    );
    assert_eq!(
        *host.logs.borrow(),
        vec!["Cannot read properties of null (reading 'replace')"],
        "BINGX-DATA source evaluation precedes typed market refusal"
    );
}

#[test]
fn audit_validation_returns_coerced_hints() {
    let cases = corpus()["audit"].as_array().unwrap();
    assert_eq!(cases.len(), 3, "BINGX-AUDIT independent hint floor");
    let hints = [
        json!({"reason":"denied"}),
        json!(["risk", "limit"]),
        json!(17),
    ];
    let types = ["object", "object", "number"];
    let native_messages = ["[object Object]", "risk,limit", "17"];
    for (index, case) in cases.iter().enumerate() {
        assert_eq!(
            case["hint"], hints[index],
            "BINGX-AUDIT frozen source input"
        );
        assert_eq!(case["type"], types[index], "BINGX-AUDIT source hint type");
        assert_eq!(case["sameHint"], true, "BINGX-AUDIT source hint identity");
        assert_eq!(
            case["result"],
            observe(&hints[index]),
            "BINGX-AUDIT source hint value"
        );
        let (venue, _, _, _) = fixture(case["input"].clone());
        let result = ready(venue.validate_account()).unwrap();
        assert_eq!(
            result.error, native_messages[index],
            "BINGX-AUDIT native coerced hint"
        );
        assert!(
            result.balance.is_none(),
            "BINGX-AUDIT native no balance on rejection"
        );
    }
}

#[test]
fn audit_validation_source_control_discriminates() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "audit_validation_returns_coerced_hints",
            "--nocapture",
        ])
        .env("SAFE_CEX_BINGX_AUDIT_SOURCE_STRING_HINT", "1")
        .output()
        .expect("BingX audit control child");
    let diagnostic = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "BINGX-AUDIT source control must fail a Rust assertion: {diagnostic}"
    );
    assert!(
        diagnostic.contains("BINGX-AUDIT source hint type"),
        "BINGX-AUDIT precise source assertion: {diagnostic}"
    );
}
