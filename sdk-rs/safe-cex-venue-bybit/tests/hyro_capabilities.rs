use safe_cex_venue_bybit::{
    hyro_capabilities as api,
    hyro_private_session::{SessionFuture, SessionHost},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Raw>>),
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        watched: bool,
        tag: Option<&'static str>,
        coercion: Option<bool>,
    },
    Function {
        owner: Raw,
        code: String,
    },
    Error(Vec<u16>),
    Settled(Result<Raw, Raw>),
    Selected {
        owner: Raw,
        callable: Raw,
    },
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn null() -> Raw {
    Rc::new(Node::Null)
}
fn boolean(b: bool) -> Raw {
    Rc::new(Node::Bool(b))
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn text(t: Vec<u16>) -> Raw {
    Rc::new(Node::Text(t))
}
fn raw(s: &str) -> Raw {
    text(u(s))
}
fn lone(s: &str) -> Raw {
    text(u(s).into_iter().chain([0xd800]).collect())
}
fn array(a: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(a)))
}
fn object(fields: Vec<(&str, Raw)>, watched: bool) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        watched,
        tag: None,
        coercion: None,
    })
}
fn with_proto(
    fields: Vec<(Vec<u16>, Raw)>,
    prototype: Option<Raw>,
    watched: bool,
    tag: Option<&'static str>,
    coercion: Option<bool>,
) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields),
        prototype,
        watched,
        tag,
        coercion,
    })
}
fn error(message: &str) -> Raw {
    Rc::new(Node::Error(u(message)))
}
fn get_bare(v: &Raw, key: &[u16]) -> Raw {
    match v.as_ref() {
        Node::Object {
            fields, prototype, ..
        } => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .or_else(|| prototype.as_ref().map(|p| get_bare(p, key)))
            .unwrap_or_else(undef),
        Node::Error(message) if key == u("message") => text(message.clone()),
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        _ => undef(),
    }
}
fn set_bare(v: &Raw, key: &[u16], value: Raw) {
    let Node::Object { fields, .. } = v.as_ref() else {
        panic!("native object write")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = value;
    } else {
        fields.push((key.to_vec(), value));
    }
}
fn truthy(v: &Raw) -> bool {
    match v.as_ref() {
        Node::Undefined | Node::Null => false,
        Node::Bool(b) => *b,
        Node::Number(n) => *n != 0.0 && !n.is_nan(),
        Node::Text(s) => !s.is_empty(),
        _ => true,
    }
}
fn eq(a: &Raw, b: &Raw) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Node::Undefined, Node::Undefined) | (Node::Null, Node::Null) => true,
        (Node::Bool(a), Node::Bool(b)) => a == b,
        (Node::Number(a), Node::Number(b)) => a == b,
        (Node::Text(a), Node::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        Node::Text(t) => t.clone(),
        Node::Array(_) => vec![],
        Node::Object {
            coercion: Some(false),
            ..
        } => u("object-").into_iter().chain([0xd800]).collect(),
        _ => u("[object Object]"),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Object { tag: Some(t), .. } => json!({"ref":t}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
        Node::Error(message) => json!({"error":true,"message":message}),
        Node::Function { code, .. } => json!({"fn":code}),
        Node::Settled(_) | Node::Selected { .. } => panic!("internal selected callable"),
    }
}
fn input(profile: &str) -> Raw {
    match profile {
        "undefined" => undef(),
        "null" => null(),
        "false" => boolean(false),
        "zero" => number(0.0),
        "negative-zero" => number(-0.0),
        "nan" => number(f64::NAN),
        "infinity" => number(f64::INFINITY),
        "empty" => raw(""),
        "true" => boolean(true),
        "lone" => lone("lone-"),
        "array" => array(vec![]),
        "object" => with_proto(vec![], None, false, None, Some(false)),
        "throw-string" => with_proto(vec![], None, false, None, Some(true)),
        _ => lone("text-"),
    }
}
fn settle<F: Future>(future: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(future);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("immediate source Promise unexpectedly pending"),
    }
}
struct Harness {
    method: String,
    profile: String,
    failure: String,
    root: Raw,
    opts: Raw,
    markets: Raw,
    socket1: Raw,
    socket2: Raw,
    returned: Raw,
    primary: Raw,
    counts: RefCell<Vec<(Vec<u16>, usize)>>,
    ops: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Raw>>,
    argument: RefCell<Raw>,
    faulted: RefCell<bool>,
    socket_reads: RefCell<usize>,
}
impl Harness {
    fn new(method: &str, profile: &str, failure: &str) -> Self {
        let socket1 = object(
            vec![("started", boolean(true)), ("ready", boolean(false))],
            true,
        );
        let socket2 = object(
            vec![("started", boolean(true)), ("ready", boolean(true))],
            true,
        );
        let initial = if profile == "not-started" {
            object(
                vec![("started", boolean(false)), ("ready", boolean(false))],
                true,
            )
        } else if profile == "ready" || profile == "valid" {
            socket2.clone()
        } else if profile == "rawtruthy" {
            object(vec![("started", raw("yes")), ("ready", number(0.0))], true)
        } else {
            socket1.clone()
        };
        let initial = if method == "validateOrderCapabilities"
            && !matches!(
                profile,
                "reconnecting" | "rawtruthy" | "fail-return" | "changed-socket" | "multi-fail"
            ) {
            object(
                vec![("started", boolean(false)), ("ready", boolean(false))],
                true,
            )
        } else {
            initial
        };
        let symbol = match profile {
            "empty-symbol" => raw(""),
            "lone-symbol" => lone("BTC"),
            "numeric-symbol" => number(7.0),
            _ => raw("BTCUSDT"),
        };
        let quote = if profile == "otherquote" || profile == "multi-fail" {
            raw("USDC")
        } else if profile == "quote-boxed" {
            object(vec![], false)
        } else {
            raw("USDT")
        };
        let market = object(vec![("quote", quote)], true);
        let markets = with_proto(
            if profile == "missing-market" {
                vec![]
            } else {
                vec![(
                    js(&symbol),
                    if profile == "nullmarket" {
                        null()
                    } else {
                        market.clone()
                    },
                )]
            },
            None,
            true,
            None,
            None,
        );
        let store = if profile == "nullstore" {
            null()
        } else {
            object(vec![("marketsBySymbol", markets.clone())], true)
        };
        let mut fields = vec![(u("symbol"), symbol)];
        let mut proto = None;
        match profile {
            "slip-null" | "multi-fail" => fields.push((u("maxSlippagePct"), null())),
            "slip-zero" => fields.push((u("maxSlippagePct"), number(0.0))),
            "slip-nan" => fields.push((u("maxSlippagePct"), number(f64::NAN))),
            "slip-undefined" => fields.push((u("maxSlippagePct"), undef())),
            "slip-proto" => proto = Some(object(vec![("maxSlippagePct", number(0.0))], false)),
            _ => {}
        }
        if matches!(profile, "bboundef" | "both" | "shadow" | "multi-fail") {
            fields.push((u("bybitBbo"), undef()));
        }
        if matches!(profile, "tickundefined" | "both") {
            fields.push((u("bybitSlippageToleranceTicks"), undef()));
        }
        if matches!(profile, "bboproto" | "shadow") {
            proto = Some(object(
                vec![(
                    "bybitBbo",
                    if profile == "shadow" {
                        boolean(true)
                    } else {
                        undef()
                    },
                )],
                false,
            ));
        }
        if profile == "tickproto" {
            proto = Some(object(
                vec![("bybitSlippageToleranceTicks", undef())],
                false,
            ));
        }
        let opts = match profile {
            "undefinedopts" => undef(),
            "nullopts" => null(),
            "numberopts" => number(7.0),
            "textopts" => raw("BTCUSDT"),
            _ => with_proto(fields, proto, true, None, None),
        };
        Self {
            method: method.into(),
            profile: profile.into(),
            failure: failure.into(),
            root: with_proto(
                vec![(u("privateWebsocket"), initial), (u("store"), store)],
                None,
                true,
                Some("this"),
                None,
            ),
            opts,
            markets,
            socket1,
            socket2,
            returned: with_proto(vec![], None, false, Some("returned"), None),
            primary: with_proto(vec![], None, false, Some("primary"), None),
            counts: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            argument: RefCell::new(undef()),
            faulted: RefCell::new(false),
            socket_reads: RefCell::new(0),
        }
    }
    fn touch_units(&self, name: Vec<u16>, args: Vec<Raw>) -> Result<(), Raw> {
        let mut counts = self.counts.borrow_mut();
        let count = if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k == &name) {
            let old = *n;
            *n += 1;
            old
        } else {
            counts.push((name.clone(), 1));
            0
        };
        if u(&self.failure)
            == name
                .iter()
                .copied()
                .chain(u(&format!("#{count}")))
                .collect::<Vec<_>>()
        {
            *self.faulted.borrow_mut() = true;
            return Err(self.primary.clone());
        }
        drop(counts);
        self.ops
            .borrow_mut()
            .push(json!({"name":name,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        Ok(())
    }
    fn touch(&self, name: &str, args: Vec<Raw>) -> Result<(), Raw> {
        self.touch_units(u(name), args)
    }
    fn read(&self, owner: &Raw, key: Vec<u16>) -> Result<Raw, Raw> {
        if matches!(owner.as_ref(), Node::Null | Node::Undefined) {
            return Err(error(&format!(
                "Cannot read properties of {} (reading '{}')",
                if matches!(owner.as_ref(), Node::Null) {
                    "null"
                } else {
                    "undefined"
                },
                String::from_utf16_lossy(&key)
            )));
        }
        if matches!(owner.as_ref(), Node::Object { watched: true, .. }) {
            self.touch_units(u("get:").into_iter().chain(key.clone()).collect(), vec![])?;
        }
        if Rc::ptr_eq(owner, &self.root) {
            let k = String::from_utf16(&key).unwrap();
            if matches!(
                k.as_str(),
                "fail"
                    | "assertPrivateReady"
                    | "emitAndLogError"
                    | "fetchAndCacheApiKeyInfo"
                    | "fetchApiKeyInfo"
                    | "getAccount"
                    | "fetchFees"
                    | "fetchPositionMode"
                    | "changePositionMode"
                    | "changeMarginType"
                    | "validateOrderCapabilities"
            ) {
                return Ok(Rc::new(Node::Function {
                    owner: owner.clone(),
                    code: if k == "emitAndLogError" {
                        "log".into()
                    } else if k == "assertPrivateReady" {
                        "ready".into()
                    } else {
                        k
                    },
                }));
            }
            if k == "privateWebsocket" && self.profile == "changed-socket" {
                let mut n = self.socket_reads.borrow_mut();
                let out = if *n == 0 {
                    self.socket1.clone()
                } else {
                    self.socket2.clone()
                };
                *n += 1;
                return Ok(out);
            }
        }
        if Rc::ptr_eq(owner, &self.opts)
            && key == u("symbol")
            && self.profile == "getter-changes-market"
        {
            set_bare(
                &self.markets,
                &u("BTCUSDT"),
                object(vec![("quote", raw("USDC"))], true),
            );
        }
        Ok(get_bare(owner, &key))
    }
    fn count(&self, key: &str) -> usize {
        self.counts
            .borrow()
            .iter()
            .find(|(k, _)| k == &u(key))
            .map(|(_, n)| *n)
            .unwrap_or(0)
    }
}
impl SessionHost for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        self.read(v, u(k))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if matches!(v.as_ref(), Node::Null | Node::Undefined) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn set(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        set_bare(v, &u(k), value);
        Ok(())
    }
    fn define(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        self.set(v, k, value)
    }
    fn truthy(&self, v: &Raw) -> bool {
        truthy(v)
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        eq(a, b)
    }
    fn undefined(&self) -> Raw {
        undef()
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn boolean(&self, b: bool) -> Raw {
        boolean(b)
    }
    fn number(&self, n: f64) -> Raw {
        number(n)
    }
    fn numeric(&self, _: &Raw) -> Result<f64, Raw> {
        panic!("unused numeric capability")
    }
    fn units(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn text(&self, v: Vec<u16>) -> Raw {
        text(v)
    }
    fn object(&self, v: Vec<(&'static str, Raw)>) -> Raw {
        object(v, false)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        array(v)
    }
    fn global(&self) -> Raw {
        panic!("unused global capability")
    }
    fn select(&self, owner: Raw, key: &str) -> Result<Raw, Raw> {
        let callable = self.get(&owner, key)?;
        Ok(Rc::new(Node::Selected { owner, callable }))
    }
    fn receiver_call(&self, owner: Raw, callable: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function {
            owner: lexical,
            code,
        } = callable.as_ref()
        else {
            panic!("native callable")
        };
        assert!(
            Rc::ptr_eq(&owner, &self.root) && Rc::ptr_eq(lexical, &self.root),
            "native_receiver_identity"
        );
        let arg = args.first().cloned().unwrap_or_else(undef);
        match code.as_str() {
            "log" => {
                self.touch("log", args)?;
                self.logs.borrow_mut().push(arg);
                if self.profile == "log-throw" {
                    Err(self.primary.clone())
                } else {
                    Ok(undef())
                }
            }
            "fail" => {
                self.touch("call:fail", args)?;
                if matches!(self.profile.as_str(), "fail-return" | "multi-fail") {
                    Ok(self.returned.clone())
                } else if self.profile == "fail-promise" {
                    Ok(Rc::new(Node::Settled(Ok(self.returned.clone()))))
                } else if self.profile == "fail-reject" {
                    Ok(Rc::new(Node::Settled(Err(self.primary.clone()))))
                } else if self.profile == "fail-throw" {
                    Err(self.primary.clone())
                } else {
                    api::fail(self, owner, arg)
                }
            }
            "ready" => {
                self.touch("call:ready", vec![])?;
                if self.profile == "readiness-throw" {
                    Err(self.primary.clone())
                } else {
                    api::assert_private_ready(self, owner)
                }
            }
            _ => panic!("unowned function {code}"),
        }
    }
    fn apply(&self, v: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Selected { owner, callable } = v.as_ref() else {
            panic!("selected call")
        };
        self.receiver_call(owner.clone(), callable.clone(), args)
    }
    fn wait(&self, value: Raw) -> SessionFuture<'_, Raw, Raw> {
        Box::pin(async move {
            if let Node::Settled(result) = value.as_ref() {
                result.clone()
            } else {
                Ok(value)
            }
        })
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        match name {
            "computed property" => self.read(&args[0], js(&args[1])),
            "property in" => {
                let key = js(&args[0]);
                let owner = &args[1];
                if matches!(owner.as_ref(), Node::Object { watched: true, .. }) {
                    self.touch_units(u("has:").into_iter().chain(key.clone()).collect(), vec![])?;
                }
                fn has(v: &Raw, k: &[u16]) -> bool {
                    match v.as_ref() {
                        Node::Object {
                            fields, prototype, ..
                        } => {
                            fields.borrow().iter().any(|(key, _)| key == k)
                                || prototype.as_ref().is_some_and(|p| has(p, k))
                        }
                        _ => false,
                    }
                }
                Ok(boolean(has(owner, &key)))
            }
            _ => panic!("unused intrinsic {name}"),
        }
    }
    fn allocate(&self, _: Raw) -> Result<Raw, Raw> {
        panic!("unused allocate capability")
    }
    fn callback(&self, _: Raw, _: &'static str, _: Vec<Raw>) -> Result<Raw, Raw> {
        panic!("unused callback capability")
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        panic!("unused Promise allocation")
    }
    fn error(&self, message: Raw) -> Result<Raw, Raw> {
        self.touch("new-error", vec![message.clone()])?;
        if matches!(
            message.as_ref(),
            Node::Object {
                coercion: Some(true),
                ..
            }
        ) {
            return Err(self.primary.clone());
        }
        Ok(Rc::new(Node::Error(
            if matches!(message.as_ref(), Node::Undefined) {
                vec![]
            } else {
                js(&message)
            },
        )))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, error: Raw) -> Result<Raw, Raw> {
        Ok(error)
    }
}
const PRIVATE: &str = "HyroTrader account is reconnecting; wait for private state reconciliation before placing orders";
const MARKET: &str = "HyroTrader supports USDT perpetual instruments only";
const SLIPPAGE: &str = "HyroTrader does not support client-defined slippage limits; protection uses the simulator-configured gate";
const BBO: &str = "HyroTrader does not support Bybit BBO or TickSize slippage controls";
fn error_units(result: &Result<Raw, Raw>) -> Vec<u16> {
    let Err(v) = result else { return vec![] };
    match v.as_ref() {
        Node::Error(t) => t.clone(),
        _ => vec![],
    }
}
fn assert_undefined(result: &Result<Raw, Raw>, site: &str) {
    assert!(
        matches!(result,Ok(v)if matches!(v.as_ref(),Node::Undefined)),
        "{site}"
    );
}
fn native(h: &Harness, result: &Result<Raw, Raw>) {
    let m = h.method.as_str();
    let p = h.profile.as_str();
    if *h.faulted.borrow() {
        assert!(
            matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
            "native_fault_original_identity"
        );
    }
    if h.failure != "none" {
        return;
    }
    match m {
        "fail" => {
            if p == "throw-string" {
                assert!(
                    matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
                    "fail_native_coercion_error"
                );
            } else {
                let message = if p == "undefined" {
                    vec![]
                } else {
                    js(&input(p))
                };
                assert!(
                    matches!(result,Err(v)if matches!(v.as_ref(),Node::Error(_))),
                    "fail_native_error"
                );
                assert_eq!(error_units(result), message, "fail_native_message");
            }
            let logs = h.logs.borrow();
            assert_eq!(logs.len(), 1, "fail_native_log_count");
            assert!(
                Rc::ptr_eq(&logs[0], &h.argument.borrow()),
                "fail_native_original_logged_value"
            );
        }
        "fetchAndCacheApiKeyInfo" => assert_undefined(result, "cache_native_undefined"),
        "fetchApiKeyInfo" | "getAccount" => {
            if matches!(p, "fail-return" | "fail-promise") {
                assert!(
                    matches!(result,Ok(v)if Rc::ptr_eq(v,&h.returned)),
                    "metadata_native_replacement_return_identity"
                );
            } else if matches!(p, "fail-throw" | "log-throw" | "fail-reject") {
                assert!(
                    matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
                    "metadata_native_original_error"
                );
            } else {
                let (message, site) = if m == "fetchApiKeyInfo" {
                    (
                        "HyroTrader does not provide API-key metadata",
                        "api_native_error",
                    )
                } else {
                    (
                        "HyroTrader does not provide account identity metadata",
                        "account_native_error",
                    )
                };
                assert_eq!(error_units(result), u(message), "{site}");
            }
        }
        "fetchFees" => assert!(
            matches!(result,Ok(v)if matches!(v.as_ref(),Node::Array(a)if a.borrow().is_empty())),
            "fees_native_empty"
        ),
        "fetchPositionMode" => assert!(
            matches!(result,Ok(v)if matches!(v.as_ref(),Node::Text(t)if t==&u("oneway"))),
            "mode_native_oneway"
        ),
        "changePositionMode" | "changeMarginType" => {
            if p == "fail-throw" {
                assert!(
                    matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
                    "change_native_original_error"
                );
            } else if p == "fail-return" || !truthy(&input(p)) {
                assert_undefined(result, "change_native_undefined");
            } else {
                let (message, site) = if m == "changePositionMode" {
                    (
                        "HyroTrader supports one-way position mode only",
                        "change_position_native_error",
                    )
                } else {
                    (
                        "HyroTrader supports cross margin only",
                        "change_margin_native_error",
                    )
                };
                assert_eq!(error_units(result), u(message), "{site}");
            }
        }
        "assertPrivateReady" => {
            if p == "fail-throw" {
                assert!(
                    matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
                    "ready_native_original_error"
                );
            } else if matches!(
                p,
                "not-started" | "ready" | "changed-socket" | "fail-return"
            ) {
                assert_undefined(result, "ready_native_undefined");
            } else {
                assert_eq!(error_units(result), u(PRIVATE), "ready_native_error");
            }
            if p == "not-started" {
                assert_eq!(h.count("get:ready"), 0, "ready_native_short_circuit");
            }
            if p == "changed-socket" {
                assert_eq!(*h.socket_reads.borrow(), 2, "ready_native_live_socket_read");
            }
        }
        "validateOrderCapabilities" => {
            let reason = match p {
                "reconnecting" => Some(PRIVATE),
                "missing-market"
                | "otherquote"
                | "quote-boxed"
                | "nullmarket"
                | "numberopts"
                | "textopts"
                | "getter-changes-market" => Some(MARKET),
                "slip-null" | "slip-zero" | "slip-nan" | "slip-proto" => Some(SLIPPAGE),
                "bboundef" | "bboproto" | "tickundefined" | "tickproto" | "both" | "shadow" => {
                    Some(BBO)
                }
                _ => None,
            };
            if p == "readiness-throw" {
                assert!(
                    matches!(result,Err(v)if Rc::ptr_eq(v,&h.primary)),
                    "capabilities_native_readiness_error"
                );
            } else if let Some(reason) = reason {
                let site = if reason == SLIPPAGE {
                    "capabilities_native_slippage"
                } else {
                    "capabilities_native_error"
                };
                assert_eq!(error_units(result), u(reason), "{site}");
                assert_eq!(
                    h.logs.borrow().len(),
                    1,
                    "capabilities_native_visible_failure"
                );
            } else if matches!(p, "nullstore" | "undefinedopts" | "nullopts") {
                assert!(result.is_err(), "capabilities_native_property_error");
            } else {
                assert_undefined(result, "capabilities_native_undefined");
            }
            if p == "bboproto" || p == "both" || p == "shadow" {
                assert_eq!(
                    h.count("has:bybitSlippageToleranceTicks"),
                    0,
                    "capabilities_native_in_short_circuit"
                );
            }
            if p == "multi-fail" {
                assert_eq!(
                    h.count("call:fail"),
                    4,
                    "capabilities_native_replaced_fail_continues"
                );
                assert!(
                    h.logs.borrow().is_empty(),
                    "capabilities_native_replaced_fail_no_log"
                );
            }
        }
        _ => panic!("unknown method"),
    }
}
fn invoke(h: &Harness) -> (Result<Raw, Raw>, usize) {
    let owner = h.root.clone();
    let m = h.method.as_str();
    if m != "fail" {
        if let Err(e) = h.get(&owner, m) {
            return (Err(e), h.ops.borrow().len());
        }
    }
    let arg = if matches!(h.profile.as_str(), "fail-return" | "fail-throw") {
        boolean(true)
    } else {
        input(&h.profile)
    };
    macro_rules! eager {
        ($f:expr) => {{
            let f = $f;
            let count = h.ops.borrow().len();
            (settle(f), count)
        }};
    }
    match m {
        "fail" => {
            let argument = input(&h.profile);
            *h.argument.borrow_mut() = argument.clone();
            (api::fail(h, owner, argument), h.ops.borrow().len())
        }
        "fetchAndCacheApiKeyInfo" => eager!(api::fetch_and_cache_api_key_info(h)),
        "fetchApiKeyInfo" => eager!(api::fetch_api_key_info(h, owner)),
        "getAccount" => eager!(api::get_account(h, owner)),
        "fetchFees" => eager!(api::fetch_fees(h, arg)),
        "fetchPositionMode" => eager!(api::fetch_position_mode(h, arg)),
        "changePositionMode" => eager!(api::change_position_mode(
            h,
            owner,
            arg,
            object(vec![("ignored", boolean(true))], false)
        )),
        "changeMarginType" => eager!(api::change_margin_type(h, owner, arg)),
        "assertPrivateReady" => {
            let result = h
                .touch("call:ready", vec![])
                .and_then(|()| api::assert_private_ready(h, owner));
            (result, h.ops.borrow().len())
        }
        "validateOrderCapabilities" => (
            api::validate_order_capabilities(h, owner, h.opts.clone()),
            h.ops.borrow().len(),
        ),
        _ => panic!("unknown row"),
    }
}
#[test]
fn original_source_and_native_capabilities() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/hyro-capabilities-fixtures.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> =
        serde_json::from_slice(&output.stdout).expect("UTF-16 source observation");
    assert_eq!(cases.len(), 1683, "declared case floor");
    assert_eq!(cases.last().unwrap()["profile"], "getter-changes-market");
    assert_eq!(cases.last().unwrap()["failure"], "get:assertPrivateReady#0");
    let mut unique = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    for case in &cases {
        let key = (
            case["method"].as_str().unwrap(),
            case["profile"].as_str().unwrap(),
            case["failure"].as_str().unwrap(),
        );
        assert!(unique.insert(key), "independent unique case floor");
        *counts.entry(key.0).or_insert(0_usize) += 1;
    }
    let expected = std::collections::BTreeMap::from([
        ("fail", 204),
        ("fetchAndCacheApiKeyInfo", 17),
        ("fetchApiKeyInfo", 102),
        ("getAccount", 102),
        ("fetchFees", 85),
        ("fetchPositionMode", 85),
        ("changePositionMode", 221),
        ("changeMarginType", 221),
        ("assertPrivateReady", 119),
        ("validateOrderCapabilities", 527),
    ]);
    assert_eq!(counts, expected, "independent method profile/fault floor");
    let mut comparisons = vec![];
    for case in cases {
        let method = case["method"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let h = Harness::new(method, profile, failure);
        let (result, sync_ops) = invoke(&h);
        native(&h, &result);
        let mut observed = match &result {
            Ok(v) => json!({"ok":true,"value":atom(v)}),
            Err(e) => json!({"ok":false,"error":atom(e),"primary":Rc::ptr_eq(e,&h.primary)}),
        };
        if method == "fetchFees" {
            if let Ok(first) = result.as_ref() {
                h.get(&h.root, "fetchFees").unwrap();
                let second = settle(api::fetch_fees(&h, undef())).unwrap();
                assert!(!Rc::ptr_eq(first, &second), "fees_native_fresh");
                let Node::Array(first_array) = first.as_ref() else {
                    panic!("fees native array")
                };
                first_array.borrow_mut().push(lone("mutated-"));
                let Node::Array(second_array) = second.as_ref() else {
                    panic!("fees second array")
                };
                assert_eq!(first_array.borrow().len(), 1, "fees_native_mutable");
                assert!(second_array.borrow().is_empty(), "fees_native_independent");
                observed["fresh"] = json!(true);
                observed["mutable"] = json!(true);
            }
        }
        let receipt = json!({"ops":*h.ops.borrow(),"logs":h.logs.borrow().iter().map(atom).collect::<Vec<_>>(),"syncOps":sync_ops,"result":observed});
        comparisons.push((case, receipt));
    }
    for (case, receipt) in comparisons {
        for key in ["ops", "logs", "syncOps", "result"] {
            assert_eq!(
                receipt[key], case[key],
                "source comparison {}/{}/{} {key}",
                case["method"], case["profile"], case["failure"]
            );
        }
    }
}
