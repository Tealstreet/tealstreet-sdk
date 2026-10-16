//! Independent TypeScript traces feed the required host. Each native operation
//! is checked in source order; native arguments, completions, UTF16, Number bits,
//! object identity and actual returned record fields are asserted directly. No native encoder.
use safe_cex_venue_lighter::{
    exchange_account_access::*,
    sdk_account_api::{AccountObject, AccountValue},
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{
        AuthTokenEncoding, HostThrownValue, SigningResult, TransactionSigningError,
    },
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
};
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
struct Marker {
    fields: RefCell<Vec<(String, AccountValue)>>,
}
impl AccountObject for Marker {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, f: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(f)
    }
    fn now_millis(&self) -> f64 {
        panic!("clock uses actual source binding")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused encoding")
    }
}
impl Runtime {
    fn drain(&self) {
        for _ in 0..100 {
            let f = self.queue.borrow_mut().pop_front();
            match f {
                Some(f) => f(),
                None => return,
            }
        }
        panic!("microtask enumeration exhausted iteration99")
    }
}
struct Env {
    label: String,
    row: Value,
    objects: BTreeMap<String, Rc<Marker>>,
    token: HostThrownValue,
    symbol: HostThrownValue,
    function: HostThrownValue,
    array_function: HostThrownValue,
    runtime: Rc<Runtime>,
    events: RefCell<VecDeque<Value>>,
    pending: RefCell<Option<PromiseResolver<AccountValue>>>,
    literal_index: RefCell<usize>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(
            self.0
                .objects
                .get(id)
                .unwrap_or_else(|| panic!("{} missing native object {id}", self.0.label))
                .clone(),
        )
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "string" => AccountValue::String(units(&v["units"])),
            "bigint" => AccountValue::BigInt(v["digits"].as_str().unwrap().parse().unwrap()),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            "function" => AccountValue::Function(if v["id"] == "Array" {
                self.0.array_function.clone()
            } else {
                self.0.function.clone()
            }),
            "object" => self.obj(v["id"].as_str().unwrap()),
            _ => panic!("{} unsupported native value {v}", self.0.label),
        }
    }
    fn assert_value(&self, n: &AccountValue, e: &Value, label: &str) {
        let l = format!("{} {label}", self.0.label);
        match (n, e["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(n), "boolean") => {
                assert_eq!(*n, e["value"].as_bool().unwrap(), "{l} nativeBool")
            }
            (AccountValue::Number(n), "number") => {
                let expected =
                    f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
                if expected.is_nan() {
                    assert!(n.is_nan(), "{l} nativeNaN")
                } else {
                    assert_eq!(n.to_bits(), expected.to_bits(), "{l} nativeNumberBits")
                }
            }
            (AccountValue::String(n), "string") => {
                assert_eq!(*n, units(&e["units"]), "{l} nativeUtf16")
            }
            (AccountValue::BigInt(n), "bigint") => assert_eq!(
                *n,
                e["digits"]
                    .as_str()
                    .unwrap()
                    .parse::<num_bigint::BigInt>()
                    .unwrap(),
                "{l} nativeBigInt"
            ),
            (AccountValue::Object(n), "object") => assert!(
                Rc::ptr_eq(
                    n,
                    &(self.0.objects[e["id"].as_str().unwrap()].clone() as Rc<dyn AccountObject>)
                ),
                "{l} nativeObjectIdentity"
            ),
            (AccountValue::Symbol(n), "symbol") => {
                assert!(n.ptr_eq(&self.0.symbol), "{l} nativeSymbolIdentity")
            }
            (AccountValue::Function(n), "function") => {
                assert!(
                    n.ptr_eq(if e["id"] == "Array" {
                        &self.0.array_function
                    } else {
                        &self.0.function
                    }),
                    "{l} nativeFunctionIdentity"
                )
            }
            _ => panic!("{l} nativeKindMismatch"),
        }
    }
    fn event(&self, op: &str) -> Value {
        let e = self
            .0
            .events
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("{} unexpected native {op}", self.0.label));
        assert_eq!(e["op"], op, "{} nativeOperationOrder", self.0.label);
        e
    }
    fn result(&self, e: &Value) -> SigningResult<AccountValue> {
        if e["error"] == "token" {
            Err(TransactionSigningError::Thrown(self.0.token.clone()))
        } else {
            assert!(
                e["error"].is_null(),
                "fixture generated errors not part of this floor"
            );
            Ok(self.decode(&e["result"]))
        }
    }
    fn completion(&self, n: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (n, e["kind"].as_str().unwrap()) {
            (Ok(n), "ok") => {
                self.assert_value(&n, &e["value"], label);
                if let AccountValue::Object(object) = &n {
                    if let Some(literal) = self.0.row["literals"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|v| v["id"] == e["value"]["id"])
                    {
                        let id = literal["id"].as_str().unwrap();
                        let expected = literal["fields"].as_array().unwrap();
                        assert_eq!(
                            self.0.objects[id].fields.borrow().len(),
                            expected.len(),
                            "{} {label} independentReturnedRecordFieldCount",
                            self.0.label
                        );
                        for field in expected {
                            let key = field["key"].as_str().unwrap();
                            self.assert_value(
                                &object.get(key).unwrap(),
                                &field["value"],
                                &format!("{label} decodedField:{key}"),
                            );
                        }
                    }
                }
            }
            (Err(TransactionSigningError::Thrown(n)), "thrown") => assert!(
                n.ptr_eq(&self.0.token),
                "{} {label} nativeThrownIdentity",
                self.0.label
            ),
            (Err(TransactionSigningError::Generated(error)), "generated") => {
                assert_eq!(
                    error.name,
                    e["name"].as_str().unwrap(),
                    "{} {label} nativeErrorName",
                    self.0.label
                );
                assert_eq!(
                    error.message,
                    units(&e["message"]),
                    "{} {label} nativeErrorUtf16",
                    self.0.label
                );
            }
            _ => panic!("{} {label} nativeCompletionMismatch", self.0.label),
        }
    }
    fn step(&self, s: &Value) {
        assert!(
            self.0.events.borrow().is_empty(),
            "{} nativePreviousPhaseComplete",
            self.0.label
        );
        self.0
            .events
            .replace(s["events"].as_array().unwrap().iter().cloned().collect());
    }
    fn end_step(&self) {
        assert_eq!(
            self.0.events.borrow().len(),
            0,
            "{} independentSourceEventCount",
            self.0.label
        );
    }
}
struct Call {
    h: Fixture,
    name: String,
    receiver: AccountValue,
}
impl Call {
    fn invoke(&self, op: &str, args: &[AccountValue]) -> SigningResult<AccountValue> {
        let e = self.h.event(op);
        assert_eq!(e["name"], self.name, "{} nativeCallName", self.h.0.label);
        if op == "call" {
            self.h
                .assert_value(&self.receiver, &e["receiver"], "callReceiver");
        }
        let expected = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "{} independentSourceArgumentCount",
            self.h.0.label
        );
        for (i, (value, expected)) in args.iter().zip(expected).enumerate() {
            self.h
                .assert_value(value, expected, &format!("{} arg{i}", self.name));
        }
        let value = self.h.result(&e)?;
        if op == "construct" && e["result"]["id"] == "ConstructedError" {
            // Actual native Error fields derive from native constructor arguments,
            // never from the source output expected at completion.
            self.h.0.objects["ConstructedError"].fields.replace(vec![
                (
                    "name".into(),
                    AccountValue::String("Error".encode_utf16().collect()),
                ),
                ("message".into(), args[0].clone()),
            ]);
        }
        Ok(value)
    }
}
impl AccountAccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke("call", args)
    }
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.invoke("construct", args)
    }
}
impl ExchangeAccountAccessHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, obj: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        let e = self.event("get");
        self.assert_value(obj, &e["object"], "getReceiver");
        assert_eq!(e["key"], key, "{} nativeGetKey", self.0.label);
        self.result(&e)
    }
    fn set(&self, obj: &AccountValue, key: &str, v: AccountValue) -> SigningResult<()> {
        let e = self.event("set");
        self.assert_value(obj, &e["object"], "setReceiver");
        assert_eq!(e["key"], key, "{} nativeSetKey", self.0.label);
        self.assert_value(&v, &e["value"], &format!("set:{key}"));
        self.result(&e).map(|_| ())
    }
    fn member(&self, obj: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.event("member");
        self.assert_value(obj, &e["object"], "memberReceiver");
        assert_eq!(e["key"], key, "{} nativeMemberKey", self.0.label);
        if e["error"] == "token" {
            return Err(TransactionSigningError::Thrown(self.0.token.clone()));
        }
        Ok(Rc::new(Call {
            h: self.clone(),
            name: e["name"].as_str().unwrap().into(),
            receiver: obj.clone(),
        }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.event("global");
        assert_eq!(e["name"], name, "{} nativeGlobalName", self.0.label);
        if e["error"] == "token" {
            return Err(TransactionSigningError::Thrown(self.0.token.clone()));
        }
        let receiver = if name == "Array.isArray" {
            AccountValue::Function(self.0.array_function.clone())
        } else if let Some((id, _)) = name.split_once('.') {
            self.obj(id)
        } else {
            AccountValue::Undefined
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            name: name.into(),
            receiver,
        }))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        let mut i = self.0.literal_index.borrow_mut();
        let e = &self.0.row["literals"][*i];
        *i += 1;
        let expected = e["fields"]
            .as_array()
            .unwrap_or_else(|| panic!("{} missing source literal at {}", self.0.label, *i - 1));
        assert_eq!(
            fields.len(),
            expected.len(),
            "{} independentSourceLiteralFieldCount",
            self.0.label
        );
        for ((key, value), e) in fields.iter().zip(expected) {
            assert_eq!(
                *key,
                e["key"].as_str().unwrap(),
                "{} nativeLiteralOwnKeySchema",
                self.0.label
            );
            self.assert_value(value, &e["value"], &format!("literal:{key}"));
        }
        self.0.objects[e["id"].as_str().unwrap()].fields.replace(
            fields
                .iter()
                .map(|(key, value)| ((*key).into(), value.clone()))
                .collect(),
        );
        Ok(self.obj(e["id"].as_str().unwrap()))
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        if let AccountValue::Object(value) = value {
            if Rc::ptr_eq(
                &value,
                &(self.0.objects["Token"].clone() as Rc<dyn AccountObject>),
            ) {
                return TransactionSigningError::Thrown(self.0.token.clone());
            }
            let AccountValue::String(name) = value.get("name").unwrap() else {
                panic!("native Error name")
            };
            assert_eq!(
                name,
                "Error".encode_utf16().collect::<Vec<_>>(),
                "{} nativeConstructedErrorName",
                self.0.label
            );
            let AccountValue::String(message) = value.get("message").unwrap() else {
                panic!("native Error message")
            };
            TransactionSigningError::Generated(
                safe_cex_venue_lighter::transaction_builders::TransactionBuildError {
                    name: "Error",
                    message,
                },
            )
        } else {
            panic!("source Error or original throw object required")
        }
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>) {
        if self.0.row["input"]["returnThen"].is_string() && matches!(value, AccountValue::Object(_))
        {
            let e = self.event("return-then-get");
            self.assert_value(&value, &e["object"], "asyncReturnThenGetReceiver");
            if e["error"] == "token" {
                resolver.settle(Err(TransactionSigningError::Thrown(self.0.token.clone())));
                return;
            }
            assert_eq!(
                e["result"]["kind"], "callee",
                "{} nativeReturnThenCallable",
                self.0.label
            );
            let live = self.clone();
            self.0.runtime.enqueue_microtask(Box::new(move || {
                let e = live.event("return-then-call");
                live.assert_value(&value, &e["object"], "asyncReturnThenCallReceiver");
                resolver.settle(if live.0.row["input"]["returnThen"] == "resolve" {
                    Ok(AccountValue::String(
                        "adopted/".encode_utf16().chain([0xd800]).collect(),
                    ))
                } else {
                    Err(TransactionSigningError::Thrown(live.0.token.clone()))
                });
            }));
        } else {
            resolver.settle(Ok(value));
        }
    }
    fn await_value(&self, v: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        self.assert_value(
            &v,
            &serde_json::json!({"kind":"object","id":"Awaited"}),
            "awaitPromiseIdentity",
        );
        let (p, r) = NativePromise::pending(self.runtime());
        assert!(
            self.0.pending.replace(Some(r)).is_none(),
            "{} one independently planned source await",
            self.0.label
        );
        Ok(p)
    }
}
fn fixture(row: &Value, i: usize) -> Fixture {
    let mut ids = std::collections::BTreeSet::new();
    fn visit(v: &Value, ids: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if v["kind"] == "object" {
                    ids.insert(v["id"].as_str().unwrap().into());
                }
                for v in o.values() {
                    visit(v, ids)
                }
            }
            Value::Array(a) => {
                for v in a {
                    visit(v, ids)
                }
            }
            _ => {}
        }
    }
    visit(row, &mut ids);
    ids.extend(
        [
            "Owner",
            "Token",
            "ConstructedError",
            "Account",
            "Payload",
            "Crypto",
        ]
        .map(str::to_owned),
    );
    Fixture(Rc::new(Env {
        label: format!("iteration{i} {}", row["method"].as_str().unwrap()),
        row: row.clone(),
        objects: ids
            .into_iter()
            .map(|id| {
                (
                    id,
                    Rc::new(Marker {
                        fields: RefCell::default(),
                    }),
                )
            })
            .collect(),
        token: HostThrownValue::new(()),
        symbol: HostThrownValue::new(()),
        function: HostThrownValue::new(()),
        array_function: HostThrownValue::new(()),
        runtime: Rc::new(Runtime::default()),
        events: RefCell::default(),
        pending: RefCell::default(),
        literal_index: RefCell::new(0),
    }))
}
#[test]
#[ignore = "executes actual TypeScript with SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_account_access_native_floor() {
    let output = Command::new("node")
        .arg(format!(
            "{}/scripts/account-access-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source producer: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let h = fixture(row, i);
        let args = row["args"].as_array().unwrap();
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let result = match row["method"].as_str().unwrap() {
            "extractAccountFromResponse" => {
                Some(extract_account_from_response(&h, &h.decode(&args[0])))
            }
            "getValidatedApiKeyIndex" => Some(get_validated_api_key_index(&h)),
            "getPrivateWsAccountIndex" => Some(get_private_ws_account_index(&h)),
            "getPrivateWsApiKeyIndex" => Some(get_private_ws_api_key_index(&h)),
            _ => None,
        };
        if let Some(result) = result {
            h.completion(result, &steps[0]["output"], "initialResult");
            h.end_step();
        } else {
            let promise = match row["method"].as_str().unwrap() {
                "fetchParsedAccountLookup" => {
                    fetch_parsed_account_lookup(Rc::new(h.clone()), h.decode(&args[0]))
                }
                "ensureAccountIndex" => ensure_account_index(Rc::new(h.clone())),
                "getAccount" => get_account(Rc::new(h.clone())),
                "warmupSignerRuntime" => warmup_signer_runtime(Rc::new(h.clone())),
                _ => panic!("unknown body"),
            };
            h.end_step();
            let result = Rc::new(RefCell::new(None));
            let saved = result.clone();
            promise.subscribe(move |v| {
                saved.replace(Some(v.map(|v| (*v).clone())));
            });
            assert!(result.borrow().is_none(), "{} nativeAsyncQueued", h.0.label);
            let mut index = 1;
            if steps[index]["phase"] == "pending" {
                h.step(&steps[index]);
                h.0.runtime.drain();
                h.end_step();
                // A cached/ready fast path can settle without the unused source promise.
                if h.0.pending.borrow().is_some() {
                    assert!(
                        result.borrow().is_none(),
                        "{} nativePendingBeforeResolve",
                        h.0.label
                    );
                }
                index += 1;
            }
            h.step(&steps[index]);
            if let Some(resolver) = h.0.pending.borrow_mut().take() {
                let value = if row["method"] == "warmupSignerRuntime" {
                    h.obj("Crypto")
                } else if row["method"] == "fetchParsedAccountLookup" {
                    h.obj("Payload")
                } else {
                    h.obj("Account")
                };
                let resolution = if row["input"]["reject"] == true {
                    Err(TransactionSigningError::Thrown(h.0.token.clone()))
                } else if row["method"] == "warmupSignerRuntime" {
                    Ok(value)
                } else {
                    // Source fake API resolves a Proxy: its observable Get(then)
                    // occurs at this dependency boundary, before continuation.
                    h.get(&value, "then").map(|_| value)
                };
                resolver.settle(resolution);
            }
            h.0.runtime.drain();
            h.completion(
                result.borrow_mut().take().expect("native promise settled"),
                &steps[index]["output"],
                "asyncResult",
            );
            h.end_step();
        }
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 190, "independent authored total floor");
    for (name, n) in [
        ("extractAccountFromResponse", 57),
        ("fetchParsedAccountLookup", 27),
        ("ensureAccountIndex", 14),
        ("getValidatedApiKeyIndex", 24),
        ("getPrivateWsAccountIndex", 19),
        ("getPrivateWsApiKeyIndex", 16),
        ("getAccount", 22),
        ("warmupSignerRuntime", 11),
    ] {
        assert_eq!(counts[name], n, "independent authored {name} floor");
    }
    assert_eq!(
        corpus["contentHash"], "25272e0d2d77d8e4db95fe424bb5150c92bed4c15c49387f418777aa068fe46c",
        "source-only corpus pin AFTER all native assertions"
    );
    println!("190 native cases; stopped at iteration189; no exhaustive/runtime/engine claim");
}
