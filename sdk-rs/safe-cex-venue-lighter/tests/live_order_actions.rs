use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_order_actions::*,
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
    symbol: HostThrownValue,
    places: RefCell<Vec<AccountValue>>,
    cancels: RefCell<Vec<AccountValue>>,
    removed: RefCell<Vec<AccountValue>>,
    sleeps: RefCell<Vec<AccountValue>>,
    cancelled: RefCell<Vec<(AccountValue, AccountValue)>>,
    closed: std::cell::Cell<usize>,
    promises: RefCell<BTreeMap<String, NativePromise<AccountValue>>>,
    native_promises: RefCell<Vec<(Rc<Object>, NativePromise<AccountValue>)>>,
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
                        self.0.callbacks.borrow().iter().any(|(f, _)| f.ptr_eq(n)),
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
    fn callback(&self, e: &Value) -> SigningResult<AccountValue> {
        let f = self.0.functions.borrow()[e["id"].as_str().unwrap()].clone();
        let cb = self
            .0
            .callbacks
            .borrow()
            .iter()
            .find(|(v, _)| v.ptr_eq(&f))
            .unwrap()
            .1
            .clone();
        let args = e["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| self.decode(v))
            .collect::<Vec<_>>();
        let result = cb(&args);
        let expected = self.event("invoke-result");
        self.completion(result.clone(), &expected["result"], "nativeCallbackResult");
        result
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
        let mut mapped = vec![];
        let mut found = AccountValue::Undefined;
        loop {
            let next = self.0.events.borrow().front().cloned().unwrap();
            if next["op"] == "end-call" && next["depth"] == e["depth"] {
                self.event("end-call");
                break;
            }
            if next["op"] == "invoke" {
                let event = self.event("invoke");
                let result = self.callback(&event);
                if c.name == "Array.map" {
                    if let Ok(v) = result.clone() {
                        mapped.push(v);
                    }
                }
                if c.name == "Array.find" {
                    if result.as_ref().is_ok_and(|v| v.truthy()) {
                        found = self.decode(&event["args"][0]);
                    }
                }

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
        if c.name == "Iterator.return" {
            self.0.closed.set(self.0.closed.get() + 1);
        }
        if e.get("error").is_some() {
            return Err(self.error(&e["error"]));
        }
        let actual = match c.name.as_str() {
            "Array.map" => Some(self.array(&mapped)?),
            "Array.find" => Some(found),
            "Array.forEach" => Some(AccountValue::Undefined),
            "Array.push" => {
                let object = self.marker(&c.receiver);
                assert!(object.array, "native push actual array");
                let mut fields = object.fields.borrow_mut();
                for arg in args {
                    let key = u(&fields.len().to_string());
                    fields.push((key, arg.clone()));
                }
                Some(AccountValue::Number(fields.len() as f64))
            }
            "Cancelled.set" => {
                let mut entries = self.0.cancelled.borrow_mut();
                if let Some((_, old)) = entries.iter_mut().find(|(k, _)| same(k, &args[0])) {
                    *old = args[1].clone();
                } else {
                    entries.push((args[0].clone(), args[1].clone()));
                }
                Some(c.receiver.clone())
            }
            _ => None,
        };
        match c.name.as_str() {
            "Owner.placeOrder" => self.0.places.borrow_mut().push(args[0].clone()),
            "Signer.cancelOrder" => {
                let object = self.marker(&args[0]);
                let fields = object.fields.borrow();
                assert_eq!(fields.len(), 2, "authored two cancel request fields");
                assert_eq!(fields[0].0, u("orderId"), "authored native orderId key");
                assert!(
                    matches!(fields[0].1, AccountValue::BigInt(_)),
                    "authored native BigInt cancel"
                );
                assert_eq!(fields[1].0, u("marketId"), "authored native marketId key");
                drop(fields);
                self.0.cancels.borrow_mut().push(args[0].clone());
            }
            "Store.removeOrders" => self.0.removed.borrow_mut().push(args[0].clone()),
            "sleep" => self.0.sleeps.borrow_mut().push(args[0].clone()),
            _ => (),
        }
        if let Some(v) = actual {
            self.assert_value(&v, &e["result"], "nativeBuiltinResult");
            Ok(v)
        } else {
            self.result(&e)
        }
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
    fn set(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        self.define_data(object, key, value)
    }
    fn member(&self, object: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        if object.is_nullish() {
            self.get(object, key)?;
            unreachable!("nullish member get throws")
        }
        let prefix = self
            .0
            .events
            .borrow()
            .front()
            .filter(|e| e["op"] == "get")
            .cloned();
        if let Some(prefix) = &prefix {
            self.event("get");
            self.assert_value(object, &prefix["object"], "nativeProxyMemberReceiver");
            assert_eq!(u(key), units(&prefix["keyUnits"]), "nativeProxyMemberKey");
            if self
                .0
                .events
                .borrow()
                .front()
                .is_none_or(|e| e["op"] != "member")
            {
                return self
                    .result(prefix)
                    .map(|_| panic!("authored method getter requires inner member"));
            }
        }
        let e = self.event("member");
        self.assert_value(object, &e["object"], "memberReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativePropertyKey",
            self.0.label
        );
        let callee_result = self.result(&e);
        if let Some(prefix) = prefix {
            if let Some(error) = prefix.get("error") {
                self.completion(callee_result.clone(), error, "nativeProxyGetterThrow");
            } else {
                self.assert_value(
                    callee_result.as_ref().expect("source getter completed"),
                    &prefix["result"],
                    "nativeProxyGetterCallee",
                );
            }
        }
        let callee = callee_result?;
        Ok(Rc::new(Call {
            h: self.clone(),
            name: e["name"].as_str().unwrap().into(),
            receiver: object.clone(),
            callee,
        }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let mut e = self.event("global");
        let receiver = if name == "Number.isFinite" {
            assert_eq!(e["name"], "Number", "native Number receiver capture");
            let number = self.result(&e)?;
            e = self.event("global");
            Some(number)
        } else {
            None
        };
        assert_eq!(e["name"], name, "{} nativeImportBinding", self.0.label);
        let callee = self.result(&e)?;
        let receiver = if let Some(receiver) = receiver {
            receiver
        } else if e.get("receiver").is_some() {
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

impl OrderActionHost for Fixture {
    fn object_binding_pair(
        &self,
        value: AccountValue,
        keys: (&str, &str),
    ) -> SigningResult<(AccountValue, AccountValue)> {
        if value.is_nullish() {
            let message = if matches!(value, AccountValue::Null) {
                "Cannot destructure property 'order' of 'object null' as it is null."
            } else {
                "Cannot destructure property 'order' of 'undefined' as it is undefined."
            };
            return Err(self.native_type_error(message));
        }
        Ok((self.get(&value, keys.0)?, self.get(&value, keys.1)?))
    }
    fn promise_value(&self, p: NativePromise<AccountValue>) -> AccountValue {
        let v = self.fresh(vec![], false);
        self.0
            .native_promises
            .borrow_mut()
            .push((self.marker(&v), p));
        v
    }
    fn has(&self, value: &AccountValue, key: &str) -> SigningResult<bool> {
        let e = self.event("has");
        self.assert_value(value, &e["object"], "nativeHasReceiver");
        assert_eq!(u(key), units(&e["keyUnits"]), "nativeHasKey");
        let result = self.result(&e)?;
        let native = self
            .marker(value)
            .fields
            .borrow()
            .iter()
            .any(|(k, _)| *k == u(key));
        self.assert_value(
            &AccountValue::Boolean(native),
            &e["result"],
            "nativeHasOwnProperty",
        );
        let AccountValue::Boolean(v) = result else {
            panic!("native has boolean")
        };
        Ok(v)
    }
    fn spread(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        let own = self.marker(value);
        let mut keys = own
            .fields
            .borrow()
            .iter()
            .map(|(k, _)| AccountValue::String(k.clone()))
            .collect::<Vec<_>>();
        keys.extend(
            own.symbols
                .borrow()
                .iter()
                .map(|(k, _)| AccountValue::Symbol(k.clone())),
        );
        let e = self.event("ownKeys");
        self.assert_value(value, &e["object"], "nativeOwnKeysReceiver");
        self.result(&e)?;
        let expected = e["result"]["fields"].as_array().unwrap();
        assert_eq!(
            keys.len(),
            expected.len(),
            "independent native own-key count"
        );
        for (key, expected) in keys.iter().zip(expected) {
            self.assert_value(key, &expected["value"], "nativeOwnKey");
        }
        let mut fields = vec![];
        let mut symbols = vec![];
        for key in keys {
            let e = self.event("descriptor");
            self.assert_value(value, &e["object"], "nativeDescriptorReceiver");
            match &key {
                AccountValue::String(k) => {
                    assert_eq!(*k, units(&e["keyUnits"]), "native descriptor key")
                }
                _ => self.assert_value(&key, &e["keyValue"], "nativeDescriptorSymbol"),
            };
            self.result(&e)?;
            let v = self.index(value, key.clone())?;
            match key {
                AccountValue::String(k) => fields.push((k, v)),
                AccountValue::Symbol(k) => symbols.push((k, v)),
                _ => panic!("native own-key domain"),
            };
        }
        Ok(self.fresh_units(fields, symbols, None))
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        let own = self.marker(object);
        assert!(own.local, "ordinary native fresh data object");
        let mut fields = own.fields.borrow_mut();
        if let Some((_, old)) = fields.iter_mut().find(|(k, _)| *k == u(key)) {
            *old = value;
        } else {
            fields.push((u(key), value));
        }
        Ok(())
    }
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn OrderActionIterator>> {
        if value.is_nullish() {
            return Err(self.native_type_error(
                "(intermediate value) is not iterable (cannot read property null)",
            ));
        }
        let iter = self.member(&value, "@@iterator")?.call(&[])?;
        let next = self.member(&iter, "next")?;
        Ok(Box::new(Iter {
            h: self.clone(),
            iter,
            next,
            done: false,
        }))
    }
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        let mut iter = self.iterator(value)?;
        let mut args = vec![];
        for _ in 0..100 {
            match iter.next()? {
                Some(v) => args.push(v),
                None => return callee.call(&args),
            }
        }
        panic!("argument spread floor exhausted iteration99")
    }
    fn subtract(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) - number(b)))
    }
    fn divide(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) / number(b)))
    }
    fn multiply(&self, a: AccountValue, b: AccountValue) -> SigningResult<AccountValue> {
        Ok(AccountValue::Number(number(a) * number(b)))
    }
    fn greater(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) > number(b))
    }
    fn less_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) <= number(b))
    }
    fn greater_equal(&self, a: AccountValue, b: AccountValue) -> SigningResult<bool> {
        Ok(number(a) >= number(b))
    }
}
struct Iter {
    h: Fixture,
    iter: AccountValue,
    next: Rc<dyn AccountAccessCall>,
    done: bool,
}
impl OrderActionIterator for Iter {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        if self.done {
            return Ok(None);
        }
        let result = (|| {
            let step = self.next.call(&[])?;
            if self.h.get(&step, "done")?.truthy() {
                return Ok(None);
            }
            Ok(Some(self.h.get(&step, "value")?))
        })();
        if result.as_ref().is_err() || result.as_ref().is_ok_and(|v| v.is_none()) {
            self.done = true;
        }
        result
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        if !self.done {
            self.done = true;
            let op = self.h.0.events.borrow().front().cloned().unwrap();
            if op["op"] == "member" {
                if let Ok(close) = self.h.member(&self.iter, "return") {
                    let _ = close.call(&[]);
                }
            } else {
                let value = self.h.get(&self.iter, "return");
                assert!(
                    value.is_err() || value.unwrap().is_nullish(),
                    "bounded absent return"
                );
            }
        }
        error
    }
}
fn number(v: AccountValue) -> f64 {
    let AccountValue::Number(n) = v else {
        panic!("bounded numeric operator fixture")
    };
    n
}
fn same(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        _ => false,
    }
}

