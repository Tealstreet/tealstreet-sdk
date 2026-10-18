use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost as Host},
    inverse_read_surface as api,
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
        prototype: RefCell<Option<Raw>>,
        tag: Option<String>,
        error: bool,
    },
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
        prototype: RefCell::new(None),
        tag: None,
        error: false,
    })
}
fn tagged(tag: &str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: RefCell::new(None),
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
                    .borrow()
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
        Node::Function { code, .. } => json!({"fn":code}),
        Node::Iterator { .. } => panic!("internal iterator"),
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
        prototype: RefCell::new(None),
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
fn is_array(v: &Raw) -> bool {
    match v.as_ref() {
        Node::Array(_) => true,
        Node::Reference { value, .. } => is_array(value),
        _ => false,
    }
}
fn block<F: std::future::Future>(future: F) -> F::Output {
    let mut c = Context::from_waker(Waker::noop());
    let mut f = Box::pin(future);
    for _ in 0..1024 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut c) {
            return v;
        }
    }
    panic!("fixture completion bound")
}
struct Harness {
    method: String,
    profile: String,
    failure: String,
    owner: Raw,
    xhr: Raw,
    log: Raw,
    first: Raw,
    balance_list: Raw,
    old_balance: Raw,
    old_info: Raw,
    old_markets: Raw,
    cached: Raw,
    returned: Raw,
    rows: [Raw; 2],
    mapped: [Raw; 2],
    markets: [Raw; 2],
    primary: Raw,
    calls: RefCell<Vec<Value>>,
    logs: RefCell<Vec<Value>>,
    queries: RefCell<usize>,
    globals: Raw,
    promise: Raw,
    boundary_trace: RefCell<Vec<String>>,
    then_seen: RefCell<bool>,
    promise_replaced: RefCell<bool>,
}
impl Harness {
    fn new(m: &str, p: &str, f: &str) -> Self {
        let owner = tagged("this", vec![]);
        let xhr = tagged("xhr", vec![]);
        let log = func(owner.clone(), "log", vec![]);
        let cached = if p == "undefined" {
            undef()
        } else if p == "null" {
            null()
        } else if p == "lone" {
            text(u("key-").into_iter().chain([0xd800]).collect())
        } else {
            tagged("cached", vec![("key", raw("cached"))])
        };
        let old_info = if p == "no-info" {
            undef()
        } else {
            reference(
                "old-info",
                array(if p == "empty-info" {
                    vec![]
                } else {
                    vec![object(vec![("old", boolean(true))])]
                }),
            )
        };
        let old_balance = tagged(
            "old-balance",
            vec![("total", number(7.0)), ("info", old_info.clone())],
        );
        let old_markets = reference(
            "old-markets",
            array(vec![object(vec![("symbol", raw("old"))])]),
        );
        let positions = array(vec![
            object(vec![(
                "unrealizedPnl",
                if p == "string-pnl" {
                    raw("2")
                } else {
                    number(2.0)
                },
            )]),
            object(vec![("unrealizedPnl", number(3.0))]),
        ]);
        let coin = |key: Raw, equity: &str, usd: &str| {
            object(vec![
                ("coin", key),
                ("equity", raw(equity)),
                ("usdValue", raw(usd)),
            ])
        };
        let mut coins = if p == "empty-coin" {
            vec![]
        } else {
            vec![
                coin(
                    if p == "lone-coin" {
                        text(u("C").into_iter().chain([0xd800]).collect())
                    } else {
                        raw(match p {
                            "proto-coin" => "__proto__",
                            "constructor-coin" => "constructor",
                            _ => "BTC",
                        })
                    },
                    "1.5",
                    "100",
                ),
                coin(raw("ETH"), "2", "80"),
            ]
        };
        if p == "duplicates" {
            coins.push(coin(raw("BTC"), "3", "90"));
        }
        let first = tagged(
            "first-account",
            vec![
                (
                    "totalEquity",
                    if p == "missing-equity" {
                        undef()
                    } else {
                        raw("100.25")
                    },
                ),
                ("totalMaintenanceMargin", raw("1.25")),
                ("totalInitialMargin", raw("2.5")),
                (
                    "totalAvailableBalance",
                    match p {
                        "free-null" => null(),
                        "free-zero" => number(0.0),
                        "free-false" => boolean(false),
                        _ => raw("70"),
                    },
                ),
                ("totalMarginBalance", raw("60")),
                ("coin", array(coins)),
            ],
        );
        let balance_list = reference("balance-list", array(vec![first.clone()]));
        for (k, v) in [
            ("xhr", xhr.clone()),
            ("log", log.clone()),
            ("apiKeyInfo", cached.clone()),
            (
                "store",
                object(vec![
                    ("balance", old_balance.clone()),
                    ("markets", old_markets.clone()),
                    ("positions", positions),
                ]),
            ),
            ("forceUnifiedBalance", boolean(p == "force")),
            (
                "unifiedMarginStatus",
                number(if p == "classic" { 1.0 } else { 6.0 }),
            ),
            ("needsReinit", boolean(false)),
            (
                "supportedCategories",
                array(if p == "empty" {
                    vec![]
                } else if p == "multiple" {
                    vec![raw("inverse"), raw("linear")]
                } else {
                    vec![raw("inverse")]
                }),
            ),
        ] {
            set(&owner, k, v);
        }
        let promise = object(vec![]);
        let globals = object(vec![("Promise", promise.clone())]);
        Self {
            globals,
            promise,
            boundary_trace: RefCell::new(vec![]),
            then_seen: RefCell::new(false),
            promise_replaced: RefCell::new(false),
            method: m.into(),
            profile: p.into(),
            failure: f.into(),
            owner,
            xhr,
            log,
            first,
            balance_list,
            old_balance,
            old_info,
            old_markets,
            cached,
            returned: reference(
                "returned",
                array(vec![object(vec![("income", boolean(true))])]),
            ),
            rows: [
                tagged("row-1", vec![("id", raw("one"))]),
                tagged("row-2", vec![("id", raw("two"))]),
            ],
            mapped: [
                tagged("mapped-1", vec![("id", raw("mapped-one"))]),
                tagged("mapped-2", vec![("id", raw("mapped-two"))]),
            ],
            markets: [
                tagged("market-1", vec![("symbol", raw("BTCUSD"))]),
                tagged("market-2", vec![("symbol", raw("ETHUSD"))]),
            ],
            primary: tagged("primary", vec![]),
            calls: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            queries: RefCell::new(0),
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
    fn count(&self, name: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|c| c["name"] == name)
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
        let result = match self.method.as_str() {
            "cachedApiKeyInfo" => api::cached_api_key_info(self, &self.owner),
            "incomeContext" => api::income_context(self, &self.owner),
            "formatBalance" => api::format_balance(
                self,
                self.owner.clone(),
                match self.profile.as_str() {
                    "empty" => array(vec![]),
                    "null" => null(),
                    "nonarray" => object(vec![]),
                    _ => self.balance_list.clone(),
                },
            ),
            "fetchRawBalance" => return eager!(api::fetch_raw_balance(self, self.owner.clone())),
            "fetchOrdersHelper" => {
                return eager!(api::fetch_orders_helper(
                    self,
                    self.owner.clone(),
                    raw("inverse"),
                    if self.profile == "settle-null" {
                        null()
                    } else {
                        undef()
                    }
                ));
            }
            "fetchMarkets" => return eager!(api::fetch_markets(self, self.owner.clone())),
            "fetchIncomeHistory" => {
                return eager!(api::fetch_income_history(
                    self,
                    self.owner.clone(),
                    match self.profile.as_str() {
                        "undefined" => undef(),
                        "null" => null(),
                        "lone" => text(u("opts-").into_iter().chain([0xd800]).collect()),
                        _ => object(vec![("accountName", raw("account"))]),
                    }
                ));
            }
            "fetchIncomeHistoryRange" => {
                return eager!(api::fetch_income_history_range(
                    self,
                    self.owner.clone(),
                    number(1.0),
                    number(2.0),
                    match self.profile.as_str() {
                        "undefined" => undef(),
                        "null" => null(),
                        "lone" => text(u("kind-").into_iter().chain([0xd800]).collect()),
                        _ => raw("funding"),
                    }
                ));
            }
            _ => panic!("row"),
        };
        (result, self.calls.borrow().len())
    }
    fn state(&self) -> Value {
        json!({"force":truthy(&get(&self.owner,"forceUnifiedBalance")),"margin":atom(&get(&self.owner,"unifiedMarginStatus")),"needsReinit":truthy(&get(&self.owner,"needsReinit")),"queries":*self.queries.borrow()})
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if let Node::Reference { tag, .. } = v.as_ref()
            && tag == "length-string-zero"
            && k == "length"
        {
            self.boundary_trace.borrow_mut().push("length:get".into());
            return Ok(raw("0"));
        }
        if Rc::ptr_eq(v, &self.promise) && k == "all" {
            if matches!(
                self.profile.as_str(),
                "markets-all-capture-before-map" | "markets-noncallable-all-map-throws"
            ) {
                self.boundary_trace.borrow_mut().push("all:get".into());
            }
            return Ok(if self.profile == "markets-noncallable-all-map-throws" {
                number(17.0)
            } else {
                func(
                    v.clone(),
                    if *self.promise_replaced.borrow() {
                        "replacement-all"
                    } else {
                        "Promise.all"
                    },
                    vec![],
                )
            });
        }
        if self.profile == "markets-noncallable-all-map-throws" {
            if Rc::ptr_eq(v, &self.owner) && k == "supportedCategories" {
                self.boundary_trace
                    .borrow_mut()
                    .push("categories:get".into());
            }
            if Rc::ptr_eq(v, &get(&self.owner, "supportedCategories")) && k == "map" {
                self.boundary_trace.borrow_mut().push("map:get".into());
            }
        }
        if self.profile == "markets-all-capture-before-map" {
            if Rc::ptr_eq(v, &get(&self.owner, "supportedCategories")) && k == "map" {
                self.boundary_trace.borrow_mut().push("map:get".into());
            }
            if Rc::ptr_eq(v, &self.owner) && k == "supportedCategories" {
                self.boundary_trace
                    .borrow_mut()
                    .push("categories:get".into());
            }
            if Rc::ptr_eq(v, &self.owner) && k == "fetchMarketsHelper" {
                self.boundary_trace.borrow_mut().push("helper:get".into());
            }
            if matches!(v.as_ref(), Node::Function { code, .. } if code == "fetchMarketsHelper")
                && k == "bind"
            {
                self.boundary_trace.borrow_mut().push("bind:get".into());
            }
        }
        if Rc::ptr_eq(v, &self.owner) {
            if ["xhr", "log", "apiKeyInfo"].contains(&k) {
                self.touch(&format!("get:{k}"), &[])?;
            }
            if k == "accountType" {
                return Ok(raw(if eq(&get(v, "unifiedMarginStatus"), &number(1.0)) {
                    "CONTRACT"
                } else {
                    "UNIFIED"
                }));
            }
            if k == "incomeContext" {
                return api::income_context(self, v);
            }
        }
        if Rc::ptr_eq(v, &self.owner)
            && k == "apiKeyInfo"
            && self.profile == "changed"
            && self.count("get:apiKeyInfo") > 1
        {
            return Ok(tagged("alternate", vec![("key", raw("alternate"))]));
        }
        let real = get(v, k);
        if !eq(&real, &undef()) {
            return Ok(real);
        }
        let functions = [
            "v",
            "parseFloat",
            "subtract",
            "fetchBybitIncomeHistory",
            "fetchBybitIncomeHistoryRange",
            "get",
            "emitAndLogError",
            "fetchRawBalance",
            "reinitOnThrow",
            "mapOrder",
            "fetchMarketsHelper",
            "bind",
            "map",
            "some",
            "flat",
            "flatMap",
            "reduce",
            "next",
        ];
        if functions.contains(&k) {
            Ok(func(
                v.clone(),
                if Rc::ptr_eq(v, &self.xhr) && k == "get" {
                    "xhr:get"
                } else {
                    k
                },
                vec![],
            ))
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
        self.globals.clone()
    }
    fn select(&self, v: Raw, k: &str) -> Result<Raw, Raw> {
        self.get(&v, k)
    }
    fn receiver_call(&self, receiver: Raw, f: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function { code, captures, .. } = f.as_ref() else {
            panic!("callable")
        };
        if code == "Promise.all" {
            assert!(
                Rc::ptr_eq(&receiver, &self.promise),
                "markets_native_explicit_all_receiver"
            );
        }
        self.apply(func(receiver, code, captures.clone()), args)
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
        match code.as_str() {
            "replacement-all" => Err(self.primary.clone()),
            "Promise.all" => {
                assert!(
                    Rc::ptr_eq(owner, &self.promise),
                    "markets_native_all_receiver"
                );
                if self.profile == "markets-all-capture-before-map" {
                    self.boundary_trace
                        .borrow_mut()
                        .push("all:call:original-receiver".into());
                }
                let iterator = self.intrinsic("iterator", vec![args[0].clone()])?;
                let next = self.select(iterator, "next")?;
                let mut values = vec![];
                loop {
                    let step = self.apply(next.clone(), vec![])?;
                    if truthy(&get(&step, "done")) {
                        break;
                    }
                    let value = get(&step, "value");
                    if self.profile == "markets-then-getter-interleave" {
                        self.boundary_trace.borrow_mut().push("then:get".into());
                        *self.then_seen.borrow_mut() = true;
                    }
                    values.push(value);
                }
                if self.profile == "markets-then-getter-interleave" {
                    self.boundary_trace.borrow_mut().push("then:call".into());
                }
                self.all(values)
            }
            "inverse-pnl" | "inverse-coin" | "inverse-null-market" => api::dispatch(
                self,
                api::Callback::from_name(code).expect("source callback selector"),
                args,
            ),
            "v" => {
                let key = String::from_utf16(&js(&args[1])).unwrap();
                let real = get(&args[0], &key);
                fn has(v: &Raw, key: &[u16]) -> bool {
                    match v.as_ref() {
                        Node::Object {
                            fields, prototype, ..
                        } => {
                            fields.borrow().iter().any(|(k, _)| k == key)
                                || prototype.borrow().as_ref().is_some_and(|p| has(p, key))
                        }
                        _ => false,
                    }
                }
                if has(&args[0], &u(&key)) {
                    return Ok(real);
                }
                let snake = key
                    .chars()
                    .flat_map(|c| {
                        if c.is_ascii_uppercase() {
                            vec!['_', c.to_ascii_lowercase()]
                        } else {
                            vec![c]
                        }
                    })
                    .collect::<String>();
                Ok(get(&args[0], &snake))
            }
            "parseFloat" => {
                self.touch("parseFloat", &args)?;
                let units = js(&args[0]);
                let well = String::from_utf16_lossy(&units);
                Ok(number(safe_cex_num::parse_float(&well)))
            }
            "subtract" => {
                self.touch("subtract", &args)?;
                Ok(number(safe_cex_num::subtract(
                    numeric(&args[0]),
                    numeric(&args[1]),
                )))
            }
            "log" => {
                self.touch("log", &args)?;
                self.logs
                    .borrow_mut()
                    .push(json!(args.iter().map(atom).collect::<Vec<_>>()));
                Ok(undef())
            }
            "emitAndLogError" => {
                self.touch("emit", &args)?;
                self.logs
                    .borrow_mut()
                    .push(json!(args.iter().map(atom).collect::<Vec<_>>()));
                Ok(undef())
            }
            "fetchBybitIncomeHistory" | "fetchBybitIncomeHistoryRange" => {
                self.touch("income", &args)?;
                if self.profile == "throw" {
                    return Err(self.primary.clone());
                }
                Ok(Rc::new(Node::Settled(if self.profile == "reject" {
                    Err(self.primary.clone())
                } else {
                    Ok(self.returned.clone())
                })))
            }
            "fetchRawBalance" => Ok(Rc::new(Node::Settled(block(api::fetch_raw_balance(
                self,
                self.owner.clone(),
            ))))),
            "xhr:get" => {
                if self.profile == "orders-length-string-zero" {
                    self.boundary_trace.borrow_mut().push("xhr:get".into());
                }
                let touched = self.touch("xhr:get", &args);
                let result = touched.and_then(|()| {
                    *self.queries.borrow_mut() += 1;
                    let query = *self.queries.borrow();
                    if self.profile == "reject" {
                        return Err(self.primary.clone());
                    }
                    if eq(&args[0], &raw("balance")) {
                        let message = if self.profile == "retry" && query == 1
                            || self.profile == "repeat-error"
                        {
                            "accountType only support UNIFIED."
                        } else if matches!(
                            self.profile.as_str(),
                            "ret-error" | "missing-code" | "code-zero" | "no-info" | "empty-info"
                        ) {
                            "denied"
                        } else {
                            "OK"
                        };
                        let code = if self.profile == "missing-code" {
                            undef()
                        } else if self.profile == "code-zero" {
                            raw("0")
                        } else {
                            number(if message == "OK" { 0.0 } else { 1.0 })
                        };
                        let (msg_key, code_key) = if self.profile == "snake" {
                            ("ret_msg", "ret_code")
                        } else {
                            ("retMsg", "retCode")
                        };
                        Ok(object(vec![(
                            "data",
                            object(vec![
                                (msg_key, raw(message)),
                                (code_key, code),
                                ("result", object(vec![("list", self.balance_list.clone())])),
                            ]),
                        )]))
                    } else {
                        let empty = matches!(self.profile.as_str(), "empty" | "malformed")
                            || self.profile == "repeat-cursor" && query == 3;
                        let cursor = if self.profile == "pages" && query == 1
                            || self.profile == "second-error" && query == 1
                        {
                            raw("next")
                        } else if self.profile == "repeat-cursor" && query < 3 {
                            raw("same")
                        } else if self.profile == "numeric-cursor" && query == 1 {
                            number(7.0)
                        } else {
                            undef()
                        };
                        Ok(object(vec![(
                            "data",
                            object(vec![
                                (
                                    "retMsg",
                                    raw(
                                        if self.profile == "ret-error"
                                            || self.profile == "second-error" && query == 2
                                        {
                                            "denied"
                                        } else {
                                            "OK"
                                        },
                                    ),
                                ),
                                (
                                    "result",
                                    object(vec![
                                        (
                                            "list",
                                            if self.profile == "malformed" {
                                                null()
                                            } else {
                                                let rows = array(if empty {
                                                    vec![]
                                                } else {
                                                    vec![self.rows[usize::from(query != 1)].clone()]
                                                });
                                                if self.profile == "orders-length-string-zero" {
                                                    reference("length-string-zero", rows)
                                                } else {
                                                    rows
                                                }
                                            },
                                        ),
                                        ("nextPageCursor", cursor),
                                    ]),
                                ),
                            ]),
                        )]))
                    }
                });
                Ok(Rc::new(Node::Settled(result)))
            }
            "reinitOnThrow" => {
                if self.profile == "orders-length-string-zero" {
                    self.boundary_trace.borrow_mut().push("reinit:call".into());
                }
                self.touch("reinit", &[])?;
                Ok(args[0].clone())
            }
            "mapOrder" => {
                if self.profile == "orders-length-string-zero" {
                    self.boundary_trace.borrow_mut().push("map:row-one".into());
                }
                self.touch("mapOrder", &args[..1])?;
                Ok(array(vec![
                    self.mapped[usize::from(!Rc::ptr_eq(&args[0], &self.rows[0]))].clone(),
                ]))
            }
            "fetchMarketsHelper" => {
                self.touch("fetchMarketsHelper", &args[..1])?;
                Ok(Rc::new(Node::Settled(if self.profile == "reject" {
                    Err(self.primary.clone())
                } else {
                    Ok(if self.profile == "null" {
                        array(vec![null()])
                    } else if self.profile == "undefined" {
                        undef()
                    } else {
                        array(vec![
                            self.markets[usize::from(!eq(&args[0], &raw("inverse")))].clone(),
                        ])
                    })
                })))
            }
            "bind" => Ok(func(owner.clone(), "bound", vec![args[0].clone()])),
            "bound" => {
                assert!(
                    Rc::ptr_eq(&captures[0], &self.owner),
                    "reader_native_bound_receiver"
                );
                self.apply(owner.clone(), args)
            }
            "next" => {
                let Node::Iterator { value, index } = owner.as_ref() else {
                    panic!("iterator")
                };
                let i = *index.borrow();
                if self.profile == "markets-then-getter-interleave" {
                    self.boundary_trace.borrow_mut().push(format!("next:{i}"));
                    if i != 0 {
                        assert!(*self.then_seen.borrow(), "markets_native_then_before_next");
                    }
                }
                let rows = elements(value);
                if i < rows.len() {
                    *index.borrow_mut() += 1;
                    Ok(object(vec![
                        ("done", boolean(false)),
                        ("value", rows[i].clone()),
                    ]))
                } else {
                    Ok(object(vec![("done", boolean(true))]))
                }
            }
            "reduce" => {
                let mut accumulator = args[1].clone();
                for value in elements(owner) {
                    accumulator = self.apply(args[0].clone(), vec![accumulator, value])?;
                }
                Ok(accumulator)
            }
            "map" | "flatMap" => {
                if code == "map" && self.profile.starts_with("markets-") {
                    self.boundary_trace.borrow_mut().push("map:call".into());
                    if self.profile == "markets-noncallable-all-map-throws" {
                        return Err(self.primary.clone());
                    }
                    if self.profile == "markets-all-capture-before-map" {
                        *self.promise_replaced.borrow_mut() = true;
                        set(
                            &self.globals,
                            "Promise",
                            object(vec![(
                                "all",
                                func(self.promise.clone(), "replacement-all", vec![]),
                            )]),
                        );
                    }
                    return Ok(array(vec![Rc::new(Node::Settled(Ok(array(vec![
                        self.markets[0].clone(),
                    ]))))]));
                }
                let mut out = vec![];
                for value in elements(owner) {
                    let result = self.apply(args[0].clone(), vec![value])?;
                    if code == "flatMap" && is_array(&result) {
                        out.extend(elements(&result))
                    } else {
                        out.push(result)
                    }
                }
                Ok(array(out))
            }
            "flat" => {
                let mut out = vec![];
                for value in elements(owner) {
                    if is_array(&value) {
                        out.extend(elements(&value))
                    } else {
                        out.push(value)
                    }
                }
                Ok(array(out))
            }
            "some" => {
                for value in elements(owner) {
                    if truthy(&self.apply(args[0].clone(), vec![value])?) {
                        return Ok(boolean(true));
                    }
                }
                Ok(boolean(false))
            }
            _ => panic!("runtime function {code}"),
        }
    }
    fn wait(&self, value: Raw) -> SessionFuture<'_, Raw, Raw> {
        let mut pending = true;
        Box::pin(std::future::poll_fn(move |cx| {
            if pending {
                pending = false;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(if let Node::Settled(result) = value.as_ref() {
                result.clone()
            } else {
                Ok(value.clone())
            })
        }))
    }
    fn intrinsic(&self, name: &str, args: Vec<Raw>) -> Result<Raw, Raw> {
        match name {
            "ENDPOINTS" => Ok(object(vec![
                ("BALANCE", raw("balance")),
                ("UNFILLED_ORDERS", raw("orders")),
            ])),
            "LogSeverity" => Ok(object(vec![("Warning", number(1.0))])),
            "null" => Ok(null()),
            "Array.isArray" => Ok(boolean(is_array(&args[0]))),
            "addition" => Ok(addition(&args[0], &args[1])),
            "property key" => Ok(text(js(&args[0]))),
            "computed assignment" => {
                let Node::Object { prototype, .. } = args[0].as_ref() else {
                    panic!("assignment")
                };
                if eq(&args[1], &raw("__proto__")) {
                    *prototype.borrow_mut() = Some(args[2].clone());
                } else {
                    let key = String::from_utf16(&js(&args[1]));
                    if let Ok(key) = key {
                        set(&args[0], &key, args[2].clone());
                    } else if let Node::Object { fields, .. } = args[0].as_ref() {
                        fields.borrow_mut().push((js(&args[1]), args[2].clone()));
                    }
                }
                Ok(args[2].clone())
            }
            "destructure first" => {
                if self.profile == "balance-length-string-zero" {
                    self.boundary_trace.borrow_mut().push("iterator:get".into());
                }
                Ok(elements(&args[0]).first().cloned().unwrap_or_else(undef))
            }
            "iterator" => {
                if self.profile == "markets-then-getter-interleave"
                    || matches!(args[0].as_ref(), Node::Reference { tag, .. } if tag == "length-string-zero")
                {
                    self.boundary_trace.borrow_mut().push("iterator:get".into());
                }
                Ok(Rc::new(Node::Iterator {
                    value: args[0].clone(),
                    index: RefCell::new(0),
                }))
            }
            _ => panic!("intrinsic {name}"),
        }
    }
    fn allocate(&self, _: Raw) -> Result<Raw, Raw> {
        panic!("unused allocate")
    }
    fn callback(&self, owner: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(func(owner, code, captures))
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        panic!("unused promise")
    }
    fn error(&self, value: Raw) -> Result<Raw, Raw> {
        Ok(error_units(if eq(&value, &undef()) {
            vec![]
        } else {
            js(&value)
        }))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, error: Raw) -> Result<Raw, Raw> {
        Ok(error)
    }
}
impl AccountRefreshHost for Harness {
    fn spread(&self, _: Raw, _: Vec<(&'static str, Raw)>) -> Result<Raw, Raw> {
        panic!("unused spread")
    }
    fn then(&self, _: Raw, _: Raw) -> Result<Raw, Raw> {
        panic!("unused then")
    }
    fn all(&self, values: Vec<Raw>) -> Result<Raw, Raw> {
        let result: Result<Vec<_>, _> = values
            .into_iter()
            .map(|v| {
                if let Node::Settled(result) = v.as_ref() {
                    result.clone()
                } else {
                    Ok(v)
                }
            })
            .collect();
        Ok(Rc::new(Node::Settled(result.map(array))))
    }
}
fn native(h: &Harness, result: &Result<Raw, Raw>) {
    if h.failure != "none" {
        let reached = match h.failure.as_str() {
            "get:apiKeyInfo" => h.method == "cachedApiKeyInfo",
            "get:xhr" => matches!(
                h.method.as_str(),
                "incomeContext"
                    | "fetchIncomeHistory"
                    | "fetchIncomeHistoryRange"
                    | "fetchRawBalance"
                    | "fetchOrdersHelper"
            ),
            "get:log" => {
                matches!(
                    h.method.as_str(),
                    "incomeContext" | "fetchIncomeHistory" | "fetchIncomeHistoryRange"
                ) || h.method == "formatBalance"
                    && matches!(h.profile.as_str(), "empty" | "null" | "nonarray")
            }
            "xhr:get" => matches!(h.method.as_str(), "fetchRawBalance" | "fetchOrdersHelper"),
            "income" => matches!(
                h.method.as_str(),
                "fetchIncomeHistory" | "fetchIncomeHistoryRange"
            ),
            "parseFloat" => {
                h.method == "formatBalance"
                    && !matches!(h.profile.as_str(), "empty" | "null" | "nonarray")
            }
            "emit" => {
                h.method == "fetchRawBalance"
                    && matches!(
                        h.profile.as_str(),
                        "ret-error" | "missing-code" | "repeat-error" | "no-info" | "empty-info"
                    )
                    || h.method == "fetchOrdersHelper"
                        && matches!(h.profile.as_str(), "ret-error" | "second-error")
                    || h.method == "fetchMarkets" && h.profile == "null"
            }
            _ => panic!("undeclared fault"),
        };
        assert_eq!(
            h.count(&h.failure) > 0,
            reached,
            "reader_native_declared_fault_reach {}/{}/{}",
            h.method,
            h.profile,
            h.failure
        );
    }
    for call in h.calls.borrow().iter().filter(|c| c["name"] == "income") {
        if h.method == "fetchIncomeHistoryRange" {
            let kind = match h.profile.as_str() {
                "undefined" => undef(),
                "null" => null(),
                "lone" => text(u("kind-").into_iter().chain([0xd800]).collect()),
                _ => raw("funding"),
            };
            assert_eq!(call["args"][3], atom(&kind), "income_range_native_kind");
        }
    }
    for call in h.calls.borrow().iter().filter(|c| c["name"] == "xhr:get") {
        if h.method == "fetchRawBalance" {
            let params = &call["args"][1]["o"][0][1]["o"];
            assert_eq!(
                params[0][0],
                json!(u("accountType")),
                "wallet_native_account_key"
            );
            assert_eq!(
                params[0][1],
                atom(&raw("UNIFIED")),
                "wallet_native_account_value"
            );
        }
    }

    if h.failure != "none" {
        if h.count(&h.failure) > 0 {
            assert!(result.is_err(), "reader_native_fault");
            assert!(
                Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
                "reader_native_fault_identity"
            );
            return;
        }
    }
    match h.method.as_str() {
        "cachedApiKeyInfo" => assert!(
            eq(result.as_ref().unwrap(), &h.cached),
            "cache_native_identity"
        ),
        "incomeContext" => {
            let v = result.as_ref().unwrap();
            assert!(
                Rc::ptr_eq(&get(v, "xhr"), &h.xhr),
                "income_context_native_xhr"
            );
            assert!(
                Rc::ptr_eq(&get(v, "log"), &h.log),
                "income_context_native_log"
            );
            assert!(
                eq(&get(v, "category"), &raw("inverse")),
                "income_context_native_category"
            );
            assert!(
                eq(&get(v, "accountType"), &raw("UNIFIED")),
                "income_context_native_unified"
            );
        }
        "fetchIncomeHistory" | "fetchIncomeHistoryRange" => {
            let failure = matches!(h.profile.as_str(), "reject" | "throw");
            assert_eq!(result.is_err(), failure, "income_native_outcome");
            if !failure {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &h.returned),
                    "income_native_return_alias"
                );
            }
            let calls = h.calls.borrow();
            let args = calls.iter().find(|c| c["name"] == "income").unwrap()["args"]
                .as_array()
                .unwrap();
            let site = if h.method == "fetchIncomeHistory" {
                "income_native_arguments"
            } else {
                "income_range_native_arguments"
            };
            assert_eq!(
                args.len(),
                if h.method == "fetchIncomeHistory" {
                    2
                } else {
                    4
                },
                "{site}"
            );
            let context = &args[0]["o"];
            assert_eq!(context[0][1], json!({"ref":"xhr"}), "{site}");
            assert_eq!(context[2][1], atom(&raw("inverse")), "{site}");
            assert_eq!(
                context[3][1],
                atom(&raw("UNIFIED")),
                "income_context_native_through_delegate"
            );
            if h.method == "fetchIncomeHistory" {
                let expected = match h.profile.as_str() {
                    "undefined" => undef(),
                    "null" => null(),
                    "lone" => text(u("opts-").into_iter().chain([0xd800]).collect()),
                    _ => object(vec![("accountName", raw("account"))]),
                };
                assert_eq!(args[1], atom(&expected), "income_native_arguments");
            }
            if h.method == "fetchIncomeHistoryRange" {
                assert_eq!(args[1], atom(&number(1.0)), "{site}");
                assert_eq!(args[2], atom(&number(2.0)), "{site}");
            }
        }
        "fetchRawBalance" => {
            assert_eq!(
                result.is_err(),
                h.profile == "reject",
                "wallet_native_error"
            );
            let query = if matches!(h.profile.as_str(), "retry" | "repeat-error") {
                2
            } else {
                1
            };
            assert_eq!(*h.queries.borrow(), query, "wallet_native_request_floor");
            assert!(
                eq(&get(&h.owner, "unifiedMarginStatus"), &number(6.0)),
                "wallet_native_margin_status"
            );
            assert_eq!(
                truthy(&get(&h.owner, "forceUnifiedBalance")),
                matches!(h.profile.as_str(), "force" | "retry" | "repeat-error"),
                "wallet_native_force"
            );
            if h.profile != "reject" {
                let v = result.as_ref().unwrap();
                if matches!(
                    h.profile.as_str(),
                    "ret-error" | "missing-code" | "repeat-error" | "no-info" | "empty-info"
                ) {
                    if h.profile == "no-info" {
                        assert!(elements(v).is_empty(), "wallet_native_fallback_empty");
                    } else {
                        assert!(Rc::ptr_eq(v, &h.old_info), "wallet_native_fallback_alias");
                    }
                    assert_eq!(h.count("emit"), 1, "wallet_native_emission");
                } else {
                    assert!(Rc::ptr_eq(v, &h.balance_list), "wallet_native_list_alias");
                }
            }
        }
        "formatBalance" => {
            let v = result.as_ref().unwrap();
            if matches!(h.profile.as_str(), "empty" | "null" | "nonarray") {
                assert!(Rc::ptr_eq(v, &h.old_balance), "balance_native_store_alias");
                assert_eq!(h.count("log"), 1, "balance_native_warning");
            } else {
                assert!(
                    Rc::ptr_eq(&get(v, "info"), &h.first),
                    "balance_native_info_alias"
                );
                let total = numeric(&get(v, "total"));
                let pnl = if h.profile == "string-pnl" { 23.0 } else { 5.0 };
                if h.profile == "missing-equity" {
                    assert!(total.is_nan(), "balance_native_total");
                } else {
                    assert_eq!(total, 100.25 - pnl, "balance_native_total");
                }
                assert_eq!(numeric(&get(v, "used")), 3.75, "balance_native_used");
                let free = numeric(&get(v, "free"));
                if h.profile == "free-false" {
                    assert!(free.is_nan(), "balance_native_free");
                } else {
                    assert_eq!(
                        free,
                        if h.profile == "free-null" {
                            60.0
                        } else if h.profile == "free-zero" {
                            0.0
                        } else {
                            70.0
                        },
                        "balance_native_free"
                    );
                }
                let coins = get(v, "coins");
                let key = match h.profile.as_str() {
                    "constructor-coin" => u("constructor"),
                    "lone-coin" => u("C").into_iter().chain([0xd800]).collect(),
                    _ => u("BTC"),
                };
                if h.profile == "proto-coin" {
                    let Node::Object {
                        fields, prototype, ..
                    } = coins.as_ref()
                    else {
                        panic!("coins")
                    };
                    assert!(
                        fields.borrow().iter().all(|(k, _)| k != &u("__proto__")),
                        "balance_native_proto_own_absent"
                    );
                    assert!(
                        eq(
                            &get(prototype.borrow().as_ref().unwrap(), "base"),
                            &raw("__proto__")
                        ),
                        "balance_native_proto_value"
                    );
                } else if h.profile != "empty-coin" {
                    assert_eq!(
                        numeric(&get(&get_units(&coins, &key), "amount")),
                        if h.profile == "duplicates" { 3.0 } else { 1.5 },
                        "balance_native_coin_amount"
                    );
                }
            }
        }
        "fetchOrdersHelper" => {
            let failure = matches!(h.profile.as_str(), "ret-error" | "second-error" | "reject");
            assert_eq!(result.is_err(), failure, "orders_native_outcome");
            let query = if h.profile == "repeat-cursor" {
                3
            } else if matches!(
                h.profile.as_str(),
                "pages" | "numeric-cursor" | "second-error"
            ) {
                2
            } else {
                1
            };
            assert_eq!(*h.queries.borrow(), query, "orders_native_request_floor");
            for call in h.calls.borrow().iter().filter(|c| c["name"] == "xhr:get") {
                let params = &call["args"][1]["o"][0][1]["o"];
                assert_eq!(
                    params[0][1],
                    atom(&raw("inverse")),
                    "orders_native_category"
                );
                assert_eq!(
                    params[1][1],
                    atom(&if h.profile == "settle-null" {
                        null()
                    } else {
                        undef()
                    }),
                    "orders_native_settle"
                );
                assert_eq!(params[2][1], atom(&number(50.0)), "orders_native_limit");
            }

            assert_eq!(
                truthy(&get(&h.owner, "needsReinit")),
                matches!(h.profile.as_str(), "ret-error" | "second-error"),
                "orders_native_reinit"
            );
            if !failure {
                let rows = elements(result.as_ref().unwrap());
                let count = if matches!(h.profile.as_str(), "empty" | "malformed") {
                    0
                } else if matches!(
                    h.profile.as_str(),
                    "pages" | "numeric-cursor" | "repeat-cursor"
                ) {
                    2
                } else {
                    1
                };
                assert_eq!(rows.len(), count, "orders_native_count");
                if count > 0 {
                    assert!(
                        Rc::ptr_eq(&rows[0], &h.mapped[0]),
                        "orders_native_map_alias"
                    );
                }
            }
        }
        "fetchMarkets" => {
            assert_eq!(
                result.is_err(),
                h.profile == "reject",
                "markets_native_outcome"
            );
            if h.profile == "null" {
                assert!(
                    Rc::ptr_eq(result.as_ref().unwrap(), &h.old_markets),
                    "markets_native_store_alias"
                );
                assert_eq!(h.count("emit"), 1, "markets_native_emission");
            } else if h.profile != "reject" {
                let rows = elements(result.as_ref().unwrap());
                let count = if h.profile == "empty" {
                    0
                } else if h.profile == "multiple" {
                    2
                } else {
                    1
                };
                assert_eq!(rows.len(), count, "markets_native_count");
                if h.profile == "undefined" {
                    assert!(eq(&rows[0], &undef()), "markets_native_undefined_retained");
                } else if count > 0 {
                    assert!(
                        Rc::ptr_eq(&rows[0], &h.markets[0]),
                        "markets_native_original_alias"
                    );
                }
            }
        }
        _ => panic!("native row"),
    }
}
#[test]
fn source_and_native_inverse_reads() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-read-surface-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 488, "declared case floor");
    let mut unique = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut comparisons = vec![];
    for case in cases {
        let m = case["method"].as_str().unwrap();
        let p = case["profile"].as_str().unwrap();
        let f = case["failure"].as_str().unwrap();
        assert!(unique.insert((m.to_string(), p.to_string(), f.to_string())));
        *counts.entry(m.to_string()).or_insert(0_usize) += 1;
        let h = Harness::new(m, p, f);
        let (result, sync) = h.invoke();
        native(&h, &result);
        let proto = if m == "formatBalance" {
            if let Ok(v) = &result {
                let coins = get(v, "coins");
                if let Node::Object { prototype, .. } = coins.as_ref() {
                    prototype
                        .borrow()
                        .as_ref()
                        .map(|p| atom(&get(p, "base")))
                        .unwrap_or_else(|| atom(&undef()))
                } else {
                    atom(&undef())
                }
            } else {
                atom(&undef())
            }
        } else {
            atom(&undef())
        };
        let first_native = result.as_ref().ok().cloned();
        let mut result = match &result {
            Ok(value) => json!({"ok":true,"value":atom(value)}),
            Err(error) => {
                json!({"ok":false,"error":atom(error),"primary":Rc::ptr_eq(&error,&h.primary)})
            }
        };
        if m == "incomeContext" && result["ok"] == true {
            let second = api::income_context(&h, &h.owner).unwrap();
            assert!(
                !Rc::ptr_eq(first_native.as_ref().unwrap(), &second),
                "context_native_fresh"
            );
            result["fresh"] = json!(true);
        }
        if m == "cachedApiKeyInfo" && p == "changed" && result["ok"] == true {
            let second = api::cached_api_key_info(&h, &h.owner).unwrap();
            assert_eq!(
                atom(&second),
                json!({"ref":"alternate"}),
                "cache_native_live"
            );
            result["second"] = atom(&second);
        }
        let receipt = json!({"calls":*h.calls.borrow(),"logs":*h.logs.borrow(),"syncCalls":sync,"result":result,"proto":proto,"state":h.state()});
        comparisons.push((case, receipt));
    }
    assert_eq!(
        counts,
        std::collections::BTreeMap::from([
            ("fetchRawBalance".into(), 88),
            ("formatBalance".into(), 112),
            ("fetchOrdersHelper".into(), 80),
            ("fetchMarkets".into(), 48),
            ("fetchIncomeHistory".into(), 48),
            ("fetchIncomeHistoryRange".into(), 48),
            ("incomeContext".into(), 24),
            ("cachedApiKeyInfo".into(), 40)
        ]),
        "independent method floor"
    );
    let groups: &[(&str, &[&str])] = &[
        (
            "fetchRawBalance",
            &[
                "normal",
                "force",
                "retry",
                "repeat-error",
                "ret-error",
                "code-zero",
                "missing-code",
                "snake",
                "no-info",
                "empty-info",
                "reject",
            ],
        ),
        (
            "formatBalance",
            &[
                "normal",
                "empty",
                "null",
                "nonarray",
                "empty-coin",
                "duplicates",
                "proto-coin",
                "constructor-coin",
                "lone-coin",
                "string-pnl",
                "free-null",
                "free-zero",
                "free-false",
                "missing-equity",
            ],
        ),
        (
            "fetchOrdersHelper",
            &[
                "normal",
                "pages",
                "empty",
                "malformed",
                "repeat-cursor",
                "numeric-cursor",
                "ret-error",
                "second-error",
                "reject",
                "settle-null",
            ],
        ),
        (
            "fetchMarkets",
            &["normal", "multiple", "null", "empty", "undefined", "reject"],
        ),
        (
            "fetchIncomeHistory",
            &["normal", "undefined", "null", "lone", "reject", "throw"],
        ),
        (
            "fetchIncomeHistoryRange",
            &["normal", "undefined", "null", "lone", "reject", "throw"],
        ),
        ("incomeContext", &["normal", "classic", "changed"]),
        (
            "cachedApiKeyInfo",
            &["normal", "undefined", "null", "lone", "changed"],
        ),
    ];
    let failures = [
        "none",
        "get:xhr",
        "get:log",
        "get:apiKeyInfo",
        "xhr:get",
        "income",
        "emit",
        "parseFloat",
    ];
    let mut expected = std::collections::BTreeSet::new();
    for (method, profiles) in groups {
        for profile in *profiles {
            for failure in failures {
                assert!(expected.insert((
                    method.to_string(),
                    profile.to_string(),
                    failure.to_string()
                )));
            }
        }
    }
    assert_eq!(expected.len(), 488, "declared native product floor");
    assert_eq!(
        unique, expected,
        "independent exact profile/fault inventory"
    );
    for (case, receipt) in comparisons {
        for key in ["calls", "logs", "syncCalls", "result", "proto", "state"] {
            assert_eq!(
                receipt[key], case[key],
                "supplemental {}/{}/{} {key}",
                case["method"], case["profile"], case["failure"]
            );
        }
    }
}

