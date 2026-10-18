use safe_cex_venue_bybit::{
    hyro_order_publication as api,
    hyro_private_session::{SessionFuture, SessionHost as Host},
    variant_account_refresh::AccountRefreshHost,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Reference {
        tag: String,
        value: Raw,
    },
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Raw>>),
    Settled(Result<Raw, Raw>),
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        tag: Option<String>,
        error: bool,
    },
    RegExp(Vec<u16>, Vec<u16>),
    Set(RefCell<Vec<Raw>>),
    Map(RefCell<Vec<(Raw, Raw)>>),
    Iterator {
        value: Raw,
        index: RefCell<usize>,
    },
    Function {
        owner: Raw,
        code: String,
        captures: Vec<Raw>,
    },
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: Vec<u16>) -> Raw {
    Rc::new(Node::Text(s))
}
fn raw(s: &str) -> Raw {
    text(u(s))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn null() -> Raw {
    Rc::new(Node::Null)
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn boolean(b: bool) -> Raw {
    Rc::new(Node::Bool(b))
}
fn object(fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn tagged(tag: &str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: Some(tag.into()),
        error: false,
    })
}
fn array(v: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn func(owner: Raw, code: &str, captures: Vec<Raw>) -> Raw {
    Rc::new(Node::Function {
        owner,
        code: code.into(),
        captures,
    })
}
fn elements(v: &Raw) -> Vec<Raw> {
    if let Node::Reference { value, .. } = v.as_ref() {
        return elements(value);
    }
    if let Node::Set(values) = v.as_ref() {
        return values.borrow().clone();
    }
    if let Node::Map(values) = v.as_ref() {
        return values.borrow().iter().map(|(_, v)| v.clone()).collect();
    }
    let Node::Array(a) = v.as_ref() else {
        panic!("not array")
    };
    a.borrow().clone()
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
fn get_units(v: &Raw, key: &[u16]) -> Raw {
    match v.as_ref() {
        Node::Reference { value, .. } => get_units(value, key),
        Node::Object {
            fields, prototype, ..
        } => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                prototype
                    .as_ref()
                    .map(|p| get_units(p, key))
                    .unwrap_or_else(undef)
            }),
        Node::Set(values) if key == u("size") => number(values.borrow().len() as f64),
        Node::Map(values) if key == u("size") => number(values.borrow().len() as f64),
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        Node::Array(a) => String::from_utf16(key)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| a.borrow().get(i).cloned())
            .unwrap_or_else(undef),
        _ => undef(),
    }
}
fn get(v: &Raw, key: &str) -> Raw {
    get_units(v, &u(key))
}
fn set(v: &Raw, key: &str, value: Raw) {
    let Node::Object { fields, .. } = v.as_ref() else {
        panic!("write nonobject")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == u(key)) {
        *v = value
    } else {
        fields.push((u(key), value));
    }
}
fn numeric(v: &Raw) -> f64 {
    match v.as_ref() {
        Node::Number(n) => *n,
        Node::Null => 0.0,
        Node::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Node::Text(t) => String::from_utf16(t)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Text(t) => t.clone(),
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        Node::Object { error: true, .. } => u("Error: ")
            .into_iter()
            .chain(js(&get(v, "message")))
            .collect(),
        _ => u("[object Object]"),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Reference { tag, .. } => json!({"ref":tag}),
        Node::Settled(_) => panic!("observer internal value"),
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Function { code, .. } => json!({"fn":code}),
        Node::Set(values) => json!({"set":values.borrow().iter().map(atom).collect::<Vec<_>>()}),
        Node::RegExp(_, _) | Node::Map(_) | Node::Iterator { .. } => {
            panic!("internal runtime value")
        }
        Node::Object { tag: Some(tag), .. } => json!({"ref":tag}),
        Node::Object { error: true, .. } => json!({"error":true,"message":js(&get(v,"message"))}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
    }
}
fn error_units(message: Vec<u16>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(vec![(u("message"), text(message))]),
        prototype: None,
        tag: None,
        error: true,
    })
}
fn raw_error(message: &str) -> Raw {
    error_units(u(message))
}
fn reference(tag: &str, value: Raw) -> Raw {
    Rc::new(Node::Reference {
        tag: tag.into(),
        value,
    })
}
fn truthy(v: &Raw) -> bool {
    match v.as_ref() {
        Node::Undefined | Node::Null => false,
        Node::Bool(b) => *b,
        Node::Number(n) => *n != 0.0 && !n.is_nan(),
        Node::Text(t) => !t.is_empty(),
        _ => true,
    }
}
fn set_value(values: Vec<Raw>) -> Raw {
    Rc::new(Node::Set(RefCell::new(values)))
}
fn block<F: std::future::Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(f);
    for _ in 0..1024 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut c) {
            return v;
        }
    }
    panic!("declared fixture completion bound")
}
struct Harness {
    method: String,
    profile: String,
    failure: String,
    recovery: Raw,
    exchange: Raw,
    group: Raw,
    payload_a: Raw,
    payload_b: Raw,
    create_a: Raw,
    create_b: Raw,
    response_a: Raw,
    response_b: Raw,
    found: Raw,
    rows: Raw,
    cancel_a: Raw,
    cancel_b: Raw,
    child_a: Raw,
    child_b: Raw,
    client: Raw,
    initial_observed: Raw,
    primary: Raw,
    calls: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Value>>,
    queries: RefCell<usize>,
    posts: RefCell<usize>,
}
impl Harness {
    fn new(m: &str, p: &str, f: &str) -> Self {
        let payload_a = tagged(
            "payload-A",
            vec![
                (
                    "orderLinkId",
                    if p == "lone-link" {
                        text(u("A").into_iter().chain([0xd800]).collect())
                    } else {
                        raw(if p == "empty-link" { "" } else { "A" })
                    },
                ),
                ("symbol", raw("BTCUSDT")),
            ],
        );
        let payload_b = reference(
            "payload-B",
            object(vec![("orderLinkId", raw("B")), ("symbol", raw("ETHUSDT"))]),
        );
        let create_a = tagged(
            "create-A",
            vec![
                ("payload", payload_a.clone()),
                ("uncertain", boolean(false)),
            ],
        );
        let create_b = tagged(
            "create-B",
            vec![
                ("payload", payload_b.clone()),
                ("uncertain", boolean(false)),
            ],
        );
        let response_a = tagged(
            "response-A",
            vec![
                ("payload", payload_a.clone()),
                (
                    "data",
                    object(vec![("result", object(vec![("orderId", raw("OA"))]))]),
                ),
            ],
        );
        let response_b = tagged(
            "response-B",
            vec![
                ("payload", payload_b.clone()),
                (
                    "data",
                    object(vec![("result", object(vec![("orderId", raw("OB"))]))]),
                ),
            ],
        );
        let group = object(vec![
            (
                "creates",
                array(if p == "empty" {
                    vec![]
                } else {
                    vec![create_a.clone(), create_b.clone()]
                }),
            ),
            (
                "responses",
                array(if p == "resume" {
                    vec![response_a.clone()]
                } else {
                    vec![]
                }),
            ),
            ("observedLinkIds", set_value(vec![raw("A")])),
        ]);
        let other = object(vec![
            (
                "creates",
                array(vec![object(vec![(
                    "payload",
                    object(vec![("orderLinkId", raw("C"))]),
                )])]),
            ),
            ("responses", array(vec![])),
            ("observedLinkIds", set_value(vec![])),
        ]);
        let pending = Rc::new(Node::Map(RefCell::new(if p == "empty" {
            vec![]
        } else {
            vec![(raw("first"), group.clone()), (raw("other"), other)]
        })));
        let recovery = tagged("recovery", vec![("pending", pending)]);
        let initial_observed = set_value(vec![raw("initial")]);
        let exchange = tagged(
            "exchange",
            vec![
                ("orderRecovery", recovery.clone()),
                (
                    "activeOrderObservations",
                    set_value(vec![initial_observed.clone()]),
                ),
                ("isDisposed", boolean(p == "disposed")),
            ],
        );
        let first = object(vec![
            (
                "orderId",
                if p == "non-string" {
                    number(7.0)
                } else {
                    raw(if p == "empty-id" { "" } else { "OA" })
                },
            ),
            (
                "orderLinkId",
                if p == "non-string" {
                    number(7.0)
                } else {
                    raw(match p {
                        "empty-link" => "",
                        "unmatched" => "Z",
                        _ => "A",
                    })
                },
            ),
        ]);
        let mut rows = if p == "empty" {
            vec![]
        } else {
            vec![
                first.clone(),
                object(vec![("orderId", raw("OB")), ("orderLinkId", raw("B"))]),
            ]
        };
        if p == "repeat" {
            rows.push(first);
        }
        let found = tagged(
            "found",
            vec![
                ("orderLinkId", get(&payload_a, "orderLinkId")),
                ("orderId", raw(if p == "empty-id" { "" } else { "OA" })),
                (
                    "orderStatus",
                    raw(match p {
                        "invalid-status" => "Bad",
                        "rejected" => "Rejected",
                        _ => "New",
                    }),
                ),
            ],
        );
        let cancel_a = tagged(
            "cancel-A",
            vec![("id", raw("OA")), ("symbol", raw("BTCUSDT"))],
        );
        let cancel_b = tagged(
            "cancel-B",
            vec![("id", raw("OB")), ("symbol", raw("ETHUSDT"))],
        );
        let child_a = tagged("child-A", vec![("id", raw("OA__stop_loss"))]);
        let child_b = tagged("child-B", vec![("id", raw("OA__take_profit"))]);
        let unrelated = tagged("unrelated", vec![("id", raw("OTHER"))]);
        let mut store = if p == "no-children" {
            vec![]
        } else {
            vec![child_a.clone(), child_b.clone(), unrelated]
        };
        if p == "existing" {
            store.push(object(vec![("id", raw("OA"))]));
        }
        set(
            &exchange,
            "store",
            tagged("store", vec![("orders", array(store))]),
        );
        set(&exchange, "xhr", tagged("xhr", vec![]));
        Self {
            method: m.into(),
            profile: p.into(),
            failure: f.into(),
            recovery,
            exchange,
            group,
            payload_a,
            payload_b,
            create_a,
            create_b,
            response_a,
            response_b,
            found,
            rows: array(rows),
            cancel_a,
            cancel_b,
            child_a,
            child_b,
            client: tagged("client", vec![]),
            initial_observed,
            primary: tagged("primary", vec![]),
            calls: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            queries: RefCell::new(0),
            posts: RefCell::new(0),
        }
    }
    fn touch(&self, n: &str, args: &[Raw]) -> Result<(), Raw> {
        self.calls
            .borrow_mut()
            .push(json!({"name":n,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        if self.failure == n {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn count(&self, n: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|v| v["name"] == n)
            .count()
    }
    fn invoke(&self) -> (Result<Raw, Raw>, usize) {
        macro_rules! eager {
            ($future:expr) => {{
                let future = $future;
                let sync = self.calls.borrow().len();
                (block(future), sync)
            }};
        }
        let r = match self.method.as_str() {
            "observeOrders" => api::observe_orders(self, self.recovery.clone(), self.rows.clone()),
            "dispose" => api::dispose(self, self.recovery.clone()),
            "unknown" => {
                let cause = match self.profile.as_str() {
                    "error" => raw_error("cause"),
                    "undefined" => undef(),
                    "null" => null(),
                    "numeric" => number(7.0),
                    "lone" => text(u("cause-").into_iter().chain([0xd800]).collect()),
                    _ => raw("cause"),
                };
                api::unknown(self, self.payload_a.clone(), cause)
            }
            "runGroup" => {
                return eager!(api::run_group(
                    self,
                    self.recovery.clone(),
                    self.group.clone(),
                    func(undef(), "onAccepted", vec![])
                ));
            }
            "findOrder" => {
                return eager!(api::find_order(
                    self,
                    self.recovery.clone(),
                    self.payload_a.clone()
                ));
            }
            "observePrivateOrders" => {
                api::observe_private_orders(self, self.exchange.clone(), self.rows.clone())
            }
            "submitCreateOrders" => {
                return eager!(api::submit_create_orders(
                    self,
                    self.exchange.clone(),
                    array(vec![self.payload_a.clone()]),
                    object(vec![("symbol", raw("BTCUSDT"))])
                ));
            }
            "cancelOrdersHelper" => {
                return eager!(api::cancel_orders_helper(
                    self,
                    self.exchange.clone(),
                    array(if self.profile == "empty" {
                        vec![]
                    } else if self.profile == "repeated" {
                        vec![self.cancel_a.clone(), self.cancel_a.clone()]
                    } else {
                        vec![self.cancel_a.clone(), self.cancel_b.clone()]
                    }),
                    raw("linear")
                ));
            }
            _ => panic!("row"),
        };
        (r, self.calls.borrow().len())
    }
    fn state(&self) -> Value {
        json!({"pendingSize":elements(&get(&self.recovery,"pending")).len(),"observed":elements(&get(&self.group,"observedLinkIds")).iter().map(atom).collect::<Vec<_>>(),"responses":elements(&get(&self.group,"responses")).iter().map(atom).collect::<Vec<_>>(),"uncertain":elements(&get(&self.group,"creates")).iter().map(|c|atom(&get(c,"uncertain"))).collect::<Vec<_>>(),"seen":[atom(&get(&self.response_a,"authoritativeSeen")),atom(&get(&self.response_b,"authoritativeSeen"))],"recorded":atom(&get(&self.response_a,"recorded")),"activeSize":elements(&get(&self.exchange,"activeOrderObservations")).len(),"initialObserved":elements(&self.initial_observed).iter().map(atom).collect::<Vec<_>>(),"queries":*self.queries.borrow(),"posts":*self.posts.borrow()})
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        let real = get(v, k);
        if !eq(&real, &undef()) {
            return Ok(real);
        }
        let known = [
            "test",
            "next",
            "values",
            "clear",
            "add",
            "has",
            "delete",
            "push",
            "some",
            "find",
            "filter",
            "includes",
            "getClient",
            "get",
            "post",
            "submitOne",
            "isDisposed",
            "getPublicationBoundary",
            "unknown",
            "observeOrders",
            "submit",
            "emitAndLogError",
            "removeOrders",
            "addOrUpdateOrders",
            "mapValidatedOrders",
            "recordRecentlyPlacedOrderFromOpts",
            "retireCancelledOrders",
            "fail",
            "getBybitResponseMeta",
        ];
        if known.contains(&k) {
            Ok(func(v.clone(), k, vec![]))
        } else {
            Ok(real)
        }
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if matches!(v.as_ref(), Node::Null | Node::Undefined) {
            Ok(undef())
        } else {
            self.get(v, k)
        }
    }
    fn set(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        set(v, k, value);
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
    fn numeric(&self, v: &Raw) -> Result<f64, Raw> {
        Ok(numeric(v))
    }
    fn units(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn text(&self, t: Vec<u16>) -> Raw {
        text(t)
    }
    fn object(&self, v: Vec<(&'static str, Raw)>) -> Raw {
        object(v)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        array(v)
    }
    fn global(&self) -> Raw {
        object(vec![])
    }
    fn select(&self, v: Raw, k: &str) -> Result<Raw, Raw> {
        self.get(&v, k)
    }
    fn receiver_call(&self, receiver: Raw, f: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        if let Node::Function { code, .. } = f.as_ref() {
            if code == "onAccepted" {
                assert!(eq(&receiver, &undef()), "group_native_unbound_acceptance");
            }
        }
        self.apply(f, args)
    }
    fn apply(&self, f: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function {
            owner,
            code,
            captures,
        } = f.as_ref()
        else {
            panic!("selected function")
        };
        match code.as_str() {
            "recovery-link"
            | "reconciliation-link"
            | "existing-order"
            | "cancel-child"
            | "accepted-order" => api::dispatch(
                self,
                owner,
                api::Callback::from_name(code).expect("source callback selector"),
                captures,
                args,
            ),
            "next" => {
                let Node::Iterator { value, index } = owner.as_ref() else {
                    panic!("iterator")
                };
                let i = *index.borrow();
                let values = elements(value);
                if i < values.len() {
                    *index.borrow_mut() += 1;
                    Ok(object(vec![
                        ("done", boolean(false)),
                        ("value", values[i].clone()),
                    ]))
                } else {
                    Ok(object(vec![("done", boolean(true))]))
                }
            }
            "values" => Ok(Rc::new(Node::Iterator {
                value: owner.clone(),
                index: RefCell::new(0),
            })),
            "clear" => {
                match owner.as_ref() {
                    Node::Map(v) => v.borrow_mut().clear(),
                    Node::Set(v) => v.borrow_mut().clear(),
                    _ => panic!("clear"),
                };
                Ok(undef())
            }
            "add" | "has" | "delete" => {
                let Node::Set(v) = owner.as_ref() else {
                    panic!("set")
                };
                let index = v.borrow().iter().position(|v| eq(v, &args[0]));
                if code == "has" {
                    return Ok(boolean(index.is_some()));
                }
                if code == "delete" {
                    if let Some(index) = index {
                        v.borrow_mut().remove(index);
                    }
                    return Ok(boolean(index.is_some()));
                }
                if index.is_none() {
                    v.borrow_mut().push(args[0].clone());
                }
                Ok(owner.clone())
            }
            "push" => {
                let Node::Array(v) = owner.as_ref() else {
                    panic!("push")
                };
                v.borrow_mut().extend(args);
                Ok(number(v.borrow().len() as f64))
            }
            "some" | "find" | "filter" => {
                let mut out = vec![];
                for value in elements(owner) {
                    let matches = truthy(&self.apply(args[0].clone(), vec![value.clone()])?);
                    if matches {
                        if code == "some" {
                            return Ok(boolean(true));
                        }
                        if code == "find" {
                            return Ok(value);
                        }
                        out.push(value);
                    }
                }
                Ok(if code == "some" {
                    boolean(false)
                } else if code == "find" {
                    undef()
                } else {
                    array(out)
                })
            }
            "includes" => Ok(boolean(elements(owner).iter().any(|v| eq(v, &args[0])))),
            "test" => {
                let Node::RegExp(pattern, flags) = owner.as_ref() else {
                    panic!("regexp runtime");
                };
                assert_eq!(flags, &u("i"));
                let input = js(&args[0])
                    .into_iter()
                    .map(|c| if (65..=90).contains(&c) { c + 32 } else { c })
                    .collect::<Vec<_>>();
                Ok(boolean(pattern.split(|c| *c == 124).any(|needle| {
                    input.windows(needle.len()).any(|w| w == needle)
                })))
            }
            "isDisposed" => Ok(boolean(self.profile == "disposed")),
            "getPublicationBoundary" => Ok(raw(if self.profile == "boundary" {
                "new"
            } else {
                "old"
            })),
            "unknown" => api::unknown(self, args[0].clone(), args[1].clone()),
            "getBybitResponseMeta" => Ok(object(vec![
                (
                    "isSuccess",
                    boolean(eq(&get(&args[0], "retCode"), &number(0.0))),
                ),
                ("retMsg", get(&args[0], "retMsg")),
            ])),
            "fail" => Err(error_units(js(&args[0]))),
            "get" | "post" => {
                let touched = self.touch(code, &args);
                let result = touched.and_then(|()| {
                    if code == "get" {
                        *self.queries.borrow_mut() += 1;
                        let queries = *self.queries.borrow();
                        if self.profile == "reject" {
                            return Err(self.primary.clone());
                        }
                        let empty = self.profile == "history" && eq(&args[0], &raw("open"))
                            || matches!(
                                self.profile.as_str(),
                                "absent"
                                    | "bad-cursor"
                                    | "repeated-cursor"
                                    | "limit"
                                    | "cursor-stop"
                            )
                            || self.profile == "last-page" && queries < 100;
                        let cursor = match self.profile.as_str() {
                            "bad-cursor" => null(),
                            "repeated-cursor" => raw("same"),
                            "limit" | "last-page" => raw(&format!("page-{queries}")),
                            "cursor-stop" => raw(""),
                            _ => undef(),
                        };
                        Ok(object(vec![(
                            "data",
                            object(vec![
                                (
                                    "retCode",
                                    number(if self.profile == "ret-failure" {
                                        1.0
                                    } else {
                                        0.0
                                    }),
                                ),
                                (
                                    "retMsg",
                                    raw(if self.profile == "ret-failure" {
                                        "denied"
                                    } else {
                                        "OK"
                                    }),
                                ),
                                (
                                    "result",
                                    object(vec![
                                        (
                                            "list",
                                            if self.profile == "malformed" {
                                                null()
                                            } else {
                                                array(if empty {
                                                    vec![]
                                                } else {
                                                    vec![self.found.clone()]
                                                })
                                            },
                                        ),
                                        ("nextPageCursor", cursor),
                                    ]),
                                ),
                            ]),
                        )]))
                    } else {
                        *self.posts.borrow_mut() += 1;
                        if self.profile == "reject"
                            || self.profile == "second-reject" && *self.posts.borrow() == 2
                        {
                            return Err(self.primary.clone());
                        }
                        Ok(object(vec![(
                            "data",
                            object(vec![
                                (
                                    "retCode",
                                    number(
                                        if matches!(
                                            self.profile.as_str(),
                                            "ret-failure" | "gone" | "late"
                                        ) {
                                            1.0
                                        } else {
                                            0.0
                                        },
                                    ),
                                ),
                                (
                                    "retMsg",
                                    raw(match self.profile.as_str() {
                                        "ret-failure" => "denied",
                                        "gone" => "ORDER NOT EXISTS",
                                        "late" => "Too late to cancel",
                                        _ => "OK",
                                    }),
                                ),
                            ]),
                        )]))
                    }
                });
                Ok(Rc::new(Node::Settled(result)))
            }
            name => {
                self.touch(name, &args)?;
                match name {
                    "getClient" => Ok(self.client.clone()),
                    "submitOne" => Ok(Rc::new(Node::Settled(if self.profile == "reject" {
                        Err(self.primary.clone())
                    } else {
                        Ok(object(vec![
                            (
                                "response",
                                if Rc::ptr_eq(&args[0], &self.create_a) {
                                    self.response_a.clone()
                                } else {
                                    self.response_b.clone()
                                },
                            ),
                            ("boundary", raw("old")),
                        ]))
                    }))),
                    "onAccepted" => {
                        if self.profile == "accepted-throw" {
                            return Err(self.primary.clone());
                        }
                        if self.profile == "callback-grow"
                            && elements(&get(&self.group, "creates")).len() == 2
                        {
                            let creates = get(&self.group, "creates");
                            let Node::Array(values) = creates.as_ref() else {
                                panic!("creates")
                            };
                            values.borrow_mut().push(object(vec![
                                ("payload", object(vec![("orderLinkId", raw("B"))])),
                                ("uncertain", boolean(false)),
                            ]));
                        }
                        Ok(undef())
                    }
                    "observeOrders" => {
                        api::observe_orders(self, self.recovery.clone(), args[0].clone())
                    }
                    "submit" => {
                        if self.profile == "submit-throw" {
                            return Err(self.primary.clone());
                        }
                        if self.profile == "reject" {
                            return Ok(Rc::new(Node::Settled(Err(self.primary.clone()))));
                        }
                        if self.profile == "observed" {
                            for observations in
                                elements(&get(&self.exchange, "activeOrderObservations"))
                            {
                                let Node::Set(v) = observations.as_ref() else {
                                    panic!("observations")
                                };
                                v.borrow_mut().push(raw("OA"));
                            }
                        }
                        for (k, p) in [
                            ("authoritativeSeen", "authoritative"),
                            ("terminal", "terminal"),
                            ("reconciled", "reconciled"),
                        ] {
                            if self.profile == p {
                                set(&self.response_a, k, boolean(true));
                            }
                        }
                        if self.profile == "recovered" {
                            set(&self.response_a, "recoveredOrder", self.found.clone());
                        }
                        self.apply(args[1].clone(), vec![self.response_a.clone()])?;
                        Ok(Rc::new(Node::Settled(Ok(array(vec![
                            self.response_a.clone(),
                        ])))))
                    }
                    "emitAndLogError" => {
                        self.logs.borrow_mut().push(atom(&args[0]));
                        Ok(undef())
                    }
                    "mapValidatedOrders" => Ok(args[0].clone()),
                    "removeOrders"
                    | "addOrUpdateOrders"
                    | "recordRecentlyPlacedOrderFromOpts"
                    | "retireCancelledOrders" => Ok(undef()),
                    _ => panic!("dependency {name}"),
                }
            }
        }
    }
    fn wait(&self, v: Raw) -> SessionFuture<'_, Raw, Raw> {
        let mut deferred = true;
        Box::pin(std::future::poll_fn(move |cx| {
            if deferred {
                deferred = false;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(if let Node::Settled(result) = v.as_ref() {
                result.clone()
            } else {
                Ok(v.clone())
            })
        }))
    }
    fn intrinsic(&self, n: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        match n {
            "less than" => Ok(boolean(numeric(&args[0]) < numeric(&args[1]))),
            "iterator" => {
                if matches!(args[0].as_ref(), Node::Iterator { .. }) {
                    Ok(args[0].clone())
                } else {
                    Ok(Rc::new(Node::Iterator {
                        value: args[0].clone(),
                        index: RefCell::new(0),
                    }))
                }
            }
            "IteratorClose normal" => Ok(args.last().unwrap().clone()),
            "IteratorClose abrupt" | "throw value" => Err(args.last().unwrap().clone()),
            "new Set" => Ok(set_value(vec![])),
            "ENDPOINTS" => Ok(object(vec![
                ("UNFILLED_ORDERS", raw("open")),
                ("USER_ORDERS", raw("history")),
                ("CANCEL_ORDER", raw("cancel")),
            ])),
            "computed property" => Ok(get_units(&args[0], &js(&args[1]))),
            "typeof" => Ok(raw(if matches!(args[0].as_ref(), Node::Text(_)) {
                "string"
            } else {
                "object"
            })),
            "instanceof Error" => Ok(boolean(matches!(
                args[0].as_ref(),
                Node::Object { error: true, .. }
            ))),
            "String" => Ok(text(js(&args[0]))),
            "Array.isArray" => Ok(boolean(matches!(args[0].as_ref(), Node::Array(_)))),
            "RegExp" => Ok(Rc::new(Node::RegExp(js(&args[0]), js(&args[1])))),
            _ => panic!("intrinsic {n}"),
        }
    }
    fn allocate(&self, _: Raw) -> Result<Raw, Raw> {
        panic!("unused allocate")
    }
    fn callback(&self, v: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(func(v, code, captures))
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        panic!("unused promise")
    }
    fn error(&self, message: Raw) -> Result<Raw, Raw> {
        Ok(error_units(js(&message)))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, error: Raw) -> Result<Raw, Raw> {
        Ok(error)
    }
}
impl AccountRefreshHost for Harness {
    fn spread(&self, base: Raw, changes: Vec<(&'static str, Raw)>) -> Result<Raw, Raw> {
        let out = object(vec![]);
        if let Node::Object { fields, .. } = base.as_ref() {
            for (k, v) in fields.borrow().iter() {
                set(&out, &String::from_utf16(k).unwrap(), v.clone());
            }
        }
        for (k, v) in changes {
            set(&out, k, v);
        }
        Ok(out)
    }
    fn then(&self, _: Raw, _: Raw) -> Result<Raw, Raw> {
        panic!("unused then")
    }
    fn all(&self, _: Vec<Raw>) -> Result<Raw, Raw> {
        panic!("unused all")
    }
}
fn native(h: &Harness, result: &Result<Raw, Raw>) {
    if h.method == "submitCreateOrders" {
        assert_eq!(
            elements(&get(&h.exchange, "activeOrderObservations")).len(),
            1,
            "publication_native_observation_cleanup"
        );
    }
    if h.failure != "none" {
        if h.count(&h.failure) > 0 {
            assert!(result.is_err(), "fault_native_error");
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
                "fault_native_identity"
            );
        }
        return;
    }
    match h.method.as_str() {
        "observeOrders" => {
            assert!(
                eq(result.as_ref().unwrap(), &undef()),
                "observe_native_undefined"
            );
            let observed = elements(&get(&h.group, "observedLinkIds"));
            assert_eq!(
                observed.len(),
                if h.profile == "empty" {
                    1
                } else if h.profile == "empty-link" {
                    3
                } else {
                    2
                },
                "observe_native_linkset"
            );
            if h.profile != "empty" {
                assert!(
                    observed.iter().any(|v| eq(v, &raw("B"))),
                    "observe_native_linkset"
                );
            }
        }
        "dispose" => assert!(
            elements(&get(&h.recovery, "pending")).is_empty(),
            "dispose_native_pending_cleared"
        ),
        "unknown" => {
            let value = result.as_ref().unwrap();
            let mut message = u("HyroTrader order outcome is unknown (orderLinkId ");
            message.extend(u(if h.profile == "empty-link" { "" } else { "A" }));
            message.extend(u("). Reconcile before placing a replacement: "));
            message.extend(match h.profile.as_str() {
                "undefined" => u("undefined"),
                "null" => u("null"),
                "numeric" => u("7"),
                "lone" => u("cause-").into_iter().chain([0xd800]).collect(),
                _ => u("cause"),
            });
            assert_eq!(
                js(&get(value, "message")),
                message,
                "unknown_native_message"
            );
        }
        "runGroup" => {
            let rejected = matches!(
                h.profile.as_str(),
                "disposed" | "boundary" | "accepted-throw" | "reject"
            );
            assert_eq!(result.is_err(), rejected, "group_native_outcome");
            if !rejected {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &get(&h.group, "responses")),
                    "group_native_response_array_alias"
                );
                let count = if h.profile == "empty" {
                    0
                } else if h.profile == "callback-grow" {
                    3
                } else {
                    2
                };
                assert_eq!(
                    elements(&get(&h.group, "responses")).len(),
                    count,
                    "group_native_count"
                );
                if !matches!(h.profile.as_str(), "empty" | "resume") {
                    assert!(
                        eq(&get(&h.response_a, "authoritativeSeen"), &boolean(true)),
                        "group_native_authoritative_seen"
                    );
                }
                if count > 0 {
                    assert!(
                        Rc::ptr_eq(&elements(&get(&h.group, "creates"))[1], &h.create_b),
                        "group_native_create_alias"
                    );
                    assert!(
                        Rc::ptr_eq(&get(&h.response_b, "payload"), &h.payload_b),
                        "group_native_payload_alias"
                    );
                }
            } else if matches!(h.profile.as_str(), "disposed" | "boundary") {
                assert!(
                    eq(&get(&h.create_a, "uncertain"), &boolean(true)),
                    "group_native_uncertain"
                );
            } else {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
                    "group_native_error_identity"
                );
            }
        }
        "findOrder" => {
            let queries = match h.profile.as_str() {
                "disposed" => 0,
                "history" | "absent" | "cursor-stop" | "repeated-cursor" => 2,
                "limit" | "last-page" => 100,
                _ => 1,
            };
            assert_eq!(*h.queries.borrow(), queries, "find_native_request_floor");
            let reject = matches!(
                h.profile.as_str(),
                "disposed"
                    | "ret-failure"
                    | "malformed"
                    | "bad-cursor"
                    | "repeated-cursor"
                    | "limit"
                    | "reject"
                    | "empty-id"
                    | "invalid-status"
            );
            assert_eq!(result.is_err(), reject, "find_native_outcome");
            if !reject {
                if matches!(h.profile.as_str(), "absent" | "cursor-stop") {
                    assert!(
                        eq(result.as_ref().unwrap(), &undef()),
                        "find_native_undefined"
                    );
                } else {
                    assert!(
                        Rc::ptr_eq(result.as_ref().unwrap(), &h.found),
                        "find_native_original_alias"
                    );
                }
            }
            for call in h.calls.borrow().iter().filter(|c| c["name"] == "get") {
                let args = call["args"].as_array().unwrap();
                assert_eq!(args.len(), 2, "find_native_arguments");
                let fields = args[1]["o"].as_array().unwrap();
                let params = &fields[0][1]["o"];
                assert_eq!(params[0][1], atom(&raw("linear")), "find_native_category");
                assert_eq!(params[1][1], atom(&raw("BTCUSDT")), "find_native_symbol");
                assert_eq!(params[2][1], atom(&number(50.0)), "find_native_limit");
            }
        }
        "observePrivateOrders" => {
            let observed = elements(&h.initial_observed);
            let count = if h.profile == "empty" {
                1
            } else if h.profile == "non-string" {
                2
            } else {
                3
            };
            assert_eq!(observed.len(), count, "private_observe_native_ids");
            assert_eq!(
                h.count("observeOrders"),
                1,
                "private_observe_native_recovery_call"
            );
            if h.profile == "empty-id" {
                assert!(
                    observed.iter().any(|v| eq(v, &raw(""))),
                    "private_observe_native_empty_id"
                );
            }
        }
        "submitCreateOrders" => {
            let reject = matches!(h.profile.as_str(), "reject" | "submit-throw");
            assert_eq!(result.is_err(), reject, "publication_native_outcome");
            if !reject {
                assert!(
                    eq(&get(&h.response_a, "recorded"), &boolean(true)),
                    "publication_native_recorded"
                );
                let recorded = !matches!(
                    h.profile.as_str(),
                    "disposed"
                        | "observed"
                        | "authoritative"
                        | "terminal"
                        | "recovered"
                        | "reconciled"
                        | "existing"
                );
                assert_eq!(
                    h.count("recordRecentlyPlacedOrderFromOpts"),
                    usize::from(recorded),
                    "publication_native_record_call"
                );
                assert_eq!(
                    h.count("removeOrders"),
                    usize::from(h.profile == "terminal"),
                    "publication_native_terminal_call"
                );
                assert_eq!(
                    h.count("addOrUpdateOrders"),
                    usize::from(h.profile == "recovered"),
                    "publication_native_recovered_call"
                );
                assert!(
                    Rc::ptr_eq(&elements(result.as_ref().unwrap())[0], &h.response_a),
                    "publication_native_response_alias"
                );
            } else {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
                    "publication_native_error_identity"
                );
                assert_eq!(
                    h.logs.borrow().len(),
                    1,
                    "publication_native_error_emission"
                );
            }
        }
        "cancelOrdersHelper" => {
            let reject = matches!(
                h.profile.as_str(),
                "reject" | "second-reject" | "ret-failure"
            );
            assert_eq!(result.is_err(), reject, "cancel_native_outcome");
            let expected = if h.profile == "empty" || reject && h.profile != "second-reject" {
                0
            } else if h.profile == "second-reject" {
                1
            } else {
                2
            };
            assert_eq!(
                h.count("retireCancelledOrders"),
                expected,
                "cancel_native_retirement_count"
            );
            if expected > 0 {
                let receipt = h
                    .calls
                    .borrow()
                    .iter()
                    .find(|c| c["name"] == "retireCancelledOrders")
                    .unwrap()["args"][0]
                    .clone();
                let expected = if h.profile == "no-children" {
                    vec![atom(&h.cancel_a)]
                } else {
                    vec![atom(&h.cancel_a), atom(&h.child_a), atom(&h.child_b)]
                };
                assert_eq!(receipt, json!(expected), "cancel_native_original_aliases");
            }
            if reject {
                assert_eq!(h.logs.borrow().len(), 1, "cancel_native_emission");
            }
        }
        _ => panic!("native row"),
    }
}
#[test]
fn source_and_native_order_publication() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/hyro-order-publication-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 441, "declared source floor");
    let mut unique = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut comparisons = vec![];
    for case in cases {
        let method = case["method"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        assert!(unique.insert((method.to_string(), profile.to_string(), failure.to_string())));
        *counts.entry(method.to_string()).or_insert(0_usize) += 1;
        let h = Harness::new(method, profile, failure);
        let (result, sync) = h.invoke();
        native(&h, &result);
        let result = match result {
            Ok(value) => json!({"ok":true,"value":atom(&value)}),
            Err(error) => {
                json!({"ok":false,"error":atom(&error),"primary":Rc::ptr_eq(&error,&h.primary)})
            }
        };
        let receipt = json!({"calls":*h.calls.borrow(),"logs":*h.logs.borrow(),"syncCalls":sync,"result":result,"state":h.state()});
        comparisons.push((case, receipt));
    }
    assert_eq!(
        counts,
        std::collections::BTreeMap::from([
            ("observeOrders".into(), 42),
            ("dispose".into(), 14),
            ("unknown".into(), 49),
            ("runGroup".into(), 56),
            ("findOrder".into(), 112),
            ("observePrivateOrders".into(), 35),
            ("submitCreateOrders".into(), 70),
            ("cancelOrdersHelper".into(), 63)
        ]),
        "independent method floors"
    );
    assert!(
        unique.contains(&(
            "cancelOrdersHelper".into(),
            "repeated".into(),
            "emitAndLogError".into()
        )),
        "last declared iteration"
    );
    for (case, receipt) in comparisons {
        for key in ["calls", "logs", "syncCalls", "result", "state"] {
            assert_eq!(
                receipt[key], case[key],
                "supplemental {}/{}/{} {key}",
                case["method"], case["profile"], case["failure"]
            );
        }
    }
}
