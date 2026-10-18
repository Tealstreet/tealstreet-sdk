use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_ohlcv::*,
    exchange_trading_preflight::{TradingCallback, TradingHost},
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
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
struct Object {
    fields: RefCell<Vec<(Vec<u16>, AccountValue)>>,
    symbols: RefCell<Vec<(HostThrownValue, AccountValue)>>,
    regex: Option<(Vec<u16>, Vec<u16>)>,
    local: bool,
    array: bool,
    error: bool,
}
impl AccountObject for Object {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .fields
            .borrow()
            .iter()
            .find(|(k, _)| *k == u(key))
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
        panic!("unused clock")
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
        panic!("native queue floor exhausted iteration99")
    }
}
struct Env {
    label: String,
    row: Value,
    runtime: Rc<Runtime>,
    events: RefCell<VecDeque<Value>>,
    objects: RefCell<BTreeMap<String, Rc<Object>>>,
    functions: RefCell<BTreeMap<String, HostThrownValue>>,
    callbacks: RefCell<Vec<(HostThrownValue, TradingCallback)>>,
    self_callbacks: RefCell<Vec<(HostThrownValue, OhlcvSelfCallback)>>,
    symbol: HostThrownValue,
    promises: RefCell<BTreeMap<String, NativePromise<AccountValue>>>,
    native_promises: RefCell<Vec<(Rc<Object>, NativePromise<AccountValue>)>>,
    state: RefCell<BTreeMap<String, AccountValue>>,
    maps: RefCell<BTreeMap<String, Vec<(AccountValue, AccountValue)>>>,
    emitted: RefCell<Vec<AccountValue>>,
    latest_calls: std::cell::Cell<usize>,
    pending: RefCell<BTreeMap<String, PromiseResolver<AccountValue>>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn object(&self, id: &str) -> Rc<Object> {
        if let Some(o) = self.0.objects.borrow().get(id).cloned() {
            return o;
        }
        let record = self.0.row["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .cloned();
        let o = Rc::new(Object {
            fields: RefCell::default(),
            local: id == "Token",
            array: false,
            error: false,
            symbols: RefCell::default(),
            regex: None,
        });
        self.0.objects.borrow_mut().insert(id.into(), o.clone());
        if let Some(record) = record {
            o.fields.replace(
                record["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (units(&f["keyUnits"]), self.decode(&f["value"])))
                    .collect(),
            );
            o.symbols.replace(
                record["symbols"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| {
                        let AccountValue::Symbol(key) = self.decode(&f["key"]) else {
                            panic!("source symbol key")
                        };
                        (key, self.decode(&f["value"]))
                    })
                    .collect(),
            );
        }
        o
    }
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.object(id))
    }
    fn marker(&self, v: &AccountValue) -> Rc<Object> {
        let AccountValue::Object(v) = v else {
            panic!("{} native object required", self.0.label)
        };
        self.0
            .objects
            .borrow()
            .values()
            .find(|o| Rc::ptr_eq(&((*o).clone() as Rc<dyn AccountObject>), v))
            .cloned()
            .unwrap_or_else(|| panic!("{} unregistered native object", self.0.label))
    }
    fn fresh(&self, fields: Vec<(String, AccountValue)>, array: bool) -> AccountValue {
        let o = Rc::new(Object {
            fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(&k), v)).collect()),
            local: true,
            array,
            error: false,
            symbols: RefCell::default(),
            regex: None,
        });
        let key = format!("$native{}", self.0.objects.borrow().len());
        self.0.objects.borrow_mut().insert(key, o.clone());
        AccountValue::Object(o)
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            )),
            "bigint" => AccountValue::BigInt(v["value"].as_str().unwrap().parse().unwrap()),
            "string" => AccountValue::String(units(&v["units"])),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            "function" => {
                let id = v["id"].as_str().unwrap();
                let existing = self.0.functions.borrow().get(id).cloned();
                let f = existing.unwrap_or_else(|| {
                    let f = HostThrownValue::new(id.to_owned());
                    self.0.functions.borrow_mut().insert(id.into(), f.clone());
                    f
                });
                AccountValue::Function(f)
            }
            "object" => {
                let id = v["id"].as_str().unwrap();
                if let Some(name) = id.strip_prefix("P.") {
                    if !self.0.promises.borrow().contains_key(id) {
                        let (p, r) = NativePromise::pending(self.runtime());
                        self.0.promises.borrow_mut().insert(id.into(), p);
                        self.0.pending.borrow_mut().insert(name.into(), r);
                    }
                }
                if v.get("fields").is_some() && !self.0.objects.borrow().contains_key(id) {
                    let o = Rc::new(Object {
                        fields: RefCell::default(),
                        local: true,
                        array: v["array"] == true,
                        error: v["error"] == true,
                        symbols: RefCell::default(),
                        regex: v
                            .get("regex")
                            .map(|r| (units(&r["pattern"]), units(&r["flags"]))),
                    });
                    self.0.objects.borrow_mut().insert(id.into(), o.clone());
                    o.fields.replace(
                        v["fields"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|f| (units(&f["keyUnits"]), self.decode(&f["value"])))
                            .collect(),
                    );
                    o.symbols.replace(
                        v["symbols"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|f| {
                                let AccountValue::Symbol(key) = self.decode(&f["key"]) else {
                                    panic!("source symbol key")
                                };
                                (key, self.decode(&f["value"]))
                            })
                            .collect(),
                    );
                }
                self.obj(id)
            }
            _ => panic!("source value domain"),
        }
    }
    fn find_promise(&self, value: &AccountValue) -> Option<NativePromise<AccountValue>> {
        let AccountValue::Object(value) = value else {
            return None;
        };
        if let Some((_, p)) = self
            .0
            .native_promises
            .borrow()
            .iter()
            .find(|(o, _)| Rc::ptr_eq(value, &(o.clone() as Rc<dyn AccountObject>)))
        {
            return Some(p.clone());
        }
        let id = self
            .0
            .objects
            .borrow()
            .iter()
            .find(|(id, o)| {
                id.starts_with("P.") && Rc::ptr_eq(value, &((*o).clone() as Rc<dyn AccountObject>))
            })
            .map(|(id, _)| id.clone());
        id.and_then(|id| self.0.promises.borrow().get(&id).cloned())
    }
    fn assert_value(&self, n: &AccountValue, e: &Value, label: &str) {
        let l = format!("{} {label}", self.0.label);
        match (n, e["kind"].as_str().unwrap()) {
            (AccountValue::Undefined, "undefined") | (AccountValue::Null, "null") => {}
            (AccountValue::Boolean(n), "boolean") => {
                assert_eq!(*n, e["value"].as_bool().unwrap(), "{l} nativeBool")
            }
            (AccountValue::Number(n), "number") => {
                let x =
                    f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
                if x.is_nan() {
                    assert!(n.is_nan(), "{l} nativeNaN")
                } else {
                    assert_eq!(n.to_bits(), x.to_bits(), "{l} nativeNumberBits")
                }
            }
            (AccountValue::String(n), "string") => {
                assert_eq!(*n, units(&e["units"]), "{l} nativeUtf16")
            }
            (AccountValue::BigInt(n), "bigint") => assert_eq!(
                *n,
                e["value"].as_str().unwrap().parse().unwrap(),
                "{l} nativeBigInt"
            ),
            (AccountValue::Symbol(n), "symbol") => {
                assert!(n.ptr_eq(&self.0.symbol), "{l} nativeSymbolIdentity")
            }
            (AccountValue::Function(n), "function") => {
                let id = e["id"].as_str().unwrap();
                let old = self.0.functions.borrow().get(id).cloned();
                if let Some(old) = old {
                    assert!(n.ptr_eq(&old), "{l} nativeFunctionIdentity")
                } else {
                    assert!(
                        self.0.callbacks.borrow().iter().any(|(f, _)| f.ptr_eq(n))
                            || self
                                .0
                                .self_callbacks
                                .borrow()
                                .iter()
                                .any(|(f, _)| f.ptr_eq(n)),
                        "{l} nativeClosureIdentity"
                    );
                    assert!(
                        self.0.functions.borrow().values().all(|old| !old.ptr_eq(n)),
                        "{l} nativeFunctionBijection"
                    );
                    self.0.functions.borrow_mut().insert(id.into(), n.clone());
                }
            }
            (AccountValue::Object(n), "object") => {
                let id = e["id"].as_str().unwrap();
                let old = self.0.objects.borrow().get(id).cloned();
                if let Some(old) = old {
                    assert!(
                        Rc::ptr_eq(n, &(old as Rc<dyn AccountObject>)),
                        "{l} nativeObjectIdentity"
                    )
                } else {
                    let own = self.marker(&AccountValue::Object(n.clone()));
                    assert!(
                        self.0
                            .objects
                            .borrow()
                            .iter()
                            .filter(|(key, _)| !key.starts_with("$native"))
                            .all(|(_, old)| !Rc::ptr_eq(
                                n,
                                &(old.clone() as Rc<dyn AccountObject>)
                            )),
                        "{l} nativeObjectBijection"
                    );
                    self.0.objects.borrow_mut().insert(id.into(), own);
                }
                if let Some(fields) = e.get("fields") {
                    let own = self.marker(&AccountValue::Object(n.clone()));
                    assert_eq!(
                        own.array,
                        e["array"].as_bool().unwrap(),
                        "{l} nativeArrayKind"
                    );
                    assert_eq!(
                        own.error,
                        e["error"].as_bool().unwrap(),
                        "{l} nativeErrorKind"
                    );
                    if let Some(regex) = e.get("regex") {
                        assert_eq!(
                            own.regex,
                            Some((units(&regex["pattern"]), units(&regex["flags"]))),
                            "{l} nativeRegExpLiteral"
                        );
                    }
                    let symbols = own.symbols.borrow().clone();
                    let expected_symbols = e["symbols"].as_array().unwrap();
                    assert_eq!(
                        symbols.len(),
                        expected_symbols.len(),
                        "{l} nativeSymbolFieldCount"
                    );
                    for ((key, value), expected) in symbols.iter().zip(expected_symbols) {
                        self.assert_value(
                            &AccountValue::Symbol(key.clone()),
                            &expected["key"],
                            "symbol own key",
                        );
                        self.assert_value(value, &expected["value"], "symbol own value");
                    }
                    let actual = own.fields.borrow().clone();
                    let fields = fields.as_array().unwrap();
                    assert_eq!(
                        actual.len(),
                        fields.len(),
                        "{l} {}",
                        if e["error"] == true {
                            "source error diagnostic field floor"
                        } else {
                            "independentSourceOwnFieldCount"
                        }
                    );
                    for ((key, value), field) in actual.iter().zip(fields) {
                        assert_eq!(
                            key,
                            &units(&field["keyUnits"]),
                            "{l} {}",
                            if e["error"] == true {
                                "source error diagnostic key"
                            } else {
                                "nativeOwnKeySchema"
                            }
                        );
                        self.assert_value(value, &field["value"], &format!("{label}:{key:?}"));
                    }
                }
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
    fn error(&self, v: &Value) -> TransactionSigningError {
        TransactionSigningError::Thrown(HostThrownValue::new(self.decode(&v["value"])))
    }

    fn result(&self, e: &Value) -> SigningResult<AccountValue> {
        if e.get("error").is_some() {
            Err(self.error(&e["error"]))
        } else {
            Ok(self.decode(&e["result"]))
        }
    }
    fn completion(&self, r: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (r, e["kind"].as_str().unwrap()) {
            (Ok(v), "ok") => self.assert_value(&v, &e["value"], label),
            (Err(TransactionSigningError::Thrown(n)), "thrown") => self.assert_value(
                n.value().downcast_ref::<AccountValue>().unwrap(),
                &e["value"],
                label,
            ),
            (Err(TransactionSigningError::Thrown(n)), "generated") => {
                let value = n.value().downcast_ref::<AccountValue>().unwrap();
                self.assert_value(value, &e["value"], label);
                let own = self.marker(value);
                assert!(own.error, "{} {label} nativeErrorMarker", self.0.label);
                self.assert_value(
                    &own.get("name").unwrap(),
                    &serde_json::json!({"kind":"string","units":u(e["name"].as_str().unwrap())}),
                    &format!("{label} nativeErrorName"),
                );
                self.assert_value(
                    &own.get("message").unwrap(),
                    &serde_json::json!({"kind":"string","units":e["message"]}),
                    &format!("{label} nativeErrorMessage"),
                );
            }
            (Err(TransactionSigningError::Generated(_)), "generated") => panic!(
                "{} {label} nativeOriginalErrorIdentityRequired",
                self.0.label
            ),
            _ => panic!("{} {label} nativeCompletionMismatch", self.0.label),
        }
    }
    fn callback(&self, e: &Value, candle_schema: bool) {
        let f = self.0.functions.borrow()[e["id"].as_str().unwrap()].clone();
        let args: Vec<_> = e["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| self.decode(v))
            .collect();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.call_value(AccountValue::Function(f), &args)
        }));
        assert!(
            result.is_ok(),
            "{} nativeCallbackMustReturnJsCompletion",
            self.0.label
        );
        let expected = self.event("invoke-result");
        let result = result.unwrap();
        if candle_schema && result.is_ok() {
            let Ok(AccountValue::Object(value)) = &result else {
                panic!("native mapped candle DTO")
            };
            let value = self.marker(&AccountValue::Object(value.clone()));
            let fields = value.fields.borrow();
            assert_eq!(fields.len(), 6, "authored six candle DTO fields");
            assert_eq!(
                fields
                    .iter()
                    .map(|(key, _)| key.clone())
                    .collect::<Vec<_>>(),
                ["timestamp", "open", "high", "low", "close", "volume"]
                    .map(u)
                    .to_vec(),
                "authored candle DTO key order"
            );
            assert_eq!(
                value.symbols.borrow().len(),
                0,
                "authored candle DTO symbol floor"
            );
        }
        self.completion(result, &expected["result"], "callbackResult")
    }
    fn native_call(
        &self,
        c: &Call,
        args: &[AccountValue],
        op: &str,
    ) -> SigningResult<AccountValue> {
        let e = self.event(op);
        assert_eq!(e["name"], c.name, "{} nativeCalleeName", self.0.label);
        self.assert_value(&c.callee, &e["callee"], "capturedCallee");
        self.assert_value(&c.receiver, &e["receiver"], "callReceiver");
        let expected = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "{} independentSourceArgumentCount",
            self.0.label
        );
        for (i, (value, expected)) in args.iter().zip(expected).enumerate() {
            self.assert_value(value, expected, &format!("{} arg{i}", c.name));
        }
        loop {
            let next = self.0.events.borrow().front().cloned().unwrap();
            if next["op"] == "end-call" && next["depth"] == e["depth"] {
                self.event("end-call");
                break;
            }
            if next["op"] == "invoke" {
                let event = self.event("invoke");
                self.callback(&event, c.name == "Array.map");
                continue;
            }
            assert!(
                next["depth"].as_u64().unwrap() > e["depth"].as_u64().unwrap(),
                "{} delegatedHostDepth",
                self.0.label
            );
            assert_ne!(
                next["op"], "invoke-result",
                "native callbacks cannot be skipped"
            );
            self.0.events.borrow_mut().pop_front();
        }
        if e.get("error").is_none()
            && (c.name.starts_with("Cooldown.") || c.name.starts_with("Fetches."))
        {
            let (name, method) = c.name.split_once('.').unwrap();
            let mut maps = self.0.maps.borrow_mut();
            let entries = maps.get_mut(name).unwrap();
            let at = entries.iter().position(|(key, _)| same(key, &args[0]));
            let result = match method {
                "get" => at
                    .map(|i| entries[i].1.clone())
                    .unwrap_or(AccountValue::Undefined),
                "set" => {
                    if let Some(i) = at {
                        entries[i].1 = args[1].clone()
                    } else {
                        entries.push((args[0].clone(), args[1].clone()))
                    }
                    c.receiver.clone()
                }
                "delete" => {
                    if let Some(i) = at {
                        entries.remove(i);
                    }
                    AccountValue::Boolean(at.is_some())
                }
                _ => panic!("actual Map method"),
            };
            drop(maps);
            self.assert_value(&result, &e["result"], "nativeMapResult");
            return Ok(result);
        }
        if c.name == "Owner.fetchLatestOhlcvWithSharedFallback" && e.get("error").is_none() {
            self.0.latest_calls.set(self.0.latest_calls.get() + 1);
        }
        if c.name == "User.callback" {
            self.0.emitted.borrow_mut().push(args[0].clone());
            let id = e["args"][0]["id"].as_str().unwrap();
            let record = self.0.row["records"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["id"] == id)
                .unwrap();
            self.assert_value(&args[0],&serde_json::json!({"kind":"object","id":id,"array":false,"error":false,"fields":record["fields"],"symbols":record["symbols"]}),"nativeEmittedFields");
        }
        let value = self.result(&e)?;
        Ok(value)
    }
    fn end_step(&self) {
        assert_eq!(
            self.0.events.borrow().len(),
            0,
            "{} independentSourceEventCount",
            self.0.label
        )
    }
    fn step(&self, s: &Value) {
        self.end_step();
        self.0
            .events
            .replace(s["events"].as_array().unwrap().iter().cloned().collect());
    }
}
struct Call {
    h: Fixture,
    name: String,
    receiver: AccountValue,
    callee: AccountValue,
}
impl AccountAccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h.native_call(self, args, "call")
    }
    fn construct(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h.native_call(self, args, "construct")
    }
}
impl ExchangeAccountAccessHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        self.get_key(object, AccountValue::String(u(key)))
    }
    fn set(&self, _: &AccountValue, _: &str, _: AccountValue) -> SigningResult<()> {
        panic!("no owner assignment in OHLCV bodies")
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        if object.is_nullish() {
            self.get(object, key)?;
            unreachable!("nullish member get throws")
        }
        let e = self.event("member");
        self.assert_value(object, &e["object"], "memberReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativePropertyKey",
            self.0.label
        );
        let callee = self.result(&e)?;
        Ok(Rc::new(Call {
            h: self.clone(),
            name: e["name"].as_str().unwrap().into(),
            receiver: object.clone(),
            callee,
        }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.event("global");
        assert_eq!(e["name"], name, "{} nativeImportBinding", self.0.label);
        let callee = self.result(&e)?;
        let receiver = if e.get("receiver").is_some() {
            self.decode(&e["receiver"])
        } else {
            AccountValue::Undefined
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            name: name.into(),
            receiver,
            callee,
        }))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(self.fresh(
            fields
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect(),
            false,
        ))
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        TransactionSigningError::Thrown(HostThrownValue::new(value))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        Ok(self
            .find_promise(&value)
            .expect("authored await native Promise"))
    }
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>) {
        if let Some(p) = self.find_promise(&value) {
            p.subscribe(move |v| resolver.settle(v.map(|v| (*v).clone())))
        } else {
            resolver.settle(Ok(value))
        }
    }
}
impl TradingHost for Fixture {
    fn function(&self, cb: TradingCallback) -> SigningResult<AccountValue> {
        let f = HostThrownValue::new("native callback");
        self.0.callbacks.borrow_mut().push((f.clone(), cb));
        Ok(AccountValue::Function(f))
    }
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(self.fresh(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            true,
        ))
    }
    fn index(&self, object: &AccountValue, key: AccountValue) -> SigningResult<AccountValue> {
        let key = match key {
            AccountValue::Number(n) => AccountValue::String(u(&format!("{n}"))),
            other => other,
        };
        self.get_key(object, key)
    }
    fn constant(&self, name: &str) -> SigningResult<AccountValue> {
        let e = self.event("constant");
        assert_eq!(e["name"], name);
        self.result(&e)
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        match error {
            TransactionSigningError::Thrown(v) => {
                v.value().downcast_ref::<AccountValue>().unwrap().clone()
            }
            _ => panic!("authored caught error domain; full projection required"),
        }
    }
    fn template_string(&self, value: AccountValue) -> SigningResult<Vec<u16>> {
        match value {
            AccountValue::String(v) => Ok(v),
            AccountValue::Undefined => Ok(u("undefined")),
            AccountValue::Null => Ok(u("null")),
            AccountValue::Boolean(v) => Ok(u(if v { "true" } else { "false" })),
            AccountValue::Number(v) => Ok(u(&if v == 0.0 { "0".into() } else { v.to_string() })),
            AccountValue::BigInt(v) => Ok(u(&v.to_string())),
            _ => panic!("bounded primitive template domain; full host required"),
        }
    }
}

