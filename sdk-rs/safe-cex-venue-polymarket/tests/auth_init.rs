use safe_cex_venue_polymarket::{
    account_reads::*, auth_init::*, order_adapters::Completion, ws_dispatch::Runtime,
};
use serde_json::{Value as J, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Function(Option<u32>, String, Rc<()>),
    Bytes(Vec<u8>),
    U,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Object(Option<u32>, Rc<RefCell<Vec<(String, V)>>>),
    Array(Option<u32>, Rc<RefCell<Vec<Option<V>>>>),
}
#[derive(Debug)]
struct Failure {
    marker: Option<u32>,
    message: V,
}
type E = Rc<Failure>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::Text(units(s))
}
fn object(id: Option<u32>, fields: Vec<(String, V)>) -> V {
    V::Object(id, Rc::new(RefCell::new(fields)))
}
fn list(id: Option<u32>, slots: Vec<Option<V>>) -> V {
    V::Array(id, Rc::new(RefCell::new(slots)))
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "function" => V::Function(
            j["id"].as_u64().map(|x| x as u32),
            j["name"].as_str().unwrap().into(),
            Rc::new(()),
        ),
        "bytes" => V::Bytes(
            j["bytes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u8)
                .collect(),
        ),
        "undefined" => V::U,
        "null" => V::Null,
        "bool" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Number(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "text" => V::Text(
            j["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| u16::try_from(x.as_u64().unwrap()).unwrap())
                .collect(),
        ),
        "object" => object(
            j["id"].as_u64().map(|v| v as u32),
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| (x[0].as_str().unwrap().into(), decode(&x[1])))
                .collect(),
        ),
        "array" => list(
            j["id"].as_u64().map(|v| v as u32),
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| {
                    if x["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(x))
                    }
                })
                .collect(),
        ),
        other => panic!("native fixture {other}"),
    }
}
fn raw_field(v: &V, key: &str) -> V {
    if let V::Object(_, f) = v {
        f.borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map_or(V::U, |(_, v)| v.clone())
    } else {
        V::U
    }
}
fn field_set(v: &V, key: &str, value: V) {
    let V::Object(_, f) = v else {
        panic!("native object")
    };
    let mut f = f.borrow_mut();
    if let Some((_, v)) = f.iter_mut().find(|(k, _)| k == key) {
        *v = value
    } else {
        f.push((key.into(), value));
    }
}
fn numeric(v: &V) -> f64 {
    match v {
        V::Number(n) => *n,
        V::Null => 0.,
        V::U => f64::NAN,
        V::Bool(b) => u8::from(*b) as f64,
        V::Text(s) => String::from_utf16_lossy(s).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn project(v: &V) -> J {
    match v {
        V::Function(id, name, _) => json!({"kind":"function","id":id,"name":name}),
        V::Bytes(bytes) => json!({"kind":"bytes","bytes":bytes}),
        V::U => json!({"kind":"undefined"}),
        V::Null => json!({"kind":"null"}),
        V::Bool(b) => json!({"kind":"bool","value":b}),
        V::Number(n) => json!({"kind":"number","bits":format!("{:016x}",n.to_bits())}),
        V::Text(t) => json!({"kind":"text","units":t}),
        V::Object(id, f) => {
            json!({"kind":"object","id":id,"fields":f.borrow().iter().map(|(k,v)|json!([k,project(v)])).collect::<Vec<_>>()})
        }
        V::Array(id, slots) => {
            json!({"kind":"array","id":id,"items":slots.borrow().iter().map(|v|v.as_ref().map_or(json!({"kind":"hole"}),project)).collect::<Vec<_>>()})
        }
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Function(_, _, a), V::Function(_, _, b)) => Rc::ptr_eq(a, b),
        (V::Object(_, a), V::Object(_, b)) => Rc::ptr_eq(a, b),
        (V::Array(_, a), V::Array(_, b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
#[derive(Clone)]
struct Call {
    namespace: &'static str,
    method: &'static str,
    receiver: Option<V>,
    generation: usize,
}
struct Op(Result<V, E>);
impl Completion<V, E> for Op {
    fn poll(&self, _: &mut Context<'_>) -> Poll<Result<V, E>> {
        Poll::Ready(self.0.clone())
    }
}
struct Host {
    name: String,
    i: usize,
    values: V,
    error: E,
    events: RefCell<Vec<J>>,
    native: RefCell<Vec<(String, Vec<V>)>>,
    writes: RefCell<Vec<(&'static str, V)>>,
    receiver: RefCell<V>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    signer: RefCell<Option<WalletSigner<Host>>>,
    delays: RefCell<Vec<Rc<dyn Fn(V) -> Result<V, E>>>>,
    bridge_returns: RefCell<Vec<V>>,
    bridge_inputs: RefCell<Vec<V>>,
    wallet_receivers: RefCell<Vec<V>>,
    generation: Cell<usize>,
    ctor_generation: Cell<usize>,
    captures: RefCell<Vec<(&'static str, usize)>>,
    creates: Cell<usize>,
}
impl Host {
    fn new(case: &J) -> Self {
        let f = &case["fixture"];
        let values = object(
            None,
            f.as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), decode(v)))
                .collect(),
        );
        Self {
            name: case["name"].as_str().unwrap().into(),
            i: case["i"].as_u64().unwrap() as usize,
            values,
            error: Rc::new(Failure {
                marker: Some(47),
                message: text("bound failure"),
            }),
            events: RefCell::new(vec![]),
            native: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
            receiver: RefCell::new(V::Null),
            tasks: RefCell::new(vec![]),
            signer: RefCell::new(None),
            delays: RefCell::new(vec![]),
            bridge_returns: RefCell::new(vec![]),
            bridge_inputs: RefCell::new(vec![]),
            wallet_receivers: RefCell::new(vec![]),
            generation: Cell::new(0),
            ctor_generation: Cell::new(0),
            captures: RefCell::new(vec![]),
            creates: Cell::new(0),
        }
    }
    fn value(&self, k: &str) -> V {
        raw_field(&self.values, k)
    }
    fn event(&self, tag: &str, args: &[V]) {
        self.native.borrow_mut().push((tag.into(), args.to_vec()));
        let mut row = vec![json!(tag)];
        row.extend(args.iter().map(project));
        self.events.borrow_mut().push(J::Array(row));
    }
    fn finish(&self) {
        for _ in 0..32 {
            let mut tasks = self.tasks.take();
            if tasks.is_empty() {
                return;
            }
            let mut cx = Context::from_waker(Waker::noop());
            for mut t in tasks.drain(..) {
                if t.as_mut().poll(&mut cx).is_pending() {
                    self.tasks.borrow_mut().push(t);
                }
            }
        }
        panic!("finite native continuation floor");
    }
    fn bridge_result(&self, method: &str, result: V) {
        self.event(&format!("bridge.{method}"), std::slice::from_ref(&result));
        self.bridge_returns.borrow_mut().push(result);
    }
    fn fn_value(&self, name: &str) -> V {
        V::Function(None, name.into(), Rc::new(()))
    }
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::U
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::Null => false,
            V::Bool(b) => *b,
            V::Number(n) => *n != 0. && !n.is_nan(),
            V::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, s: &str) -> bool {
        matches!(v,V::Text(t)if *t==units(s))
    }
    fn field(&self, v: &V, key: &str) -> Result<V, E> {
        if self.nullish(v) {
            return Err(self.new_error("nullish field"));
        }
        if self.name == "createSignedOrder"
            && self.i == 8
            && key == "signatureType"
            && same(v, &self.value("params"))
        {
            self.ctor_generation.set(1);
        }
        Ok(raw_field(v, key))
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::Text(t) => t.clone(),
            V::U => units("undefined"),
            V::Null => units("null"),
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            _ => units("[object Object]"),
        })
    }
    fn log(&self, _: &[u16], _: &'static str) -> Result<(), E> {
        unreachable!("no logger in these bodies")
    }
}
impl AccountHost for Host {
    type Callable = Call;
    type Operation = Op;
    fn start_task(&self, mut task: Pin<Box<dyn Future<Output = ()>>>) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.tasks.borrow_mut().push(task);
        }
    }
    fn capture(&self, namespace: &'static str, method: &'static str) -> Result<Call, E> {
        Ok(Call {
            namespace,
            method,
            receiver: None,
            generation: 0,
        })
    }
    fn invoke(&self, c: Call, args: Vec<V>) -> Result<Rc<Op>, E> {
        Ok(Rc::new(Op(self.call_sync(c, args))))
    }
    fn invoke_log(&self, _: Call, _: &[u16], _: &'static str) -> Result<(), E> {
        unreachable!()
    }
    fn resolve(&self, v: V) -> Result<Rc<Op>, E> {
        Ok(Rc::new(Op(Ok(v))))
    }
    fn credentials(&self) -> Result<V, E> {
        unreachable!()
    }
    fn wallet(&self) -> Result<V, E> {
        unreachable!()
    }
    fn exchange_credentials(&self) -> Result<V, E> {
        unreachable!()
    }
    fn signer_type(&self) -> Result<V, E> {
        unreachable!()
    }
    fn text(&self, t: &'static str) -> V {
        text(t)
    }
    fn number(&self, n: f64) -> V {
        V::Number(n)
    }
    fn null(&self) -> V {
        V::Null
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(
            None,
            f.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        ))
    }
    fn assign(&self, o: &V, k: &'static str, v: V) -> Result<(), E> {
        if !(self.name == "apiConstructor" && k == "getCredentials") {
            self.event(&format!("set.{k}"), std::slice::from_ref(&v));
        }
        self.writes.borrow_mut().push((k, v.clone()));
        *self.receiver.borrow_mut() = o.clone();
        field_set(
            o,
            k,
            if self.name == "apiConstructor" && self.i == 9 && k == "gammaXhr" {
                self.value("altGamma")
            } else {
                v
            },
        );
        Ok(())
    }
    fn spread_params(&self, _: &V, _: V, _: V) -> Result<V, E> {
        unreachable!()
    }
    fn array(&self, v: Vec<Option<V>>) -> V {
        list(None, v)
    }
    fn map_output(&self, _: &V, _: usize) -> Result<V, E> {
        unreachable!()
    }
    fn map_set(&self, _: &V, _: usize, _: V) -> Result<(), E> {
        unreachable!()
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::Array(..))
    }
    fn length(&self, _: &V) -> Result<usize, E> {
        unreachable!()
    }
    fn at(&self, _: &V, _: usize) -> Result<Option<V>, E> {
        unreachable!()
    }
    fn push_spread(&self, _: &V, _: &V) -> Result<(), E> {
        unreachable!()
    }
    fn less(&self, _: &V, _: &V) -> Result<bool, E> {
        unreachable!()
    }
    fn greater_equal(&self, _: &V, _: &V) -> Result<bool, E> {
        unreachable!()
    }
    fn add(&self, _: &V, _: &V) -> Result<V, E> {
        unreachable!()
    }
    fn slice(&self, _: &V, _: &V) -> Result<V, E> {
        unreachable!()
    }
    fn order_to_json(&self, _: V, _: V, _: V) -> Result<V, E> {
        unreachable!()
    }
    fn new_error(&self, m: &'static str) -> E {
        Rc::new(Failure {
            marker: None,
            message: text(m),
        })
    }
    fn error_message(&self, e: &E) -> Result<V, E> {
        Ok(e.message.clone())
    }
}
impl AuthHost for Host {
    type Constructor = (&'static str, usize);
    fn capture_constructor(&self, name: &'static str) -> Result<Self::Constructor, E> {
        let g = self.ctor_generation.get();
        self.captures.borrow_mut().push((name, g));
        Ok((name, g))
    }
    fn construct(&self, (name, g): Self::Constructor, args: Vec<V>) -> Result<V, E> {
        match name {
            "ethers.Wallet" => {
                self.event("new Wallet", &args);
                if self.i == 7 {
                    return Err(self.error.clone());
                }
                Ok(self.value("wallet"))
            }
            "SDK.OrderBuilder" => {
                self.event(
                    if g == 0 {
                        "new OrderBuilder"
                    } else {
                        "new AlternateBuilder"
                    },
                    &args,
                );
                let result = self.signer.borrow().as_ref().unwrap().get_address()?;
                self.bridge_result("getAddress", result);
                Ok(self.value("builder"))
            }
            "BaseWebSocket" => {
                self.event("new BaseWebSocket", &args);
                if self.i == 7 {
                    return Err(self.error.clone());
                }
                let base = self.value("base");
                field_set(&base, "parent", args[0].clone());
                *self.receiver.borrow_mut() = base.clone();
                Ok(base)
            }
            _ => panic!("bound constructor {name}"),
        }
    }
    fn clob_auth_types(&self) -> Result<V, E> {
        Ok(self.value("authTypes"))
    }
    fn signer(&self, signer: WalletSigner<Self>) -> Result<V, E> {
        *self.signer.borrow_mut() = Some(signer);
        self.object(vec![
            ("getAddress", self.fn_value("getAddress")),
            ("_signTypedData", self.fn_value("_signTypedData")),
        ])
    }
    fn sdk_side(&self, buy: bool) -> V {
        text(if buy { "BUY" } else { "SELL" })
    }
    fn string_value(&self, u: Vec<u16>) -> V {
        V::Text(u)
    }
    fn uppercase(&self, v: V) -> Result<V, E> {
        let V::Text(t) = v else {
            return Err(self.error.clone());
        };
        Ok(text(&String::from_utf16(&t).unwrap().to_uppercase()))
    }
    fn replace_ascii(&self, v: V, from: u16, to: u16) -> Result<V, E> {
        let V::Text(mut t) = v else {
            return Err(self.error.clone());
        };
        for u in &mut t {
            if *u == from {
                *u = to;
            }
        }
        Ok(V::Text(t))
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(v, V::U)
    }
    fn date_now(&self) -> Result<f64, E> {
        self.event("Date.now", &[]);
        Ok(numeric(&self.value("now")))
    }
    fn retry_delay(&self, d: Rc<dyn Fn(V) -> Result<V, E>>) -> Result<V, E> {
        self.delays.borrow_mut().push(d);
        Ok(self.fn_value("retryDelay"))
    }
    fn multiply(&self, a: V, b: V) -> Result<V, E> {
        Ok(V::Number(numeric(&a) * numeric(&b)))
    }
    fn capture_value(&self, receiver: V, method: &'static str) -> Result<Call, E> {
        let namespace = if same(&receiver, &self.value("wallet")) {
            "wallet"
        } else if same(&receiver, &self.value("builder")) {
            "builder"
        } else {
            "value"
        };
        Ok(Call {
            namespace,
            method,
            receiver: Some(receiver),
            generation: self.generation.get(),
        })
    }
    fn call_sync(&self, c: Call, args: Vec<V>) -> Result<V, E> {
        let Call {
            namespace,
            method,
            receiver,
            generation,
        } = c;
        match (namespace, method) {
            ("value", "toString") => {
                let v = receiver.unwrap();
                if self.nullish(&v) {
                    return Err(self.error.clone());
                }
                Ok(V::Text(self.string(&v)?))
            }
            ("wallet", method) => {
                self.wallet_receivers.borrow_mut().push(receiver.unwrap());
                self.event(&format!("wallet.{method}"), &args);
                if self.i == 7 && self.name == "signApiKeyDerivationMessage" {
                    return Err(self.error.clone());
                }
                if method == "getAddress" {
                    return Ok(self.value(if generation == 0 {
                        "address"
                    } else {
                        "address1"
                    }));
                }
                assert_eq!(method, "signTypedData");
                if self.i == 8 && self.name == "signApiKeyDerivationMessage" {
                    let V::Array(_, items) = raw_field(&args[1], "ClobAuth") else {
                        panic!("typed schema")
                    };
                    field_set(items.borrow()[0].as_ref().unwrap(), "name", text("changed"));
                }
                Ok(self.value(if generation == 0 {
                    "signature"
                } else {
                    "signature1"
                }))
            }
            ("builder", "buildOrder") => {
                self.event("builder.buildOrder", &args);
                if self.i == 8 {
                    self.generation.set(1);
                }
                let signer = self.signer.borrow();
                let signer = signer.as_ref().unwrap();
                let a = signer.get_address()?;
                self.bridge_result("getAddress", a);
                let domain = self.object(vec![("domain", self.number(1.))])?;
                let types = self.object(vec![("types", self.number(2.))])?;
                let value = self.object(vec![("value", self.number(3.))])?;
                *self.bridge_inputs.borrow_mut() =
                    vec![domain.clone(), types.clone(), value.clone()];
                let s = signer.sign_typed_data(domain, types, value)?;
                self.bridge_result("signTypedData", s);
                if self.i == 7 {
                    return Err(self.error.clone());
                }
                Ok(self.value("signed"))
            }
            ("SDK", "isV2Order") => {
                self.event("isV2Order", &args);
                Ok(V::Bool(self.i != 10))
            }
            ("ethers", method) => {
                self.event(&format!("ethers.{method}"), &args);
                if self.i == 7 && method == "decodeBase64" {
                    return Err(self.error.clone());
                }
                match method {
                    "decodeBase64" => {
                        use base64::{
                            Engine, alphabet,
                            engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
                        };
                        let s = String::from_utf16(&self.string(&args[0])?).unwrap();
                        let engine = GeneralPurpose::new(
                            &alphabet::STANDARD,
                            GeneralPurposeConfig::new()
                                .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                                .with_decode_allow_trailing_bits(true),
                        );
                        Ok(V::Bytes(engine.decode(s.trim_end_matches('=')).unwrap()))
                    }
                    "toUtf8Bytes" => Ok(V::Bytes(
                        String::from_utf16(&self.string(&args[0])?)
                            .unwrap()
                            .into_bytes(),
                    )),
                    "computeHmac" => {
                        assert!(self.is_text(&args[0], "sha256"));
                        let (V::Bytes(key), V::Bytes(message)) = (&args[1], &args[2]) else {
                            panic!("native crypto byte args")
                        };
                        Ok(text(&format!(
                            "0x{}",
                            safe_cex_crypto::hmac_sha256(
                                key,
                                message,
                                safe_cex_crypto::Encoding::Hex
                            )
                        )))
                    }
                    "getBytes" => {
                        let s = String::from_utf16(&self.string(&args[0])?).unwrap();
                        let raw = s.strip_prefix("0x").unwrap();
                        Ok(V::Bytes(
                            (0..raw.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&raw[i..i + 2], 16).unwrap())
                                .collect(),
                        ))
                    }
                    "encodeBase64" => {
                        use base64::{Engine, engine::general_purpose::STANDARD};
                        let V::Bytes(b) = &args[0] else {
                            panic!("native encode bytes")
                        };
                        Ok(text(&STANDARD.encode(b)))
                    }
                    _ => panic!("bound ethers method {method}"),
                }
            }
            ("imports", "createPolymarketAPI") => {
                self.event("createPolymarketAPI", &args);
                if self.i == 7 {
                    return Err(self.error.clone());
                }
                self.event("callback.credentials", &[]);
                self.event("callback.walletAddress", &[]);
                Ok(self.value("xhr"))
            }
            ("imports", "safeCexAxiosCreate") => {
                self.event("safeCexAxiosCreate", &args);
                let n = self.creates.get() + 1;
                self.creates.set(n);
                if self.i == 6 && n == 2 {
                    return Err(self.error.clone());
                }
                Ok(self.value(if n == 1 { "gamma" } else { "data" }))
            }
            ("imports", "networkErrorOnRead") => {
                self.event("networkErrorOnRead", &args);
                Ok(self.value("condition"))
            }
            ("imports", "retry") => {
                self.event("retry", &args);
                let delays = self.delays.borrow();
                let delay = delays.last().unwrap();
                for count in [
                    V::Number(0.),
                    V::Number(-0.),
                    V::Number(0.5),
                    V::Number(-2.),
                    text("3"),
                ] {
                    let result = delay(count.clone())?;
                    self.event("retryDelay", &[count, result]);
                }
                if self.i == 5 {
                    return Err(self.error.clone());
                }
                Ok(V::U)
            }
            _ => panic!("bound call {namespace}.{method}"),
        }
    }
}
// Independent digest expectations generated with Node's crypto.createHmac,
// fixed key byte vectors and literal messages, not the Rust/ethers artifact.
const HASH: [&str; 12] = [
    "ixwY7--N7BrJat-mex54Cl2jJS-QAc_gq-QEhYoJXhk=",
    "oMSlKhG1oRUW0kE5zuBfbFkMwoGnWgh-tXNzp37kXTw=",
    "gCWLQIut55m_5Mf2ezTyRPve6EMZSAosuZEh0Ce9HpY=",
    "cYM9SKujSmFYhuEH8P259SBry7obQlPMLp11KvCxAtM=",
    "mbNGbPZ3z1UpfnedZNLWJlBl0chByKHQidx5IRvL2xU=",
    "Fuj3T07cR5US4GHVy-joB2LFHB6jYv9EECdW8Jmr_28=",
    "ie7HvICeH5806GTbr-TcTmxcnqZOA1j4JbpyHaW7d7o=",
    "TPy8iARlr7sxk8tXk7C00wdpGuHSZ3Utpu_LRWO9L5A=",
    "zXC-yE77c0FxXoa1DC_PXe8iYuWgGfVNfoF-LD9CSzQ=",
    "LWnV0BEzZzXberNMeBlxeh6QzM0tgoxDtK91RBm_-VU=",
    "kjttj_fTrPJAXIl4Y2aIDFEhT8nkiV6NZrefk3tiLbM=",
    "A0RdNgJP-Skae_k1_zDfooqdPiOykDx9AL88XZQRjNQ=",
];
const WORDS: [&str; 12] = [
    "A/one",
    "",
    "é",
    "ß",
    "🦀",
    "__proto__",
    "constructor",
    "failure",
    "x\0y",
    "late",
    "last",
    "end",
];
const TIMESTAMPS: [&str; 12] = [
    "123",
    "0",
    "-1",
    "1e+21",
    "NaN",
    "Infinity",
    "-Infinity",
    "42",
    "undefined",
    "1700000000",
    "0",
    "undefined",
];
fn key_bytes(i: usize) -> Vec<u8> {
    match i {
        0 | 1 => b"key".to_vec(),
        2 => vec![],
        3 => vec![255],
        4 => vec![251, 255],
        5..=11 => (1..=(i - 4) as u8).collect(),
        _ => unreachable!(),
    }
}
fn n(v: &V, expected: f64, label: &str) {
    let V::Number(actual) = v else {
        panic!("native number arm {label}")
    };
    if expected.is_nan() {
        assert!(actual.is_nan(), "native NaN {label}");
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits(), "native bits {label}");
    }
}
fn t(v: &V, expected: &str, label: &str) {
    let V::Text(actual) = v else {
        panic!("native text arm {label}")
    };
    assert_eq!(actual, &units(expected), "native text {label}");
}
fn keys(v: &V, expected: &[&str], label: &str) {
    let V::Object(_, fields) = v else {
        panic!("native record {label}")
    };
    assert_eq!(
        fields
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>(),
        expected,
        "native keys {label}"
    );
}
fn native(h: &Host, result: &Result<V, E>) {
    let i = h.i;
    let name = h.name.as_str();
    let failed = i == 7
        || (name == "apiConstructor" && matches!(i, 5 | 6))
        || (name == "createSignedOrder" && i == 10);
    assert_eq!(
        result.is_err(),
        failed,
        "native independent error branch {name}/{i}"
    );
    if let Err(e) = result {
        if name == "createSignedOrder" && i == 10 {
            assert!(!Rc::ptr_eq(e, &h.error), "native V2 refusal fresh error");
            t(
                &e.message,
                "Polymarket official client returned a non-V2 order",
                "V2 refusal",
            );
        } else {
            assert!(Rc::ptr_eq(e, &h.error), "native original error identity");
        }
    }
    let frames = h.native.borrow();
    let selected = |tag: &str| {
        frames
            .iter()
            .filter(|(name, _)| name == tag)
            .map(|(_, args)| args)
            .collect::<Vec<_>>()
    };
    match name {
        "createWallet" => {
            assert_eq!(
                h.captures.borrow().as_slice(),
                &[("ethers.Wallet", 0)],
                "native wallet ctor captured"
            );
            let call = selected("new Wallet");
            assert_eq!(call.len(), 1);
            assert_eq!(call[0].len(), 1);
            t(&call[0][0], WORDS[i], "wallet private key");
            if let Ok(out) = result {
                assert!(
                    same(out, &h.value("wallet")),
                    "native original wallet return alias"
                );
            }
        }
        "createSignedOrder" => {
            assert_eq!(
                h.captures.borrow().as_slice(),
                &[("SDK.OrderBuilder", 0)],
                "native constructor captured before parameter getter"
            );
            let ct = selected("new OrderBuilder");
            assert_eq!(ct.len(), 1, "native original builder constructor");
            assert_eq!(ct[0].len(), 4);
            keys(
                &ct[0][0],
                &["getAddress", "_signTypedData"],
                "signer bridge",
            );
            assert!(matches!(
                raw_field(&ct[0][0], "getAddress"),
                V::Function(..)
            ));
            assert!(matches!(
                raw_field(&ct[0][0], "_signTypedData"),
                V::Function(..)
            ));
            n(&ct[0][1], 137., "builder chain id");
            n(
                &ct[0][2],
                if i == 2 {
                    -0.
                } else if i < 2 {
                    0.
                } else {
                    (i % 4) as f64
                },
                "builder signature type",
            );
            match i {
                3 => assert!(matches!(ct[0][3], V::Null)),
                4 => assert!(matches!(ct[0][3], V::U)),
                _ => t(&ct[0][3], &format!("funder-{}", WORDS[i]), "builder funder"),
            };
            let build = selected("builder.buildOrder");
            assert_eq!(build.len(), 1);
            assert_eq!(build[0].len(), 3);
            keys(
                &build[0][0],
                &["tokenID", "price", "size", "side"],
                "native builder order",
            );
            t(
                &raw_field(&build[0][0], "tokenID"),
                &format!("token-{}", WORDS[i]),
                "builder tokenID",
            );
            n(
                &raw_field(&build[0][0], "price"),
                if i == 1 { -0. } else { 0.375 },
                "builder price",
            );
            n(&raw_field(&build[0][0], "size"), 0.625, "builder size");
            t(
                &raw_field(&build[0][0], "side"),
                if matches!(i, 2 | 3) { "SELL" } else { "BUY" },
                "builder SDK side",
            );
            keys(&build[0][1], &["tickSize", "negRisk"], "builder options");
            t(
                &raw_field(&build[0][1], "tickSize"),
                if i == 4 { "0.0025" } else { "0.005" },
                "builder tick size",
            );
            assert!(
                matches!(raw_field(&build[0][1],"negRisk"),V::Bool(b)if b==(i%2==1)),
                "native neg-risk option"
            );
            n(&build[0][2], 2., "builder version");
            let late_address = if i == 8 {
                "changed-address".to_owned()
            } else {
                format!("address-{}", WORDS[i])
            };
            let late_signature = if i == 8 {
                "changed-signature".to_owned()
            } else {
                format!("signature-{}", WORDS[i])
            };
            let returns = h.bridge_returns.borrow();
            assert_eq!(returns.len(), 3, "native bridge callback count");
            t(
                &returns[0],
                &format!("address-{}", WORDS[i]),
                "bridge eager address",
            );
            t(&returns[1], &late_address, "bridge current address");
            t(
                &returns[2],
                &late_signature,
                "bridge current typed signature",
            );
            assert_eq!(h.wallet_receivers.borrow().len(), 3);
            for r in h.wallet_receivers.borrow().iter() {
                assert!(
                    same(r, &h.value("wallet")),
                    "native bridge original wallet receiver"
                );
            }
            let signatures = selected("wallet.signTypedData");
            assert_eq!(signatures.len(), 1);
            assert_eq!(signatures[0].len(), 3);
            for (index, key) in ["domain", "types", "value"].into_iter().enumerate() {
                assert!(
                    same(&signatures[0][index], &h.bridge_inputs.borrow()[index]),
                    "native bridge original forwarded argument alias"
                );
                keys(&signatures[0][index], &[key], "bridge forwarded args");
                n(
                    &raw_field(&signatures[0][index], key),
                    (index + 1) as f64,
                    "bridge forwarded value",
                );
            }
            assert_eq!(selected("isV2Order").len(), usize::from(i != 7));
            if i != 7 {
                assert!(
                    same(&selected("isV2Order")[0][0], &h.value("signed")),
                    "native guard original signed alias"
                );
            }
            if let Ok(out) = result {
                assert!(
                    same(out, &h.value("signed")),
                    "native signed original return alias"
                );
            }
        }
        "generateHmacSignature" | "buildL2Headers" => {
            let padded = [
                "a2V5",
                "a2V5====",
                "",
                "/w==",
                "+/8=",
                "AQ==",
                "AQI=",
                "AQID",
                "AQIDBA==",
                "AQIDBAU=",
                "AQIDBAUG",
                "AQIDBAUGBw==",
            ];
            let dec = selected("ethers.decodeBase64");
            assert_eq!(dec.len(), 1);
            t(&dec[0][0], padded[i], "native padded secret");
            let computes = selected("ethers.computeHmac");
            assert_eq!(computes.len(), usize::from(i != 7));
            assert_eq!(
                selected("Date.now").len(),
                usize::from(name == "buildL2Headers" && matches!(i, 8 | 11)),
                "native timestamp default clock count"
            );
            if i != 7 {
                let c = computes[0];
                assert_eq!(c.len(), 3);
                t(&c[0], "sha256", "hmac algorithm");
                let V::Bytes(key) = &c[1] else {
                    panic!("native hmac decoded key arm")
                };
                assert_eq!(key, &key_bytes(i), "native independent decoded key bytes");
                let ts = if name == "buildL2Headers" && i == 8 {
                    "1700000000"
                } else if name == "buildL2Headers" && i == 11 {
                    "-1"
                } else {
                    TIMESTAMPS[i]
                };
                let upper = [
                    "GET", "POST", "MIXED", "SS", "É", "GET", "PUT", "GET", "GET", "GET", "GET",
                    "GET",
                ];
                let body = match i {
                    2 => "null",
                    3 => "false",
                    4 => "0",
                    8 => "body",
                    _ => "",
                };
                let expected = format!("{}{}{}{}{}", ts, upper[i], "/path/", WORDS[i], body);
                let V::Bytes(message) = &c[2] else {
                    panic!("native hmac decoded message arm")
                };
                assert_eq!(
                    message,
                    expected.as_bytes(),
                    "native independent UTF8 message bytes"
                );
                let hash = if name == "buildL2Headers" && i == 8 {
                    "SMp1hFsjgOVVpG56Ms6DXXZwlhThU6P0t6Iviu_ZTDk="
                } else if name == "buildL2Headers" && i == 11 {
                    "R8iU0rXG0--HP3LmaQGPZLyGDtcuR1wE37zO6U5Qa00="
                } else {
                    HASH[i]
                };
                let out = result.as_ref().unwrap();
                if name == "generateHmacSignature" {
                    t(out, hash, "HMAC native digest");
                } else {
                    keys(
                        out,
                        &[
                            "POLY_ADDRESS",
                            "POLY_SIGNATURE",
                            "POLY_TIMESTAMP",
                            "POLY_API_KEY",
                            "POLY_PASSPHRASE",
                        ],
                        "L2 headers",
                    );
                    t(
                        &raw_field(out, "POLY_ADDRESS"),
                        &format!("address-{}", WORDS[i]),
                        "L2 wallet address",
                    );
                    t(
                        &raw_field(out, "POLY_SIGNATURE"),
                        hash,
                        "L2 native signature",
                    );
                    t(&raw_field(out, "POLY_TIMESTAMP"), ts, "L2 timestamp");
                    t(
                        &raw_field(out, "POLY_API_KEY"),
                        &format!("key-{}", WORDS[i]),
                        "L2 api key",
                    );
                    t(
                        &raw_field(out, "POLY_PASSPHRASE"),
                        &format!("phrase-{}", WORDS[i]),
                        "L2 passphrase",
                    );
                }
            }
        }
        "signApiKeyDerivationMessage" => {
            let count = if i == 7 {
                0
            } else if i == 8 {
                2
            } else {
                1
            };
            assert_eq!(
                selected("wallet.getAddress").len(),
                if i == 8 { 2 } else { 1 }
            );
            let signs = selected("wallet.signTypedData");
            assert_eq!(signs.len(), count, "native auth sign call count");
            for args in signs {
                assert_eq!(args.len(), 3);
                keys(&args[0], &["name", "version", "chainId"], "auth domain");
                t(
                    &raw_field(&args[0], "name"),
                    "ClobAuthDomain",
                    "auth domain name",
                );
                t(&raw_field(&args[0], "version"), "1", "auth domain version");
                n(&raw_field(&args[0], "chainId"), 137., "auth chain id");
                assert!(
                    same(&args[1], &h.value("authTypes")),
                    "native shared auth types identity"
                );
                keys(&args[1], &["ClobAuth"], "auth types");
                let V::Array(_, items) = raw_field(&args[1], "ClobAuth") else {
                    panic!("native auth schema array")
                };
                assert_eq!(
                    items.borrow().len(),
                    4,
                    "native independent auth field count"
                );
                for (j, (name, kind)) in [
                    (if i == 8 { "changed" } else { "address" }, "address"),
                    ("timestamp", "string"),
                    ("nonce", "uint256"),
                    ("message", "string"),
                ]
                .into_iter()
                .enumerate()
                {
                    let field = items.borrow()[j].as_ref().unwrap().clone();
                    keys(&field, &["name", "type"], "auth field schema");
                    t(&raw_field(&field, "name"), name, "auth field name");
                    t(&raw_field(&field, "type"), kind, "auth field type");
                }
                keys(
                    &args[2],
                    &["address", "timestamp", "nonce", "message"],
                    "auth value",
                );
                t(
                    &raw_field(&args[2], "address"),
                    &format!("address-{}", WORDS[i]),
                    "auth address",
                );
                n(
                    &raw_field(&args[2], "nonce"),
                    if i == 1 { -0. } else { i as f64 },
                    "auth nonce",
                );
                t(
                    &raw_field(&args[2], "message"),
                    "This message attests that I control the given wallet",
                    "auth attestation message",
                );
                t(
                    &raw_field(&args[2], "timestamp"),
                    if matches!(i, 8 | 11) {
                        "0"
                    } else {
                        TIMESTAMPS[i]
                    },
                    "auth timestamp string",
                );
            }
            if let Ok(out) = result {
                keys(out, &["address", "signature", "timestamp"], "auth return");
                t(
                    &raw_field(out, "address"),
                    &format!("address-{}", WORDS[i]),
                    "auth return address",
                );
                t(
                    &raw_field(out, "signature"),
                    &format!("signature-{}", WORDS[i]),
                    "auth return signature",
                );
                n(
                    &raw_field(out, "timestamp"),
                    if matches!(i, 8 | 11) {
                        0.
                    } else {
                        numeric(&h.value("timestamp"))
                    },
                    "auth return original timestamp bits",
                );
            }
        }
        "apiConstructor" => {
            let expected: &[&str] = match i {
                7 => &["getCredentials", "exchange"],
                5 | 6 => &["getCredentials", "exchange", "xhr", "gammaXhr"],
                _ => &["getCredentials", "exchange", "xhr", "gammaXhr", "dataXhr"],
            };
            assert_eq!(
                h.writes
                    .borrow()
                    .iter()
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>(),
                expected,
                "native independent API assignment prefix"
            );
            let recv = h.receiver.borrow();
            keys(&recv, expected, "partial API receiver schema");
            assert!(
                same(
                    &raw_field(&recv, "getCredentials"),
                    &h.value("getCredentials")
                ),
                "native API original credential callback alias"
            );
            assert!(
                same(&raw_field(&recv, "exchange"), &h.value("exchange")),
                "native API original exchange alias"
            );
            let factories = selected("createPolymarketAPI");
            assert_eq!(factories.len(), 1);
            assert_eq!(factories[0].len(), 4);
            for (j, k) in ["options", "exchange", "getCredentials", "getWalletAddress"]
                .into_iter()
                .enumerate()
            {
                assert!(
                    same(&factories[0][j], &h.value(k)),
                    "native API original factory argument {k}"
                );
            }
            let creates = selected("safeCexAxiosCreate");
            assert_eq!(
                creates.len(),
                if i == 7 {
                    0
                } else if i == 5 {
                    1
                } else {
                    2
                },
                "native separate client count"
            );
            for (j, args) in creates.iter().enumerate() {
                assert_eq!(args.len(), 3);
                keys(
                    &args[0],
                    &["baseURL", "timeout", "headers"],
                    "client config",
                );
                t(
                    &raw_field(&args[0], "baseURL"),
                    if j == 0 {
                        "https://gamma-api.polymarket.com"
                    } else {
                        "https://data-api.polymarket.com"
                    },
                    "client URL",
                );
                n(&raw_field(&args[0], "timeout"), 30000., "client timeout");
                let headers = raw_field(&args[0], "headers");
                keys(
                    &headers,
                    if j == 0 {
                        &["Content-Type", "Accept"]
                    } else {
                        &["Accept"]
                    },
                    "client headers",
                );
                t(
                    &raw_field(&headers, "Accept"),
                    "application/json",
                    "client accept",
                );
                if j == 0 {
                    t(
                        &raw_field(&headers, "Content-Type"),
                        "application/json",
                        "gamma content type",
                    );
                }
                assert!(
                    same(&args[1], &h.value("options")),
                    "native client original options alias"
                );
                assert!(
                    same(&args[2], &h.value("exchange")),
                    "native client original exchange alias"
                );
            }
            let retries = selected("retry");
            let count = if i == 7 {
                0
            } else if matches!(i, 5 | 6) {
                1
            } else {
                2
            };
            assert_eq!(retries.len(), count);
            assert_eq!(h.delays.borrow().len(), count);
            assert_eq!(selected("networkErrorOnRead").len(), count);
            assert_eq!(
                selected("retryDelay").len(),
                count * 5,
                "native independent delay callback floor"
            );
            for (j, args) in retries.iter().enumerate() {
                assert_eq!(args.len(), 2);
                assert!(
                    same(
                        &args[0],
                        &h.value(if j == 0 && i == 9 {
                            "altGamma"
                        } else if j == 0 {
                            "gamma"
                        } else {
                            "data"
                        })
                    ),
                    "native retry current receiver client alias"
                );
                keys(
                    &args[1],
                    &["retries", "retryCondition", "retryDelay"],
                    "retry config",
                );
                n(&raw_field(&args[1], "retries"), 3., "retry count");
                assert!(
                    same(
                        &raw_field(&args[1], "retryCondition"),
                        &h.value("condition")
                    ),
                    "native original retry predicate alias"
                );
                assert!(matches!(raw_field(&args[1], "retryDelay"), V::Function(..)));
            }
            for (j, args) in selected("retryDelay").iter().enumerate() {
                let expected = [0., -0., 500., -2000., 3000.][j % 5];
                n(&args[1], expected, "retry delay result");
            }
            if i != 7 {
                assert!(
                    same(&raw_field(&recv, "xhr"), &h.value("xhr")),
                    "native CLOB client alias"
                );
                assert!(
                    same(
                        &raw_field(&recv, "gammaXhr"),
                        &h.value(if i == 9 { "altGamma" } else { "gamma" })
                    ),
                    "native gamma client alias"
                );
            }
            if let Ok(out) = result {
                assert!(
                    same(out, &h.value("receiver")),
                    "native original supplied API receiver alias"
                );
                assert!(
                    same(&raw_field(out, "dataXhr"), &h.value("data")),
                    "native data client alias"
                );
            }
        }
        "privateConstructor" | "publicConstructor" => {
            let ctor = selected("new BaseWebSocket");
            assert_eq!(ctor.len(), 1);
            assert_eq!(ctor[0].len(), 1);
            assert!(
                same(&ctor[0][0], &h.value("parent")),
                "native original public/private parent alias"
            );
            assert_eq!(
                h.writes.borrow().len(),
                usize::from(name == "privateConstructor" && i != 7),
                "native independent private assignment count"
            );
            if let Ok(out) = result {
                assert!(
                    same(out, &h.value("base")),
                    "native original base receiver alias"
                );
                assert!(
                    same(&raw_field(out, "parent"), &h.value("parent")),
                    "native stored original parent alias"
                );
                if name == "privateConstructor" {
                    keys(
                        out,
                        &["parent", "getCredentials"],
                        "private constructor fields",
                    );
                    assert!(
                        same(
                            &raw_field(out, "getCredentials"),
                            &h.value("getCredentials")
                        ),
                        "native private credential callback alias"
                    );
                } else {
                    keys(out, &["parent"], "public constructor fields");
                }
            }
        }
        _ => unreachable!(),
    }
}
fn oracle() -> &'static J {
    static DATA: OnceLock<J> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/auth_init_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn run_family(name: &str) {
    let mut wire = vec![];
    for i in 0..12 {
        let case = oracle()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name && c["i"] == i)
            .unwrap();
        let h = Rc::new(Host::new(case));
        let result = match name {
            "createWallet" => create_wallet(h.as_ref(), h.value("key")),
            "generateHmacSignature" => generate_hmac_signature(
                h.as_ref(),
                h.value("secret"),
                V::Text(h.string(&h.value("timestamp")).unwrap()),
                h.value("method"),
                h.value("path"),
                h.value("body"),
            ),
            "buildL2Headers" => build_l2_headers(
                h.as_ref(),
                h.value("credentials"),
                h.value("address"),
                h.value("method"),
                h.value("path"),
                h.value("body"),
                h.value("timestamp"),
            ),
            "apiConstructor" => initialize_api(
                h.clone(),
                h.value("receiver"),
                h.value("options"),
                h.value("exchange"),
                h.value("getCredentials"),
                h.value("getWalletAddress"),
            ),
            "privateConstructor" => {
                initialize_private(h.as_ref(), h.value("parent"), h.value("getCredentials"))
            }
            "publicConstructor" => initialize_public(h.as_ref(), h.value("parent")),
            "createSignedOrder" | "signApiKeyDerivationMessage" => {
                let mut observe: Pin<Box<dyn Future<Output = Result<V, E>>>> =
                    if name == "createSignedOrder" {
                        Box::pin(create_signed_order(
                            h.clone(),
                            h.value("wallet"),
                            h.value("params"),
                        ))
                    } else {
                        Box::pin(sign_api_key_derivation_message(
                            h.clone(),
                            h.value("wallet"),
                            h.value("nonce"),
                            if matches!(i, 8 | 11) {
                                V::Number(0.)
                            } else {
                                h.value("timestamp")
                            },
                        ))
                    };
                h.finish();
                let mut cx = Context::from_waker(Waker::noop());
                let Poll::Ready(mut result) = observe.as_mut().poll(&mut cx) else {
                    panic!("native settled observer")
                };
                if name == "signApiKeyDerivationMessage" && i == 8 {
                    let mut second = Box::pin(sign_api_key_derivation_message(
                        h.clone(),
                        h.value("wallet"),
                        h.value("nonce"),
                        V::Number(0.),
                    ));
                    h.finish();
                    let Poll::Ready(next) = second.as_mut().poll(&mut cx) else {
                        panic!("native second settled observer")
                    };
                    result = next;
                }
                result
            }
            _ => unreachable!(),
        };
        native(&h, &result);
        wire.push((h, result, case));
    }
    // Every fixture's native assertions above precedes every wire projection below.
    for (h, result, case) in wire {
        let observed = match result {
            Ok(v) => json!({"value":project(&v)}),
            Err(e) => json!({"error":{"marker":e.marker,"message":project(&e.message)}}),
        };
        assert_eq!(
            observed, case["result"],
            "wire source result {name}/{}",
            h.i
        );
        assert_eq!(
            *h.events.borrow(),
            case["events"].as_array().unwrap().clone(),
            "wire source events {name}/{}",
            h.i
        );
        let writes = h
            .writes
            .borrow()
            .iter()
            .filter(|(key, _)| !(name == "apiConstructor" && *key == "getCredentials"))
            .map(|(key, v)| json!([key, project(v)]))
            .collect::<Vec<_>>();
        assert_eq!(
            writes,
            case["writes"].as_array().unwrap().clone(),
            "wire source writes"
        );
        assert_eq!(
            project(&h.receiver.borrow()),
            case["receiver"],
            "wire partial receiver"
        );
        assert_eq!(
            project(&h.value("authTypes")),
            case["authTypes"],
            "wire shared types"
        );
    }
}
#[test]
fn wallet_native() {
    run_family("createWallet");
}
#[test]
fn signed_order_native() {
    run_family("createSignedOrder");
}
#[test]
fn hmac_native() {
    run_family("generateHmacSignature");
}
#[test]
fn headers_native() {
    run_family("buildL2Headers");
}
#[test]
fn auth_signature_native() {
    run_family("signApiKeyDerivationMessage");
}
#[test]
fn api_constructor_native() {
    run_family("apiConstructor");
}
#[test]
fn private_constructor_native() {
    run_family("privateConstructor");
}
#[test]
fn public_constructor_native() {
    run_family("publicConstructor");
}
