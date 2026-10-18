use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_batch_placement::*,
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
    remembered: RefCell<Vec<(AccountValue, AccountValue)>>,
    hints: RefCell<Vec<(AccountValue, AccountValue)>>,
    deleted: RefCell<Vec<AccountValue>>,
    emitted: RefCell<Vec<AccountValue>>,
    throttles: std::cell::Cell<usize>,
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
        if e["entered"] == true {
            match c.name.as_str() {
                "Adapter.createOrders" => self.0.creates.borrow_mut().push(args[0].clone()),
                "Owner.rememberPlacedOrderHint" => {
                    self.0
                        .remembered
                        .borrow_mut()
                        .push((args[0].clone(), args[1].clone()));
                    let AccountValue::BigInt(id) = &args[0] else {
                        panic!("authored native BigInt hint key")
                    };
                    let key = AccountValue::String(u(&id.to_string()));
                    let mut hints = self.0.hints.borrow_mut();
                    if let Some((_, value)) = hints.iter_mut().find(|(old, _)| same_key(old, &key))
                    {
                        *value = args[1].clone()
                    } else {
                        hints.push((key, args[1].clone()))
                    }
                }
                "Hints.delete" => {
                    self.0.deleted.borrow_mut().push(args[0].clone());
                    self.0
                        .hints
                        .borrow_mut()
                        .retain(|(key, _)| !same_key(key, &args[0]));
                }
                "PrivateSocket.throttleFetchOrders" => {
                    self.0.throttles.set(self.0.throttles.get() + 1)
                }
                "Owner.emitAndLogError" => self.0.emitted.borrow_mut().push(args[0].clone()),
                _ => (),
            }
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
            "Math.floor" | "Math.random" => Some("Math"),
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
            AccountValue::Symbol(_) => {
                let object = Rc::new(Object {
                    fields: RefCell::new(vec![
                        (u("name"), AccountValue::String(u("TypeError"))),
                        (
                            u("message"),
                            AccountValue::String(u("Cannot convert a Symbol value to a string")),
                        ),
                    ]),
                    symbols: RefCell::default(),
                    regex: None,
                    local: true,
                    array: false,
                    error: true,
                });
                let id = format!("$native{}", self.0.objects.borrow().len());
                self.0.objects.borrow_mut().insert(id, object.clone());
                Err(self.throw_value(AccountValue::Object(object)))
            }
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
impl BatchPlacementHost for Fixture {
    fn binary(
        &self,
        op: BatchBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        Ok(match (left, right) {
            (AccountValue::Number(a), AccountValue::Number(b)) => match op {
                BatchBinary::Add => AccountValue::Number(a + b),
                BatchBinary::Multiply => AccountValue::Number(a * b),
                BatchBinary::Remainder => AccountValue::Number(a % b),
                BatchBinary::LessThan => AccountValue::Boolean(a < b),
                BatchBinary::GreaterThan => AccountValue::Boolean(a > b),
            },
            (AccountValue::BigInt(a), AccountValue::BigInt(b)) => match op {
                BatchBinary::Add => AccountValue::BigInt(a + b),
                BatchBinary::Multiply => AccountValue::BigInt(a * b),
                BatchBinary::Remainder => AccountValue::BigInt(a % b),
                BatchBinary::LessThan => AccountValue::Boolean(a < b),
                BatchBinary::GreaterThan => AccountValue::Boolean(a > b),
            },
            _ => panic!("bounded native primitive operator fixture; full coercion host required"),
        })
    }
    fn for_each(&self, value: AccountValue, visitor: BatchVisitor) -> SigningResult<()> {
        let native = self.marker(&value);
        assert!(native.array, "authored prepared array iteration");
        let mut cursor = Cursor::open(self.clone(), value)?;
        for index in 0..100 {
            let Some(value) = cursor.next_value()? else {
                assert_eq!(
                    index,
                    native.fields.borrow().len(),
                    "independent native iterator exhaustion"
                );
                return Ok(());
            };
            let actual = native
                .fields
                .borrow()
                .get(index)
                .expect("native dense prepared row")
                .1
                .clone();
            let (AccountValue::Object(a), AccountValue::Object(b)) = (&value, &actual) else {
                panic!("native prepared object rows")
            };
            assert!(
                Rc::ptr_eq(a, b),
                "native cursor row identity from actual prepared array"
            );
            if let Err(error) = visitor(actual) {
                return Err(cursor.close_throw(error));
            }
            assert!(index < 99, "native batch iterator bound stopped99");
        }
        unreachable!()
    }
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_batch_placement_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/batch-placement-oracle.cjs"
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
            label: format!("iteration{i} placeOrders"),
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
            remembered: RefCell::default(),
            hints: RefCell::default(),
            deleted: RefCell::default(),
            emitted: RefCell::default(),
            throttles: std::cell::Cell::new(0),
        }));
        let args = row["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| h.decode(v))
            .collect::<Vec<_>>();
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let promise = place_orders(Rc::new(h.clone()), args[0].clone());
        let observed: ObservedPromise = Rc::default();
        let seen = observed.clone();
        promise
            .subscribe(move |value| *seen.borrow_mut() = Some(value.map(|value| (*value).clone())));
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
            h.end_step();
            if step["output"].is_null() {
                assert!(
                    observed.borrow().is_none(),
                    "native completion remains queued before provider"
                )
            } else {
                h.completion(
                    observed.borrow().clone().expect("native root settled"),
                    &step["output"],
                    "nativeRootOutcome",
                )
            }
        }
        assert!(
            observed.borrow().is_some(),
            "authored native batch completed"
        );
        let creates = h.0.creates.borrow();
        let expected = row["requests"].as_array().unwrap();
        assert_eq!(
            creates.len(),
            expected.len(),
            "native submitted batch count"
        );
        for (actual, expected) in creates.iter().zip(expected) {
            h.assert_value(actual, expected, "nativeSubmittedBatch")
        }
        for (actual, expected, label) in [
            (&h.0.remembered, &row["remembered"], "nativeRememberedHint"),
            (&h.0.hints, &row["hints"], "nativeRetainedHint"),
        ] {
            let actual = actual.borrow();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{label} count");
            for ((key, value), expected) in actual.iter().zip(expected) {
                h.assert_value(key, &expected[0], label);
                h.assert_value(value, &expected[1], label)
            }
        }
        for (actual, expected, label) in [
            (&h.0.deleted, &row["deleted"], "nativeDeletedHint"),
            (&h.0.emitted, &row["emitted"], "nativeEmittedError"),
        ] {
            let actual = actual.borrow();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{label} count");
            for (value, expected) in actual.iter().zip(expected) {
                h.assert_value(value, expected, label)
            }
        }
        assert_eq!(
            h.0.throttles.get(),
            row["throttles"].as_u64().unwrap() as usize,
            "native throttle work count"
        );
        assert_eq!(
            h.0.closed.get(),
            row["closed"].as_u64().unwrap() as usize,
            "native iterator close count"
        );
        assert!(
            h.0.pending.borrow().is_empty(),
            "authored no remaining native provider"
        );
        if i == 0 || i == 14 || i == 15 {
            assert_eq!(creates.len(), 1, "authored one native submitted batch");
            let requests = h.marker(&creates[0]);
            assert_eq!(
                requests.fields.borrow().len(),
                2,
                "authored two native create requests"
            );
            assert_eq!(
                h.0.remembered.borrow().len(),
                2,
                "authored two real native hint calls"
            );
            for (index, (_, request)) in requests.fields.borrow().iter().enumerate() {
                let request = h.marker(request);
                assert_eq!(
                    request.fields.borrow().len(),
                    12,
                    "authored twelve request fields with builder"
                );
                let read = |name: &str| {
                    request
                        .fields
                        .borrow()
                        .iter()
                        .find(|(key, _)| *key == u(name))
                        .unwrap()
                        .1
                        .clone()
                };
                let AccountValue::BigInt(id) = read("orderId") else {
                    panic!("native request BigInt id")
                };
                assert_eq!(
                    id.to_string(),
                    if i == 14 {
                        "1".to_owned()
                    } else {
                        (123456322 + index).to_string()
                    },
                    "authored native candidate and zero fallback"
                );
                let AccountValue::BigInt(price) = read("price") else {
                    panic!("native scaled price BigInt")
                };
                assert_eq!(
                    price.to_string(),
                    if index == 0 { "1235" } else { "1335" },
                    "authored native scaled price"
                );
                let AccountValue::BigInt(amount) = read("amount") else {
                    panic!("native scaled amount BigInt")
                };
                assert_eq!(
                    amount.to_string(),
                    if index == 0 { "1250" } else { "1750" },
                    "authored native scaled amount"
                );
                assert!(
                    matches!(read("postOnly"),AccountValue::Boolean(value)if value==(index==0)),
                    "authored repeated TIF post-only decision"
                );
                let AccountValue::String(tif) = read("timeInForce") else {
                    panic!("native TIF")
                };
                assert_eq!(
                    tif,
                    u(if index == 0 && i != 15 {
                        "PostOnly"
                    } else {
                        "GoodTillCancel"
                    }),
                    "authored cached first TIF"
                );
            }
            assert_eq!(
                h.0.hints.borrow().len(),
                if i == 14 { 1 } else { 2 },
                "authored source duplicate wrap-ID overwrites first hint"
            );
        }
        if i == 4 {
            assert!(creates.is_empty(), "authored oversize does not submit");
            assert!(
                h.0.remembered.borrow().is_empty(),
                "authored prepared rows have no remembered hints on oversize fallback"
            )
        }
        if i == 9 {
            assert_eq!(
                h.0.deleted.borrow().len(),
                2,
                "authored two failed batch hint deletes"
            );
            assert!(
                h.0.hints.borrow().is_empty(),
                "authored no hints after batch rejection"
            );
            assert_eq!(
                h.0.emitted.borrow().len(),
                1,
                "authored one rejection message"
            )
        }
        if i == 12 {
            assert_eq!(
                h.0.throttles.get(),
                2,
                "authored success throttle failure enters catch then throttles again"
            );
            assert_eq!(
                h.0.deleted.borrow().len(),
                2,
                "authored cleanup after success throttle throw"
            )
        }
        if i == 56 || i == 57 {
            assert_eq!(
                h.0.closed.get(),
                1,
                "authored one abrupt native iterator close"
            )
        }
    }
    assert_eq!(rows.len(), 58, "independent authored batch floor");
    assert_eq!(
        corpus["contentHash"], "85d7fd145f15aa6a0f4f82c770a0c9a1affd14b8c9c6968cdfcd61232b0c7fd9",
        "source/import pin after native assertions"
    );
    eprintln!(
        "58 native batch cases; stopped at iteration57; bounded primitive arithmetic and dense-array iterator hosts"
    );
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn batch_placement_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/batch-placement-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceBatchPlacementMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr)
            .contains("authored base1000 plus random321 plus first1"),
        "sourceBatchPlacementMutationMustReachNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