impl OhlcvHost for Fixture {
    fn self_function(&self, cb: OhlcvSelfCallback) -> SigningResult<AccountValue> {
        let f = HostThrownValue::new(());
        self.0.self_callbacks.borrow_mut().push((f.clone(), cb));
        Ok(AccountValue::Function(f))
    }
    fn promise_value(&self, p: NativePromise<AccountValue>) -> AccountValue {
        let value = self.fresh(vec![], false);
        self.0
            .native_promises
            .borrow_mut()
            .push((self.marker(&value), p));
        value
    }
    fn call_value(
        &self,
        callee: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let AccountValue::Function(f) = &callee else {
            panic!("authored callable domain")
        };
        let cb = self
            .0
            .callbacks
            .borrow()
            .iter()
            .find(|(v, _)| v.ptr_eq(f))
            .map(|(_, cb)| cb.clone());
        if let Some(cb) = cb {
            return cb(args);
        }
        let self_cb = self
            .0
            .self_callbacks
            .borrow()
            .iter()
            .find(|(v, _)| v.ptr_eq(f))
            .map(|(_, cb)| cb.clone());
        if let Some(cb) = self_cb {
            return cb(callee, args);
        }
        let e = self.0.events.borrow().front().cloned().unwrap();
        let call = Call {
            h: self.clone(),
            name: e["name"].as_str().unwrap().into(),
            receiver: AccountValue::Undefined,
            callee,
        };
        self.native_call(&call, args, "call")
    }
    fn regexp(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue> {
        let value = self.fresh_units(vec![], vec![], Some((u(pattern), u(flags))));
        Ok(value)
    }
    fn interval_binding(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)> {
        let iter = self.member(&value, "@@iterator")?.call(&[])?;
        let next = self.member(&iter, "next")?;
        let mut done = false;
        let mut values = [AccountValue::Undefined, AccountValue::Undefined];
        for i in 0..3 {
            if !done {
                let step = next.call(&[])?;
                done = self.get(&step, "done")?.truthy();
                if !done && i > 0 {
                    values[i - 1] = self.get(&step, "value")?;
                }
            }
        }
        if !done {
            let close = self.get(&iter, "return")?;
            assert!(
                close.is_nullish(),
                "bounded absent iterator return; full close required"
            );
        }
        Ok((values[0].clone(), values[1].clone()))
    }
    fn spread_override(
        &self,
        value: AccountValue,
        overrides: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        let marker = self.marker(&value);
        let keys = marker
            .fields
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let symkeys = marker
            .symbols
            .borrow()
            .iter()
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        let mut fields = vec![];
        let mut symbols = vec![];
        for key in keys {
            let value = self.index(&value, AccountValue::String(key.clone()))?;
            fields.push((key, value));
        }
        for key in symkeys {
            let value = self.index(&value, AccountValue::Symbol(key.clone()))?;
            symbols.push((key, value));
        }
        for (key, value) in overrides {
            let key = u(key);
            if let Some((_, old)) = fields.iter_mut().find(|(k, _)| *k == key) {
                *old = value.clone()
            } else {
                fields.push((key, value.clone()));
            }
        }
        Ok(self.fresh_units(fields, symbols, None))
    }
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) - number(b)))
    }
    fn add(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) + number(b)))
    }
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) * number(b)))
    }
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) / number(b)))
    }
    fn less(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) < number(b))
    }
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) <= number(b))
    }
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) > number(b))
    }
}
fn number(v: AccountValue) -> f64 {
    let AccountValue::Number(v) = v else {
        panic!("authored numeric operator domain; full host required")
    };
    v
}
fn same(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
        _ => panic!("authored Map string/number key domain"),
    }
}

