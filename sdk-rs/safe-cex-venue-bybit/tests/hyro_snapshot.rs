use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost as Host},
    hyro_snapshot as api,
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
        Node::Function { .. } => json!({"fn":"current"}),
        Node::Object { tag: Some(tag), .. } => json!({"ref":tag}),
        Node::Object { error: true, .. } => json!({"error":true,"message":js(&get(v,"message"))}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
    }
}
fn error(s: &str) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(vec![(u("message"), raw(s))]),
        prototype: None,
        tag: None,
        error: true,
    })
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
fn addition(a: &Raw, b: &Raw) -> Raw {
    if matches!(a.as_ref(), Node::Text(_)) || matches!(b.as_ref(), Node::Text(_)) {
        text(js(a).into_iter().chain(js(b)).collect())
    } else {
        number(numeric(a) + numeric(b))
    }
}
fn block<F: std::future::Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture dependency must settle"),
    }
}
struct Harness {
    method: String,
    profile: String,
    failure: String,
    owner: Raw,
    store: Raw,
    global: Raw,
    orders: Raw,
    positions: Raw,
    raw: Raw,
    markets: Raw,
    primary: Raw,
    calls: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Value>>,
    detached: RefCell<Vec<Raw>>,
}
impl Harness {
    fn new(method: &str, profile: &str, failure: &str) -> Self {
        let store = tagged(
            "store",
            vec![
                (
                    "loaded",
                    object(vec![
                        ("markets", boolean(true)),
                        ("orders", boolean(false)),
                        ("custom", raw("keep")),
                    ]),
                ),
                ("orders", raw("old-orders")),
                ("positions", raw("old-positions")),
                ("balance", raw("old-balance")),
            ],
        );
        let owner = tagged(
            "this",
            vec![
                ("snapshotVersion", number(0.0)),
                ("ordersSnapshotVersion", number(0.0)),
                ("isDisposed", boolean(profile == "disposed")),
                (
                    "privateWebsocket",
                    object(vec![
                        ("stateRevision", number(7.0)),
                        ("started", boolean(profile != "not-started")),
                        ("ready", boolean(profile != "reconnecting")),
                    ]),
                ),
                ("store", store.clone()),
            ],
        );
        let orders = reference(
            "orders",
            array(if profile == "empty-orders" {
                vec![]
            } else {
                vec![object(vec![("id", raw("order"))])]
            }),
        );
        let positions = reference(
            "positions",
            array(if profile == "empty-positions" {
                vec![]
            } else {
                vec![
                    object(vec![(
                        "unrealizedPnl",
                        if profile == "string-pnl" {
                            raw("2")
                        } else {
                            number(2.0)
                        },
                    )]),
                    object(vec![("unrealizedPnl", number(3.0))]),
                ]
            }),
        );
        let urls = |live: &str, test: &str| {
            let live = if profile == "empty" {
                raw("")
            } else if profile == "object-url" {
                object(vec![("url", boolean(true))])
            } else {
                raw(live)
            };
            let result = object(vec![("livenet", live), ("testnet", raw(test))]);
            if profile == "inherited" {
                if let Node::Object { fields, .. } = result.as_ref() {
                    return Rc::new(Node::Object {
                        fields: RefCell::new(fields.borrow().clone()),
                        prototype: Some(object(vec![("inherited", raw("proto-url"))])),
                        tag: None,
                        error: false,
                    });
                }
            }
            result
        };
        let global = object(vec![
            ("HYROTRADER_SIMULATOR_URL", urls("rest-live", "rest-test")),
            (
                "HYROTRADER_SIMULATOR_WS_URL",
                object(vec![
                    (
                        "public",
                        object(vec![("linear", urls("public-live", "public-test"))]),
                    ),
                    ("private", urls("private-live", "private-test")),
                ]),
            ),
        ]);
        let market = |symbol: &str, quote: &str, id: f64| {
            object(vec![
                ("symbol", raw(symbol)),
                ("quote", raw(quote)),
                ("id", number(id)),
            ])
        };
        Self {
            method: method.into(),
            profile: profile.into(),
            failure: failure.into(),
            owner,
            store,
            global,
            orders,
            positions,
            raw: reference("raw", object(vec![("raw", boolean(true))])),
            markets: array(if profile == "empty" {
                vec![]
            } else {
                vec![
                    market("A", "USDT", 1.0),
                    market("B", "USDC", 2.0),
                    market("C", "USDT", 3.0),
                    market("A", "USDT", 4.0),
                ]
            }),
            primary: tagged("primary", vec![]),
            calls: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            detached: RefCell::new(vec![]),
        }
    }
    fn touch(&self, name: &str, args: &[Raw]) -> Result<(), Raw> {
        self.calls
            .borrow_mut()
            .push(json!({"name":name,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        if self.failure == name {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn run(&self) -> Result<Raw, Raw> {
        let include = match self.profile.as_str() {
            "omit-orders" => boolean(false),
            "null-orders" => null(),
            _ => undef(),
        };
        match self.method.as_str() {
            "reconcilePrivateState" => block(api::reconcile_private_state(
                self,
                self.owner.clone(),
                func(self.owner.clone(), "input-current", vec![]),
                include,
            )),
            "fetchAndSetPrivateSnapshot" => block(api::fetch_and_set_private_snapshot(
                self,
                self.owner.clone(),
            )),
            "fetchOrdersPositionsAndBalance" => block(api::fetch_orders_positions_and_balance(
                self,
                self.owner.clone(),
            )),
            "fetchAndSetPrivateSnapshotHelper" => block(
                api::fetch_and_set_private_snapshot_helper(self, self.owner.clone(), include),
            ),
            "fetchMarkets" => block(api::fetch_markets(self, self.owner.clone())),
            method => {
                let network = match self.profile.as_str() {
                    "missing" | "inherited" => raw(&self.profile),
                    "lone" => text(u("x").into_iter().chain([0xd800]).collect()),
                    "testnet" => raw("testnet"),
                    _ => raw("livenet"),
                };
                match method {
                    "getBybitRestBaseUrl" => api::get_bybit_rest_base_url(self, network),
                    "getBybitPublicWsUrl" => api::get_bybit_public_ws_url(
                        self,
                        raw(if self.profile == "invalid-category" {
                            "inverse"
                        } else {
                            "linear"
                        }),
                        network,
                    ),
                    "getBybitPrivateWsUrl" => api::get_bybit_private_ws_url(self, network),
                    _ => panic!("row"),
                }
            }
        }
    }
    fn state(&self) -> Value {
        json!({"snapshotVersion":atom(&get(&self.owner,"snapshotVersion")),"ordersSnapshotVersion":atom(&get(&self.owner,"ordersSnapshotVersion")),"orders":atom(&get(&self.store,"orders")),"positions":atom(&get(&self.store,"positions")),"balance":atom(&get(&self.store,"balance")),"loaded":atom(&get(&self.store,"loaded"))})
    }
    fn count(&self, name: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|v| v["name"] == name)
            .count()
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if Rc::ptr_eq(v, &self.global) && k == "Map" {
            return Ok(func(v.clone(), "Map constructor", vec![]));
        }
        let functions = [
            "fetchOrders",
            "fetchPositions",
            "fetchRawBalance",
            "formatBalance",
            "emitAndLogError",
            "reconcilePrivateState",
            "fetchMarketsHelper",
        ];
        if (Rc::ptr_eq(v, &self.owner) && functions.contains(&k))
            || (Rc::ptr_eq(v, &self.store) && k == "update")
            || matches!(k, "reduce" | "filter" | "map" | "values")
        {
            Ok(func(v.clone(), k, vec![]))
        } else {
            Ok(get(v, k))
        }
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        self.get(v, k)
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
    fn object(&self, fields: Vec<(&'static str, Raw)>) -> Raw {
        object(fields)
    }
    fn array(&self, a: Vec<Raw>) -> Raw {
        array(a)
    }
    fn global(&self) -> Raw {
        self.global.clone()
    }
    fn select(&self, v: Raw, k: &str) -> Result<Raw, Raw> {
        self.get(&v, k)
    }
    fn receiver_call(&self, _receiver: Raw, callable: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        if let Node::Function { code, .. } = callable.as_ref() {
            if code == "input-current" {
                assert!(eq(&_receiver, &undef()), "snapshot_native_unbound_current");
            }
        }
        self.apply(callable, args)
    }
    fn apply(&self, f: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function {
            owner,
            code,
            captures,
        } = f.as_ref()
        else {
            panic!("callable")
        };
        assert!(captures.is_empty(), "native callback captures");
        match code.as_str() {
            "input-current" => {
                self.touch("isCurrent", &[])?;
                Ok(boolean(self.profile != "stale-current"))
            }
            "snapshot-current" | "snapshot-pnl" | "market-usdt" | "market-entry" => api::dispatch(
                self,
                api::Callback::from_name(code).expect("source callback selector"),
                args,
            ),
            "reduce" => {
                let mut sum = args[1].clone();
                for value in elements(owner) {
                    sum = self.apply(args[0].clone(), vec![sum, value])?;
                }
                Ok(sum)
            }
            "filter" | "map" => {
                let mut out = vec![];
                for v in elements(owner) {
                    let result = self.apply(args[0].clone(), vec![v.clone()])?;
                    if code == "map" {
                        out.push(result)
                    } else if truthy(&result) {
                        out.push(v)
                    }
                }
                Ok(array(out))
            }
            "values" => Ok(owner.clone()),
            name => {
                self.touch(name, &args)?;
                match name {
                    "fetchOrders" | "fetchPositions" | "fetchRawBalance" | "fetchMarketsHelper" => {
                        if self.profile == "sync-throw" && name == "fetchOrders" {
                            return Err(self.primary.clone());
                        }
                        let result =
                            if matches!(self.profile.as_str(), "reject" | "primitive-reject")
                                && matches!(name, "fetchOrders" | "fetchMarketsHelper")
                            {
                                Err(if self.profile == "primitive-reject" {
                                    raw("primitive")
                                } else {
                                    self.primary.clone()
                                })
                            } else {
                                Ok(match name {
                                    "fetchOrders" => self.orders.clone(),
                                    "fetchPositions" => self.positions.clone(),
                                    "fetchRawBalance" => {
                                        if self.profile == "stale-version" {
                                            set(&self.owner, "snapshotVersion", number(2.0));
                                        }
                                        if self.profile == "stale-revision" {
                                            set(
                                                &get(&self.owner, "privateWebsocket"),
                                                "stateRevision",
                                                number(8.0),
                                            );
                                        }
                                        self.raw.clone()
                                    }
                                    _ => self.markets.clone(),
                                })
                            };
                        if let Err(error) = &result {
                            self.detached.borrow_mut().push(error.clone());
                        }
                        Ok(Rc::new(Node::Settled(result)))
                    }
                    "reconcilePrivateState" => {
                        Ok(Rc::new(Node::Settled(if self.profile == "reject" {
                            Err(self.primary.clone())
                        } else {
                            Ok(boolean(false))
                        })))
                    }
                    "formatBalance" => {
                        assert!(Rc::ptr_eq(&args[0], &self.raw), "snapshot_native_raw_alias");
                        Ok(object(vec![
                            ("total", number(100.0)),
                            ("upnl", number(9.0)),
                            ("free", number(60.0)),
                        ]))
                    }
                    "update" => {
                        if self.profile == "update-throw" {
                            return Err(self.primary.clone());
                        }
                        let Node::Object { fields, .. } = args[0].as_ref() else {
                            panic!("payload")
                        };
                        for (k, v) in fields.borrow().iter() {
                            set(&self.store, &String::from_utf16(k).unwrap(), v.clone());
                        }
                        Ok(undef())
                    }
                    "emitAndLogError" => {
                        self.logs.borrow_mut().push(atom(&args[0]));
                        if self.profile == "log-throw" {
                            Err(self.primary.clone())
                        } else {
                            Ok(undef())
                        }
                    }
                    _ => panic!("original dependency {name}"),
                }
            }
        }
    }
    fn wait(&self, v: Raw) -> SessionFuture<'_, Raw, Raw> {
        self.detached.borrow_mut().clear();
        Box::pin(async move {
            if let Node::Settled(result) = v.as_ref() {
                result.clone()
            } else {
                Ok(v)
            }
        })
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        match name {
            "Promise.resolve" => Ok(Rc::new(Node::Settled(Ok(args[0].clone())))),
            "destructure three" => Ok(array(elements(&args[0]).into_iter().take(3).collect())),
            "addition" => Ok(addition(&args[0], &args[1])),
            "computed property" => Ok(get_units(&args[0], &js(&args[1]))),
            "iterable spread" => Ok(array(elements(&args[0]))),
            "construct" => {
                assert!(
                    matches!(args[0].as_ref(), Node::Function{ code, .. } if code == "Map constructor"),
                    "markets_native_constructor"
                );
                let mut pairs: Vec<(Raw, Raw)> = vec![];
                for entry in elements(&args[1]) {
                    let e = elements(&entry);
                    if let Some((_, v)) = pairs.iter_mut().find(|(k, _)| eq(k, &e[0])) {
                        *v = e[1].clone()
                    } else {
                        pairs.push((e[0].clone(), e[1].clone()))
                    }
                }
                Ok(array(pairs.into_iter().map(|(_, v)| v).collect()))
            }
            _ => panic!("runtime operation {name}"),
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
    fn error(&self, v: Raw) -> Result<Raw, Raw> {
        Ok(error(&String::from_utf16(&js(&v)).unwrap()))
    }
    fn error_value(&self, e: Raw) -> Raw {
        e
    }
    fn normalize_error(&self, e: Raw) -> Result<Raw, Raw> {
        if matches!(e.as_ref(), Node::Text(_)) {
            Ok(error(&String::from_utf16(&js(&e)).unwrap()))
        } else {
            Ok(e)
        }
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
    fn all(&self, values: Vec<Raw>) -> Result<Raw, Raw> {
        self.detached.borrow_mut().clear();
        let results: Result<Vec<_>, _> = values
            .into_iter()
            .map(|v| {
                if let Node::Settled(result) = v.as_ref() {
                    result.clone()
                } else {
                    Ok(v)
                }
            })
            .collect();
        Ok(Rc::new(Node::Settled(results.map(array))))
    }
}
fn native(h: &Harness, result: &Result<Raw, Raw>) {
    if h.method == "reconcilePrivateState" {
        assert_eq!(
            numeric(&get(&h.owner, "snapshotVersion")),
            if h.profile == "stale-version"
                && !matches!(
                    h.failure.as_str(),
                    "fetchOrders" | "fetchPositions" | "fetchRawBalance"
                )
            {
                2.0
            } else {
                1.0
            },
            "snapshot_native_version"
        );
    }
    let detached = h.method == "reconcilePrivateState"
        && matches!(h.profile.as_str(), "reject" | "primitive-reject")
        && matches!(h.failure.as_str(), "fetchPositions" | "fetchRawBalance");
    assert_eq!(
        h.detached.borrow().len(),
        usize::from(detached),
        "snapshot_native_detached_floor"
    );
    if detached {
        let errors = h.detached.borrow();
        assert!(
            Rc::ptr_eq(&errors[0], &h.primary) || eq(&errors[0], &raw("primitive")),
            "snapshot_native_detached_identity"
        );
    }
    if h.failure != "none" {
        if h.count(&h.failure) > 0 {
            assert!(result.is_err(), "fault_native_propagation");
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
                "fault_native_original_identity"
            );
            if h.method == "reconcilePrivateState" {
                assert!(
                    eq(&get(&h.store, "positions"), &raw("old-positions")),
                    "fault_native_publication_suppressed"
                );
            }
        }
        return;
    }
    match h.method.as_str() {
        "reconcilePrivateState" => {
            let reject = matches!(
                h.profile.as_str(),
                "reject" | "primitive-reject" | "sync-throw" | "update-throw"
            );
            let stale = matches!(
                h.profile.as_str(),
                "disposed" | "stale-current" | "stale-version" | "stale-revision"
            );
            assert_eq!(result.is_err(), reject, "snapshot_native_failure");
            if reject {
                let e = result.as_ref().unwrap_err();
                assert!(
                    Rc::ptr_eq(e, &h.primary) || eq(e, &raw("primitive")),
                    "snapshot_native_error_identity"
                );
                assert_eq!(h.logs.borrow().len(), 1, "snapshot_native_error_log");
            } else {
                assert!(
                    eq(result.as_ref().unwrap(), &boolean(!stale)),
                    "snapshot_native_publication_result"
                );
                assert_eq!(
                    h.count("update"),
                    usize::from(!stale),
                    "snapshot_native_update_count"
                );
                if !stale {
                    let omitted = matches!(h.profile.as_str(), "omit-orders" | "null-orders");
                    assert_eq!(
                        numeric(&get(&h.owner, "ordersSnapshotVersion")),
                        if omitted { 0.0 } else { 1.0 },
                        "snapshot_native_orders_version"
                    );
                    assert!(
                        Rc::ptr_eq(&get(&h.store, "positions"), &h.positions),
                        "snapshot_native_positions_alias"
                    );
                    if !omitted {
                        assert!(
                            Rc::ptr_eq(&get(&h.store, "orders"), &h.orders),
                            "snapshot_native_orders_alias"
                        );
                    }
                    let balance = get(&h.store, "balance");
                    let pnl = if h.profile == "string-pnl" {
                        23.0
                    } else if h.profile == "empty-positions" {
                        0.0
                    } else {
                        5.0
                    };
                    assert_eq!(
                        numeric(&get(&balance, "total")),
                        109.0 - pnl,
                        "snapshot_native_total"
                    );
                    assert!(
                        eq(
                            &get(&balance, "upnl"),
                            &if h.profile == "string-pnl" {
                                raw("023")
                            } else {
                                number(pnl)
                            }
                        ),
                        "snapshot_native_pnl"
                    );
                    assert!(
                        eq(&get(&get(&h.store, "loaded"), "custom"), &raw("keep")),
                        "snapshot_native_loaded_preserved"
                    );
                }
            }
        }
        "fetchAndSetPrivateSnapshot"
        | "fetchOrdersPositionsAndBalance"
        | "fetchAndSetPrivateSnapshotHelper" => {
            let site = match h.method.as_str() {
                "fetchAndSetPrivateSnapshot" => "snapshot_wrapper_native_call",
                "fetchOrdersPositionsAndBalance" => "account_wrapper_native_call",
                _ => "snapshot_helper_native_call",
            };
            let skip = h.profile == "reconnecting";
            assert_eq!(
                h.count("reconcilePrivateState"),
                usize::from(!skip),
                "{site}"
            );
            assert_eq!(
                result.is_err(),
                h.profile == "reject",
                "wrapper_native_error"
            );
            if result.is_ok() {
                assert!(
                    eq(result.as_ref().unwrap(), &undef()),
                    "wrapper_native_undefined"
                );
            }
            if h.method == "fetchAndSetPrivateSnapshotHelper" && !skip {
                let calls = h.calls.borrow();
                assert_eq!(
                    calls[0]["args"].as_array().unwrap().len(),
                    2,
                    "snapshot_helper_native_arguments"
                );
            }
        }
        "fetchMarkets" => {
            assert_eq!(
                result.is_err(),
                h.profile == "reject",
                "markets_native_failure"
            );
            if let Ok(v) = result {
                let rows = elements(v);
                assert_eq!(
                    rows.len(),
                    if h.profile == "empty" { 0 } else { 2 },
                    "markets_native_count"
                );
                if !rows.is_empty() {
                    assert!(
                        Rc::ptr_eq(&rows[0], &elements(&h.markets)[3]),
                        "markets_native_last_alias"
                    );
                    assert!(
                        eq(&get(&rows[1], "symbol"), &raw("C")),
                        "markets_native_insertion_order"
                    );
                }
            }
        }
        method => {
            let absent = matches!(h.profile.as_str(), "missing" | "empty" | "lone")
                || (method == "getBybitPublicWsUrl" && h.profile == "invalid-category");
            let site = match method {
                "getBybitRestBaseUrl" => "rest_url_native_value",
                "getBybitPublicWsUrl" => "public_url_native_value",
                _ => "private_url_native_value",
            };
            assert_eq!(result.is_err(), absent, "{site}");
            if let Err(e) = result {
                let message = if method == "getBybitPublicWsUrl" && h.profile == "invalid-category"
                {
                    "HyroTrader simulator only supports linear public WebSocket data"
                } else {
                    match method {
                        "getBybitRestBaseUrl" => "HyroTrader simulator REST URL is not configured",
                        "getBybitPublicWsUrl" => {
                            "HyroTrader simulator public WebSocket URL is not configured"
                        }
                        _ => "HyroTrader simulator private WebSocket URL is not configured",
                    }
                };
                assert!(eq(&get(e, "message"), &raw(message)), "{site}");
            }
            if let Ok(v) = result {
                if h.profile == "object-url" {
                    assert!(truthy(&get(v, "url")), "{site}");
                    let table = match method {
                        "getBybitRestBaseUrl" => get(&h.global, "HYROTRADER_SIMULATOR_URL"),
                        "getBybitPublicWsUrl" => get(
                            &get(&get(&h.global, "HYROTRADER_SIMULATOR_WS_URL"), "public"),
                            "linear",
                        ),
                        _ => get(&get(&h.global, "HYROTRADER_SIMULATOR_WS_URL"), "private"),
                    };
                    assert!(
                        Rc::ptr_eq(v, &get(&table, "livenet")),
                        "url_native_original_alias"
                    );
                } else {
                    let wanted = if h.profile == "inherited" {
                        "proto-url".into()
                    } else {
                        format!(
                            "{}-{}",
                            match method {
                                "getBybitRestBaseUrl" => "rest",
                                "getBybitPublicWsUrl" => "public",
                                _ => "private",
                            },
                            if h.profile == "testnet" {
                                "test"
                            } else {
                                "live"
                            }
                        )
                    };
                    assert!(eq(v, &raw(&wanted)), "{site}");
                }
            }
        }
    }
}
#[test]
fn source_and_native_snapshot_facade() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/hyro-snapshot-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 371, "declared case floor");
    let mut unique = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut supplemental = vec![];
    for case in cases {
        let m = case["method"].as_str().unwrap();
        let p = case["profile"].as_str().unwrap();
        let f = case["failure"].as_str().unwrap();
        assert!(unique.insert((m.to_string(), p.to_string(), f.to_string())));
        *counts.entry(m.to_string()).or_insert(0_usize) += 1;
        let h = Harness::new(m, p, f);
        let result = h.run();
        native(&h, &result);
        let result = match result {
            Ok(v) => json!({"ok":true,"value":atom(&v)}),
            Err(e) => json!({"ok":false,"error":atom(&e),"primary":Rc::ptr_eq(&e,&h.primary)}),
        };
        let receipt = json!({"calls":*h.calls.borrow(),"logs":*h.logs.borrow(),"detached":h.detached.borrow().iter().map(atom).collect::<Vec<_>>(),"result":result,"state":h.state()});
        supplemental.push((case, receipt));
    }
    assert_eq!(
        counts,
        std::collections::BTreeMap::from([
            ("reconcilePrivateState".into(), 105),
            ("fetchAndSetPrivateSnapshot".into(), 14),
            ("fetchOrdersPositionsAndBalance".into(), 14),
            ("fetchAndSetPrivateSnapshotHelper".into(), 42),
            ("fetchMarkets".into(), 28),
            ("getBybitRestBaseUrl".into(), 56),
            ("getBybitPublicWsUrl".into(), 56),
            ("getBybitPrivateWsUrl".into(), 56)
        ]),
        "independent per-method floor"
    );
    assert!(
        unique.contains(&(
            "getBybitPrivateWsUrl".into(),
            "lone".into(),
            "emitAndLogError".into()
        )),
        "last declared iteration"
    );
    for (case, receipt) in supplemental {
        for key in ["calls", "logs", "detached", "result", "state"] {
            assert_eq!(
                receipt[key], case[key],
                "supplemental {}/{}/{} {key}",
                case["method"], case["profile"], case["failure"]
            );
        }
    }
}