fn boundary_source(id: &str) -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/inverse-read-surface-boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(corpus["declaredCases"], 5);
    corpus["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["id"] == id)
        .unwrap()["source"]
        .clone()
}
#[test]
fn balance_strict_length_boundary() {
    let h = Harness::new("formatBalance", "balance-length-string-zero", "none");
    set(&get(&h.owner, "store"), "positions", array(vec![]));
    set(&h.first, "totalEquity", raw("100"));
    let result = api::format_balance(
        &h,
        h.owner.clone(),
        reference("length-string-zero", h.balance_list.clone()),
    )
    .unwrap();
    assert_eq!(
        numeric(&get(&result, "total")),
        100.0,
        "balance_native_strict_length"
    );
    assert!(
        Rc::ptr_eq(&get(&result, "info"), &h.first),
        "balance_native_boundary_info"
    );
    assert_eq!(
        *h.boundary_trace.borrow(),
        ["length:get", "iterator:get"],
        "balance_native_boundary_trace"
    );
    assert_eq!(h.count("log"), 0, "balance_native_boundary_no_warning");
    assert!(
        h.logs.borrow().is_empty(),
        "balance_native_boundary_empty_logs"
    );
    let receipt = json!({"outcome":"fulfilled", "total":100, "originalInfo":true, "oldFallback":Rc::ptr_eq(&result,&h.old_balance), "trace":*h.boundary_trace.borrow()});
    assert_eq!(
        receipt,
        boundary_source("balance-length-string-zero"),
        "supplemental balance boundary"
    );
}
#[test]
fn orders_strict_length_boundary() {
    let h = Harness::new("fetchOrdersHelper", "orders-length-string-zero", "none");
    let (result, _) = h.invoke();
    let rows = elements(&result.unwrap());
    assert_eq!(rows.len(), 1, "orders_native_strict_length");
    assert!(
        Rc::ptr_eq(&rows[0], &h.mapped[0]),
        "orders_native_boundary_alias"
    );
    assert_eq!(
        *h.boundary_trace.borrow(),
        [
            "xhr:get",
            "reinit:call",
            "length:get",
            "iterator:get",
            "map:row-one"
        ],
        "orders_native_boundary_trace"
    );
    let receipt = json!({"outcome":"fulfilled", "count":rows.len(), "originalMapped":true, "trace":*h.boundary_trace.borrow()});
    assert_eq!(
        receipt,
        boundary_source("orders-length-string-zero"),
        "supplemental orders boundary"
    );
}
fn market_boundary(id: &str, trace: &[&str], fails: bool) {
    let h = Harness::new("fetchMarkets", id, "none");
    let (result, _) = h.invoke();
    assert_eq!(
        result.is_err(),
        fails,
        "markets_native_boundary_outcome {id}"
    );
    let receipt = if fails {
        assert!(
            Rc::ptr_eq(result.as_ref().unwrap_err(), &h.primary),
            "markets_native_boundary_primary"
        );
        json!({"outcome":"rejected", "originalPrimary":true, "trace":*h.boundary_trace.borrow()})
    } else {
        let rows = elements(result.as_ref().unwrap());
        assert_eq!(rows.len(), 1, "markets_native_boundary_count");
        assert!(
            Rc::ptr_eq(&rows[0], &h.markets[0]),
            "markets_native_boundary_alias"
        );
        let mut receipt = json!({"outcome":"fulfilled", "count":rows.len(), "originalMarket":true, "trace":*h.boundary_trace.borrow()});
        if id == "markets-all-capture-before-map" {
            receipt["originalPromiseReceiver"] = json!(true);
        }
        receipt
    };
    assert_eq!(
        *h.boundary_trace.borrow(),
        trace,
        "markets_native_boundary_trace {id}"
    );
    assert_eq!(
        receipt,
        boundary_source(id),
        "supplemental markets boundary {id}"
    );
}
#[test]
fn markets_then_getter_boundary() {
    market_boundary(
        "markets-then-getter-interleave",
        &[
            "map:call",
            "iterator:get",
            "next:0",
            "then:get",
            "next:1",
            "then:call",
        ],
        false,
    );
}
#[test]
fn markets_all_capture_boundary() {
    market_boundary(
        "markets-all-capture-before-map",
        &[
            "all:get",
            "categories:get",
            "map:get",
            "helper:get",
            "bind:get",
            "map:call",
            "all:call:original-receiver",
        ],
        false,
    );
}
#[test]
fn markets_noncallable_evaluation_boundary() {
    market_boundary(
        "markets-noncallable-all-map-throws",
        &["all:get", "categories:get", "map:get", "map:call"],
        true,
    );
}
