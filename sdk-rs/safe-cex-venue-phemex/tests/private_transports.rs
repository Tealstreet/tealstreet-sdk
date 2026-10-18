use safe_cex_venue_phemex::{api_transport as api, private_session as session};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Array(Rc<RefCell<Vec<V>>>),
    Object(Rc<Vec<(Vec<u16>, V)>>),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v)))
}
fn object(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(v.into_iter().map(|(k, v)| (u(k), v)).collect()))
}
impl V {
    fn decode(v: &Value) -> Self {
        if let Some(a) = v.get("$units").and_then(Value::as_array) {
            return Self::Text(a.iter().map(|v| v.as_u64().unwrap() as u16).collect());
        }
        if let Some(bits) = v.get("$bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
        }
        if v.get("$undefined").is_some() {
            return Self::Undefined;
        }
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) => Self::Num(n.as_f64().unwrap()),
            Value::String(s) => Self::Text(u(s)),
            Value::Array(a) => array(a.iter().map(Self::decode).collect()),
            Value::Object(o) => Self::Object(Rc::new(
                o.iter().map(|(k, v)| (u(k), Self::decode(v))).collect(),
            )),
        }
    }
    fn get(&self, key: &str) -> V {
        match self {
            Self::Object(o) => o
                .iter()
                .find(|(k, _)| *k == u(key))
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::Undefined),
            _ => Self::Undefined,
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn text(&self) -> Option<Vec<u16>> {
        if let Self::Text(s) = self {
            Some(s.clone())
        } else {
            None
        }
    }
    fn values(&self) -> Vec<V> {
        if let Self::Array(v) = self {
            v.borrow().clone()
        } else {
            vec![]
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Num(n) => *n,
            Self::Text(s) => String::from_utf16(s)
                .map(|s| safe_cex_num::number_from_string(&s))
                .unwrap_or(f64::NAN),
            Self::Object(_) => f64::NAN,
            Self::Array(a) => {
                let a = a.borrow();
                if a.is_empty() {
                    0.0
                } else if a.len() == 1 {
                    a[0].number()
                } else {
                    f64::NAN
                }
            }
        }
    }
    fn obs(&self) -> Value {
        match self {
            Self::Undefined => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::Bool(b) => json!(b),
            Self::Num(n) => {
                json!({"bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
            }
            Self::Text(s) => json!({"units":s}),
            Self::Array(a) => Value::Array(a.borrow().iter().map(Self::obs).collect()),
            Self::Object(o) => {
                let mut entries = o
                    .iter()
                    .map(|(k, v)| json!([{"units":k},v.obs()]))
                    .collect::<Vec<_>>();
                entries.sort_by_key(|a| a[0].to_string());
                json!({"object":entries})
            }
        }
    }
    fn wire(&self) -> String {
        match self {
            Self::Undefined | Self::Null => "null".into(),
            Self::Bool(b) => b.to_string(),
            Self::Num(n) => {
                if n.is_finite() {
                    safe_cex_num::js_number_string(*n)
                } else {
                    "null".into()
                }
            }
            Self::Text(s) => quote(s),
            Self::Array(a) => format!(
                "[{}]",
                a.borrow()
                    .iter()
                    .map(Self::wire)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Object(o) => format!(
                "{{{}}}",
                o.iter()
                    .filter(|(_, v)| !matches!(v, Self::Undefined))
                    .map(|(k, v)| format!("{}:{}", quote(k), v.wire()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
}
fn quote(s: &[u16]) -> String {
    let mut out = String::from("\"");
    for ch in char::decode_utf16(s.iter().copied()) {
        match ch {
            Err(e) => out.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok(c) if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            Ok(c) => out.push(c),
        }
    }
    out.push('"');
    out
}
#[derive(Clone, Debug)]
struct E {
    original: bool,
    token: Rc<()>,
}
struct Deferred {
    value: Option<Result<V, E>>,
    pending: usize,
    disposed: Option<Rc<Cell<bool>>>,
    key: Option<(Rc<RefCell<V>>, V)>,
    finish: Option<(Rc<RefCell<Vec<V>>>, V)>,
    counter: Rc<Cell<usize>>,
}
impl Future for Deferred {
    type Output = Result<V, E>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.pending > 0 {
            self.pending -= 1;
            self.counter.set(self.counter.get() + 1);
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        if let Some(disposed) = self.disposed.take() {
            disposed.set(true)
        }
        if let Some((key, value)) = self.key.take() {
            *key.borrow_mut() = value
        }
        if let Some((effects, symbol)) = self.finish.take() {
            effects
                .borrow_mut()
                .push(array(vec![V::Text(u("settle")), symbol]));
        }
        Poll::Ready(self.value.take().unwrap())
    }
}
fn complete<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let waker = Waker::noop().clone();
    let mut cx = Context::from_waker(&waker);
    for _ in 0..100 {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
    }
    panic!("PRIVATE bounded continuation")
}
struct Host {
    input: V,
    effects: Rc<RefCell<Vec<V>>>,
    timer: V,
    timer_reads: usize,
    symbols: Vec<V>,
    disposed: Rc<Cell<bool>>,
    key: Rc<RefCell<V>>,
    request: f64,
    ping_at: V,
    next_timer: f64,
    fetch_count: usize,
    symbol_count: usize,
    tasks: Vec<(f64, &'static str)>,
    error: Rc<()>,
    pending: Rc<Cell<usize>>,
    interceptor: Option<fn(&mut Self, V) -> Result<V, E>>,
    client: Rc<()>,
}
impl Host {
    fn new(input: V) -> Self {
        let key = match input.get("key") {
            V::Undefined if !has(&input, "key") => V::Text(u("KEY")),
            v => v,
        };
        let mut symbols = vec![];
        for symbol in input.get("symbols").values() {
            if !symbols.iter().any(|s| same(s, &symbol)) {
                symbols.push(symbol)
            }
        }
        let request = match input.get("requestId") {
            V::Undefined | V::Null => 1200.0,
            v => v.number(),
        };
        Self {
            timer: input.get("timer"),
            timer_reads: 0,
            symbol_count: symbols.len(),
            symbols,
            disposed: Rc::new(Cell::new(input.get("disposed").truthy())),
            input,
            key: Rc::new(RefCell::new(key)),
            effects: Rc::new(RefCell::new(vec![])),
            request,
            ping_at: V::Undefined,
            next_timer: 100.0,
            fetch_count: 0,
            tasks: vec![],
            error: Rc::new(()),
            pending: Rc::new(Cell::new(0)),
            interceptor: None,
            client: Rc::new(()),
        }
    }
    fn effect(&mut self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut row = vec![V::Text(u(name))];
        row.extend(args);
        self.effects.borrow_mut().push(array(row));
        if self.input.get("failure").text() == Some(u(name)) {
            Err(self.original())
        } else {
            Ok(())
        }
    }
    fn original(&self) -> E {
        E {
            original: true,
            token: self.error.clone(),
        }
    }
    fn clock_value(&self) -> f64 {
        if has(&self.input, "timestamp") {
            self.input.get("timestamp").number()
        } else {
            1700000000000.0
        }
    }
    fn secret_value(&self) -> V {
        if has(&self.input, "secret") {
            self.input.get("secret")
        } else {
            V::Text(u("SECRET"))
        }
    }
    fn hmac_start(&mut self, secret: V) -> Result<Vec<u8>, E> {
        if self.input.get("failure").text() == Some(u("hmac")) {
            self.effect("hmac", vec![])?
        }
        let Some(secret) = secret.text() else {
            return Err(E {
                original: false,
                token: Rc::new(()),
            });
        };
        Ok(String::from_utf16_lossy(&secret).into_bytes())
    }
    fn hmac_finish(&mut self, key: Vec<u8>, payload: Vec<u16>) -> Result<V, E> {
        let payload = String::from_utf16_lossy(&payload);
        Ok(V::Text(u(&safe_cex_crypto::hmac_sha256(
            &key,
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        ))))
    }
    fn deferred(&self, value: Result<V, E>) -> Deferred {
        Deferred {
            value: Some(value),
            pending: 1,
            disposed: None,
            key: None,
            finish: None,
            counter: self.pending.clone(),
        }
    }
    fn unset(&mut self) -> Result<(), E> {
        self.timer = V::Undefined;
        Ok(())
    }
    fn clear(&mut self) -> Result<(), E> {
        self.symbols.clear();
        Ok(())
    }
    fn api_client(&self, client: &Rc<()>) {
        assert!(
            Rc::ptr_eq(client, &self.client),
            "PRIVATE native client identity"
        )
    }
}
fn has(v: &V, key: &str) -> bool {
    match v {
        V::Object(o) => o.iter().any(|(k, _)| *k == u(key)),
        _ => false,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Num(a), V::Num(b)) => a == b || (a.is_nan() && b.is_nan()),
        _ => a.obs() == b.obs(),
    }
}
fn str_units(v: V, join: bool) -> Vec<u16> {
    match v {
        V::Undefined => {
            if join {
                vec![]
            } else {
                u("undefined")
            }
        }
        V::Null => {
            if join {
                vec![]
            } else {
                u("null")
            }
        }
        V::Text(s) => s,
        V::Bool(b) => u(if b { "true" } else { "false" }),
        V::Num(n) => u(&safe_cex_num::js_number_string(n)),
        _ => u("[object Object]"),
    }
}
impl session::SessionHost for Host {
    type Timer = V;
    type Symbol = V;
    type Symbols = V;
    type Socket = Rc<()>;
    type Call = ();
    type Error = E;
    fn timeout(&mut self) -> Result<Option<V>, E> {
        let timer = self.timeout_value()?;
        Ok(if timer.truthy() { Some(timer) } else { None })
    }
    fn timeout_value(&mut self) -> Result<V, E> {
        self.timer_reads += 1;
        Ok(
            if self.input.get("timeoutClearsOnRead").truthy() && self.timer_reads > 1 {
                V::Undefined
            } else {
                self.timer.clone()
            },
        )
    }
    fn clear_timeout(&mut self, timer: V) -> Result<(), E> {
        self.effect("clear", vec![timer.clone()])?;
        self.tasks.retain(|(id, _)| *id != timer.number());
        Ok(())
    }
    fn unset_timeout(&mut self) -> Result<(), E> {
        self.unset()
    }
    fn clear_symbols(&mut self) -> Result<(), E> {
        self.clear()
    }
    fn base_dispose(&mut self) -> Result<(), E> {
        self.effect("baseDispose", vec![])?;
        self.disposed.set(true);
        Ok(())
    }
    fn empty_symbols(&mut self) -> Result<V, E> {
        Ok(array(vec![]))
    }
    fn symbols_length(&mut self, s: &V) -> Result<usize, E> {
        Ok(s.values().len())
    }
    fn symbol_at(&mut self, s: &V, index: usize) -> Result<Option<V>, E> {
        Ok(s.values().get(index).cloned())
    }
    fn add_symbol(&mut self, s: V) -> Result<(), E> {
        if !self.symbols.iter().any(|v| same(v, &s)) {
            self.symbols.push(s)
        }
        Ok(())
    }
    fn debounce(&mut self) -> Result<f64, E> {
        Ok(1000.0)
    }
    fn schedule(&mut self, delay: f64, member: &'static str) -> Result<V, E> {
        self.effect("schedule", vec![V::Num(delay)])?;
        self.next_timer += 1.0;
        self.tasks.push((self.next_timer, member));
        Ok(V::Num(self.next_timer))
    }
    fn set_timeout(&mut self, timer: V) -> Result<(), E> {
        self.timer = timer;
        Ok(())
    }
    fn disposed(&mut self) -> Result<bool, E> {
        Ok(self.disposed.get())
    }
    fn testnet(&mut self) -> Result<bool, E> {
        Ok(self.input.get("testnet").truthy())
    }
    fn create(&mut self, url: &'static str) -> Result<Rc<()>, E> {
        self.effect("create", vec![V::Text(u(url))])?;
        Ok(self.client.clone())
    }
    fn set_socket(&mut self, socket: Rc<()>) -> Result<(), E> {
        self.api_client(&socket);
        Ok(())
    }
    fn listener(&mut self, event: &'static str, member: &'static str) -> Result<(), E> {
        self.effect(&format!("listener:{event}"), vec![V::Text(u(member))])
    }
    fn start_auth(&mut self) -> Result<(), E> {
        self.effect("authStart", vec![])
    }
    fn ping(&mut self) -> Result<(), E> {
        self.effect("ping", vec![])
    }
    fn now(&mut self) -> Result<f64, E> {
        Ok(123.25)
    }
    fn set_ping_at(&mut self, n: f64) -> Result<(), E> {
        self.ping_at = V::Num(n);
        Ok(())
    }
    fn send_target(&mut self) -> Result<(), E> {
        Ok(())
    }
    fn next_id(&mut self) -> Result<f64, E> {
        let id = self.request;
        self.request += 1.0;
        Ok(id)
    }
    fn send(&mut self, _: (), text: Vec<u16>) -> Result<(), E> {
        self.effect("send", vec![V::Text(text)])
    }
}
impl session::AuthHost for Host {
    type Value = V;
    type Signer = Rc<()>;
    type Hmac = Vec<u8>;
    type Request = V;
    type Reply = V;
    type Call = ();
    type Error = E;
    fn clock(&mut self) -> Result<f64, E> {
        Ok(self.clock_value())
    }
    fn remote(&mut self) -> Result<bool, E> {
        Ok(self.input.get("remote").truthy())
    }
    fn signer_target(&mut self) -> Result<Rc<()>, E> {
        Ok(self.client.clone())
    }
    fn nonce(&mut self) -> Result<V, E> {
        Ok(V::Text(u("nonce")))
    }
    fn account(&mut self) -> Result<V, E> {
        Ok(if has(&self.input, "accountName") {
            self.input.get("accountName")
        } else {
            V::Text(u("account"))
        })
    }
    fn key(&mut self) -> Result<V, E> {
        Ok(self.key.borrow().clone())
    }
    fn key_text(&mut self, key: V, template: bool) -> Result<Vec<u16>, E> {
        Ok(str_units(key, !template))
    }
    fn number_text(&mut self, text: &[u16]) -> Result<f64, E> {
        Ok(V::Text(text.to_vec()).number())
    }
    fn request(&mut self, id: V, account: V, to_sign: Vec<u16>, expiry: f64) -> Result<V, E> {
        Ok(object(vec![
            ("id", id),
            ("type", V::Text(u("ws-auth"))),
            ("exchangeName", V::Text(u("phemex"))),
            ("accountName", account),
            (
                "wsAuth",
                object(vec![
                    ("toSign", V::Text(to_sign)),
                    ("outputFormat", V::Text(u("hex"))),
                    ("extraData", object(vec![("expiry", V::Num(expiry))])),
                ]),
            ),
        ]))
    }
    fn sign(&mut self, signer: Rc<()>, request: V) -> impl Future<Output = Result<V, E>> {
        self.api_client(&signer);
        let effect = self.effect("sign", vec![request]);
        let reply = if has(&self.input, "reply") {
            self.input.get("reply")
        } else {
            object(vec![("wsAuthSignature", V::Text(u("remote-signature")))])
        };
        let mut future = self.deferred(effect.map(|_| reply));
        if has(&self.input, "keyAfterSign") {
            future.key = Some((self.key.clone(), self.input.get("keyAfterSign")))
        }
        future
    }
    fn reply_error(&mut self, reply: &V) -> Result<bool, E> {
        Ok(reply.get("error").truthy())
    }
    fn error_text(&mut self, reply: &V) -> Result<Vec<u16>, E> {
        Ok(str_units(reply.get("error"), false))
    }
    fn emit_error(&mut self, message: Vec<u16>) -> Result<(), E> {
        self.effect("emitError", vec![V::Text(message)])
    }
    fn signature(&mut self, reply: V) -> Result<V, E> {
        Ok(reply.get("wsAuthSignature"))
    }
    fn secret(&mut self) -> Result<V, E> {
        Ok(self.secret_value())
    }
    fn create_hmac(&mut self, secret: V) -> Result<Vec<u8>, E> {
        self.hmac_start(secret)
    }
    fn hmac(&mut self, hmac: Vec<u8>, message: Vec<u16>) -> Result<V, E> {
        self.hmac_finish(hmac, message)
    }
    fn send_target(&mut self) -> Result<(), E> {
        Ok(())
    }
    fn serialize(&mut self, key: V, signature: V, expiry: f64, id: f64) -> Result<Vec<u16>, E> {
        Ok(u(&object(vec![
            ("method", V::Text(u("user.auth"))),
            (
                "params",
                array(vec![V::Text(u("API")), key, signature, V::Num(expiry)]),
            ),
            ("id", V::Num(id)),
        ])
        .wire()))
    }
    fn send(&mut self, _: (), text: Vec<u16>) -> Result<(), E> {
        self.effect("send", vec![V::Text(text)])
    }
}
impl session::SyncHost for Host {
    type Symbol = V;
    type Balance = V;
    type Position = V;
    type Positions = V;
    type Account = V;
    type Id = V;
    type Error = E;
    type Read = Deferred;
    fn unset_timeout(&mut self) -> Result<(), E> {
        self.unset()
    }
    fn symbols(&mut self) -> Result<Vec<V>, E> {
        Ok(self.symbols.clone())
    }
    fn clear_symbols(&mut self) -> Result<(), E> {
        self.clear()
    }
    fn disposed(&mut self) -> Result<bool, E> {
        Ok(self.disposed.get())
    }
    type AccountRead = Deferred;
    fn account(&mut self) -> Deferred {
        let effect = self.effect("account", vec![]);
        let positions = if has(&self.input, "positions") {
            self.input.get("positions")
        } else {
            array(vec![])
        };
        let account = object(vec![
            ("balance", object(vec![("total", V::Num(9.0))])),
            ("positions", positions),
        ]);
        let mut future = self.deferred(effect.map(|_| account));
        if self.input.get("disposedAfterAccount").truthy() {
            future.disposed = Some(self.disposed.clone())
        }
        future
    }
    fn parts(&mut self, account: V) -> Result<(V, V), E> {
        Ok((account.get("balance"), account.get("positions")))
    }
    fn fetch(&mut self, symbol: V) -> Result<Deferred, E> {
        self.effect("fetch", vec![array(vec![symbol.clone()])])?;
        let name = String::from_utf16_lossy(&symbol.text().unwrap());
        let reject = self
            .input
            .get("rejectSymbols")
            .values()
            .iter()
            .any(|v| same(v, &symbol));
        let result = if reject {
            Err(self.original())
        } else {
            let rows = self.input.get("symbolResults").get(&name);
            Ok(if matches!(rows, V::Undefined) {
                array(vec![])
            } else {
                rows
            })
        };
        let mut future = self.deferred(result);
        self.fetch_count += 1;
        if self.input.get("reverseCompletion").truthy() {
            future.pending = self.symbol_count - self.fetch_count + 1;
        }
        future.finish = Some((self.effects.clone(), symbol));
        Ok(future)
    }
    fn rows(&mut self, positions: V) -> Result<Vec<V>, E> {
        Ok(positions.values())
    }
    fn id(&mut self, p: &V) -> Result<V, E> {
        Ok(p.get("id"))
    }
    fn same_id(&mut self, a: &V, b: &V) -> Result<bool, E> {
        Ok(same(a, b))
    }
    fn store_positions(&mut self, positions: Vec<V>) -> Result<(), E> {
        for p in &positions {
            if let V::Object(actual) = p {
                let source = self
                    .input
                    .get("positions")
                    .values()
                    .into_iter()
                    .chain(match self.input.get("symbolResults") {
                        V::Object(o) => o.iter().flat_map(|(_, v)| v.values()).collect::<Vec<_>>(),
                        _ => vec![],
                    })
                    .find(|original| same(original, p));
                if let Some(V::Object(original)) = source {
                    assert!(
                        Rc::ptr_eq(actual, &original),
                        "PRIVATE native position identity"
                    )
                }
            }
        }
        self.effect("positions", vec![array(positions)])
    }
    fn update(&mut self, balance: V, b: bool, p: bool) -> Result<(), E> {
        self.effect(
            "update",
            vec![object(vec![
                ("balance", balance),
                (
                    "loaded",
                    object(vec![
                        ("old", V::Bool(true)),
                        ("balance", V::Bool(b)),
                        ("positions", V::Bool(p)),
                    ]),
                ),
            ])],
        )
    }
}
fn query(v: &V, prefix: Vec<u16>, out: &mut Vec<Vec<u16>>) {
    match v {
        V::Object(fields) => {
            for (k, v) in fields.iter() {
                let name = if prefix.is_empty() {
                    k.clone()
                } else {
                    prefix
                        .iter()
                        .copied()
                        .chain([91])
                        .chain(k.iter().copied())
                        .chain([93])
                        .collect()
                };
                query(v, name, out)
            }
        }
        V::Array(items) => {
            for (i, v) in items.borrow().iter().enumerate() {
                let name = prefix
                    .iter()
                    .copied()
                    .chain([91])
                    .chain(u(&i.to_string()))
                    .chain([93])
                    .collect();
                query(v, name, out)
            }
        }
        V::Undefined => {}
        _ => {
            let mut field = prefix;
            field.push(61);
            if !matches!(v, V::Null) {
                field.extend(str_units(v.clone(), false))
            }
            out.push(field)
        }
    }
}
impl api::RequestHost for Host {
    type Config = V;
    type Value = V;
    type Hmac = Vec<u8>;
    type Error = E;
    fn copy(&mut self, config: &V) -> Result<V, E> {
        let V::Object(fields) = config else { panic!() };
        Ok(V::Object(Rc::new(fields.as_ref().clone())))
    }
    fn remote(&mut self) -> Result<bool, E> {
        Ok(self.input.get("remote").truthy())
    }
    fn key(&mut self) -> Result<V, E> {
        Ok(self.key.borrow().clone())
    }
    fn secret(&mut self) -> Result<V, E> {
        Ok(self.secret_value())
    }
    fn truthy(&mut self, v: &V) -> Result<bool, E> {
        Ok(v.truthy())
    }
    fn params(&mut self, c: &V) -> Result<Option<Vec<u16>>, E> {
        let params = c.get("params");
        if !params.truthy() {
            return Ok(None);
        }
        let mut fields = vec![];
        query(&params, vec![], &mut fields);
        Ok(Some(
            fields
                .into_iter()
                .enumerate()
                .flat_map(|(i, v)| if i == 0 { v } else { [vec![38], v].concat() })
                .collect(),
        ))
    }
    fn data(&mut self, c: &V) -> Result<Option<Vec<u16>>, E> {
        let data = c.get("data");
        Ok(if data.truthy() {
            Some(u(&data.wire()))
        } else {
            None
        })
    }
    fn clock(&mut self) -> Result<f64, E> {
        Ok(self.clock_value())
    }
    fn url(&mut self, c: &V) -> Result<V, E> {
        Ok(c.get("url"))
    }
    fn create_hmac(&mut self, secret: V) -> Result<Vec<u8>, E> {
        self.hmac_start(secret)
    }
    fn join(
        &mut self,
        url: V,
        params: Vec<u16>,
        expiry: &[u16],
        data: Option<Vec<u16>>,
    ) -> Result<Vec<u16>, E> {
        Ok(str_units(url, true)
            .into_iter()
            .chain(params)
            .chain(expiry.iter().copied())
            .chain(data.unwrap_or_default())
            .collect())
    }
    fn hmac(&mut self, hmac: Vec<u8>, payload: Vec<u16>) -> Result<V, E> {
        self.hmac_finish(hmac, payload)
    }
    fn headers(&mut self, config: V, key: V, expiry: Vec<u16>, signature: V) -> Result<V, E> {
        let mut fields = match config.get("headers") {
            V::Object(o) => o.as_ref().clone(),
            _ => vec![],
        };
        fields.extend([
            (u("Content-Type"), V::Text(u("application/json"))),
            (u("X-PHEMEX-ACCESS-TOKEN"), key),
            (u("X-PHEMEX-REQUEST-EXPIRY"), V::Text(expiry)),
            (u("X-PHEMEX-REQUEST-SIGNATURE"), signature),
        ]);
        let headers = V::Object(Rc::new(fields));
        self.effect("headers", vec![headers.clone()])?;
        let V::Object(fields) = config else { panic!() };
        let mut result = fields.as_ref().clone();
        if let Some((_, h)) = result.iter_mut().find(|(k, _)| *k == u("headers")) {
            *h = headers
        } else {
            result.push((u("headers"), headers))
        }
        Ok(V::Object(Rc::new(result)))
    }
}
impl api::ApiHost for Host {
    type Client = Rc<()>;
    type RetryCondition = fn(&str) -> bool;
    fn testnet(&mut self) -> Result<bool, E> {
        Ok(self.input.get("testnet").truthy())
    }
    fn create(&mut self, base: &'static str, timeout: f64, credentials: bool) -> Result<Rc<()>, E> {
        self.effect(
            "create",
            vec![object(vec![
                ("baseURL", V::Text(u(base))),
                ("timeout", V::Num(timeout)),
                ("withCredentials", V::Bool(credentials)),
            ])],
        )?;
        Ok(self.client.clone())
    }
    fn retry_condition(&mut self) -> Result<fn(&str) -> bool, E> {
        Ok(|method| matches!(method.to_ascii_lowercase().as_str(), "get" | "head"))
    }
    fn retry(&mut self, client: &Rc<()>, n: usize, condition: fn(&str) -> bool) -> Result<(), E> {
        self.api_client(client);
        self.effect(
            "retry",
            vec![
                V::Num(n as f64),
                array(
                    ["get", "head", "post", "delete", "GET"]
                        .into_iter()
                        .map(|method| V::Bool(condition(method)))
                        .collect(),
                ),
            ],
        )
    }
    fn tunnel(&mut self) -> Result<bool, E> {
        Ok(self.input.get("tunnel").truthy())
    }
    fn install(&mut self, client: &Rc<()>) -> Result<(), E> {
        self.api_client(client);
        self.effect("install", vec![V::Text(u("phemex"))])
    }
    fn interceptor(
        &mut self,
        client: &Rc<()>,
        cb: fn(&mut Self, V) -> Result<V, E>,
    ) -> Result<(), E> {
        self.api_client(client);
        self.effect("interceptor", vec![])?;
        self.interceptor = Some(cb);
        Ok(())
    }
}
fn run(name: &str, count: usize) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/private-transport-oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PRIVATE actual source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), count, "PRIVATE source case census");
    for (index, case) in cases.into_iter().enumerate() {
        let input = V::decode(&case["input"]);
        let mut host = Host::new(input.clone());
        let mut result = V::Undefined;
        let mut copy = V::Undefined;
        let outcome = match name {
            "dispose" => session::dispose(&mut host),
            "schedule" => {
                let requests = if has(&input, "requests") {
                    input.get("requests").values()
                } else {
                    vec![V::Undefined]
                };
                let mut outcome = Ok(());
                for symbols in requests {
                    if let Err(e) = session::schedule_positions_and_balance_sync(
                        &mut host,
                        if matches!(symbols, V::Undefined) {
                            None
                        } else {
                            Some(symbols)
                        },
                    ) {
                        outcome = Err(e);
                        break;
                    }
                }
                if outcome.is_ok() && input.get("fire").truthy() {
                    for (_, member) in host.tasks.clone() {
                        assert_eq!(
                            member, "syncPositionsAndBalance",
                            "PRIVATE native timer member"
                        );
                        outcome = host.effect("syncStart", vec![]);
                        if outcome.is_err() {
                            break;
                        }
                    }
                }
                outcome
            }
            "connect" => session::connect_and_subscribe(&mut host),
            "open" => session::on_open(&mut host),
            "ping" => session::ping(&mut host).and_then(|_| {
                if input.get("repeat").truthy() {
                    session::ping(&mut host)
                } else {
                    Ok(())
                }
            }),
            "auth" => complete(session::auth(&mut host)),
            "sync" => complete(session::sync_positions_and_balance(&mut host)),
            "api" => {
                let config = if has(&input, "config") {
                    input.get("config")
                } else {
                    object(vec![
                        ("url", V::Text(u("/g-orders"))),
                        ("headers", object(vec![("old", V::Text(u("value")))])),
                    ])
                };
                match api::create_api(&mut host) {
                    Err(e) => Err(e),
                    Ok(client) => {
                        host.api_client(&client);
                        if input.get("mutateOptions").truthy() {
                            *host.key.borrow_mut() = V::Text(u("CHANGED"))
                        }
                        let cb = host.interceptor.unwrap();
                        match cb(&mut host, config.clone()) {
                            Err(e) => Err(e),
                            Ok(value) => {
                                let (V::Object(a), V::Object(b)) = (&value, &config) else {
                                    panic!()
                                };
                                assert!(!Rc::ptr_eq(a, b), "PRIVATE native shallow config copy");
                                copy = V::Bool(true);
                                result = value;
                                Ok(())
                            }
                        }
                    }
                }
            }
            _ => panic!(),
        };
        let mut thrown = match outcome {
            Ok(()) => V::Undefined,
            Err(e) => {
                if e.original {
                    assert!(
                        Rc::ptr_eq(&e.token, &host.error),
                        "PRIVATE native error identity"
                    );
                    V::Text(u("original"))
                } else {
                    V::Text(u("intrinsic"))
                }
            }
        };
        let source_thrown = case["expected"]["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0]["units"] == json!(u("thrown")))
            .unwrap()[1]
            .clone();
        if name == "sync"
            && matches!(
                input.get("failure").text().as_deref(),
                Some([97, 99, 99, 111, 117, 110, 116])
                    | Some([112, 111, 115, 105, 116, 105, 111, 110, 115])
                    | Some([117, 112, 100, 97, 116, 101])
                    | Some([102, 101, 116, 99, 104])
            )
        {
            assert_eq!(
                source_thrown,
                V::Undefined.obs(),
                "PRIVATE source silent reconciliation fallback"
            );
            assert_eq!(
                thrown.obs(),
                V::Text(u("original")).obs(),
                "PRIVATE corrected native reconciliation surfaces original error"
            );
            thrown = V::Undefined;
        }
        let observed = if name == "api" {
            object(vec![
                ("effects", array(host.effects.borrow().clone())),
                ("result", result),
                ("thrown", thrown),
                ("returnedCopy", copy),
            ])
        } else {
            object(vec![
                ("effects", array(host.effects.borrow().clone())),
                ("thrown", thrown),
                ("timer", host.timer.clone()),
                ("symbols", array(host.symbols.clone())),
                ("disposed", V::Bool(host.disposed.get())),
                ("requestId", V::Num(host.request)),
                ("pingAt", host.ping_at.clone()),
            ])
        };
        let expected_effects = case["expected"]["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0]["units"] == json!(u("effects")))
            .unwrap()[1]
            .as_array()
            .unwrap()
            .clone();
        for (a, b) in host.effects.borrow().iter().zip(&expected_effects) {
            let label = String::from_utf16_lossy(&a.values()[0].text().unwrap());
            assert_eq!(
                a.obs(),
                *b,
                "PRIVATE native effect {label} family {name} case {index}"
            );
        }
        assert_eq!(
            observed.obs(),
            case["expected"],
            "PRIVATE native complete observation family {name} case {index}"
        );
        if (name == "auth"
            && input.get("remote").truthy()
            && input.get("failure").text() != Some(u("sign")))
            || (name == "sync"
                && !input.get("disposed").truthy()
                && input.get("failure").text() != Some(u("account")))
        {
            assert!(host.pending.get() > 0, "PRIVATE native actual Pending");
        }
    }
}
#[test]
fn actual_typescript_dispose() {
    run("dispose", 6)
}
#[test]
fn actual_typescript_schedule() {
    run("schedule", 7)
}
#[test]
fn actual_typescript_connect() {
    run("connect", 5)
}
#[test]
fn actual_typescript_open() {
    run("open", 4)
}
#[test]
fn actual_typescript_ping() {
    run("ping", 6)
}
#[test]
fn actual_typescript_auth() {
    run("auth", 14)
}
#[test]
fn actual_typescript_sync() {
    run("sync", 11)
}
#[test]
fn actual_typescript_api() {
    run("api", 16)
}

#[test]
fn reconciliation_starts_account_read_before_the_future_is_polled() {
    let mut host = Host::new(object(vec![
        ("symbols", array(vec![V::Text(u("BTCUSDT"))])),
        ("timer", V::Num(7.0)),
    ]));
    let effects = host.effects.clone();
    let future = session::sync_positions_and_balance(&mut host);
    assert_eq!(
        effects.borrow().len(),
        1,
        "PRIVATE eager account read begins before polling"
    );
    assert_eq!(
        effects.borrow()[0].values()[0].text(),
        Some(u("account")),
        "PRIVATE eager account read identity"
    );
    drop(future);
    assert!(
        matches!(host.timer, V::Undefined),
        "PRIVATE eager timeout reset"
    );
    assert!(host.symbols.is_empty(), "PRIVATE eager sync queue snapshot");
}
