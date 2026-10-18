use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_order_mapping::*,
    exchange_trading_preflight::*,
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
    fields: RefCell<Vec<(Vec<u16>, AccountValue)>>,
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
        panic!("source clock is explicitly bound")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused")
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
    regex: RefCell<BTreeMap<String, (String, String)>>,
    closed: RefCell<usize>,
    gets: RefCell<BTreeMap<String, usize>>,
    string_version: RefCell<usize>,
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
            (Ok(n), "ok") => self.assert_value(&n, &e["value"], label),
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
        let e = self.0.constructs.borrow_mut().pop_front().unwrap();
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
                u(k),
                units(&f["keyUnits"]),
                "{} nativeOwnKeySchema",
                self.0.label
            );
            self.assert_value(v, &f["value"], &format!("{op}:{k}"));
        }
        self.0.objects[id]
            .fields
            .replace(fields.iter().map(|(k, v)| (u(k), v.clone())).collect());
        Ok(self.obj(id))
    }
    fn native_call(
        &self,
        name: &str,
        receiver: &AccountValue,
        args: &[AccountValue],
        op: &str,
        callee: &HostThrownValue,
        string_version: usize,
    ) -> SigningResult<AccountValue> {
        let e = self.event(op);
        self.assert_value(
            &AccountValue::Function(callee.clone()),
            &e["callee"],
            "actual captured native callee",
        );
        assert_eq!(e["name"], name, "{} nativeCalleeIdentity", self.0.label);
        self.assert_value(receiver, &e["receiver"], "callReceiver");
        let expected = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "{} independentSourceArgumentCount",
            self.0.label
        );
        for (i, (a, b)) in args.iter().zip(expected).enumerate() {
            self.assert_value(a, b, &format!("{name} arg{i}"));
        }
        let entries = if name == "Object.entries" && e["error"].is_null() {
            let mut fields = Vec::new();
            for (key, _) in self.own(&args[0]) {
                let value = self.get_units(&args[0], &key)?;
                fields.push((key, value))
            }
            Some(fields)
        } else {
            None
        };
        let mut found = None;
        let mut callback_index = 0;
        loop {
            let peek = self
                .0
                .events
                .borrow()
                .front()
                .cloned()
                .expect("captured call must end");
            if peek["op"] == "end-call" && peek["depth"] == e["depth"] {
                self.event("end-call");
                break;
            }
            if peek["op"] == "invoke" {
                let called = self.event("invoke");
                assert_eq!(name, "Array.find", "actual only mapOrder callback");
                let object = self.0.objects[&self.id(receiver)].clone();
                let value = object.fields.borrow()[callback_index].1.clone();
                let native_args = [
                    value.clone(),
                    AccountValue::Number(callback_index as f64),
                    receiver.clone(),
                ];
                for (a, b) in native_args.iter().zip(called["args"].as_array().unwrap()) {
                    self.assert_value(a, b, "find actual native callback input");
                }
                callback_index += 1;
                let callback = self.0.callbacks.borrow()[called["id"].as_str().unwrap()].clone();
                let result = callback(&native_args);
                let expected = self.event("invoke-result");
                if let Ok(v) = &result {
                    if v.truthy() {
                        found = Some(value)
                    }
                }
                self.completion(result, &expected["result"], "callbackResult");
                continue;
            }
            assert!(
                peek["depth"].as_u64().unwrap() > e["depth"].as_u64().unwrap(),
                "{} hostInternalDepth",
                self.0.label
            );
            assert_ne!(
                peek["op"], "invoke-result",
                "native callback cannot be skipped"
            );
            self.0.events.borrow_mut().pop_front();
        }
        if !e["error"].is_null() {
            return Err(self.error(&e["error"]));
        }
        let value = if let Some(entries) = entries {
            self.entries_result(&e["result"], entries)
        } else if name == "Array.find" {
            found.unwrap_or(AccountValue::Undefined)
        } else {
            self.builtin(name, receiver, args, &e["result"], string_version)?
        };
        self.assert_value(&value, &e["result"], &format!("{name} actualNativeReturn"));
        Ok(value)
    }
}
struct Call {
    h: Fixture,
    name: String,
    receiver: AccountValue,
    callee: HostThrownValue,
    string_version: usize,
}
impl AccountAccessCall for Call {
    fn call(&self, a: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h.native_call(
            &self.name,
            &self.receiver,
            a,
            "call",
            &self.callee,
            self.string_version,
        )
    }
    fn construct(&self, a: &[AccountValue]) -> SigningResult<AccountValue> {
        self.h.native_call(
            &self.name,
            &self.receiver,
            a,
            "construct",
            &self.callee,
            self.string_version,
        )
    }
}
impl ExchangeAccountAccessHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, o: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        if matches!(o, AccountValue::Object(_)) {
            let id = self.id(o);
            if self.0.objects[&id].array && key == "length" {
                return Ok(AccountValue::Number(
                    self.0.objects[&id].fields.borrow().len() as f64,
                ));
            }
        }
        self.get_units(o, &u(key))
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
            callee: self.0.functions[e["result"]["id"].as_str().unwrap()].clone(),
            string_version: *self.0.string_version.borrow(),
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
            callee: self.0.functions[e["result"]["id"].as_str().unwrap()].clone(),
            string_version: *self.0.string_version.borrow(),
        }))
    }
    fn literal(&self, f: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
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
        let name = fields.iter().find(|(k, _)| *k == u("name")).unwrap();
        let message = fields.iter().find(|(k, _)| *k == u("message")).unwrap();
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
        assert!(
            !matches!(v, AccountValue::Object(_) | AccountValue::Function(_)),
            "authored returned primitives only; full engine host remains required"
        );
        r.settle(Ok(v))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn await_value(&self, v: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        let id = self.id(&v);
        let name = id
            .strip_prefix("P.")
            .expect("actual source provider promise");
        let (p, r) = NativePromise::pending(self.runtime());
        assert!(
            self.0.pending.borrow_mut().insert(name.into(), r).is_none(),
            "one await per provider"
        );
        Ok(p)
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
        let key = self.native_string(key)?;
        self.get_units(o, &key)
    }
    fn constant(&self, name: &str) -> SigningResult<AccountValue> {
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
            _ => panic!("authored catches use original injected throws"),
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
    let objects = row["objects"]
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
        regex: RefCell::default(),
        closed: RefCell::new(0),
        gets: RefCell::default(),
        string_version: RefCell::new(0),
    }));
    for o in row["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| !o["id"].as_str().unwrap().starts_with("Fresh"))
    {
        h.0.objects[o["id"].as_str().unwrap()].fields.replace(
            o["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (units(&f["keyUnits"]), h.decode(&f["value"])))
                .collect(),
        );
    }
    h
}
fn whitespace(v: u16) -> bool {
    matches!(v,0x9..=0xD|0x20|0xA0|0x1680|0x2000..=0x200A|0x2028|0x2029|0x202F|0x205F|0x3000|0xFEFF)
}
impl Fixture {
    fn get_units(&self, o: &AccountValue, key: &[u16]) -> SigningResult<AccountValue> {
        let e = self.event("get");
        self.assert_value(o, &e["object"], "getReceiver");
        assert_eq!(
            key,
            units(&e["keyUnits"]),
            "{} nativePropertyKeyUTF16",
            self.0.label
        );
        if !e["error"].is_null() {
            return Err(self.error(&e["error"]));
        }
        let id = self.id(o);
        let label = format!("{id}:{key:?}");
        let count = {
            let mut gets = self.0.gets.borrow_mut();
            let n = gets.entry(label).or_default();
            let old = *n;
            *n += 1;
            old
        };
        let value = if id == "Input"
            && key == u("order")
            && !self.0.row["input"]["changingOrder"].is_null()
        {
            if count == 0 {
                self.obj("Nested")
            } else if count == 1 && self.0.row["input"]["changingOrder"] == "notobject" {
                AccountValue::Number(7.0)
            } else {
                self.obj("Nested2")
            }
        } else if id == "Hint"
            && key == u("type")
            && !self.0.row["input"]["hintTypeSeries"].is_null()
        {
            let series = self.0.row["input"]["hintTypeSeries"].as_array().unwrap();
            AccountValue::String(u(series[count.min(series.len() - 1)].as_str().unwrap()))
        } else {
            self.own(o)
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.clone())
                .unwrap_or(AccountValue::Undefined)
        };
        self.assert_value(
            &value,
            &e["result"],
            &format!("get {} {:?} actualNativeValue", self.id(o), key),
        );
        Ok(value)
    }
    fn native_string(&self, v: AccountValue) -> SigningResult<Vec<u16>> {
        Ok(match v {
            AccountValue::String(s) => s,
            AccountValue::Number(n) => u(&safe_cex_num::js_number_string(n)),
            AccountValue::BigInt(n) => u(&n.to_string()),
            AccountValue::Boolean(b) => u(if b { "true" } else { "false" }),
            AccountValue::Undefined => u("undefined"),
            AccountValue::Null => u("null"),
            AccountValue::Symbol(_) => [u("Symbol(mapping/"), vec![0xd800], u(")")].concat(),
            _ => panic!("unproved full object/function String coercion"),
        })
    }
    fn own(&self, v: &AccountValue) -> Vec<(Vec<u16>, AccountValue)> {
        self.0.objects[&self.id(v)].fields.borrow().clone()
    }
    fn meta(&self, id: &str) -> Value {
        self.0.row["objects"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == id)
            .unwrap()
            .clone()
    }
    fn host_array(&self, expected: &Value, values: &[AccountValue]) -> AccountValue {
        let id = expected["id"].as_str().unwrap();
        let m = self.meta(id);
        assert!(m["array"].as_bool().unwrap());
        let fields = m["fields"].as_array().unwrap();
        assert_eq!(
            values.len(),
            fields.len(),
            "{} independent host array count",
            self.0.label
        );
        for (v, e) in values.iter().zip(fields) {
            self.assert_value(v, &e["value"], "hostArray actual element")
        }
        self.0.objects[id].fields.replace(
            values
                .iter()
                .enumerate()
                .map(|(i, v)| (u(&i.to_string()), v.clone()))
                .collect(),
        );
        self.obj(id)
    }
    fn entries_result(
        &self,
        expected: &Value,
        values: Vec<(Vec<u16>, AccountValue)>,
    ) -> AccountValue {
        let array_id = expected["id"].as_str().unwrap();
        let outer = self.meta(array_id);
        let entries = outer["fields"].as_array().unwrap();
        assert_eq!(
            values.len(),
            entries.len(),
            "{} independent Object.entries count",
            self.0.label
        );
        let mut pairs = Vec::new();
        for ((key, value), e) in values.into_iter().zip(entries) {
            let pair = self.host_array(&e["value"], &[AccountValue::String(key), value]);
            pairs.push(pair)
        }
        self.host_array(expected, &pairs)
    }
    fn builtin(
        &self,
        name: &str,
        receiver: &AccountValue,
        args: &[AccountValue],
        expected: &Value,
        string_version: usize,
    ) -> SigningResult<AccountValue> {
        let a = || args.first().cloned().unwrap_or(AccountValue::Undefined);
        let number = |v: &AccountValue| match v {
            AccountValue::Number(n) => *n,
            _ => panic!("bounded Number builtin args"),
        };
        Ok(match name {
            "String" => AccountValue::String(
                [
                    if string_version == 1 {
                        u("new/")
                    } else {
                        Vec::new()
                    },
                    self.native_string(a())?,
                ]
                .concat(),
            ),
            "Boolean" => AccountValue::Boolean(a().truthy()),
            "Number.isFinite" => {
                AccountValue::Boolean(matches!(a(),AccountValue::Number(n)if n.is_finite()))
            }
            "Number.parseFloat" => AccountValue::Number(safe_cex_num::parse_float(
                &String::from_utf16(&self.native_string(a())?)
                    .expect("bounded parsed ASCII strings"),
            )),
            "BigInt" => AccountValue::BigInt(
                String::from_utf16(&self.native_string(a())?)
                    .unwrap()
                    .parse()
                    .expect("regex-checked source digits"),
            ),
            "BigInt.toString" => {
                let AccountValue::BigInt(n) = receiver else {
                    panic!("BigInt receiver")
                };
                AccountValue::String(u(&n.to_string()))
            }
            "String.trim" => {
                let s = self.native_string(receiver.clone())?;
                let begin = s.iter().position(|v| !whitespace(*v)).unwrap_or(s.len());
                let end = s
                    .iter()
                    .rposition(|v| !whitespace(*v))
                    .map(|i| i + 1)
                    .unwrap_or(begin);
                AccountValue::String(s[begin..end].to_vec())
            }
            "String.toUpperCase" | "String.toLowerCase" => {
                let upper = name == "String.toUpperCase";
                let v = self
                    .native_string(receiver.clone())?
                    .into_iter()
                    .map(|v| {
                        if upper && (97..=122).contains(&v) {
                            v - 32
                        } else if !upper && (65..=90).contains(&v) {
                            v + 32
                        } else {
                            v
                        }
                    })
                    .collect();
                AccountValue::String(v)
            }
            "String.replace" => {
                let s = self.native_string(receiver.clone())?;
                let id = self.id(&args[0]);
                let (pattern, flags) = self.0.regex.borrow()[&id].clone();
                assert_eq!(flags, "g");
                let replacement = self.native_string(args[1].clone())?;
                let mut result = Vec::new();
                for v in s {
                    let matched = match pattern.as_str() {
                        "[_\\s-]" => v == 95 || v == 45 || whitespace(v),
                        "[-\\s]" => v == 45 || whitespace(v),
                        "-" => v == 45,
                        _ => panic!("bounded source pattern"),
                    };
                    if matched {
                        result.extend(&replacement)
                    } else {
                        result.push(v)
                    }
                }
                AccountValue::String(result)
            }
            "RegExp.test" => {
                let id = self.id(receiver);
                assert_eq!(self.0.regex.borrow()[&id], ("^\\d+$".into(), "".into()));
                let s = self.native_string(a())?;
                AccountValue::Boolean(!s.is_empty() && s.iter().all(|v| (48..=57).contains(v)))
            }
            "Math.max" => AccountValue::Number(number(&args[0]).max(number(&args[1]))),
            "virtualClock.getCurrentTimeMs" => AccountValue::Number(10000.0),
            "console.warn" => AccountValue::Undefined,
            // Required live owner/import bodies; not substituted native implementations.
            "v" => {
                if self.0.row["input"]["replaceString"] == true
                    && matches!(&args[1],AccountValue::String(s)if *s==u("symbol"))
                {
                    *self.0.string_version.borrow_mut() = 1;
                }
                self.decode(expected)
            }
            "Owner.readPlacedOrderHint" | "Owner.inferStopLossTypeFromReduceOnlyLimit" => {
                self.decode(expected)
            }
            _ => panic!("unsupported bounded builtin {name}"),
        })
    }
}
struct Iterator {
    h: Fixture,
    id: String,
    values: Vec<AccountValue>,
    index: usize,
}
impl OrderMapIterator for Iterator {
    fn next(&mut self) -> SigningResult<Option<AccountValue>> {
        let e = self.h.event("iter-next");
        assert_eq!(e["name"], self.id);
        if !e["error"].is_null() {
            return Err(self.h.error(&e["error"]));
        }
        let m = self.h.meta(e["result"]["id"].as_str().unwrap());
        let fields = m["fields"].as_array().unwrap();
        let done = &fields
            .iter()
            .find(|f| f["keyUnits"] == serde_json::json!(u("done")))
            .unwrap()["value"];
        let value = self.values.get(self.index).cloned();
        self.h.assert_value(
            &AccountValue::Boolean(value.is_none()),
            done,
            "actual native iterator done",
        );
        if let Some(v) = &value {
            let expected = &fields
                .iter()
                .find(|f| f["keyUnits"] == serde_json::json!(u("value")))
                .unwrap()["value"];
            self.h
                .assert_value(v, expected, "actual native iterator element");
            self.index += 1
        }
        Ok(value)
    }
    fn close_return(&mut self) -> SigningResult<()> {
        let e = self.h.event("iter-close");
        assert_eq!(e["name"], self.id);
        if !e["error"].is_null() {
            Err(self.h.error(&e["error"]))
        } else {
            *self.h.0.closed.borrow_mut() += 1;
            Ok(())
        }
    }
    fn close_throw(&mut self, error: TransactionSigningError) -> TransactionSigningError {
        let _closed = self.close_return();
        error
    }
}
impl OrderMappingHost for Fixture {
    fn iterator(&self, value: &AccountValue) -> SigningResult<Box<dyn OrderMapIterator>> {
        let e = self.event("iter-open");
        self.assert_value(value, &e["object"], "iterator native receiver");
        if !e["error"].is_null() {
            return Err(self.error(&e["error"]));
        }
        let values = self.own(value).into_iter().map(|(_, v)| v).collect();
        Ok(Box::new(Iterator {
            h: self.clone(),
            id: e["name"].as_str().unwrap().into(),
            values,
            index: 0,
        }))
    }
    fn copy_data_properties(&self, sources: &[AccountValue]) -> SigningResult<AccountValue> {
        let mut fields: Vec<(Vec<u16>, AccountValue)> = Vec::new();
        for source in sources {
            for (key, _) in self.own(source) {
                let v = self.get_units(source, &key)?;
                if let Some((_, old)) = fields.iter_mut().find(|(k, _)| *k == key) {
                    *old = v
                } else {
                    fields.push((key, v))
                }
            }
        }
        self.construct(
            "copy",
            &fields
                .into_iter()
                .map(|(k, v)| (String::from_utf16(&k).expect("bounded spread own keys"), v))
                .collect::<Vec<_>>(),
        )
    }
    fn binary(
        &self,
        op: OrderMapBinary,
        left: AccountValue,
        right: AccountValue,
    ) -> SigningResult<AccountValue> {
        if let (AccountValue::BigInt(a), AccountValue::BigInt(b)) = (&left, &right) {
            return Ok(AccountValue::Boolean(match op {
                OrderMapBinary::GreaterOrEqual => a >= b,
                OrderMapBinary::LessOrEqual => a <= b,
                _ => panic!("bounded BigInt comparison"),
            }));
        }
        let (AccountValue::Number(a), AccountValue::Number(b)) = (left, right) else {
            panic!("unproved full generic binary/coercion host")
        };
        Ok(match op {
            OrderMapBinary::Add => AccountValue::Number(a + b),
            OrderMapBinary::Subtract => AccountValue::Number(a - b),
            OrderMapBinary::Multiply => AccountValue::Number(a * b),
            OrderMapBinary::Divide => AccountValue::Number(a / b),
            OrderMapBinary::LessOrEqual => AccountValue::Boolean(a <= b),
            OrderMapBinary::GreaterThan => AccountValue::Boolean(a > b),
            OrderMapBinary::GreaterOrEqual => AccountValue::Boolean(a >= b),
        })
    }
    fn regex_literal(&self, pattern: &str, flags: &str) -> SigningResult<AccountValue> {
        let e = self.0.constructs.borrow().front().cloned().unwrap();
        let id = e["id"].as_str().unwrap();
        let m = self.meta(id);
        assert_eq!(
            pattern,
            m["pattern"].as_str().unwrap(),
            "{} native regex source",
            self.0.label
        );
        assert_eq!(flags, m["flags"].as_str().unwrap());
        let result = self.construct("regex", &[])?;
        self.0
            .regex
            .borrow_mut()
            .insert(id.into(), (pattern.into(), flags.into()));
        Ok(result)
    }
}
#[test]
#[ignore = "actual maintained TypeScript producer requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_order_mapping_native_floor() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/order-mapping-oracle.cjs",
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
    let mut count = 0;
    for (i, row) in rows.iter().enumerate() {
        let h = fixture(row, i);
        let step = &row["steps"][0];
        h.step(step);
        let result = map_order(Rc::new(h.clone()), h.decode(&row["args"][0]));
        h.completion(result.clone(), &step["output"], "nativeResult");
        h.end_step();
        assert_eq!(
            *h.0.closed.borrow(),
            row["closed"].as_u64().unwrap() as usize,
            "{} independent closed count",
            h.0.label
        );
        if i == 0 {
            let orders = h.own(result.as_ref().unwrap());
            assert_eq!(orders.len(), 1, "authored mapped1");
            let order = h.own(&orders[0].1);
            assert_eq!(order.len(), 14, "authored mapped14fields");
            let field = |key: &str| order.iter().find(|(k, _)| *k == u(key)).unwrap().1.clone();
            assert!(matches!(field("amount"),AccountValue::Number(n)if n==10.0));
            assert!(matches!(field("remaining"),AccountValue::Number(n)if n==10.0));
            assert!(matches!(field("filled"),AccountValue::Number(n)if n==0.0));
            assert!(matches!(field("price"),AccountValue::Number(n)if n==5.0));
            assert!(matches!(field("id"),AccountValue::String(s)if s==u("17")));
            assert!(matches!(field("side"),AccountValue::String(s)if s==u("buy")));
            assert!(matches!(field("lastUpdate"),AccountValue::Number(n)if n==10000.0));
            h.assert_value(
                &field("info"),
                &row["args"][0],
                "authored original info alias",
            );
            assert_eq!(h.own(&field("extra")).len(), 2, "authored extra2fields");
        }
        if [17, 18, 19, 80, 81, 82, 83, 84, 91, 93, 94, 95, 96, 97, 98].contains(&i) {
            let orders = h.own(result.as_ref().expect("authored selected success"));
            assert_eq!(orders.len(), 1, "{} authored mapped1", h.0.label);
            let order = h.own(&orders[0].1);
            let field = |key: &str| order.iter().find(|(k, _)| *k == u(key)).unwrap().1.clone();
            let number = |key: &str, value: f64| {
                assert!(
                    matches!(field(key),AccountValue::Number(n)if n.to_bits()==value.to_bits()),
                    "{} authored {key}",
                    h.0.label
                )
            };
            let text = |key: &str, value: &str| {
                assert!(
                    matches!(field(key),AccountValue::String(s)if s==u(value)),
                    "{} authored {key}",
                    h.0.label
                )
            };
            match i {
                17 => text("id", "9223372036854775807"),
                18 => text("id", "18"),
                19 => text("id", "19"),
                80 => number("amount", 0.0),
                81 => {
                    number("price", 5.0);
                    assert_eq!(h.id(&field("info")), "Input", "authored original info81")
                }
                82 => {
                    number("price", 13.0);
                    assert_ne!(h.id(&field("info")), "Input", "authored fresh info82")
                }
                83 => {
                    text("type", "stop_market");
                    text("timeInForce", "PostOnly")
                }
                84 => text("id", "17"),
                91 => assert!(
                    matches!(field("reduceOnly"), AccountValue::Boolean(true)),
                    "authored NaN boolean true91"
                ),
                93 => {
                    text("type", "stop_market");
                    text("timeInForce", "PostOnly");
                    number("price", 10.0)
                }
                94 => text("type", "market"),
                95 => text("type", "stop_market"),
                96 | 97 => number("remaining", 10.0),
                98 => {
                    text("type", "stop_market");
                    number("price", 9.0)
                }
                _ => unreachable!(),
            }
        }
        if [85, 86, 87, 88].contains(&i) {
            assert!(
                matches!(result,Err(TransactionSigningError::Thrown(ref v))if v.ptr_eq(&h.0.token)),
                "{} authored original throw",
                h.0.label
            );
        }
        if [20, 21, 50, 89, 99].contains(&i) {
            assert_eq!(
                h.own(result.as_ref().expect("authored empty success"))
                    .len(),
                0,
                "{} authored empty result",
                h.0.label
            );
        }
        count += 1;
    }
    assert_eq!(count, 100, "independent authored order mapping floor100");
    assert_eq!(
        corpus["pin"],
        "16ea63d2aef500c8401da1fe84ef750281f43833daccc881d80537963c8a1b0e"
    );
    assert_eq!(corpus["lodashVersion"], "4.18.1");
    eprintln!(
        "100 native order mapping cases; stopped at iteration99; bounded primitives/dense arrays; no wire assertions"
    );
}
#[test]
fn order_mapping_source_mutation_must_fail_by_name() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/order-mapping-source-control.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "sourceMappingMutationMustFailNamed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
