#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_venue_grvt::builder_workflow::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
const MAIN: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
const WALLET: &str = "0xde709f2102306220921060314715629080e2fb77";
const BUILDER: &str = "0xee8f3559f298fffa79b8253437969d15b5dccff3";
#[derive(Clone)]
enum N {
    U,
    Null,
    B(bool),
    Num(f64),
    S(String),
    A(Rc<RefCell<Vec<N>>>),
    O(Rc<RefCell<IndexMap<String, N>>>),
}
impl N {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::B(v),
            Value::Number(v) => Self::Num(v.as_f64().unwrap()),
            Value::String(v) => Self::S(v),
            Value::Array(v) => Self::A(Rc::new(RefCell::new(
                v.into_iter().map(Self::from).collect(),
            ))),
            Value::Object(v) => Self::O(Rc::new(RefCell::new(
                v.into_iter().map(|(k, v)| (k, Self::from(v))).collect(),
            ))),
        }
    }
    fn object(fields: Vec<(&str, N)>) -> Self {
        Self::O(Rc::new(RefCell::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )))
    }
    fn get(&self, key: &str) -> N {
        match self {
            Self::O(v) => v.borrow().get(key).cloned().unwrap_or(Self::U),
            Self::A(v) if key == "length" => Self::Num(v.borrow().len() as f64),
            Self::A(v) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| v.borrow().get(i).cloned())
                .unwrap_or(Self::U),
            _ => Self::U,
        }
    }
    fn set(&self, key: &str, value: N) {
        let Self::O(v) = self else { panic!("object") };
        v.borrow_mut().insert(key.into(), value);
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::O(a), Self::O(b)) => Rc::ptr_eq(a, b),
            (Self::A(a), Self::A(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn text(&self) -> String {
        match self {
            Self::U => "undefined".into(),
            Self::Null => "null".into(),
            Self::B(v) => v.to_string(),
            Self::Num(v) => safe_cex_num::js_number_string(*v),
            Self::S(v) => v.clone(),
            Self::O(_) => "[object Object]".into(),
            Self::A(v) => v
                .borrow()
                .iter()
                .map(|v| {
                    if matches!(v, Self::U | Self::Null) {
                        String::new()
                    } else {
                        v.text()
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        }
    }
    fn wire(&self) -> safe_cex_num::JsJsonValue {
        use safe_cex_num::JsJsonValue as J;
        match self {
            Self::U => J::Undefined,
            Self::Null => J::Null,
            Self::B(v) => J::Bool(*v),
            Self::Num(v) => J::Number(*v),
            Self::S(v) => J::String(v.clone()),
            Self::A(v) => J::Array(v.borrow().iter().map(Self::wire).collect()),
            Self::O(v) => J::Object(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.wire()))
                    .collect(),
            ),
        }
    }
}
fn scalar(v: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(v.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            format!("{:016x}", v.to_bits()),
            source["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn native(v: &N, source: &Value, label: &str) {
    match v {
        N::U => assert_eq!(source["undefined"], true, "{label} native own undefined"),
        N::Null => assert!(source.is_null(), "{label} native null"),
        N::B(v) => assert_eq!(Some(*v), source.as_bool(), "{label} native boolean"),
        N::Num(v) => scalar(*v, source, label),
        N::S(v) => assert_eq!(Some(v.as_str()), source.as_str(), "{label} native string"),
        N::A(v) => {
            let s = source.as_array().expect(label);
            assert_eq!(v.borrow().len(), s.len(), "{label} native array shape");
            for (v, s) in v.borrow().iter().zip(s) {
                native(v, s, label);
            }
        }
        N::O(v) => {
            let s = source.as_object().expect(label);
            let values = v.borrow();
            assert_eq!(values.len(), s.len(), "{label} native object shape");
            for (k, v) in values.iter() {
                assert!(s.contains_key(k), "{label} native own key {k}");
                native(v, &s[k], label);
            }
        }
    }
}
fn err(name: &str, msg: &str) -> N {
    N::object(vec![
        ("name", N::S(name.into())),
        ("message", N::S(msg.into())),
    ])
}
fn rejected(msg: &str, code: N) -> N {
    let e = err("Error", msg);
    e.set("code", code);
    e
}
fn corpus() -> &'static Value {
    static X: OnceLock<Value> = OnceLock::new();
    X.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/builder-workflow-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "BUILDER source {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
struct Delay<V> {
    first: bool,
    result: Option<JsResult<V>>,
}
impl<V: Unpin> Future for Delay<V> {
    type Output = JsResult<V>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.first {
            self.first = true;
            Poll::Pending
        } else {
            Poll::Ready(self.result.take().unwrap())
        }
    }
}
fn delayed(v: JsResult<N>) -> JsPromise<N> {
    Box::pin(Delay {
        first: false,
        result: Some(v),
    })
}
struct AbortFetch(N);
impl Future for AbortFetch {
    type Output = JsResult<N>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if matches!(self.0.get("aborted"), N::B(true)) {
            Poll::Ready(Err(err("AbortError", "aborted")))
        } else {
            Poll::Pending
        }
    }
}
struct Tick(bool);
impl Future for Tick {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        if !self.0 {
            self.0 = true;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    }
}
struct Job {
    future: RefCell<Option<JsPromise<N>>>,
    result: RefCell<Option<JsResult<N>>>,
}
struct Handle {
    job: Rc<Job>,
    first: bool,
}
impl Future for Handle {
    type Output = JsResult<N>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.first {
            self.first = true;
            return Poll::Pending;
        }
        self.job
            .result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
struct Iter {
    value: N,
    index: Cell<usize>,
}
struct Host {
    group: String,
    name: String,
    events: RefCell<Vec<N>>,
    requests: RefCell<Vec<FetchRequest<N>>>,
    payloads: RefCell<Vec<N>>,
    signs: RefCell<Vec<(N, N, N)>>,
    types: N,
    ethereum: N,
    proxy: N,
    timers: RefCell<IndexMap<u64, N>>,
    controllers: RefCell<Vec<N>>,
    jobs: RefCell<Vec<Rc<Job>>>,
}
impl Host {
    fn new(group: &str, name: &str) -> Rc<Self> {
        Rc::new(Self {
            group: group.into(),
            name: name.into(),
            events: RefCell::new(vec![]),
            requests: RefCell::new(vec![]),
            payloads: RefCell::new(vec![]),
            signs: RefCell::new(vec![]),
            types: N::object(vec![(
                "AuthorizeBuilder",
                N::from(
                    json!([{"name":"mainAccountID","type":"address"},{"name":"builderAccountID","type":"address"},{"name":"maxFutureFeeRate","type":"uint32"},{"name":"maxSpotFeeRate","type":"uint32"},{"name":"nonce","type":"uint32"},{"name":"expiration","type":"int64"}]),
                ),
            )]),
            ethereum: N::from(json!({"identity":"ethereum"})),
            proxy: N::object(vec![
                ("Zulu", N::S("z".into())),
                ("Alpha", N::S("a".into())),
                ("Content-Type", N::S("override".into())),
                ("X-Grvt-Remember-Me", N::S("false".into())),
            ]),
            timers: RefCell::new(IndexMap::new()),
            controllers: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
        })
    }
    fn event(&self, v: Value) {
        self.events.borrow_mut().push(N::from(v));
    }
    fn event_values(&self, v: Vec<N>) {
        self.events
            .borrow_mut()
            .push(N::A(Rc::new(RefCell::new(v))));
    }
    fn poll_job(&self, j: &Rc<Job>) {
        let Some(mut f) = j.future.borrow_mut().take() else {
            return;
        };
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(v) => *j.result.borrow_mut() = Some(v),
            Poll::Pending => *j.future.borrow_mut() = Some(f),
        }
    }
    fn drive(&self) {
        for _ in 0..1000 {
            let jobs = self.jobs.borrow().clone();
            if jobs.iter().all(|j| j.result.borrow().is_some()) {
                return;
            }
            for job in jobs {
                self.poll_job(&job);
            }
        }
        panic!("BUILDER owned task did not settle");
    }
    fn response(&self, k: &str) -> N {
        let bad_http = self.name == "http-error" && (self.group == "auth" || k == "authorize");
        let ok = !(self.name == "non-ok" || bad_http || self.name == "auth-failed" && k == "auth");
        N::object(vec![
            ("kind", self.string(k)),
            ("ok", self.boolean(ok)),
            ("status", self.number(if ok { 200.0 } else { 403.0 })),
        ])
    }
    fn data(&self, k: &str) -> N {
        if self.name == "json-null" || self.name == "null-json" {
            return N::Null;
        }
        if self.name == "http-error" && (self.group == "auth" || k == "authorize") {
            return N::from(json!({"message":"declined"}));
        }
        if self.name == "funding-alias" {
            return N::from(
                json!({"funding_account_address":13,"fundingAccountAddress":"","main_account_id":MAIN}),
            );
        }
        match k {
            "auth" => {
                if ["summary-fallback", "summary-missing"].contains(&self.name.as_str()) {
                    N::object(vec![])
                } else {
                    N::from(json!({"funding_account_address":MAIN}))
                }
            }
            "summary" => {
                if self.name == "summary-missing" {
                    N::object(vec![])
                } else if self.name == "lite" {
                    N::from(json!({"r":{"ma":MAIN}}))
                } else {
                    N::from(json!({"result":{"main_account_id":MAIN}}))
                }
            }
            "check" => match self.name.as_str() {
                "lite" => N::from(json!({"r":[{"ba":BUILDER,"mfr":"0.05"}]})),
                "missing" => N::from(json!({"results":[]})),
                "missing-fee" => N::from(json!({"results":[{"ba":BUILDER}]})),
                "prefix-fee" => N::from(json!({"results":[{"ba":BUILDER,"mfr":"0.031junk"}]})),
                "overflow-fee" => N::from(json!({"results":[{"ba":BUILDER,"mfr":"1e308"}]})),
                "nonstring-addr" => N::from(json!({"results":[{"ba":7}]})),
                _ => N::from(
                    json!({"results":[{"builder_account_id":BUILDER,"max_futures_fee_rate":"0.03"}]}),
                ),
            },
            _ => {
                if self.name == "false-flags" {
                    N::from(json!({"success":false,"s":false}))
                } else {
                    N::from(json!({"success":true}))
                }
            }
        }
    }
}
impl BuilderHost for Host {
    type Value = N;
    type Iterator = Iter;
    fn start(&self, task: JsPromise<N>) -> JsPromise<N> {
        let job = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        self.poll_job(&job);
        Box::pin(Handle { job, first: false })
    }
    fn reaction(&self) -> Reaction {
        Box::pin(Tick(false))
    }
    fn string(&self, v: &str) -> N {
        N::S(v.into())
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn null(&self) -> N {
        N::Null
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn object(&self, v: Vec<(&'static str, N)>) -> N {
        N::object(v)
    }
    fn array(&self, v: Vec<N>) -> N {
        N::A(Rc::new(RefCell::new(v)))
    }
    fn property(&self, v: &N, key: &str) -> JsResult<N> {
        if matches!(v, N::U | N::Null) {
            return Err(err(
                "TypeError",
                &format!("Cannot read properties of {} (reading '{key}')", v.text()),
            ));
        }
        if self.name == "getter-error" && key == "ethereum" {
            return Err(err("Error", "getter-fail"));
        }
        if self.name == "candidate-error" && key == "ma" {
            return Err(err("Error", "candidate-fail"));
        }
        Ok(v.get(key))
    }
    fn set(&self, v: &N, key: &'static str, value: N) -> JsResult<N, ()> {
        v.set(key, value);
        Ok(())
    }
    fn spread(&self, target: &N, source: &N) -> JsResult<N, ()> {
        if let N::O(v) = source {
            for (k, v) in v.borrow().iter() {
                target.set(k, v.clone());
            }
        }
        Ok(())
    }
    fn truthy(&self, v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(v) => *v,
            N::Num(v) => *v != 0.0 && !v.is_nan(),
            N::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn is_null(&self, v: &N) -> bool {
        matches!(v, N::Null)
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn is_string(&self, v: &N) -> bool {
        matches!(v, N::S(_))
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        match (a, b) {
            (N::U, N::U) | (N::Null, N::Null) => true,
            (N::B(a), N::B(b)) => a == b,
            (N::S(a), N::S(b)) => a == b,
            (N::Num(a), N::Num(b)) => a == b,
            _ => a.same(b),
        }
    }
    fn concat(&self, v: Vec<N>) -> JsResult<N> {
        Ok(self.string(&v.iter().map(N::text).collect::<String>()))
    }
    fn lowercase(&self, v: &N) -> JsResult<N> {
        let N::S(v) = v else {
            return Err(err("TypeError", "toLowerCase is not a function"));
        };
        Ok(self.string(&v.to_lowercase()))
    }
    fn stringify(&self, v: &N) -> JsResult<N> {
        self.payloads.borrow_mut().push(v.clone());
        Ok(self.string(&safe_cex_num::js_json_stringify(&v.wire()).unwrap().unwrap()))
    }
    fn to_string(&self, v: &N) -> JsResult<N> {
        Ok(self.string(&v.text()))
    }
    fn parse_float(&self, v: &N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_float(&v.text()))
    }
    fn parse_int(&self, v: &N, r: i32) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_int(&v.text(), Some(r)))
    }
    fn to_fixed(&self, v: f64, d: f64) -> JsResult<N> {
        safe_cex_num::js_to_fixed(v, d)
            .map(|v| self.string(&v))
            .map_err(|e| err("RangeError", e))
    }
    fn error(&self, message: N) -> N {
        N::object(vec![("name", self.string("Error")), ("message", message)])
    }
    fn window(&self) -> JsResult<N, Option<N>> {
        Ok(
            if ["no-window", "no-wallet"].contains(&self.name.as_str()) {
                None
            } else if self.name == "null-window" {
                Some(N::Null)
            } else {
                Some(N::object(vec![(
                    "ethereum",
                    if self.name == "no-ethereum" {
                        N::U
                    } else {
                        self.ethereum.clone()
                    },
                )]))
            },
        )
    }
    fn read_window(&self) -> JsResult<N> {
        Ok(self.window()?.unwrap_or(N::U))
    }
    fn browser_provider(&self, e: &N) -> JsResult<N> {
        assert!(e.same(&self.ethereum), "WALLET native provider identity");
        self.event(json!(["provider"]));
        if self.name == "construct-error" {
            Err(err("Error", "construct-fail"))
        } else {
            Ok(N::from(json!({"provider":true})))
        }
    }
    fn provider_send(&self, _: &N, method: &'static str, args: N) -> JsResult<N, JsPromise<N>> {
        self.event_values(vec![self.string("send"), self.string(method), args]);
        let result = if self.name == "send-error" {
            Err(err("Error", "send-fail"))
        } else if self.name == "null-error" {
            Err(N::Null)
        } else if ["eth_accounts", "eth_requestAccounts"].contains(&method) {
            match self.name.as_str() {
                "reject-number" => Err(rejected("request-fail", N::Num(4001.0))),
                "reject-string" => Err(rejected("request-fail", self.string("4001"))),
                "empty" | "no-accounts" => Ok(self.array(vec![])),
                "null-accounts" => Ok(N::Null),
                _ => Ok(self.array(vec![self.string(WALLET)])),
            }
        } else if method == "wallet_switchEthereumChain" && self.name == "switch-rejected" {
            let e = err("Error", "switch-fail");
            e.set("error", N::from(json!({"code":4001})));
            Err(e)
        } else if method == "wallet_switchEthereumChain"
            && ["add-success", "add-rejected", "add-failed"].contains(&self.name.as_str())
        {
            Err(rejected("switch-fail", N::Num(4902.0)))
        } else if method == "wallet_addEthereumChain" && self.name == "add-rejected" {
            Err(rejected("add-fail", self.string("ACTION_REJECTED")))
        } else if method == "wallet_addEthereumChain" && self.name == "add-failed" {
            Err(rejected("add-fail", N::Num(4902.0)))
        } else {
            Ok(N::Null)
        };
        Ok(delayed(result))
    }
    fn signer(&self, _: &N) -> JsResult<N, JsPromise<N>> {
        self.event(json!(["signer"]));
        if self.name == "types-mutation" {
            let N::A(types) = self.types.get("AuthorizeBuilder") else {
                panic!()
            };
            types.borrow()[0].set("name", self.string("changed"));
        }
        Ok(delayed(Ok(N::from(json!({"signer":true})))))
    }
    fn authorize_builder_types(&self) -> N {
        self.types.clone()
    }
    fn sign(&self, _: &N, domain: N, types: N, message: N) -> JsResult<N, JsPromise<N>> {
        assert!(types.same(&self.types), "SIGN native module singleton");
        self.event(json!(["sign"]));
        self.signs.borrow_mut().push((domain, types, message));
        Ok(delayed(if self.name == "sign-rejected" {
            Err(rejected("sign-fail", self.string("ACTION_REJECTED")))
        } else {
            Ok(self.string(&if self.name == "short-signature" {
                "0xabc".into()
            } else {
                format!("0x{}{}1b", "11".repeat(32), "22".repeat(32))
            }))
        }))
    }
    fn slice(&self, v: &N, start: usize, end: Option<usize>) -> JsResult<N> {
        let N::S(v) = v else {
            return Err(err("TypeError", "signature.slice is not a function"));
        };
        let units = v.encode_utf16().collect::<Vec<_>>();
        let a = start.min(units.len());
        let b = end.unwrap_or(units.len()).min(units.len()).max(a);
        Ok(self.string(&String::from_utf16(&units[a..b]).unwrap()))
    }
    fn edge_url(&self, t: bool) -> JsResult<N> {
        self.event(json!(["edge", t]));
        Ok(self.string(if t {
            "https://edge.test"
        } else {
            "https://edge.prod"
        }))
    }
    fn trading_url(&self, t: bool) -> JsResult<N> {
        self.event(json!(["trading", t]));
        Ok(self.string(if t {
            "https://trade.test"
        } else {
            "https://trade.prod"
        }))
    }
    fn proxy_headers(&self) -> JsResult<N, JsPromise<N>> {
        self.event(json!(["proxy-headers"]));
        Ok(delayed(if self.name == "proxy-error" {
            Err(err("Error", "proxy-fail"))
        } else {
            Ok(self.proxy.clone())
        }))
    }
    fn controller(&self) -> JsResult<N> {
        let id = self.controllers.borrow().len() + 1;
        self.event(json!(["controller", id]));
        let signal = N::from(json!({"id":id,"aborted":false}));
        let c = N::object(vec![("id", N::Num(id as f64)), ("signal", signal)]);
        self.controllers.borrow_mut().push(c.clone());
        Ok(c)
    }
    fn timeout(&self, c: &N, delay: f64) -> JsResult<N> {
        assert_eq!(delay, 10000.0, "HTTP native deadline");
        let id = self.timers.borrow().len() + 1;
        self.event(json!(["timer", id, delay as u64]));
        self.timers.borrow_mut().insert(id as u64, c.clone());
        Ok(N::Num(id as f64))
    }
    fn clear_timeout(&self, t: &N) -> JsResult<N, ()> {
        let N::Num(id) = t else { panic!() };
        self.event(json!(["clear", *id as u64]));
        if ["clear-error", "finally-error"].contains(&self.name.as_str()) {
            return Err(err("Error", "clear-fail"));
        }
        self.timers.borrow_mut().shift_remove(&(*id as u64));
        Ok(())
    }
    fn fetch(&self, r: FetchRequest<N>) -> JsResult<N, JsPromise<N>> {
        assert_eq!(r.method, "POST", "HTTP native method");
        let url = r.url.text();
        let k = if url.contains("/login") {
            "auth"
        } else if url.contains("aggregated_") {
            "summary"
        } else if url.contains("get_authorized") {
            "check"
        } else {
            "authorize"
        };
        if let Some(signal) = &r.signal {
            assert!(
                self.controllers
                    .borrow()
                    .iter()
                    .any(|c| c.get("signal").same(signal)),
                "HTTP native signal identity"
            );
        }
        self.event(json!(["fetch", k]));
        let signal = r.signal.clone();
        self.requests.borrow_mut().push(r);
        if self.name == "fetch-sync-error" {
            return Err(err("Error", "fetch-fail"));
        }
        let result = if self.name == "fetch-error"
            && (self.group == "auth" || self.group == "main" || k == "check")
        {
            Err(err("Error", "fetch-fail"))
        } else {
            Ok(self.response(k))
        };
        if self.name == "alias-mutation" && k == "auth" {
            self.proxy.set("Alpha", self.string("later"));
        }
        if self.name == "abort" {
            Ok(Box::pin(AbortFetch(signal.unwrap())))
        } else {
            Ok(delayed(result))
        }
    }
    fn json(&self, r: &N) -> JsResult<N, JsPromise<N>> {
        let k = r.get("kind").text();
        self.event(json!(["json", k]));
        if self.name == "json-sync-error" {
            return Err(err("Error", "json-fail"));
        }
        Ok(delayed(if self.name == "json-reject" {
            Err(err("Error", "json-fail"))
        } else {
            Ok(self.data(&k))
        }))
    }
    fn response_header(&self, _: &N, key: &'static str) -> JsResult<N> {
        assert!(
            ["x-grvt-account-id", "x-ts-grvt-session"].contains(&key),
            "HTTP native header getter key"
        );
        self.event(json!(["header", key]));
        if self.name == "header-error" {
            return Err(err("Error", "header-fail"));
        }
        Ok(self.string(if key == "x-grvt-account-id" {
            if self.name == "no-account" {
                ""
            } else {
                "acct-1"
            }
        } else if self.name == "no-cookie" {
            ""
        } else {
            "cookie-1"
        }))
    }
    fn is_address(&self, v: &N) -> bool {
        matches!(v,N::S(v)if safe_cex_crypto::ethereum::get_address(v).is_ok())
    }
    fn iterator(&self, v: &N) -> JsResult<N, Iter> {
        if !matches!(v, N::A(_)) {
            return Err(err("TypeError", "builders is not iterable"));
        }
        Ok(Iter {
            value: v.clone(),
            index: Cell::new(0),
        })
    }
    fn next(&self, i: &Iter) -> JsResult<N, Option<N>> {
        let N::A(v) = &i.value else { panic!() };
        let n = i.index.get();
        i.index.set(n + 1);
        Ok(v.borrow().get(n).cloned())
    }
    fn close_iterator(&self, _: &Iter, result: JsResult<N>) -> JsResult<N> {
        result
    }
    fn random(&self) -> JsResult<N, f64> {
        self.event(json!(["random"]));
        Ok(0.5)
    }
    fn now(&self) -> JsResult<N, f64> {
        self.event(json!(["now"]));
        Ok(1700000000000.0)
    }
    fn fee_to_raw(&self, v: &N) -> JsResult<N, f64> {
        safe_cex_venue_grvt::builder::grvt_builder_fee_rate_to_raw(&v.text())
            .map_err(|e| err("Error", &e))
    }
}
fn outcome(result: JsResult<N>, row: &Value, label: &str) {
    match result {
        Ok(v) => {
            assert!(row["error"].is_null(), "{label} unexpected native success");
            native(&v, &row["result"], label);
        }
        Err(e) => {
            assert!(
                !row["error"].is_null(),
                "{label} unexpected native rejection"
            );
            for key in ["name", "message", "code"] {
                native(&e.get(key), &row["error"][key], label);
            }
        }
    }
}
fn cases(group: &str) -> Vec<&'static str> {
    match group {
        "provider" | "available" => vec![
            "no-window",
            "no-ethereum",
            "present",
            "getter-error",
            "null-window",
        ],
        "connected" => vec![
            "no-window",
            "present",
            "empty",
            "send-error",
            "construct-error",
            "getter-error",
            "null-accounts",
        ],
        "request" => vec![
            "no-window",
            "present",
            "empty",
            "send-error",
            "reject-number",
            "reject-string",
            "construct-error",
            "null-error",
        ],
        "auth" => vec![
            "ok",
            "no-account",
            "no-cookie",
            "funding-alias",
            "http-error",
            "json-reject",
            "json-null",
            "fetch-error",
            "fetch-sync-error",
            "abort",
            "clear-error",
            "header-error",
        ],
        "main" => vec![
            "ok",
            "lite",
            "non-ok",
            "json-reject",
            "json-null",
            "fetch-error",
            "fetch-sync-error",
            "finally-error",
            "candidate-error",
        ],
        "check" => vec![
            "ok",
            "lite",
            "missing",
            "missing-fee",
            "prefix-fee",
            "overflow-fee",
            "nonstring-addr",
            "fetch-error",
            "proxy-error",
            "null-json",
            "auth-failed",
            "alias-mutation",
        ],
        _ => vec![
            "ok",
            "testnet",
            "no-wallet",
            "invalid-builder",
            "no-key",
            "proxy-error",
            "auth-failed",
            "no-accounts",
            "switch-rejected",
            "add-success",
            "add-rejected",
            "add-failed",
            "summary-fallback",
            "summary-missing",
            "sign-rejected",
            "short-signature",
            "http-error",
            "false-flags",
            "types-mutation",
            "drop-result",
            "fee-error",
        ],
    }
}
fn request_count(group: &str, i: usize) -> usize {
    match group {
        "auth" | "main" => 1,
        "check" => [2, 2, 2, 2, 2, 2, 2, 2, 0, 1, 1, 2][i],
        "authorize" => [
            2, 2, 0, 0, 0, 0, 1, 1, 1, 2, 1, 1, 3, 2, 1, 2, 2, 2, 2, 2, 1,
        ][i],
        _ => 0,
    }
}
fn sign_count(group: &str, i: usize) -> usize {
    if group == "authorize" {
        [
            1, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 0,
        ][i]
    } else {
        0
    }
}
fn pending(group: &str, name: &str) -> usize {
    usize::from(
        ["clear-error", "finally-error"].contains(&name)
            || group == "check" && name == "fetch-error",
    )
}
fn check_calls(h: &Host, row: &Value, requests: usize, signs: usize) {
    assert_eq!(
        h.requests.borrow().len(),
        requests,
        "HTTP independent native request count"
    );
    assert_eq!(
        row["requests"].as_array().unwrap().len(),
        requests,
        "HTTP independent source request count"
    );
    assert_eq!(
        h.payloads.borrow().len(),
        requests,
        "HTTP independent native payload count"
    );
    assert_eq!(
        row["payloads"].as_array().unwrap().len(),
        requests,
        "HTTP independent source payload count"
    );
    for (i, (r, s)) in h
        .requests
        .borrow()
        .iter()
        .zip(row["requests"].as_array().unwrap())
        .enumerate()
    {
        native(&r.url, &s["url"], "HTTP native URL");
        assert_eq!(
            r.method,
            s["method"].as_str().unwrap(),
            "HTTP native method"
        );
        native(&r.headers, &s["headers"], "HTTP native headers");
        let N::O(headers) = &r.headers else {
            panic!("HTTP native header object")
        };
        let expected = if s["kind"] == "auth" {
            vec!["Content-Type", "Zulu", "Alpha", "X-Grvt-Remember-Me"]
        } else {
            vec![
                "Content-Type",
                "Zulu",
                "Alpha",
                "X-Grvt-Remember-Me",
                "X-Grvt-Account-Id",
                "X-Ts-Grvt-Session",
            ]
        };
        assert_eq!(
            headers.borrow().len(),
            expected.len(),
            "HTTP independent header count"
        );
        let actual = headers.borrow().keys().cloned().collect::<Vec<_>>();
        assert_eq!(actual, expected, "HTTP native header order");
        assert_eq!(s["headerKeys"], json!(expected), "HTTP source header order");
        native(&r.payload, &row["payloads"][i], "HTTP native payload");
        native(
            &h.payloads.borrow()[i],
            &row["payloads"][i],
            "HTTP native pre-stringify payload",
        );
        let include = s["credentials"] == "include";
        assert_eq!(r.credentials, include, "HTTP native credentials");
        if !include {
            assert_eq!(
                s["credentials"]["undefined"], true,
                "HTTP source omitted credentials"
            );
        }
        let signal = r.signal.as_ref().map(|v| v.get("id")).unwrap_or(N::U);
        native(&signal, &s["signal"], "HTTP native signal ID");
        let keys = match &r.payload {
            N::O(v) => v.borrow().len(),
            _ => panic!("HTTP native object payload"),
        };
        assert_eq!(
            keys,
            match s["kind"].as_str().unwrap() {
                "auth" => 1,
                "authorize" => 5,
                _ => 0,
            },
            "HTTP independent payload field count"
        );
    }
    assert_eq!(
        h.signs.borrow().len(),
        signs,
        "SIGN independent native count"
    );
    assert_eq!(
        row["signs"].as_array().unwrap().len(),
        signs,
        "SIGN independent source count"
    );
    for ((domain, types, message), source) in h
        .signs
        .borrow()
        .iter()
        .zip(row["signs"].as_array().unwrap())
    {
        native(domain, &source["domain"], "SIGN native domain");
        native(types, &source["types"], "SIGN native types");
        native(message, &source["message"], "SIGN native message");
        assert!(types.same(&h.types), "SIGN native retained types identity");
        assert_eq!(source["sameTypes"], true, "SIGN source module identity");
        let N::O(domain) = domain else { panic!() };
        assert_eq!(domain.borrow().len(), 3, "SIGN independent domain count");
        let N::O(message) = message else { panic!() };
        assert_eq!(message.borrow().len(), 6, "SIGN independent message count");
        let N::A(fields) = types.get("AuthorizeBuilder") else {
            panic!()
        };
        assert_eq!(
            fields.borrow().len(),
            6,
            "SIGN independent typed field count"
        );
    }
}
fn matrix(group: &str) {
    let names = cases(group);
    let rows = corpus()[group].as_array().unwrap();
    let floor = match group {
        "provider" | "available" => 5,
        "connected" => 7,
        "request" => 8,
        "auth" => 12,
        "main" => 9,
        "check" => 12,
        _ => 21,
    };
    assert_eq!(names.len(), floor, "BUILDER independent expectation floor");
    assert_eq!(rows.len(), floor, "BUILDER source floor");
    let mut wire_pairs = vec![];
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i], "BUILDER case order");
        let h = Host::new(group, names[i]);
        let mut synchronous = None;
        let mut promise = match group {
            "provider" => {
                synchronous = Some(get_ethereum_provider(h.as_ref()));
                None
            }
            "available" => {
                synchronous = Some(is_browser_wallet_available(h.as_ref()).map(N::B));
                None
            }
            "connected" => Some(get_connected_wallet_address(h.clone())),
            "request" => Some(request_wallet_connection(h.clone())),
            "auth" => Some(authenticate_grvt(
                h.clone(),
                h.string("key-1"),
                false,
                h.proxy.clone(),
            )),
            "main" => Some(fetch_grvt_main_account_id(
                h.clone(),
                false,
                h.proxy.clone(),
                N::object(vec![
                    ("accountId", h.string("acct-1")),
                    ("sessionCookie", h.string("cookie-1")),
                ]),
            )),
            "check" => Some(check_grvt_builder_authorized(
                h.clone(),
                h.string("key-1"),
                None,
                false,
            )),
            _ => Some(authorize_grvt_builder_with_browser_wallet(
                h.clone(),
                h.string(if names[i] == "no-key" { "" } else { "key-1" }),
                h.string(if names[i] == "invalid-builder" {
                    "invalid"
                } else {
                    BUILDER
                }),
                h.string(if names[i] == "fee-error" {
                    "bad"
                } else {
                    "0.03"
                }),
                names[i] == "testnet",
            )),
        };
        native(
            &N::A(Rc::new(RefCell::new(h.events.borrow().clone()))),
            &row["initial"],
            "BUILDER native eager prefix",
        );
        if names[i] == "abort" {
            let job = h.jobs.borrow()[0].clone();
            for _ in 0..3 {
                h.poll_job(&job);
                assert!(
                    job.result.borrow().is_none(),
                    "ABORT native pending before timer"
                );
            }
            assert_eq!(
                row["abortPending"], true,
                "ABORT source pending before timer"
            );
            let timers = h.timers.borrow().values().cloned().collect::<Vec<_>>();
            assert_eq!(timers.len(), 1, "HTTP independent abort timer count");
            let c = &timers[0];
            c.get("signal").set("aborted", N::B(true));
            let N::Num(id) = c.get("id") else { panic!() };
            h.event(json!(["abort", id as u64]));
        }
        if names[i] == "drop-result" {
            drop(promise.take());
        }
        h.drive();
        let result =
            synchronous.unwrap_or_else(|| h.jobs.borrow()[0].result.borrow().clone().unwrap());
        if group == "provider" && names[i] == "present" {
            assert!(
                result
                    .as_ref()
                    .unwrap_or_else(|_| panic!())
                    .same(&h.ethereum),
                "PROVIDER native identity"
            );
            assert_eq!(row["providerIdentity"], true, "PROVIDER source identity");
        }
        outcome(
            result,
            row,
            match group {
                "provider" => "PROVIDER native result",
                "available" => "AVAILABLE native result",
                "connected" => "CONNECTED native address",
                "request" => "REQUEST native wallet outcome",
                "auth" => "AUTH native result",
                "main" => "MAIN native address",
                "check" => "CHECK native result",
                _ => "AUTHORIZE native result",
            },
        );
        let rq = request_count(group, i);
        let sg = sign_count(group, i);
        check_calls(&h, row, rq, sg);
        let pending = pending(group, names[i]);
        assert_eq!(
            h.timers.borrow().len(),
            pending,
            "HTTP independent native pending timer count"
        );
        assert_eq!(
            row["pendingTimers"].as_u64().unwrap() as usize,
            pending,
            "HTTP independent source pending timer count"
        );
        native(
            &N::A(Rc::new(RefCell::new(h.events.borrow().clone()))),
            &row["events"],
            "BUILDER native operation order",
        );
        let retained = h.timers.borrow().values().cloned().collect::<Vec<_>>();
        for controller in retained {
            let id = match controller.get("id") {
                N::Num(id) => id as usize,
                _ => panic!("HTTP retained controller ID"),
            };
            assert!(
                controller.same(&h.controllers.borrow()[id - 1]),
                "HTTP retained controller identity"
            );
            controller.get("signal").set("aborted", N::B(true));
        }
        let aborted = h
            .controllers
            .borrow()
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                matches!(c.get("signal").get("aborted"), N::B(true)).then_some(i + 1)
            })
            .collect::<Vec<_>>();
        let expected_aborted = if group == "check" && names[i] == "fetch-error" {
            vec![2]
        } else if pending == 1 || names[i] == "abort" {
            vec![1]
        } else {
            vec![]
        };
        assert_eq!(
            aborted, expected_aborted,
            "HTTP native abort after workflow completion"
        );
        assert_eq!(
            row["retainedAborts"],
            json!(expected_aborted),
            "HTTP source abort after workflow completion"
        );
        for (r, s) in h
            .requests
            .borrow()
            .iter()
            .zip(row["wires"].as_array().unwrap())
        {
            wire_pairs.push((r.body.text(), s.as_str().unwrap().to_owned()));
        }
    }
    for (native, source) in wire_pairs {
        assert_eq!(native, source, "BUILDER wire after native assertions");
    }
}
#[test]
fn provider_native() {
    matrix("provider")
}
#[test]
fn available_native() {
    matrix("available")
}
#[test]
fn connected_native() {
    matrix("connected")
}
#[test]
fn request_native() {
    matrix("request")
}
#[test]
fn authentication_native() {
    matrix("auth")
}
#[test]
fn main_account_native() {
    matrix("main")
}
#[test]
fn check_native() {
    matrix("check")
}
#[test]
fn authorization_native() {
    matrix("authorize")
}
