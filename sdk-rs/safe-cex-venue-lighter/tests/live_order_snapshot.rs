use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_order_snapshot::*,
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
type ObservedPromise = Rc<RefCell<Option<SigningResult<AccountValue>>>>;
struct Env {
    label: String,
    row: Value,
    runtime: Rc<Runtime>,
    events: RefCell<VecDeque<Value>>,
    objects: RefCell<BTreeMap<String, Rc<Object>>>,
    functions: RefCell<BTreeMap<String, HostThrownValue>>,
    callbacks: RefCell<Vec<(HostThrownValue, TradingCallback)>>,
    cancelled: RefCell<Vec<(AccountValue, AccountValue)>>,
    requests: RefCell<Vec<Vec<AccountValue>>>,
    map_closed: std::cell::Cell<usize>,
    receipts: RefCell<BTreeMap<String, ObservedPromise>>,
    symbol: HostThrownValue,
    state: RefCell<BTreeMap<String, AccountValue>>,
    updates: RefCell<Vec<AccountValue>>,
    posts: RefCell<Vec<Vec<AccountValue>>>,
    creates: RefCell<Vec<AccountValue>>,
    connections: RefCell<Vec<String>>,
    loops: RefCell<Vec<(AccountValue, AccountValue)>>,
    awaited: RefCell<Vec<AccountValue>>,
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
            local: false,
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
    fn native_promise(&self, value: &AccountValue) -> Option<NativePromise<AccountValue>> {
        let AccountValue::Object(value) = value else {
            return None;
        };
        self.0
            .native_promises
            .borrow()
            .iter()
            .find(|(object, _)| Rc::ptr_eq(value, &(object.clone() as Rc<dyn AccountObject>)))
            .map(|(_, promise)| promise.clone())
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
                        "{l} nativeObjectBijection expectedId={id}; aliases={:?}",
                        self.0
                            .objects
                            .borrow()
                            .iter()
                            .filter(|(key, old)| !key.starts_with("$native")
                                && Rc::ptr_eq(n, &(Rc::clone(*old) as Rc<dyn AccountObject>)))
                            .map(|(key, _)| key.clone())
                            .collect::<Vec<_>>()
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
        if let Ok(value) = &result {
            let promise = self.native_promise(value);
            if let Some(promise) = promise {
                let id = expected["result"]["value"]["id"].as_str().unwrap();
                let observed: ObservedPromise = Rc::default();
                let seen = observed.clone();
                promise.subscribe(move |value| {
                    *seen.borrow_mut() = Some(value.map(|value| (*value).clone()))
                });
                assert!(
                    self.0
                        .receipts
                        .borrow_mut()
                        .insert(id.into(), observed)
                        .is_none(),
                    "authored one receipt per actual native callback Promise"
                );
            }
        }
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
        let mut filtered = vec![];
        let mut flat = vec![];
        let mut some = false;
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
                if c.name == "Array.filter" && result.as_ref().is_ok_and(|value| value.truthy()) {
                    filtered.push(self.decode(&event["args"][0]));
                }
                if c.name == "Array.some" {
                    some |= result.as_ref().is_ok_and(|value| value.truthy());
                }
                if c.name == "Array.flatMap" {
                    if let Ok(value) = &result {
                        if matches!(value, AccountValue::Object(_)) && self.marker(value).array {
                            flat.extend(
                                self.marker(value)
                                    .fields
                                    .borrow()
                                    .iter()
                                    .map(|(_, value)| value.clone()),
                            );
                        } else {
                            flat.push(value.clone());
                        }
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
        if c.name == "MapIterator.return" {
            self.0.map_closed.set(self.0.map_closed.get() + 1);
        }
        if c.name == "Iterator.return" {
            self.0.closed.set(self.0.closed.get() + 1);
        }
        if c.name == "ApiClient.get" {
            self.0.requests.borrow_mut().push(args.to_vec());
        }
        if e.get("error").is_some() {
            return Err(self.error(&e["error"]));
        }
        let actual = match c.name.as_str() {
            "Array.map" => Some(self.array(&mapped)?),
            "Array.filter" => Some(self.array(&filtered)?),
            "Array.flatMap" => Some(self.array(&flat)?),
            "Array.some" => Some(AccountValue::Boolean(some)),
            "Cancelled.has" => Some(AccountValue::Boolean(
                self.0
                    .cancelled
                    .borrow()
                    .iter()
                    .any(|(key, _)| same_key(key, &args[0])),
            )),
            "Cancelled.delete" => {
                let mut entries = self.0.cancelled.borrow_mut();
                let old = entries.len();
                entries.retain(|(key, _)| !same_key(key, &args[0]));
                Some(AccountValue::Boolean(entries.len() != old))
            }
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
            _ => None,
        };
        match c.name.as_str() {
            "Store.update" => self.0.updates.borrow_mut().push(args[0].clone()),
            "ApiClient.post" => self.0.posts.borrow_mut().push(args.to_vec()),
            "createLighterSignerAdapter" => {
                let object = self.marker(&args[0]);
                assert_eq!(
                    object.fields.borrow().len(),
                    5,
                    "authored five signer constructor fields"
                );
                self.0.creates.borrow_mut().push(args[0].clone());
            }
            "PublicSocket.connectAndSubscribe" => {
                self.0.connections.borrow_mut().push("public".into())
            }
            "PrivateSocket.connectAndSubscribe" => {
                self.0.connections.borrow_mut().push("private".into())
            }
            "loop" => self
                .0
                .loops
                .borrow_mut()
                .push((args[0].clone(), args[1].clone())),
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
        let e = self.event("set");
        self.assert_value(object, &e["object"], "nativeSetReceiver");
        assert_eq!(u(key), units(&e["keyUnits"]), "nativeSetKey");
        self.assert_value(&value, &e["value"], "nativeSetValue");
        self.result(&e)?;
        assert_eq!(e["object"]["id"], "Owner", "authored owner slots");
        assert!(
            self.0.state.borrow().contains_key(key),
            "independent owner slot schema"
        );
        self.0.state.borrow_mut().insert(key.into(), value);
        Ok(())
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
        let prefix = match name {
            "Array.isArray" => Some("Array"),
            "Number.isFinite" | "Number.parseInt" => Some("Number"),
            "Object.values" => Some("Object"),
            "Date.now" => Some("Date"),
            "Promise.allSettled" => Some("Promise"),
            _ => None,
        };
        let receiver = if let Some(prefix) = prefix {
            assert_eq!(e["name"], prefix, "native builtin receiver capture");
            let receiver = self.result(&e)?;
            e = self.event("global");
            Some(receiver)
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
        self.0.awaited.borrow_mut().push(value.clone());
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

impl Fixture {
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
        if !matches!(object, AccountValue::Object(_)) {
            if let AccountValue::String(key) = &key {
                if let AccountValue::String(value) = object {
                    if *key == u("length") {
                        return Ok(AccountValue::Number(value.len() as f64));
                    }
                }
                return Ok(AccountValue::Undefined);
            }
            panic!("bounded primitive property key");
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
        // A Proxy wrapping an accessor records both the outer [[Get]] and
        // the accessor's own observation; validate both, including thrown identity.
        let nested = self
            .0
            .events
            .borrow()
            .front()
            .filter(|next| {
                e["object"]["id"] == "Store"
                    && units(&e["keyUnits"]) == u("loaded")
                    && next["op"] == "get"
                    && next["object"] == e["object"]
                    && next["keyUnits"] == e["keyUnits"]
                    && next["keyValue"] == e["keyValue"]
            })
            .cloned();
        if nested.is_some() {
            let inner = self.event("get");
            self.assert_value(object, &inner["object"], "nativeNestedGetterReceiver");
            if let AccountValue::String(key) = &key {
                assert_eq!(*key, units(&inner["keyUnits"]), "nativeNestedGetterKey");
            } else {
                self.assert_value(&key, &inner["keyValue"], "nativeNestedSymbolKey");
            }
            let actual = self.result(&inner);
            if let Some(error) = e.get("error") {
                self.completion(actual.clone(), error, "nativeNestedGetterThrown");
            } else {
                self.assert_value(
                    actual.as_ref().expect("source accessor completed"),
                    &e["result"],
                    "nativeNestedGetterValue",
                );
            }
            return actual;
        }
        if e.get("error").is_some() {
            return Err(self.error(&e["error"]));
        }
        if e["object"]["id"] == "Owner" {
            let AccountValue::String(key) = &key else {
                panic!("owner named key")
            };
            let key = String::from_utf16(key).unwrap();
            let value = self.0.state.borrow().get(&key).cloned();
            if let Some(value) = value {
                self.assert_value(&value, &e["result"], "nativeOwnerSlot");
                return Ok(value);
            }
        }
        self.result(&e)
    }
}
fn same_key(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => panic!("bounded cancellation string/Symbol keys"),
    }
}
fn same_scalar(a: &AccountValue, b: &AccountValue) {
    match (a, b) {
        (AccountValue::String(a), AccountValue::String(b)) => {
            assert_eq!(a, b, "native cancellation key units")
        }
        (AccountValue::Number(a), AccountValue::Number(b)) => assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "native cancellation timestamp bits"
        ),
        _ => panic!("authored cancellation scalar domain"),
    }
}
struct Cursor {
    h: Fixture,
    iterator: AccountValue,
    next: Rc<dyn AccountAccessCall>,
    done: bool,
}
impl Cursor {
    fn open(h: Fixture, value: AccountValue) -> SigningResult<Self> {
        let iterator = h.member(&value, "@@iterator")?.call(&[])?;
        let next = h.member(&iterator, "next")?;
        Ok(Self {
            h,
            iterator,
            next,
            done: false,
        })
    }
    fn next_value(&mut self) -> SigningResult<Option<AccountValue>> {
        if self.done {
            return Ok(None);
        }
        let step = self.next.call(&[])?;
        if self.h.get(&step, "done")?.truthy() {
            self.done = true;
            return Ok(None);
        }
        Ok(Some(self.h.get(&step, "value")?))
    }
    fn close_normal(&mut self) -> SigningResult<()> {
        if self.done {
            return Ok(());
        }
        self.done = true;
        let next = self
            .h
            .0
            .events
            .borrow()
            .front()
            .cloned()
            .expect("authored IteratorClose observation");
        if next["op"] == "member" {
            let method = self.h.member(&self.iterator, "return")?;
            let result = method.call(&[])?;
            assert!(
                matches!(result, AccountValue::Object(_) | AccountValue::Function(_)),
                "authored IteratorClose object return"
            );
        } else {
            assert!(
                self.h.get(&self.iterator, "return")?.is_nullish(),
                "authored missing iterator return"
            );
        }
        Ok(())
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        let _ = self.close_normal();
        error
    }
}
struct Entries {
    outer: Cursor,
    seen: Vec<AccountValue>,
}
impl CancelledOrderEntries for Entries {
    fn next_entry(&mut self) -> SigningResult<Option<(AccountValue, AccountValue)>> {
        let Some(pair) = self.outer.next_value()? else {
            return Ok(None);
        };
        let h = self.outer.h.clone();
        let bound = (|| {
            let mut inner = Cursor::open(h.clone(), pair)?;
            let result = (|| {
                let id = inner.next_value()?.unwrap_or(AccountValue::Undefined);
                let timestamp = inner.next_value()?.unwrap_or(AccountValue::Undefined);
                inner.close_normal()?;
                Ok((id, timestamp))
            })();
            match result {
                Ok(value) => Ok(value),
                Err(error) => Err(inner.close_throw(error)),
            }
        })();
        let (id, timestamp) = match bound {
            Ok(value) => value,
            Err(error) => return Err(self.outer.close_throw(error)),
        };
        let expected =
            h.0.cancelled
                .borrow()
                .iter()
                .find(|(key, _)| !self.seen.iter().any(|seen| same_key(key, seen)))
                .cloned()
                .expect("independent native cancellation iterator state");
        same_scalar(&id, &expected.0);
        same_scalar(&timestamp, &expected.1);
        self.seen.push(expected.0.clone());
        Ok(Some(expected))
    }
    fn close_on_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        self.outer.close_throw(error)
    }
}
impl OrderSnapshotHost for Fixture {
    fn visit_until(
        &self,
        iterable: AccountValue,
        visitor: CandidateVisitor,
    ) -> SigningResult<Option<AccountValue>> {
        let mut cursor = Cursor::open(self.clone(), iterable)?;
        for index in 0..100 {
            let Some(value) = cursor.next_value()? else {
                return Ok(None);
            };
            match visitor(value) {
                Ok(Some(value)) => {
                    cursor.close_normal()?;
                    return Ok(Some(value));
                }
                Ok(None) => (),
                Err(error) => return Err(cursor.close_throw(error)),
            }
            assert!(index < 99, "authored candidate iterator floor stopped99");
        }
        unreachable!()
    }
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        let value = self.fresh(vec![], false);
        self.0
            .native_promises
            .borrow_mut()
            .push((self.marker(&value), promise));
        value
    }
    fn regex(&self, pattern: &[u16], flags: &[u16]) -> SigningResult<AccountValue> {
        let object = Rc::new(Object {
            fields: RefCell::default(),
            symbols: RefCell::default(),
            regex: Some((pattern.to_vec(), flags.to_vec())),
            local: true,
            array: false,
            error: false,
        });
        let id = format!("$native{}", self.0.objects.borrow().len());
        self.0.objects.borrow_mut().insert(id, object.clone());
        Ok(AccountValue::Object(object))
    }
    fn has_key(&self, object: &AccountValue, key: &str) -> SigningResult<bool> {
        let event = self.event("has");
        self.assert_value(object, &event["object"], "nativeHasReceiver");
        assert_eq!(u(key), units(&event["keyUnits"]), "nativeHasKey");
        if event.get("error").is_some() {
            return Err(self.error(&event["error"]));
        }
        let actual = self
            .marker(object)
            .fields
            .borrow()
            .iter()
            .any(|(field, _)| *field == u(key));
        self.assert_value(
            &AccountValue::Boolean(actual),
            &event["result"],
            "nativeOwnHasResult",
        );
        Ok(actual)
    }
    fn global_value(&self, name: &str) -> SigningResult<AccountValue> {
        let event = self.event("global-value");
        assert_eq!(event["name"], name, "nativeGlobalValueName");
        self.result(&event)
    }
    fn instance_of(&self, value: &AccountValue, constructor: AccountValue) -> SigningResult<bool> {
        let lookup = self.event("instanceof-lookup");
        self.assert_value(
            &constructor,
            &lookup["constructor"],
            "nativeInstanceofConstructor",
        );
        self.result(&lookup)?;
        let event = self.event("instanceof");
        self.assert_value(
            &constructor,
            &event["constructor"],
            "nativeInstanceofReceiver",
        );
        self.assert_value(value, &event["object"], "nativeInstanceofValue");
        if event.get("error").is_some() {
            return Err(self.error(&event["error"]));
        }
        let actual = matches!(value, AccountValue::Object(_)) && self.marker(value).error;
        self.assert_value(
            &AccountValue::Boolean(actual),
            &event["result"],
            "nativeAuthoredErrorInstance",
        );
        Ok(actual)
    }
    fn subtract(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        let (AccountValue::Number(left), AccountValue::Number(right)) = (left, right) else {
            panic!("bounded numeric subtraction");
        };
        Ok(AccountValue::Number(left - right))
    }
    fn greater_than(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        let (AccountValue::Number(left), AccountValue::Number(right)) = (left, right) else {
            panic!("bounded numeric comparison");
        };
        Ok(left > right)
    }
    fn greater_or_equal(&self, left: AccountValue, right: AccountValue) -> SigningResult<bool> {
        let (AccountValue::Number(left), AccountValue::Number(right)) = (left, right) else {
            panic!("bounded numeric comparison");
        };
        Ok(left >= right)
    }
    fn cancelled_entries(
        &self,
        value: AccountValue,
    ) -> SigningResult<Box<dyn CancelledOrderEntries>> {
        Ok(Box::new(Entries {
            outer: Cursor::open(self.clone(), value)?,
            seen: vec![],
        }))
    }
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_order_snapshot_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-snapshot-oracle.cjs"
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
    for (i, row) in rows.iter().enumerate() {
        let h = Fixture(Rc::new(Env {
            label: format!("iteration{i} fetchOrdersOnce"),
            row: row.clone(),
            runtime: Rc::default(),
            events: RefCell::default(),
            objects: RefCell::default(),
            functions: RefCell::default(),
            callbacks: RefCell::default(),
            symbol: HostThrownValue::new("source Symbol"),
            cancelled: RefCell::default(),
            requests: RefCell::default(),
            map_closed: std::cell::Cell::new(0),
            receipts: RefCell::default(),
            state: RefCell::default(),
            updates: RefCell::default(),
            posts: RefCell::default(),
            creates: RefCell::default(),
            connections: RefCell::default(),
            loops: RefCell::default(),
            awaited: RefCell::default(),
            closed: std::cell::Cell::new(0),
            promises: RefCell::default(),
            native_promises: RefCell::default(),
            pending: RefCell::default(),
        }));
        for field in row["initialState"].as_array().unwrap() {
            h.0.state.borrow_mut().insert(
                field["key"].as_str().unwrap().into(),
                h.decode(&field["value"]),
            );
        }
        assert_eq!(
            h.0.state.borrow().len(),
            3,
            "authored three native warning slots"
        );
        for pair in row["initialCancelled"].as_array().unwrap() {
            h.0.cancelled
                .borrow_mut()
                .push((h.decode(&pair[0]), h.decode(&pair[1])));
        }
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let promise = fetch_orders_once(Rc::new(h.clone()));
        let observed: ObservedPromise = Rc::default();
        let seen = observed.clone();
        promise
            .subscribe(move |value| *seen.borrow_mut() = Some(value.map(|value| (*value).clone())));
        let mut receipt_count = 0;
        for (index, step) in steps.iter().enumerate() {
            if index > 0 {
                h.step(step);
                let name = step["phase"].as_str().unwrap();
                let resolver =
                    h.0.pending
                        .borrow_mut()
                        .remove(name)
                        .unwrap_or_else(|| panic!("{} missing native provider {name}", h.0.label));
                let provider = row["providers"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|p| p["name"] == name)
                    .unwrap();
                resolver.settle(if provider["reject"].is_null() {
                    Ok(h.decode(&provider["value"]))
                } else {
                    Err(h.throw_value(h.decode(&provider["reject"])))
                });
            }
            h.0.runtime.drain();
            for event_index in 0..100 {
                let next = h.0.events.borrow().front().cloned();
                match next.as_ref().and_then(|e| e["op"].as_str()) {
                    Some("invoke") => {
                        let event = h.event("invoke");
                        let _ = h.callback(&event);
                        h.0.runtime.drain();
                    }
                    Some("callback-ready") => {
                        let event = h.event("callback-ready");
                        let id = event["id"].as_str().unwrap();
                        let receipt =
                            h.0.receipts
                                .borrow()
                                .get(id)
                                .unwrap_or_else(|| {
                                    panic!("{} missing native callback Promise {id}", h.0.label)
                                })
                                .clone();
                        let actual = receipt.borrow().clone().unwrap_or_else(|| {
                            panic!(
                                "{} native callback outcome not settled at source receipt",
                                h.0.label
                            )
                        });
                        h.completion(actual, &event["result"], "nativeAsyncCallbackOutcome");
                        receipt_count += 1;
                    }
                    Some("ready") => {
                        let event = h.event("ready");
                        let name = event["name"].as_str().unwrap();
                        if name.starts_with("settled") {
                            assert_eq!(
                                receipt_count,
                                h.0.receipts.borrow().len(),
                                "native async outcomes asserted before settlement input"
                            );
                            assert!(
                                h.0.receipts
                                    .borrow()
                                    .values()
                                    .all(|receipt| receipt.borrow().is_some()),
                                "native callback outcomes before settlement input"
                            );
                        }
                        let resolver = h.0.pending.borrow_mut().remove(name).unwrap_or_else(|| {
                            panic!("{} missing native delegated promise {name}", h.0.label)
                        });
                        resolver.settle(if event["result"]["kind"] == "ok" {
                            Ok(h.decode(&event["result"]["value"]))
                        } else {
                            Err(h.error(&event["result"]))
                        });
                        h.0.runtime.drain();
                    }
                    _ => break,
                }
                assert!(
                    event_index < 99,
                    "authored native observer event floor stopped99"
                );
            }
            if step["output"].is_null() {
                assert!(
                    observed.borrow().is_none(),
                    "{} nativeDeferredCompletion",
                    h.0.label
                );
            } else {
                h.completion(
                    observed.borrow().clone().expect("native root completion"),
                    &step["output"],
                    "nativeRootResult",
                );
            }
            h.end_step();
        }
        for field in row["finalState"].as_array().unwrap() {
            h.assert_value(
                &h.0.state.borrow()[field["key"].as_str().unwrap()],
                &field["value"],
                "nativeFinalWarningSlot",
            );
        }
        let expected = row["finalCancelled"].as_array().unwrap();
        assert_eq!(
            h.0.cancelled.borrow().len(),
            expected.len(),
            "native final cancellation count"
        );
        for ((key, value), expected) in h.0.cancelled.borrow().iter().zip(expected) {
            h.assert_value(key, &expected[0], "nativeCancelledKey");
            h.assert_value(value, &expected[1], "nativeCancelledTimestamp");
        }
        let requests = row["requests"].as_array().unwrap();
        assert_eq!(
            h.0.requests.borrow().len(),
            requests.len(),
            "native request count"
        );
        for (actual, expected) in h.0.requests.borrow().iter().zip(requests) {
            assert_eq!(actual.len(), 2, "authored two GET operands");
            for (value, expected) in actual.iter().zip(expected.as_array().unwrap()) {
                h.assert_value(value, expected, "nativeRequestArgument");
            }
        }
        assert_eq!(
            h.0.closed.get(),
            row["closed"].as_u64().unwrap() as usize,
            "native array IteratorClose count"
        );
        assert_eq!(
            h.0.map_closed.get(),
            row["mapClosed"].as_u64().unwrap() as usize,
            "native outer IteratorClose count"
        );
        assert_eq!(
            h.0.receipts.borrow().len(),
            row["callbackPromises"].as_array().unwrap().len(),
            "native async callback Promise count"
        );
        assert_eq!(
            receipt_count,
            h.0.receipts.borrow().len(),
            "all actual native callback outcomes checked"
        );
        if i == 12 || i == 13 || i == 14 {
            assert_eq!(
                receipt_count, 2,
                "authored two real native async market outcomes"
            );
        }
        if i == 32 {
            assert_eq!(
                h.0.closed.get(),
                5,
                "authored native candidate plus four pair closes"
            );
        }
        if i == 34 || i == 35 {
            assert_eq!(
                h.0.map_closed.get(),
                1,
                "authored native abrupt outer close"
            );
        }
        assert_eq!(
            h.0.pending.borrow().len(),
            0,
            "authored no remaining native provider"
        );
    }
    assert_eq!(rows.len(), 60, "independent authored order snapshot floor");
    assert_eq!(
        corpus["contentHash"], "87ea98d83a0a9b5a2054530bafe8f28c119bcfdccc5ba3a41d56e758fbaca7f7",
        "actual source/import pin after native assertions"
    );
    eprintln!(
        "60 native cases; stopped at iteration59; bounded iterator, Promise, owner and import hosts"
    );
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn order_snapshot_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-snapshot-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceOrderSnapshotMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored all-markets sentinel255"),
        "sourceOrderSnapshotMutationMustReachNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
