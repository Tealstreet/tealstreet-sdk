use safe_cex_venue_lighter::{
    exchange_account_access::{AccountAccessCall, ExchangeAccountAccessHost},
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
        let result = cb(&args);
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
        self.result(&e)
    }
    fn provider(&self, name: &str) -> AccountValue {
        match name {
            "limits" => {
                if self.0.row["input"]["noTier"] == true {
                    AccountValue::Undefined
                } else {
                    self.obj("AccountLimits")
                }
            }
            "account" => self.obj("Account"),
            "positions" => self.obj("Positions"),
            "signer" => self.obj("Signer"),
            "update" | "sync" | "cancel" => AccountValue::Undefined,
            _ => panic!("authored provider"),
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
        if matches!(o, AccountValue::Object(_)) {
            let id = self.id(o);
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
        let AccountValue::String(key) = key else {
            panic!("authored string index floor; full ToPropertyKey required")
        };
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
    h
}
#[test]
#[ignore = "actual TypeScript oracle requires SAFE_CEX_TYPESCRIPT_PATH"]
fn actual_trading_preflight_native_floor() {
    let out = Command::new("node")
        .arg(format!(
            "{}/scripts/trading-preflight-oracle.cjs",
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
        if row["method"] == "resolveConditionalExecutionPrice" {
            let AccountValue::Number(trigger) = a(0) else {
                panic!("typed trigger")
            };
            h.completion(
                resolve_conditional_execution_price(&h, trigger, a(1), a(2)),
                &steps[0]["output"],
                "nativeResult",
            )
        } else {
            let live: Rc<dyn TradingHost> = Rc::new(h.clone());
            let promise = match row["method"].as_str().unwrap() {
                "ensurePartnerAttributionTierEligible" => {
                    ensure_partner_attribution_tier_eligible(live, a(0))
                }
                "ensurePartnerAttributionApproved" => {
                    ensure_partner_attribution_approved(live, a(0))
                }
                "refreshMarginModeStateAfterInvalidOrder" => {
                    refresh_margin_mode_state_after_invalid_order(live, a(0))
                }
                "assertReduceOnlyLimitHasPosition" => {
                    assert_reduce_only_limit_has_position(live, a(0))
                }
                "setLeverage" => {
                    let AccountValue::Number(n) = a(1) else {
                        panic!("typed leverage")
                    };
                    set_leverage(live, a(0), n)
                }
                "changeMarginType" => change_margin_type(live, a(0), a(1)),
                "cancelSymbolOrders" => cancel_symbol_orders(live, a(0)),
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
            let resolver = h.0.pending.borrow_mut().remove(name).unwrap();
            let value = if row["input"]["reject"] == name {
                Err(TransactionSigningError::Thrown(h.0.token.clone()))
            } else {
                let value = h.provider(name);
                while h.0.events.borrow().front().is_some_and(|e| {
                    e["op"] == "get" && e["keyUnits"] == serde_json::json!(u("then"))
                }) {
                    h.get(&value, "then").unwrap();
                }
                Ok(value)
            };
            resolver.settle(value);
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
        assert!(
            h.0.pending.borrow().is_empty(),
            "{} independentProviderConsumption",
            h.0.label
        );
        *counts
            .entry(row["method"].as_str().unwrap().into())
            .or_default() += 1;
    }
    assert_eq!(rows.len(), 148, "independent authored total floor");
    for (name, n) in [
        ("ensurePartnerAttributionTierEligible", 20),
        ("ensurePartnerAttributionApproved", 29),
        ("refreshMarginModeStateAfterInvalidOrder", 19),
        ("assertReduceOnlyLimitHasPosition", 18),
        ("resolveConditionalExecutionPrice", 12),
        ("setLeverage", 27),
        ("changeMarginType", 16),
        ("cancelSymbolOrders", 7),
    ] {
        assert_eq!(counts[name], n, "independent authored {name} floor");
    }
    // All decoded native results/args/identity/schema/closures precede source pin.
    assert_eq!(
        corpus["contentHash"], "43cb74fbead1c8292220634543fba0beecf49bb8510a2f28ae221b657899b331",
        "source-only pin AFTER native assertions"
    );
    println!("148 native cases; stopped at iteration147; no exhaustive/runtime/engine claim");
}
