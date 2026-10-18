use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_market_reads::*,
    exchange_trading_preflight::{TradingCallback, TradingHost},
    sdk_account_api::{AccountObject, AccountValue},
    transaction_builders::TransactionBuildError,
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
#[derive(Default)]
struct Object {
    fields: RefCell<Vec<(String, AccountValue)>>,
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
        panic!("source clock is explicitly bound")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused")
    }
}
impl Runtime {
    fn drain(&self) {
        for _ in 0..100 {
            let next = self.queue.borrow_mut().pop_front();
            match next {
                Some(f) => f(),
                None => return,
            }
        }
        panic!("runtime floor exhausted99")
    }
}
struct Env {
    label: String,
    row: Value,
    objects: BTreeMap<String, Rc<Object>>,
    functions: BTreeMap<String, HostThrownValue>,
    token: HostThrownValue,
    symbol: HostThrownValue,
    runtime: Rc<Runtime>,
    events: RefCell<VecDeque<Value>>,
    constructs: RefCell<VecDeque<Value>>,
    callbacks: RefCell<BTreeMap<String, TradingCallback>>,
    callback_count: RefCell<usize>,
    pending: RefCell<BTreeMap<String, PromiseResolver<AccountValue>>>,
    promises: RefCell<BTreeMap<String, NativePromise<AccountValue>>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.0.objects[id].clone())
    }
    fn id(&self, v: &AccountValue) -> String {
        let AccountValue::Object(v) = v else {
            panic!("native object")
        };
        self.0
            .objects
            .iter()
            .find(|(_, o)| Rc::ptr_eq(v, &((*o).clone() as Rc<dyn AccountObject>)))
            .unwrap()
            .0
            .clone()
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
            "object" => self.obj(v["id"].as_str().unwrap()),
            "function" => {
                AccountValue::Function(self.0.functions[v["id"].as_str().unwrap()].clone())
            }
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            _ => panic!("native source kind"),
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
                let x =
                    f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
                if x.is_nan() {
                    assert!(n.is_nan(), "{l} nativeNaN")
                } else {
                    assert_eq!(n.to_bits(), x.to_bits(), "{l} nativeNumberBits")
                }
            }
            (AccountValue::BigInt(n), "bigint") => assert_eq!(
                *n,
                e["value"].as_str().unwrap().parse().unwrap(),
                "{l} nativeBigInt"
            ),
            (AccountValue::String(n), "string") => {
                assert_eq!(*n, units(&e["units"]), "{l} nativeUtf16")
            }
            (AccountValue::Object(n), "object") => assert!(
                Rc::ptr_eq(
                    n,
                    &(self.0.objects[e["id"].as_str().unwrap()].clone() as Rc<dyn AccountObject>)
                ),
                "{l} nativeObjectIdentity"
            ),
            (AccountValue::Function(n), "function") => assert!(
                n.ptr_eq(&self.0.functions[e["id"].as_str().unwrap()]),
                "{l} nativeFunctionIdentity"
            ),
            (AccountValue::Symbol(n), "symbol") => {
                assert!(n.ptr_eq(&self.0.symbol), "{l} nativeSymbolIdentity")
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
    fn error(&self, e: &Value) -> TransactionSigningError {
        match e["kind"].as_str().unwrap() {
            "thrown" => TransactionSigningError::Thrown(self.0.token.clone()),
            "generated" => TransactionSigningError::Generated(TransactionBuildError {
                name: match e["name"].as_str().unwrap() {
                    "Error" => "Error",
                    "TypeError" => "TypeError",
                    _ => panic!("unproved source error"),
                },
                message: units(&e["message"]),
            }),
            _ => panic!("source failure kind"),
        }
    }
    fn result(&self, e: &Value) -> SigningResult<AccountValue> {
        if !e["error"].is_null() {
            Err(self.error(&e["error"]))
        } else {
            Ok(self.decode(&e["result"]))
        }
    }
    fn completion(&self, n: SigningResult<AccountValue>, e: &Value, label: &str) {
        match (n, e["kind"].as_str().unwrap()) {
            (Ok(n), "ok") => self.assert_deep(&n, &e["value"], label),
            (Err(TransactionSigningError::Thrown(n)), "thrown") => assert!(
                n.ptr_eq(&self.0.token),
                "{} {label} nativeThrownIdentity",
                self.0.label
            ),
            (Err(TransactionSigningError::Generated(n)), "generated") => {
                assert_eq!(
                    n.name,
                    e["name"].as_str().unwrap(),
                    "{} {label} nativeErrorName",
                    self.0.label
                );
                assert_eq!(
                    n.message,
                    units(&e["message"]),
                    "{} {label} nativeErrorUtf16",
                    self.0.label
                )
            }
            _ => panic!("{} {label} nativeCompletionMismatch", self.0.label),
        }
    }
    fn end_step(&self) {
        assert_eq!(
            self.0.events.borrow().len(),
            0,
            "{} independentSourceEventCount",
            self.0.label
        );
        assert_eq!(
            self.0.constructs.borrow().len(),
            0,
            "{} independentSourceConstructionCount",
            self.0.label
        )
    }
    fn step(&self, s: &Value) {
        self.end_step();
        self.0
            .events
            .replace(s["events"].as_array().unwrap().iter().cloned().collect());
        self.0.constructs.replace(
            s["constructs"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect(),
        );
    }
    fn construct(
        &self,
        op: &str,
        fields: &[(String, AccountValue)],
    ) -> SigningResult<AccountValue> {
        let e = self
            .0
            .constructs
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("{} unexpected native construction {op}", self.0.label));
        assert_eq!(e["op"], op, "{} nativeConstructionOrder", self.0.label);
        let id = e["id"].as_str().unwrap();
        let m = self.0.row["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap();
        let expected = m["fields"].as_array().unwrap();
        assert_eq!(
            fields.len(),
            expected.len(),
            "{} independentSourceOwnFieldCount",
            self.0.label
        );
        for ((k, v), f) in fields.iter().zip(expected) {
            assert_eq!(
                k,
                f["key"].as_str().unwrap(),
                "{} nativeOwnKeySchema",
                self.0.label
            );
            self.assert_value(v, &f["value"], &format!("{op}:{k}"));
        }
        self.0.objects[id].fields.replace(fields.to_vec());
        Ok(self.obj(id))
    }
    fn callback(&self, e: &Value) {
        let id = e["id"].as_str().unwrap();
        let cb = self.0.callbacks.borrow()[id].clone();
        let args: Vec<_> = e["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| self.decode(a))
            .collect();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| cb(&args)));
        assert!(
            result.is_ok(),
            "{} nativeCallbackMustReturnJsCompletion",
            self.0.label
        );
        let result = result.unwrap();
        let expected = self.event("invoke-result");
        self.completion(result, &expected["result"], "callbackResult");
    }
    fn native_call(
        &self,
        name: &str,
        receiver: &AccountValue,
        args: &[AccountValue],
        op: &str,
    ) -> SigningResult<AccountValue> {
        let e = self.event(op);
        assert_eq!(e["name"], name, "{} nativeCalleeIdentity", self.0.label);
        self.assert_value(receiver, &e["receiver"], "callReceiver");
        let a = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            a.len(),
            "{} independentSourceArgumentCount",
            self.0.label
        );
        for (i, (v, x)) in args.iter().zip(a).enumerate() {
            self.assert_value(v, x, &format!("{name} arg{i}"));
        }
        loop {
            let peek = self.0.events.borrow().front().cloned().unwrap();
            if peek["op"] == "end-call" && peek["depth"] == e["depth"] {
                self.event("end-call");
                break;
            }
            if peek["op"] == "invoke" {
                let invoked = self.event("invoke");
                self.callback(&invoked);
                continue;
            }
            // Only actual delegated engine/import work inside this captured Call may be
            // consumed here. Callback bodies are always executed as native closures.
            assert!(
                peek["depth"].as_u64().unwrap() > e["depth"].as_u64().unwrap(),
                "{} hostInternalDepth",
                self.0.label
            );
            assert_ne!(
                peek["op"], "invoke-result",
                "{} native callback cannot be skipped",
                self.0.label
            );
            self.0.events.borrow_mut().pop_front();
        }
        let value = self.result(&e)?;
        if let AccountValue::Object(_) = &value {
            let id = self.id(&value);
            if id.starts_with("P.") {
                if name == "Promise.finally" {
                    let (p, r) = NativePromise::pending(self.runtime());
                    self.0.promises.borrow_mut().insert(id.clone(), p);
                    let original = self.0.promises.borrow()[&self.id(receiver)].clone();
                    let owner = self.clone();
                    original.subscribe(move |result| {
                        let invoke = owner.event("invoke");
                        let cb = owner.0.callbacks.borrow()[invoke["id"].as_str().unwrap()].clone();
                        assert_eq!(
                            invoke["args"].as_array().unwrap().len(),
                            0,
                            "authored finally has zero arguments"
                        );
                        let completed = cb(&[]);
                        let e = owner.event("invoke-result");
                        owner.completion(completed.clone(), &e["result"], "finallyCallbackResult");
                        r.settle(match completed {
                            Err(e) => Err(e),
                            Ok(AccountValue::Undefined) => result.map(|v| (*v).clone()),
                            _ => panic!("authored finally completion"),
                        });
                    });
                } else {
                    self.bind_provider(&id);
                }
            }
        }
        Ok(value)
    }
    fn bind_provider(&self, id: &str) {
        if self.0.promises.borrow().contains_key(id) {
            return;
        }
        let (p, r) = NativePromise::pending(self.runtime());
        self.0.promises.borrow_mut().insert(id.into(), p);
        assert!(
            self.0
                .pending
                .borrow_mut()
                .insert(id.strip_prefix("P.").unwrap().into(), r)
                .is_none()
        );
    }
    fn provider(&self, name: &str) -> AccountValue {
        match name {
            "snapshot" => self.obj("Balance"),
            "limits" => {
                if self.0.row["input"]["noAccountLimits"] == true {
                    AccountValue::Undefined
                } else {
                    self.obj("AccountLimits")
                }
            }
            "markets" | "rows" => self.obj("Rows"),
            "details" => self.obj("Details"),
            "fallback" => self.obj("Fallback"),
            "orders" | "cached" => self.obj("Orders"),
            _ => panic!("authored provider"),
        }
    }
    fn assert_deep(&self, value: &AccountValue, expected: &Value, label: &str) {
        self.assert_value(value, expected, label);
        if matches!(value, AccountValue::Object(_)) {
            let id = self.id(value);
            if id.starts_with("Literal") {
                let meta = self.0.row["objects"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|o| o["id"] == id)
                    .unwrap();
                let fields = self.0.objects[&id].fields.borrow().clone();
                let e = meta["fields"].as_array().unwrap();
                assert_eq!(
                    fields.len(),
                    e.len(),
                    "{} {label} independentNativeOwnFieldCount",
                    self.0.label
                );
                for ((key, v), field) in fields.iter().zip(e) {
                    assert_eq!(
                        key,
                        field["key"].as_str().unwrap(),
                        "{} {label} nativeDecodedOwnKey",
                        self.0.label
                    );
                    self.assert_deep(v, &field["value"], &format!("{label}.{key}"));
                }
            }
        }
    }
}
struct Call {
    h: Fixture,
    name: String,
    receiver: AccountValue,
}
impl AccountAccessCall for Call {
    fn call(&self, a: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h.native_call(&self.name, &self.receiver, a, "call")
    }
    fn construct(&self, a: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h
            .native_call(&self.name, &self.receiver, a, "construct")
    }
}
impl ExchangeAccountAccessHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, o: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        if o.is_nullish() {
            return Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u(&format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if matches!(o, AccountValue::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
            }));
        }

        if matches!(o, AccountValue::Object(_)) {
            let id = self.id(o);
            if id.starts_with("Literal") || id == "DefaultOpts" || self.0.objects[&id].error {
                return self.0.objects[&id].get(key);
            }
            if self.0.objects[&id].array && key == "length" {
                return Ok(AccountValue::Number(
                    self.0.objects[&id].fields.borrow().len() as f64,
                ));
            }
        }
        let e = self.event("get");
        self.assert_value(o, &e["object"], "getReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativePropertyKey",
            self.0.label
        );
        self.result(&e)
    }
    fn set(&self, o: &AccountValue, key: &str, v: AccountValue) -> SigningResult<()> {
        let e = self.event("set");
        self.assert_value(o, &e["object"], "setReceiver");
        assert_eq!(u(key), units(&e["keyUnits"]));
        self.assert_value(&v, &e["value"], "setValue");
        self.result(&e)?;
        Ok(())
    }
    fn member(&self, o: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.event("member");
        self.assert_value(o, &e["object"], "memberReceiver");
        assert_eq!(
            u(key),
            units(&e["keyUnits"]),
            "{} nativeMemberKey",
            self.0.label
        );
        self.result(&e)?;
        Ok(Rc::new(Call {
            h: self.clone(),
            name: e["name"].as_str().unwrap().into(),
            receiver: o.clone(),
        }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        if name.starts_with("Number.") {
            let lookup = self.event("global");
            assert_eq!(
                lookup["name"], "Number",
                "{} nativeNumberNamespaceGet",
                self.0.label
            );
            self.result(&lookup)?;
        }
        let e = self.event("global");
        assert_eq!(e["name"], name, "{} nativeImportBinding", self.0.label);
        self.result(&e)?;
        let receiver = if e["receiver"].is_null() {
            AccountValue::Undefined
        } else {
            self.decode(&e["receiver"])
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            name: name.into(),
            receiver,
        }))
    }
    fn literal(&self, f: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        if f.is_empty()
            && self.0.row["method"] == "fetchRawBalance"
            && self.0.row["input"]["opts"] == "undefined"
        {
            assert_eq!(f.len(), 0, "authored empty default options");
            return Ok(self.obj("DefaultOpts"));
        }

        self.construct(
            "literal",
            &f.iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect::<Vec<_>>(),
        )
    }
    fn throw_value(&self, v: AccountValue) -> TransactionSigningError {
        let id = self.id(&v);
        let o = &self.0.objects[&id];
        assert!(o.error, "native constructed Error");
        let fields = o.fields.borrow();
        let name = fields.iter().find(|(k, _)| k == "name").unwrap();
        let message = fields.iter().find(|(k, _)| k == "message").unwrap();
        let AccountValue::String(name) = &name.1 else {
            panic!("native error name")
        };
        let AccountValue::String(message) = &message.1 else {
            panic!("native error message")
        };
        assert_eq!(*name, u("Error"), "native Error constructor name");
        TransactionSigningError::Generated(TransactionBuildError {
            name: "Error",
            message: message.clone(),
        })
    }
    fn resolve_return(&self, v: AccountValue, r: PromiseResolver<AccountValue>) {
        if matches!(v, AccountValue::Object(_)) {
            let id = self.id(&v);
            if id.starts_with("P.") {
                self.bind_provider(&id);
                let p = self.0.promises.borrow()[&id].clone();
                p.subscribe(move |v| r.settle(v.map(|v| (*v).clone())));
                return;
            }
        }
        if self.0.row["input"]["returnThen"].is_string() {
            let e = self.event("return-then-get");
            self.assert_deep(&v, &e["object"], "nativeReturnedValue");
            match self.result(&e) {
                Err(e) => r.settle(Err(e)),
                Ok(AccountValue::Function(_)) => {
                    let owner = self.clone();
                    self.0.runtime.enqueue_microtask(Box::new(move || {
                        let e = owner.event("return-then-call");
                        owner.assert_deep(&v, &e["object"], "nativeThenReceiver");
                        if owner.0.row["input"]["returnThen"] == "resolve" {
                            r.settle(Ok(AccountValue::String(
                                u("adopted/").into_iter().chain([0xd800]).collect(),
                            )))
                        } else {
                            r.settle(Err(TransactionSigningError::Thrown(owner.0.token.clone())))
                        }
                    }));
                }
                _ => panic!("authored callable then"),
            };
            return;
        }
        r.settle(Ok(v))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn await_value(&self, v: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        let id = self.id(&v);
        assert!(
            id.starts_with("P."),
            "source real Promise floor; full engine required"
        );
        self.bind_provider(&id);
        Ok(self.0.promises.borrow()[&id].clone())
    }
}
impl TradingHost for Fixture {
    fn function(&self, callback: TradingCallback) -> SigningResult<AccountValue> {
        let mut n = self.0.callback_count.borrow_mut();
        let id = format!("Callback{n}");
        *n += 1;
        assert!(
            self.0
                .callbacks
                .borrow_mut()
                .insert(id.clone(), callback)
                .is_none()
        );
        Ok(AccountValue::Function(self.0.functions[&id].clone()))
    }
    fn array(&self, v: &[AccountValue]) -> SigningResult<AccountValue> {
        self.construct(
            "array",
            &v.iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect::<Vec<_>>(),
        )
    }
    fn index(&self, o: &AccountValue, key: AccountValue) -> SigningResult<AccountValue> {
        let key = self.template_string(key)?;
        let e = self.event("get");
        self.assert_value(o, &e["object"], "indexReceiver");
        assert_eq!(
            key,
            units(&e["keyUnits"]),
            "{} nativeIndexUtf16",
            self.0.label
        );
        self.result(&e)
    }
    fn constant(&self, name: &str) -> SigningResult<AccountValue> {
        if name.starts_with("Number.") {
            let e = self.event("global");
            assert_eq!(e["name"], "Number");
            self.result(&e)?;
        }
        let e = self.event("constant");
        assert_eq!(e["name"], name, "{} nativeConstantBinding", self.0.label);
        self.result(&e)
    }
    fn error_value(&self, e: TransactionSigningError) -> AccountValue {
        match e {
            TransactionSigningError::Thrown(t) => {
                assert!(t.ptr_eq(&self.0.token));
                self.obj("Thrown")
            }
            TransactionSigningError::Generated(e) => {
                let id = self
                    .0
                    .objects
                    .iter()
                    .find(|(_, o)| {
                        o.error
                            && o.fields.borrow().iter().any(|(k, v)| {
                                k == "message"
                                    && matches!(v,AccountValue::String(s)if *s==e.message)
                            })
                    })
                    .unwrap()
                    .0;
                self.obj(id)
            }
        }
    }
    fn template_string(&self, v: AccountValue) -> SigningResult<Vec<u16>> {
        match v {
            AccountValue::String(s) => Ok(s),
            AccountValue::Number(n) => Ok(u(&safe_cex_num::js_number_string(n))),
            AccountValue::BigInt(n) => Ok(u(&n.to_string())),
            AccountValue::Boolean(n) => Ok(u(if n { "true" } else { "false" })),
            AccountValue::Null => Ok(u("null")),
            AccountValue::Undefined => Ok(u("undefined")),
            AccountValue::Symbol(_) => {
                Err(TransactionSigningError::Generated(TransactionBuildError {
                    name: "TypeError",
                    message: u("Cannot convert a Symbol value to a string"),
                }))
            }
            _ => panic!("full object template coercion is required host, unproved in fixture"),
        }
    }
}
fn fixture(row: &Value, i: usize) -> Fixture {
    let mut objects: BTreeMap<String, Rc<Object>> = row["objects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["id"].as_str().unwrap().into(),
                Rc::new(Object {
                    array: o["array"].as_bool().unwrap(),
                    error: o["error"].as_bool().unwrap(),
                    ..Default::default()
                }),
            )
        })
        .collect();
    objects.insert("DefaultOpts".into(), Rc::default());
    let mut names = std::collections::BTreeSet::new();
    fn scan(v: &Value, n: &mut std::collections::BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if v["kind"] == "function" {
                    n.insert(v["id"].as_str().unwrap().into());
                }
                for v in o.values() {
                    scan(v, n)
                }
            }
            Value::Array(a) => {
                for v in a {
                    scan(v, n)
                }
            }
            _ => {}
        }
    }
    scan(row, &mut names);
    let h = Fixture(Rc::new(Env {
        label: format!("iteration{i} {}", row["method"].as_str().unwrap()),
        row: row.clone(),
        objects,
        functions: names
            .into_iter()
            .map(|n| (n, HostThrownValue::new(())))
            .collect(),
        token: HostThrownValue::new(()),
        symbol: HostThrownValue::new(()),
        runtime: Rc::default(),
        events: RefCell::default(),
        constructs: RefCell::default(),
        callbacks: RefCell::default(),
        callback_count: RefCell::new(0),
        pending: RefCell::default(),
        promises: RefCell::default(),
    }));
    for o in row["objects"].as_array().unwrap() {
        h.0.objects[o["id"].as_str().unwrap()].fields.replace(
            o["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (f["key"].as_str().unwrap().into(), h.decode(&f["value"])))
                .collect(),
        );
    }
    if row["input"]["cached"] == true {
        h.bind_provider("P.cached");
    }
    h
}
impl MarketReadHost for Fixture {
    fn add(&self, left: AccountValue, right: AccountValue) -> SigningResult<AccountValue> {
        let AccountValue::Number(right) = right else {
            panic!("authored numeric delta");
        };
        Ok(match left {
            AccountValue::Number(left) => AccountValue::Number(left + right),
            AccountValue::Undefined => AccountValue::Number(f64::NAN),
            AccountValue::String(mut left) => {
                assert_eq!(
                    right, 20.0,
                    "authored string addition operand; full JS host required"
                );
                left.extend(u("20"));
                AccountValue::String(left)
            }
            _ => panic!("authored accumulator domain; full JS addition required"),
        })
    }
    fn number_power(&self, base: f64, exponent: f64) -> f64 {
        assert_eq!(base, 10.0, "authored source power base");
        assert!(
            exponent == 2.0 || exponent == 3.0,
            "bounded integer exponent fixture; full Number::exponentiate required"
        );
        base.powf(exponent)
    }
    fn function_value(&self, name: &str) -> SigningResult<AccountValue> {
        let e = self.event("global");
        assert_eq!(e["name"], name, "{} nativeHelperBinding", self.0.label);
        self.result(&e)
    }
    fn for_of(&self, array: AccountValue, body: ForOfBody) -> SigningResult<Option<AccountValue>> {
        let iterator = self.member(&array, "@@iterator")?.call(&[])?;
        let next = self.member(&iterator, "next")?;
        for _ in 0..10 {
            let result = next.call(&[])?;
            if self.get(&result, "done")?.truthy() {
                return Ok(None);
            }
            let value = self.get(&result, "value")?;
            match body(value) {
                Ok(ForOfStep::Continue) => {}
                Ok(ForOfStep::Break(v)) => {
                    assert!(
                        matches!(self.get(&iterator, "return")?, AccountValue::Undefined),
                        "authored iterator return absent; full IteratorClose required"
                    );
                    return Ok(Some(v));
                }
                Err(e) => {
                    let _ = self.get(&iterator, "return");
                    return Err(e);
                }
            }
        }
        panic!("for-of floor exhausted9")
    }
    fn spread_override(
        &self,
        source: AccountValue,
        overrides: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        let keys = self.event("own-keys");
        self.assert_value(&source, &keys["object"], "spreadReceiver");
        let keys = self.result(&keys)?;
        let id = self.id(&keys);
        let keys = self.0.objects[&id].fields.borrow().clone();
        let mut fields: Vec<(String, AccountValue)> = Vec::new();
        for (_, key) in keys {
            let descriptor = self.event("descriptor");
            self.assert_value(&source, &descriptor["object"], "descriptorReceiver");
            let AccountValue::String(units) = key.clone() else {
                panic!("ASCII spread key fixture; Symbol keys required host")
            };
            assert_eq!(
                units,
                units_from_descriptor(&descriptor),
                "{} nativeDescriptorKey",
                self.0.label
            );
            let descriptor = self.result(&descriptor)?;
            let id = self.id(&descriptor);
            let enumerable = self.0.objects[&id].get("enumerable")?;
            if enumerable.truthy() {
                assert!(
                    units.iter().all(|u| *u < 128),
                    "ASCII spread key floor only"
                );
                let key_string = String::from_utf16(&units).unwrap();
                let value = self.index(&source, key)?;
                fields.push((key_string, value));
            }
        }
        for (key, value) in overrides {
            if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
                *v = value.clone()
            } else {
                fields.push(((*key).into(), value.clone()))
            }
        }
        self.construct("spread", &fields)
    }
}
fn units_from_descriptor(d: &Value) -> Vec<u16> {
    units(&d["keyUnits"])
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_market_reads_native_floor() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/market-reads-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
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
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let h = fixture(row, i);
        let steps = row["steps"].as_array().unwrap();
        h.step(&steps[0]);
        let args = row["args"].as_array().unwrap();
        let a = |i: usize| h.decode(&args[i]);
        let n = |i: usize| {
            let AccountValue::Number(n) = a(i) else {
                panic!("source typed Number")
            };
            n
        };
        let observed = Rc::new(RefCell::new(None));
        let live: Rc<dyn MarketReadHost> = Rc::new(h.clone());
        if matches!(
            row["method"].as_str().unwrap(),
            "resolvePositionLeverage" | "resolvePositionLiquidationPrice" | "mapPosition"
        ) {
            let result = match row["method"].as_str().unwrap() {
                "resolvePositionLeverage" => resolve_position_leverage(live, a(0), a(1)),
                "resolvePositionLiquidationPrice" => resolve_position_liquidation_price(
                    live.as_ref(),
                    a(0),
                    a(1),
                    a(2),
                    n(3),
                    n(4),
                    n(5),
                    a(6),
                ),
                "mapPosition" => map_position(live.as_ref(), a(0)),
                _ => unreachable!(),
            };
            h.completion(result, &steps[0]["output"], "nativeResult");
        } else {
            let promise = match row["method"].as_str().unwrap() {
                "fetchRawBalance" => fetch_raw_balance(live, a(0)),
                "fetchMarkets" => fetch_markets(live),
                "fetchLighterMarketRows" => fetch_lighter_market_rows(live),
                "fetchMarketsHelper" => fetch_markets_helper(live),
                "fetchOrders" => fetch_orders(live),
                _ => panic!("authored method"),
            };
            let seen = observed.clone();
            promise.subscribe(move |v| *seen.borrow_mut() = Some(v.map(|v| (*v).clone())));
            h.0.runtime.drain();
            if !steps[0]["output"].is_null() {
                h.completion(
                    observed
                        .borrow_mut()
                        .take()
                        .expect("eager source completion"),
                    &steps[0]["output"],
                    "nativeResult",
                )
            } else {
                assert!(
                    observed.borrow().is_none(),
                    "{} nativeDeferredCompletion",
                    h.0.label
                )
            }
        }
        h.end_step();
        for s in steps.iter().skip(1) {
            h.step(s);
            let name = s["phase"].as_str().unwrap();
            let r = h.0.pending.borrow_mut().remove(name).unwrap();
            let result = if row["input"]["reject"] == name {
                Err(TransactionSigningError::Thrown(h.0.token.clone()))
            } else {
                let v = h.provider(name);
                while h.0.events.borrow().front().is_some_and(|e| {
                    e["op"] == "get" && e["keyUnits"] == serde_json::json!(u("then"))
                }) {
                    h.get(&v, "then").unwrap();
                }
                Ok(v)
            };
            r.settle(result);
            h.0.runtime.drain();
            if !s["output"].is_null() {
                h.completion(
                    observed
                        .borrow_mut()
                        .take()
                        .expect("source resolved completion"),
                    &s["output"],
                    "nativeResult",
                )
            } else {
                assert!(
                    observed.borrow().is_none(),
                    "{} nativeDeferredCompletion",
                    h.0.label
                )
            }
            h.end_step();
        }
        let native_pending: Vec<_> = h.0.pending.borrow().keys().cloned().collect();
        let mut source_pending: Vec<_> = row["pendingProviders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect();
        source_pending.sort();
        assert_eq!(
            native_pending, source_pending,
            "{} independentSourcePendingProviders",
            h.0.label
        );
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 161, "independent authored total floor");
    for (name, n) in [
        ("resolvePositionLeverage", 22),
        ("resolvePositionLiquidationPrice", 25),
        ("mapPosition", 22),
        ("fetchRawBalance", 33),
        ("fetchMarkets", 9),
        ("fetchLighterMarketRows", 15),
        ("fetchMarketsHelper", 23),
        ("fetchOrders", 12),
    ] {
        assert_eq!(counts[name], n, "independent authored {name} floor");
    }
    // All native decoded fields/values/closures/identities precede source-only pin.
    assert_eq!(
        corpus["contentHash"], "35db19e5e1a26cd7955ff12661e0fcdb2ba51078f88f75a5a49ad1da20ba428a",
        "source-only pin AFTER native assertions"
    );
    println!("161 native cases; stopped at iteration160; no exhaustive/runtime/engine claim");
}

#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn market_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/market-reads-source-control.cjs"
        ))
        .output()
        .expect("node source mutation control");
    assert_eq!(out.status.code(), Some(1), "sourceMutationMustFailNamed");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("authored balance980"),
        "sourceMutationMustReachBalanceAssertion: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
