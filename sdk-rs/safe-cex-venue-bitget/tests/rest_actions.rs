use safe_cex_venue_bitget::rest_actions::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Hole,
    U,
    Null,
    Bool(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
}
fn s(x: &str) -> V {
    V::S(x.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(fields: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::Hole | V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => n.to_string(),
        V::S(s) => String::from_utf16_lossy(s),
        V::A(_) => "".into(),
        V::O(_) => "[object Object]".into(),
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == &k.encode_utf16().collect::<Vec<_>>())
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Hole | V::U | V::Null => false,
        V::Bool(b) => *b,
        V::N(n) => *n != 0. && !n.is_nan(),
        V::S(t) => !t.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("REST native array required"),
    }
}
fn native(actual: &V, expected: &V, path: &str) {
    match (actual, expected) {
        (V::N(x), V::N(y)) => assert!(
            if y.is_nan() {
                x.is_nan()
            } else {
                x.to_bits() == y.to_bits()
            },
            "REST native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "REST native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "REST native independent array count {path}"
            );
            for (i, (x, y)) in x.iter().zip(y.iter()).enumerate() {
                native(x, y, &format!("{path}.{i}"))
            }
        }
        (V::O(x), V::O(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "REST native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "REST native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "REST native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Hole => json!({"hole":true}),
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::Bool(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(t) => json!({"s":t}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(_) => {
            json!({"o":keys(v).into_iter().map(|k|json!([k,wire(&key(v,&k))])).collect::<Vec<_>>()})
        }
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("settled fixture futures"),
    }
}

#[derive(Clone, Debug)]
struct E {
    identity: Rc<()>,
    value: V,
}
fn put(v: &V, key: Vec<u16>, field: V) {
    let V::O(v) = v else { panic!("native object") };
    let mut v = v.borrow_mut();
    if let Some((_, f)) = v.iter_mut().find(|(k, _)| *k == key) {
        *f = field
    } else {
        v.push((key, field))
    }
}
fn keys(v: &V) -> Vec<Vec<u16>> {
    let V::O(v) = v else {
        panic!("native keys object")
    };
    let mut keys = v
        .borrow()
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    keys.sort_by_key(|k| {
        let t = String::from_utf16_lossy(k);
        match t.parse::<u32>() {
            Ok(n) if n != u32::MAX && n.to_string() == t => (0, n),
            _ => (1, 0),
        }
    });
    keys
}
fn key(v: &V, k: &[u16]) -> V {
    let V::O(v) = v else { return V::U };
    v.borrow()
        .iter()
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::U)
}
fn compare(a: &Json, b: &Json, path: &str) {
    if a == b {
        return;
    }
    match (a, b) {
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "REST source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "REST source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("REST source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "REST source value {path}"),
    }
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
struct H {
    op: &'static str,
    scene: &'static str,
    order: V,
    update: V,
    market: V,
    positions: V,
    orders: V,
    store: V,
    cache: V,
    inflight: V,
    state: V,
    original: E,
    events: RefCell<Vec<Event>>,
    side: V,
    symbol: V,
    input: V,
    isolated: V,
    hedged: V,
    fetched: V,
    amended: V,
}
fn position(side: &str, lev: f64, mode: &str, margin: &str) -> V {
    o(vec![
        ("symbol", s("BTCUSD")),
        ("side", s(side)),
        ("leverage", V::N(lev)),
        ("positionMode", s(mode)),
        ("marginType", s(margin)),
    ])
}
fn make(op: &'static str, scene: &'static str) -> H {
    let market = if scene == "missing-market" {
        V::U
    } else {
        o(vec![
            ("id", s("BTC_NATIVE")),
            (
                "precision",
                o(vec![("price", V::N(0.5)), ("amount", V::N(0.1))]),
            ),
            (
                "limits",
                o(vec![(
                    "leverage",
                    o(vec![("min", V::N(1.)), ("max", V::N(20.))]),
                )]),
            ),
        ])
    };
    let kind = if scene == "algo" || scene == "uta-algo" {
        "stop_loss"
    } else {
        "limit"
    };
    let order = o(vec![
        (
            "id",
            s(if op.contains("algo") || scene == "virtual" {
                "old__stop_loss"
            } else {
                "old"
            }),
        ),
        ("symbol", s("BTCUSD")),
        ("type", s(kind)),
        ("side", s("buy")),
        ("price", V::N(10.2)),
        (
            "amount",
            V::N(if scene == "position-level" { 0. } else { 2.24 }),
        ),
        ("reduceOnly", V::U),
        ("timeInForce", s("GTC")),
        (
            "info",
            o(vec![(
                "__tealstreetUseLegacyBitgetDragUpdate",
                V::Bool(scene == "legacy"),
            )]),
        ),
        ("tpTriggerType", V::U),
        ("slTriggerType", s("mark")),
    ]);
    let update = match scene {
        "price-only" => o(vec![("price", V::N(11.7))]),
        "amount-only" => o(vec![("amount", V::N(3.36))]),
        "no-amount" | "position-level" => o(vec![("price", V::N(11.7))]),
        "invalid" => o(vec![("price", V::N(0.)), ("amount", V::N(3.36))]),
        "zero-amount" => o(vec![("amount", V::N(0.))]),
        "explicit-undefined" => o(vec![("price", V::U), ("amount", V::U)]),
        _ => o(vec![("price", V::N(11.7)), ("amount", V::N(3.36))]),
    };
    let orders = if scene == "virtual" || scene == "virtual-child" {
        a(vec![
            o(vec![("id", s("old"))]),
            o(vec![("id", s("old__stop_loss"))]),
        ])
    } else {
        a(vec![V::Hole])
    };
    let positions = if scene == "missing-position" {
        a(vec![])
    } else {
        a(vec![
            position(
                "long",
                if scene == "unchanged" { 20. } else { 2. },
                "hedged",
                if scene == "cross" {
                    "cross"
                } else {
                    "isolated"
                },
            ),
            position(
                "short",
                if scene == "unchanged" { 20. } else { 3. },
                "hedged",
                if scene == "cross" {
                    "cross"
                } else {
                    "isolated"
                },
            ),
        ])
    };
    let store = o(vec![
        ("marketsBySymbol", o(vec![("BTCUSD", market.clone())])),
        ("positions", positions.clone()),
        ("orders", orders.clone()),
    ]);
    let cache = o(vec![("BTCUSD", s("cached")), ("KEEP", s("keep"))]);
    let inflight = o(vec![("BTCUSD", s("flight")), ("KEEP", s("keep-flight"))]);
    let state = o(vec![
        ("cache", cache.clone()),
        ("inflight", inflight.clone()),
    ]);
    let original = E {
        identity: Rc::new(()),
        value: o(vec![
            ("message", s("original")),
            (
                "response",
                o(vec![("data", o(vec![("msg", s("api original"))]))]),
            ),
        ]),
    };
    H {
        op,
        scene,
        order,
        update,
        market,
        positions,
        orders,
        store,
        cache,
        inflight,
        state,
        original,
        events: RefCell::new(vec![]),
        side: s(if scene == "fallback-side" {
            "missing"
        } else {
            "short"
        }),
        symbol: if scene == "missing-symbol" {
            V::U
        } else {
            s("BTCUSD")
        },
        input: V::N(27.),
        isolated: V::Bool(true),
        hedged: V::Bool(scene != "one-way"),
        fetched: a(vec![position("short", 7., "hedged", "isolated")]),
        amended: if scene == "empty-amend" {
            a(vec![])
        } else if scene == "fallback"
            || scene == "legacy"
            || scene == "uta"
            || scene == "explicit-undefined"
        {
            V::Null
        } else {
            a(vec![s("amended")])
        },
    }
}
fn path_method(path: &str, owner: V) -> V {
    o(vec![("path", s(path)), ("owner", owner)])
}
fn n(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        V::U => f64::NAN,
        V::Null => 0.,
        V::Bool(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        _ => text(v).parse().unwrap_or(f64::NAN),
    }
}
impl H {
    fn record(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        })
    }
    fn uta(&self) -> bool {
        self.scene == "uta" || self.scene == "uta-algo"
    }
    fn fail(&self, path: &str) -> bool {
        self.scene == "reject" && path.ends_with(".post")
            || self.scene == "fetch-reject" && path == "fetchPositions"
            || self.scene == "schedule-throw" && path == "schedulePrivateStreamWriteCheck"
            || self.scene == "adjust-throw" && path == "adjust"
    }
}
impl RestActionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, path: &str) -> Result<V, E> {
        Ok(match path {
            "store.marketsBySymbol" => get(&self.store, "marketsBySymbol"),
            "store.orders" => self.orders.clone(),
            "store.positions" => self.positions.clone(),
            "apiProductType" => s("dmcbl"),
            "apiMarginCoin" => s("USDT"),
            "apiUtaCategory" => s("inverse"),
            "usesUtaRouting" => V::Bool(self.uta()),
            "_positioningCache" => get(&self.state, "cache"),
            "_positioningInflight" => get(&self.state, "inflight"),
            "OrderType.Limit" => s("limit"),
            "PositionMode.Hedged" => s("hedged"),
            "MarginType.Isolated" => s("isolated"),
            "PositionSide.Short" => s("short"),
            _ if path.starts_with("ENDPOINTS.") => s(path.strip_prefix("ENDPOINTS.").unwrap()),
            _ => panic!("REST unknown receiver {path}"),
        })
    }
    fn set_receiver(&self, path: &str, v: V) -> Result<(), E> {
        self.record(&format!("set:{path}"), vec![v.clone()]);
        put(
            &self.state,
            s(if path == "_positioningCache" {
                "cache"
            } else {
                "inflight"
            })
            .units(),
            v,
        );
        Ok(())
    }
    fn capture(&self, path: &str) -> Result<V, E> {
        let owner = if path.starts_with("store.") {
            self.store.clone()
        } else {
            V::U
        };
        if ![
            "uuid",
            "adjust",
            "String",
            "Boolean",
            "parseInt",
            "Math.min",
            "Math.max",
            "isBitgetSuccessCode",
            "Error",
        ]
        .contains(&path)
        {
            self.record(&format!("capture:{path}"), vec![])
        }
        Ok(path_method(path, owner))
    }
    fn capture_member(&self, v: &V, name: &str) -> Result<V, E> {
        Ok(path_method(name, v.clone()))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        let path = text(&get(&f, "path"));
        let owner = get(&f, "owner");
        if path.starts_with("store.") {
            assert!(equal(&owner, &self.store), "REST native bound store owner");
        }
        let ordinary = ![
            "uuid",
            "adjust",
            "String",
            "Boolean",
            "parseInt",
            "Math.min",
            "Math.max",
            "isBitgetSuccessCode",
            "Error",
            "split",
            "startsWith",
            "toUpperCase",
        ]
        .contains(&path.as_str());
        if ordinary {
            self.record(&format!("call:{path}"), args.clone())
        }
        if self.fail(&path) {
            return Err(self.original.clone());
        }
        Ok(match path.as_str() {
            "uuid" => s("native-uuid"),
            "adjust" => V::N(safe_cex_num::adjust(n(&args[0]), n(&args[1]))),
            "String" => s(&text(&args[0])),
            "Boolean" => V::Bool(truth(&args[0])),
            "parseInt" => V::N(safe_cex_num::parse_int(&text(&args[0]), None)),
            "Math.max" => V::N(n(&args[0]).max(n(&args[1]))),
            "Math.min" => V::N(n(&args[0]).min(n(&args[1]))),
            "isBitgetSuccessCode" => V::Bool(safe_cex_num::parse_int(&text(&args[0]), None) == 0.),
            "split" => a(text(&owner).split(&text(&args[0])).map(s).collect()),
            "startsWith" => V::Bool(text(&owner).starts_with(&text(&args[0]))),
            "toUpperCase" => s(&text(&owner).to_uppercase()),
            "getMarginCoinForSymbol" => s("BTC"),
            "isVirtualTPSLOrder" => V::Bool(self.scene == "virtual"),
            "isAlgoOrder" => V::Bool(self.scene == "algo" || self.scene == "uta-algo"),
            "tryAmendLimitOrder" => self.amended.clone(),
            "updateVirtualTpSlOrder" => a(vec![s("virtual-result")]),
            "updateAlgoOrder" => a(vec![s("algo-result")]),
            "updateUtaStrategyOrder" => a(vec![s("uta-algo-result")]),
            "placeOrder" => a(vec![s("placed")]),
            "fetchPositions" => self.fetched.clone(),
            "fetchUtaPositionMetadata" => a(vec![o(vec![("leverage", V::N(0.))])]),
            "xhr.post" | "unlimitedXHR.post" => o(vec![(
                "data",
                if path == "unlimitedXHR.post" && self.op.contains("algo") {
                    o(vec![("data", o(vec![("orderId", s("updated"))]))])
                } else {
                    o(vec![
                        (
                            "code",
                            s(if self.scene == "api-code" {
                                "400"
                            } else {
                                "00000"
                            }),
                        ),
                        ("msg", s("api failure")),
                        ("data", o(vec![("orderId", s("updated"))])),
                    ])
                },
            )]),
            _ => V::U,
        })
    }
    fn start(&self, f: V, args: Vec<V>) -> Result<RestActionFuture<'_, V, E>, E> {
        let r = self.invoke(f, args);
        Ok(Box::pin(async move { r }))
    }
    fn property(&self, v: &V, k: &str) -> Result<V, E> {
        if k == "length" {
            if let V::A(a) = v {
                return Ok(V::N(a.borrow().len() as f64));
            }
        }
        if matches!(v, V::U | V::Null) {
            return Err(self.original.clone());
        }
        Ok(get(v, k))
    }
    fn key_get(&self, v: &V, k: &V) -> Result<V, E> {
        Ok(if let V::A(a) = v {
            a.borrow().get(n(k) as usize).cloned().unwrap_or(V::U)
        } else {
            get(v, &text(k))
        })
    }
    fn has(&self, v: &V, k: &str) -> Result<bool, E> {
        Ok(keys(v).contains(&k.encode_utf16().collect()))
    }
    fn assign(&self, v: &V, k: &str, field: V) -> Result<(), E> {
        put(v, k.encode_utf16().collect(), field);
        Ok(())
    }
    fn define(&self, v: &V, k: &str, field: V) -> Result<(), E> {
        put(v, k.encode_utf16().collect(), field);
        Ok(())
    }
    fn delete(&self, v: &V, k: &V) -> Result<(), E> {
        let path = if equal(v, &self.cache) {
            "_positioningCache"
        } else {
            "_positioningInflight"
        };
        self.record(&format!("delete:{path}"), vec![k.clone()]);
        let V::O(o) = v else {
            panic!("REST delete object")
        };
        o.borrow_mut().retain(|(key, _)| *key != k.units());
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Null | V::U)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn string(&self, s: &[u16]) -> V {
        V::S(s.to_vec())
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn null(&self) -> V {
        V::Null
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, fields: Vec<(&str, V)>) -> V {
        o(fields)
    }
    fn filter<F>(&self, _f: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        let mut out = vec![];
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v.clone())? {
                out.push(v)
            }
        }
        Ok(a(out))
    }
    fn find<F>(&self, _f: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        for v in values(&array) {
            let v = if matches!(v, V::Hole) { V::U } else { v };
            if f(v.clone())? {
                return Ok(v);
            }
        }
        Ok(V::U)
    }
    fn some<F>(&self, _f: V, array: V, mut f: F) -> Result<bool, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn operation(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match name {
            "template" => s(&text(&args[0])),
            "missing" => s(&format!("{} {} not found", text(&args[0]), text(&args[1]))),
            "suffix" => s(&format!("{}{}", text(&args[0]), text(&args[1]))),
            "greater" => V::Bool(n(&args[0]) > n(&args[1])),
            "unaryPlus" => V::N(n(&args[0])),
            "destructureFirst" => values(&args[0]).first().cloned().unwrap_or(V::U),
            _ => panic!("REST unknown operation {name}"),
        })
    }
    fn new_error(&self, _f: V, msg: V) -> Result<E, E> {
        Ok(E {
            identity: Rc::new(()),
            value: o(vec![("message", msg)]),
        })
    }
    fn error_value(&self, e: &E) -> V {
        e.value.clone()
    }
}
impl V {
    fn units(&self) -> Vec<u16> {
        match self {
            V::S(s) => s.clone(),
            _ => panic!("REST native string"),
        }
    }
}
fn execute(h: &H) -> Result<V, E> {
    run(async {
        match h.op {
            "amend" => body::try_amend_limit_order(h, h.order.clone(), h.update.clone()).await,
            "route" => body::update_order(h, h.order.clone(), h.update.clone()).await,
            "algo" => body::update_algo_order(h, h.order.clone(), h.update.clone()).await,
            "inverse-algo" => {
                body::inverse_update_algo_order(h, h.order.clone(), h.update.clone()).await
            }
            "leverage" => {
                body::set_leverage(h, h.symbol.clone(), h.input.clone(), h.side.clone()).await
            }
            "inverse-leverage" => {
                body::inverse_set_leverage(h, h.symbol.clone(), h.input.clone(), h.side.clone())
                    .await
            }
            "margin" => body::change_margin_type(h, h.isolated.clone(), h.symbol.clone()).await,
            "inverse-margin" => {
                body::inverse_change_margin_type(h, h.isolated.clone(), h.symbol.clone()).await
            }
            "mode" => body::change_position_mode(h, h.hedged.clone()).await,
            _ => unreachable!(),
        }
    })
}
fn inputs(h: &H) -> Json {
    json!({"op":h.op,"scene":h.scene,"order":wire(&h.order),"update":wire(&h.update),"market":wire(&h.market),"positions":wire(&h.positions),"orders":wire(&h.orders),"side":wire(&h.side),"symbol":wire(&h.symbol),"input":wire(&h.input),"isolated":wire(&h.isolated),"hedged":wire(&h.hedged)})
}
fn output(h: &H, r: &Result<V, E>) -> Json {
    json!({"result":match r{Ok(v)=>json!({"ok":wire(v)}),Err(e)=>json!({"original":Rc::ptr_eq(&e.identity,&h.original.identity),"message":wire(&get(&e.value,"message"))})},"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"state":wire(&h.state)})
}
fn ev(h: &H, name: &str) -> Vec<Event> {
    h.events
        .borrow()
        .iter()
        .filter(|e| e.name == name)
        .cloned()
        .collect()
}
fn count(h: &H, name: &str, expected: usize) {
    assert_eq!(
        ev(h, name).len(),
        expected,
        "REST native independent event count {} {} {name}",
        h.op,
        h.scene
    )
}
fn payload(h: &H, path: &str) -> V {
    let e = ev(h, &format!("call:{path}"));
    assert_eq!(e.len(), 1, "REST native independent HTTP count");
    assert_eq!(
        e[0].args.len(),
        2,
        "REST native independent HTTP argument count"
    );
    e[0].args[1].clone()
}
fn expect_result(r: &Result<V, E>, v: V) {
    native(
        r.as_ref().expect("REST expected native success"),
        &v,
        "result",
    )
}
fn original_error(h: &H, r: &Result<V, E>) {
    let e = r.as_ref().expect_err("REST expected native original error");
    assert!(
        Rc::ptr_eq(&e.identity, &h.original.identity),
        "REST native original Error identity"
    )
}
fn refreshed(h: &H, global: bool) {
    count(h, "call:fetchPositions", 1);
    let e = ev(h, "call:fetchPositions");
    native(
        &a(e[0].args.clone()),
        &if global {
            a(vec![])
        } else {
            a(vec![a(vec![s("BTCUSD")])])
        },
        "fetchPositions arguments",
    );
    let method = if global {
        "call:store.update"
    } else {
        "call:store.addOrUpdatePositions"
    };
    count(h, method, 1);
    let e = ev(h, method);
    assert_eq!(e[0].args.len(), 1, "REST native store argument count");
    let p = if global {
        get(&e[0].args[0], "positions")
    } else {
        e[0].args[0].clone()
    };
    assert!(
        equal(&p, &h.fetched),
        "REST native fetched positions original reference"
    );
    native(
        &p,
        &a(vec![position("short", 7., "hedged", "isolated")]),
        "updated positions",
    );
}
fn check(h: &H, r: &Result<V, E>) {
    let inv = h.op.starts_with("inverse");
    match h.op {
        "amend" => {
            let no_post =
                ["invalid", "missing-market", "virtual-child", "adjust-throw"].contains(&h.scene);
            count(h, "call:unlimitedXHR.post", if no_post { 0 } else { 1 });
            count(h, "call:emitAndLogError", 0);
            if h.scene == "adjust-throw" {
                original_error(h, r);
                return;
            }
            let null = [
                "missing-market",
                "virtual-child",
                "api-code",
                "reject",
                "schedule-throw",
            ]
            .contains(&h.scene);
            expect_result(
                r,
                if null {
                    V::Null
                } else if h.scene == "invalid" {
                    a(vec![])
                } else {
                    a(vec![s("updated")])
                },
            );
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(!no_post && !matches!(h.scene, "api-code" | "reject")),
            );
            if !no_post {
                native(
                    &ev(h, "call:unlimitedXHR.post")[0].args[0],
                    &s("UPDATE_ORDER"),
                    "amend endpoint",
                );
                native(
                    &payload(h, "unlimitedXHR.post"),
                    &o(vec![
                        ("orderId", s("old")),
                        ("symbol", s("BTCUSD")),
                        ("productType", s("dmcbl")),
                        ("newClientOid", s("native-uuid")),
                        (
                            "newPrice",
                            s(if h.scene == "amount-only" {
                                "10"
                            } else {
                                "11.5"
                            }),
                        ),
                        (
                            "newSize",
                            s(if h.scene == "price-only" {
                                "2.2"
                            } else {
                                "3.4"
                            }),
                        ),
                    ]),
                    "amend payload",
                );
            }
        }
        "route" => {
            let result = match h.scene {
                "virtual" => "virtual-result",
                "algo" => "algo-result",
                "uta-algo" => "uta-algo-result",
                "empty-amend" => "",
                "normal" => "amended",
                _ => "placed",
            };
            expect_result(
                r,
                if result.is_empty() {
                    a(vec![])
                } else {
                    a(vec![s(result)])
                },
            );
            let replacement =
                !["virtual", "algo", "uta-algo", "empty-amend", "normal"].contains(&h.scene);
            count(h, "call:cancelOrders", usize::from(replacement));
            count(h, "call:placeOrder", usize::from(replacement));
            count(
                h,
                "call:ensureAccountTypeDetected",
                usize::from(h.scene != "virtual"),
            );
            count(
                h,
                "call:tryAmendLimitOrder",
                usize::from(!["virtual", "algo", "uta-algo", "legacy", "uta"].contains(&h.scene)),
            );
            if matches!(h.scene, "normal" | "empty-amend") {
                assert!(
                    equal(r.as_ref().unwrap(), &h.amended),
                    "REST native amend result original array reference"
                )
            }
            if h.scene == "virtual" {
                let e = ev(h, "call:updateVirtualTpSlOrder");
                assert_eq!(e.len(), 1, "REST native virtual route count");
                assert_eq!(e[0].args.len(), 2, "REST native virtual argument count");
                let related = values(&e[0].args[1]);
                assert_eq!(
                    related.len(),
                    2,
                    "REST native related-order independent count"
                );
                for (i, v) in related.iter().enumerate() {
                    assert!(
                        equal(v, &values(&h.orders)[i]),
                        "REST native related-order aliases"
                    )
                }
                assert!(
                    equal(&get(&e[0].args[0], "order"), &h.order),
                    "REST native routed order alias"
                )
            }
            if replacement {
                let e = ev(h, "call:cancelOrders");
                let canceled = values(&e[0].args[0]);
                assert_eq!(canceled.len(), 1, "REST native cancellation count");
                assert!(
                    equal(&canceled[0], &h.order),
                    "REST native cancellation original order"
                );
                let e = ev(h, "call:placeOrder");
                native(
                    &e[0].args[0],
                    &o(vec![
                        ("symbol", s("BTCUSD")),
                        ("type", s("limit")),
                        ("side", s("buy")),
                        (
                            "price",
                            if h.scene == "explicit-undefined" {
                                V::U
                            } else {
                                V::N(11.7)
                            },
                        ),
                        (
                            "amount",
                            if h.scene == "explicit-undefined" {
                                V::U
                            } else {
                                V::N(3.36)
                            },
                        ),
                        ("reduceOnly", V::Bool(false)),
                        ("timeInForce", s("GTC")),
                        ("tpTriggerType", V::U),
                        ("slTriggerType", s("mark")),
                    ]),
                    "replacement order fields",
                );
                assert!(
                    !equal(&e[0].args[0], &h.order),
                    "REST native replacement is a new object"
                );
                let names = h
                    .events
                    .borrow()
                    .iter()
                    .map(|e| e.name.clone())
                    .collect::<Vec<_>>();
                assert!(
                    names.iter().position(|n| n == "call:cancelOrders").unwrap()
                        < names.iter().position(|n| n == "call:placeOrder").unwrap(),
                    "REST native cancel before place"
                );
            }
        }
        "algo" | "inverse-algo" => {
            if h.scene == "missing-market" {
                let e = r.as_ref().expect_err("REST native missing market");
                native(
                    &get(&e.value, "message"),
                    &s("Market BTCUSD not found"),
                    "missing market Error message",
                );
                count(h, "call:unlimitedXHR.post", 0);
                count(h, "call:emitAndLogError", 0);
                return;
            }
            let mut fields = vec![
                ("orderId", s(if inv { "old__stop_loss" } else { "old" })),
                ("marginCoin", s(if inv { "BTC" } else { "USDT" })),
                ("symbol", s("BTC_NATIVE")),
                ("productType", s("dmcbl")),
            ];
            if h.scene != "zero-amount" {
                fields.push(("triggerPrice", s("11.5")))
            }
            fields.push((
                "size",
                s(if inv || h.scene == "position-level" {
                    ""
                } else if matches!(h.scene, "zero-amount" | "no-amount") {
                    "2.2"
                } else {
                    "3.4"
                }),
            ));
            native(
                &ev(h, "call:unlimitedXHR.post")[0].args[0],
                &s("UPDATE_ALGO_ORDER"),
                "algo endpoint",
            );
            native(&payload(h, "unlimitedXHR.post"), &o(fields), "algo payload");
            expect_result(
                r,
                if h.scene == "reject" {
                    a(vec![])
                } else {
                    a(vec![s("updated")])
                },
            );
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(!inv && h.scene != "reject"),
            );
            count(h, "call:getMarginCoinForSymbol", usize::from(inv));
            count(h, "call:emitAndLogError", usize::from(h.scene == "reject"));
            if h.scene == "reject" {
                native(
                    &a(ev(h, "call:emitAndLogError")[0].args.clone()),
                    &a(vec![s("api original")]),
                    "algo emitted message",
                )
            }
        }
        "leverage" | "inverse-leverage" => {
            if h.scene == "missing-market" {
                let e = r.as_ref().expect_err("REST native missing market");
                native(
                    &get(&e.value, "message"),
                    &s("Market BTCUSD not found"),
                    "leverage missing market",
                );
                count(h, "call:xhr.post", 0);
                count(h, "call:emitAndLogError", 0);
                return;
            }
            count(h, "call:getMarginCoinForSymbol", usize::from(inv));
            if h.scene == "unchanged" {
                expect_result(r, V::U);
                count(h, "call:xhr.post", 0);
                count(h, "call:ensureAccountTypeDetected", 0);
                return;
            }
            let per = inv || h.scene != "cross";
            let v3 = !inv && h.scene == "uta";
            let direction = if h.scene == "fallback-side" {
                "long"
            } else {
                "short"
            };
            let expected = if v3 {
                o(vec![
                    ("symbol", s("BTC_NATIVE")),
                    ("coin", s("USDT")),
                    ("category", s("DMCBL")),
                    ("leverage", s("20")),
                    ("posSide", s(direction)),
                ])
            } else {
                let mut fields = vec![
                    ("symbol", s("BTC_NATIVE")),
                    ("marginCoin", s(if inv { "BTC" } else { "USDT" })),
                    ("productType", s("dmcbl")),
                    ("leverage", V::N(20.)),
                ];
                if per {
                    fields.push(("holdSide", s(direction)))
                }
                o(fields)
            };
            native(&payload(h, "xhr.post"), &expected, "leverage payload");
            native(
                &ev(h, "call:xhr.post")[0].args[0],
                &s(if v3 {
                    "SET_LEVERAGE_V3"
                } else {
                    "SET_LEVERAGE"
                }),
                "leverage endpoint",
            );
            count(h, "call:ensureAccountTypeDetected", usize::from(!inv));
            if matches!(h.scene, "reject" | "api-code") {
                if h.scene == "reject" {
                    original_error(h, r)
                } else {
                    let e = r.as_ref().expect_err("REST API failure");
                    native(
                        &get(&e.value, "message"),
                        &s("api failure"),
                        "native API Error message",
                    )
                }
                count(h, "call:fetchPositions", 0);
                let e = ev(h, "call:emitAndLogError");
                assert_eq!(e.len(), 1, "REST native leverage emit count");
                assert_eq!(
                    e[0].args.len(),
                    2,
                    "REST native leverage emit argument count"
                );
                let error = r.as_ref().unwrap_err();
                assert!(
                    equal(&e[0].args[0], &error.value),
                    "REST native leverage emitted Error alias"
                );
                native(
                    &e[0].args[1],
                    &s(if h.scene == "reject" {
                        "api original"
                    } else {
                        "api failure"
                    }),
                    "leverage emitted message",
                );
            } else {
                expect_result(r, V::U);
                count(h, "call:emitAndLogError", 0);
                refreshed(h, false)
            }
        }
        "margin" | "inverse-margin" => {
            if h.scene == "missing-symbol" {
                let e = r.as_ref().expect_err("REST native missing symbol");
                native(
                    &get(&e.value, "message"),
                    &s("Symbol is required"),
                    "missing symbol Error",
                );
                native(
                    &a(ev(h, "call:emitAndLogError")[0].args.clone()),
                    &a(vec![s("Symbol is required")]),
                    "missing symbol emit",
                );
                count(h, "call:xhr.post", 0);
                return;
            }
            let v3 = !inv && h.scene == "uta";
            native(
                &payload(h, "xhr.post"),
                &if v3 {
                    o(vec![
                        ("category", s("inverse")),
                        ("symbol", s("BTC_NATIVE")),
                        ("leverage", s("1")),
                        ("marginMode", s("isolated")),
                    ])
                } else {
                    o(vec![
                        ("productType", s("dmcbl")),
                        ("marginCoin", s(if inv { "BTC" } else { "USDT" })),
                        ("marginMode", s("isolated")),
                        ("symbol", s("BTCUSD")),
                    ])
                },
                "margin payload",
            );
            native(
                &ev(h, "call:xhr.post")[0].args[0],
                &s(if v3 {
                    "SET_LEVERAGE_V3"
                } else {
                    "SET_MARGIN_MODE"
                }),
                "margin endpoint",
            );
            count(h, "call:ensureAccountTypeDetected", usize::from(!inv));
            count(h, "call:getMarginCoinForSymbol", usize::from(inv));
            count(h, "call:fetchUtaPositionMetadata", usize::from(v3));
            let cleared = !inv && !matches!(h.scene, "reject" | "api-code");
            for (path, old, name) in [
                ("cache", &h.cache, "_positioningCache"),
                ("inflight", &h.inflight, "_positioningInflight"),
            ] {
                let v = get(&h.state, path);
                assert!(
                    equal(&v, old),
                    "REST native per-symbol cache retains original object"
                );
                native(
                    &v,
                    &if cleared {
                        o(vec![(
                            "KEEP",
                            s(if path == "cache" {
                                "keep"
                            } else {
                                "keep-flight"
                            }),
                        )])
                    } else {
                        o(vec![
                            (
                                "BTCUSD",
                                s(if path == "cache" { "cached" } else { "flight" }),
                            ),
                            (
                                "KEEP",
                                s(if path == "cache" {
                                    "keep"
                                } else {
                                    "keep-flight"
                                }),
                            ),
                        ])
                    },
                    "per-symbol cache",
                );
                count(h, &format!("delete:{name}"), usize::from(cleared));
            }
            if matches!(h.scene, "reject" | "api-code" | "fetch-reject") {
                if h.scene != "api-code" {
                    original_error(h, r)
                } else {
                    native(
                        &get(&r.as_ref().unwrap_err().value, "message"),
                        &s("api failure"),
                        "margin API Error",
                    )
                };
                count(h, "call:emitAndLogError", 1);
                native(
                    &a(ev(h, "call:emitAndLogError")[0].args.clone()),
                    &a(vec![s(if h.scene == "api-code" {
                        "api failure"
                    } else {
                        "api original"
                    })]),
                    "margin emitted message",
                );
                count(h, "call:store.addOrUpdatePositions", 0)
            } else {
                expect_result(r, V::U);
                count(h, "call:emitAndLogError", 0);
                refreshed(h, false)
            }
        }
        "mode" => {
            let uta = h.scene == "uta";
            native(
                &payload(h, "xhr.post"),
                &if uta {
                    o(vec![("holdMode", s("hedge_mode"))])
                } else {
                    o(vec![
                        ("productType", s("dmcbl")),
                        (
                            "posMode",
                            s(if h.scene == "one-way" {
                                "one_way_mode"
                            } else {
                                "hedge_mode"
                            }),
                        ),
                    ])
                },
                "position-mode payload",
            );
            native(
                &ev(h, "call:xhr.post")[0].args[0],
                &s(if uta {
                    "UTA_SET_HOLD_MODE"
                } else {
                    "SET_POSITION_MODE"
                }),
                "position-mode endpoint",
            );
            count(h, "call:ensureAccountTypeDetected", 1);
            let cleared = !matches!(h.scene, "reject" | "api-code");
            for (path, old, name) in [
                ("cache", &h.cache, "_positioningCache"),
                ("inflight", &h.inflight, "_positioningInflight"),
            ] {
                let v = get(&h.state, path);
                count(h, &format!("set:{name}"), usize::from(cleared));
                if cleared {
                    native(&v, &o(vec![]), "fresh global empty cache");
                    assert!(!equal(&v, old), "REST native global cache replaced")
                } else {
                    assert!(
                        equal(&v, old),
                        "REST native global cache unchanged on failed request"
                    )
                }
            }
            if cleared {
                assert!(
                    !equal(&get(&h.state, "cache"), &get(&h.state, "inflight")),
                    "REST native cache and inflight are distinct fresh objects"
                )
            }
            if matches!(h.scene, "reject" | "api-code" | "fetch-reject") {
                if h.scene != "api-code" {
                    original_error(h, r)
                }
                let e = ev(h, "call:emitAndLogError");
                assert_eq!(e.len(), 1, "REST native mode emit count");
                assert_eq!(e[0].args.len(), 2, "REST native mode emit argument count");
                assert!(
                    equal(&e[0].args[0], &r.as_ref().unwrap_err().value),
                    "REST native mode original Error alias"
                );
                count(h, "call:store.update", 0)
            } else {
                expect_result(r, V::U);
                count(h, "call:emitAndLogError", 0);
                refreshed(h, true)
            }
        }
        _ => unreachable!(),
    }
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut c = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/rest-actions-capture.mjs"),
        )
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let r = c.wait_with_output().unwrap();
    assert!(
        r.status.success(),
        "REST source oracle {}",
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
fn suite(ops: &[&'static str], scenes: &[&'static str], mode: &str, floor: usize) {
    assert_eq!(
        ops.len() * scenes.len(),
        floor,
        "REST independent fixture recipe count"
    );
    let mut fixtures = vec![];
    for &op in ops {
        for &scene in scenes {
            let h = make(op, scene);
            let input = inputs(&h);
            let r = execute(&h);
            check(&h, &r);
            fixtures.push((h, r, input))
        }
    } // Every native assertion finishes before any source/wire comparison.
    let inputs = fixtures
        .iter()
        .map(|(_, _, i)| i.clone())
        .collect::<Vec<_>>();
    let outputs = source(&inputs, mode);
    assert_eq!(
        outputs.len(),
        floor,
        "REST independent actual source result count"
    );
    for (i, ((h, r, _), out)) in fixtures.iter().zip(outputs).enumerate() {
        compare(
            &output(h, r),
            &out,
            &format!("{mode}:{i}:{}:{}", h.op, h.scene),
        )
    }
}
#[test]
fn native_amend_then_source() {
    suite(
        &["amend"],
        &[
            "normal",
            "price-only",
            "amount-only",
            "invalid",
            "missing-market",
            "virtual-child",
            "api-code",
            "reject",
            "adjust-throw",
            "schedule-throw",
        ],
        "amend",
        10,
    )
}
#[test]
fn native_update_route_then_source() {
    suite(
        &["route"],
        &[
            "normal",
            "empty-amend",
            "fallback",
            "legacy",
            "uta",
            "algo",
            "uta-algo",
            "virtual",
            "explicit-undefined",
        ],
        "route",
        9,
    )
}
#[test]
fn native_algo_updates_then_source() {
    suite(
        &["algo", "inverse-algo"],
        &[
            "normal",
            "no-amount",
            "zero-amount",
            "position-level",
            "reject",
            "missing-market",
        ],
        "algo",
        12,
    )
}
#[test]
fn native_leverage_then_source() {
    suite(
        &["leverage", "inverse-leverage"],
        &[
            "normal",
            "uta",
            "cross",
            "unchanged",
            "fallback-side",
            "missing-market",
            "reject",
            "api-code",
        ],
        "leverage",
        16,
    )
}
#[test]
fn native_margin_then_source() {
    suite(
        &["margin", "inverse-margin"],
        &[
            "normal",
            "uta",
            "missing-symbol",
            "api-code",
            "reject",
            "fetch-reject",
        ],
        "margin",
        12,
    )
}
#[test]
fn native_position_mode_then_source() {
    suite(
        &["mode"],
        &[
            "normal",
            "uta",
            "one-way",
            "reject",
            "api-code",
            "fetch-reject",
        ],
        "mode",
        6,
    )
}
