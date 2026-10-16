use safe_cex_venue_lighter::{sdk_account_api::*, transaction_promise::*, transaction_signing::*};
use safe_cex_venue_lighter::{sdk_order_queries::*, transaction_builders::TransactionBuildError};
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
                "/scripts/order-queries-oracle.cjs"
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
struct Coercion {
    token: HostThrownValue,
}
impl AccountObject for Coercion {
    fn get(&self, _: &str) -> SigningResult<AccountValue> {
        Err(self.token.clone().into())
    }
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
    extras: RefCell<BTreeMap<String, Rc<Object>>>,
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
            "tuple" | "converter" => {
                AccountValue::Object(self.extras.borrow()[v["id"].as_str().unwrap()].clone())
            }
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
fn prefixed_object(
    fields: BTreeMap<String, Vec<Entry>>,
    trace: &Trace,
    token: &HostThrownValue,
    prefix: &str,
) -> Rc<Object> {
    Rc::new(Object {
        fields: RefCell::new(fields),
        counts: RefCell::new(BTreeMap::new()),
        trace: trace.clone(),
        token: token.clone(),
        prefix: prefix.into(),
    })
}
#[derive(Clone)]
struct Converter {
    receiver: std::rc::Weak<Object>,
    result: AccountValue,
    should_throw: bool,
    trace: Trace,
    token: HostThrownValue,
}
struct Query {
    scenario: String,
    trace: Trace,
    token: HostThrownValue,
    a: Rc<Object>,
}
impl OrderQuerySemantics for Query {
    fn to_string_method(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        match value {
            AccountValue::Number(v) => {
                if self.scenario == "toStringOverride" {
                    self.trace.borrow_mut().push("number.toString".into());
                    Ok(AccountValue::Object(self.a.clone()))
                } else if self.scenario == "toStringGetterThrow" {
                    self.trace.borrow_mut().push("number.toString.get".into());
                    Err(self.token.clone().into())
                } else {
                    Ok(AccountValue::String(u(&safe_cex_num::js_number_string(*v))))
                }
            }
            AccountValue::BigInt(v) => Ok(AccountValue::String(u(&v.to_string()))),
            AccountValue::Boolean(v) => {
                Ok(AccountValue::String(u(if *v { "true" } else { "false" })))
            }
            AccountValue::String(v) => Ok(AccountValue::String(v.clone())),
            AccountValue::Undefined | AccountValue::Null => Err(TransactionBuildError {
                name: "TypeError",
                message: u(&format!(
                    "Cannot read properties of {} (reading 'toString')",
                    if matches!(value, AccountValue::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
            }
            .into()),
            AccountValue::Object(receiver) => {
                let callback = receiver.get("toString")?;
                let AccountValue::Function(callback) = callback else {
                    panic!("converter witness callable")
                };
                let call = callback.value().downcast_ref::<Converter>().unwrap();
                let expected: Rc<dyn AccountObject> = call.receiver.upgrade().unwrap();
                assert!(Rc::ptr_eq(receiver, &expected), "native toString receiver");
                call.trace
                    .borrow_mut()
                    .push("converter.toString.call".into());
                if call.should_throw {
                    Err(call.token.clone().into())
                } else {
                    Ok(call.result.clone())
                }
            }
            _ => panic!("toString witness primitive domain"),
        }
    }
}
#[test]
#[ignore = "actual TypeScript ordinary OrderApi methods"]
fn order_methods_native_results_and_reads() {
    let rows = capture()["cases"].as_array().unwrap();
    let mut methods = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    for (iteration, row) in rows.iter().enumerate() {
        let trace: Trace = Rc::new(RefCell::new(vec![]));
        let token =
            HostThrownValue::new(u("token").into_iter().chain([0xd800]).collect::<Vec<_>>());
        let pool = Pool {
            a: object(BTreeMap::new(), &trace, &token),
            b: object(BTreeMap::new(), &trace, &token),
            coercion: Rc::new(Coercion {
                token: token.clone(),
            }),
            symbol: HostThrownValue::new("symbol"),
            function: HostThrownValue::new("function"),
            extras: RefCell::new(BTreeMap::new()),
        };
        for (id, v) in row["converters"].as_object().unwrap() {
            let receiver = prefixed_object(BTreeMap::new(), &trace, &token, "converter");
            receiver.fields.borrow_mut().insert(
                "toString".into(),
                vec![Entry::Value(AccountValue::Function(HostThrownValue::new(
                    Converter {
                        receiver: Rc::downgrade(&receiver),
                        result: pool.value(&v["value"]),
                        should_throw: v["shouldThrow"] == true,
                        trace: trace.clone(),
                        token: token.clone(),
                    },
                )))],
            );
            pool.extras.borrow_mut().insert(id.clone(), receiver);
        }
        for (id, vs) in row["tuples"].as_object().unwrap() {
            let fields = vs
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), vec![pool.entry(v)]))
                .collect();
            pool.extras
                .borrow_mut()
                .insert(id.clone(), prefixed_object(fields, &trace, &token, id));
        }
        let error = object(
            BTreeMap::from([(
                "message".into(),
                row["message"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| pool.entry(v))
                    .collect(),
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
            vec![if row["errorThrow"] == true {
                Entry::Throw
            } else {
                match row["errorKind"].as_str().unwrap() {
                    "null" => Entry::Value(AccountValue::Null),
                    "undefined" => Entry::Value(AccountValue::Undefined),
                    _ => Entry::Value(AccountValue::Object(error)),
                }
            }],
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
        let response = object(fields, &trace, &token);
        let options = if row["options"].is_null() {
            None
        } else {
            Some(AccountValue::Object(object(
                row["options"]
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
                    .collect(),
                &trace,
                &token,
            )))
        };
        let runtime = Rc::new(Runtime::default());
        let calls = Rc::new(RefCell::new(vec![]));
        let getter = Rc::new(Getter {
            runtime: runtime.clone(),
            trace: trace.clone(),
            calls: calls.clone(),
            scenario: row["scenario"].as_str().unwrap().into(),
            response: AccountValue::Object(response),
            token: token.clone(),
        });
        let client: Rc<dyn AccountHttpClient> = Rc::new(Http { getter });
        let api = OrderApi::new(
            client.clone(),
            runtime.clone(),
            Rc::new(Semantics),
            Rc::new(Query {
                scenario: row["scenario"].as_str().unwrap().into(),
                trace: trace.clone(),
                token: token.clone(),
                a: pool.a.clone(),
            }),
        );
        assert_eq!(
            Rc::strong_count(&client),
            2,
            "native order constructor retains original client"
        );
        let method = row["method"].as_str().unwrap();
        *methods.entry(method).or_insert(0usize) += 1;
        let index = row["index"]["decimal"].as_str().unwrap().parse().unwrap();
        let market = if row["market"]["kind"] == "undefined" {
            None
        } else {
            Some(n(&row["market"]))
        };
        let limit = if row["limit"]["kind"] == "undefined" {
            None
        } else {
            Some(n(&row["limit"]))
        };
        let promise = match method {
            "getAccountActiveOrders" => api.get_account_active_orders(&index, market),
            "getAccountInactiveOrders" => api.get_account_inactive_orders(&index, options.as_ref()),
            "getOrderBooks" => api.get_order_books(market),
            "getOrderBookDetails" => api.get_order_book_details(market),
            "getRecentTrades" => api.get_recent_trades(n(&row["recentMarket"]), limit),
            "getTrades" => api.get_trades(options.as_ref()),
            "getExchangeStats" => api.get_exchange_stats(),
            _ => panic!("unported method"),
        };
        trace.borrow_mut().push("after-call".into());
        let observed = Rc::new(RefCell::new(None));
        let out = observed.clone();
        let trace_out = trace.clone();
        let observer = promise.clone();
        drop(promise);
        observer.subscribe(move |v| {
            *out.borrow_mut() = Some(v);
            trace_out.borrow_mut().push("settled".into())
        });
        drop(observer);
        assert!(
            observed.borrow().is_none(),
            "native order observer asynchronous"
        );
        runtime.drain();
        let result = observed
            .borrow_mut()
            .take()
            .expect("native detached order work completes");
        let expected = &row["output"];
        let label = format!("{method} iteration{iteration}");
        match result {
            Ok(v) => {
                assert_eq!(expected["kind"], "ok", "{label} native result kind");
                assert_value(&v, &expected["value"], &pool, &label)
            }
            Err(TransactionSigningError::Thrown(v)) => {
                assert_eq!(expected["kind"], "thrown", "{label} native throw kind");
                assert!(v.ptr_eq(&token), "{label} native exception identity");
                assert_eq!(
                    v.value().downcast_ref::<Vec<u16>>().unwrap(),
                    &u("token").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                    "native exception payload"
                )
            }
            Err(TransactionSigningError::Generated(e)) => {
                assert_eq!(
                    expected["kind"], "error",
                    "{label} native generated error kind"
                );
                assert_eq!(
                    u(e.name),
                    units(&expected["name"]),
                    "{label} native error name"
                );
                assert_eq!(
                    e.message,
                    units(&expected["message"]),
                    "{label} native error UTF16 message"
                )
            }
        }
        *outcomes
            .entry(expected["kind"].as_str().unwrap())
            .or_insert(0usize) += 1;
        let native_calls = calls.borrow();
        assert_eq!(
            native_calls.len(),
            row["calls"].as_array().unwrap().len(),
            "source expected native call count"
        );
        for (actual, want) in native_calls.iter().zip(row["calls"].as_array().unwrap()) {
            assert_eq!(actual.url, units(&want["url"]), "{label} native URL");
            match (&actual.config, &want["config"]) {
                (None, v) => assert!(v.is_null(), "native absent config"),
                (Some(v), w) => {
                    let w = w.as_array().unwrap();
                    assert_eq!(
                        v.params.len(),
                        w.len(),
                        "source expected native param count"
                    );
                    for ((k, v), w) in v.params.iter().zip(w) {
                        assert_eq!(*k, units(&w["key"]), "{label} native param key/order");
                        assert_value(v, &w["value"], &pool, &label)
                    }
                }
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
    assert_eq!(rows.len(), 251, "independent order method floor");
    assert_eq!(
        methods,
        BTreeMap::from([
            ("getAccountActiveOrders", 31),
            ("getAccountInactiveOrders", 50),
            ("getOrderBooks", 31),
            ("getOrderBookDetails", 31),
            ("getRecentTrades", 41),
            ("getTrades", 44),
            ("getExchangeStats", 23)
        ]),
        "independent order method counts"
    );
    assert_eq!(
        outcomes,
        BTreeMap::from([("ok", 113), ("error", 52), ("thrown", 86)]),
        "independent order outcomes"
    );
    assert_corpus();
}
fn assert_corpus() {
    assert_eq!(
        capture()["contentHash"],
        "a2ba42777218eb7545002ddffbd6b185820730068a52a4008035d360c4281a87",
        "independent order source corpus pin"
    );
}

fn opt_n(v: &Value, key: &str) -> Option<f64> {
    v.get(key).map(n)
}
fn opt_t(v: &Value, key: &str) -> Option<Vec<u16>> {
    v.get(key).map(t)
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
fn expected_text(field: &str, i: usize) -> Vec<u16> {
    format!("{field}.{i}")
        .encode_utf16()
        .chain([0xd800, 0xd83d, 0xdc69])
        .collect()
}
macro_rules! text_field {
    ($actual:expr,$field:literal,$i:expr) => {
        assert_eq!(
            $actual,
            expected_text($field, $i),
            concat!("native ", $field, " UTF16 field")
        )
    };
}
macro_rules! optional_text {
    ($actual:expr,$field:literal,$i:expr) => {
        assert_eq!(
            $actual,
            if $i == 0 {
                None
            } else {
                Some(expected_text($field, $i))
            },
            concat!("native ", $field, " optional UTF16")
        )
    };
}
macro_rules! number_field {
    ($actual:expr,$field:literal,$ordinal:expr,$i:expr) => {
        assert_eq!(
            $actual.to_bits(),
            NUMBER_BITS[($ordinal + $i) % 7],
            concat!("native ", $field, " number bits")
        )
    };
}
macro_rules! optional_number {
    ($actual:expr,$field:literal,$ordinal:expr,$i:expr) => {
        assert_eq!(
            $actual.map(f64::to_bits),
            if $i == 0 {
                None
            } else {
                Some(NUMBER_BITS[($ordinal + $i) % 7])
            },
            concat!("native ", $field, " optional number bits")
        )
    };
}
fn side(v: &Value) -> TradeSide {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "buy" => TradeSide::Buy,
        "sell" => TradeSide::Sell,
        _ => panic!("unknown side"),
    }
}
fn inactive_sort(v: &Value) -> InactiveSortBy {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "createdAt" => InactiveSortBy::CreatedAt,
        "updatedAt" => InactiveSortBy::UpdatedAt,
        "price" => InactiveSortBy::Price,
        _ => panic!("unknown inactive sort"),
    }
}
fn trades_sort(v: &Value) -> TradesSortBy {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "timestamp" => TradesSortBy::Timestamp,
        "price" => TradesSortBy::Price,
        _ => panic!("unknown trade sort"),
    }
}
fn sort_order(v: &Value) -> SortOrder {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "asc" => SortOrder::Asc,
        "desc" => SortOrder::Desc,
        _ => panic!("unknown sort order"),
    }
}
fn trade(v: &Value) -> ApiTrade {
    ApiTrade {
        trade_id: t(&v["tradeId"]),
        market_id: n(&v["marketId"]),
        symbol: t(&v["symbol"]),
        price: t(&v["price"]),
        quantity: t(&v["quantity"]),
        quote_quantity: t(&v["quoteQuantity"]),
        side: side(&v["side"]),
        is_buyer_maker: v["isBuyerMaker"]["value"].as_bool().unwrap(),
        timestamp: n(&v["timestamp"]),
        maker_account_index: opt_n(v, "makerAccountIndex"),
        taker_account_index: opt_n(v, "takerAccountIndex"),
    }
}
fn assert_trade(x: &ApiTrade, i: usize) {
    text_field!(x.trade_id, "ApiTrade.tradeId", i);
    number_field!(x.market_id, "ApiTrade.marketId", 1, i);
    text_field!(x.symbol, "ApiTrade.symbol", i);
    text_field!(x.price, "ApiTrade.price", i);
    text_field!(x.quantity, "ApiTrade.quantity", i);
    text_field!(x.quote_quantity, "ApiTrade.quoteQuantity", i);
    assert_eq!(
        x.side,
        if i.is_multiple_of(2) {
            TradeSide::Buy
        } else {
            TradeSide::Sell
        },
        "native trade side variant"
    );
    assert_eq!(x.is_buyer_maker, i == 1, "native buyer maker boolean");
    number_field!(x.timestamp, "ApiTrade.timestamp", 8, i);
    optional_number!(x.maker_account_index, "ApiTrade.makerAccountIndex", 9, i);
    optional_number!(x.taker_account_index, "ApiTrade.takerAccountIndex", 10, i);
}
fn market_stats(v: &Value) -> MarketStats {
    MarketStats {
        market_id: n(&v["marketId"]),
        symbol: t(&v["symbol"]),
        volume24h: t(&v["volume24h"]),
        trades24h: n(&v["trades24h"]),
        price_change24h: t(&v["priceChange24h"]),
        price_change_percent24h: t(&v["priceChangePercent24h"]),
        high24h: t(&v["high24h"]),
        low24h: t(&v["low24h"]),
        last_price: t(&v["lastPrice"]),
    }
}
fn assert_market(x: &MarketStats, i: usize) {
    number_field!(x.market_id, "MarketStats.marketId", 0, i);
    text_field!(x.symbol, "MarketStats.symbol", i);
    text_field!(x.volume24h, "MarketStats.volume24h", i);
    number_field!(x.trades24h, "MarketStats.trades24h", 3, i);
    text_field!(x.price_change24h, "MarketStats.priceChange24h", i);
    text_field!(
        x.price_change_percent24h,
        "MarketStats.priceChangePercent24h",
        i
    );
    text_field!(x.high24h, "MarketStats.high24h", i);
    text_field!(x.low24h, "MarketStats.low24h", i);
    text_field!(x.last_price, "MarketStats.lastPrice", i);
}
fn exchange_stats(v: &Value) -> ExchangeStats {
    ExchangeStats {
        volume24h: t(&v["volume24h"]),
        trades24h: n(&v["trades24h"]),
        total_markets: n(&v["totalMarkets"]),
        active_orders: n(&v["activeOrders"]),
        total_accounts: n(&v["totalAccounts"]),
        tvl: t(&v["tvl"]),
        market_stats: v["marketStats"]
            .as_array()
            .unwrap()
            .iter()
            .map(market_stats)
            .collect(),
        timestamp: n(&v["timestamp"]),
    }
}
fn assert_stats(x: &ExchangeStats, i: usize) {
    text_field!(x.volume24h, "ExchangeStats.volume24h", i);
    number_field!(x.trades24h, "ExchangeStats.trades24h", 1, i);
    number_field!(x.total_markets, "ExchangeStats.totalMarkets", 2, i);
    number_field!(x.active_orders, "ExchangeStats.activeOrders", 3, i);
    number_field!(x.total_accounts, "ExchangeStats.totalAccounts", 4, i);
    text_field!(x.tvl, "ExchangeStats.tvl", i);
    number_field!(x.timestamp, "ExchangeStats.timestamp", 7, i);
    assert_eq!(
        x.market_stats.len(),
        if i == 0 { 0 } else { 4 },
        "independent market stats count"
    );
    if i != 0 {
        for (x, j) in x.market_stats.iter().zip([2, 1, 2, 0]) {
            assert_market(x, j)
        }
    }
}
#[test]
#[ignore = "actual TypeScript inherited order declarations and native values"]
fn order_models_native_fields() {
    let rows = capture()["models"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for row in rows {
        let name = row["record"].as_str().unwrap();
        let i = row["variant"].as_u64().unwrap() as usize;
        let v = &row["input"];
        *counts.entry(name).or_insert(0usize) += 1;
        match name {
            "ApiTrade" => assert_trade(&trade(v), i),
            "TradesResponse" => {
                let x = TradesResponse {
                    trades: v["trades"].as_array().unwrap().iter().map(trade).collect(),
                    total: opt_n(v, "total"),
                    next_cursor: opt_t(v, "nextCursor"),
                };
                optional_number!(x.total, "TradesResponse.total", 1, i);
                optional_text!(x.next_cursor, "TradesResponse.nextCursor", i);
                assert_eq!(
                    x.trades.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent trades count"
                );
                if i != 0 {
                    for (x, j) in x.trades.iter().zip([2, 1, 2, 0]) {
                        assert_trade(x, j)
                    }
                }
            }
            "RecentTradesResponse" => {
                let x = RecentTradesResponse {
                    market_id: n(&v["marketId"]),
                    trades: v["trades"].as_array().unwrap().iter().map(trade).collect(),
                };
                number_field!(x.market_id, "RecentTradesResponse.marketId", 0, i);
                assert_eq!(
                    x.trades.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent recent trades count"
                );
                if i != 0 {
                    for (x, j) in x.trades.iter().zip([2, 1, 2, 0]) {
                        assert_trade(x, j)
                    }
                }
            }
            "ExchangeStats" => assert_stats(&exchange_stats(v), i),
            "ExchangeStatsResponse" => {
                let x = ExchangeStatsResponse {
                    stats: exchange_stats(&v["stats"]),
                };
                assert_stats(&x.stats, i);
            }
            "GetInactiveOrdersOptions" => {
                let x = GetInactiveOrdersOptions {
                    market_id: opt_n(v, "marketId"),
                    limit: opt_n(v, "limit"),
                    ask_filter: v.get("askFilter").map(|v| v["value"].as_bool().unwrap()),
                    between_timestamps: v.get("betweenTimestamps").map(|v| [n(&v[0]), n(&v[1])]),
                    cursor: opt_t(v, "cursor"),
                    sort_by: v.get("sortBy").map(inactive_sort),
                    sort_order: v.get("sortOrder").map(sort_order),
                };
                optional_number!(x.market_id, "GetInactiveOrdersOptions.marketId", 0, i);
                optional_number!(x.limit, "GetInactiveOrdersOptions.limit", 1, i);
                assert_eq!(
                    x.ask_filter,
                    if i == 0 { None } else { Some(i == 1) },
                    "native optional ask filter"
                );
                assert_eq!(
                    x.between_timestamps.map(|v| v.map(f64::to_bits)),
                    if i == 0 {
                        None
                    } else {
                        Some([NUMBER_BITS[(i + 3) % 7], NUMBER_BITS[(i + 5) % 7]])
                    },
                    "native ordered timestamp tuple bits"
                );
                optional_text!(x.cursor, "GetInactiveOrdersOptions.cursor", i);
                assert_eq!(
                    x.sort_by,
                    if i == 0 {
                        None
                    } else {
                        Some(
                            [
                                InactiveSortBy::CreatedAt,
                                InactiveSortBy::UpdatedAt,
                                InactiveSortBy::Price,
                            ][i % 3],
                        )
                    },
                    "native inactive sort variant"
                );
                assert_eq!(
                    x.sort_order,
                    if i == 0 {
                        None
                    } else {
                        Some(if i.is_multiple_of(2) {
                            SortOrder::Asc
                        } else {
                            SortOrder::Desc
                        })
                    },
                    "native inactive sort order"
                );
            }
            "GetTradesOptions" => {
                let x = GetTradesOptions {
                    market_id: opt_n(v, "marketId"),
                    account_index: v.get("accountIndex").map(|v| {
                        v["decimal"]
                            .as_str()
                            .unwrap()
                            .parse::<num_bigint::BigInt>()
                            .unwrap()
                    }),
                    limit: opt_n(v, "limit"),
                    start_time: opt_n(v, "startTime"),
                    end_time: opt_n(v, "endTime"),
                    cursor: opt_t(v, "cursor"),
                    sort_by: v.get("sortBy").map(trades_sort),
                    sort_order: v.get("sortOrder").map(sort_order),
                };
                optional_number!(x.market_id, "GetTradesOptions.marketId", 0, i);
                assert_eq!(
                    x.account_index,
                    if i == 0 {
                        None
                    } else {
                        Some(-(num_bigint::BigInt::from(1) << 210usize))
                    },
                    "native unbounded account index"
                );
                optional_number!(x.limit, "GetTradesOptions.limit", 2, i);
                optional_number!(x.start_time, "GetTradesOptions.startTime", 3, i);
                optional_number!(x.end_time, "GetTradesOptions.endTime", 4, i);
                optional_text!(x.cursor, "GetTradesOptions.cursor", i);
                assert_eq!(
                    x.sort_by,
                    if i == 0 {
                        None
                    } else {
                        Some(if i.is_multiple_of(2) {
                            TradesSortBy::Timestamp
                        } else {
                            TradesSortBy::Price
                        })
                    },
                    "native trades sort variant"
                );
                assert_eq!(
                    x.sort_order,
                    if i == 0 {
                        None
                    } else {
                        Some(if i.is_multiple_of(2) {
                            SortOrder::Asc
                        } else {
                            SortOrder::Desc
                        })
                    },
                    "native trades sort order"
                );
            }
            _ => panic!("unmapped record"),
        }
    }
    assert_eq!(rows.len(), 28, "independent order record floor");
    assert_eq!(
        counts,
        BTreeMap::from([
            ("ApiTrade", 4),
            ("TradesResponse", 4),
            ("RecentTradesResponse", 4),
            ("ExchangeStats", 4),
            ("ExchangeStatsResponse", 4),
            ("GetInactiveOrdersOptions", 4),
            ("GetTradesOptions", 4)
        ]),
        "independent order declaration counts"
    );
    for (native, literal) in [(TradeSide::Buy, "buy"), (TradeSide::Sell, "sell")] {
        assert_eq!(native.as_str(), literal, "native trade side string")
    }
    for (native, literal) in [
        (InactiveSortBy::CreatedAt, "createdAt"),
        (InactiveSortBy::UpdatedAt, "updatedAt"),
        (InactiveSortBy::Price, "price"),
    ] {
        assert_eq!(native.as_str(), literal, "native inactive sort string")
    }
    for (native, literal) in [
        (TradesSortBy::Timestamp, "timestamp"),
        (TradesSortBy::Price, "price"),
    ] {
        assert_eq!(native.as_str(), literal, "native trades sort string")
    }
    for (native, literal) in [(SortOrder::Asc, "asc"), (SortOrder::Desc, "desc")] {
        assert_eq!(native.as_str(), literal, "native sort order string")
    }
    assert_corpus();
}
