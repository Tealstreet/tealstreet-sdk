use safe_cex_venue_lighter::{sdk_account_api::*, transaction_promise::*, transaction_signing::*};
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
                "/scripts/account-api-oracle.cjs"
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
}
impl AccountObject for Object {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        if !self.fields.borrow().contains_key(key) {
            return Ok(AccountValue::Undefined);
        }
        self.trace.borrow_mut().push(if key == "then" {
            "data.then.get".into()
        } else {
            key.into()
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
            _ => panic!("primitive property not in this declared response corpus"),
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
#[test]
#[ignore = "actual TypeScript AccountApi runtime oracle"]
fn account_methods_native_results_and_reads() {
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
        };
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
            response: AccountValue::Object(response.clone()),
            token: token.clone(),
        });
        let client: Rc<dyn AccountHttpClient> = Rc::new(Http { getter });
        let api = AccountApi::new(client.clone(), runtime.clone(), Rc::new(Semantics));
        assert_eq!(
            Rc::strong_count(&client),
            2,
            "native constructor retains original client handle"
        );
        let method = row["method"].as_str().unwrap();
        *methods.entry(method).or_insert(0usize) += 1;
        let index = row["index"]["decimal"].as_str().unwrap().parse().unwrap();
        let promise = match method {
            "getAccount" => api.get_account(&index),
            "getAccountLimits" => api.get_account_limits(&index),
            "getAccountMetadata" => api.get_account_metadata(&index),
            "getAccountsByL1Address" => api.get_accounts_by_l1_address(units(&row["address"])),
            "getApiKeys" => api.get_api_keys(&index),
            "getLiquidations" => api.get_liquidations(&index, options.as_ref()),
            "getPnl" => api.get_pnl(&index, options.as_ref()),
            _ => panic!("unknown method"),
        };
        trace.borrow_mut().push("after-call".into());
        if row["responseMutation"] == true {
            response.fields.borrow_mut().insert(
                "data".into(),
                vec![Entry::Value(AccountValue::Object(pool.b.clone()))],
            );
        }
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
            "native observer must remain asynchronous"
        );
        runtime.drain();
        let result = observed
            .borrow_mut()
            .take()
            .expect("native detached work completes");
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
            "{label} source expected call count"
        );
        for (actual, want) in native_calls.iter().zip(row["calls"].as_array().unwrap()) {
            assert_eq!(
                actual.url,
                units(&want["url"]),
                "{label} native request URL"
            );
            match (&actual.config, &want["config"]) {
                (None, v) => assert!(v.is_null(), "native absent request config"),
                (Some(v), w) => {
                    let want = w.as_array().unwrap();
                    assert_eq!(
                        v.params.len(),
                        want.len(),
                        "native source expected params count"
                    );
                    for ((k, v), w) in v.params.iter().zip(want) {
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
    assert_eq!(rows.len(), 179, "independent account case floor");
    assert_eq!(
        methods,
        BTreeMap::from([
            ("getAccount", 23),
            ("getAccountLimits", 23),
            ("getAccountMetadata", 23),
            ("getAccountsByL1Address", 23),
            ("getApiKeys", 23),
            ("getLiquidations", 32),
            ("getPnl", 32)
        ]),
        "independent method counts"
    );
    assert_eq!(
        outcomes,
        BTreeMap::from([("ok", 49), ("error", 49), ("thrown", 81)]),
        "independent outcome counts"
    );
    assert_corpus();
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
fn sub_account(v: &Value) -> SubAccount {
    SubAccount {
        account_index: t(&v["accountIndex"]),
        alias: opt_t(v, "alias"),
        status: t(&v["status"]),
        created_at: n(&v["createdAt"]),
    }
}
fn assert_sub(v: &SubAccount, i: usize) {
    text_field!(v.account_index, "SubAccount.accountIndex", i);
    optional_text!(v.alias, "SubAccount.alias", i);
    text_field!(v.status, "SubAccount.status", i);
    number_field!(v.created_at, "SubAccount.createdAt", 3, i);
}
fn key_info(v: &Value) -> ApiKeyInfo {
    ApiKeyInfo {
        index: n(&v["index"]),
        public_key: t(&v["publicKey"]),
        permissions: n(&v["permissions"]),
        label: opt_t(v, "label"),
        is_active: v["isActive"]["value"].as_bool().unwrap(),
        created_at: n(&v["createdAt"]),
        last_used_at: opt_n(v, "lastUsedAt"),
    }
}
fn assert_key_info(v: &ApiKeyInfo, i: usize) {
    number_field!(v.index, "ApiKeyInfo.index", 0, i);
    text_field!(v.public_key, "ApiKeyInfo.publicKey", i);
    number_field!(v.permissions, "ApiKeyInfo.permissions", 2, i);
    optional_text!(v.label, "ApiKeyInfo.label", i);
    assert_eq!(v.is_active, i == 1, "native isActive");
    number_field!(v.created_at, "ApiKeyInfo.createdAt", 5, i);
    optional_number!(v.last_used_at, "ApiKeyInfo.lastUsedAt", 6, i);
}
fn liquidation(v: &Value) -> Liquidation {
    Liquidation {
        liquidation_id: t(&v["liquidationId"]),
        account_index: t(&v["accountIndex"]),
        market_id: opt_n(v, "marketId"),
        amount: t(&v["amount"]),
        price: t(&v["price"]),
        timestamp: n(&v["timestamp"]),
        tx_hash: opt_t(v, "txHash"),
    }
}
fn assert_liquidation(v: &Liquidation, i: usize) {
    text_field!(v.liquidation_id, "Liquidation.liquidationId", i);
    text_field!(v.account_index, "Liquidation.accountIndex", i);
    optional_number!(v.market_id, "Liquidation.marketId", 2, i);
    text_field!(v.amount, "Liquidation.amount", i);
    text_field!(v.price, "Liquidation.price", i);
    number_field!(v.timestamp, "Liquidation.timestamp", 5, i);
    optional_text!(v.tx_hash, "Liquidation.txHash", i);
}
fn pnl_point(v: &Value) -> PnLDataPoint {
    PnLDataPoint {
        timestamp: n(&v["timestamp"]),
        realized_pnl: t(&v["realizedPnl"]),
        unrealized_pnl: t(&v["unrealizedPnl"]),
        total_pnl: t(&v["totalPnl"]),
        roi_percent: opt_n(v, "roiPercent"),
    }
}
fn assert_pnl_point(v: &PnLDataPoint, i: usize) {
    number_field!(v.timestamp, "PnLDataPoint.timestamp", 0, i);
    text_field!(v.realized_pnl, "PnLDataPoint.realizedPnl", i);
    text_field!(v.unrealized_pnl, "PnLDataPoint.unrealizedPnl", i);
    text_field!(v.total_pnl, "PnLDataPoint.totalPnl", i);
    optional_number!(v.roi_percent, "PnLDataPoint.roiPercent", 4, i);
}
fn expected_children(i: usize) -> Vec<usize> {
    if i == 0 { vec![] } else { vec![2, 1, 2, 0] }
}
#[test]
#[ignore = "actual TypeScript account declaration schema and native values"]
fn account_models_native_fields() {
    let rows = capture()["models"].as_array().unwrap();
    let mut counts = BTreeMap::new();
    for row in rows {
        let name = row["record"].as_str().unwrap();
        let i = row["variant"].as_u64().unwrap() as usize;
        let v = &row["input"];
        *counts.entry(name).or_insert(0usize) += 1;
        match name {
            "AccountInfo" => {
                let x = AccountInfo {
                    account_index: t(&v["accountIndex"]),
                    l1_address: t(&v["l1Address"]),
                    tier: opt_t(v, "tier"),
                    status: t(&v["status"]),
                    created_at: n(&v["createdAt"]),
                    last_activity_at: opt_n(v, "lastActivityAt"),
                };
                text_field!(x.account_index, "AccountInfo.accountIndex", i);
                text_field!(x.l1_address, "AccountInfo.l1Address", i);
                optional_text!(x.tier, "AccountInfo.tier", i);
                text_field!(x.status, "AccountInfo.status", i);
                number_field!(x.created_at, "AccountInfo.createdAt", 4, i);
                optional_number!(x.last_activity_at, "AccountInfo.lastActivityAt", 5, i);
            }
            "AccountLimits" => {
                let r = &v["rateLimits"];
                let x = AccountLimits {
                    account_index: t(&v["accountIndex"]),
                    max_open_orders: n(&v["maxOpenOrders"]),
                    max_order_size: t(&v["maxOrderSize"]),
                    max_position_size: t(&v["maxPositionSize"]),
                    daily_withdrawal_limit: t(&v["dailyWithdrawalLimit"]),
                    rate_limits: AccountRateLimits {
                        requests_per_second: n(&r["requestsPerSecond"]),
                        orders_per_second: n(&r["ordersPerSecond"]),
                        orders_per_day: n(&r["ordersPerDay"]),
                    },
                };
                text_field!(x.account_index, "AccountLimits.accountIndex", i);
                number_field!(x.max_open_orders, "AccountLimits.maxOpenOrders", 1, i);
                text_field!(x.max_order_size, "AccountLimits.maxOrderSize", i);
                text_field!(x.max_position_size, "AccountLimits.maxPositionSize", i);
                text_field!(
                    x.daily_withdrawal_limit,
                    "AccountLimits.dailyWithdrawalLimit",
                    i
                );
                number_field!(
                    x.rate_limits.requests_per_second,
                    "AccountLimits.rateLimits.requestsPerSecond",
                    3,
                    i
                );
                number_field!(
                    x.rate_limits.orders_per_second,
                    "AccountLimits.rateLimits.ordersPerSecond",
                    4,
                    i
                );
                number_field!(
                    x.rate_limits.orders_per_day,
                    "AccountLimits.rateLimits.ordersPerDay",
                    5,
                    i
                );
            }
            "AccountMetadata" => {
                let trace = Rc::new(RefCell::new(vec![]));
                let token = HostThrownValue::new("unused");
                let pool = Pool {
                    a: object(BTreeMap::new(), &trace, &token),
                    b: object(BTreeMap::new(), &trace, &token),
                    coercion: Rc::new(Coercion {
                        token: token.clone(),
                    }),
                    symbol: HostThrownValue::new("symbol"),
                    function: HostThrownValue::new("function"),
                };
                let x = AccountMetadata {
                    account_index: t(&v["accountIndex"]),
                    alias: opt_t(v, "alias"),
                    labels: v
                        .get("labels")
                        .map(|v| v.as_array().unwrap().iter().map(t).collect::<Vec<_>>()),
                    metadata: v.get("metadata").map(|v| {
                        v.as_array()
                            .unwrap()
                            .iter()
                            .map(|v| (units(&v["key"]), pool.value(&v["value"])))
                            .collect::<safe_cex_venue_lighter::http_models::Utf16Record<_>>()
                    }),
                    updated_at: n(&v["updatedAt"]),
                };
                text_field!(x.account_index, "AccountMetadata.accountIndex", i);
                optional_text!(x.alias, "AccountMetadata.alias", i);
                number_field!(x.updated_at, "AccountMetadata.updatedAt", 4, i);
                if i == 0 {
                    assert!(x.labels.is_none(), "native absent labels");
                    assert!(x.metadata.is_none(), "native absent metadata");
                } else {
                    assert_eq!(
                        x.labels.unwrap(),
                        vec![
                            u("label-B").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                            u("label-A👩"),
                            u("label-B").into_iter().chain([0xd800]).collect(),
                            u("label-C").into_iter().chain([0xdfff]).collect()
                        ],
                        "native asymmetric labels"
                    );
                    let map = x.metadata.unwrap();
                    assert_eq!(map.len(), 4, "independent native metadata count");
                    let key = u("key").into_iter().chain([0xdfff]).collect::<Vec<_>>();
                    assert!(
                        matches!(&map[&key],AccountValue::BigInt(v)if *v==(num_bigint::BigInt::from(1)<<210usize)),
                        "native unrestricted metadata BigInt"
                    );
                    assert!(
                        matches!(&map[&u("undefined")], AccountValue::Undefined),
                        "native metadata undefined present"
                    );
                    assert!(
                        matches!(&map[&u("symbol")],AccountValue::Symbol(v)if v.ptr_eq(&pool.symbol)),
                        "native metadata symbol identity"
                    );
                    assert!(
                        matches!(&map[&u("function")],AccountValue::Function(v)if v.ptr_eq(&pool.function)),
                        "native metadata function identity"
                    );
                }
            }
            "SubAccount" => assert_sub(&sub_account(v), i),
            "AccountsByL1Address" => {
                let x = AccountsByL1Address {
                    l1_address: t(&v["l1Address"]),
                    accounts: v["accounts"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(sub_account)
                        .collect(),
                    total: n(&v["total"]),
                };
                text_field!(x.l1_address, "AccountsByL1Address.l1Address", i);
                number_field!(x.total, "AccountsByL1Address.total", 2, i);
                assert_eq!(
                    x.accounts.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent account child count"
                );
                for (x, j) in x.accounts.iter().zip(expected_children(i)) {
                    assert_sub(x, j)
                }
            }
            "ApiKeyInfo" => assert_key_info(&key_info(v), i),
            "AccountApiKeys" => {
                let x = AccountApiKeys {
                    account_index: t(&v["accountIndex"]),
                    api_keys: v["apiKeys"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(key_info)
                        .collect(),
                };
                text_field!(x.account_index, "AccountApiKeys.accountIndex", i);
                assert_eq!(
                    x.api_keys.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent API-key child count"
                );
                for (x, j) in x.api_keys.iter().zip(expected_children(i)) {
                    assert_key_info(x, j)
                }
            }
            "Liquidation" => assert_liquidation(&liquidation(v), i),
            "Liquidations" => {
                let x = Liquidations {
                    liquidations: v["liquidations"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(liquidation)
                        .collect(),
                    total: n(&v["total"]),
                    has_more: v["hasMore"]["value"].as_bool().unwrap(),
                };
                number_field!(x.total, "Liquidations.total", 1, i);
                assert_eq!(x.has_more, i == 1, "native hasMore");
                assert_eq!(
                    x.liquidations.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent liquidation child count"
                );
                for (x, j) in x.liquidations.iter().zip(expected_children(i)) {
                    assert_liquidation(x, j)
                }
            }
            "PnLDataPoint" => assert_pnl_point(&pnl_point(v), i),
            "PnL" => {
                let x = PnL {
                    account_index: t(&v["accountIndex"]),
                    data: v["data"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(pnl_point)
                        .collect(),
                    summary: v.get("summary").map(|r| PnLSummary {
                        total_realized_pnl: t(&r["totalRealizedPnl"]),
                        total_unrealized_pnl: t(&r["totalUnrealizedPnl"]),
                        total_pnl: t(&r["totalPnl"]),
                        win_rate: opt_n(r, "winRate"),
                    }),
                };
                text_field!(x.account_index, "PnL.accountIndex", i);
                assert_eq!(
                    x.data.len(),
                    if i == 0 { 0 } else { 4 },
                    "independent PnL child count"
                );
                for (x, j) in x.data.iter().zip(expected_children(i)) {
                    assert_pnl_point(x, j)
                }
                if i == 0 {
                    assert!(x.summary.is_none(), "native absent PnL summary")
                } else {
                    let x = x.summary.unwrap();
                    text_field!(x.total_realized_pnl, "PnL.summary.totalRealizedPnl", i);
                    text_field!(x.total_unrealized_pnl, "PnL.summary.totalUnrealizedPnl", i);
                    text_field!(x.total_pnl, "PnL.summary.totalPnl", i);
                    optional_number!(x.win_rate, "PnL.summary.winRate", 6, i);
                }
            }
            "GetLiquidationsOptions" => {
                let x = GetLiquidationsOptions {
                    limit: opt_n(v, "limit"),
                    offset: opt_n(v, "offset"),
                    market_id: opt_n(v, "marketId"),
                    start_time: opt_n(v, "startTime"),
                    end_time: opt_n(v, "endTime"),
                };
                optional_number!(x.limit, "GetLiquidationsOptions.limit", 0, i);
                optional_number!(x.offset, "GetLiquidationsOptions.offset", 1, i);
                optional_number!(x.market_id, "GetLiquidationsOptions.marketId", 2, i);
                optional_number!(x.start_time, "GetLiquidationsOptions.startTime", 3, i);
                optional_number!(x.end_time, "GetLiquidationsOptions.endTime", 4, i);
            }
            "GetPnlOptions" => {
                let x = GetPnlOptions {
                    period: opt_t(v, "period"),
                    market_id: opt_n(v, "marketId"),
                    start_time: opt_n(v, "startTime"),
                    end_time: opt_n(v, "endTime"),
                };
                optional_text!(x.period, "GetPnlOptions.period", i);
                optional_number!(x.market_id, "GetPnlOptions.marketId", 1, i);
                optional_number!(x.start_time, "GetPnlOptions.startTime", 2, i);
                optional_number!(x.end_time, "GetPnlOptions.endTime", 3, i);
            }
            _ => panic!("unmapped source declaration"),
        }
    }
    assert_eq!(rows.len(), 39, "independent account model floor");
    assert_eq!(
        counts,
        BTreeMap::from([
            ("AccountInfo", 3),
            ("AccountLimits", 3),
            ("AccountMetadata", 3),
            ("SubAccount", 3),
            ("AccountsByL1Address", 3),
            ("ApiKeyInfo", 3),
            ("AccountApiKeys", 3),
            ("Liquidation", 3),
            ("Liquidations", 3),
            ("PnLDataPoint", 3),
            ("PnL", 3),
            ("GetLiquidationsOptions", 3),
            ("GetPnlOptions", 3)
        ]),
        "independent account declaration counts"
    );
    assert_corpus();
}

fn assert_corpus() {
    assert_eq!(
        capture()["contentHash"],
        "afd802fe39f395840f7dfbba3e5ca4ab751c3f14d1e5cc80b64f25b57d72bce7",
        "independent account source corpus pin"
    );
}
