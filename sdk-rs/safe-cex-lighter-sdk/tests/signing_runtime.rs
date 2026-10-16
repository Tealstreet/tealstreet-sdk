use num_bigint::BigInt;
use safe_cex_lighter_sdk::{key_runtime::*, signing_runtime::*};
use serde_json::{Value as J, json};
use std::{
    cell::RefCell,
    fmt,
    future::Future,
    pin::Pin,
    rc::{Rc, Weak},
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Big(BigInt),
    Object(Rc<Obj>),
    Function(Rc<()>),
    Thenable(bool),
    Promise(Rc<Promise>),
}
struct Obj {
    fields: RefCell<Vec<(String, V)>>,
    inherited_error: bool,
    error_fault: bool,
}
struct Promise {
    future: RefCell<Option<KeyFuture<V, Rc<E>>>>,
    result: RefCell<Option<Result<V, Rc<E>>>>,
}
#[derive(Debug)]
struct E {
    name: &'static str,
    message: Vec<u16>,
    cause: Option<Rc<E>>,
}
impl fmt::Debug for V {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        encode(self).fmt(f)
    }
}
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    V::Text(units(s))
}
fn object(fields: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(Obj {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
        inherited_error: false,
        error_fault: false,
    }))
}
fn decode(j: &J) -> V {
    match j {
        J::Null => V::Null,
        J::Bool(b) => V::Bool(*b),
        J::Array(_) => panic!("fixture declaration"),
        J::Object(o) if o.contains_key("undefined") => V::Undefined,
        J::Object(o) if o.contains_key("text") => V::Text(
            o["text"]
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_u64().unwrap() as u16)
                .collect(),
        ),
        J::Object(o) if o.contains_key("bits") => V::Number(if o["bits"] == "NaN" {
            f64::NAN
        } else {
            f64::from_bits(u64::from_str_radix(o["bits"].as_str().unwrap(), 16).unwrap())
        }),
        J::Object(o) if o.contains_key("bigint") => {
            V::Big(o["bigint"].as_str().unwrap().parse().unwrap())
        }
        J::Object(o) => V::Object(Rc::new(Obj {
            fields: RefCell::new(o.iter().map(|(k, v)| (k.clone(), decode(v))).collect()),
            inherited_error: false,
            error_fault: false,
        })),
        _ => panic!("raw fixture value {j}"),
    }
}
fn encode(v: &V) -> J {
    match v {
        V::Undefined => json!({"undefined":true}),
        V::Null => J::Null,
        V::Bool(b) => json!(b),
        V::Number(n) => {
            json!({"bits":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
        }
        V::Text(s) => json!({"text":s}),
        V::Big(b) => json!({"bigint":b.to_string()}),
        V::Object(o) => J::Object(
            o.fields
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), encode(v)))
                .collect(),
        ),
        V::Function(_) => json!({"function":true}),
        V::Thenable(_) => json!({"then":{"function":true}}),
        V::Promise(_) => json!({"nativePromise":true}),
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) => true,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Function(a), V::Function(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn corpus() -> &'static J {
    static C: OnceLock<J> = OnceLock::new();
    C.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/signing_runtime_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "SDK-SIGN source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
struct Host {
    input: J,
    root: RefCell<Weak<Host>>,
    receiver: V,
    tx: V,
    hash: V,
    signed: V,
    verified: V,
    override_result: RefCell<Option<V>>,
    api: Rc<()>,
    failure: Rc<E>,
    calls: RefCell<Vec<J>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
}
impl Host {
    fn setting(&self, k: &str) -> String {
        match decode(self.input.get(k).unwrap_or(&json!({"undefined":true}))) {
            V::Text(t) => String::from_utf16(&t).unwrap(),
            _ => String::new(),
        }
    }
    fn flag(&self, k: &str) -> bool {
        self.input[k] == true
    }
    fn arg(&self, k: &str) -> V {
        decode(self.input.get(k).unwrap_or(&json!({"undefined":true})))
    }
    fn handle(&self) -> Rc<Host> {
        self.root.borrow().upgrade().expect("owned runtime host")
    }
    fn trace(&self, v: Vec<V>) {
        self.calls
            .borrow_mut()
            .push(J::Array(v.iter().map(encode).collect()));
    }
    fn fail(&self, k: &str) -> Result<(), Rc<E>> {
        if self.setting("fault") == k {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn overridden(&self) -> V {
        object(vec![
            ("txHash", text("overridden-hash")),
            ("signature", text("overridden-signature")),
        ])
    }
    fn promise(&self, f: KeyFuture<V, Rc<E>>) -> V {
        V::Promise(Rc::new(Promise {
            future: RefCell::new(Some(f)),
            result: RefCell::new(None),
        }))
    }
    fn pump(&self) {
        let queued = self.tasks.take();
        let mut pending = vec![];
        for mut f in queued {
            if f.as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                pending.push(f)
            }
        }
        self.tasks.borrow_mut().extend(pending);
    }
}
fn result(mode: &str, normal: V) -> V {
    match mode {
        "error" => object(vec![(
            "error",
            V::Text({
                let mut t = units("wasm");
                t.push(0xd800);
                t.extend(units(" error"));
                t
            }),
        )]),
        "inherited" => V::Object(Rc::new(Obj {
            fields: RefCell::new(vec![]),
            inherited_error: true,
            error_fault: false,
        })),
        "getter" => V::Object(Rc::new(Obj {
            fields: RefCell::new(vec![]),
            inherited_error: false,
            error_fault: true,
        })),
        "function" => V::Function(Rc::new(())),
        "object" => object(vec![("opaque", V::Bool(true))]),
        "null" => V::Null,
        "thenable" => V::Thenable(false),
        _ => normal,
    }
}
fn fixture(input: &J) -> Rc<Host> {
    let setting = |k: &str| {
        if let Some(v) = input.get(k) {
            if let V::Text(t) = decode(v) {
                String::from_utf16(&t).unwrap()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    };
    let h = Rc::new(Host {
        input: input.clone(),
        root: RefCell::new(Weak::new()),
        receiver: object(vec![("initialized", decode(&input["initialized"]))]),
        tx: object(vec![("original", V::Bool(true))]),
        hash: result(
            &setting("hash"),
            V::Text({
                let mut t = units("hash");
                t.push(0xd800);
                t
            }),
        ),
        signed: result(
            &setting("sign"),
            V::Text({
                let mut t = units("signature");
                t.push(0xdfff);
                t
            }),
        ),
        verified: result(&setting("verify"), V::Bool(input["verify"] != false)),
        override_result: RefCell::new(None),
        api: Rc::new(()),
        failure: Rc::new(E {
            name: "Error",
            message: units("original fault"),
            cause: None,
        }),
        calls: RefCell::new(vec![]),
        tasks: RefCell::new(vec![]),
    });
    *h.root.borrow_mut() = Rc::downgrade(&h);
    h
}
impl KeyHost for Host {
    type Value = V;
    type Error = Rc<E>;
    type Api = Rc<()>;
    fn spawn(&self, f: Pin<Box<dyn Future<Output = ()>>>) {
        self.tasks.borrow_mut().push(f)
    }
    fn promise_resolve(&self, v: V) -> KeyFuture<V, Rc<E>> {
        match v {
            V::Promise(p) => Box::pin(std::future::poll_fn(move |cx| {
                if let Some(r) = p.result.borrow().as_ref() {
                    return Poll::Ready(r.clone());
                }
                let result = p.future.borrow_mut().as_mut().unwrap().as_mut().poll(cx);
                if let Poll::Ready(r) = result {
                    p.future.borrow_mut().take();
                    *p.result.borrow_mut() = Some(r.clone());
                    Poll::Ready(r)
                } else {
                    Poll::Pending
                }
            })),
            V::Thenable(overridden) => {
                let h = self.handle();
                Box::pin(async move {
                    h.trace(vec![text(if overridden {
                        "override-then"
                    } else {
                        "then"
                    })]);
                    Ok(if overridden {
                        h.override_result.borrow().as_ref().unwrap().clone()
                    } else {
                        V::Bool(true)
                    })
                })
            }
            value => Box::pin(async move { Ok(value) }),
        }
    }
    fn is_ready(&self) -> Result<bool, Rc<E>> {
        self.trace(vec![text("ready")]);
        self.fail("ready")?;
        Ok(self.input["ready"] != false)
    }
    fn init_wasm(&self) -> Result<KeyFuture<(), Rc<E>>, Rc<E>> {
        unreachable!("controlled original ensure import")
    }
    fn get_crypto_api(&self) -> Result<Rc<()>, Rc<E>> {
        self.trace(vec![text("api")]);
        self.fail("api")?;
        Ok(self.api.clone())
    }
    fn generate_keys(&self, _: &Rc<()>) -> Result<V, Rc<E>> {
        unreachable!("unused key export")
    }
    fn derive_public_key(&self, _: &Rc<()>, _: &[u16]) -> Result<V, Rc<E>> {
        unreachable!("unused key export")
    }
    fn is_object(&self, v: &V) -> bool {
        matches!(v, V::Object(_) | V::Null | V::Thenable(_) | V::Promise(_))
    }
    fn is_null(&self, v: &V) -> bool {
        matches!(v, V::Null)
    }
    fn has_error(&self, v: &V) -> Result<bool, Rc<E>> {
        Ok(
            matches!(v,V::Object(o) if o.inherited_error||o.error_fault||o.fields.borrow().iter().any(|(k,_)|k=="error")),
        )
    }
    fn property(&self, v: &V, k: &str) -> Result<V, Rc<E>> {
        if same(v, &self.receiver)
            && k == "initialized"
            && self.setting("fault") == "get-initialized"
        {
            self.trace(vec![text("initialized-get")]);
            return Err(self.failure.clone());
        }
        match v {
            V::Object(o) => {
                if k == "error" && o.error_fault {
                    self.trace(vec![text("error-get")]);
                    return Err(self.failure.clone());
                }
                Ok(o.fields
                    .borrow()
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(V::Undefined))
            }
            V::Text(s) if k == "length" => Ok(V::Number(s.len() as f64)),
            V::Null | V::Undefined => Err(Rc::new(E {
                name: "TypeError",
                message: units(&format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, V::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
                cause: None,
            })),
            _ => Ok(V::Undefined),
        }
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, Rc<E>> {
        Ok(match v {
            V::Text(s) => s.clone(),
            V::Undefined => units("undefined"),
            V::Null => units("null"),
            V::Bool(b) => units(if *b { "true" } else { "false" }),
            V::Number(n) => units(&safe_cex_num::js_number_string(*n)),
            V::Big(b) => units(&b.to_string()),
            _ => units("[object Object]"),
        })
    }
    fn key_error(&self, m: Vec<u16>, cause: Option<Rc<E>>) -> Rc<E> {
        Rc::new(E {
            name: "KeyManagerError",
            message: m,
            cause,
        })
    }
}
impl SigningHost for Host {
    fn set_property(&self, r: &V, k: &str, v: V) -> Result<(), Rc<E>> {
        if self.setting("fault") == "set" {
            self.trace(vec![text("initialized-set")]);
            return Err(self.failure.clone());
        }
        let V::Object(o) = r else {
            panic!("native receiver")
        };
        let mut fields = o.fields.borrow_mut();
        if let Some((_, old)) = fields.iter_mut().find(|(key, _)| key == k) {
            *old = v;
        } else {
            fields.push((k.into(), v));
        }
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::Undefined | V::Null => false,
            V::Bool(b) => *b,
            V::Text(s) => !s.is_empty(),
            V::Number(n) => *n != 0.0 && !n.is_nan(),
            _ => true,
        }
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn text(&self, s: Vec<u16>) -> V {
        V::Text(s)
    }
    fn number(&self, n: f64) -> V {
        V::Number(n)
    }
    fn object(&self, f: Vec<(&'static str, V)>) -> V {
        object(f)
    }
    fn call_method(&self, r: &V, name: &str, args: Vec<V>) -> Result<V, Rc<E>> {
        if name == "startsWith" {
            if self.flag("stringOverride") {
                self.trace(vec![text("starts-override")]);
                return Ok(V::Bool(false));
            }
            let (V::Text(s), V::Text(prefix)) = (r, &args[0]) else {
                panic!("controlled string member")
            };
            return Ok(V::Bool(s.starts_with(prefix)));
        }
        if name == "slice" {
            let V::Text(s) = r else {
                panic!("controlled string member")
            };
            return Ok(V::Text(s[2..].to_vec()));
        }
        assert!(
            same(r, &self.receiver),
            "SDK-SIGN bound native signer receiver"
        );
        if name == "ensureInitialized" {
            if self.flag("ensureOverride") {
                self.trace(vec![text("ensure-override")]);
                return Ok(V::Undefined);
            }
            ensure_initialized(self, r)?;
            return Ok(V::Undefined);
        }
        assert_eq!(name, "signMessage", "native member import");
        let mode = self.setting("override");
        if !mode.is_empty() {
            self.trace(vec![
                text("sign-override"),
                args[0].clone(),
                args[1].clone(),
            ]);
            if mode == "reject" {
                let e = self.failure.clone();
                return Ok(self.promise(Box::pin(async move { Err(e) })));
            }
            let result = self.overridden();
            *self.override_result.borrow_mut() = Some(result.clone());
            return Ok(if mode == "thenable" {
                V::Thenable(true)
            } else {
                result
            });
        }
        Ok(self.promise(sign_message(
            self.handle(),
            r.clone(),
            args[0].clone(),
            args[1].clone(),
        )))
    }
    fn ensure_wasm_initialized(&self) -> Result<KeyFuture<(), Rc<E>>, Rc<E>> {
        self.trace(vec![text("ensure-wasm")]);
        self.fail("init-sync")?;
        let result = self.fail("init");
        Ok(Box::pin(async move { result }))
    }
    fn normalize_private_key(&self, key: &V) -> Result<V, Rc<E>> {
        self.trace(vec![text("normalize"), key.clone()]);
        self.fail("normalize")?;
        let starts = self.call_method(key, "startsWith", vec![text("0x")])?;
        let clean = if self.truthy(&starts) {
            self.call_method(key, "slice", vec![V::Number(2.0)])?
        } else {
            key.clone()
        };
        let V::Text(s) = &clean else {
            panic!("controlled original normalize import")
        };
        if s.len() != 80 || !s.iter().all(|u| matches!(*u,48..=57|65..=70|97..=102)) {
            return Err(self.key_error(units(&format!("Invalid private key format after normalization. Expected 80 hex characters, got {}",s.len())),None));
        }
        Ok(clean)
    }
    fn hash_transaction(&self, kind: HashKind, tx: &V, chain: &V) -> Result<V, Rc<E>> {
        assert!(same(tx, &self.tx), "SDK-SIGN native tx identity");
        let V::Number(n) = chain else {
            panic!("native chain value")
        };
        assert!(n.is_nan(), "SDK-SIGN native chain NaN");
        self.trace(vec![
            text("hash-tx"),
            text(match kind {
                HashKind::CreateOrder => "create",
                HashKind::CancelOrder => "cancel",
                HashKind::Transfer => "transfer",
                HashKind::Withdraw => "withdraw",
            }),
            tx.clone(),
            chain.clone(),
        ]);
        self.fail("hash-tx")?;
        Ok(text(&"ab".repeat(8)))
    }
    fn hash_auth_token(&self, a: &V, k: &V, d: &V, c: &V) -> Result<V, Rc<E>> {
        let V::Big(b) = d else {
            panic!("native auth deadline")
        };
        assert_eq!(
            b,
            &(BigInt::from(1) << 200),
            "SDK-SIGN native auth deadline"
        );
        self.trace(vec![
            text("hash-auth"),
            a.clone(),
            k.clone(),
            d.clone(),
            c.clone(),
        ]);
        self.fail("hash-auth")?;
        Ok(text(&"ab".repeat(8)))
    }
    fn poseidon2_hash(&self, api: &Rc<()>, m: &V) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(api, &self.api),
            "SDK-SIGN native hash API receiver"
        );
        self.trace(vec![text("poseidon"), m.clone()]);
        self.fail("hash")?;
        Ok(self.hash.clone())
    }
    fn schnorr_sign(&self, api: &Rc<()>, hash: &V, key: &V) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(api, &self.api),
            "SDK-SIGN native sign API receiver"
        );
        assert!(
            same(hash, &self.hash),
            "SDK-SIGN native sign hash input identity"
        );
        self.trace(vec![text("sign"), hash.clone(), key.clone()]);
        self.fail("sign")?;
        Ok(self.signed.clone())
    }
    fn schnorr_verify(&self, api: &Rc<()>, hash: &V, pk: &V, sig: &V) -> Result<V, Rc<E>> {
        assert!(
            Rc::ptr_eq(api, &self.api),
            "SDK-SIGN native verify API receiver"
        );
        assert!(
            same(hash, &self.hash),
            "SDK-SIGN native verify hash input identity"
        );
        self.trace(vec![text("verify"), hash.clone(), pk.clone(), sig.clone()]);
        self.fail("verify")?;
        Ok(self.verified.clone())
    }
    fn remainder_16(&self, v: &V) -> Result<f64, Rc<E>> {
        let V::Number(n) = v else {
            panic!("controlled numeric primitive")
        };
        Ok(*n % 16.0)
    }
    fn less_than(&self, v: &V, b: f64) -> Result<bool, Rc<E>> {
        let V::Number(n) = v else {
            panic!("controlled numeric primitive")
        };
        Ok(*n < b)
    }
    fn greater_than(&self, v: &V, b: f64) -> Result<bool, Rc<E>> {
        let V::Number(n) = v else {
            panic!("controlled numeric primitive")
        };
        Ok(*n > b)
    }
    fn template_string(&self, v: &V) -> Result<Vec<u16>, Rc<E>> {
        self.string(v)
    }
    fn signer_error(&self, m: Vec<u16>, cause: Option<Rc<E>>) -> Rc<E> {
        self.trace(vec![
            text("signer-error"),
            V::Text(m.clone()),
            V::Bool(cause.as_ref().is_some_and(|e| Rc::ptr_eq(e, &self.failure))),
        ]);
        Rc::new(E {
            name: "SignerError",
            message: m,
            cause,
        })
    }
}
fn settle(h: &Host, mut f: KeyFuture<V, Rc<E>>) -> Result<V, Rc<E>> {
    for _ in 0..100 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
        h.pump();
    }
    panic!("SDK-SIGN owned job settled")
}
fn start(h: Rc<Host>, method: &str) -> KeyFuture<V, Rc<E>> {
    let r = h.receiver.clone();
    match method {
        "initialize" => {
            let f = initialize(h, r);
            Box::pin(async move { f.await.map(|_| V::Undefined) })
        }
        "ensureInitialized" => {
            let result = ensure_initialized(h.as_ref(), &r).map(|_| V::Undefined);
            Box::pin(async move { result })
        }
        "signMessage" => sign_message(h.clone(), r, h.arg("message"), h.arg("key")),
        "signCreateOrder" => {
            sign_create_order(h.clone(), r, h.tx.clone(), h.arg("key"), h.arg("chain"))
        }
        "signCancelOrder" => {
            sign_cancel_order(h.clone(), r, h.tx.clone(), h.arg("key"), h.arg("chain"))
        }
        "signTransfer" => sign_transfer(h.clone(), r, h.tx.clone(), h.arg("key"), h.arg("chain")),
        "signWithdraw" => sign_withdraw(h.clone(), r, h.tx.clone(), h.arg("key"), h.arg("chain")),
        "createAuthToken" => create_auth_token(
            h.clone(),
            r,
            h.arg("account"),
            h.arg("apiKey"),
            h.arg("deadline"),
            h.arg("key"),
            h.arg("chain"),
        ),
        "verifySignature" => verify_signature(
            h.clone(),
            r,
            h.arg("message"),
            V::Text({
                let mut t = units("public");
                t.push(0xd800);
                t
            }),
            V::Text({
                let mut t = units("signature");
                t.push(0xdfff);
                t
            }),
        ),
        _ => unreachable!(),
    }
}
fn optional(h: &Host, v: &V, k: &str) -> V {
    if matches!(v, V::Undefined | V::Null) {
        V::Undefined
    } else {
        h.property(v, k).unwrap()
    }
}
fn run_family(method: &str, floor: usize) {
    let all = corpus()["cases"].as_array().unwrap();
    let cases: Vec<_> = all
        .iter()
        .filter(|c| c["input"]["method"] == encode(&text(method)))
        .collect();
    assert_eq!(cases.len(), floor, "SDK-SIGN physical floor {method}");
    for (i, c) in cases.iter().enumerate() {
        let h = fixture(&c["input"]);
        let f = start(h.clone(), method);
        let before = json!(h.calls.borrow().clone());
        let result = settle(h.as_ref(), f);
        let output = match result {
            Ok(v) => {
                if method.starts_with("sign") && method != "signMessage" {
                    if let Some(original) = h.override_result.borrow().as_ref() {
                        assert!(
                            same(&v, original),
                            "SDK-SIGN native override result identity {method} {i}"
                        );
                    }
                }
                if method == "signMessage" {
                    assert!(
                        same(&h.property(&v, "txHash").unwrap(), &h.hash),
                        "SDK-SIGN native hash {method} {i}"
                    );
                    assert!(
                        same(&h.property(&v, "signature").unwrap(), &h.signed),
                        "SDK-SIGN native signature {method} {i}"
                    );
                }
                if method == "createAuthToken" {
                    let expected = if h.setting("override").is_empty() {
                        h.signed.clone()
                    } else {
                        text("overridden-signature")
                    };
                    assert!(
                        same(&h.property(&v, "token").unwrap(), &expected),
                        "SDK-SIGN auth native token {method} {i}"
                    );
                    let V::Number(a) = h.property(&v, "accountIndex").unwrap() else {
                        panic!("native account number")
                    };
                    let V::Number(original) = h.arg("account") else {
                        panic!("native account input")
                    };
                    assert!(
                        a.is_nan() && original.is_nan() || a.to_bits() == original.to_bits(),
                        "SDK-SIGN auth account bits"
                    );
                    let V::Number(k) = h.property(&v, "apiKeyIndex").unwrap() else {
                        panic!("native API number")
                    };
                    let V::Number(original) = h.arg("apiKey") else {
                        panic!("native API input")
                    };
                    assert_eq!(k.to_bits(), original.to_bits(), "SDK-SIGN auth API bits");
                    let V::Big(b) = h.property(&v, "deadline").unwrap() else {
                        panic!("native deadline value")
                    };
                    assert_eq!(b, BigInt::from(1) << 200, "SDK-SIGN native return deadline");
                }
                json!({"ok":encode(&v),"hashSame":same(&optional(h.as_ref(),&v,"txHash"),&h.hash),"signatureSame":same(&optional(h.as_ref(),&v,"signature"),&h.signed),"verifySame":same(&v,&h.verified),"overrideSame":h.override_result.borrow().as_ref().is_some_and(|original|same(&v,original))})
            }
            Err(e) => {
                let original = Rc::ptr_eq(&e, &h.failure);
                let cause = e.cause.as_ref().is_some_and(|c| Rc::ptr_eq(c, &h.failure));
                if c["output"]["error"]["original"] == true {
                    assert!(original, "SDK-SIGN native original exception {method} {i}");
                }
                if c["output"]["error"]["causeSame"] == true {
                    assert!(cause, "SDK-SIGN native init cause identity");
                }
                json!({"error":{"name":e.name,"message":{"text":e.message},"original":original,"causeSame":cause}})
            }
        };
        assert_eq!(output, c["output"], "SDK-SIGN output {method} {i}");
        assert_eq!(
            json!(h.calls.borrow().clone()),
            c["calls"],
            "SDK-SIGN calls {method} {i}"
        );
        assert_eq!(before, c["before"], "SDK-SIGN eager calls {method} {i}");
        let state = if h.setting("fault") == "get-initialized" {
            json!("getter")
        } else {
            encode(&h.property(&h.receiver, "initialized").unwrap())
        };
        assert_eq!(state, c["state"], "SDK-SIGN state {method} {i}");
    }
}
#[test]
fn initialize_matches_source() {
    run_family("initialize", 7)
}
#[test]
fn ensure_initialized_matches_source() {
    run_family("ensureInitialized", 6)
}
#[test]
fn sign_message_matches_source() {
    run_family("signMessage", 19)
}
#[test]
fn sign_create_order_matches_source() {
    run_family("signCreateOrder", 7)
}
#[test]
fn sign_cancel_order_matches_source() {
    run_family("signCancelOrder", 7)
}
#[test]
fn sign_transfer_matches_source() {
    run_family("signTransfer", 7)
}
#[test]
fn sign_withdraw_matches_source() {
    run_family("signWithdraw", 7)
}
#[test]
fn create_auth_token_matches_source() {
    run_family("createAuthToken", 12)
}
#[test]
fn verify_signature_matches_source() {
    run_family("verifySignature", 12)
}
#[test]
fn ignored_initialize_future_retains_source_body() {
    let c = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| {
            c["input"]["method"] == encode(&text("initialize"))
                && c["input"]["initialized"] == false
                && c["input"].get("fault").is_none()
        })
        .unwrap();
    let h = fixture(&c["input"]);
    let f = initialize(h.clone(), h.receiver.clone());
    assert_eq!(
        *h.calls.borrow(),
        vec![json!([encode(&text("ensure-wasm"))])],
        "SDK-SIGN ignored init eager import"
    );
    drop(f);
    for _ in 0..10 {
        h.pump();
    }
    assert!(
        matches!(
            h.property(&h.receiver, "initialized").unwrap(),
            V::Bool(true)
        ),
        "SDK-SIGN ignored init owned continuation"
    );
    assert!(h.tasks.borrow().is_empty(), "SDK-SIGN owned task completed");
}
