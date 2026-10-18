use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
    exchange_reconciliation::*,
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
    collections::{BTreeMap, BTreeSet, VecDeque},
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
        .map(|x| u16::try_from(x.as_u64().unwrap()).unwrap())
        .collect()
}
#[derive(Default)]
struct Object {
    fields: RefCell<Vec<(String, AccountValue)>>,
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
        panic!("clock uses actual bound host")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("unused auth encoding")
    }
}
impl Runtime {
    fn drain(&self) {
        for _ in 0..100 {
            let task = self.queue.borrow_mut().pop_front();
            match task {
                Some(task) => task(),
                None => return,
            }
        }
        panic!("native queue floor exhausted at iteration99")
    }
}
struct Env {
    label: String,
    row: Value,
    objects: BTreeMap<String, Rc<Object>>,
    functions: BTreeMap<String, HostThrownValue>,
    symbol: HostThrownValue,
    token: HostThrownValue,
    runtime: Rc<Runtime>,
    events: RefCell<VecDeque<Value>>,
    construction: RefCell<VecDeque<Value>>,
    pending: RefCell<BTreeMap<String, PromiseResolver<AccountValue>>>,
    resolved: RefCell<BTreeSet<String>>,
    callback: RefCell<Option<ReconciliationCallback>>,
}
#[derive(Clone)]
struct Fixture(Rc<Env>);
impl Fixture {
    fn obj(&self, id: &str) -> AccountValue {
        AccountValue::Object(
            self.0
                .objects
                .get(id)
                .unwrap_or_else(|| panic!("{} missing object {id}", self.0.label))
                .clone(),
        )
    }
    fn id(&self, v: &AccountValue) -> String {
        let AccountValue::Object(v) = v else {
            panic!("{} object required", self.0.label)
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
            "string" => AccountValue::String(units(&v["units"])),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            "function" => {
                AccountValue::Function(self.0.functions[v["id"].as_str().unwrap()].clone())
            }
            "object" => self.obj(v["id"].as_str().unwrap()),
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
            (AccountValue::Symbol(n), "symbol") => {
                assert!(n.ptr_eq(&self.0.symbol), "{l} nativeSymbolIdentity")
            }
            (AccountValue::Function(n), "function") => assert!(
                n.ptr_eq(&self.0.functions[e["id"].as_str().unwrap()]),
                "{l} nativeFunctionIdentity"
            ),
            (AccountValue::Object(n), "object") => assert!(
                Rc::ptr_eq(
                    n,
                    &(self.0.objects[e["id"].as_str().unwrap()].clone() as Rc<dyn AccountObject>)
                ),
                "{l} nativeObjectIdentity"
            ),
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
        } else if e["error"].is_object() {
            let name = match e["error"]["name"].as_str().unwrap() {
                "TypeError" => "TypeError",
                _ => panic!("unproved source generated error"),
            };
            Err(TransactionSigningError::Generated(TransactionBuildError {
                name,
                message: units(&e["error"]["message"]),
            }))
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
    fn step(&self, s: &Value) {
        self.end_step();
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
    fn literal_meta(&self, id: &str) -> Value {
        self.0.row["literals"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["id"] == id)
            .unwrap()
            .clone()
    }
    fn engine_fields(&self, v: &AccountValue) {
        if let AccountValue::Object(_) = v {
            let id = self.id(v);
            if id.starts_with("Literal") {
                let meta = self.literal_meta(&id);
                self.0.objects[&id].fields.replace(
                    meta["fields"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|f| (f["key"].as_str().unwrap().into(), self.decode(&f["value"])))
                        .collect(),
                );
            }
        }
    }
    fn construct(
        &self,
        op: &str,
        fields: &[(String, AccountValue)],
    ) -> SigningResult<AccountValue> {
        let e = self
            .0
            .construction
            .borrow_mut()
            .pop_front()
            .unwrap_or_else(|| panic!("{} unexpected native construction {op}", self.0.label));
        assert_eq!(e["op"], op, "{} nativeConstructionOrder", self.0.label);
        let id = e["id"].as_str().unwrap();
        let meta = self.literal_meta(id);
        let expected = meta["fields"].as_array().unwrap();
        assert_eq!(
            fields.len(),
            expected.len(),
            "{} independentSourceOwnFieldCount",
            self.0.label
        );
        for ((key, value), e) in fields.iter().zip(expected) {
            assert_eq!(
                key,
                e["key"].as_str().unwrap(),
                "{} nativeOwnKeySchema",
                self.0.label
            );
            self.assert_value(value, &e["value"], &format!("{op}:{key}"));
        }
        self.0.objects[id].fields.replace(fields.to_vec());
        Ok(self.obj(id))
    }
    fn provider_value(&self, name: &str) -> AccountValue {
        match name {
            "positions" => self.obj("Positions"),
            "orders" => self.obj("Orders"),
            "raw" | "balance" => self.obj("RawBalance"),
            "helper" => AccountValue::Undefined,
            "ensure" => self.obj("Signer"),
            "auth" => AccountValue::String(u("auth/").into_iter().chain([0xd800]).collect()),
            "All" => self.obj("Pair"),
            _ => panic!("independent provider {name}"),
        }
    }
    fn resolve(&self, name: &str) {
        self.0.resolved.borrow_mut().insert(name.into());
        if let Some(r) = self.0.pending.borrow_mut().remove(name) {
            r.settle(if self.0.row["input"]["reject"] == name {
                Err(TransactionSigningError::Thrown(self.0.token.clone()))
            } else {
                let value = self.provider_value(name);
                if name == "raw" || name == "balance" {
                    self.get(&value, "then").map(|_| value)
                } else {
                    Ok(value)
                }
            });
        }
        let all_error = matches!(
            self.0.row["input"]["reject"].as_str(),
            Some("positions" | "orders")
        ) && self.0.row["input"]["reject"] == name;
        let all_done = {
            let done = self.0.resolved.borrow();
            done.contains("positions") && done.contains("orders")
        };
        if all_error || all_done {
            if let Some(r) = self.0.pending.borrow_mut().remove("All") {
                let result = if all_error {
                    Err(TransactionSigningError::Thrown(self.0.token.clone()))
                } else {
                    let pair = self.obj("Pair");
                    self.get(&pair, "then").map(|_| pair)
                };
                r.settle(result);
            }
        }
    }
    fn generated_member_error(
        &self,
        v: &AccountValue,
        key: &str,
    ) -> Option<TransactionSigningError> {
        let name = match v {
            AccountValue::Undefined => "undefined",
            AccountValue::Null => "null",
            _ => return None,
        };
        Some(TransactionSigningError::Generated(TransactionBuildError {
            name: "TypeError",
            message: u(&format!(
                "Cannot read properties of {name} (reading '{key}')"
            )),
        }))
    }
}
struct Call {
    h: Fixture,
    name: String,
    receiver: AccountValue,
    noncallable: bool,
}
impl AccountAccessCall for Call {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        if self.noncallable {
            return Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u("this.signerAdapter.dispose is not a function"),
            }));
        }
        let e = self.h.event("call");
        assert_eq!(e["name"], self.name, "{} nativeCallName", self.h.0.label);
        self.h
            .assert_value(&self.receiver, &e["receiver"], "callReceiver");
        let expected = e["args"].as_array().unwrap();
        assert_eq!(
            args.len(),
            expected.len(),
            "{} independentSourceArgumentCount",
            self.h.0.label
        );
        for (i, (value, e)) in args.iter().zip(expected).enumerate() {
            self.h
                .assert_value(value, e, &format!("{} arg{i}", self.name));
        }
        if self.name == "loop" && self.h.0.row["input"]["loopNow"] == true {
            let callback = self.h.0.callback.borrow().as_ref().unwrap().clone();
            let _ = callback();
        }
        self.h.result(&e)
    }
    fn construct(&self, _: &[AccountValue]) -> SigningResult<AccountValue> {
        panic!("no Construct in these source bodies")
    }
}
impl ExchangeAccountAccessHost for Fixture {
    fn owner(&self) -> AccountValue {
        self.obj("Owner")
    }
    fn get(&self, obj: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        if matches!(obj, AccountValue::String(_) | AccountValue::Symbol(_)) && key == "message" {
            return Ok(AccountValue::Undefined);
        }
        let e = self.event("get");
        self.assert_value(obj, &e["object"], "getReceiver");
        assert_eq!(e["key"], key, "{} nativeGetKey", self.0.label);
        self.result(&e)
    }
    fn set(&self, _: &AccountValue, _: &str, _: AccountValue) -> SigningResult<()> {
        panic!("no direct owner Set in source bodies")
    }
    fn member(&self, obj: &AccountValue, key: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        if let Some(error) = self.generated_member_error(obj, key) {
            return Err(error);
        }
        let special = self.id_if_object(obj) == Some("Signer".into()) && key == "dispose";
        let e = self.event(if special { "get" } else { "member" });
        self.assert_value(obj, &e["object"], "memberReceiver");
        assert_eq!(e["key"], key, "{} nativeMemberKey", self.0.label);
        let captured = self.result(&e)?;
        let noncallable = !matches!(captured, AccountValue::Function(_));
        let name = if special {
            "Signer.dispose".into()
        } else {
            e["name"].as_str().unwrap().into()
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            name,
            receiver: obj.clone(),
            noncallable,
        }))
    }
    fn global(&self, name: &str) -> SigningResult<Rc<dyn AccountAccessCall>> {
        let e = self.event("global");
        assert_eq!(e["name"], name, "{} nativeGlobalName", self.0.label);
        self.result(&e)?;
        let receiver = if name == "super.dispose" {
            self.owner()
        } else if name == "Promise.all" {
            AccountValue::Function(self.0.functions["Promise"].clone())
        } else if name == "virtualClock.getCurrentTimeMs" {
            self.obj("virtualClock")
        } else {
            AccountValue::Undefined
        };
        Ok(Rc::new(Call {
            h: self.clone(),
            name: name.into(),
            receiver,
            noncallable: false,
        }))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        if fields.is_empty() && self.0.construction.borrow().is_empty() {
            assert_eq!(
                fields.len(),
                0,
                "{} authoredEmptyDefaultOptions",
                self.0.label
            );
            return Ok(self.obj("DefaultOpts"));
        }
        self.construct(
            "literal",
            &fields
                .iter()
                .map(|(k, v)| ((*k).into(), v.clone()))
                .collect::<Vec<_>>(),
        )
    }
    fn throw_value(&self, _: AccountValue) -> TransactionSigningError {
        panic!("no explicit throw construction in source bodies")
    }
    fn resolve_return(&self, v: AccountValue, r: PromiseResolver<AccountValue>) {
        if self
            .id_if_object(&v)
            .is_some_and(|id| id == "RawBalance" || id == "StoredBalance")
        {
            match self.get(&v, "then") {
                Err(error) => r.settle(Err(error)),
                Ok(AccountValue::Undefined) => r.settle(Ok(v)),
                Ok(AccountValue::Function(_)) => {
                    let live = self.clone();
                    self.0.runtime.enqueue_microtask(Box::new(move || {
                        let e = live.event("return-then-call");
                        live.assert_value(&v, &e["object"], "nativeReturnedThenReceiver");
                        r.settle(if live.0.row["input"]["returnThen"] == "resolve" {
                            Ok(AccountValue::String(
                                u("adopted/").into_iter().chain([0xd800]).collect(),
                            ))
                        } else {
                            Err(TransactionSigningError::Thrown(live.0.token.clone()))
                        });
                    }));
                }
                _ => panic!("authored returned then descriptor"),
            }
        } else {
            r.settle(Ok(v))
        }
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn await_value(&self, v: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        let id = self.id(&v);
        let name = id
            .strip_suffix("Promise")
            .unwrap_or_else(|| panic!("{} actual promise marker required {id}", self.0.label));
        let (p, r) = NativePromise::pending(self.runtime());
        assert!(
            self.0.pending.borrow_mut().insert(name.into(), r).is_none(),
            "{} independent one await per provider",
            self.0.label
        );
        Ok(p)
    }
}
impl Fixture {
    fn id_if_object(&self, v: &AccountValue) -> Option<String> {
        matches!(v, AccountValue::Object(_)).then(|| self.id(v))
    }
}
impl ReconciliationHost for Fixture {
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        self.construct(
            "array",
            &values
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect::<Vec<_>>(),
        )
    }
    fn pair(&self, value: AccountValue) -> SigningResult<(AccountValue, AccountValue)> {
        let iterator = self.get(&value, "@@iterator")?;
        self.assert_value(
            &iterator,
            &serde_json::json!({"kind":"function","id":"IteratorFunction"}),
            "nativeIteratorFunction",
        );
        let iter = Call {
            h: self.clone(),
            name: "Pair.iterator".into(),
            receiver: value,
            noncallable: false,
        }
        .call(&[])?;
        let next = self.get(&iter, "next")?;
        self.assert_value(
            &next,
            &serde_json::json!({"kind":"function","id":"Callee"}),
            "nativeIteratorNextFunction",
        );
        let mut values = vec![];
        for _ in 0..2 {
            let result = Call {
                h: self.clone(),
                name: "Iterator.next".into(),
                receiver: iter.clone(),
                noncallable: false,
            }
            .call(&[])?;
            let done = self.get(&result, "done")?;
            assert!(!done.truthy(), "authored two source pair elements");
            values.push(self.get(&result, "value")?);
        }
        let close = self.get(&iter, "return")?;
        assert!(
            matches!(close, AccountValue::Undefined),
            "authored ordinary array iterator has no return"
        );
        Ok((values.remove(0), values.remove(0)))
    }
    fn spread_override(
        &self,
        value: AccountValue,
        fields: &[(&str, AccountValue)],
    ) -> SigningResult<AccountValue> {
        let e = self.event("own-keys");
        self.assert_value(&value, &e["object"], "spreadReceiver");
        let keys = self.result(&e)?;
        self.engine_fields(&keys);
        let key_id = self.id(&keys);
        let keys = self.0.objects[&key_id]
            .fields
            .borrow()
            .iter()
            .map(|(_, v)| {
                let AccountValue::String(v) = v else {
                    panic!("authored ASCII source own keys")
                };
                String::from_utf16(v).unwrap()
            })
            .collect::<Vec<_>>();
        let mut output = vec![];
        for key in keys {
            let e = self.event("descriptor");
            self.assert_value(&value, &e["object"], "descriptorReceiver");
            assert_eq!(e["key"], key, "{} nativeDescriptorKey", self.0.label);
            let desc = self.result(&e)?;
            self.engine_fields(&desc);
            let AccountValue::Object(desc) = desc else {
                panic!("own descriptor")
            };
            if desc.get("enumerable")?.truthy() {
                output.push((key.clone(), self.get(&value, &key)?));
            }
        }
        for (key, v) in fields {
            if let Some((_, old)) = output.iter_mut().find(|(k, _)| k == key) {
                *old = v.clone()
            } else {
                output.push(((*key).into(), v.clone()))
            }
        }
        self.construct("spread", &output)
    }
    fn callback(&self, value: ReconciliationCallback) -> SigningResult<AccountValue> {
        self.0.callback.replace(Some(value));
        Ok(AccountValue::Function(self.0.functions["Callback"].clone()))
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        let TransactionSigningError::Thrown(token) = error else {
            panic!("generated catch projection outside current floor")
        };
        assert!(
            token.ptr_eq(&self.0.token),
            "{} nativeOriginalCaughtIdentity",
            self.0.label
        );
        match self.0.row["input"]["errorKind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "string" => AccountValue::String(u("err/").into_iter().chain([0xd800]).collect()),
            "symbol" => AccountValue::Symbol(self.0.symbol.clone()),
            _ => self.obj("Token"),
        }
    }
    fn template_string(&self, v: AccountValue) -> SigningResult<Vec<u16>> {
        match v {
            AccountValue::String(v) => Ok(v),
            AccountValue::Symbol(_) => {
                Err(TransactionSigningError::Generated(TransactionBuildError {
                    name: "TypeError",
                    message: u("Cannot convert a Symbol value to a string"),
                }))
            }
            _ => panic!("ordinary fixture error/symbol supplied as String before template"),
        }
    }
}
fn fixture(row: &Value, i: usize) -> Fixture {
    let mut objects = BTreeSet::new();
    let mut functions = BTreeSet::new();
    fn visit(v: &Value, objects: &mut BTreeSet<String>, functions: &mut BTreeSet<String>) {
        match v {
            Value::Object(o) => {
                if v["kind"] == "object" {
                    objects.insert(v["id"].as_str().unwrap().into());
                }
                if v["kind"] == "function" {
                    functions.insert(v["id"].as_str().unwrap().into());
                }
                for v in o.values() {
                    visit(v, objects, functions)
                }
            }
            Value::Array(a) => {
                for v in a {
                    visit(v, objects, functions)
                }
            }
            _ => {}
        }
    }
    visit(row, &mut objects, &mut functions);
    objects.extend(
        [
            "Owner",
            "Pair",
            "DefaultOpts",
            "Positions",
            "Orders",
            "RawBalance",
            "Signer",
        ]
        .map(str::to_owned),
    );
    for l in row["literals"].as_array().unwrap() {
        objects.insert(l["id"].as_str().unwrap().into());
    }
    functions.extend(["Callback", "Callee", "Promise", "IteratorFunction"].map(str::to_owned));
    Fixture(Rc::new(Env {
        label: format!("iteration{i} {}", row["method"].as_str().unwrap()),
        row: row.clone(),
        objects: objects
            .into_iter()
            .map(|id| (id, Rc::new(Object::default())))
            .collect(),
        functions: functions
            .into_iter()
            .map(|id| (id, HostThrownValue::new(())))
            .collect(),
        symbol: HostThrownValue::new(()),
        token: HostThrownValue::new(()),
        runtime: Rc::new(Runtime::default()),
        events: RefCell::default(),
        construction: RefCell::new(
            row["construction"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect(),
        ),
        pending: RefCell::default(),
        resolved: RefCell::default(),
        callback: RefCell::default(),
    }))
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_reconciliation_native_floor() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/reconciliation-oracle.cjs",
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
        let observed = Rc::new(RefCell::new(None));
        if row["method"] == "dispose" {
            h.completion(dispose(&h), &steps[0]["output"], "nativeResult");
            h.end_step();
        } else {
            let host: Rc<dyn ReconciliationHost> = Rc::new(h.clone());
            let p = match row["method"].as_str().unwrap() {
                "fetchAndSetPrivateSnapshot" => fetch_and_set_private_snapshot(host),
                "fetchAndSetPrivateSnapshotHelper" => fetch_and_set_private_snapshot_helper(host),
                "tick" => tick(host),
                "fetchBalance" => fetch_balance(host, a(0)),
                "fetchAndSetBalance" => fetch_and_set_balance(host, a(0)),
                "syncLeverageFromExchange" => sync_leverage_from_exchange(host, a(0)),
                "getPrivateWsAuthToken" => get_private_ws_auth_token(host, a(0)),
                _ => panic!("source body"),
            };
            h.end_step();
            let saved = observed.clone();
            p.subscribe(move |v| {
                saved.replace(Some(v.map(|v| (*v).clone())));
            });
            assert!(
                observed.borrow().is_none(),
                "{} nativeAsyncQueued",
                h.0.label
            );
        }
        for s in &steps[1..] {
            h.step(s);
            if let Some(resolve) = s["resolve"].as_str() {
                h.resolve(resolve);
            }
            h.0.runtime.drain();
            if s["phase"] == "settled" {
                h.completion(
                    observed.borrow_mut().take().expect("native settled"),
                    &s["output"],
                    "nativeResult",
                );
            }
            if s["phase"] == "callback" {
                let callback = h.0.callback.borrow().as_ref().unwrap().clone();
                h.completion(callback(), &s["output"], "callbackResult");
            }
            h.end_step();
        }
        assert_eq!(
            h.0.construction.borrow().len(),
            0,
            "{} independentSourceConstructionCount",
            h.0.label
        );
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 170, "independent authored total floor");
    for (name, n) in [
        ("fetchAndSetPrivateSnapshot", 4),
        ("fetchAndSetPrivateSnapshotHelper", 42),
        ("tick", 18),
        ("fetchBalance", 19),
        ("fetchAndSetBalance", 19),
        ("syncLeverageFromExchange", 17),
        ("getPrivateWsAuthToken", 31),
        ("dispose", 20),
    ] {
        assert_eq!(counts[name], n, "independent authored {name} floor");
    }
    // All decoded native values/effects/receiver/schema assertions precede pin.
    assert_eq!(
        corpus["contentHash"], "cf71da8343509d29802437b00a4d5a5a3935d566267fc96f51fd2e69aca3133f",
        "source-only pin AFTER native assertions"
    );
    println!("170 native cases; stopped at iteration169; no exhaustive/runtime/engine claim");
}
