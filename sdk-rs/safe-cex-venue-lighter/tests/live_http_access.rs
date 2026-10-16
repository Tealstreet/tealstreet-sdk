use safe_cex_venue_lighter::{
    sdk_account_api::{AccountObject, AccountSemantics, AccountValue},
    sdk_http_access::*,
    transaction_builders::TransactionBuildError,
    transaction_promise::*,
    transaction_signing::*,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
    process::Command,
    rc::Rc,
};
fn capture() -> &'static Value {
    static C: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/http-access-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "TS oracle: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, task: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(task)
    }
    fn now_millis(&self) -> f64 {
        panic!("no source clock call")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("no source encoding call")
    }
}
impl Runtime {
    fn drain(&self) {
        loop {
            let next = self.queue.borrow_mut().pop_front();
            let Some(task) = next else { break };
            task();
        }
    }
}
struct Obj {
    props: RefCell<Vec<(String, AccountValue, bool)>>,
    symbols: RefCell<Vec<(HostThrownValue, AccountValue)>>,
    array: RefCell<Option<Vec<AccountValue>>>,
}
impl Obj {
    fn new() -> Self {
        Self {
            props: RefCell::new(vec![]),
            symbols: RefCell::new(vec![]),
            array: RefCell::new(None),
        }
    }
    fn set(&self, key: &str, value: AccountValue) {
        let mut p = self.props.borrow_mut();
        if let Some((_, v, _)) = p.iter_mut().find(|(k, _, _)| k == key) {
            *v = value
        } else {
            p.push((key.into(), value, true))
        }
    }
}
impl AccountObject for Obj {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        Ok(self
            .props
            .borrow()
            .iter()
            .find(|(k, _, _)| k == key)
            .map(|(_, v, _)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
struct Fixture {
    row: Value,
    objects: HashMap<String, Rc<Obj>>,
    runtime: Rc<Runtime>,
    token: HostThrownValue,
    symbol: HostThrownValue,
    functions: HashMap<String, HostThrownValue>,
    trace: RefCell<Vec<String>>,
    calls: RefCell<Vec<(String, Option<String>, Vec<AccountValue>)>>,
    registry: RefCell<String>,
    registry_reads: Cell<usize>,
    config: RefCell<String>,
    symbol_reads: Cell<usize>,
    release: RefCell<Option<PromiseResolver<AccountValue>>>,
    empty_created: Cell<bool>,
}
impl Fixture {
    fn new(row: &Value) -> Rc<Self> {
        let h = Rc::new(Self {
            row: row.clone(),
            objects: ["A", "B", "C", "D", "R", "T", "J0", "J1", "X", "Y", "E"]
                .into_iter()
                .map(|id| (id.into(), Rc::new(Obj::new())))
                .collect(),
            runtime: Rc::new(Runtime::default()),
            token: HostThrownValue::new("token"),
            symbol: HostThrownValue::new("proof symbol"),
            functions: ["I", "then"]
                .into_iter()
                .map(|id| (id.into(), HostThrownValue::new(id)))
                .collect(),
            trace: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            registry: RefCell::new("J0".into()),
            registry_reads: Cell::new(0),
            config: RefCell::new("C".into()),
            symbol_reads: Cell::new(0),
            release: RefCell::new(None),
            empty_created: Cell::new(false),
        });
        *h.objects["X"].array.borrow_mut() = Some(if h.scenario() == "seeded" {
            vec![AccountValue::Function(h.functions["I"].clone())]
        } else {
            vec![]
        });
        *h.objects["Y"].array.borrow_mut() = Some(vec![]);
        for category in ["request", "response", "error"] {
            h.objects["J0"].set(category, h.object("X"));
            h.objects["J1"].set(category, h.object("Y"));
        }
        h.objects["J0"].set(h.category(), h.decode(&row["input"]["initial"]));
        let mut base = u("https://api/");
        base.push(0xd800);
        for (key, value) in [
            ("baseUrl", AccountValue::String(base)),
            ("timeout", AccountValue::Number(321.)),
            ("headers", h.object("A")),
            ("debug", AccountValue::Boolean(true)),
            ("Symbol(proof)", h.object("A")),
        ] {
            h.objects["C"].set(key, value)
        }
        let mut base = u("https://api/");
        base.push(0xd800);
        h.objects["C"].set("baseUrl", AccountValue::String(base));
        h.objects["C"]
            .props
            .borrow_mut()
            .push(("hidden".into(), h.object("B"), false));
        h.objects["C"]
            .symbols
            .borrow_mut()
            .push((h.symbol.clone(), h.object("B")));
        h.objects["R"].set("data", h.decode(&row["input"]["data"]));
        h.objects["T"].set("then", AccountValue::Function(h.functions["then"].clone()));
        h
    }
    fn scenario(&self) -> &str {
        self.row["scenario"].as_str().unwrap()
    }
    fn category(&self) -> &str {
        match self.row["method"].as_str().unwrap() {
            "addResponseInterceptor" => "response",
            "addErrorInterceptor" => "error",
            _ => "request",
        }
    }
    fn object(&self, id: &str) -> AccountValue {
        AccountValue::Object(self.objects[id].clone())
    }
    fn decode(&self, v: &Value) -> AccountValue {
        match v["kind"].as_str().unwrap() {
            "undefined" => AccountValue::Undefined,
            "null" => AccountValue::Null,
            "boolean" => AccountValue::Boolean(v["value"].as_bool().unwrap()),
            "number" => AccountValue::Number(number(v)),
            "string" => AccountValue::String(units(&v["units"])),
            "bigint" => AccountValue::BigInt(v["decimal"].as_str().unwrap().parse().unwrap()),
            "object" => self.object(v["id"].as_str().unwrap()),
            "function" => AccountValue::Function(self.functions[v["id"].as_str().unwrap()].clone()),
            "symbol" => AccountValue::Symbol(self.symbol.clone()),
            _ => panic!("unwitnessed carrier"),
        }
    }
    fn identify(&self, value: &AccountValue) -> Option<String> {
        if let AccountValue::Object(v) = value {
            for (id, o) in &self.objects {
                let candidate: Rc<dyn AccountObject> = o.clone();
                if Rc::ptr_eq(v, &candidate) {
                    return Some(id.clone());
                }
            }
        }
        None
    }
    fn touch(&self, event: &str) -> SigningResult<()> {
        self.trace.borrow_mut().push(event.into());
        let s = self.scenario();
        let mut fail = matches!(
            (s, event),
            ("axiosOwnerThrow", "owner:axios")
                | ("httpLookupThrow", "lookup:http")
                | ("httpCallThrow", "call:http")
                | ("dataGetterThrow", "property:R:data")
                | ("dataThenGetterThrow", "property:T:then")
                | ("ownerThrow", "owner:registry")
                | ("pushLookupThrow", "lookup:push:X")
                | ("pushCallThrow", "call:push:X")
                | ("configOwnerThrow", "owner:config")
                | ("baseGetterThrow", "property:C:baseUrl")
        );
        fail |= s == "getThrow" && event == format!("property:J0:{}", self.category());
        fail |= s == "setThrow" && event == format!("set:J0:{}", self.category());
        if s == "copySymbolThrow" && event == "property:C:Symbol(proof)" {
            let n = self.symbol_reads.get();
            self.symbol_reads.set(n + 1);
            fail |= n == 1;
        }
        if fail {
            return Err(TransactionSigningError::Thrown(self.token.clone()));
        }
        if s == "setterReplace" && event == format!("set:J0:{}", self.category())
            || s == "pushLookupReplace" && event == "lookup:push:X"
        {
            *self.registry.borrow_mut() = "J1".into();
        }
        if s == "copyReplace" && event == "property:C:baseUrl" {
            *self.config.borrow_mut() = "B".into();
        }
        Ok(())
    }
    fn property(&self, value: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        if let Some(id) = self.identify(value) {
            self.touch(&format!("property:{id}:{key}"))?;
            self.objects[&id].get(key)
        } else if value.is_nullish() {
            let kind = if matches!(value, AccountValue::Null) {
                "null"
            } else {
                "undefined"
            };
            Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u(&format!(
                    "Cannot read properties of {kind} (reading '{key}')"
                )),
            }))
        } else {
            Ok(AccountValue::Undefined)
        }
    }
    fn assert_value(&self, value: &AccountValue, w: &Value, label: &str) {
        match value {
            AccountValue::Undefined => {
                assert_eq!(w["kind"], "undefined", "{label} native undefined")
            }
            AccountValue::Null => assert_eq!(w["kind"], "null", "{label} native null"),
            AccountValue::Boolean(v) => {
                assert_eq!(w["kind"], "boolean");
                assert_eq!(*v, w["value"].as_bool().unwrap(), "{label} native boolean")
            }
            AccountValue::Number(v) => {
                assert_eq!(w["kind"], "number");
                assert_eq!(
                    v.to_bits(),
                    number(w).to_bits(),
                    "{label} native number bits"
                )
            }
            AccountValue::BigInt(v) => {
                assert_eq!(w["kind"], "bigint");
                let expected: num_bigint::BigInt = w["decimal"].as_str().unwrap().parse().unwrap();
                assert_eq!(*v, expected, "{label} native BigInt")
            }
            AccountValue::String(v) => {
                assert_eq!(w["kind"], "string");
                assert_eq!(*v, units(&w["units"]), "{label} native UTF16")
            }
            AccountValue::Object(v) => {
                assert_eq!(w["kind"], "object");
                let expected: Rc<dyn AccountObject> =
                    self.objects[w["id"].as_str().unwrap()].clone();
                assert!(Rc::ptr_eq(v, &expected), "{label} native object identity")
            }
            AccountValue::Function(v) => {
                assert_eq!(w["kind"], "function");
                assert!(
                    v.ptr_eq(&self.functions[w["id"].as_str().unwrap()]),
                    "{label} native function identity"
                )
            }
            AccountValue::Symbol(v) => {
                assert_eq!(w["kind"], "symbol");
                assert_eq!(w["id"], "proof");
                assert!(v.ptr_eq(&self.symbol), "{label} native symbol identity")
            }
            _ => panic!("unwitnessed native value"),
        }
    }
}
struct Semantics(Rc<Fixture>);
impl AccountSemantics for Semantics {
    fn get_property(&self, v: &AccountValue, k: &str) -> SigningResult<AccountValue> {
        self.0.property(v, k)
    }
    fn error_message(&self, _: &AccountValue) -> SigningResult<Vec<u16>> {
        panic!("no source error-message coercion")
    }
    fn resolve_return(
        self: Rc<Self>,
        value: AccountValue,
        resolver: PromiseResolver<AccountValue>,
        runtime: Rc<dyn TransactionRuntime>,
    ) {
        if let AccountValue::Object(_) = value {
            match self.0.property(&value, "then") {
                Err(error) => {
                    resolver.settle(Err(error));
                    return;
                }
                Ok(AccountValue::Function(_)) => {
                    let h = self.0.clone();
                    let next_runtime = runtime.clone();
                    runtime.enqueue_microtask(Box::new(move || {
                        if let Err(error) = h.touch("call:then") {
                            resolver.settle(Err(error));
                            return;
                        }
                        self.resolve_return(h.object("B"), resolver, next_runtime);
                    }));
                    return;
                }
                Ok(_) => {}
            }
        }
        resolver.settle(Ok(value));
    }
}
struct RequestCall {
    h: Rc<Fixture>,
    verb: HttpVerb,
}
impl HttpCall for RequestCall {
    fn call(&self, args: &[AccountValue]) -> SigningResult<NativePromise<AccountValue>> {
        self.h.touch("call:http")?;
        self.h.calls.borrow_mut().push((
            format!("{:?}", self.verb).to_lowercase(),
            None,
            args.to_vec(),
        ));
        let h = &self.h;
        let (p, resolver) = NativePromise::pending(h.runtime.clone());
        if h.scenario() == "rejection" {
            resolver.settle(Err(TransactionSigningError::Thrown(h.token.clone())))
        } else if h.scenario() == "delayed" {
            *h.release.borrow_mut() = Some(resolver);
        } else {
            Rc::new(Semantics(h.clone())).resolve_return(response(h), resolver, h.runtime.clone());
        }
        Ok(p)
    }
}
fn response(h: &Fixture) -> AccountValue {
    match h.scenario() {
        "responseNull" => AccountValue::Null,
        "responseUndefined" => AccountValue::Undefined,
        _ => h.object("R"),
    }
}
struct Push {
    h: Rc<Fixture>,
    id: String,
}
impl AccessCall for Push {
    fn call(&self, args: &[AccountValue]) -> SigningResult<AccountValue> {
        if self.id == "A" {
            return Err(TransactionSigningError::Generated(TransactionBuildError {
                name: "TypeError",
                message: u(&format!(
                    "this.interceptors.{}.push is not a function",
                    self.h.category()
                )),
            }));
        }
        self.h.touch(&format!("call:push:{}", self.id))?;
        self.h
            .calls
            .borrow_mut()
            .push(("push".into(), Some(self.id.clone()), args.to_vec()));
        let mut array = self.h.objects[&self.id].array.borrow_mut();
        let v = array.as_mut().unwrap();
        v.push(args[0].clone());
        Ok(AccountValue::Number(v.len() as f64))
    }
}
struct Host(Rc<Fixture>);
impl HttpAccessHost for Host {
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.0.runtime.clone()
    }
    fn semantics(&self) -> Rc<dyn AccountSemantics> {
        Rc::new(Semantics(self.0.clone()))
    }
    fn http_callback(&self, verb: HttpVerb) -> SigningResult<Rc<dyn HttpCall>> {
        self.0.touch("owner:axios")?;
        self.0.touch("lookup:http")?;
        Ok(Rc::new(RequestCall {
            h: self.0.clone(),
            verb,
        }))
    }
    fn interceptors(&self) -> SigningResult<AccountValue> {
        let h = &self.0;
        h.touch("owner:registry")?;
        let n = h.registry_reads.get() + 1;
        h.registry_reads.set(n);
        if h.scenario() == "rhsReplace" && n == 2 {
            *h.registry.borrow_mut() = "J1".into();
        }
        Ok(h.object(&h.registry.borrow()))
    }
    fn property(&self, v: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        self.0.property(v, key)
    }
    fn set_property(&self, v: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        let h = &self.0;
        let id = h.identify(v).unwrap();
        h.touch(&format!("set:{id}:{key}"))?;
        h.touch(&format!("descriptor:{id}:{key}"))?;
        h.objects[&id].set(key, value);
        Ok(())
    }
    fn empty_array(&self) -> SigningResult<AccountValue> {
        self.0.empty_created.set(true);
        *self.0.objects["E"].array.borrow_mut() = Some(vec![]);
        Ok(self.0.object("E"))
    }
    fn push_callback(&self, value: &AccountValue) -> SigningResult<Rc<dyn AccessCall>> {
        let id = self.0.identify(value).unwrap();
        if id == "A" {
            self.0.property(value, "push")?;
        } else {
            self.0.touch(&format!("lookup:push:{id}"))?;
        }
        Ok(Rc::new(Push {
            h: self.0.clone(),
            id,
        }))
    }
    fn config(&self) -> SigningResult<AccountValue> {
        self.0.touch("owner:config")?;
        Ok(match self.0.scenario() {
            "nullConfig" => AccountValue::Null,
            "undefinedConfig" => AccountValue::Undefined,
            _ => self.0.object(&self.0.config.borrow()),
        })
    }
    fn copy_data_properties(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        let h = &self.0;
        if let Some(id) = h.identify(value) {
            h.touch(&format!("keys:{id}"))?;
            for (key, v, enumerable) in h.objects[&id].props.borrow().clone() {
                h.touch(&format!("descriptor:{id}:{key}"))?;
                if enumerable {
                    h.touch(&format!("property:{id}:{key}"))?;
                    h.objects["D"].set(&key, v);
                }
            }
            for (key, v) in h.objects[&id].symbols.borrow().clone() {
                h.touch(&format!("descriptor:{id}:Symbol(proof)"))?;
                h.touch(&format!("property:{id}:Symbol(proof)"))?;
                h.objects["D"].symbols.borrow_mut().push((key, v));
            }
        }
        Ok(h.object("D"))
    }
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn http_access_native_identity_state_and_order() {
    let oracle = capture();
    let rows = oracle["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        145,
        "independent authored floor, stopped iteration144"
    );
    let mut counts = std::collections::BTreeMap::new();
    for (i, row) in rows.iter().enumerate() {
        let method = row["method"].as_str().unwrap();
        let label = format!("{method} iteration{i}");
        *counts.entry(method).or_insert(0usize) += 1;
        let h = Fixture::new(row);
        let host = Host(h.clone());
        let input = &row["input"];
        let output = Rc::new(RefCell::new(None));
        if ["get", "post", "put", "delete"].contains(&method) {
            let path = units(&input["path"]["units"]);
            let config = h.decode(&input["config"]);
            let data = h.decode(&input["body"]);
            let p = match method {
                "get" => get(&host, path, config),
                "post" => post(&host, path, data, config),
                "put" => put(&host, path, data, config),
                "delete" => delete(&host, path, config),
                _ => unreachable!(),
            };
            let result = output.clone();
            let fixture = h.clone();
            let dropped = h.scenario() == "dropped";
            p.subscribe(move |value| {
                fixture
                    .touch(if value.is_ok() {
                        "observer"
                    } else {
                        "observer:error"
                    })
                    .unwrap();
                *result.borrow_mut() = Some(if dropped {
                    Ok(AccountValue::Undefined)
                } else {
                    value.map(|v| (*v).clone())
                });
            });
            h.touch("checkpoint:sync").unwrap();
            if let Some(resolver) = h.release.borrow_mut().take() {
                h.touch("release").unwrap();
                Rc::new(Semantics(h.clone())).resolve_return(
                    response(&h),
                    resolver,
                    h.runtime.clone(),
                );
            }
            drop(p);
            h.runtime.drain();
        } else {
            let result = match method {
                "addRequestInterceptor" => {
                    add_request_interceptor(&host, AccountValue::Function(h.functions["I"].clone()))
                        .map(|()| AccountValue::Undefined)
                }
                "addResponseInterceptor" => add_response_interceptor(
                    &host,
                    AccountValue::Function(h.functions["I"].clone()),
                )
                .map(|()| AccountValue::Undefined),
                "addErrorInterceptor" => {
                    add_error_interceptor(&host, AccountValue::Function(h.functions["I"].clone()))
                        .map(|()| AccountValue::Undefined)
                }
                "getBaseUrl" => get_base_url(&host),
                "getConfig" => get_config(&host),
                _ => panic!("unknown method"),
            };
            h.touch(if result.is_ok() {
                "checkpoint:sync"
            } else {
                "observer:error"
            })
            .unwrap();
            *output.borrow_mut() = Some(result);
        }
        match output.borrow_mut().take().expect("detached body completes") {
            Ok(v) => {
                assert!(row["error"].is_null(), "{label} native success");
                h.assert_value(&v, &row["result"], &format!("{label} result"));
                if method == "getConfig" {
                    let AccountValue::Object(v) = v else {
                        panic!("source fresh object")
                    };
                    let c: Rc<dyn AccountObject> = h.objects["C"].clone();
                    assert!(!Rc::ptr_eq(&v, &c), "{label} native fresh copy identity");
                }
            }
            Err(TransactionSigningError::Thrown(v)) => {
                assert_eq!(row["error"]["kind"], "token", "{label} native throw kind");
                assert!(v.ptr_eq(&h.token), "{label} native exception identity")
            }
            Err(TransactionSigningError::Generated(error)) => {
                assert_eq!(row["error"]["kind"], "generated");
                assert_eq!(
                    error.name,
                    row["error"]["name"].as_str().unwrap(),
                    "{label} native error name"
                );
                assert_eq!(
                    error.message,
                    units(&row["error"]["message"]),
                    "{label} native error UTF16"
                )
            }
        }
        for id in ["J0", "J1"] {
            for key in ["request", "response", "error"] {
                h.assert_value(
                    &h.objects[id].get(key).unwrap(),
                    &row[id][key],
                    &format!("{label} {id}.{key}"),
                );
            }
        }
        assert_eq!(
            *h.registry.borrow(),
            row["currentRegistry"].as_str().unwrap(),
            "{label} native owner registry"
        );
        assert_eq!(
            *h.config.borrow(),
            row["currentConfig"].as_str().unwrap(),
            "{label} native owner config"
        );
        for (id, want) in row["arrays"].as_object().unwrap() {
            let object = &h.objects[id];
            let array = object.array.borrow();
            let values = array.as_ref().unwrap();
            let want = want.as_array().unwrap();
            assert_eq!(values.len(), want.len(), "{label} native registered length");
            for (value, w) in values.iter().zip(want) {
                h.assert_value(value, w, &label)
            }
        }
        if row["result"]["id"] == "D" {
            let props = h.objects["D"].props.borrow();
            let wanted = row["copied"].as_array().unwrap();
            assert_eq!(
                props.len(),
                wanted.len(),
                "{label} native copied field count"
            );
            for ((key, value, enumerable), w) in props.iter().zip(wanted) {
                assert!(*enumerable);
                assert_eq!(u(key), units(&w["key"]), "{label} native copied key");
                h.assert_value(value, &w["value"], &label);
            }
            let symbols = h.objects["D"].symbols.borrow();
            let wanted = row["symbols"].as_array().unwrap();
            assert_eq!(
                symbols.len(),
                wanted.len(),
                "{label} native copied symbol count"
            );
            for ((key, value), w) in symbols.iter().zip(wanted) {
                assert!(
                    key.ptr_eq(&h.symbol),
                    "{label} native copied symbol key identity"
                );
                assert_eq!(w["key"], "proof");
                h.assert_value(value, &w["value"], &label);
            }
        }
        let calls = h.calls.borrow();
        let wanted = row["calls"].as_array().unwrap();
        assert_eq!(calls.len(), wanted.len(), "{label} native call count");
        for ((kind, receiver, args), w) in calls.iter().zip(wanted) {
            assert_eq!(
                kind,
                w["kind"].as_str().unwrap(),
                "{label} native call verb"
            );
            assert_eq!(
                receiver.as_deref(),
                w["receiver"].as_str(),
                "{label} native push receiver"
            );
            let wa = w["args"].as_array().unwrap();
            assert_eq!(args.len(), wa.len(), "{label} native argument count");
            for (j, (value, w)) in args.iter().zip(wa).enumerate() {
                h.assert_value(value, w, &format!("{label} argument{j}"));
            }
        }
        let expected: Vec<String> = row["trace"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().into())
            .collect();
        assert_eq!(
            *h.trace.borrow(),
            expected,
            "{label} native read/call/microtask order"
        );
    }
    assert_eq!(
        counts,
        std::collections::BTreeMap::from([
            ("get", 20),
            ("post", 20),
            ("put", 20),
            ("delete", 20),
            ("addRequestInterceptor", 17),
            ("addResponseInterceptor", 17),
            ("addErrorInterceptor", 17),
            ("getBaseUrl", 7),
            ("getConfig", 7)
        ]),
        "independent per-method floor"
    );
    assert_eq!(
        oracle["contentHash"], "753432cbb560febc15d8b8f2ad47274aa918b8a9c9a815b1d0a56d4f57ff01e6",
        "authored source-only corpus pin after native assertions"
    );
}
