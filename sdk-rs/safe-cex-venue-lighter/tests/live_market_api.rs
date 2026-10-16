// identity_op: `NUMBER_BITS[(0 + variant) % 7]` carries a per-FIELD offset into
// one rotating bit table, and the siblings read (1 + variant), (3 + variant) and
// (7 + variant). Dropping the `0 +` erases which field the row belongs to.
#![allow(non_snake_case, clippy::identity_op)]
use safe_cex_venue_lighter::{sdk_account_api::*, transaction_promise::*, transaction_signing::*};
use safe_cex_venue_lighter::{sdk_market_api::*, transaction_builders::TransactionBuildError};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/market-api-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn u(v: &str) -> Vec<u16> {
    v.encode_utf16().collect()
}
fn n(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn t(v: &Value) -> Vec<u16> {
    units(&v["units"])
}
#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl Runtime {
    fn drain(&self) {
        let mut count = 0;
        loop {
            let next = self.queue.borrow_mut().pop_front();
            let Some(next) = next else { break };
            next();
            count += 1;
            assert!(count <= 20, "independent runtime task ceiling");
        }
    }
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(task)
    }
    fn now_millis(&self) -> f64 {
        panic!("no clock in AccountApi")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("no auth encoding in AccountApi")
    }
}
type Trace = Rc<RefCell<Vec<String>>>;
#[derive(Clone)]
enum Entry {
    Value(AccountValue),
    Throw,
}
struct Object {
    fields: RefCell<BTreeMap<String, Vec<Entry>>>,
    counts: RefCell<BTreeMap<String, usize>>,
    trace: Trace,
    token: HostThrownValue,
    prefix: String,
}
impl AccountObject for Object {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        if !self.fields.borrow().contains_key(key) {
            return Ok(AccountValue::Undefined);
        }
        self.trace
            .borrow_mut()
            .push(match (self.prefix.as_str(), key) {
                ("container", "then") => "container.then.get".into(),
                (_, "then") => "data.then.get".into(),
                ("converter", "toString") => "converter.toString.get".into(),
                (_, "@@toPrimitive") => "coerce".into(),
                (prefix, "0" | "1") => format!("{prefix}.{key}"),
                ("types", "length") => "types.length".into(),
                ("types", "join") => "types.join.get".into(),
                ("filter", "status") => "filter.status".into(),
                ("filter", "tokenId") => "filter.tokenId".into(),
                _ => key.into(),
            });
        let mut counts = self.counts.borrow_mut();
        let count = counts.entry(key.into()).or_default();
        let fields = self.fields.borrow();
        let values = fields.get(key).unwrap();
        let v = values[(*count).min(values.len() - 1)].clone();
        *count += 1;
        match v {
            Entry::Value(v) => Ok(v),
            Entry::Throw => Err(self.token.clone().into()),
        }
    }
}
struct Semantics;
impl AccountSemantics for Semantics {
    fn get_property(&self, value: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        match value {
            AccountValue::Object(o) => o.get(key),
            AccountValue::Null | AccountValue::Undefined => Err(TransactionBuildError {
                name: "TypeError",
                message: format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if matches!(value, AccountValue::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )
                .encode_utf16()
                .collect(),
            }
            .into()),
            _ => Ok(AccountValue::Undefined),
        }
    }
    fn error_message(&self, value: &AccountValue) -> SigningResult<Vec<u16>> {
        match value {
            AccountValue::String(v) => Ok(v.clone()),
            AccountValue::Object(v) => {
                v.get("@@toPrimitive")?;
                panic!("coercion witness must throw")
            }
            _ => panic!("other Error message coercion outside this corpus"),
        }
    }
    fn resolve_return(
        self: Rc<Self>,
        value: AccountValue,
        resolver: PromiseResolver<AccountValue>,
        runtime: Rc<dyn TransactionRuntime>,
    ) {
        if let AccountValue::Object(object) = &value {
            match object.get("then") {
                Err(error) => {
                    resolver.settle(Err(error));
                    return;
                }
                Ok(AccountValue::Function(function)) => {
                    let then = function
                        .value()
                        .downcast_ref::<Then>()
                        .expect("typed then witness")
                        .clone();
                    let job_runtime = runtime.clone();
                    runtime.enqueue_microtask(Box::new(move || {
                        then.trace.borrow_mut().push("data.then.call".into());
                        if then.reject {
                            resolver.settle(Err(then.token.into()));
                        } else {
                            self.resolve_return(
                                AccountValue::Object(then.value),
                                resolver,
                                job_runtime,
                            );
                        }
                    }));
                    return;
                }
                Ok(_) => {}
            }
        }
        resolver.settle(Ok(value));
    }
}
#[derive(Clone)]
struct Then {
    reject: bool,
    trace: Trace,
    value: Rc<dyn AccountObject>,
    token: HostThrownValue,
}
fn object(
    fields: BTreeMap<String, Vec<Entry>>,
    trace: &Trace,
    token: &HostThrownValue,
) -> Rc<Object> {
    Rc::new(Object {
        fields: RefCell::new(fields),
        counts: RefCell::new(BTreeMap::new()),
        trace: trace.clone(),
        token: token.clone(),
        prefix: String::new(),
    })
}
struct Pool {
    a: Rc<Object>,
    b: Rc<Object>,
    coercion: Rc<dyn AccountObject>,
    symbol: HostThrownValue,
    function: HostThrownValue,
}
impl Pool {
    fn value(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(n(v)),
            "bigint" => AccountValue::BigInt(v["decimal"].as_str().unwrap().parse().unwrap()),
            "string" => AccountValue::String(t(v)),
            "object" => AccountValue::Object(if v["id"] == "A" {
                self.a.clone()
            } else {
                self.b.clone()
            }),
            "coercion" => AccountValue::Object(self.coercion.clone()),
            "symbol" => AccountValue::Symbol(self.symbol.clone()),
            "function" => AccountValue::Function(self.function.clone()),
            _ => panic!("unknown native fixture"),
        }
    }
    fn entry(&self, v: &Value) -> Entry {
        if v["kind"] == "thrown" {
            Entry::Throw
        } else {
            Entry::Value(self.value(v))
        }
    }
}
fn assert_value(actual: &AccountValue, expected: &Value, pool: &Pool, label: &str) {
    match actual {
        AccountValue::Undefined => {
            assert_eq!(expected["kind"], "undefined", "{label} native undefined")
        }
        AccountValue::Null => assert_eq!(expected["kind"], "null", "{label} native null"),
        AccountValue::Boolean(v) => assert_eq!(
            *v,
            expected["value"].as_bool().unwrap(),
            "{label} native boolean"
        ),
        AccountValue::Number(v) => assert_eq!(
            v.to_bits(),
            n(expected).to_bits(),
            "{label} native number bits"
        ),
        AccountValue::BigInt(v) => assert_eq!(
            *v,
            expected["decimal"]
                .as_str()
                .unwrap()
                .parse::<num_bigint::BigInt>()
                .unwrap(),
            "{label} native BigInt"
        ),
        AccountValue::String(v) => assert_eq!(*v, t(expected), "{label} native UTF16"),
        AccountValue::Object(v) => {
            assert_eq!(expected["kind"], "object");
            let want: Rc<dyn AccountObject> = if expected["id"] == "A" {
                pool.a.clone()
            } else {
                pool.b.clone()
            };
            assert!(
                Rc::ptr_eq(v, &want),
                "{label} native returned object identity"
            )
        }
        AccountValue::Symbol(v) => assert!(v.ptr_eq(&pool.symbol), "native symbol identity"),
        AccountValue::Function(v) => assert!(v.ptr_eq(&pool.function), "native function identity"),
        AccountValue::HtmlDda => panic!("browser marker unproved"),
    }
}
#[derive(Clone)]
struct Call {
    url: Vec<u16>,
    config: Option<AccountRequestConfig>,
}
struct Getter {
    runtime: Rc<Runtime>,
    trace: Trace,
    calls: Rc<RefCell<Vec<Call>>>,
    scenario: String,
    response: AccountValue,
    token: HostThrownValue,
}
impl AccountGet for Getter {
    fn call(
        &self,
        url: Vec<u16>,
        config: Option<AccountRequestConfig>,
    ) -> SigningResult<NativePromise<AccountValue>> {
        self.trace.borrow_mut().push("get.call".into());
        self.calls.borrow_mut().push(Call { url, config });
        if self.scenario == "callThrow" {
            return Err(self.token.clone().into());
        }
        Ok(NativePromise::settled(
            self.runtime.clone(),
            if self.scenario == "reject" {
                Err(self.token.clone().into())
            } else {
                Ok(self.response.clone())
            },
        ))
    }
}
struct Http {
    getter: Rc<Getter>,
}
impl AccountHttpClient for Http {
    fn get_callback(&self) -> SigningResult<Rc<dyn AccountGet>> {
        self.getter.trace.borrow_mut().push("client.get".into());
        if self.getter.scenario == "lookupThrow" {
            Err(self.getter.token.clone().into())
        } else {
            Ok(self.getter.clone())
        }
    }
}

