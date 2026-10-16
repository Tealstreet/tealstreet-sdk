use safe_cex_venue_paradex::runtime_assembly::{
    self as api, RuntimeAssemblyHost as Host, RuntimeFuture,
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
    Map(RefCell<Vec<(Raw, Raw)>>),
    Set(RefCell<Vec<Raw>>),
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        tag: Option<String>,
        error: bool,
    },
    Function {
        owner: Raw,
        code: String,
        captures: Vec<Raw>,
    },
    Regex(String, String),
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
fn tag(v: &Raw) -> String {
    match v.as_ref() {
        Node::Object { tag: Some(s), .. } => s.clone(),
        _ => "this".into(),
    }
}
fn elements(v: &Raw) -> Vec<Raw> {
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
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        Node::Array(a) => String::from_utf16(key)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| a.borrow().get(i).cloned())
            .unwrap_or_else(undef),
        Node::Map(m) if key == u("size") => number(m.borrow().len() as f64),
        Node::Set(s) if key == u("size") => number(s.borrow().len() as f64),
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
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Map(m) => {
            json!({"map":m.borrow().iter().map(|(k,v)|json!([atom(k),atom(v)])).collect::<Vec<_>>()})
        }
        Node::Set(s) => json!({"set":s.borrow().iter().map(atom).collect::<Vec<_>>()}),
        Node::Regex(s, f) => json!({"regex":s,"flags":f}),
        Node::Function {
            owner,
            code,
            captures,
        } => {
            json!({"fn":code,"owner":tag(owner),"captures":captures.iter().map(atom).collect::<Vec<_>>()})
        }
        Node::Object { tag: Some(tag), .. } => json!({"ref":tag}),
        Node::Object { error: true, .. } => json!({"error":true,"message":js(&get(v,"message"))}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
    }
}
fn decode(v: &Value) -> Raw {
    if v.is_null() {
        return null();
    }
    if let Some(b) = v.as_bool() {
        return boolean(b);
    }
    if v["u"] == true {
        return undef();
    }
    if let Some(t) = v["s"].as_array() {
        return text(t.iter().map(|n| n.as_u64().unwrap() as u16).collect());
    }
    if let Some(n) = v["n"].as_str() {
        return number(f64::from_bits(u64::from_str_radix(n, 16).unwrap()));
    }
    if let Some(tag) = v["ref"].as_str() {
        return tagged(tag, vec![]);
    }
    if let Some(a) = v.as_array() {
        return array(a.iter().map(decode).collect());
    }
    if let Some(code) = v["fn"].as_str() {
        let owner = tagged(v["owner"].as_str().unwrap(), vec![]);
        let captures = v["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect();
        return func(owner, code, captures);
    }
    let fields = v["o"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (serde_json::from_value(p[0].clone()).unwrap(), decode(&p[1])))
        .collect();
    Rc::new(Node::Object {
        fields: RefCell::new(fields),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn block_on<F: Future>(f: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut f = pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(r) => return r,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
struct Def {
    owner: Raw,
    key: String,
    value: Raw,
    flags: (bool, bool, bool),
}
struct Harness {
    mode: String,
    profile: String,
    failure: String,
    counts: RefCell<Vec<(String, usize)>>,
    primary: Raw,
    root: RefCell<Raw>,
    global: Raw,
    prototypes: RefCell<Vec<(String, Raw)>>,
    defs: RefCell<Vec<Def>>,
    throttle_values: RefCell<Vec<(Raw, Raw)>>,
    probe_calls: RefCell<Vec<(Raw, usize)>>,
    ops: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Value>>,
    writes: RefCell<Vec<Value>>,
    request: RefCell<Option<Raw>>,
    success: RefCell<Option<Raw>>,
    error: RefCell<Option<Raw>>,
    serializer: RefCell<Option<Raw>>,
    retry_delay: RefCell<Option<Raw>>,
    xhr: Raw,
    positions: Raw,
    orders: Raw,
    emits: RefCell<Vec<Value>>,
    timers: RefCell<Vec<f64>>,
    poll: RefCell<Option<Raw>>,
    detached_errors: RefCell<Vec<Raw>>,
    rejections: RefCell<Vec<(Raw, Raw)>>,
}
impl Harness {
    fn new(mode: &str, profile: &str, failure: &str) -> Self {
        let primary = Rc::new(Node::Object {
            fields: RefCell::new(vec![(
                u("message"),
                text(u("primary-").into_iter().chain([0xd800]).collect()),
            )]),
            prototype: None,
            tag: None,
            error: true,
        });
        let global = tagged(
            "global",
            vec![
                ("Date", tagged("date", vec![])),
                ("qs", tagged("qs", vec![])),
            ],
        );
        let xhr = tagged(
            "xhr",
            vec![(
                "interceptors",
                object(vec![
                    ("request", tagged("request-use", vec![])),
                    ("response", tagged("response-use", vec![])),
                ]),
            )],
        );
        Self {
            mode: mode.into(),
            profile: profile.into(),
            failure: failure.into(),
            counts: RefCell::new(vec![]),
            primary,
            root: RefCell::new(tagged("this", vec![])),
            global,
            prototypes: RefCell::new(vec![]),
            defs: RefCell::new(vec![]),
            throttle_values: RefCell::new(vec![]),
            probe_calls: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
            request: RefCell::new(None),
            success: RefCell::new(None),
            error: RefCell::new(None),
            serializer: RefCell::new(None),
            retry_delay: RefCell::new(None),
            xhr,
            positions: array(vec![object(vec![(
                "id",
                text(u("pos-").into_iter().chain([0xd800]).collect()),
            )])]),
            orders: array(vec![object(vec![(
                "id",
                text(u("order-").into_iter().chain([0xd800]).collect()),
            )])]),
            emits: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            poll: RefCell::new(None),
            detached_errors: RefCell::new(vec![]),
            rejections: RefCell::new(vec![]),
        }
    }
    fn owner(&self) -> Raw {
        self.root.borrow().clone()
    }
    fn touch(&self, name: &str) -> Result<(), Raw> {
        let mut counts = self.counts.borrow_mut();
        let index = if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k == name) {
            let old = *n;
            *n += 1;
            old
        } else {
            counts.push((name.into(), 1));
            0
        };
        if self.failure == format!("{name}#{index}") {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn record(&self, name: &str, args: &[Raw]) -> Result<(), Raw> {
        self.touch(name)?;
        self.ops
            .borrow_mut()
            .push(json!({"name":name,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        Ok(())
    }
    fn map_get(&self, map: &Raw, key: &Raw) -> Raw {
        let Node::Map(m) = map.as_ref() else {
            panic!("map get")
        };
        m.borrow()
            .iter()
            .find(|(k, _)| eq(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(undef)
    }
    fn map_set(&self, map: &Raw, key: Raw, value: Raw) {
        let Node::Map(m) = map.as_ref() else {
            panic!("map set")
        };
        let mut m = m.borrow_mut();
        if let Some((_, v)) = m.iter_mut().find(|(k, _)| eq(k, &key)) {
            *v = value
        } else {
            m.push((key, value));
        }
    }
    fn execute_callback(&self, callback: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function {
            owner,
            code,
            captures,
        } = callback.as_ref()
        else {
            panic!("not callback")
        };
        match code.as_str() {
            "api-serialize" => api::serialize_api_params(self, args[0].clone()),
            "retry-delay" => api::api_retry_delay(self, args[0].clone()),
            "api-request" => block_on(api::api_request(self, owner.clone(), args[0].clone())),
            "api-success" => api::api_success(self, args[0].clone()),
            "api-error" => api::api_error(self, args[0].clone()),
            "ohlcv-cleanup" => api::cleanup_ohlcv_listener(
                self,
                owner.clone(),
                captures[0].clone(),
                captures[1].clone(),
                captures[2].clone(),
            ),
            "ohlcv-poll" => block_on(api::poll_ohlcv(
                self,
                owner.clone(),
                captures[0].clone(),
                captures[1].clone(),
                captures[2].clone(),
                captures[3].clone(),
                callback.clone(),
            )),
            "ohlcv-slippage-error" => api::ohlcv_slippage_error(
                self,
                owner.clone(),
                captures[0].clone(),
                self.primary.clone(),
            ),
            "listener" | "listener2" => {
                self.emits.borrow_mut().push(atom(&args[0]));
                if self.profile == "callback-failure" {
                    return Err(self.primary.clone());
                }
                Ok(undef())
            }
            "dispatch" => {
                let method = String::from_utf16(&js(&captures[0])).unwrap();
                let data = if self.truthy(&captures[1]) {
                    Some(args.first().cloned().unwrap_or_else(undef))
                } else {
                    None
                };
                api::invoke_bound_handler(self, owner.clone(), &method, data)
            }
            "rule-mint" => api::mint_rule_client_id(self),
            "rule-shape" => api::honorable_rule_shape(self, args[0].clone()),
            _ => panic!("unexecuted registered member {code}"),
        }
    }
    fn complete_class(&self, name: &str, args: &[Raw]) -> Result<Raw, Raw> {
        let owner = match name {
            "ParadexRequestSigner" => api::allocate_request_signer(self, args.to_vec())?,
            "ParadexJWTManager" => api::allocate_jwt_manager(self, args.to_vec())?,
            "ParadexExchange" => api::allocate_exchange(self, args.to_vec())?,
            "ParadexPrivateWebSocket" => api::allocate_private_socket(self, args.to_vec())?,
            "ParadexPublicWebSocket" => api::allocate_public_socket(self, args.to_vec())?,
            _ => unreachable!(),
        };
        match name {
            "ParadexRequestSigner" => {
                safe_cex_venue_paradex::auth_operations::initialize_request_signer(
                    &args[0],
                    Some(args[1].clone()),
                    || Ok::<Raw, Raw>(object(vec![])),
                    |v| {
                        set(&owner, "sdkAccount", v.clone());
                        Ok::<(), Raw>(())
                    },
                    |v| {
                        set(&owner, "options", v);
                        Ok::<(), Raw>(())
                    },
                )?
            }
            "ParadexJWTManager" => safe_cex_venue_paradex::auth_operations::initialize_jwt_manager(
                &args[0],
                self.truthy(&args[1]),
                args[2].clone(),
                Some(args[3].clone()),
                || Ok::<Raw, Raw>(object(vec![])),
                |v| {
                    set(&owner, "sdkAccount", v.clone());
                    Ok::<(), Raw>(())
                },
                |v| {
                    set(&owner, "options", v);
                    Ok::<(), Raw>(())
                },
                |v| {
                    set(&owner, "subkeyAddress", v);
                    Ok::<(), Raw>(())
                },
                |v| Ok::<Raw, Raw>(get(&get(v, "account"), "address")),
                |v| {
                    set(&owner, "mainAccountAddress", v);
                    Ok::<(), Raw>(())
                },
                |v| {
                    set(&owner, "apiBaseUrl", raw(v));
                    Ok::<(), Raw>(())
                },
            )?,
            "ParadexExchange" => {
                // Constructor body dependencies are declared here; its independent row is unchanged.
                self.record("createAPI", &[args[1].clone(), owner.clone()])?;
                let api = object(vec![("tag", raw("api"))]);
                self.record("rateLimit", &[api, object(vec![("maxRPS", number(10.0))])])?;
                set(&owner, "xhr", object(vec![("tag", raw("xhr"))]));
                self.record("newPublic", std::slice::from_ref(&owner))?;
                set(
                    &owner,
                    "publicWebsocket",
                    object(vec![("tag", raw("public"))]),
                );
                self.record("newPrivate", std::slice::from_ref(&owner))?;
                set(
                    &owner,
                    "privateWebsocket",
                    object(vec![("tag", raw("private"))]),
                );
            }
            "ParadexPrivateWebSocket" | "ParadexPublicWebSocket" => {
                let test = self.truthy(&get(&get(&args[0], "options"), "testnet"));
                let url = raw(if test {
                    "wss://ws.api.testnet.paradex.trade/v1?cancel-on-disconnect=false"
                } else {
                    "wss://ws.api.prod.paradex.trade/v1?cancel-on-disconnect=false"
                });
                if name == "ParadexPublicWebSocket" {
                    safe_cex_venue_paradex::public_ingress::initialize_public_socket(
                        &args[0],
                        |_| Ok::<(), Raw>(()),
                        || Ok::<Raw, Raw>(url.clone()),
                        |_| Ok::<bool, Raw>(test),
                        |v, _| Ok::<Raw, Raw>(v),
                        |v| {
                            set(&owner, "connectUrl", v);
                            Ok::<(), Raw>(())
                        },
                    )?;
                } else {
                    safe_cex_venue_paradex::private_ingress::initialize_private_socket(
                        &args[0],
                        |_| Ok::<(), Raw>(()),
                        || Ok::<Raw, Raw>(url.clone()),
                        |_| Ok::<bool, Raw>(test),
                        |v, _| Ok::<Raw, Raw>(v),
                        |v| {
                            set(&owner, "connectUrl", v);
                            Ok::<(), Raw>(())
                        },
                        |_| Ok::<(), Raw>(()),
                    )?;
                }
            }
            _ => unreachable!(),
        }
        Ok(owner)
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        Ok(get(v, k))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        Ok(get(v, k))
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(v) => *v,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(s) => !s.is_empty(),
            _ => true,
        }
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
    fn number(&self, n: f64) -> Raw {
        number(n)
    }
    fn boolean(&self, b: bool) -> Raw {
        boolean(b)
    }
    fn object(&self, f: Vec<(&'static str, Raw)>) -> Raw {
        object(f)
    }
    fn array(&self, v: Vec<Raw>) -> Raw {
        array(v)
    }
    fn template(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn numeric(&self, v: &Raw) -> Result<f64, Raw> {
        Ok(numeric(v))
    }
    fn global(&self) -> Raw {
        self.global.clone()
    }
    fn select(&self, receiver: Raw, name: &str) -> Result<Raw, Raw> {
        let value = get(&receiver, name);
        if let Node::Function { code, .. } = value.as_ref() {
            if code == "pong-probe" {
                return Ok(func(receiver, code, vec![]));
            }
        }
        Ok(func(receiver, name, vec![]))
    }
    fn apply(&self, selected: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function { owner, code, .. } = selected.as_ref() else {
            panic!("apply")
        };
        let a = |i| args.get(i).cloned().unwrap_or_else(undef);
        if let Node::Map(m) = owner.as_ref() {
            return Ok(match code.as_str() {
                "get" => self.map_get(owner, &a(0)),
                "set" => {
                    self.map_set(owner, a(0), a(1));
                    owner.clone()
                }
                "delete" => {
                    m.borrow_mut().retain(|(k, _)| !eq(k, &a(0)));
                    boolean(true)
                }
                _ => panic!("map op {code}"),
            });
        }
        if let Node::Set(s) = owner.as_ref() {
            return Ok(match code.as_str() {
                "add" => {
                    if !s.borrow().iter().any(|v| eq(v, &a(0))) {
                        s.borrow_mut().push(a(0));
                    }
                    owner.clone()
                }
                "delete" => {
                    s.borrow_mut().retain(|v| !eq(v, &a(0)));
                    boolean(true)
                }
                _ => panic!("set op {code}"),
            });
        }
        if tag(owner) == "headers" {
            let name = if code == "set" {
                "header.set"
            } else {
                "header.has"
            };
            self.record(name, &args)?;
            return Ok(if code == "set" {
                set_units(owner, js(&a(0)), a(1));
                undef()
            } else {
                boolean(!matches!(
                    get_units(owner, &js(&a(0))).as_ref(),
                    Node::Undefined
                ))
            });
        }
        let result = match code.as_str() {
            "pong-probe" => {
                self.probe_calls
                    .borrow_mut()
                    .push((owner.clone(), args.len()));
                undef()
            }
            "patternClientOrderIdRule" => {
                self.record("rule", &args)?;
                object(vec![
                    ("label", a(0)),
                    ("regex", a(1)),
                    ("mint", get(&a(2), "mint")),
                    ("isHonorableShape", get(&a(2), "isHonorableShape")),
                ])
            }
            "uuid" => text(u("id-").into_iter().chain([0xd800]).collect()),
            "isRegularClientOrderShape" => boolean(self.truthy(&get(&a(0), "regular"))),
            "throttle" => {
                self.record("throttle", &args)?;
                let callback = a(0);
                let Node::Function {
                    owner,
                    code,
                    captures,
                } = callback.as_ref()
                else {
                    panic!("throttle callback")
                };
                let wrapped = func(owner.clone(), code, captures.clone());
                self.throttle_values
                    .borrow_mut()
                    .push((callback, wrapped.clone()));
                wrapped
            }
            "safeCexAxiosCreate" => {
                self.record("create", &args)?;
                *self.serializer.borrow_mut() =
                    Some(get(&get(&a(0), "paramsSerializer"), "serialize"));
                self.xhr.clone()
            }
            "networkErrorOnRead" => {
                self.record("network", &args)?;
                func(self.owner(), "network", vec![])
            }
            "retry" => {
                self.record("retry", &args)?;
                *self.retry_delay.borrow_mut() = Some(get(&a(1), "retryDelay"));
                undef()
            }
            "installTransportAdapter" => {
                self.record("install", &args)?;
                undef()
            }
            "use" if tag(owner) == "request-use" => {
                self.record("request.use", &args)?;
                *self.request.borrow_mut() = Some(a(0));
                undef()
            }
            "use" if tag(owner) == "response-use" => {
                self.record("response.use", &args)?;
                *self.success.borrow_mut() = Some(a(0));
                *self.error.borrow_mut() = Some(a(1));
                undef()
            }
            "stringify" => {
                self.record("qs", &args)?;
                raw("q=1,2")
            }
            "authenticateRequest" => {
                self.record("authenticate", &args)?;
                a(0)
            }
            "log" => {
                self.touch("log")?;
                self.logs
                    .borrow_mut()
                    .push(json!(args.iter().map(atom).collect::<Vec<_>>()));
                undef()
            }
            "now" => {
                self.record("now", &args)?;
                number(123456.0)
            }
            "fetchPositions" | "fetchOrders" => {
                self.record(code, &args)?;
                if self.profile == "dispose-during-fetch" {
                    set(&self.owner(), "isDisposed", boolean(true));
                }
                if code == "fetchPositions" {
                    self.positions.clone()
                } else {
                    self.orders.clone()
                }
            }
            "addOrUpdatePositionsFull" => {
                self.record("positions", &args)?;
                undef()
            }
            "update" => {
                self.record("update", &args)?;
                undef()
            }
            "subscribeToTrades" => {
                self.record("subscribe", &args)?;
                undef()
            }
            "unsubscribeFromTrades" => {
                self.record("unsubscribe", &args)?;
                undef()
            }
            "cleanupOhlcvSubscription" => {
                self.record("cleanup", &args)?;
                let root = self.owner();
                self.map_set(&get(&root, "ohlcvPollingActive"), a(0), boolean(false));
                let versions = get(&root, "ohlcvSubscriptionVersions");
                let old = self.map_get(&versions, &a(0));
                self.map_set(
                    &versions,
                    a(0),
                    number(if self.truthy(&old) {
                        numeric(&old) + 1.0
                    } else {
                        1.0
                    }),
                );
                for name in [
                    "ohlcvCallbacks",
                    "ohlcvIntervals",
                    "lastCandleCache",
                    "ohlcvPollingTimeouts",
                ] {
                    let map = get(&root, name);
                    let Node::Map(m) = map.as_ref() else { panic!() };
                    m.borrow_mut().retain(|(key, _)| !eq(key, &a(0)));
                }
                undef()
            }
            "setMaxSlippage" => {
                self.record("slippage", &args)?;
                tagged("promise", vec![])
            }
            "fetchOHLCV" => {
                self.record("fetch", &args)?;
                let root = self.owner();
                let key = text(u("X").into_iter().chain([0xd800]).chain(u(":1m")).collect());
                if self.profile == "fetch-failure" {
                    return Err(self.primary.clone());
                }
                if self.profile == "inactive-inflight" {
                    self.map_set(&get(&root, "ohlcvPollingActive"), key, boolean(false));
                } else if self.profile == "replace-version" {
                    self.map_set(&get(&root, "ohlcvSubscriptionVersions"), key, number(8.0));
                }
                if self.profile == "empty" {
                    array(vec![])
                } else {
                    array(vec![candle()])
                }
            }
            "maybeRecoverOhlcvTradeStream" => {
                self.record("recover", &args)?;
                if self.profile == "recovery-failure" {
                    return Err(self.primary.clone());
                }
                undef()
            }
            "setTimeout" => {
                self.record("setTimeout", &[a(1)])?;
                *self.poll.borrow_mut() = Some(a(0));
                self.timers.borrow_mut().push(numeric(&a(1)));
                number(7.0)
            }
            _ => panic!("unexpected op {code}/{}", tag(owner)),
        };
        Ok(result)
    }
    fn invoke(&self, s: Raw, args: Vec<Raw>) -> Result<RuntimeFuture<'_, Raw, Raw>, Raw> {
        Ok(Box::pin(async move { self.apply(s, args) }))
    }
    fn wait(&self, p: Raw) -> Result<RuntimeFuture<'_, Raw, Raw>, Raw> {
        assert!(
            Rc::ptr_eq(&p, &get(&self.owner(), "sdkInitPromise")),
            "api_native_wait_promise_identity"
        );
        Ok(Box::pin(async { Ok(undef()) }))
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        let a = |i| args.get(i).cloned().unwrap_or_else(undef);
        Ok(match name {
            "null" => null(),
            "new Map" => Rc::new(Node::Map(RefCell::new(vec![]))),
            "new Set" => Rc::new(Node::Set(RefCell::new(vec![]))),
            "Set.from" => Rc::new(Node::Set(RefCell::new(elements(&a(0))))),
            "RegExp" => Rc::new(Node::Regex(
                String::from_utf16(&js(&a(0))).unwrap(),
                String::from_utf16(&js(&a(1))).unwrap(),
            )),
            "text" => text(elements(&a(0)).iter().map(|v| numeric(v) as u16).collect()),
            "multiply" => number(numeric(&a(0)) * numeric(&a(1))),
            "subtract" => number(numeric(&a(0)) - numeric(&a(1))),
            "add" => number(numeric(&a(0)) + numeric(&a(1))),
            "includes" => {
                let value = js(&a(0));
                let search = js(&a(1));
                boolean(value.windows(search.len()).any(|w| w == search))
            }
            "Object.create" => Rc::new(Node::Object {
                fields: RefCell::new(vec![]),
                prototype: Some(a(0)),
                tag: None,
                error: false,
            }),
            "spread" => {
                let value = a(0);
                let Node::Object { fields, .. } = value.as_ref() else {
                    panic!()
                };
                Rc::new(Node::Object {
                    fields: RefCell::new(fields.borrow().clone()),
                    prototype: None,
                    tag: None,
                    error: false,
                })
            }
            "member" => get(&a(0), &numeric(&a(1)).to_string()),
            "timeframe" => {
                if js(&a(0)) == u("1m") {
                    number(60.0)
                } else {
                    undef()
                }
            }
            "broadcast" => {
                let set = a(0);
                let Node::Set(s) = set.as_ref() else { panic!() };
                let mut i = 0;
                loop {
                    let next = s.borrow().get(i).cloned();
                    let Some(next) = next else { break };
                    self.execute_callback(next, vec![a(1)])?;
                    i += 1;
                }
                undef()
            }
            _ => panic!("intrinsic {name}"),
        })
    }
    fn write(&self, owner: &Raw, key: &str, value: Raw) -> Result<(), Raw> {
        if self.mode == "private"
            && Rc::ptr_eq(owner, &self.owner())
            && [
                "lastPositionFetchTime",
                "hasPendingPositionEvents",
                "ordersThrottleTimeout",
            ]
            .contains(&key)
        {
            self.touch(&format!("write:{key}"))?;
            self.writes
                .borrow_mut()
                .push(json!({"key":key,"value":atom(&value)}));
        }
        set(owner, key, value);
        Ok(())
    }
    fn allocate(&self, class: &'static str, args: Vec<Raw>) -> Result<Raw, Raw> {
        let proto = {
            let mut protos = self.prototypes.borrow_mut();
            if let Some((_, p)) = protos.iter().find(|(name, _)| name == class) {
                p.clone()
            } else {
                let proto = object(vec![("nominal", raw(class))]);
                protos.push((class.into(), proto.clone()));
                proto
            }
        };
        let owner = Rc::new(Node::Object {
            fields: RefCell::new(vec![]),
            prototype: Some(proto),
            tag: Some("this".into()),
            error: false,
        });
        *self.root.borrow_mut() = owner.clone();
        if class == "ParadexExchange" {
            self.record("base", &args)?;
            set(&owner, "name", args[0].clone());
            set(&owner, "options", args[1].clone());
            set(&owner, "store", args[2].clone());
        } else if class.contains("WebSocket") {
            self.record("base", &args)?;
            set(&owner, "parent", args[0].clone());
        }
        Ok(owner)
    }
    fn define(
        &self,
        owner: &Raw,
        key: &'static str,
        value: Raw,
        w: bool,
        e: bool,
        c: bool,
    ) -> Result<(), Raw> {
        self.touch(&format!("define:{key}"))?;
        self.defs.borrow_mut().push(Def {
            owner: owner.clone(),
            key: key.into(),
            value: value.clone(),
            flags: (w, e, c),
        });
        set(owner, key, value);
        Ok(())
    }
    fn callback(&self, owner: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(func(owner, code, captures))
    }
    fn start(&self, callback: Raw) -> Result<(), Raw> {
        match self.execute_callback(callback, vec![]) {
            Ok(_) => {}
            Err(e) => self.detached_errors.borrow_mut().push(e),
        }
        Ok(())
    }
    fn observe(&self, p: Raw, cb: Raw) -> Result<(), Raw> {
        self.rejections.borrow_mut().push((p, cb));
        Ok(())
    }
    fn make_error(&self, message: Vec<u16>) -> Result<Raw, Raw> {
        let value = text(message);
        let error = Rc::new(Node::Object {
            fields: RefCell::new(vec![(u("message"), value.clone())]),
            prototype: None,
            tag: None,
            error: true,
        });
        self.defs.borrow_mut().push(Def {
            owner: error.clone(),
            key: "message".into(),
            value,
            flags: (true, false, true),
        });
        Ok(error)
    }
    fn error_value(&self, e: &Raw) -> Raw {
        e.clone()
    }
    fn error_from_value(&self, e: Raw) -> Raw {
        e
    }
}
fn set_units(owner: &Raw, key: Vec<u16>, value: Raw) {
    let Node::Object { fields, .. } = owner.as_ref() else {
        panic!()
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == key) {
        *v = value
    } else {
        fields.push((key, value));
    }
}
fn candle() -> Raw {
    object(vec![
        ("timestamp", number(1.0)),
        ("close", number(2.0)),
        ("volume", number(3.0)),
        ("open", number(0.0)),
    ])
}
fn basic_outcome(h: &Harness, result: &Result<Raw, Raw>) -> Value {
    match result {
        Ok(_) => json!({"ok":true}),
        Err(e) => {
            assert!(Rc::ptr_eq(e, &h.primary), "runtime_native_primary_identity");
            json!({"ok":false,"primary":true})
        }
    }
}
#[test]
fn source_oracle() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/runtime_assembly_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source instrument: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["classes"].as_array().unwrap().len(), 60);
    assert_eq!(corpus["apis"].as_array().unwrap().len(), 174);
    assert_eq!(corpus["responses"].as_array().unwrap().len(), 4);
    assert_eq!(corpus["private"].as_array().unwrap().len(), 72);
    assert_eq!(corpus["ohlcv"].as_array().unwrap().len(), 20);
    for case in corpus["classes"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let h = Harness::new("class", profile, failure);
        let args = elements(&decode(&case["input"]["args"]));
        if name.contains("WebSocket") {
            set(
                &args[0],
                "options",
                object(vec![("testnet", boolean(profile == "testnet"))]),
            );
        }
        let result = h.complete_class(name, &args);
        let mut probes = json!({});
        let outcome = match result {
            Ok(first) => {
                if profile == "basic" && failure == "none" {
                    match name {
                        "ParadexRequestSigner" => {
                            let defs = h.defs.borrow();
                            assert_eq!(defs[0].key, "sdkAccount", "signer_native_initial_field");
                            assert!(
                                matches!(defs[0].value.as_ref(), Node::Undefined),
                                "signer_native_initial_value"
                            );
                        }
                        "ParadexJWTManager" => assert_eq!(
                            numeric(&get(&first, "tokenExpiry")),
                            0.0,
                            "jwt_class_native_expiry"
                        ),
                        "ParadexExchange" => assert!(
                            matches!(get(&first, "leverageSettings").as_ref(), Node::Map(_)),
                            "exchange_class_native_leverage_map"
                        ),
                        "ParadexPrivateWebSocket" => assert_eq!(
                            numeric(&get(&first, "positionFetchThrottleMs")),
                            500.0,
                            "private_class_native_position_throttle"
                        ),
                        "ParadexPublicWebSocket" => assert!(
                            matches!(get(&first, "ohlcvLastRecoveryAt").as_ref(), Node::Map(_)),
                            "public_class_native_recovery_map"
                        ),
                        _ => unreachable!(),
                    }
                }
                if name == "ParadexPublicWebSocket" {
                    for (argument, returned) in h.throttle_values.borrow().iter() {
                        let Node::Function { code, .. } = returned.as_ref() else {
                            panic!("returned throttle")
                        };
                        assert!(
                            !Rc::ptr_eq(argument, returned),
                            "class_native_throttle_factory_identity"
                        );
                        assert!(
                            Rc::ptr_eq(&get(&first, code), returned),
                            "class_native_retains_returned_throttle"
                        );
                    }
                }
                let second = h.complete_class(name, &args).unwrap();
                let Node::Object {
                    prototype: Some(proto),
                    fields,
                    ..
                } = first.as_ref()
                else {
                    panic!("class native prototype")
                };
                let expected = h
                    .prototypes
                    .borrow()
                    .iter()
                    .find(|(n, _)| n == name)
                    .unwrap()
                    .1
                    .clone();
                assert!(
                    Rc::ptr_eq(proto, &expected),
                    "class_native_nominal_prototype"
                );
                assert!(
                    !Rc::ptr_eq(&first, &second),
                    "class_native_fresh_allocation"
                );
                let collections = fields
                    .borrow()
                    .iter()
                    .filter_map(|(_, value)| {
                        if matches!(value.as_ref(), Node::Map(_) | Node::Set(_)) {
                            Some(value.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                for (i, value) in collections.iter().enumerate() {
                    for other in &collections[i + 1..] {
                        assert!(
                            !Rc::ptr_eq(value, other),
                            "class_native_distinct_collections"
                        );
                    }
                }
                for (key, value) in fields.borrow().iter() {
                    match value.as_ref() {
                        Node::Map(_) | Node::Set(_) => assert!(
                            !Rc::ptr_eq(value, &get_units(&second, key)),
                            "class_native_fresh_collection"
                        ),
                        Node::Function { owner, .. } => {
                            assert!(Rc::ptr_eq(owner, &first), "class_native_arrow_receiver");
                            assert!(
                                !Rc::ptr_eq(value, &get_units(&second, key)),
                                "class_native_fresh_arrow"
                            );
                        }
                        _ => {}
                    }
                }
                if name == "ParadexExchange" {
                    let mint = api::mint_rule_client_id(&h).unwrap();
                    let shape = api::honorable_rule_shape(
                        &h,
                        object(vec![("regular", boolean(true)), ("close", boolean(false))]),
                    )
                    .unwrap();
                    let closed = api::honorable_rule_shape(
                        &h,
                        object(vec![("regular", boolean(true)), ("close", boolean(true))]),
                    )
                    .unwrap();
                    assert_eq!(
                        js(&mint),
                        u("ts_id-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                        "class_native_rule_mint"
                    );
                    probes = json!({"mint":js(&mint),"shape":h.truthy(&shape),"closed":h.truthy(&closed)});
                }
                if name == "ParadexPublicWebSocket" {
                    set(
                        &first,
                        "handlePongEvent",
                        func(first.clone(), "pong-probe", vec![]),
                    );
                    let callback =
                        elements(&get(&get(&first, "messageHandlers"), "ping"))[0].clone();
                    let Node::Function { owner, .. } = callback.as_ref() else {
                        panic!("handler callback")
                    };
                    assert!(Rc::ptr_eq(owner, &first), "class_native_handler_capture");
                    h.execute_callback(callback, vec![number(123.0)]).unwrap();
                    let calls = h.probe_calls.borrow();
                    assert_eq!(calls.len(), 1, "class_native_handler_call_count");
                    assert!(
                        Rc::ptr_eq(&calls[0].0, &first),
                        "class_native_handler_dynamic_receiver"
                    );
                    assert_eq!(calls[0].1, 0, "class_native_handler_drops_ping_data");
                    probes = json!({"handlerBound":Rc::ptr_eq(&calls[0].0,&first),"handlerArgc":calls[0].1});
                }
                json!({"ok":true,"prototype":Rc::ptr_eq(proto,&expected),"fresh":!Rc::ptr_eq(&first,&second),"collections":true,"functions":true})
            }
            Err(e) => {
                assert!(Rc::ptr_eq(&e, &h.primary), "class_native_failure_identity");
                json!({"ok":false,"primary":true})
            }
        };
        for def in h.defs.borrow().iter() {
            assert_eq!(
                def.flags,
                (true, true, true),
                "class_native_field_attributes"
            );
        }
        let defs=h.defs.borrow().iter().map(|d|json!({"key":d.key,"value":atom(&d.value),"writable":d.flags.0,"enumerable":d.flags.1,"configurable":d.flags.2})).collect::<Vec<_>>();
        assert_eq!(
            json!(defs),
            case["defs"],
            "class_supplemental_definitions {name}/{profile}/{failure}"
        );
        assert_eq!(
            json!(*h.ops.borrow()),
            case["ops"],
            "class_supplemental_ops {name}/{profile}/{failure}"
        );
        assert_eq!(outcome, case["outcome"], "class_supplemental_outcome");
        assert_eq!(probes, case["probes"], "class_supplemental_probes");
    }
    for case in corpus["apis"].as_array().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let mode = case["requestMode"].as_str().unwrap();
        let h = Harness::new("api", profile, failure);
        let owner = h.owner();
        set(&owner, "name", raw("paradex"));
        set(
            &owner,
            "sdkInitPromise",
            if case["input"]["sdkWait"] == true {
                tagged("promise", vec![])
            } else {
                null()
            },
        );
        let result = api::create_api(&h, decode(&case["input"]["opts"]), owner.clone());
        if profile == "basic" && failure == "none" && mode.is_empty() {
            let ops = h.ops.borrow();
            let create = ops.iter().find(|v| v["name"] == "create").unwrap();
            assert_eq!(
                numeric(&get(&decode(&create["args"][0]), "timeout")),
                20_000.0,
                "api_native_timeout"
            );
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap(), &h.xhr),
                "api_native_xhr_identity"
            );
        }
        let outcome = match &result {
            Ok(v) => json!({"ok":true,"identity":Rc::ptr_eq(v,&h.xhr)}),
            Err(e) => {
                assert!(
                    Rc::ptr_eq(e, &h.primary),
                    "api_native_factory_error_identity"
                );
                json!({"ok":false,"primary":true})
            }
        };
        let mut secondary = json!({});
        if result.is_ok() {
            if !mode.is_empty() {
                let headers = tagged("headers", vec![]);
                if mode == "signed" {
                    set(&headers, "PARADEX-STARKNET-SIGNATURE", raw("s"));
                }
                let config = tagged(
                    "config",
                    vec![
                        (
                            "url",
                            if mode == "missing-url" {
                                undef()
                            } else {
                                raw(if mode == "public" {
                                    "/markets/klines"
                                } else if mode == "account" {
                                    "/markets/account"
                                } else {
                                    "/orders"
                                })
                            },
                        ),
                        (
                            "method",
                            raw(if mode == "upper-post" { "POST" } else { "post" }),
                        ),
                        ("headers", headers.clone()),
                    ],
                );
                let cb = h.request.borrow().as_ref().unwrap().clone();
                let r = h.execute_callback(cb, vec![config.clone()]);
                secondary = match r {
                    Ok(v) => json!({"ok":true,"identity":Rc::ptr_eq(&v,&config)}),
                    Err(e) => {
                        assert!(
                            Rc::ptr_eq(&e, &h.primary),
                            "api_native_request_error_identity"
                        );
                        json!({"ok":false,"primary":true})
                    }
                };
                let Node::Object { fields, .. } = headers.as_ref() else {
                    panic!()
                };
                secondary["headers"] = json!(
                    fields
                        .borrow()
                        .iter()
                        .map(|(k, v)| json!([k, js(v)]))
                        .collect::<Vec<_>>()
                );
            } else {
                let a = h.serializer.borrow().as_ref().unwrap().clone();
                let b = h.retry_delay.borrow().as_ref().unwrap().clone();
                let s = h.execute_callback(
                    a,
                    vec![object(vec![("q", array(vec![number(1.0), number(2.0)]))])],
                );
                secondary = match s {
                    Ok(s) => {
                        let delay = h.execute_callback(b, vec![number(2.0)]).unwrap();
                        json!({"serialized":atom(&s),"delay":atom(&delay)})
                    }
                    Err(e) => {
                        assert!(
                            Rc::ptr_eq(&e, &h.primary),
                            "api_native_serializer_error_identity"
                        );
                        json!({"ok":false,"primary":true})
                    }
                };
            }
        }
        assert_eq!(
            outcome, case["outcome"],
            "api_supplemental_outcome {profile}/{failure}/{mode}"
        );
        assert_eq!(
            secondary, case["secondary"],
            "api_supplemental_secondary {profile}/{failure}/{mode}"
        );
        assert_eq!(
            json!(*h.ops.borrow()),
            case["ops"],
            "api_supplemental_ops {profile}/{failure}/{mode}"
        );
        assert_eq!(
            json!(*h.logs.borrow()),
            case["logs"],
            "api_supplemental_logs"
        );
    }
    for case in corpus["responses"].as_array().unwrap() {
        let mode = case["mode"].as_str().unwrap();
        let h = Harness::new("response", mode, "none");
        let response = decode(&case["input"]["response"]);
        let original = h
            .make_error(u("original-").into_iter().chain([0xd800]).collect())
            .unwrap();
        set(&original, "response", response.clone());
        set(&original, "config", get(&response, "config"));
        set(&original, "request", get(&response, "request"));
        let r = if mode == "error" {
            api::api_error(&h, original.clone())
        } else {
            api::api_success(&h, response.clone())
        };
        let outcome = match r {
            Ok(v) => {
                assert!(Rc::ptr_eq(&v, &response), "api_native_response_identity");
                json!({"ok":true,"identity":true})
            }
            Err(e) => {
                let prototype = matches!(e.as_ref(),Node::Object{prototype:Some(p),..} if Rc::ptr_eq(p,&original));
                if mode == "error" {
                    assert!(prototype, "api_native_error_prototype");
                }
                assert!(
                    Rc::ptr_eq(&get(&e, "response"), &response),
                    "api_native_error_response_identity"
                );
                let defs = h.defs.borrow();
                let descriptor = defs
                    .iter()
                    .find(|d| Rc::ptr_eq(&d.owner, &e) && d.key == "message")
                    .unwrap();
                if mode == "error" {
                    assert_eq!(
                        descriptor.flags,
                        (false, true, true),
                        "api_native_message_descriptor"
                    );
                }
                let descriptor = json!({"writable":descriptor.flags.0,"enumerable":descriptor.flags.1,"configurable":descriptor.flags.2});
                json!({"ok":false,"message":js(&get(&e,"message")),"original":Rc::ptr_eq(&e,&original),"prototype":prototype,"axios":h.truthy(&get(&e,"isAxiosError")),"response":Rc::ptr_eq(&get(&e,"response"),&response),"config":Rc::ptr_eq(&get(&e,"config"),&get(&response,"config")),"request":Rc::ptr_eq(&get(&e,"request"),&get(&response,"request")),"descriptor":descriptor})
            }
        };
        assert_eq!(outcome, case["outcome"], "api_response_supplemental {mode}");
    }
    for case in corpus["private"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let profile = case["profile"].as_str().unwrap();
        let failure = case["failure"].as_str().unwrap();
        let h = Harness::new("private", profile, failure);
        let root = h.owner();
        set(
            &root,
            "isDisposed",
            boolean(case["input"]["disposed"] == true),
        );
        set(&root, "hasPendingPositionEvents", boolean(true));
        set(&root, "lastPositionFetchTime", number(0.0));
        set(&root, "ordersThrottleTimeout", number(9.0));
        set(
            &root,
            "parent",
            object(vec![
                (
                    "isDisposed",
                    boolean(case["input"]["parentDisposed"] == true),
                ),
                (
                    "store",
                    object(vec![("loaded", decode(&case["input"]["loaded"]))]),
                ),
            ]),
        );
        let r = if name == "executePositionFetch" {
            block_on(api::execute_position_fetch(&h, root.clone()))
        } else {
            block_on(api::fetch_throttled_orders(&h, root.clone()))
        };
        if profile == "basic" && failure == "none" {
            if name == "executePositionFetch" {
                assert!(
                    !h.truthy(&get(&root, "hasPendingPositionEvents")),
                    "positions_native_pending_reset"
                );
                assert_eq!(
                    numeric(&get(&root, "lastPositionFetchTime")),
                    123456.0,
                    "positions_native_timestamp"
                );
            } else {
                let ops = h.ops.borrow();
                let update = ops.iter().find(|v| v["name"] == "update").unwrap();
                assert!(
                    h.truthy(&get(&get(&decode(&update["args"][0]), "loaded"), "orders")),
                    "orders_native_loaded"
                );
            }
        }
        let outcome = basic_outcome(&h, &r);
        let expected_correction = if name == "executePositionFetch" {
            [
                "now#0",
                "write:lastPositionFetchTime#0",
                "write:hasPendingPositionEvents#0",
                "fetchPositions#0",
            ]
            .contains(&failure)
                || (failure == "positions#0" && profile == "basic")
        } else {
            ["fetchOrders#0", "update#0"].contains(&failure) && profile != "disposed"
        };
        if expected_correction {
            assert!(r.is_err(), "private_native_visible_failure");
            assert_eq!(
                case["outcome"],
                json!({"ok":true}),
                "source_native_silent_catch_boundary"
            );
        } else {
            assert_eq!(outcome, case["outcome"], "private_supplemental_outcome");
        }
        let final_state = json!({"pending":h.truthy(&get(&root,"hasPendingPositionEvents")),"last":atom(&get(&root,"lastPositionFetchTime")),"timeout":atom(&get(&root,"ordersThrottleTimeout"))});
        assert_eq!(
            json!(*h.ops.borrow()),
            case["ops"],
            "private_supplemental_ops {name}/{profile}/{failure}"
        );
        assert_eq!(
            json!(*h.writes.borrow()),
            case["writes"],
            "private_supplemental_writes {name}/{profile}/{failure}"
        );
        assert_eq!(final_state, case["final"], "private_supplemental_state");
    }
    for case in corpus["ohlcv"].as_array().unwrap() {
        let profile = case["profile"].as_str().unwrap();
        let h = Harness::new("ohlcv", profile, "none");
        let root = h.owner();
        for name in [
            "ohlcvCallbacks",
            "ohlcvIntervals",
            "ohlcvSubscriptionVersions",
            "ohlcvPollingActive",
            "lastCandleCache",
            "ohlcvPollingTimeouts",
        ] {
            set(&root, name, Rc::new(Node::Map(RefCell::new(vec![]))));
        }
        let symbol = text(u("X").into_iter().chain([0xd800]).collect());
        let interval = raw(if profile == "unknown-interval" {
            "?"
        } else {
            "1m"
        });
        let key = text(
            js(&symbol)
                .into_iter()
                .chain(u(":"))
                .chain(js(&interval))
                .collect(),
        );
        let opts = object(vec![("symbol", symbol.clone()), ("interval", interval)]);
        let listener = func(root.clone(), "listener", vec![]);
        let other = func(root.clone(), "listener2", vec![]);
        if profile == "existing" {
            h.map_set(
                &get(&root, "ohlcvCallbacks"),
                key.clone(),
                Rc::new(Node::Set(RefCell::new(vec![other.clone()]))),
            );
        }
        if profile == "unchanged" {
            h.map_set(&get(&root, "lastCandleCache"), key.clone(), candle());
        }
        if profile == "existing-version" {
            h.map_set(
                &get(&root, "ohlcvSubscriptionVersions"),
                key.clone(),
                number(4.0),
            );
        }
        set(
            &root,
            "parent",
            object(vec![
                ("isPublicDataOnly", boolean(profile == "public-only")),
                (
                    "jwtManager",
                    if profile == "no-jwt" {
                        null()
                    } else {
                        object(vec![])
                    },
                ),
            ]),
        );
        let cleanup = api::listen_ohlcv(&h, root.clone(), opts.clone(), listener.clone()).unwrap();
        let rejected = h
            .rejections
            .borrow()
            .iter()
            .map(|(_, cb)| cb.clone())
            .collect::<Vec<_>>();
        if profile == "slippage-failure" {
            for cb in rejected {
                h.execute_callback(cb, vec![]).unwrap();
            }
        }
        if profile == "duplicate" {
            api::listen_ohlcv(&h, root.clone(), opts.clone(), listener.clone()).unwrap();
        }
        if profile == "two" {
            api::listen_ohlcv(&h, root.clone(), opts.clone(), other.clone()).unwrap();
        }
        if profile == "replace-callbacks" {
            h.map_set(
                &get(&root, "ohlcvCallbacks"),
                key.clone(),
                Rc::new(Node::Set(RefCell::new(vec![other]))),
            );
        }
        if profile == "second-poll" {
            let poll = h.poll.borrow_mut().take().unwrap();
            h.execute_callback(poll, vec![]).unwrap();
        }
        if profile == "basic" {
            assert_eq!(*h.timers.borrow(), vec![3000.0], "ohlcv_native_poll_delay");
            assert_eq!(h.emits.borrow().len(), 1, "ohlcv_native_emission_count");
            assert_eq!(
                numeric(&get(
                    &h.map_get(&get(&root, "lastCandleCache"), &key),
                    "close"
                )),
                2.0,
                "ohlcv_native_cached_close"
            );
        }
        if ["fetch-failure", "callback-failure", "recovery-failure"].contains(&profile) {
            assert_eq!(
                h.detached_errors.borrow().len(),
                1,
                "ohlcv_native_visible_failure_count"
            );
            assert!(
                Rc::ptr_eq(&h.detached_errors.borrow()[0], &h.primary),
                "ohlcv_native_visible_failure_identity"
            );
            assert_eq!(
                *h.timers.borrow(),
                vec![3000.0],
                "ohlcv_native_failure_retains_schedule"
            );
        } else {
            assert!(
                h.detached_errors.borrow().is_empty(),
                "ohlcv_native_no_spurious_failure"
            );
        }
        let before = json!({"callbacks":optional_size(&h.map_get(&get(&root,"ohlcvCallbacks"),&key)),"interval":atom(&h.map_get(&get(&root,"ohlcvIntervals"),&key)),"version":atom(&h.map_get(&get(&root,"ohlcvSubscriptionVersions"),&key)),"active":atom(&h.map_get(&get(&root,"ohlcvPollingActive"),&key)),"cache":atom(&h.map_get(&get(&root,"lastCandleCache"),&key))});
        if profile == "mutate-opts" {
            set(
                &opts,
                "symbol",
                text(u("Y").into_iter().chain([0xd800]).collect()),
            );
        }
        h.execute_callback(cleanup.clone(), vec![]).unwrap();
        if profile == "repeat-cleanup" {
            h.execute_callback(cleanup, vec![]).unwrap();
        }
        let after = json!({"callbacks":optional_size(&h.map_get(&get(&root,"ohlcvCallbacks"),&key)),"active":atom(&h.map_get(&get(&root,"ohlcvPollingActive"),&key)),"version":atom(&h.map_get(&get(&root,"ohlcvSubscriptionVersions"),&key))});
        for (key, actual) in [
            ("ops", json!(*h.ops.borrow())),
            ("emits", json!(*h.emits.borrow())),
            (
                "timers",
                json!(
                    h.timers
                        .borrow()
                        .iter()
                        .map(|v| format!("{:016x}", v.to_bits()))
                        .collect::<Vec<_>>()
                ),
            ),
            ("logs", json!(*h.logs.borrow())),
            ("before", before),
            ("after", after),
        ] {
            assert_eq!(actual, case[key], "ohlcv_supplemental_{key} {profile}");
        }
    }
}
fn optional_size(v: &Raw) -> Option<usize> {
    match v.as_ref() {
        Node::Undefined => None,
        Node::Set(s) => Some(s.borrow().len()),
        _ => panic!("expected set"),
    }
}
