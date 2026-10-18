use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_account_cache::*,
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
    fields: RefCell<Vec<(String, AccountValue)>>,
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
    symbol: HostThrownValue,
    promises: RefCell<BTreeMap<String, NativePromise<AccountValue>>>,
    native_promises: RefCell<Vec<(Rc<Object>, NativePromise<AccountValue>)>>,
    state: RefCell<BTreeMap<String, AccountValue>>,
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
        });
        self.0.objects.borrow_mut().insert(id.into(), o.clone());
        if let Some(record) = record {
            o.fields.replace(
                record["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f["key"].as_str().unwrap().into(), self.decode(&f["value"])))
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
            fields: RefCell::new(fields),
            local: true,
            array,
            error: false,
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
                    });
                    self.0.objects.borrow_mut().insert(id.into(), o.clone());
                    o.fields.replace(
                        v["fields"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|f| (f["key"].as_str().unwrap().into(), self.decode(&f["value"])))
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
                        self.0.callbacks.borrow().iter().any(|(f, _)| f.ptr_eq(n)),
                        "{l} nativeClosureIdentity"
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
                            field["key"].as_str().unwrap(),
                            "{l} {}",
                            if e["error"] == true {
                                "source error diagnostic key"
                            } else {
                                "nativeOwnKeySchema"
                            }
                        );
                        self.assert_value(value, &field["value"], &format!("{label}:{key}"));
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
                panic!("unexpected inline callback {event}");
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
        if c.name == "Promise.finally" && e.get("error").is_none() {
            let original = self
                .find_promise(&c.receiver)
                .expect("native IIFE Promise identity");
            let AccountValue::Function(f) = &args[0] else {
                panic!("finally native callback")
            };
            let cb = self
                .0
                .callbacks
                .borrow()
                .iter()
                .find(|(v, _)| v.ptr_eq(f))
                .unwrap()
                .1
                .clone();
            let (p, r) = NativePromise::pending(self.runtime());
            original.subscribe(move |result| match cb(&[]) {
                Err(error) => r.settle(Err(error)),
                Ok(AccountValue::Undefined) => r.settle(result.map(|v| (*v).clone())),
                _ => panic!("bounded finally callback result"),
            });
            let value = self.promise_value(p);
            self.assert_value(&value, &e["result"], "nativeFinallyPromise");
            return Ok(value);
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
        if object.is_nullish() {
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
                    ("name".into(), AccountValue::String(u("TypeError"))),
                    ("message".into(), AccountValue::String(u(&message))),
                ]),
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
                if own.array && key == "length" {
                    return Ok(AccountValue::Number(own.fields.borrow().len() as f64));
                }
                return own.get(key);
            }
        }
        let e = self.event("get");
        self.assert_value(object, &e["object"], "getReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativePropertyKey",
            self.0.label
        );
        if self.0.events.borrow().front().is_some_and(|nested| {
            key == "then"
                && nested["op"] == "return-then-get"
                && nested["object"]["id"] == e["object"]["id"]
        }) {
            let nested = self.event("return-then-get");
            self.assert_value(object, &nested["object"], "inheritedThenReceiver");
        }
        if e.get("error").is_some() {
            return Err(self.error(&e["error"]));
        }
        if e["object"]["id"] == "Owner" {
            let value = self.0.state.borrow().get(key).unwrap().clone();
            self.assert_value(&value, &e["result"], "nativeOwnerRead");
            Ok(value)
        } else {
            self.result(&e)
        }
    }
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        let e = self.event("set");
        self.assert_value(object, &e["object"], "setReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativeSetKey",
            self.0.label
        );
        self.assert_value(&value, &e["value"], "nativeSetValue");
        self.result(&e)?;
        self.0.state.borrow_mut().insert(key.into(), value);
        Ok(())
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
        let AccountValue::Number(key) = key else {
            panic!("authored numeric key")
        };
        self.get(object, &format!("{key}"))
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
        let AccountValue::String(value) = value else {
            panic!("authored String last error; full ToString required")
        };
        Ok(value)
    }
}