impl Fixture {
    fn native_type_error(&self, message: &str) -> TransactionSigningError {
        let o = Rc::new(Object {
            fields: RefCell::new(vec![
                (u("name"), AccountValue::String(u("TypeError"))),
                (u("message"), AccountValue::String(u(message))),
            ]),
            symbols: RefCell::default(),
            regex: None,
            local: true,
            array: false,
            error: true,
        });
        let id = format!("$native{}", self.0.objects.borrow().len());
        self.0.objects.borrow_mut().insert(id, o.clone());
        self.throw_value(AccountValue::Object(o))
    }
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
        self.result(&e)
    }
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_order_actions_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-actions-oracle.cjs"
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
            places: RefCell::default(),
            cancels: RefCell::default(),
            removed: RefCell::default(),
            sleeps: RefCell::default(),
            cancelled: RefCell::default(),
            closed: std::cell::Cell::new(0),
            promises: RefCell::default(),
            native_promises: RefCell::default(),
            pending: RefCell::default(),
        }));
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| h.decode(v))
            .collect::<Vec<_>>();
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let live: Rc<dyn OrderActionHost> = Rc::new(h.clone());
        let observed = Rc::new(RefCell::new(None));
        let promise = match row["method"].as_str().unwrap() {
            "normalizePlaceOrderErrorMessage" => {
                *observed.borrow_mut() = Some(normalize_place_order_error_message(
                    live.as_ref(),
                    args[0].clone(),
                    args[1].clone(),
                ));
                None
            }
            "placeOrdersSequentially" => Some(place_orders_sequentially(live, args[0].clone())),
            "updateOrder" => Some(update_order(live, args[0].clone())),
            "cancelOrders" => Some(cancel_orders(live, args[0].clone())),
            _ => panic!("authored method"),
        };
        if let Some(p) = promise {
            let seen = observed.clone();
            p.subscribe(move |v| *seen.borrow_mut() = Some(v.map(|v| (*v).clone())));
        }
        for (stage, step) in steps.iter().enumerate() {
            if stage > 0 {
                h.step(step);
                let name = step["phase"].as_str().unwrap();
                let provider = row["providers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["name"] == name)
                    .unwrap();
                if provider["hostOnly"] == true {
                    assert!(
                        name.starts_with("timer"),
                        "only delegated sleep timer is host-only"
                    );
                } else {
                    let resolver =
                        h.0.pending.borrow_mut().remove(name).unwrap_or_else(|| {
                            panic!("{} missing native provider {name}", h.0.label)
                        });
                    let value = if provider["reject"].is_null() {
                        Ok(h.decode(&provider["value"]))
                    } else {
                        Err(h.throw_value(h.decode(&provider["reject"])))
                    };
                    resolver.settle(value);
                }
            }
            h.0.runtime.drain();
            for _ in 0..100 {
                let front = h.0.events.borrow().front().cloned();
                match front.as_ref().and_then(|e| e["op"].as_str()) {
                    Some("invoke") => {
                        let e = h.event("invoke");
                        let _ = h.callback(&e);
                        h.0.runtime.drain();
                    }
                    Some("ready") => {
                        let e = h.event("ready");
                        let name = e["name"].as_str().unwrap();
                        let resolver = h.0.pending.borrow_mut().remove(name).unwrap_or_else(|| {
                            panic!("{} missing native delegated promise {name}", h.0.label)
                        });
                        let result = if e["result"]["kind"] == "ok" {
                            Ok(h.decode(&e["result"]["value"]))
                        } else {
                            Err(h.error(&e["result"]))
                        };
                        resolver.settle(result);
                        h.0.runtime.drain();
                    }
                    _ => break,
                }
            }
            if step["output"].is_null() {
                assert!(
                    observed.borrow().is_none(),
                    "{} nativeDeferredCompletion",
                    h.0.label
                );
            } else {
                let value = observed
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| panic!("{} missing native root completion", h.0.label));
                h.completion(value, &step["output"], "nativeRootResult");
            }
            h.end_step();
        }
        for callback in row["callbackResults"].as_array().unwrap() {
            assert!(
                !callback["output"].is_null(),
                "authored source async callback completed"
            );
            let value = h.obj(callback["id"].as_str().unwrap());
            let promise = h
                .find_promise(&value)
                .expect("actual native async callback promise");
            let seen = Rc::new(RefCell::new(None));
            let observed = seen.clone();
            promise.subscribe(move |v| *observed.borrow_mut() = Some(v.map(|v| (*v).clone())));
            h.0.runtime.drain();
            let result = seen
                .borrow()
                .clone()
                .expect("actual native callback completed");
            h.completion(result, &callback["output"], "nativeAsyncCallbackCompletion");
        }
        for (key, actual) in [
            ("places", &h.0.places),
            ("cancels", &h.0.cancels),
            ("removed", &h.0.removed),
        ] {
            let expected = row[key].as_array().unwrap();
            assert_eq!(
                actual.borrow().len(),
                expected.len(),
                "{} native {key} count",
                h.0.label
            );
            for (v, e) in actual.borrow().iter().zip(expected) {
                h.assert_value(v, e, &format!("nativeFinal:{key}"));
            }
        }
        let entries = h.0.cancelled.borrow();
        let expected = row["finalCancelled"].as_array().unwrap();
        assert_eq!(entries.len(), expected.len(), "native final cancelled size");
        for ((key, value), entry) in entries.iter().zip(expected) {
            h.assert_value(key, &entry["key"], "nativeCancelledKey");
            h.assert_value(value, &entry["value"], "nativeCancelledTime");
        }
        assert_eq!(
            h.0.closed.get(),
            row["closed"].as_u64().unwrap() as usize,
            "native iterator close count"
        );
        assert_eq!(
            h.0.pending.borrow().len(),
            0,
            "independent native unresolved provider count"
        );
        if i == 27 {
            assert_eq!(
                h.0.places.borrow().len(),
                2,
                "authored two native sequential placements"
            );
            assert_eq!(
                h.0.closed.get(),
                0,
                "authored native successful iterator no close"
            );
        }
        if i == 30 || i == 31 {
            assert_eq!(
                h.0.closed.get(),
                1,
                "authored native rejected iterator close once"
            );
        }
        if i == 60 {
            assert_eq!(
                h.0.native_promises.borrow().len(),
                13,
                "authored two lot plus eleven request callback promises"
            );
            assert_eq!(
                h.0.cancels.borrow().len(),
                11,
                "authored eleven native cancels"
            );
            assert_eq!(
                h.0.sleeps.borrow().len(),
                1,
                "authored one native lot delay"
            );
            let AccountValue::Number(ms) = h.0.sleeps.borrow()[0] else {
                panic!("native delay")
            };
            assert_eq!(ms, 1000.0);
        }
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 70, "independent authored order-action floor");
    for (name, n) in [
        ("normalizePlaceOrderErrorMessage", 27),
        ("placeOrdersSequentially", 9),
        ("updateOrder", 22),
        ("cancelOrders", 12),
    ] {
        assert_eq!(counts[name], n, "independent method floor");
    }
    assert_eq!(
        corpus["contentHash"], "9786fe5d07878aff50bb990f5c44a15e9aa9701af1cc869753b838bfbc93384e",
        "actual source/import pin after native assertions"
    );
    eprintln!(
        "70 native cases; stopped at iteration69; bounded order-action/iterator/async-import hosts"
    );
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn order_actions_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-actions-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceOrderActionsMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored accidental deviation10"),
        "sourceOrderActionsMutationMustReachNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