struct NativeApiError {
    name: Vec<u16>,
    message: Vec<u16>,
    code: AccountValue,
}
struct ErrorFactory {
    trace: Trace,
}
impl MarketApiErrors for ErrorFactory {
    fn construct(
        &self,
        message: AccountValue,
        code: AccountValue,
    ) -> SigningResult<HostThrownValue> {
        self.trace.borrow_mut().push("construct".into());
        let message = Semantics.error_message(&message)?;
        Ok(HostThrownValue::new(NativeApiError {
            name: u("LighterApiError"),
            message,
            code,
        }))
    }
}
#[test]
#[ignore = "actual TypeScript market-data APIs"]
fn market_methods_native_results_and_reads() {
    let rows = capture()["cases"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for (iteration, row) in rows.iter().enumerate() {
        let trace: Trace = Rc::new(RefCell::new(vec![]));
        let token = HostThrownValue::new(vec![0xd800u16]);
        let pool = Pool {
            a: object(BTreeMap::new(), &trace, &token),
            b: object(BTreeMap::new(), &trace, &token),
            coercion: object(
                BTreeMap::from([("@@toPrimitive".into(), vec![Entry::Throw])]),
                &trace,
                &token,
            ),
            symbol: HostThrownValue::new("symbol"),
            function: HostThrownValue::new("function"),
        };
        let error = object(
            ["message", "code"]
                .into_iter()
                .map(|k| {
                    (
                        k.into(),
                        row[k]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| pool.entry(v))
                            .collect(),
                    )
                })
                .collect(),
            &trace,
            &token,
        );
        let other = object(
            BTreeMap::from([(
                "code".into(),
                vec![Entry::Value(AccountValue::String(
                    u("second").into_iter().chain([0xd800]).collect(),
                ))],
            )]),
            &trace,
            &token,
        );
        let mut fields: BTreeMap<String, Vec<Entry>> = row["response"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| pool.entry(v))
                        .collect(),
                )
            })
            .collect();
        fields.insert(
            "error".into(),
            row["errorKinds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| match v.as_str().unwrap() {
                    "error" => Entry::Value(AccountValue::Object(error.clone())),
                    "other" => Entry::Value(AccountValue::Object(other.clone())),
                    "null" => Entry::Value(AccountValue::Null),
                    "undefined" => Entry::Value(AccountValue::Undefined),
                    "thrown" => Entry::Throw,
                    _ => panic!("unknown error fixture"),
                })
                .collect(),
        );
        if row["scenario"].as_str().unwrap().starts_with("then") {
            let then = match row["scenario"].as_str().unwrap() {
                "thenGetterThrow" => Entry::Throw,
                "thenNonCallable" => Entry::Value(AccountValue::Number(17.0)),
                _ => Entry::Value(AccountValue::Function(HostThrownValue::new(Then {
                    reject: row["scenario"] == "thenReject",
                    trace: trace.clone(),
                    value: pool.b.clone(),
                    token: token.clone(),
                }))),
            };
            pool.a.fields.borrow_mut().insert("then".into(), vec![then]);
        }
        let runtime = Rc::new(Runtime::default());
        let calls = Rc::new(RefCell::new(vec![]));
        let client: Rc<dyn AccountHttpClient> = Rc::new(Http {
            getter: Rc::new(Getter {
                runtime: runtime.clone(),
                trace: trace.clone(),
                calls: calls.clone(),
                scenario: row["scenario"].as_str().unwrap().into(),
                response: AccountValue::Object(object(fields, &trace, &token)),
                token: token.clone(),
            }),
        });
        let errors = Rc::new(ErrorFactory {
            trace: trace.clone(),
        });
        let method = row["method"].as_str().unwrap();
        *counts.entry(method).or_insert(0usize) += 1;
        let promise = match method {
            "getCandlesticks" | "getFundings" => {
                let api = CandlestickApi::new(
                    client.clone(),
                    runtime.clone(),
                    Rc::new(Semantics),
                    errors,
                );
                assert_eq!(
                    Rc::strong_count(&client),
                    2,
                    "native candlestick constructor retains client"
                );
                assert!(
                    trace.borrow().is_empty(),
                    "native constructor does not read get"
                );
                let args = &row["args"];
                if method == "getCandlesticks" {
                    api.get_candlesticks(
                        n(&args[0]),
                        t(&args[1]),
                        n(&args[2]),
                        n(&args[3]),
                        n(&args[4]),
                    )
                } else {
                    api.get_fundings(
                        n(&args[0]),
                        t(&args[1]),
                        n(&args[2]),
                        n(&args[3]),
                        n(&args[4]),
                    )
                }
            }
            "getFundingRates" => {
                let api =
                    FundingApi::new(client.clone(), runtime.clone(), Rc::new(Semantics), errors);
                assert_eq!(
                    Rc::strong_count(&client),
                    2,
                    "native funding constructor retains client"
                );
                assert!(
                    trace.borrow().is_empty(),
                    "native constructor does not read get"
                );
                api.get_funding_rates()
            }
            "getWithdrawalDelay" => {
                let api = InfoApi::new(client.clone(), runtime.clone(), Rc::new(Semantics), errors);
                assert_eq!(
                    Rc::strong_count(&client),
                    2,
                    "native info constructor retains client"
                );
                assert!(
                    trace.borrow().is_empty(),
                    "native constructor does not read get"
                );
                api.get_withdrawal_delay()
            }
            _ => panic!("unknown method"),
        };
        trace.borrow_mut().push("after-call".into());
        let observed = Rc::new(RefCell::new(None));
        let out = observed.clone();
        let trace_out = trace.clone();
        let observer = promise.clone();
        drop(promise);
        observer.subscribe(move |v| {
            *out.borrow_mut() = Some(v);
            trace_out.borrow_mut().push("settled".into());
        });
        drop(observer);
        assert!(observed.borrow().is_none(), "native observer asynchronous");
        runtime.drain();
        let label = format!("{method} iteration{iteration}");
        let expected = &row["output"];
        match observed
            .borrow_mut()
            .take()
            .expect("native detached market request completes")
        {
            Ok(v) => {
                assert_eq!(expected["kind"], "ok", "{label} native result kind");
                assert_value(&v, &expected["value"], &pool, &label)
            }
            Err(TransactionSigningError::Thrown(v)) => {
                if let Some(e) = v.value().downcast_ref::<NativeApiError>() {
                    assert_eq!(expected["kind"], "error", "{label} native API error kind");
                    assert_eq!(
                        e.name,
                        units(&expected["name"]),
                        "{label} native API error name"
                    );
                    assert_eq!(
                        e.message,
                        units(&expected["message"]),
                        "{label} native API error message"
                    );
                    assert_value(
                        &e.code,
                        &expected["code"],
                        &pool,
                        &format!("{label} native API error code"),
                    );
                } else {
                    assert_eq!(expected["kind"], "thrown", "{label} native throw kind");
                    assert!(v.ptr_eq(&token), "{label} native exception identity");
                    assert_eq!(
                        v.value().downcast_ref::<Vec<u16>>().unwrap(),
                        &vec![0xd800],
                        "native exception payload"
                    );
                }
            }
            Err(TransactionSigningError::Generated(e)) => {
                assert_eq!(expected["kind"], "error");
                assert_eq!(u(e.name), units(&expected["name"]));
                assert_eq!(e.message, units(&expected["message"]));
            }
        }
        let actual = calls.borrow();
        let expected_count = usize::from(row["scenario"] != "lookupThrow");
        assert_eq!(
            actual.len(),
            expected_count,
            "independent native HTTP call count"
        );
        assert_eq!(
            row["calls"].as_array().unwrap().len(),
            expected_count,
            "independent source HTTP call count"
        );
        for (call, want) in actual.iter().zip(row["calls"].as_array().unwrap()) {
            assert_eq!(call.url, units(&want["url"]), "{label} native URL");
            let count = if method == "getCandlesticks" || method == "getFundings" {
                5
            } else {
                0
            };
            if let Some(config) = &call.config {
                assert_eq!(config.params.len(), count, "independent native param count");
                assert_eq!(
                    want["config"].as_array().unwrap().len(),
                    count,
                    "independent source param count"
                );
                for ((k, v), w) in config.params.iter().zip(want["config"].as_array().unwrap()) {
                    assert_eq!(*k, units(&w["key"]), "{label} native param key/order");
                    assert_value(v, &w["value"], &pool, &label);
                }
            } else {
                assert_eq!(count, 0);
                assert!(want["config"].is_null(), "native absent config");
            }
        }
        assert_eq!(
            *trace.borrow(),
            row["trace"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "{label} native getter/call/continuation order"
        );
    }
    assert_eq!(
        rows.len(),
        120,
        "independent market method floor, stopped after iteration119"
    );
    assert_eq!(
        counts,
        BTreeMap::from([
            ("getCandlesticks", 30),
            ("getFundings", 30),
            ("getFundingRates", 30),
            ("getWithdrawalDelay", 30)
        ]),
        "independent method counts"
    );
    assert_corpus();
}
fn assert_corpus() {
    assert_eq!(
        capture()["contentHash"],
        "2e53775707d95ab56689c3e8bb2d4d8f1ab94bb80ff77bf57f95f651be1fe060",
        "authored source input/result corpus after native assertions"
    );
}