impl AccountCacheHost for Fixture {
    fn promise_value(&self, p: NativePromise<AccountValue>) -> AccountValue {
        let value = self.fresh(vec![], false);
        self.0
            .native_promises
            .borrow_mut()
            .push((self.marker(&value), p));
        value
    }
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        let (AccountValue::Number(a), AccountValue::Number(b)) = (left, right) else {
            panic!("authored numeric subtraction; full host required")
        };
        Ok(AccountValue::Number(a - b))
    }
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        let (AccountValue::Number(a), AccountValue::Number(b)) = (left, right) else {
            panic!("authored numeric addition; full host required")
        };
        Ok(AccountValue::Number(a + b))
    }
    fn less(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        let (AccountValue::Number(a), AccountValue::Number(b)) = (left, right) else {
            panic!("authored numeric comparison; full host required")
        };
        Ok(a < b)
    }
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_account_cache_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-cache-oracle.cjs"
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
            symbol: HostThrownValue::new("source Symbol"),
            promises: RefCell::default(),
            native_promises: RefCell::default(),
            state: RefCell::default(),
            pending: RefCell::default(),
        }));
        for field in row["initialState"].as_array().unwrap() {
            let value = h.decode(&field["value"]);
            h.0.state
                .borrow_mut()
                .insert(field["key"].as_str().unwrap().into(), value);
        }
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let live: Rc<dyn AccountCacheHost> = Rc::new(h.clone());
        let promise = if row["method"] == "fetchAccountSnapshot" {
            fetch_account_snapshot(live, h.decode(&row["args"][0]))
        } else {
            fetch_account_limits(live)
        };
        let observed = Rc::new(RefCell::new(None));
        let seen = observed.clone();
        promise.subscribe(move |v| *seen.borrow_mut() = Some(v.map(|v| (*v).clone())));
        h.0.runtime.drain();
        if row["input"]["advanceVersion"] == true {
            h.0.state.borrow_mut().insert(
                "accountSnapshotFetchVersion".into(),
                AccountValue::Number(44.0),
            );
        }
        for (stage, step) in steps.iter().enumerate() {
            if stage > 0 {
                h.step(step);
                let name = step["phase"].as_str().unwrap();
                let resolver = h.0.pending.borrow_mut().remove(name).unwrap();
                let provider = row["providers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["name"] == name)
                    .unwrap();
                let result = if provider["reject"].is_null() {
                    Ok(h.decode(&provider["value"]))
                } else {
                    Err(h.throw_value(h.decode(&provider["reject"])))
                };
                resolver.settle(result);
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
                    .expect("source completed native Promise");
                if i == 0 {
                    let Ok(value) = &result else {
                        panic!("authored native success")
                    };
                    let AccountValue::Object(value) = value else {
                        panic!("authored native account")
                    };
                    let AccountValue::Object(account) = h.obj("Account") else {
                        unreachable!()
                    };
                    assert!(
                        Rc::ptr_eq(value, &account),
                        "authored native account identity"
                    );
                }
                h.completion(result, &step["output"], "nativeResult");
            }
            h.end_step();
        }
        let final_state = row["finalState"].as_array().unwrap();
        assert_eq!(
            h.0.state.borrow().len(),
            9,
            "independent authored owner fields floor"
        );
        assert_eq!(
            final_state.len(),
            9,
            "independent source owner fields floor"
        );
        for field in final_state {
            let key = field["key"].as_str().unwrap();
            h.assert_value(
                &h.0.state.borrow()[key],
                &field["value"],
                &format!("nativeFinalState:{key}"),
            );
        }
        assert_eq!(
            h.0.pending.borrow().len(),
            0,
            "independent completed provider count"
        );
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 72, "independent authored cache floor");
    assert_eq!(counts["fetchAccountSnapshot"], 39);
    assert_eq!(counts["fetchAccountLimits"], 33);
    assert_eq!(
        corpus["contentHash"], "bcaf3a4cc1a62b6847d1efa0b8157d3d28211c06fada0e6d469621e2758a6625",
        "actual source/import pin after all native assertions"
    );
    eprintln!("72 native cases; stopped at iteration71; bounded cache/Promise host fixture");
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn account_cache_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-cache-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceCacheMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored first cache success"),
        "sourceCacheMutationMustReachNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
