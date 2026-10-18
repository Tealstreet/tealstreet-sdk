use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_single_placement::*,
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
    wire: RefCell<Vec<(AccountValue, Value, String)>>,
    logs: RefCell<Vec<Vec<AccountValue>>>,
    sleeps: RefCell<Vec<AccountValue>>,
    children: RefCell<Vec<AccountValue>>,
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
            if c.name == "Owner.log" && i == 0 {
                self.0.wire.borrow_mut().push((
                    value.clone(),
                    expected.clone(),
                    format!("{} forwarded JSON log", self.0.label),
                ));
            } else {
                self.assert_value(value, expected, &format!("{} arg{i}", c.name));
            }
        }
        let mut mapped = vec![];
        let mut from_values = vec![];
        let mut json_values = vec![];
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
                if c.name == "Array.from" {
                    assert_eq!(
                        event["args"].as_array().unwrap().len(),
                        2,
                        "Array.from callback two native arguments"
                    );
                    self.assert_value(
                        &AccountValue::Undefined,
                        &event["args"][0],
                        "native from missing element",
                    );
                    self.assert_value(
                        &AccountValue::Number(from_values.len() as f64),
                        &event["args"][1],
                        "native from index",
                    );
                }
                if c.name == "JSON.stringify" {
                    let input = self.marker(&args[0]);
                    let fields = input.fields.borrow();
                    let index = json_values.len();
                    let (key, value) = if index == 0 {
                        (vec![], args[0].clone())
                    } else {
                        fields
                            .get(index - 1)
                            .expect("native JSON own field")
                            .clone()
                    };
                    self.assert_value(
                        &AccountValue::String(key),
                        &event["args"][0],
                        "native JSON replacer key BEFORE wire",
                    );
                    self.assert_value(
                        &value,
                        &event["args"][1],
                        "native JSON replacer value BEFORE wire",
                    );
                }
                let result = self.callback(&event);
                if c.name == "Array.from" {
                    if let Ok(value) = &result {
                        from_values.push(value.clone())
                    }
                }
                if c.name == "JSON.stringify" {
                    if let Ok(value) = &result {
                        json_values.push(value.clone())
                    }
                }
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
                "Adapter.createOrder" => self.0.creates.borrow_mut().push(args[0].clone()),
                "Owner.log" => self.0.logs.borrow_mut().push(args.to_vec()),
                "sleep" => self.0.sleeps.borrow_mut().push(args[0].clone()),
                "Owner.placeOrder" => self.0.children.borrow_mut().push(args[0].clone()),
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
            "Array.from" => {
                let source = self.marker(&args[0]);
                let fields = source.fields.borrow();
                assert_eq!(fields.len(), 1, "native from input one own field");
                assert_eq!(fields[0].0, u("length"));
                let AccountValue::Number(length) = fields[0].1 else {
                    panic!("bounded native from length")
                };
                assert!(length.is_finite() && (0.0..=100.0).contains(&length));
                assert_eq!(
                    from_values.len(),
                    length.floor() as usize,
                    "independent native from length"
                );
                Some(self.array(&from_values)?)
            }
            "Array.pop" => {
                let array = self.marker(&c.receiver);
                assert!(array.array);
                let value = array
                    .fields
                    .borrow_mut()
                    .pop()
                    .map(|(_, value)| value)
                    .unwrap_or(AccountValue::Undefined);
                Some(value)
            }
            "BigInt.toString" => {
                let AccountValue::BigInt(value) = &c.receiver else {
                    panic!("native BigInt receiver")
                };
                assert!(args.is_empty());
                Some(AccountValue::String(u(&value.to_string())))
            }
            "JSON.stringify" => Some(self.json_from_native(&args[0], &json_values)),
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
            if c.name == "JSON.stringify" {
                self.0.wire.borrow_mut().push((
                    v.clone(),
                    e["result"].clone(),
                    format!("{} bounded JSON wire", self.0.label),
                ));
            } else {
                self.assert_value(&v, &e["result"], "nativeBuiltinResult");
            }
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
            "Array.isArray" | "Array.from" => Some("Array"),
            "Number.isFinite" | "Number.parseInt" => Some("Number"),
            "Math.floor" | "Math.random" | "Math.ceil" => Some("Math"),
            "JSON.stringify" => Some("JSON"),
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
fn same_native(actual: &AccountValue, expected: &AccountValue, label: &str) {
    match (actual, expected) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null) => (),
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => {
            assert_eq!(a, b, "{label} native bool")
        }
        (AccountValue::Number(a), AccountValue::Number(b)) => {
            assert_eq!(a.to_bits(), b.to_bits(), "{label} native Number bits")
        }
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => {
            assert_eq!(a, b, "{label} native BigInt")
        }
        (AccountValue::String(a), AccountValue::String(b)) => {
            assert_eq!(a, b, "{label} native UTF16")
        }
        (AccountValue::Object(a), AccountValue::Object(b)) => {
            assert!(Rc::ptr_eq(a, b), "{label} native object alias")
        }
        (AccountValue::Function(a), AccountValue::Function(b))
        | (AccountValue::Symbol(a), AccountValue::Symbol(b)) => {
            assert!(a.ptr_eq(b), "{label} native callable/symbol alias")
        }
        _ => panic!("{label} native kind"),
    }
}
fn json_quote(units: &[u16]) -> Vec<u16> {
    let mut out = vec![34];
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        match unit {
            34 => out.extend(u("\\\"")),
            92 => out.extend(u("\\\\")),
            8 => out.extend(u("\\b")),
            9 => out.extend(u("\\t")),
            10 => out.extend(u("\\n")),
            12 => out.extend(u("\\f")),
            13 => out.extend(u("\\r")),
            0..=31 => out.extend(u(&format!("\\u{unit:04x}"))),
            0xd800..=0xdbff => {
                if index + 1 < units.len() && (0xdc00..=0xdfff).contains(&units[index + 1]) {
                    out.push(unit);
                    index += 1;
                    out.push(units[index])
                } else {
                    out.extend(u(&format!("\\u{unit:04x}")))
                }
            }
            0xdc00..=0xdfff => out.extend(u(&format!("\\u{unit:04x}"))),
            _ => out.push(unit),
        }
        index += 1;
    }
    out.push(34);
    out
}
fn json_scalar(value: &AccountValue) -> Option<Vec<u16>> {
    match value {
        AccountValue::Undefined | AccountValue::Symbol(_) => None,
        AccountValue::Null => Some(u("null")),
        AccountValue::Boolean(value) => Some(u(if *value { "true" } else { "false" })),
        AccountValue::String(value) => Some(json_quote(value)),
        AccountValue::Number(value) => Some(u(&if value.is_finite() {
            safe_cex_num::js_number_string(*value)
        } else {
            "null".into()
        })),
        _ => panic!("bounded scalar JSON-after-native-replacer fixture; full engine host required"),
    }
}
impl Fixture {
    fn json_from_native(&self, params: &AccountValue, results: &[AccountValue]) -> AccountValue {
        let object = self.marker(params);
        let fields = object.fields.borrow();
        assert_eq!(
            results.len(),
            fields.len() + 1,
            "{} independent native JSON callback count",
            self.0.label
        );
        same_native(
            &results[0],
            params,
            &format!("{} JSON root identity", self.0.label),
        );
        let mut out = vec![123];
        let mut first = true;
        for ((key, _), result) in fields.iter().zip(&results[1..]) {
            let Some(value) = json_scalar(result) else {
                continue;
            };
            if !first {
                out.push(44)
            }
            first = false;
            out.extend(json_quote(key));
            out.push(58);
            out.extend(value)
        }
        out.push(125);
        AccountValue::String(out)
    }
    fn native_values(&self, value: &AccountValue) -> Vec<AccountValue> {
        let array = self.marker(value);
        assert!(array.array, "bounded dense native array");
        let values = array
            .fields
            .borrow()
            .iter()
            .map(|(_, value)| value.clone())
            .collect();
        values
    }
    fn copy_object(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        if source.is_nullish() {
            return Ok(self.fresh(vec![], false));
        }
        let source_object = self.marker(source);
        if source_object.local {
            let fresh = self.fresh(
                source_object
                    .fields
                    .borrow()
                    .iter()
                    .map(|(key, value)| {
                        (
                            String::from_utf16(key).expect("authored string keys"),
                            value.clone(),
                        )
                    })
                    .collect(),
                false,
            );
            self.marker(&fresh)
                .symbols
                .replace(source_object.symbols.borrow().clone());
            return Ok(fresh);
        }
        let own_keys = self.event("ownKeys");
        self.assert_value(source, &own_keys["object"], "copy ownKeys receiver");
        let keys = self.result(&own_keys)?;
        let native_keys = source_object
            .fields
            .borrow()
            .iter()
            .map(|(key, _)| AccountValue::String(key.clone()))
            .chain(
                source_object
                    .symbols
                    .borrow()
                    .iter()
                    .map(|(key, _)| AccountValue::Symbol(key.clone())),
            )
            .collect::<Vec<_>>();
        let observed_keys = self.native_values(&keys);
        assert_eq!(
            observed_keys.len(),
            native_keys.len(),
            "{} independent CopyDataProperties key count",
            self.0.label
        );
        let fresh = self.fresh(vec![], false);
        for (key, observed) in native_keys.into_iter().zip(observed_keys) {
            same_native(&key, &observed, "CopyDataProperties native key");
            let descriptor = self.event("descriptor");
            self.assert_value(source, &descriptor["object"], "copy descriptor receiver");
            match &key {
                AccountValue::String(key) => assert_eq!(*key, units(&descriptor["keyUnits"])),
                AccountValue::Symbol(_) => {
                    self.assert_value(&key, &descriptor["keyValue"], "copy symbol descriptor key")
                }
                _ => unreachable!(),
            };
            let descriptor = self.result(&descriptor)?;
            assert!(
                self.get(&descriptor, "enumerable")?.truthy(),
                "authored enumerable source field"
            );
            let value = self.get_key(source, key.clone())?;
            let own = self.marker(&fresh);
            match key {
                AccountValue::String(key) => own.fields.borrow_mut().push((key, value)),
                AccountValue::Symbol(key) => own.symbols.borrow_mut().push((key, value)),
                _ => unreachable!(),
            }
        }
        Ok(fresh)
    }
}
struct NativeIterator {
    cursor: Cursor,
    array: Rc<Object>,
    index: usize,
}
impl PlacementIterator for NativeIterator {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        let value = self.cursor.next_value()?;
        let fields = self.array.fields.borrow();
        match value {
            Some(value) => {
                let actual = fields
                    .get(self.index)
                    .expect("native dense iterator element")
                    .1
                    .clone();
                same_native(
                    &value,
                    &actual,
                    &format!("{} iterator{}", self.cursor.h.0.label, self.index),
                );
                self.index += 1;
                assert!(self.index <= 100, "native iterator floor stopped99");
                Ok(Some(actual))
            }
            None => {
                assert_eq!(
                    self.index,
                    fields.len(),
                    "independent native iterator exhaustion"
                );
                Ok(None)
            }
        }
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        self.cursor.close_throw(error)
    }
}
impl SinglePlacementHost for Fixture {
    fn binary(
        &self,
        op: PlacementBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        Ok(match (left, right) {
            (AccountValue::Number(a), AccountValue::Number(b)) => match op {
                PlacementBinary::Add => AccountValue::Number(a + b),
                PlacementBinary::Subtract => AccountValue::Number(a - b),
                PlacementBinary::Multiply => AccountValue::Number(a * b),
                PlacementBinary::Divide => AccountValue::Number(a / b),
                PlacementBinary::Remainder => AccountValue::Number(a % b),
                PlacementBinary::LessThan => AccountValue::Boolean(a < b),
                PlacementBinary::LessOrEqual => AccountValue::Boolean(a <= b),
                PlacementBinary::GreaterThan => AccountValue::Boolean(a > b),
            },
            (AccountValue::BigInt(a), AccountValue::BigInt(b)) => match op {
                PlacementBinary::Add => AccountValue::BigInt(a + b),
                PlacementBinary::Subtract => AccountValue::BigInt(a - b),
                PlacementBinary::Multiply => AccountValue::BigInt(a * b),
                PlacementBinary::Divide => AccountValue::BigInt(a / b),
                PlacementBinary::Remainder => AccountValue::BigInt(a % b),
                PlacementBinary::LessThan => AccountValue::Boolean(a < b),
                PlacementBinary::LessOrEqual => AccountValue::Boolean(a <= b),
                PlacementBinary::GreaterThan => AccountValue::Boolean(a > b),
            },
            _ => panic!("bounded primitive operator fixture; full coercion host required"),
        })
    }
    fn spread_object(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        self.copy_object(source)
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        let object = self.marker(object);
        let mut fields = object.fields.borrow_mut();
        let key = u(key);
        if let Some((_, previous)) = fields.iter_mut().find(|(name, _)| *name == key) {
            *previous = value
        } else {
            fields.push((key, value))
        }
        Ok(())
    }
    fn set_index(
        &self,
        object: &AccountValue,
        key: AccountValue,
        value: AccountValue,
    ) -> SigningResult<()> {
        let AccountValue::Number(index) = key else {
            panic!("bounded native numeric assignment key")
        };
        assert!(index.is_finite() && index >= 0.0 && index.fract() == 0.0);
        let array = self.marker(object);
        assert!(array.array);
        array
            .fields
            .borrow_mut()
            .get_mut(index as usize)
            .expect("existing native dense index")
            .1 = value;
        Ok(())
    }
    fn for_each(&self, value: AccountValue, visitor: PlacementVisitor) -> SigningResult<()> {
        let mut iterator = self.iterator(value)?;
        loop {
            let Some(value) = iterator.next()? else {
                return Ok(());
            };
            if let Err(error) = visitor(value) {
                return Err(iterator.close_throw(error));
            }
        }
    }
    fn iterator(&self, value: AccountValue) -> SigningResult<Box<dyn PlacementIterator>> {
        let array = self.marker(&value);
        assert!(array.array);
        Ok(Box::new(NativeIterator {
            cursor: Cursor::open(self.clone(), value)?,
            array,
            index: 0,
        }))
    }
    fn call_spread(
        &self,
        callee: Rc<dyn AccountAccessCall>,
        value: AccountValue,
    ) -> SigningResult<AccountValue> {
        if value.is_nullish() {
            let name = if matches!(value, AccountValue::Null) {
                "null"
            } else {
                "undefined"
            };
            let object = self.fresh(
                vec![
                    ("name".into(), AccountValue::String(u("TypeError"))),
                    (
                        "message".into(),
                        AccountValue::String(u(&format!(
                            "siblingOrderIds is not iterable (cannot read property {name})"
                        ))),
                    ),
                ],
                false,
            );
            let object = self.marker(&object);
            let error = Rc::new(Object {
                fields: RefCell::new(object.fields.borrow().clone()),
                symbols: RefCell::default(),
                regex: None,
                local: true,
                array: false,
                error: true,
            });
            let id = format!("$native{}", self.0.objects.borrow().len());
            self.0.objects.borrow_mut().insert(id, error.clone());
            return Err(self.throw_value(AccountValue::Object(error)));
        }
        let mut iterator = self.iterator(value)?;
        let mut args = vec![];
        loop {
            let Some(value) = iterator.next()? else { break };
            args.push(value)
        }
        callee.call(&args)
    }
}
#[test]
#[ignore = "live TypeScript placement oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_single_placement_native_floor() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/single-placement-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    let mut deferred_wire = vec![];
    for (i, row) in rows.iter().enumerate() {
        let h = Fixture(Rc::new(Env {
            label: format!("iteration{i} placeOrder"),
            wire: RefCell::default(),
            logs: RefCell::default(),
            sleeps: RefCell::default(),
            children: RefCell::default(),
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
        let promise = place_order(Rc::new(h.clone()), args[0].clone());
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
                    "{} native queued completion",
                    h.0.label
                )
            } else {
                h.completion(
                    observed.borrow().clone().expect("native settled"),
                    &step["output"],
                    "nativeRootOutcome",
                )
            }
        }
        let requests = h.0.creates.borrow();
        let expected = row["requests"].as_array().unwrap();
        assert_eq!(
            requests.len(),
            expected.len(),
            "{} native submitted count",
            h.0.label
        );
        for (actual, expected) in requests.iter().zip(expected) {
            h.assert_value(actual, expected, "nativeSubmittedOrder")
        }
        for (actual, expected, label) in [
            (&h.0.remembered, &row["remembered"], "nativeRememberedHint"),
            (&h.0.hints, &row["hints"], "nativeRetainedHint"),
        ] {
            let actual = actual.borrow();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{} {label} count", h.0.label);
            for ((key, value), expected) in actual.iter().zip(expected) {
                h.assert_value(key, &expected[0], label);
                h.assert_value(value, &expected[1], label)
            }
        }
        for (actual, expected, label) in [
            (&h.0.deleted, &row["deleted"], "nativeDeletedHint"),
            (&h.0.emitted, &row["emitted"], "nativeEmittedError"),
            (&h.0.sleeps, &row["sleeps"], "nativeSleep"),
            (&h.0.children, &row["children"], "nativeChildRequest"),
        ] {
            let actual = actual.borrow();
            let expected = expected.as_array().unwrap();
            assert_eq!(actual.len(), expected.len(), "{} {label} count", h.0.label);
            for (value, expected) in actual.iter().zip(expected) {
                h.assert_value(value, expected, label)
            }
        }
        assert_eq!(
            h.0.closed.get(),
            row["closed"].as_u64().unwrap() as usize,
            "{} nativeIteratorCloseCount",
            h.0.label
        );
        assert_eq!(
            h.0.throttles.get(),
            row["throttles"].as_u64().unwrap() as usize,
            "{} nativeThrottleCount",
            h.0.label
        );
        // All encoded/stringified log assertions wait until the entire native
        // corpus has completed; severity/count are native language values.
        let logs = h.0.logs.borrow();
        let expected = row["logs"].as_array().unwrap();
        assert_eq!(logs.len(), expected.len(), "native log count");
        for (actual, expected) in logs.iter().zip(expected) {
            assert_eq!(actual.len(), 2, "authored two log arguments");
            h.assert_value(&actual[1], &expected[1], "nativeLogSeverity")
        }
        let field = |request: &AccountValue, key: &str| {
            h.marker(request)
                .fields
                .borrow()
                .iter()
                .find(|(name, _)| *name == u(key))
                .unwrap()
                .1
                .clone()
        };
        if i == 0 {
            assert_eq!(requests.len(), 1, "authored one native primary request");
            assert_eq!(
                h.marker(&requests[0]).fields.borrow().len(),
                13,
                "authored native request own count13"
            );
            for (key, value) in [
                ("orderId", "123456"),
                ("price", "1235"),
                ("amount", "1250"),
                ("integratorAccountIndex", "17"),
            ] {
                let AccountValue::BigInt(actual) = field(&requests[0], key) else {
                    panic!("native request BigInt {key}")
                };
                assert_eq!(actual.to_string(), value, "authored native request {key}")
            }
            let AccountValue::Number(market) = field(&requests[0], "marketId") else {
                panic!("native market Number")
            };
            assert_eq!(market.to_bits(), 7_f64.to_bits());
            assert!(matches!(
                field(&requests[0], "postOnly"),
                AccountValue::Boolean(true)
            ));
            assert!(matches!(
                field(&requests[0], "triggerPrice"),
                AccountValue::Undefined
            ));
        }
        if let Some((_, amounts)) = [
            (8, vec![6667, 6667, 6667, 5000]),
            (9, vec![10000, 10000]),
            (10, vec![10000]),
            (11, vec![6667, 6667, 7667]),
            (12, vec![6667, 6667, 6667, 1000]),
        ]
        .into_iter()
        .find(|(index, _)| *index == i)
        {
            assert_eq!(
                requests.len(),
                amounts.len(),
                "authored independent lot count"
            );
            for (request, expected) in requests.iter().zip(amounts) {
                let AccountValue::BigInt(amount) = field(request, "amount") else {
                    panic!("native scaled lot BigInt")
                };
                assert_eq!(
                    amount.to_string(),
                    expected.to_string(),
                    "authored native scaled lot quantity"
                )
            }
        }
        if i == 1 || i == 2 {
            let AccountValue::BigInt(price) = field(&requests[0], "price") else {
                panic!("native scaled market BigInt")
            };
            assert_eq!(
                price.to_string(),
                if i == 1 { "10200" } else { "9800" },
                "authored native market slippage"
            )
        }
        if i == 4 {
            let AccountValue::String(tif) = field(&requests[0], "timeInForce") else {
                panic!("native conditional TIF")
            };
            assert_eq!(
                tif,
                u("ImmediateOrCancel"),
                "authored native conditional IOC"
            );
            assert!(matches!(
                field(&requests[0], "postOnly"),
                AccountValue::Boolean(false)
            ))
        }
        if i == 30 {
            assert!(
                h.0.deleted.borrow().is_empty(),
                "authored sleep rejection no cleanup"
            );
            assert_eq!(
                h.0.hints.borrow().len(),
                1,
                "authored sleep rejection retains successful hint"
            )
        }
        if i == 34 || i == 35 {
            assert_eq!(h.0.closed.get(), 1, "authored sibling failure closes once")
        }
        if i == 38 {
            assert_eq!(requests.len(), 4, "authored overflow four lots");
            for (request, id) in requests.iter().zip([
                "281474976710654",
                "281474976710655",
                "281474976710656",
                "281474976710657",
            ]) {
                let AccountValue::BigInt(actual) = field(request, "orderId") else {
                    panic!("native overflow BigInt")
                };
                assert_eq!(
                    actual.to_string(),
                    id,
                    "authored source per-lot int48 overflow preserved"
                )
            }
        }
        if i == 39 {
            let AccountValue::String(tif) = field(&requests[0], "timeInForce") else {
                panic!("native cached TIF")
            };
            assert_eq!(tif, u("GoodTillCancel"), "authored cached first TIF");
            assert!(
                matches!(field(&requests[0], "postOnly"), AccountValue::Boolean(true)),
                "authored second TIF read controls postOnly"
            )
        }
        if i == 14 || i == 15 || i == 16 {
            let children = h.0.children.borrow();
            assert_eq!(
                children.len(),
                if i == 16 { 2 } else { 1 },
                "authored native sibling count"
            );
            for child in children.iter() {
                assert_eq!(
                    h.marker(child).fields.borrow().len(),
                    7,
                    "authored native sibling own count7"
                );
                assert!(
                    matches!(field(child, "reduceOnly"), AccountValue::Boolean(true)),
                    "authored sibling reduceOnly"
                );
                let AccountValue::String(tif) = field(child, "timeInForce") else {
                    panic!("native sibling TIF")
                };
                assert_eq!(tif, u("ImmediateOrCancel"), "authored native sibling IOC")
            }
        }
        if i == 0 || i == 18 {
            assert_eq!(
                h.0.wire.borrow().len(),
                2,
                "authored two deferred wire checks"
            );
        }
        if i == 18 {
            let AccountValue::String(value) = field(&requests[0], "reduceOnly") else {
                panic!("native truthy UTF16 payload")
            };
            let mut expected = u("yes/");
            expected.push(0xd800);
            assert_eq!(
                value, expected,
                "authored native lone-surrogate value BEFORE any wire"
            );
        }
        deferred_wire.extend(h.0.wire.borrow().iter().cloned());
    }
    assert_eq!(rows.len(), 91, "independent authored placement floor91");
    eprintln!(
        "91 native cases completed; stopped at iteration90; beginning deferred wire comparisons"
    );
    // Every decoded native assertion in every case precedes any wire equality.
    for (actual, expected, label) in deferred_wire {
        let AccountValue::String(actual) = actual else {
            panic!("{label} native encoded UTF16 string")
        };
        assert_eq!(expected["kind"], "string", "{label} expected string");
        assert_eq!(
            actual,
            units(&expected["units"]),
            "{label} encoded wire after ALL native values"
        )
    }

    assert_eq!(
        corpus["contentHash"], "68a7bd2aa72d03563351ccdc81cbe59128b95f5c292ffc1954a7ee7532f08c84",
        "source/import pin after native values"
    );
    eprintln!(
        "91 native single placement cases; stopped at iteration90; bounded dense iterator, primitive operators and scalar JSON after actual native replacer"
    );
}
#[test]
#[ignore = "live TypeScript placement oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn single_placement_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/single-placement-source-control.cjs"
        ))
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(1),
        "sourceSinglePlacementMutationMustFailNamed"
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored base100 plus random56"),
        "sourceSinglePlacementNamedAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
