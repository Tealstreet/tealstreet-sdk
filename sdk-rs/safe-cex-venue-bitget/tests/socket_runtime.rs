use safe_cex_venue_bitget::socket_runtime::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
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
        _ => panic!("SOCKET native array required"),
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
            "SOCKET native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "SOCKET native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "SOCKET native independent array count {path}"
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
                "SOCKET native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "SOCKET native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "SOCKET native scalar {path}: {actual:?} != {expected:?}"
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
fn stringify(v: &V) -> String {
    match v {
        V::Hole | V::U | V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        V::S(_) => serde_json::to_string(&text(v)).unwrap(),
        V::A(v) => format!(
            "[{}]",
            v.borrow()
                .iter()
                .map(stringify)
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::O(_) => format!(
            "{{{}}}",
            keys(v)
                .into_iter()
                .filter_map(|k| {
                    let field = key(v, &k);
                    if matches!(field, V::U) {
                        None
                    } else {
                        Some(format!(
                            "{}:{}",
                            serde_json::to_string(&String::from_utf16_lossy(&k)).unwrap(),
                            stringify(&field)
                        ))
                    }
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
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
            assert_eq!(a.len(), b.len(), "SOCKET source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "SOCKET source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("SOCKET source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "SOCKET source value {path}"),
    }
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
struct Iter {
    value: V,
    index: usize,
}
struct H {
    op: &'static str,
    scene: &'static str,
    data: V,
    state: V,
    current: V,
    fetched: V,
    positions: V,
    balance: V,
    loaded: V,
    tombs: V,
    recent: V,
    handlers: V,
    callbacks: V,
    events: RefCell<Vec<Event>>,
    payloads: RefCell<Vec<V>>,
    parsed: RefCell<Vec<V>>,
    invoked: RefCell<Vec<V>>,
    original: Rc<()>,
    post: Cell<bool>,
    dates: Cell<usize>,
    signer_reads: Cell<usize>,
    error_reads: Cell<usize>,
}
const SIGN: &str = "ntCOgO3bBOD6SxGsLfbQ98zohwl0Xh+f/HLJyVTbGwQ=";
fn row(id: &str, status: &str, kind: &str) -> V {
    o(vec![
        ("id", s(id)),
        ("status", s(status)),
        ("type", s(kind)),
    ])
}
fn from_json(v: Json) -> V {
    match v {
        Json::Null => V::Null,
        Json::Bool(b) => V::Bool(b),
        Json::Number(n) => V::N(n.as_f64().unwrap()),
        Json::String(t) => s(&t),
        Json::Array(a) => V::A(Rc::new(RefCell::new(
            a.into_iter().map(from_json).collect(),
        ))),
        Json::Object(o) => V::O(Rc::new(RefCell::new(
            o.into_iter()
                .map(|(k, v)| (k.encode_utf16().collect(), from_json(v)))
                .collect(),
        ))),
    }
}
fn make(op: &'static str, scene: &'static str) -> H {
    let current = a(vec![
        row("r1", "canceling", "limit"),
        row("r2", "canceled", "limit"),
        row("r4", "open", "limit"),
        row("sl", "open", "stop_market"),
        row("tp", "open", "take_profit_market"),
        row("trail", "open", "trailing_stop_market"),
        row("fresh", "open", "limit"),
        row("expired", "open", "limit"),
        row("canceled", "canceled", "stop_market"),
    ]);
    let fetched = a(vec![
        row("r0", "open", "limit"),
        row("r1", "open", "limit"),
        row("r2", "closed", "limit"),
        row("r3", "open", "limit"),
        row("r4", "open", "limit"),
        V::Hole,
    ]);
    let data = match scene {
        "pong" => "pong",
        "invalid" => "bad JSON",
        "login" => r#"{"event":"login","code":0}"#,
        "login-error" => r#"{"event":"login","code":400,"msg":"bad"}"#,
        "uta-order" => r#"{"arg":{"topic":"order","channel":"orders"},"data":[]}"#,
        "uta-position" => r#"{"arg":{"topic":"position"},"data":[]}"#,
        "uta-account" => r#"{"arg":{"topic":"account"},"data":[]}"#,
        "orders" | "disposed" => r#"{"arg":{"channel":"ORDERS"},"data":[]}"#,
        "algo" => r#"{"arg":{"channel":"Orders-Algo"},"data":[]}"#,
        "positions" => r#"{"arg":{"channel":"positions"},"data":[]}"#,
        "account" => r#"{"arg":{"channel":"account"},"data":[]}"#,
        "ack" => r#"{"arg":{"channel":"orders"},"event":"subscribe"}"#,
        "ticker" => r#"{"arg":{"channel":"ticker"},"data":[{"id":"x"}]}"#,
        "ticker-ack" => r#"{"arg":{"channel":"ticker"},"event":"subscribe"}"#,
        "candle" | "live" => {
            r#"{"action":"update","arg":{"channel":"candle1m","instId":"BTC"},"data":[]}"#
        }
        "books" => r#"{"arg":{"channel":"books","instId":"BTC"},"data":[]}"#,
        "both" => {
            r#"{"action":"update","arg":{"channel":"candle1m","instId":"BTC"},"extra":{"channel":"books"}}"#
        }
        "spaced" => r#"{"arg":{"channel": "ticker"}}"#,
        _ => "",
    };
    let callbacks = a(vec![o(vec![("id", s("cb1"))]), o(vec![("id", s("cb2"))])]);
    H {
        op,
        scene,
        data: s(data),
        state: o(vec![
            ("isDisposed", V::Bool(scene == "disposed")),
            ("messageCount", V::N(0.)),
            ("lastMessageCount", V::N(0.)),
            (
                "connectionAttempts",
                V::N(
                    if matches!(scene, "fallback" | "no-fallback" | "emit-error" | "uta") {
                        1.
                    } else {
                        0.
                    },
                ),
            ),
            ("MAX_ATTEMPTS", V::N(2.)),
            ("useFallbackUrl", V::Bool(false)),
            ("ws", s("socket")),
            ("ordersFetchRequestSeq", V::N(1.)),
            ("lastPositionFetchTime", V::N(0.)),
            ("lastBalanceFetchTime", V::N(0.)),
            ("lastOrdersFetchTime", V::N(0.)),
            ("hasPendingPositionEvents", V::Bool(true)),
            ("hasPendingBalanceEvents", V::Bool(true)),
            ("hasPendingOrderEvents", V::Bool(true)),
        ]),
        current,
        fetched,
        positions: a(vec![o(vec![("id", s("p"))])]),
        balance: o(vec![("total", V::N(90.))]),
        loaded: o(vec![
            ("balance", V::Bool(false)),
            ("orders", V::Bool(false)),
            ("other", s("keep")),
        ]),
        tombs: o(vec![("r0", V::N(5000.))]),
        recent: o(vec![("fresh", V::N(5000.)), ("expired", V::N(2000.))]),
        handlers: o(vec![
            ("candle_BTC_1m", callbacks.clone()),
            ("orderBook_BTC", callbacks.clone()),
        ]),
        callbacks,
        events: RefCell::new(vec![]),
        payloads: RefCell::new(vec![]),
        parsed: RefCell::new(vec![]),
        invoked: RefCell::new(vec![]),
        original: Rc::new(()),
        post: Cell::new(false),
        dates: Cell::new(0),
        signer_reads: Cell::new(0),
        error_reads: Cell::new(0),
    }
}
fn set(v: &V, n: &str, f: V) {
    put(v, n.encode_utf16().collect(), f)
}
fn method(n: &str, owner: V) -> V {
    o(vec![("method", s(n)), ("owner", owner)])
}
impl H {
    fn rec(&self, n: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: n.into(),
            args,
        })
    }
    fn err(&self) -> E {
        E {
            identity: self.original.clone(),
        }
    }
    fn result(&self) -> V {
        o(vec![
            (
                "error",
                if matches!(self.scene, "error" | "error-getter") {
                    s("first")
                } else {
                    V::U
                },
            ),
            ("wsAuthSignature", s("remote-sign")),
        ])
    }
}
impl SocketRuntimeHost for H {
    type Value = V;
    type Error = E;
    type Iterator = Iter;
    fn receiver(&self, n: &str) -> Result<V, E> {
        if n == "parent.options.remoteSigner.accountName" {
            let signer = self.receiver("parent.options.remoteSigner")?;
            self.rec(&format!("get:{n}"), vec![]);
            return Ok(get(&signer, "accountName"));
        }
        self.rec(&format!("get:{n}"), vec![]);
        Ok(match n {
            "isDisposed" => {
                V::Bool(self.scene == "disposed" || self.scene == "stop" && self.post.get())
            }
            "parent.usesUtaRouting" => V::Bool(self.scene == "uta"),
            "parent.name" => s("bitget"),
            "parent.options" => o(vec![]),
            "parent.apiProductType" => s("usdt-futures"),
            "parent.options.key" => s("key"),
            "parent.options.passphrase" => s("pass"),
            "parent.options.secret" => s("secret"),
            "parent.options.remoteSigner" => {
                let i = self.signer_reads.get();
                self.signer_reads.set(i + 1);
                if self.scene == "local" {
                    V::U
                } else {
                    o(vec![(
                        "accountName",
                        s(if self.scene == "rotate" && i == 2 {
                            "rotated"
                        } else {
                            "alice"
                        }),
                    )])
                }
            }
            "parent.options.remoteSigner.accountName" => {
                let signer = self.receiver("parent.options.remoteSigner")?;
                get(&signer, "accountName")
            }
            "onOpen" | "onMessage" | "onClose" => o(vec![("id", s(n))]),
            "store.orders" => self.current.clone(),
            "store.loaded" => self.loaded.clone(),
            "terminalOrderTombstones" => self.tombs.clone(),
            "recentWsOpenOrders" => self.recent.clone(),
            "messageHandlers" => self.handlers.clone(),
            _ => get(&self.state, n),
        })
    }
    fn set_receiver(&self, n: &str, v: V) -> Result<(), E> {
        self.rec(&format!("set:{n}"), vec![v.clone()]);
        if self.scene == "prefix-error" && n.starts_with("last") {
            return Err(self.err());
        }
        set(&self.state, n, v);
        Ok(())
    }
    fn capture(&self, n: &str) -> Result<V, E> {
        if let Some(map) = n.strip_suffix(".get") {
            if matches!(map, "terminalOrderTombstones" | "recentWsOpenOrders") {
                return Ok(method("get", self.receiver(map)?));
            }
        }
        if n == "ws.addEventListener" {
            self.receiver("ws")?;
        }
        if n == "parent.options.remoteSigner.sign" {
            let owner = self.receiver("parent.options.remoteSigner")?;
            self.rec(&format!("capture:{n}"), vec![]);
            return Ok(method(n, owner));
        }
        self.rec(&format!("capture:{n}"), vec![]);
        Ok(method(n, V::U))
    }
    fn capture_member(&self, v: &V, n: &str) -> Result<V, E> {
        if matches!(n, "update" | "digest") {
            self.rec(&format!("capture:crypto.{n}"), vec![]);
        }
        Ok(method(n, v.clone()))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        let name = text(&get(&f, "method"));
        let owner = get(&f, "owner");
        Ok(match name.as_str() {
            "includes" => {
                let V::S(hay) = owner else { panic!() };
                let V::S(needle) = &args[0] else { panic!() };
                V::Bool(hay.windows(needle.len()).any(|v| v == needle))
            }
            "replace" => s(&text(&owner).replacen(&text(&args[0]), &text(&args[1]), 1)),
            "get" => {
                let V::S(k) = &args[0] else { panic!() };
                key(&owner, k)
            }
            "has" => V::Bool(values(&owner).iter().any(|v| equal(v, &args[0]))),
            "update" => {
                self.rec("call:crypto.update", args.clone());
                set(&owner, "payload", args[0].clone());
                owner
            }
            "digest" => {
                self.rec("call:crypto.digest", args);
                s(&safe_cex_crypto::hmac_sha256(
                    text(&get(&owner, "secret")).as_bytes(),
                    text(&get(&owner, "payload")).as_bytes(),
                    safe_cex_crypto::Encoding::Base64,
                ))
            }
            _ => {
                self.rec(&format!("call:{name}"), args.clone());
                if self.scene == "emit-error" && name == "parent.emitter.emit" {
                    return Err(self.err());
                }
                match name.as_str() {
                    "createWebsocket" => s("new-socket"),
                    "normalizeChannel" => s(&if matches!(args[0], V::U | V::Null) {
                        "".into()
                    } else {
                        text(&args[0])
                    }
                    .to_lowercase()
                    .replace(['-', '_'], "")),
                    "mergeRestPositionsWithFreshWsState" => args[0].clone(),
                    "pruneOrderLifecycleCaches" => {
                        let now = if let V::N(n) = args[0] { n } else { panic!() };
                        let V::O(map) = &self.recent else { panic!() };
                        map.borrow_mut()
                            .retain(|(_, v)| !matches!(v,V::N(n)if*n<=now));
                        V::U
                    }
                    _ => V::U,
                }
            }
        })
    }
    fn invoke_bare(&self, cb: V, args: Vec<V>) -> Result<(), E> {
        self.invoked.borrow_mut().push(cb.clone());
        let id = text(&get(&cb, "id"));
        self.rec(&format!("callback:{id}"), args);
        if self.scene == "live" && id == "cb1" {
            let V::A(cbs) = &self.callbacks else { panic!() };
            cbs.borrow_mut().push(o(vec![("id", s("cb3"))]));
        }
        Ok(())
    }
    fn start(&self, f: V, args: Vec<V>) -> Result<SocketFuture<'_, V, E>, E> {
        let name = text(&get(&f, "method"));
        self.rec(&format!("call:{name}"), args);
        Ok(Box::pin(async move {
            if self.scene == "reject" {
                return Err(self.err());
            }
            self.post.set(true);
            if self.scene == "stale" {
                set(&self.state, "ordersFetchRequestSeq", V::N(99.))
            }
            Ok(match name.as_str() {
                "parent.fetchPositions" => self.positions.clone(),
                "parent.fetchBalance" => self.balance.clone(),
                "parent.fetchOrders" => self.fetched.clone(),
                "parent.options.remoteSigner.sign" => self.result(),
                _ => panic!("start {name}"),
            })
        }))
    }
    fn property(&self, v: &V, n: &str) -> Result<V, E> {
        if n == "error" && self.scene == "error-getter" {
            self.rec("probe:error", vec![]);
            let i = self.error_reads.get();
            self.error_reads.set(i + 1);
            return Ok(s(if i == 0 { "first" } else { "second" }));
        }
        Ok(get(v, n))
    }
    fn key_get(&self, v: &V, k: &V) -> Result<V, E> {
        let V::S(k) = k else { panic!() };
        Ok(key(v, k))
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
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
    fn string(&self, u: &[u16]) -> V {
        V::S(u.to_vec())
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        let V::O(v) = v else { panic!() };
        Ok(V::O(Rc::new(RefCell::new(v.borrow().clone()))))
    }
    fn define(&self, v: &V, k: &str, f: V) -> Result<(), E> {
        set(v, k, f);
        Ok(())
    }
    fn iterator(&self, v: &V) -> Result<Iter, E> {
        Ok(Iter {
            value: v.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        let V::A(v) = &i.value else { panic!() };
        let v = v.borrow();
        if i.index >= v.len() {
            return Ok(None);
        }
        let item = v[i.index].clone();
        i.index += 1;
        Ok(Some(if matches!(item, V::Hole) { V::U } else { item }))
    }
    fn filter<F>(&self, v: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        let len = values(&v).len();
        let mut out = vec![];
        for i in 0..len {
            let item = values(&v)[i].clone();
            if !matches!(item, V::Hole) && f(item.clone(), i)? {
                out.push(item)
            }
        }
        Ok(a(out))
    }
    fn map<F>(&self, v: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        let len = values(&v).len();
        let mut out = vec![];
        for i in 0..len {
            let item = values(&v)[i].clone();
            out.push(if matches!(item, V::Hole) {
                V::Hole
            } else {
                f(item, i)?
            })
        }
        Ok(a(out))
    }
    fn operation(&self, n: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match n {
            "increment" => {
                let V::N(n) = args[0] else { panic!() };
                V::N(n + 1.)
            }
            "greater" | "greaterEqual" => {
                let V::N(a) = args[0] else { panic!() };
                let V::N(b) = args[1] else { panic!() };
                V::Bool(if n == "greater" { a > b } else { a >= b })
            }
            "template" => {
                let mut out = vec![];
                for v in args {
                    if let V::S(u) = v {
                        out.extend(u)
                    } else {
                        out.extend(
                            (if let V::N(n) = v {
                                safe_cex_num::js_number_string(n)
                            } else {
                                text(&v)
                            })
                            .encode_utf16(),
                        )
                    }
                }
                V::S(out)
            }
            "toUpperCase" => s(&text(&args[0]).to_uppercase()),
            "JSON.stringify" => {
                self.payloads.borrow_mut().push(args[0].clone());
                s(&stringify(&args[0]))
            }
            "jsonParse" => {
                self.rec("jsonParse", args.clone());
                let parsed = serde_json::from_str(&text(&args[0]))
                    .map(from_json)
                    .unwrap_or(V::Null);
                self.parsed.borrow_mut().push(parsed.clone());
                parsed
            }
            "Date.now" => {
                self.rec("Date.now", vec![]);
                let n = self.dates.get();
                self.dates.set(n + 1);
                V::N(if n == 0 { 1000. } else { 3000. })
            }
            "clock.ms" => {
                self.rec("clock.ms", vec![]);
                V::N(2000.)
            }
            "clock.unix" => {
                self.rec("clock.unix", vec![]);
                V::N(1234.)
            }
            "nanoid" => {
                self.rec("nanoid", vec![]);
                s("fixture-id")
            }
            "createHmac" => {
                self.rec("crypto.create", args.clone());
                o(vec![("secret", args[1].clone()), ("payload", V::U)])
            }
            "resolveTransportRouting" => {
                self.rec("routing", args);
                o(vec![(
                    "wsPrivate",
                    o(vec![(
                        "fallback",
                        if self.scene == "no-fallback" {
                            V::U
                        } else {
                            s("wss://fallback")
                        },
                    )]),
                )])
            }
            "BASE_WS_PRIVATE_URL" => s("wss://ws.bitget.com/v2/ws/private"),
            "BASE_WS_PRIVATE_UTA_URL" => s("wss://ws.bitget.com/v3/ws/private"),
            "newMap" => {
                let out = o(vec![]);
                for entry in values(&args[0]) {
                    let e = values(&entry);
                    let V::S(k) = &e[0] else { panic!() };
                    put(&out, k.clone(), e[1].clone())
                }
                out
            }
            "newSet" => {
                let mut out = vec![];
                for v in values(&args[0]) {
                    if !out.iter().any(|a| equal(a, &v)) {
                        out.push(v)
                    }
                }
                a(out)
            }
            "OrderStatus.Open" => s("open"),
            "OrderStatus.Canceled" => s("canceled"),
            "OrderStatus.Closed" => s("closed"),
            "OrderStatus.Canceling" => s("canceling"),
            "OrderType.StopLoss" => s("stop_market"),
            "OrderType.TakeProfit" => s("take_profit_market"),
            "OrderType.TrailingStopLoss" => s("trailing_stop_market"),
            _ => panic!("op {n}"),
        })
    }
}
fn execute(h: &H) -> Result<V, E> {
    match h.op {
        "connect" => body::connect_and_subscribe(h),
        "private" => body::on_private_message(h, h.data.clone()),
        "public" => body::on_public_message(h, h.data.clone()),
        "positions" => run(body::execute_position_fetch(h)),
        "balance" => run(body::execute_balance_fetch(h)),
        "orders" => run(body::execute_orders_fetch(h)),
        "auth" => run(body::auth(h)),
        "subscribe" => body::subscribe(h),
        _ => panic!(),
    }
    .map(|()| V::U)
}
fn inputs(h: &H) -> Json {
    json!({"op":h.op,"scene":h.scene,"data":wire(&h.data),"state":wire(&h.state),"current":wire(&h.current),"fetched":wire(&h.fetched),"positions":wire(&h.positions),"balance":wire(&h.balance),"loaded":wire(&h.loaded),"tombs":wire(&h.tombs),"recent":wire(&h.recent)})
}
fn output(h: &H, r: &Result<V, E>) -> Json {
    json!({"result":match r{Ok(v)=>json!({"ok":wire(v)}),Err(e)=>json!({"original":Rc::ptr_eq(&e.identity,&h.original)})},"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"state":wire(&h.state)})
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut c = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/socket-runtime-capture.mjs"),
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
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "SOCKET source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn event(h: &H, name: &str) -> Vec<Event> {
    h.events
        .borrow()
        .iter()
        .filter(|e| e.name == name)
        .cloned()
        .collect()
}
fn check(h: &H, r: &Result<V, E>) {
    let rejects = h.op == "auth" && h.scene == "reject";
    assert_eq!(
        r.is_err(),
        rejects,
        "SOCKET native disposition {}/{}",
        h.op,
        h.scene
    );
    if let Err(e) = r {
        assert!(
            Rc::ptr_eq(&e.identity, &h.original),
            "SOCKET native original error identity"
        )
    } else {
        native(r.as_ref().unwrap(), &V::U, "return")
    }
    match h.op {
        "connect" => {
            let active = h.scene != "disposed";
            assert_eq!(
                event(h, "call:createWebsocket").len(),
                usize::from(active),
                "SOCKET independent connection count"
            );
            if active {
                native(
                    &get(&h.state, "connectionAttempts"),
                    &V::N(
                        if matches!(h.scene, "fallback" | "no-fallback" | "emit-error" | "uta") {
                            2.
                        } else {
                            1.
                        },
                    ),
                    "connection attempts",
                );
                native(
                    &get(&h.state, "useFallbackUrl"),
                    &V::Bool(matches!(
                        h.scene,
                        "fallback" | "no-fallback" | "emit-error" | "uta"
                    )),
                    "fallback flag",
                );
                native(
                    &event(h, "call:createWebsocket")[0].args[0],
                    &s(if h.scene == "uta" {
                        "wss://ws.bitget.com/v3/ws/private"
                    } else if matches!(h.scene, "fallback" | "emit-error") {
                        "wss://fallback"
                    } else {
                        "wss://ws.bitget.com/v2/ws/private"
                    }),
                    "connection URL",
                );
                let listeners = event(h, "call:ws.addEventListener");
                assert_eq!(listeners.len(), 3, "SOCKET independent listener count");
                for (e, (k, handler)) in listeners.iter().zip([
                    ("open", "onOpen"),
                    ("message", "onMessage"),
                    ("close", "onClose"),
                ]) {
                    native(
                        &a(e.args.clone()),
                        &a(vec![s(k), o(vec![("id", s(handler))])]),
                        "listener selection",
                    )
                }
            }
        }
        "subscribe" => {
            assert_eq!(
                h.payloads.borrow().len(),
                1,
                "SOCKET independent subscription payload count"
            );
            let args = if h.scene == "uta" {
                ["account", "position", "order"]
                    .into_iter()
                    .map(|t| o(vec![("instType", s("UTA")), ("topic", s(t))]))
                    .collect()
            } else {
                [
                    ("account", "coin"),
                    ("positions", "instId"),
                    ("orders", "instId"),
                    ("orders-algo", "instId"),
                ]
                .into_iter()
                .map(|(ch, k)| {
                    o(vec![
                        ("instType", s("USDT-FUTURES")),
                        ("channel", s(ch)),
                        (k, s("default")),
                    ])
                })
                .collect()
            };
            native(
                &h.payloads.borrow()[0],
                &o(vec![("op", s("subscribe")), ("args", a(args))]),
                "subscription payload",
            );
        }
        "auth" => {
            if h.scene == "local" {
                for (name, expected) in [
                    ("crypto.create", a(vec![s("sha256"), s("secret")])),
                    ("call:crypto.update", a(vec![s("1234GET/user/verify")])),
                    ("call:crypto.digest", a(vec![s("base64")])),
                ] {
                    let calls = event(h, name);
                    assert_eq!(
                        calls.len(),
                        1,
                        "SOCKET independent local crypto operation count {name}"
                    );
                    native(
                        &a(calls[0].args.clone()),
                        &expected,
                        &format!("local crypto native arguments {name}"),
                    );
                }
            }
            let sends = usize::from(!matches!(h.scene, "reject" | "error" | "error-getter"));
            assert_eq!(
                h.payloads.borrow().len(),
                sends,
                "SOCKET independent auth payload count"
            );
            if sends > 0 {
                native(
                    &h.payloads.borrow()[0],
                    &o(vec![
                        ("op", s("login")),
                        (
                            "args",
                            a(vec![o(vec![
                                ("apiKey", s("key")),
                                ("passphrase", s("pass")),
                                ("timestamp", V::N(1234.)),
                                (
                                    "sign",
                                    s(if h.scene == "local" {
                                        SIGN
                                    } else {
                                        "remote-sign"
                                    }),
                                ),
                            ])]),
                        ),
                    ]),
                    "auth payload",
                )
            }
            let remote = event(h, "call:parent.options.remoteSigner.sign");
            assert_eq!(
                remote.len(),
                usize::from(h.scene != "local"),
                "SOCKET independent remote signer count"
            );
            if h.scene != "local" {
                native(
                    &remote[0].args[0],
                    &o(vec![
                        ("id", s("fixture-id")),
                        ("type", s("ws-auth")),
                        ("exchangeName", s("bitget")),
                        (
                            "accountName",
                            s(if h.scene == "rotate" {
                                "rotated"
                            } else {
                                "alice"
                            }),
                        ),
                        (
                            "wsAuth",
                            o(vec![
                                ("toSign", s("1234GET/user/verify")),
                                ("outputFormat", s("base64")),
                                ("extraData", o(vec![("timestamp", V::N(1234.))])),
                            ]),
                        ),
                    ]),
                    "remote request",
                );
                assert_eq!(
                    h.signer_reads.get(),
                    3,
                    "SOCKET native repeated signer getter count"
                )
            }
            if matches!(h.scene, "error" | "error-getter") {
                native(
                    &event(h, "call:emitAndLogError")[0].args[0],
                    &s(if h.scene == "error" {
                        "Remote WS signing unavailable: first"
                    } else {
                        "Remote WS signing unavailable: second"
                    }),
                    "remote unavailable message",
                );
            }
            if h.scene == "error-getter" {
                assert_eq!(
                    h.error_reads.get(),
                    2,
                    "SOCKET native repeated error getter count"
                )
            }
        }
        "positions" | "balance" | "orders" => {
            let prefix = h.scene == "prefix-error";
            let suffix = match h.op {
                "positions" => "Position",
                "balance" => "Balance",
                _ => "Orders",
            };
            native(
                &get(&h.state, &format!("last{suffix}FetchTime")),
                &V::N(if prefix { 0. } else { 1000. }),
                "fetch timestamp",
            );
            let pending = match h.op {
                "positions" => "hasPendingPositionEvents",
                "balance" => "hasPendingBalanceEvents",
                _ => "hasPendingOrderEvents",
            };
            native(&get(&h.state, pending), &V::Bool(prefix), "pending reset");
            let applies = !matches!(h.scene, "prefix-error" | "reject" | "stop")
                && !(h.op == "orders" && h.scene == "stale");
            let name = match h.op {
                "positions" => "call:store.addOrUpdatePositionsFull",
                "balance" => "call:store.update",
                _ => "call:store.addOrUpdateOrdersFull",
            };
            let updates = event(h, name);
            assert_eq!(
                updates.len(),
                usize::from(applies),
                "SOCKET independent fetch update count {}/{}",
                h.op,
                h.scene
            );
            if applies {
                if h.op == "positions" {
                    native(
                        &updates[0].args[0],
                        &a(vec![o(vec![("id", s("p"))])]),
                        "position snapshot",
                    );
                    assert!(
                        equal(&updates[0].args[0], &h.positions),
                        "SOCKET native positions alias"
                    );
                    native(
                        &event(h, "call:mergeRestPositionsWithFreshWsState")[0].args[1],
                        &V::N(2000.),
                        "position request clock",
                    )
                } else if h.op == "balance" {
                    native(
                        &updates[0].args[0],
                        &o(vec![
                            ("balance", o(vec![("total", V::N(90.))])),
                            (
                                "loaded",
                                o(vec![
                                    ("balance", V::Bool(true)),
                                    ("orders", V::Bool(false)),
                                    ("other", s("keep")),
                                ]),
                            ),
                        ]),
                        "balance patch",
                    );
                    assert!(
                        equal(&get(&updates[0].args[0], "balance"), &h.balance),
                        "SOCKET native balance alias"
                    );
                    assert!(
                        !equal(&get(&updates[0].args[0], "loaded"), &h.loaded),
                        "SOCKET native loaded copy"
                    )
                } else {
                    let expected = a(vec![
                        row("r2", "closed", "limit"),
                        row("r3", "open", "limit"),
                        row("r4", "open", "limit"),
                        row("sl", "open", "stop_market"),
                        row("tp", "open", "take_profit_market"),
                        row("trail", "open", "trailing_stop_market"),
                        row("fresh", "open", "limit"),
                    ]);
                    native(&updates[0].args[0], &expected, "reconciled orders");
                    for v in values(&updates[0].args[0]) {
                        assert!(
                            values(&h.fetched)
                                .iter()
                                .chain(values(&h.current).iter())
                                .any(|raw| equal(raw, &v)),
                            "SOCKET native reconciled order alias"
                        )
                    }
                    native(
                        &event(h, "call:pruneOrderLifecycleCaches")[0].args[0],
                        &V::N(3000.),
                        "order prune time",
                    )
                }
            }
            if h.op == "orders" {
                native(
                    &get(&h.state, "ordersFetchRequestSeq"),
                    &V::N(if h.scene == "stale" { 99. } else { 2. }),
                    "orders sequence",
                )
            }
            if h.op == "balance" && !prefix {
                native(
                    &event(h, "call:parent.fetchBalance")[0].args[0],
                    &o(vec![("preferFast", V::Bool(true))]),
                    "fast balance options",
                )
            }
            native(
                &h.loaded,
                &o(vec![
                    ("balance", V::Bool(false)),
                    ("orders", V::Bool(false)),
                    ("other", s("keep")),
                ]),
                "loaded unchanged",
            );
        }
        "private" | "public" => {
            let pub_ = h.op == "public";
            let disposed = h.scene == "disposed";
            native(
                &get(&h.state, "messageCount"),
                &V::N(if disposed || h.scene == "pong" && !pub_ {
                    0.
                } else {
                    1.
                }),
                "message count",
            );
            let expected: Option<(&str, &str)> = if pub_ {
                match h.scene {
                    "ticker" => Some(("ticker", "handleTickerTopic")),
                    "candle" | "live" => Some(("candle_BTC_1m", "callback:cb1")),
                    "books" => Some(("orderBook_BTC", "callback:cb1")),
                    "both" => Some(("candle_BTC_1m", "callback:cb1")),
                    _ => None,
                }
            } else {
                match h.scene {
                    "uta-order" => Some(("order", "handleUtaOrderTopic")),
                    "uta-position" => Some(("position", "handleUtaPositionsTopic")),
                    "uta-account" => Some(("account", "handleUtaAccountTopic")),
                    "orders" | "disposed" => Some(("order", "handleOrderTopic")),
                    "algo" => Some(("order-algo", "handleAlgoOrdersTopic")),
                    "positions" => Some(("position", "handlePositionsTopic")),
                    "account" => Some(("account", "handleAccountTopic")),
                    _ => None,
                }
            };
            let stats = event(h, "call:recordMessageHandlerStats");
            let count = if h.scene == "both" {
                2
            } else {
                usize::from(expected.is_some())
            };
            assert_eq!(
                stats.len(),
                count,
                "SOCKET independent message stats count {}/{}",
                h.op,
                h.scene
            );
            if let Some((topic, handler)) = expected {
                native(&stats[0].args[0], &s(topic), "message stats topic");
                let handlers = event(
                    h,
                    &if handler.starts_with("callback:") {
                        handler.into()
                    } else {
                        format!("call:{handler}")
                    },
                );
                assert_eq!(
                    handlers.len(),
                    if h.scene == "both" { 2 } else { 1 },
                    "SOCKET independent handler count"
                );
                for e in handlers {
                    native(
                        &e.args[0],
                        &from_json(serde_json::from_str(&text(&h.data)).unwrap()),
                        "dispatch native JSON",
                    );
                    assert!(
                        h.parsed.borrow().iter().any(|p| equal(p, &e.args[0])),
                        "SOCKET native parsed object alias"
                    )
                }
            }
            if pub_ && matches!(h.scene, "candle" | "books" | "both" | "live") {
                let n = if h.scene == "both" {
                    4
                } else if h.scene == "live" {
                    3
                } else {
                    2
                };
                assert_eq!(
                    h.invoked.borrow().len(),
                    n,
                    "SOCKET independent live callback count"
                );
                let refs = values(&h.callbacks);
                for cb in h.invoked.borrow().iter() {
                    assert!(
                        refs.iter().any(|v| equal(v, cb)),
                        "SOCKET native callback reference alias"
                    )
                }
            }
            if !pub_ && h.scene == "login" {
                assert_eq!(
                    event(h, "call:subscribe").len(),
                    1,
                    "SOCKET native login success subscribes"
                )
            }
            if !pub_ && h.scene == "login-error" {
                native(
                    &event(h, "call:emitAndLogError")[0].args[0],
                    &s("Bitget private WS login failed: bad"),
                    "login error",
                )
            }
        }
        _ => panic!(),
    }
}
fn suite(op: &'static str, scenes: &[&'static str], mode: &str, floor: usize) {
    let mut cases = vec![];
    let mut outputs = vec![];
    for scene in scenes {
        let h = make(op, scene);
        cases.push(inputs(&h));
        let r = execute(&h);
        check(&h, &r);
        outputs.push(output(&h, &r))
    }
    let src = source(&cases, mode);
    assert_eq!(src.len(), floor, "SOCKET independent source floor");
    for (i, (a, b)) in outputs.iter().zip(&src).enumerate() {
        compare(a, b, &format!("{mode}/{i}"))
    }
}
#[test]
fn native_connect_then_source() {
    suite(
        "connect",
        &[
            "normal",
            "disposed",
            "fallback",
            "no-fallback",
            "uta",
            "emit-error",
        ],
        "connect",
        6,
    )
}
#[test]
fn native_subscribe_then_source() {
    suite("subscribe", &["normal", "uta"], "subscribe", 2)
}
#[test]
fn native_auth_then_source() {
    suite(
        "auth",
        &[
            "local",
            "remote",
            "reject",
            "error",
            "rotate",
            "error-getter",
        ],
        "auth",
        6,
    )
}
#[test]
fn native_private_dispatch_then_source() {
    suite(
        "private",
        &[
            "pong",
            "invalid",
            "login",
            "login-error",
            "uta-order",
            "uta-position",
            "uta-account",
            "orders",
            "algo",
            "positions",
            "account",
            "disposed",
            "ack",
        ],
        "private",
        13,
    )
}
#[test]
fn native_public_dispatch_then_source() {
    suite(
        "public",
        &[
            "pong",
            "invalid",
            "ticker",
            "ticker-ack",
            "candle",
            "books",
            "both",
            "spaced",
            "disposed",
            "live",
        ],
        "public",
        10,
    )
}
#[test]
fn native_fetches_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in ["positions", "balance", "orders"] {
        for scene in ["normal", "reject", "stop", "prefix-error", "stale"] {
            let h = make(op, scene);
            cases.push(inputs(&h));
            let r = execute(&h);
            check(&h, &r);
            outputs.push(output(&h, &r))
        }
    }
    let src = source(&cases, "fetch");
    assert_eq!(src.len(), 15, "SOCKET independent fetch floor");
    for (i, (a, b)) in outputs.iter().zip(&src).enumerate() {
        compare(a, b, &format!("fetch/{i}"))
    }
}