impl Fixture {
    fn fresh_units(
        &self,
        fields: Vec<(Vec<u16>, AccountValue)>,
        symbols: Vec<(HostThrownValue, AccountValue)>,
        regex: Option<(Vec<u16>, Vec<u16>)>,
    ) -> AccountValue {
        let o = Rc::new(Object {
            fields: RefCell::new(fields),
            symbols: RefCell::new(symbols),
            regex,
            local: true,
            array: false,
            error: false,
        });
        let id = format!("$native{}", self.0.objects.borrow().len());
        self.0.objects.borrow_mut().insert(id, o.clone());
        AccountValue::Object(o)
    }
    fn get_key(&self, object: &AccountValue, key: AccountValue) -> SigningResult<AccountValue> {
        if object.is_nullish() {
            let AccountValue::String(key) = key else {
                panic!("authored null property key")
            };
            let key = String::from_utf16(&key).unwrap();
            let message = format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(object, AccountValue::Null) {
                    "null"
                } else {
                    "undefined"
                }
            );
            let o = Rc::new(Object {
                fields: RefCell::new(vec![
                    (u("name"), AccountValue::String(u("TypeError"))),
                    (u("message"), AccountValue::String(u(&message))),
                ]),
                symbols: RefCell::default(),
                regex: None,
                local: true,
                array: false,
                error: true,
            });
            let id = format!("$native{}", self.0.objects.borrow().len());
            self.0.objects.borrow_mut().insert(id, o.clone());
            return Err(self.throw_value(AccountValue::Object(o)));
        }
        if let AccountValue::Object(_) = object {
            let own = self.marker(object);
            if own.local {
                return Ok(match &key {
                    AccountValue::String(key) => {
                        if own.array && *key == u("length") {
                            AccountValue::Number(own.fields.borrow().len() as f64)
                        } else {
                            own.fields
                                .borrow()
                                .iter()
                                .find(|(k, _)| k == key)
                                .map(|(_, v)| v.clone())
                                .unwrap_or(AccountValue::Undefined)
                        }
                    }
                    AccountValue::Symbol(key) => own
                        .symbols
                        .borrow()
                        .iter()
                        .find(|(k, _)| k.ptr_eq(key))
                        .map(|(_, v)| v.clone())
                        .unwrap_or(AccountValue::Undefined),
                    _ => panic!("authored property key"),
                });
            }
        }
        let e = self.event("get");
        self.assert_value(object, &e["object"], "getReceiver");
        match &key {
            AccountValue::String(key) => assert_eq!(
                *key,
                units(&e["keyUnits"]),
                "{} nativePropertyKey",
                self.0.label
            ),
            AccountValue::Symbol(_) => {
                self.assert_value(&key, &e["keyValue"], "nativeSymbolPropertyKey")
            }
            _ => panic!("authored property key"),
        };
        if e.get("error").is_some() {
            return Err(self.error(&e["error"]));
        }
        if e["object"]["id"] == "Owner" {
            let AccountValue::String(key) = key else {
                panic!("owner named key")
            };
            let key = String::from_utf16(&key).unwrap();
            let value = self.0.state.borrow()[&key].clone();
            self.assert_value(&value, &e["result"], "nativeOwnerRead");
            Ok(value)
        } else {
            self.result(&e)
        }
    }
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_ohlcv_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ohlcv-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source producer: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    let mut counts = BTreeMap::<String, usize>::new();
    for (i, row) in rows.iter().enumerate() {
        let h = Fixture(Rc::new(Env {
            label: format!("iteration{i} {}", row["method"].as_str().unwrap()),
            row: row.clone(),
            runtime: Rc::default(),
            events: RefCell::default(),
            objects: RefCell::default(),
            functions: RefCell::default(),
            callbacks: RefCell::default(),
            self_callbacks: RefCell::default(),
            symbol: HostThrownValue::new("source Symbol"),
            promises: RefCell::default(),
            native_promises: RefCell::default(),
            state: RefCell::default(),
            maps: RefCell::default(),
            emitted: RefCell::default(),
            latest_calls: std::cell::Cell::new(0),
            pending: RefCell::default(),
        }));
        for field in row["initialState"].as_array().unwrap() {
            let value = h.decode(&field["value"]);
            h.0.state
                .borrow_mut()
                .insert(field["key"].as_str().unwrap().into(), value);
        }
        assert_eq!(
            h.0.state.borrow().len(),
            6,
            "independent six owner input fields"
        );
        for map in row["initialMaps"].as_array().unwrap() {
            let entries = map["entries"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (h.decode(&e["key"]), h.decode(&e["value"])))
                .collect();
            h.0.maps
                .borrow_mut()
                .insert(map["id"].as_str().unwrap().into(), entries);
        }
        assert_eq!(h.0.maps.borrow().len(), 2, "independent two native Maps");
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| h.decode(v))
            .collect::<Vec<_>>();
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let live: Rc<dyn OhlcvHost> = Rc::new(h.clone());
        let observed = Rc::new(RefCell::new(None));
        match row["method"].as_str().unwrap() {
            "fetchOHLCV" => {
                let seen = observed.clone();
                fetch_ohlcv(live, args[0].clone())
                    .subscribe(move |v| *seen.borrow_mut() = Some(v.map(|v| (*v).clone())));
            }
            "fetchLatestOhlcvWithSharedFallback" => {
                let seen = observed.clone();
                fetch_latest_ohlcv_with_shared_fallback(live, args[0].clone(), args[1].clone())
                    .subscribe(move |v| *seen.borrow_mut() = Some(v.map(|v| (*v).clone())));
            }
            "listenOHLCV" => {
                *observed.borrow_mut() = Some(listen_ohlcv(live, args[0].clone(), args[1].clone()))
            }
            "listenOrderBook" => {
                *observed.borrow_mut() = Some(listen_order_book(
                    live.as_ref(),
                    args[0].clone(),
                    args[1].clone(),
                ))
            }
            _ => panic!("authored method"),
        };
        h.0.runtime.drain();
        for (stage, step) in steps.iter().enumerate() {
            if stage > 0 {
                h.step(step);
                if step["action"] == "provider" {
                    let name = step["phase"].as_str().unwrap();
                    let r = h.0.pending.borrow_mut().remove(name).unwrap();
                    let provider = row["providers"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|p| p["name"] == name)
                        .unwrap();
                    let value = if provider["reject"].is_null() {
                        Ok(h.decode(&provider["value"]))
                    } else {
                        Err(h.throw_value(h.decode(&provider["reject"])))
                    };
                    r.settle(value);
                } else {
                    if step["action"] == "dispose" {
                        h.0.state
                            .borrow_mut()
                            .insert("isDisposed".into(), AccountValue::Boolean(true));
                    }
                    while h
                        .0
                        .events
                        .borrow()
                        .front()
                        .is_some_and(|e| e["op"] == "invoke")
                    {
                        let e = h.event("invoke");
                        h.callback(&e, false);
                    }
                }
                h.0.runtime.drain();
            }
            if step["output"].is_null() {
                assert!(
                    observed.borrow().is_none(),
                    "{} nativeDeferredCompletion",
                    h.0.label
                )
            } else {
                let result = observed
                    .borrow()
                    .clone()
                    .expect("source completed native result");
                if i == 0 {
                    let Ok(AccountValue::Object(value)) = &result else {
                        panic!("authored native candle array")
                    };
                    let array = h.marker(&AccountValue::Object(value.clone()));
                    assert!(array.array, "authored native Array");
                    assert_eq!(
                        array.fields.borrow().len(),
                        2,
                        "authored two sorted candles"
                    );
                    let AccountValue::Object(first) = array.get("0").unwrap() else {
                        panic!("native first candle")
                    };
                    let AccountValue::Number(timestamp) = first.get("timestamp").unwrap() else {
                        panic!("native timestamp")
                    };
                    assert_eq!(
                        timestamp.to_bits(),
                        1.0_f64.to_bits(),
                        "authored native first timestamp1"
                    );
                }
                h.completion(result, &step["output"], "nativeResult");
            }
            h.end_step();
        }
        for map in row["finalMaps"].as_array().unwrap() {
            let maps = h.0.maps.borrow();
            let actual = &maps[map["id"].as_str().unwrap()];
            let expected = map["entries"].as_array().unwrap();
            assert_eq!(
                actual.len(),
                expected.len(),
                "{} independentSourceMapSize",
                h.0.label
            );
            for ((key, value), expected) in actual.iter().zip(expected) {
                h.assert_value(key, &expected["key"], "nativeMapKey");
                h.assert_value(value, &expected["value"], "nativeMapValue");
            }
        }
        if i == 30 {
            assert_eq!(
                h.0.maps.borrow()["Fetches"].len(),
                0,
                "authored ordinary fallback clears map"
            );
        }
        if i == 36 {
            assert_eq!(
                h.0.maps.borrow()["Fetches"].len(),
                1,
                "authored synchronous failure clears before outer stores"
            );
        }
        if i == 48 {
            assert_eq!(
                h.0.latest_calls.get(),
                1,
                "authored WS stale equality does not refetch"
            );
        }
        if i == 51 {
            assert_eq!(
                h.0.latest_calls.get(),
                2,
                "authored WS stale plus1 refetches"
            );
        }
        if i == 55 {
            assert_eq!(
                h.0.emitted.borrow().len(),
                2,
                "authored distinct UTF16 fingerprints emit twice"
            );
        }
        if i == 68 {
            assert_eq!(
                h.0.emitted.borrow().len(),
                1,
                "authored fingerprint assigned before callback throws"
            );
        }
        let emitted = h.0.emitted.borrow();
        let expected = row["emitted"].as_array().unwrap();
        assert_eq!(
            emitted.len(),
            expected.len(),
            "{} independentSourceEmitCount",
            h.0.label
        );
        for (value, expected) in emitted.iter().zip(expected) {
            h.assert_value(value, expected, "nativeEmittedIdentity");
        }
        assert_eq!(
            h.0.pending.borrow().len(),
            0,
            "authored source providers completed"
        );
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 69, "independent authored OHLCV floor");
    for (name, n) in [
        ("fetchOHLCV", 30),
        ("fetchLatestOhlcvWithSharedFallback", 18),
        ("listenOHLCV", 16),
        ("listenOrderBook", 5),
    ] {
        assert_eq!(counts[name], n, "independent method floor");
    }
    assert_eq!(
        corpus["contentHash"], "0cc5812e4a287202afd6784755f08bf7062bb4c2913f8baca7671950e14f99df",
        "actual source/import pin after native assertions"
    );
    eprintln!("69 native cases; stopped at iteration68; bounded OHLCV/Map/timer/Promise hosts");
}

#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn ohlcv_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ohlcv-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceOhlcvMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored first candle timestamp1"),
        "sourceOhlcvMutationMustReachNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