const NUMBER_BITS: [u64; 7] = [
    0x8000000000000000,
    0x7ff8000000000000,
    0x7ff0000000000000,
    0xfff0000000000000,
    0x4340000000000000,
    0x4031400000000000,
    1,
];
fn expected_text(label: &str, v: usize) -> Vec<u16> {
    u(&format!("{label}.{v}"))
        .into_iter()
        .chain([0xd800, 0xd83d, 0xdc69])
        .collect()
}
fn check_Candlestick(input: &Value, variant: usize) {
    assert_Candlestick(&decode_Candlestick(input), variant);
}
fn decode_Candlestick(input: &Value) -> Candlestick {
    Candlestick {
        timestamp: n(&input["timestamp"]),
        open: t(&input["open"]),
        high: t(&input["high"]),
        low: t(&input["low"]),
        close: t(&input["close"]),
        volume: t(&input["volume"]),
    }
}
fn assert_Candlestick(native: &Candlestick, variant: usize) {
    assert_eq!(
        native.timestamp.to_bits(),
        NUMBER_BITS[(0 + variant) % 7],
        "native Candlestick.timestamp"
    );
    assert_eq!(
        native.open.clone(),
        expected_text("Candlestick.open", variant),
        "native Candlestick.open"
    );
    assert_eq!(
        native.high.clone(),
        expected_text("Candlestick.high", variant),
        "native Candlestick.high"
    );
    assert_eq!(
        native.low.clone(),
        expected_text("Candlestick.low", variant),
        "native Candlestick.low"
    );
    assert_eq!(
        native.close.clone(),
        expected_text("Candlestick.close", variant),
        "native Candlestick.close"
    );
    assert_eq!(
        native.volume.clone(),
        expected_text("Candlestick.volume", variant),
        "native Candlestick.volume"
    );
}
fn check_CandlesticksResponse(input: &Value, variant: usize) {
    assert_CandlesticksResponse(&decode_CandlesticksResponse(input), variant);
}
fn decode_CandlesticksResponse(input: &Value) -> CandlesticksResponse {
    CandlesticksResponse {
        candlesticks: input["candlesticks"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode_Candlestick)
            .collect(),
        market_id: n(&input["marketId"]),
        resolution: t(&input["resolution"]),
    }
}
fn assert_CandlesticksResponse(native: &CandlesticksResponse, variant: usize) {
    assert_eq!(native.candlesticks.len(), if variant == 0 { 0 } else { 4 });
    for (i, v) in native.candlesticks.iter().enumerate() {
        assert_Candlestick(v, [2, 1, 2, 0][i]);
    }
    assert_eq!(
        native.market_id.to_bits(),
        NUMBER_BITS[(1 + variant) % 7],
        "native CandlesticksResponse.marketId"
    );
    assert_eq!(
        native.resolution.clone(),
        expected_text("CandlesticksResponse.resolution", variant),
        "native CandlesticksResponse.resolution"
    );
}
fn check_Funding(input: &Value, variant: usize) {
    assert_Funding(&decode_Funding(input), variant);
}
fn decode_Funding(input: &Value) -> Funding {
    Funding {
        timestamp: n(&input["timestamp"]),
        rate: t(&input["rate"]),
        mark_price: input.get("markPrice").map(t),
        index_price: input.get("indexPrice").map(t),
    }
}
fn assert_Funding(native: &Funding, variant: usize) {
    assert_eq!(
        native.timestamp.to_bits(),
        NUMBER_BITS[(0 + variant) % 7],
        "native Funding.timestamp"
    );
    assert_eq!(
        native.rate.clone(),
        expected_text("Funding.rate", variant),
        "native Funding.rate"
    );
    assert_eq!(
        native.mark_price.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("Funding.markPrice", variant))
        },
        "native Funding.markPrice"
    );
    assert_eq!(
        native.index_price.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("Funding.indexPrice", variant))
        },
        "native Funding.indexPrice"
    );
}
fn check_FundingsResponse(input: &Value, variant: usize) {
    assert_FundingsResponse(&decode_FundingsResponse(input), variant);
}
fn decode_FundingsResponse(input: &Value) -> FundingsResponse {
    FundingsResponse {
        fundings: input["fundings"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode_Funding)
            .collect(),
        market_id: n(&input["marketId"]),
        resolution: t(&input["resolution"]),
    }
}
fn assert_FundingsResponse(native: &FundingsResponse, variant: usize) {
    assert_eq!(native.fundings.len(), if variant == 0 { 0 } else { 4 });
    for (i, v) in native.fundings.iter().enumerate() {
        assert_Funding(v, [2, 1, 2, 0][i]);
    }
    assert_eq!(
        native.market_id.to_bits(),
        NUMBER_BITS[(1 + variant) % 7],
        "native FundingsResponse.marketId"
    );
    assert_eq!(
        native.resolution.clone(),
        expected_text("FundingsResponse.resolution", variant),
        "native FundingsResponse.resolution"
    );
}
fn check_FundingRate(input: &Value, variant: usize) {
    assert_FundingRate(&decode_FundingRate(input), variant);
}
fn decode_FundingRate(input: &Value) -> FundingRate {
    FundingRate {
        market_id: n(&input["marketId"]),
        symbol: input.get("symbol").map(t),
        rate: t(&input["rate"]),
        next_funding_time: input.get("nextFundingTime").map(n),
        predicted_rate: input.get("predictedRate").map(t),
        mark_price: input.get("markPrice").map(t),
        index_price: input.get("indexPrice").map(t),
        timestamp: input.get("timestamp").map(n),
    }
}
fn assert_FundingRate(native: &FundingRate, variant: usize) {
    assert_eq!(
        native.market_id.to_bits(),
        NUMBER_BITS[(0 + variant) % 7],
        "native FundingRate.marketId"
    );
    assert_eq!(
        native.symbol.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("FundingRate.symbol", variant))
        },
        "native FundingRate.symbol"
    );
    assert_eq!(
        native.rate.clone(),
        expected_text("FundingRate.rate", variant),
        "native FundingRate.rate"
    );
    assert_eq!(
        native.next_funding_time.map(|v| v.to_bits()),
        if variant == 0 {
            None
        } else {
            Some(NUMBER_BITS[(3 + variant) % 7])
        },
        "native FundingRate.nextFundingTime"
    );
    assert_eq!(
        native.predicted_rate.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("FundingRate.predictedRate", variant))
        },
        "native FundingRate.predictedRate"
    );
    assert_eq!(
        native.mark_price.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("FundingRate.markPrice", variant))
        },
        "native FundingRate.markPrice"
    );
    assert_eq!(
        native.index_price.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("FundingRate.indexPrice", variant))
        },
        "native FundingRate.indexPrice"
    );
    assert_eq!(
        native.timestamp.map(|v| v.to_bits()),
        if variant == 0 {
            None
        } else {
            // FIXME(lighter): `(7 + variant) % 7` is arithmetically
            // `(0 + variant) % 7`, so this asserts the SAME bits as
            // FundingRate.marketId above - a swap between those two fields
            // would pass. Every other offset in this file is distinct (0, 1,
            // 3), so 7 reads as an intended distinct field that aliases row 0.
            // A different offset only works if the captured corpus carries
            // those bits for this field, which is the owning lane's call.
            Some(NUMBER_BITS[(7 + variant) % 7])
        },
        "native FundingRate.timestamp"
    );
}
fn check_FundingRatesResponse(input: &Value, variant: usize) {
    assert_FundingRatesResponse(&decode_FundingRatesResponse(input), variant);
}
fn decode_FundingRatesResponse(input: &Value) -> FundingRatesResponse {
    FundingRatesResponse {
        funding_rates: input["fundingRates"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode_FundingRate)
            .collect(),
    }
}
fn assert_FundingRatesResponse(native: &FundingRatesResponse, variant: usize) {
    assert_eq!(native.funding_rates.len(), if variant == 0 { 0 } else { 4 });
    for (i, v) in native.funding_rates.iter().enumerate() {
        assert_FundingRate(v, [2, 1, 2, 0][i]);
    }
}
fn check_WithdrawalDelay(input: &Value, variant: usize) {
    assert_WithdrawalDelay(&decode_WithdrawalDelay(input), variant);
}
fn decode_WithdrawalDelay(input: &Value) -> WithdrawalDelay {
    WithdrawalDelay {
        delay_seconds: n(&input["delaySeconds"]),
        delay_ms: input.get("delayMs").map(n),
        description: input.get("description").map(t),
    }
}
fn assert_WithdrawalDelay(native: &WithdrawalDelay, variant: usize) {
    assert_eq!(
        native.delay_seconds.to_bits(),
        NUMBER_BITS[(0 + variant) % 7],
        "native WithdrawalDelay.delaySeconds"
    );
    assert_eq!(
        native.delay_ms.map(|v| v.to_bits()),
        if variant == 0 {
            None
        } else {
            Some(NUMBER_BITS[(1 + variant) % 7])
        },
        "native WithdrawalDelay.delayMs"
    );
    assert_eq!(
        native.description.clone(),
        if variant == 0 {
            None
        } else {
            Some(expected_text("WithdrawalDelay.description", variant))
        },
        "native WithdrawalDelay.description"
    );
}
fn check_WithdrawalDelayResponse(input: &Value, variant: usize) {
    assert_WithdrawalDelayResponse(&decode_WithdrawalDelayResponse(input), variant);
}
fn decode_WithdrawalDelayResponse(input: &Value) -> WithdrawalDelayResponse {
    WithdrawalDelayResponse {
        withdrawal_delay: decode_WithdrawalDelay(&input["withdrawalDelay"]),
    }
}
fn assert_WithdrawalDelayResponse(native: &WithdrawalDelayResponse, variant: usize) {
    assert_WithdrawalDelay(&native.withdrawal_delay, variant);
}
#[test]
#[ignore = "actual TypeScript market-data interface schemas and native fields"]
fn market_models_native_fields() {
    let rows = capture()["models"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for row in rows {
        let name = row["record"].as_str().unwrap();
        let v = row["variant"].as_u64().unwrap() as usize;
        *counts.entry(name).or_insert(0usize) += 1;
        match name {
            "Candlestick" => check_Candlestick(&row["input"], v),
            "CandlesticksResponse" => check_CandlesticksResponse(&row["input"], v),
            "Funding" => check_Funding(&row["input"], v),
            "FundingsResponse" => check_FundingsResponse(&row["input"], v),
            "FundingRate" => check_FundingRate(&row["input"], v),
            "FundingRatesResponse" => check_FundingRatesResponse(&row["input"], v),
            "WithdrawalDelay" => check_WithdrawalDelay(&row["input"], v),
            "WithdrawalDelayResponse" => check_WithdrawalDelayResponse(&row["input"], v),
            _ => panic!("unknown schema"),
        }
    }
    assert_eq!(
        rows.len(),
        32,
        "independent model floor, stopped after iteration31"
    );
    assert_eq!(
        counts,
        BTreeMap::from([
            ("Candlestick", 4),
            ("CandlesticksResponse", 4),
            ("Funding", 4),
            ("FundingsResponse", 4),
            ("FundingRate", 4),
            ("FundingRatesResponse", 4),
            ("WithdrawalDelay", 4),
            ("WithdrawalDelayResponse", 4)
        ]),
        "independent model variant counts"
    );
    assert_corpus();
}
