use safe_cex_venue_lighter::{sdk_account_api::*, transaction_promise::*, transaction_signing::*};
use safe_cex_venue_lighter::{sdk_transaction_api::*, transaction_builders::TransactionBuildError};
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
                "/scripts/transaction-api-oracle.cjs"
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
            "container" | "array" | "filter" => {
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
struct Join {
    receiver: std::rc::Weak<Object>,
    result: AccountValue,
    should_throw: bool,
    trace: Trace,
    token: HostThrownValue,
}
struct Query;
impl TransactionQuerySemantics for Query {
    fn greater_than_zero(&self, value: &AccountValue) -> SigningResult<bool> {
        match value {
            AccountValue::Number(v) => Ok(*v > 0.0),
            AccountValue::BigInt(v) => Ok(v > &num_bigint::BigInt::from(0)),
            AccountValue::Symbol(_) => Err(TransactionBuildError {
                name: "TypeError",
                message: u("Cannot convert a Symbol value to a number"),
            }
            .into()),
            _ => panic!("comparison witness domain"),
        }
    }
    fn join(&self, value: &AccountValue, separator: &[u16]) -> SigningResult<AccountValue> {
        let AccountValue::Object(receiver) = value else {
            panic!("join witness domain")
        };
        let callback = receiver.get("join")?;
        let AccountValue::Function(callback) = callback else {
            panic!("join witness callable")
        };
        let call = callback.value().downcast_ref::<Join>().unwrap();
        let expected: Rc<dyn AccountObject> = call.receiver.upgrade().unwrap();
        assert!(
            Rc::ptr_eq(receiver, &expected),
            "native third types join receiver"
        );
        assert_eq!(separator, &[44], "native join separator");
        call.trace.borrow_mut().push("types.join.call".into());
        if call.should_throw {
            Err(call.token.clone().into())
        } else {
            Ok(call.result.clone())
        }
    }
}
#[test]
#[ignore = "actual TypeScript TransactionApi method oracle"]
fn transaction_methods_native_results_and_reads() {
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
        for id in ["D1", "D2"] {
            let transaction = if id == "D2" {
                Entry::Value(AccountValue::Object(pool.b.clone()))
            } else if row["scenario"] == "transactionThrow" {
                Entry::Throw
            } else if row["scenario"] == "transactionUndefined" {
                Entry::Value(AccountValue::Undefined)
            } else {
                Entry::Value(AccountValue::Object(pool.a.clone()))
            };
            pool.extras.borrow_mut().insert(
                id.into(),
                prefixed_object(
                    BTreeMap::from([
                        ("transaction".into(), vec![transaction]),
                        ("then".into(), vec![Entry::Throw]),
                    ]),
                    &trace,
                    &token,
                    "container",
                ),
            );
        }
        for (id, v) in row["arrays"].as_object().unwrap() {
            let array = prefixed_object(BTreeMap::new(), &trace, &token, "types");
            let length = if v["lengthThrow"] == true {
                Entry::Throw
            } else {
                pool.entry(&v["length"])
            };
            let join = if v["joinGetThrow"] == true {
                Entry::Throw
            } else {
                Entry::Value(AccountValue::Function(HostThrownValue::new(Join {
                    receiver: Rc::downgrade(&array),
                    result: pool.value(&v["join"]),
                    should_throw: v["joinCallThrow"] == true,
                    trace: trace.clone(),
                    token: token.clone(),
                })))
            };
            *array.fields.borrow_mut() =
                BTreeMap::from([("length".into(), vec![length]), ("join".into(), vec![join])]);
            pool.extras.borrow_mut().insert(id.clone(), array);
        }
        for (id, v) in row["filters"].as_object().unwrap() {
            let fields = v
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
            pool.extras.borrow_mut().insert(
                id.clone(),
                prefixed_object(fields, &trace, &token, "filter"),
            );
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
        let api = TransactionApi::new(
            client.clone(),
            runtime.clone(),
            Rc::new(Semantics),
            Rc::new(Query),
        );
        assert_eq!(
            Rc::strong_count(&client),
            2,
            "native transaction constructor retains original client"
        );
        let method = row["method"].as_str().unwrap();
        *methods.entry(method).or_insert(0usize) += 1;
        let index = row["index"]["decimal"].as_str().unwrap().parse().unwrap();
        let identifier = if row["identifier"]["kind"] == "number" {
            TransactionIdentifier::Number(n(&row["identifier"]))
        } else {
            TransactionIdentifier::String(t(&row["identifier"]))
        };
        let promise = match method {
            "getAccountTransactions" => api.get_account_transactions(&index, options.as_ref()),
            "getBlockTransactions" => api.get_block_transactions(n(&row["block"])),
            "getDepositHistory" => api.get_deposit_history(&index, options.as_ref()),
            "getTransferHistory" => api.get_transfer_history(&index, options.as_ref()),
            "getTransaction" => api.get_transaction(&identifier),
            "getTransactionFromL1Hash" => api.get_transaction_from_l1_hash(&units(&row["hash"])),
            _ => panic!("unknown source method"),
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
            "native transaction observer asynchronous"
        );
        runtime.drain();
        let result = observed
            .borrow_mut()
            .take()
            .expect("native detached transaction work completes");
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
            "source independently expected call count"
        );
        for (actual, want) in native_calls.iter().zip(row["calls"].as_array().unwrap()) {
            assert_eq!(actual.url, units(&want["url"]), "{label} native URL");
            match (&actual.config, &want["config"]) {
                (None, v) => assert!(v.is_null(), "native absent config"),
                (Some(v), w) => {
                    let w = w.as_array().unwrap();
                    assert!(v.params.len() >= 3, "native required initial query keys");
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
    assert_eq!(rows.len(), 192, "independent transaction case floor");
    assert_eq!(
        methods,
        BTreeMap::from([
            ("getAccountTransactions", 38),
            ("getBlockTransactions", 31),
            ("getDepositHistory", 37),
            ("getTransferHistory", 26),
            ("getTransaction", 34),
            ("getTransactionFromL1Hash", 26)
        ]),
        "independent transaction method counts"
    );
    assert_eq!(
        outcomes,
        BTreeMap::from([("ok", 66), ("error", 47), ("thrown", 79)]),
        "independent transaction outcomes"
    );
    assert_corpus();
}
fn assert_corpus() {
    assert_eq!(
        capture()["contentHash"],
        "f9cabcd7c7f6229ce63d33c649ed64112f62a4b353ede9f0f6dc4032ffea277c",
        "independent transaction source corpus pin"
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
fn status(v: &Value) -> TransactionStatus {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "pending" => TransactionStatus::Pending,
        "confirmed" => TransactionStatus::Confirmed,
        "failed" => TransactionStatus::Failed,
        _ => panic!("unknown transaction status"),
    }
}
fn tx_type(v: &Value) -> TransactionType {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "create_order" => TransactionType::CreateOrder,
        "cancel_order" => TransactionType::CancelOrder,
        "transfer" => TransactionType::Transfer,
        "withdrawal" => TransactionType::Withdrawal,
        "deposit" => TransactionType::Deposit,
        _ => panic!("unknown transaction type"),
    }
}
fn deposit_status(v: &Value) -> DepositStatus {
    match String::from_utf16(&t(v)).unwrap().as_str() {
        "pending" => DepositStatus::Pending,
        "confirming" => DepositStatus::Confirming,
        "completed" => DepositStatus::Completed,
        "failed" => DepositStatus::Failed,
        _ => panic!("unknown deposit status"),
    }
}
const STATUSES: [TransactionStatus; 3] = [
    TransactionStatus::Pending,
    TransactionStatus::Confirmed,
    TransactionStatus::Failed,
];
const TYPES: [TransactionType; 5] = [
    TransactionType::CreateOrder,
    TransactionType::CancelOrder,
    TransactionType::Transfer,
    TransactionType::Withdrawal,
    TransactionType::Deposit,
];
const DEPOSIT_STATUSES: [DepositStatus; 4] = [
    DepositStatus::Pending,
    DepositStatus::Confirming,
    DepositStatus::Completed,
    DepositStatus::Failed,
];
fn transaction(v: &Value) -> Transaction {
    Transaction {
        tx_hash: t(&v["txHash"]),
        r#type: tx_type(&v["type"]),
        account_index: t(&v["accountIndex"]),
        api_key_index: n(&v["apiKeyIndex"]),
        nonce: t(&v["nonce"]),
        status: status(&v["status"]),
        block_number: opt_n(v, "blockNumber"),
        block_timestamp: opt_n(v, "blockTimestamp"),
        sequence: opt_n(v, "sequence"),
        data: t(&v["data"]),
        signature: t(&v["signature"]),
        gas_used: opt_t(v, "gasUsed"),
        error: opt_t(v, "error"),
        created_at: n(&v["createdAt"]),
    }
}
fn assert_transaction(x: &Transaction, i: usize) {
    text_field!(x.tx_hash, "Transaction.txHash", i);
    assert_eq!(x.r#type, TYPES[i % 5], "native transaction type variant");
    text_field!(x.account_index, "Transaction.accountIndex", i);
    number_field!(x.api_key_index, "Transaction.apiKeyIndex", 3, i);
    text_field!(x.nonce, "Transaction.nonce", i);
    assert_eq!(
        x.status,
        STATUSES[i % 3],
        "native transaction status variant"
    );
    optional_number!(x.block_number, "Transaction.blockNumber", 6, i);
    optional_number!(x.block_timestamp, "Transaction.blockTimestamp", 7, i);
    optional_number!(x.sequence, "Transaction.sequence", 8, i);
    text_field!(x.data, "Transaction.data", i);
    text_field!(x.signature, "Transaction.signature", i);
    optional_text!(x.gas_used, "Transaction.gasUsed", i);
    optional_text!(x.error, "Transaction.error", i);
    number_field!(x.created_at, "Transaction.createdAt", 13, i);
}
fn deposit(v: &Value) -> Deposit {
    Deposit {
        deposit_id: t(&v["depositId"]),
        account_index: t(&v["accountIndex"]),
        l1_address: t(&v["l1Address"]),
        token_id: n(&v["tokenId"]),
        amount: t(&v["amount"]),
        l1_tx_hash: t(&v["l1TxHash"]),
        status: deposit_status(&v["status"]),
        confirmations: n(&v["confirmations"]),
        required_confirmations: n(&v["requiredConfirmations"]),
        created_at: n(&v["createdAt"]),
        completed_at: opt_n(v, "completedAt"),
    }
}
fn assert_deposit(x: &Deposit, i: usize) {
    text_field!(x.deposit_id, "Deposit.depositId", i);
    text_field!(x.account_index, "Deposit.accountIndex", i);
    text_field!(x.l1_address, "Deposit.l1Address", i);
    number_field!(x.token_id, "Deposit.tokenId", 3, i);
    text_field!(x.amount, "Deposit.amount", i);
    text_field!(x.l1_tx_hash, "Deposit.l1TxHash", i);
    assert_eq!(
        x.status,
        DEPOSIT_STATUSES[i % 4],
        "native deposit status variant"
    );
    number_field!(x.confirmations, "Deposit.confirmations", 7, i);
    number_field!(
        x.required_confirmations,
        "Deposit.requiredConfirmations",
        8,
        i
    );
    number_field!(x.created_at, "Deposit.createdAt", 9, i);
    optional_number!(x.completed_at, "Deposit.completedAt", 10, i);
}
fn transfer(v: &Value) -> Transfer {
    Transfer {
        transfer_id: t(&v["transferId"]),
        tx_hash: t(&v["txHash"]),
        from_account_index: t(&v["fromAccountIndex"]),
        to_account_index: t(&v["toAccountIndex"]),
        token_id: n(&v["tokenId"]),
        amount: t(&v["amount"]),
        status: status(&v["status"]),
        created_at: n(&v["createdAt"]),
    }
}
fn assert_transfer(x: &Transfer, i: usize) {
    text_field!(x.transfer_id, "Transfer.transferId", i);
    text_field!(x.tx_hash, "Transfer.txHash", i);
    text_field!(x.from_account_index, "Transfer.fromAccountIndex", i);
    text_field!(x.to_account_index, "Transfer.toAccountIndex", i);
    number_field!(x.token_id, "Transfer.tokenId", 4, i);
    text_field!(x.amount, "Transfer.amount", i);
    assert_eq!(x.status, STATUSES[i % 3], "native transfer status variant");
    number_field!(x.created_at, "Transfer.createdAt", 7, i);
}
#[test]
#[ignore = "actual TypeScript transaction enum/declaration native oracle"]
fn transaction_models_and_enums_native_fields() {
    let rows = capture()["models"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for row in rows {
        let name = row["record"].as_str().unwrap();
        let i = row["variant"].as_u64().unwrap() as usize;
        let v = &row["input"];
        *counts.entry(name).or_insert(0usize) += 1;
        match name {
            "Transaction" => assert_transaction(&transaction(v), i),
            "TransactionsResponse" => {
                let x = TransactionsResponse {
                    transactions: v["transactions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(transaction)
                        .collect(),
                    total: opt_n(v, "total"),
                    cursor: opt_t(v, "cursor"),
                };
                optional_number!(x.total, "TransactionsResponse.total", 1, i);
                optional_text!(x.cursor, "TransactionsResponse.cursor", i);
                assert_eq!(
                    x.transactions.len(),
                    if i == 0 { 0 } else { 5 },
                    "independent transactions count"
                );
                if i != 0 {
                    for (x, j) in x.transactions.iter().zip([2, 1, 2, 0, 4]) {
                        assert_transaction(x, j)
                    }
                }
            }
            "BlockTransactionsResponse" => {
                let x = BlockTransactionsResponse {
                    block_number: n(&v["blockNumber"]),
                    block_timestamp: n(&v["blockTimestamp"]),
                    transactions: v["transactions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(transaction)
                        .collect(),
                    total_transactions: n(&v["totalTransactions"]),
                };
                number_field!(
                    x.block_number,
                    "BlockTransactionsResponse.blockNumber",
                    0,
                    i
                );
                number_field!(
                    x.block_timestamp,
                    "BlockTransactionsResponse.blockTimestamp",
                    1,
                    i
                );
                number_field!(
                    x.total_transactions,
                    "BlockTransactionsResponse.totalTransactions",
                    3,
                    i
                );
                assert_eq!(
                    x.transactions.len(),
                    if i == 0 { 0 } else { 5 },
                    "independent block transaction count"
                );
                if i != 0 {
                    for (x, j) in x.transactions.iter().zip([2, 1, 2, 0, 4]) {
                        assert_transaction(x, j)
                    }
                }
            }
            "Deposit" => assert_deposit(&deposit(v), i),
            "DepositHistoryResponse" => {
                let x = DepositHistoryResponse {
                    deposits: v["deposits"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(deposit)
                        .collect(),
                    total: opt_n(v, "total"),
                    cursor: opt_t(v, "cursor"),
                };
                optional_number!(x.total, "DepositHistoryResponse.total", 1, i);
                optional_text!(x.cursor, "DepositHistoryResponse.cursor", i);
                assert_eq!(
                    x.deposits.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent deposit count"
                );
                if i != 0 {
                    for (x, j) in x.deposits.iter().zip([2, 1, 2, 0]) {
                        assert_deposit(x, j)
                    }
                }
            }
            "Transfer" => assert_transfer(&transfer(v), i),
            "TransferHistoryResponse" => {
                let x = TransferHistoryResponse {
                    transfers: v["transfers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(transfer)
                        .collect(),
                    total: opt_n(v, "total"),
                    cursor: opt_t(v, "cursor"),
                };
                optional_number!(x.total, "TransferHistoryResponse.total", 1, i);
                optional_text!(x.cursor, "TransferHistoryResponse.cursor", i);
                assert_eq!(
                    x.transfers.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent transfer count"
                );
                if i != 0 {
                    for (x, j) in x.transfers.iter().zip([2, 1, 2, 0]) {
                        assert_transfer(x, j)
                    }
                }
            }
            "TransactionDetailResponse" => {
                let x = TransactionDetailResponse {
                    transaction: transaction(&v["transaction"]),
                };
                assert_transaction(&x.transaction, i);
            }
            "GetAccountTransactionsOptions" => {
                let x = GetAccountTransactionsOptions {
                    limit: opt_n(v, "limit"),
                    cursor: opt_t(v, "cursor"),
                    types: v.get("types").map(|v| {
                        v.as_array()
                            .unwrap()
                            .iter()
                            .map(tx_type)
                            .collect::<Vec<_>>()
                    }),
                    auth: opt_t(v, "auth"),
                };
                optional_number!(x.limit, "GetAccountTransactionsOptions.limit", 0, i);
                optional_text!(x.cursor, "GetAccountTransactionsOptions.cursor", i);
                optional_text!(x.auth, "GetAccountTransactionsOptions.auth", i);
                assert_eq!(
                    x.types,
                    if i == 0 {
                        None
                    } else {
                        Some(vec![
                            TransactionType::Deposit,
                            TransactionType::CreateOrder,
                            TransactionType::Deposit,
                            TransactionType::CancelOrder,
                        ])
                    },
                    "native ordered type filter variants"
                );
            }
            "GetDepositHistoryOptions" => {
                let x = GetDepositHistoryOptions {
                    l1_address: opt_t(v, "l1Address"),
                    auth: opt_t(v, "auth"),
                    cursor: opt_t(v, "cursor"),
                    filter: v.get("filter").map(|v| DepositFilter {
                        status: v.get("status").map(deposit_status),
                        token_id: opt_n(v, "tokenId"),
                    }),
                };
                optional_text!(x.l1_address, "GetDepositHistoryOptions.l1Address", i);
                optional_text!(x.auth, "GetDepositHistoryOptions.auth", i);
                optional_text!(x.cursor, "GetDepositHistoryOptions.cursor", i);
                if i == 0 {
                    assert!(x.filter.is_none(), "native absent filter")
                } else {
                    let x = x.filter.unwrap();
                    assert_eq!(
                        x.status,
                        Some(DEPOSIT_STATUSES[i % 4]),
                        "native filter status variant"
                    );
                    optional_number!(x.token_id, "GetDepositHistoryOptions.filter.tokenId", 4, i);
                }
            }
            "GetTransferHistoryOptions" => {
                let x = GetTransferHistoryOptions {
                    auth: opt_t(v, "auth"),
                    cursor: opt_t(v, "cursor"),
                    limit: opt_n(v, "limit"),
                };
                optional_text!(x.auth, "GetTransferHistoryOptions.auth", i);
                optional_text!(x.cursor, "GetTransferHistoryOptions.cursor", i);
                optional_number!(x.limit, "GetTransferHistoryOptions.limit", 2, i);
            }
            _ => panic!("unmapped declaration"),
        }
    }
    assert_eq!(rows.len(), 45, "independent transaction model floor");
    assert_eq!(
        counts,
        BTreeMap::from([
            ("Transaction", 5),
            ("TransactionsResponse", 4),
            ("BlockTransactionsResponse", 4),
            ("Deposit", 4),
            ("DepositHistoryResponse", 4),
            ("Transfer", 4),
            ("TransferHistoryResponse", 4),
            ("TransactionDetailResponse", 4),
            ("GetAccountTransactionsOptions", 4),
            ("GetDepositHistoryOptions", 4),
            ("GetTransferHistoryOptions", 4)
        ]),
        "independent transaction declaration counts"
    );
    for (native, name, literal) in [
        (TransactionStatus::Pending, "Pending", "pending"),
        (TransactionStatus::Confirmed, "Confirmed", "confirmed"),
        (TransactionStatus::Failed, "Failed", "failed"),
    ] {
        assert_eq!(
            native.as_str(),
            literal,
            "native TransactionStatus exact string"
        );
        assert_eq!(
            capture()["enums"]["TransactionStatus"][name],
            literal,
            "source status value"
        );
    }
    for (native, name, literal) in [
        (TransactionType::CreateOrder, "CreateOrder", "create_order"),
        (TransactionType::CancelOrder, "CancelOrder", "cancel_order"),
        (TransactionType::Transfer, "Transfer", "transfer"),
        (TransactionType::Withdrawal, "Withdrawal", "withdrawal"),
        (TransactionType::Deposit, "Deposit", "deposit"),
    ] {
        assert_eq!(
            native.as_str(),
            literal,
            "native TransactionType exact string"
        );
        assert_eq!(
            capture()["enums"]["TransactionType"][name],
            literal,
            "source type value"
        );
    }
    for (native, literal) in [
        (DepositStatus::Pending, "pending"),
        (DepositStatus::Confirming, "confirming"),
        (DepositStatus::Completed, "completed"),
        (DepositStatus::Failed, "failed"),
    ] {
        assert_eq!(
            native.as_str(),
            literal,
            "native deposit status exact string"
        );
    }
    assert_corpus();
}
