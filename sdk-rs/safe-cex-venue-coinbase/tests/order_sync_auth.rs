#[path = "support/retained_info.rs"]
mod retained_info;
#[path = "support/retained_mutation.rs"]
mod retained_mutation;
use indexmap::IndexMap;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{Market, OrderSide, OrderStatus, OrderType};
use safe_cex_venue_coinbase::{
    cdp_signing::*,
    exchange_tick::*,
    exchange_ws_auth::*,
    market_position_reads::{ReadItem, ReadItems, ReadMarket},
    order_sync::*,
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    starbase_auth::{
        CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseResolver as Resolver,
    },
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
    utils::CoinbaseSignatureAlgorithm,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    process::Command,
    rc::Rc,
};
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
const FIXTURE: &str = include_str!("fixtures/order-sync-auth.json");
fn source() -> Value {
    let f = fixture();
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "2b3f5b2ac1d16bc3214baacf267fd1002daca3c53b662a6d2cd4d9d993bab7c8",
        "full corpus content fingerprint"
    );
    for (k, n) in [
        ("signing", 11),
        ("auth", 11),
        ("snapshots", 10),
        ("events", 10),
        ("ticks", 14),
    ] {
        assert_eq!(
            f[k].as_array().unwrap().len(),
            n,
            "independent fixture floor {k}"
        );
    }
    let o = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-sync-auth.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "live source oracle: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    {
        let result: Value = serde_json::from_slice(&o.stdout).expect("nonempty source output");
        for (k, n) in [
            ("signing", 11),
            ("auth", 11),
            ("snapshots", 10),
            ("events", 10),
            ("ticks", 14),
        ] {
            assert_eq!(
                result[k].as_array().unwrap().len(),
                n,
                "independent source floor {k}"
            );
        }
        result
    }
}
fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}
fn failure(v: Value) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(v)))
}
fn num(a: f64, e: &Value, n: &str) {
    let b = u64::from_str_radix(e["$bits"].as_str().expect("source numeric bits"), 16).unwrap();
    if f64::from_bits(b).is_nan() {
        assert!(a.is_nan(), "{n}: native NaN")
    } else {
        assert_eq!(a.to_bits(), b, "{n}: native numeric bits")
    }
}
fn value(a: Option<&Value>, e: &Value, n: &str) {
    if e.get("$undefined") == Some(&json!(true)) {
        assert!(a.is_none(), "{n}: native undefined");
        return;
    }
    let a = a.expect("native value");
    match a {
        Value::Number(v) => {
            if e.get("$bits").is_some() {
                num(v.as_f64().unwrap(), e, n)
            } else {
                assert_eq!(a, e, "{n}: native primitive metadata scalar")
            }
        }
        Value::Array(v) => {
            let es = e.as_array().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: source array count");
            for (a, e) in v.iter().zip(es) {
                value(Some(a), e, n)
            }
        }
        Value::Object(v) => {
            let es = e.as_object().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: source own field count");
            for (k, e) in es {
                value(v.get(k), e, n)
            }
        }
        _ => assert_eq!(a, e, "{n}: native scalar"),
    }
}
fn frame(a: Option<Frame>, e: &Value, n: &str) {
    if e.get("$undefined") == Some(&json!(true)) {
        assert!(a.is_none(), "{n}: native undefined");
        return;
    }
    match a.expect("native frame") {
        Frame::Null => assert!(e.is_null(), "{n}: null"),
        Frame::Bool(v) => assert_eq!(Some(v), e.as_bool(), "{n}: bool"),
        Frame::Number(v) => num(v, e, n),
        Frame::String(v) => assert_eq!(
            v,
            e.as_str().unwrap().encode_utf16().collect::<Vec<_>>(),
            "{n}: UTF16"
        ),
        Frame::Array(v) => {
            let v = v.borrow();
            let es = e.as_array().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: array count");
            for (a, e) in v.iter().zip(es) {
                frame(Some(a.clone()), e, n)
            }
        }
        Frame::Object(v) => {
            let v = v.borrow();
            let es = e.as_object().unwrap();
            assert_eq!(v.len(), es.len(), "{n}: own fields");
            for (k, e) in es {
                frame(v.get(&k.encode_utf16().collect::<Vec<_>>()).cloned(), e, n)
            }
        }
    }
}
fn same(a: &Frame, b: &Frame) -> bool {
    match (a, b) {
        (Frame::Object(a), Frame::Object(b)) => Rc::ptr_eq(a, b),
        (Frame::Array(a), Frame::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn set(v: &Frame, k: &str, x: Frame) {
    let Frame::Object(v) = v else {
        panic!("object")
    };
    v.borrow_mut().insert(k.encode_utf16().collect(), x);
}
fn ready<T: Clone + 'static>(_q: &Microtasks, x: T) -> Promise<T> {
    Promise::resolved(x)
}
struct Trace {
    expected: RefCell<VecDeque<Value>>,
    label: String,
}
impl Trace {
    fn new(v: &Value, label: &str) -> Self {
        Self {
            expected: RefCell::new(v.as_array().unwrap().iter().cloned().collect()),
            label: label.into(),
        }
    }
    fn next(&self, kind: &str) -> Value {
        let e = self
            .expected
            .borrow_mut()
            .pop_front()
            .expect("independent source trace floor exceeded");
        assert_eq!(e["kind"], kind, "{}: native callback order", self.label);
        e
    }
    fn done(&self) {
        assert!(
            self.expected.borrow().is_empty(),
            "{}: source trace unconsumed",
            self.label
        )
    }
}
fn assert_error(a: &PortfolioFailure, e: &Value, n: &str) {
    value(a.value.as_ref(), e, n)
}
fn orders(a: &SyncedOrders, e: &Value, indices: &Value, roots: &[ReadItem], n: &str) {
    let a = a.borrow();
    let es = e.as_array().unwrap();
    assert_eq!(a.len(), es.len(), "{n}: native order count");
    for (i, (a, e)) in a.iter().zip(es).enumerate() {
        let a = a.borrow();
        let o = &a.fields;
        assert_eq!(o.id, e["id"].as_str().unwrap(), "{n}: native order id");
        assert_eq!(
            o.parent_id.as_deref(),
            e["parentId"].as_str(),
            "{n}: parent id"
        );
        assert_eq!(o.symbol, e["symbol"].as_str().unwrap(), "{n}: symbol");
        let status = match e["status"].as_str().unwrap() {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            _ => panic!("source status"),
        };
        assert_eq!(o.status, status, "{n}: native status");
        let kind = match e["type"].as_str().unwrap() {
            "limit" => OrderType::Limit,
            "market" => OrderType::Market,
            "stop_market" => OrderType::StopLoss,
            _ => panic!("source type"),
        };
        assert_eq!(o.r#type, kind, "{n}: native type");
        assert_eq!(
            o.side,
            if e["side"] == "buy" {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            "{n}: side"
        );
        for (k, v) in [
            ("price", o.price.to_number()),
            ("amount", o.amount),
            ("filled", o.filled),
            ("remaining", o.remaining),
            ("lastUpdate", o.last_update),
        ] {
            num(v, &e[k], &format!("{n}.{k}"))
        }
        assert_eq!(
            o.reduce_only.into_value(),
            e["reduceOnly"].as_bool(),
            "{n}: reduceOnly"
        );
        let extra = o.extra.as_ref().expect("native extra");
        num(
            extra.remaining_native.unwrap(),
            &e["extra"]["remainingNative"],
            n,
        );
        assert!(
            extra.filled_in_placement_units.is_none()
                && extra.remaining_notional.is_none()
                && extra.conditional_order_price.is_none()
                && extra.reject_reason.is_none()
                && extra.is_from_chaser.is_none()
                && extra.chaser_id.is_none()
                && extra.chaser_attempts.is_none()
                && extra.chaser_max_attempts.is_none(),
            "{n}: absent extra slots"
        );
        assert!(
            o.info.is_none()
                && o.close.is_none()
                && o.time_in_force.is_none()
                && o.trigger_type.is_none()
                && o.tp_trigger_type.is_none()
                && o.sl_trigger_type.is_none()
                && o.take_profit.is_none()
                && o.stop_loss.is_none(),
            "{n}: absent optional slots"
        );
        let ix = indices[i].as_u64().map(|v| v as usize).unwrap_or_else(|| {
            f64::from_bits(u64::from_str_radix(indices[i]["$bits"].as_str().unwrap(), 16).unwrap())
                as usize
        });
        assert!(
            Rc::ptr_eq(&a.info, &roots[ix]),
            "{n}: native original raw info root"
        );
        retained_info::assert_info(Some(&a.info.borrow()), &e["info"], n);
        assert_eq!(
            e.as_object().unwrap().len(),
            14,
            "{n}: independent source order shape"
        );
    }
}
#[test]
fn corpus_is_content_pinned_and_independently_counted() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE.as_bytes())),
        "2b3f5b2ac1d16bc3214baacf267fd1002daca3c53b662a6d2cd4d9d993bab7c8",
        "full corpus content fingerprint"
    );
    let f = fixture();
    let s = source();
    for (k, n) in [
        ("signing", 11),
        ("auth", 11),
        ("snapshots", 10),
        ("events", 10),
        ("ticks", 14),
    ] {
        assert_eq!(
            f[k].as_array().unwrap().len(),
            n,
            "independent fixture floor {k}"
        );
        assert_eq!(
            s[k].as_array().unwrap().len(),
            n,
            "independent source floor {k}"
        );
    }
}
struct SignHost {
    q: Microtasks,
    input: Value,
    trace: Trace,
    imports: RefCell<Vec<CdpImportKey>>,
    inputs: RefCell<Vec<CdpSigningInput>>,
    keys: RefCell<Vec<CdpCryptoKey>>,
    seed: RefCell<Option<Vec<u8>>>,
    wire: RefCell<Vec<Vec<u8>>>,
    primitive_failure: Option<PortfolioFailure>,
    pem_failure: PortfolioFailure,
    pending: RefCell<Vec<Resolver<Result<CdpCryptoKey, PortfolioFailure>>>>,
    defer: Cell<bool>,
}
impl SignHost {
    fn new(q: &Microtasks, input: Value, e: &Value) -> Self {
        Self {
            q: q.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap_or("cache")),
            input,
            imports: RefCell::new(vec![]),
            inputs: RefCell::new(vec![]),
            keys: RefCell::new(vec![]),
            seed: RefCell::new(None),
            wire: RefCell::new(vec![]),
            primitive_failure: None,
            pem_failure: failure(
                json!({"name":"InvalidCharacterError","message":"Invalid character","code":5,"kind":"DOMException"}),
            ),
            pending: RefCell::new(vec![]),
            defer: Cell::new(false),
        }
    }
    fn primitive(&self, input: CdpSigningInput) -> Result<Vec<u8>, PortfolioFailure> {
        self.trace.next("sign");
        self.wire.borrow_mut().push(input.message.clone());
        self.inputs.borrow_mut().push(input);
        if let Some(e) = &self.primitive_failure {
            Err(e.clone())
        } else {
            Ok(vec![1, 2, 255, 64])
        }
    }
}
impl CdpSigningHost for SignHost {
    fn prepare_pkcs8(&self, s: &str) -> Result<Vec<u8>, PortfolioFailure> {
        if s.contains('!') {
            Err(self.pem_failure.clone())
        } else {
            Ok(safe_cex_venue_coinbase::utils::pem_to_pkcs8_der(s)
                .expect("valid primitive PEM fixture"))
        }
    }
    fn import_key(&self, r: CdpImportKey) -> CdpAsync<CdpCryptoKey> {
        self.trace.next("import");
        self.imports.borrow_mut().push(r);
        if self.defer.get() {
            let (p, d) = Promise::pending();
            self.pending.borrow_mut().push(d);
            return Ok(p);
        }
        if let Some(e) = self.input.get("importError") {
            return Ok(ready(&self.q, Err(failure(e.clone()))));
        }
        let key: CdpCryptoKey = Rc::new(self.keys.borrow().len());
        self.keys.borrow_mut().push(key.clone());
        Ok(ready(&self.q, Ok(key)))
    }
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure> {
        self.trace.next("clock");
        Ok(match self.input.get("now") {
            Some(Value::String(v)) if v == "NaN" => f64::NAN,
            Some(Value::String(v)) if v == "negative-zero" => -0.0,
            Some(v) => v.as_f64().unwrap(),
            None => 37.0,
        })
    }
    fn uuid(&self) -> Result<String, PortfolioFailure> {
        self.trace.next("uuid");
        Ok("nonce".into())
    }
    fn sign_ed25519(&self, i: CdpSigningInput, seed: Vec<u8>) -> Result<Vec<u8>, PortfolioFailure> {
        *self.seed.borrow_mut() = Some(seed);
        self.primitive(i)
    }
    fn sign_ecdsa(
        &self,
        n: &'static str,
        h: &'static str,
        k: CdpCryptoKey,
        i: CdpSigningInput,
    ) -> CdpAsync<Vec<u8>> {
        assert_eq!(
            (n, h),
            ("ECDSA", "SHA-256"),
            "native signing primitive algorithm"
        );
        assert!(
            Rc::ptr_eq(&k, &self.keys.borrow()[0]),
            "native imported key identity"
        );
        Ok(ready(&self.q, self.primitive(i)))
    }
}
#[test]
fn signing_native_claims_precede_wire_and_jwt() {
    let f = fixture();
    let s = source();
    for (input, e) in f["signing"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["signing"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let mut host = SignHost::new(&q, input.clone(), e);
        host.primitive_failure = input.get("signError").cloned().map(failure);
        let r = Rc::new(CdpSigningReceiver {
            host,
            cache: Rc::default(),
            spawn: q.spawner(),
        });
        let p = sign(
            r.clone(),
            input["key"].as_str().unwrap().into(),
            input["secret"].as_str().unwrap().into(),
            input["uri"].as_str().map(str::to_owned),
        );
        q.drain();
        let result = completed(p);
        r.host.trace.done();
        let imports = r.host.imports.borrow();
        let es = e["imports"].as_array().unwrap();
        assert_eq!(imports.len(), es.len(), "native import count");
        for (a, e) in imports.iter().zip(es) {
            assert_eq!(a.format, e["format"].as_str().unwrap());
            assert_eq!(
                (a.name, a.named_curve),
                (
                    e["algorithm"]["name"].as_str().unwrap(),
                    e["algorithm"]["namedCurve"].as_str().unwrap()
                )
            );
            assert_eq!(Some(a.extractable), e["extractable"].as_bool());
            assert_eq!(a.usages, vec!["sign"]);
            assert_eq!(
                a.usages,
                e["usages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap())
                    .collect::<Vec<_>>(),
                "native usages versus source"
            );
            for (a, e) in a.der.iter().zip(e["der"].as_array().unwrap()) {
                num(*a as f64, e, "native DER byte")
            }
            assert_eq!(a.der.len(), e["der"].as_array().unwrap().len());
        }
        let inputs = r.host.inputs.borrow();
        let es = e["signs"].as_array().unwrap();
        assert_eq!(inputs.len(), es.len(), "native signing call count");
        for (a, e) in inputs.iter().zip(es) {
            let h = &e["header"];
            assert_eq!(
                a.header.algorithm,
                if h["alg"] == "EdDSA" {
                    CoinbaseSignatureAlgorithm::EdDsa
                } else {
                    CoinbaseSignatureAlgorithm::Es256
                },
                "native algorithm"
            );
            assert_eq!(a.header.typ, h["typ"].as_str().unwrap());
            assert_eq!(a.header.key_id, h["kid"].as_str().unwrap());
            assert_eq!(a.header.nonce, h["nonce"].as_str().unwrap());
            let p = &e["payload"];
            assert_eq!(
                a.payload.issuer,
                p["iss"].as_str().unwrap(),
                "native issuer"
            );
            assert_eq!(
                a.payload.audience.as_deref(),
                p["aud"].as_array().map(|_| ["cdp_service"].as_slice()),
                "native audience presence"
            );
            num(a.payload.not_before, &p["nbf"], "native nbf");
            num(a.payload.expires, &p["exp"], "native exp");
            assert_eq!(a.payload.subject, p["sub"].as_str().unwrap());
            assert_eq!(
                a.payload.uri.as_deref(),
                p["uri"].as_str(),
                "native uri presence"
            );
            if e["kind"] == "ed25519" {
                let seed = r.host.seed.borrow();
                let seed = seed.as_ref().unwrap();
                assert_eq!(seed.len(), 32, "independent native seed size");
                assert_eq!(
                    e["seed"].as_array().unwrap().len(),
                    32,
                    "independent source seed size"
                );
                for (a, e) in seed.iter().zip(e["seed"].as_array().unwrap()) {
                    assert_eq!(*a as u64, e.as_u64().unwrap())
                }
            }
        }
        for e in es {
            if e["kind"] == "ecdsa" {
                assert_eq!(
                    e["algorithm"],
                    json!({"name":"ECDSA","hash":"SHA-256"}),
                    "source primitive versus asserted native algorithm"
                );
                assert_eq!(
                    e["keyIndex"], 0,
                    "source primitive versus asserted native key root index"
                );
            }
        }
        if let Err(err) = &result {
            assert_error(err, &e["error"], "native signing failure");
            if input["label"] == "PEM exception preserves DOM metadata" {
                assert!(
                    Rc::ptr_eq(err, &r.host.pem_failure),
                    "sign preserves native PEM exception carrier"
                );
            }
            if let Some(original) = &r.host.primitive_failure {
                assert!(Rc::ptr_eq(err, original), "sign failure original root");
            }
        }
        // Every native claim, handle and primitive argument above precedes any wire check.
        for (a, e) in inputs.iter().zip(es) {
            assert_eq!(
                a.message,
                e["message"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u8)
                    .collect::<Vec<_>>(),
                "JWT primitive wire bytes"
            );
        }
        if let Ok(v) = result {
            assert!(e["error"].is_null());
            assert_eq!(v, e["value"].as_str().unwrap(), "JWT output bytes");
        }
    }
}
#[test]
fn signing_key_cache_races_and_pem_exception_identity() {
    let q = Microtasks::default();
    q.enqueue(|| {});
    q.drain();
    let e = json!({"trace":[{"kind":"import"},{"kind":"import"},{"kind":"import"}]});
    let host = SignHost::new(&q, json!({}), &e);
    let r = Rc::new(CdpSigningReceiver {
        host,
        cache: Rc::default(),
        spawn: q.spawner(),
    });
    let pem = "-----BEGIN PRIVATE KEY-----\nAQIDBA==\n-----END PRIVATE KEY-----";
    let p = get_signing_key(r.clone(), pem.into());
    q.drain();
    let a = completed(p).unwrap();
    let p = get_signing_key(r.clone(), pem.into());
    q.drain();
    assert!(
        Rc::ptr_eq(&a, &completed(p).unwrap()),
        "same-secret cached key root"
    );
    assert_eq!(
        r.host.imports.borrow().len(),
        1,
        "independent one import after cache hit"
    );
    r.host.defer.set(true);
    let mut first = get_signing_key(r.clone(), format!("{pem} "));
    let mut second = get_signing_key(r.clone(), format!("{pem}  "));
    assert_eq!(
        r.host.pending.borrow().len(),
        2,
        "two eager distinct imports"
    );
    assert!(is_pending(&mut first));
    assert!(is_pending(&mut first), "unresolved first import");
    assert!(is_pending(&mut second));
    assert!(is_pending(&mut second));
    let ka: CdpCryptoKey = Rc::new("a");
    let kb: CdpCryptoKey = Rc::new("b");
    r.host.pending.borrow()[1].resolve(Ok(kb.clone()));
    q.drain();
    assert!(Rc::ptr_eq(&completed(second).unwrap(), &kb));
    r.host.pending.borrow()[0].resolve(Ok(ka.clone()));
    q.drain();
    assert!(Rc::ptr_eq(&completed(first).unwrap(), &ka));
    let p = get_signing_key(r.clone(), format!("{pem} "));
    q.drain();
    assert!(
        Rc::ptr_eq(&completed(p).unwrap(), &ka),
        "last completion cache root"
    );
    assert_eq!(
        *r.cache.key_secret.borrow(),
        format!("{pem} "),
        "last completion cache secret"
    );
    let p = get_signing_key(
        r.clone(),
        "-----BEGIN PRIVATE KEY-----\n!\n-----END PRIVATE KEY-----".into(),
    );
    q.drain();
    let err = completed(p).unwrap_err();
    assert!(
        Rc::ptr_eq(&err, &r.host.pem_failure),
        "get key preserves original DOM exception carrier"
    );
    assert_error(
        &err,
        &source()["pemFailure"],
        "native DOM exception metadata",
    );
    assert!(
        Rc::ptr_eq(r.cache.crypto_key.borrow().as_ref().unwrap(), &ka),
        "failed import preparation leaves cache root"
    );
    r.host.trace.done();
}
struct AuthHost {
    signature: Option<Frame>,
    q: Microtasks,
    input: Value,
    request: Frame,
    options: Frame,
    calls: RefCell<Vec<(Option<Frame>, Option<Frame>)>>,
}
impl ExchangeWsAuthHost for AuthHost {
    fn options(&self) -> Frame {
        self.options.clone()
    }
    fn sign_local(&self, k: Option<Frame>, s: Option<Frame>) -> ExchangeWsAuthAsync {
        self.calls.borrow_mut().push((k, s));
        if self.input["mutate"] == true {
            set(&self.request, "id", Frame::from_json(&json!("changed")))
        }
        Ok(ready(
            &self.q,
            if let Some(e) = self.input.get("error") {
                Err(failure(e.clone()))
            } else {
                Ok(self.signature.clone())
            },
        ))
    }
}
#[test]
fn websocket_auth_native_own_slots_and_id_timing() {
    let f = fixture();
    let s = source();
    for (input, e) in f["auth"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["auth"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let request = Frame::from_json(&input["request"]);
        let original_id = request.get("id");
        let host = AuthHost {
            signature: input.get("signature").map(Frame::from_json),
            q: q.clone(),
            input: input.clone(),
            request: request.clone(),
            options: Frame::from_json(&input["options"]),
            calls: RefCell::new(vec![]),
        };
        let r = Rc::new(ExchangeWsAuthReceiver {
            host,
            spawn: q.spawner(),
        });
        let p = sign_ws_auth(r.clone(), Some(request));
        q.drain();
        match completed(p) {
            Ok(v) => {
                assert!(e["error"].is_null());
                let expected = &e["value"];
                if input["label"] == "auth retains original id signature and credential roots" {
                    assert!(
                        same(v.id.as_ref().unwrap(), original_id.as_ref().unwrap()),
                        "auth retains original native id root"
                    );
                    assert!(
                        same(
                            v.signature.as_ref().unwrap(),
                            r.host.signature.as_ref().unwrap()
                        ),
                        "auth retains original native signature root"
                    );
                }
                frame(
                    v.id,
                    expected.get("id").unwrap_or(&json!({"$undefined":true})),
                    "native auth id captured at source time",
                );
                assert_eq!(
                    v.has_error,
                    expected.get("error").is_some(),
                    "native error own slot"
                );
                assert_eq!(
                    v.has_signature,
                    expected.get("wsAuthSignature").is_some(),
                    "native signature own slot"
                );
                if v.has_error {
                    frame(v.error, &expected["error"], "native auth error")
                }
                if v.has_signature {
                    frame(
                        v.signature,
                        &expected["wsAuthSignature"],
                        "native auth signature",
                    )
                }
            }
            Err(err) => assert_error(&err, &e["error"], "native auth throw"),
        };
        let calls = r.host.calls.borrow();
        let es = e["calls"].as_array().unwrap();
        assert_eq!(
            calls.len(),
            es.len(),
            "independent signing invocation count"
        );
        for ((k, s), e) in calls.iter().zip(es) {
            if input["label"] == "auth retains original id signature and credential roots" {
                assert!(
                    same(
                        k.as_ref().unwrap(),
                        r.host.options.get("key").as_ref().unwrap()
                    ),
                    "auth passes original key root"
                );
                assert!(
                    same(
                        s.as_ref().unwrap(),
                        r.host.options.get("secret").as_ref().unwrap()
                    ),
                    "auth passes original secret root"
                );
            }
            frame(k.clone(), &e["key"], "native sign key");
            frame(s.clone(), &e["secret"], "native sign secret");
        }
    }
}
struct SnapshotHost {
    q: Microtasks,
    input: Value,
    trace: Trace,
    pages: RefCell<VecDeque<(OrderPage, Option<PortfolioFailure>)>>,
    roots: Rc<RefCell<Vec<ReadItem>>>,
    markets: PrivateOrderMarkets,
    first_page: ReadItems,
    append: RefCell<Option<Value>>,
    failure: Option<PortfolioFailure>,
    mapped: RefCell<Vec<SyncedOrderRoot>>,
}
fn raw_items(v: &Value) -> ReadItems {
    Rc::new(RefCell::new(
        v.as_array()
            .unwrap()
            .iter()
            .cloned()
            .map(|v| Rc::new(RefCell::new(Input::from(v))))
            .collect(),
    ))
}
fn markets(v: &Value) -> PrivateOrderMarkets {
    Rc::new(RefCell::new(
        v.as_array()
            .unwrap()
            .iter()
            .map(|v| {
                let m: Market = serde_json::from_value(v.clone()).unwrap();
                (m.id.clone(), Rc::new(RefCell::new(m)))
            })
            .collect(),
    ))
}
impl OrderSnapshotHost for SnapshotHost {
    fn public_only(&self) -> bool {
        self.input["publicOnly"] == true
    }
    fn supported_types(&self) -> OrderSyncAsync<Rc<RefCell<Vec<String>>>> {
        self.trace.next("supported");
        Ok(ready(
            &self.q,
            Ok(Rc::new(RefCell::new(
                self.input["supported"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().into())
                    .collect(),
            ))),
        ))
    }
    fn get_orders(
        &self,
        endpoint: &'static str,
        query: OrderPageQuery,
    ) -> OrderSyncAsync<OrderPage> {
        let e = self.trace.next("get");
        assert_eq!(
            endpoint,
            e["endpoint"].as_str().unwrap(),
            "native snapshot endpoint"
        );
        let p = &e["params"];
        assert_eq!(
            query.product_type,
            p["product_type"].as_str(),
            "native request product type"
        );
        assert_eq!(
            query.contract_expiry_type,
            p["contract_expiry_type"].as_str(),
            "native request expiry"
        );
        assert_eq!(
            query.order_status,
            p["order_status"].as_str().unwrap(),
            "native snapshot status order"
        );
        num(query.limit, &p["limit"], "native snapshot page limit");
        value(
            query
                .cursor
                .as_ref()
                .map(|v| v.clone().into_value().unwrap())
                .as_ref(),
            p.get("cursor").unwrap_or(&json!({"$undefined":true})),
            "native pagination cursor",
        );
        let (p, error) = self
            .pages
            .borrow_mut()
            .pop_front()
            .expect("independent planned page");
        self.roots
            .borrow_mut()
            .extend(p.orders.borrow().iter().cloned());
        Ok(ready(
            &self.q,
            match error {
                Some(e) => Err(e),
                None => Ok(p),
            },
        ))
    }
    fn resolve_market_by_product(
        &self,
        product: String,
    ) -> Result<Option<ReadMarket>, PortfolioFailure> {
        let e = self.trace.next("resolve");
        assert_eq!(
            product,
            e["product"].as_str().unwrap(),
            "native resolver product"
        );
        if self.input["mutateResolve"] == true {
            for raw in self.roots.borrow().iter() {
                if raw
                    .borrow()
                    .get("product_id")
                    .is_some_and(|v| v.text_equals(&product))
                {
                    retained_mutation::set(
                        &mut raw.borrow_mut(),
                        "product_id",
                        json!("changed").into(),
                    )
                }
            }
        }
        if product == "BTC-USD" {
            if let Some(raw) = self.append.borrow_mut().take() {
                let raw = Rc::new(RefCell::new(Input::from(raw)));
                self.first_page.borrow_mut().push(raw.clone());
                self.roots.borrow_mut().push(raw);
            }
        }
        Ok(self.markets.borrow().get(&product).cloned())
    }
    fn map_rest_order(
        &self,
        raw: ReadItem,
        market: ReadMarket,
        now: f64,
    ) -> Result<Option<SyncedOrderRoot>, PortfolioFailure> {
        assert!(
            self.roots.borrow().iter().any(|r| Rc::ptr_eq(r, &raw)),
            "mapper receives original raw root"
        );
        assert!(
            self.markets
                .borrow()
                .values()
                .any(|m| Rc::ptr_eq(m, &market)),
            "mapper receives original market root"
        );
        let order = safe_cex_venue_coinbase::orders::map_coinbase_rest_order(
            &*raw.borrow(),
            &market.borrow(),
            now,
        );
        Ok(order.map(|mut fields| {
            fields.info = (None).into();
            let root = Rc::new(RefCell::new(SyncedOrder { fields, info: raw }));
            self.mapped.borrow_mut().push(root.clone());
            root
        }))
    }
    fn now_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(37.0)
    }
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("emit");
        assert_eq!(
            message,
            e["message"].as_str().unwrap(),
            "native snapshot diagnostic summary"
        );
        Ok(())
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("ordersError");
        assert!(
            Rc::ptr_eq(&error, self.failure.as_ref().unwrap()),
            "fetch wrapper emits original error carrier"
        );
        assert_error(&error, &e["error"], "native emitted order error");
        Ok(())
    }
}
#[test]
fn snapshot_pagination_native_orders_and_fetch_error_identity() {
    let f = fixture();
    let s = source();
    for (input, e) in f["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["snapshots"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let pages: VecDeque<_> = input["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    OrderPage {
                        orders: p.get("orders").map(raw_items).unwrap_or_default(),
                        has_next: p.get("has_next").cloned().map(Input::from),
                        cursor: p.get("cursor").cloned().map(Input::from),
                    },
                    p.get("error").cloned().map(failure),
                )
            })
            .collect();
        let first_page = pages
            .front()
            .map(|p| p.0.orders.clone())
            .unwrap_or_default();
        let error = pages.front().and_then(|p| p.1.clone());
        let host = SnapshotHost {
            q: q.clone(),
            input: input.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
            pages: RefCell::new(pages),
            roots: Rc::default(),
            markets: markets(&input["markets"]),
            first_page,
            append: RefCell::new(input.get("appendResolve").cloned()),
            failure: error,
            mapped: RefCell::new(vec![]),
        };
        let r = Rc::new(OrderSnapshotReceiver {
            host,
            spawn: q.spawner(),
        });
        let p = if input["fetchWrapper"] == true {
            fetch_orders(r.clone())
        } else {
            fetch_open_orders_snapshot(r.clone())
        };
        q.drain();
        match completed(p) {
            Ok(v) => {
                assert!(e["error"].is_null());
                orders(
                    &v,
                    &e["value"],
                    &e["info"],
                    &r.host.roots.borrow(),
                    input["label"].as_str().unwrap(),
                );
                for order in v.borrow().iter() {
                    assert!(
                        r.host.mapped.borrow().iter().any(|o| Rc::ptr_eq(o, order)),
                        "snapshot returns original mapper order root"
                    );
                }
                if input["publicOnly"] == true {
                    let p = fetch_open_orders_snapshot(r.clone());
                    q.drain();
                    assert!(
                        !Rc::ptr_eq(&v, &completed(p).unwrap()),
                        "public-only snapshot returns fresh array"
                    );
                }
            }
            Err(err) => {
                assert_error(&err, &e["error"], "native snapshot error");
                assert!(
                    Rc::ptr_eq(&err, r.host.failure.as_ref().unwrap()),
                    "fetch wrapper rethrows original failure"
                );
                assert_eq!(e["sameError"], true)
            }
        }
        r.host.trace.done();
        assert!(
            r.host.pages.borrow().is_empty(),
            "independent page plan consumed"
        );
    }
}
struct EventHost {
    input: Value,
    trace: Trace,
    console: Trace,
    roots: Rc<RefCell<Vec<ReadItem>>>,
    markets: PrivateOrderMarkets,
    raw_orders: ReadItems,
    append: RefCell<Option<Value>>,
    captured_map: RefCell<Option<PrivateOrderMarkets>>,
    selected: RefCell<Vec<ReadMarket>>,
}
impl EventHost {
    fn store(
        &self,
        kind: &str,
        orders: SyncedOrders,
        count: Option<f64>,
    ) -> Result<(), PortfolioFailure> {
        let e = self.trace.next(kind);
        if let Some(count) = count {
            num(count, &e["count"], "native raw snapshot count")
        }
        crate::orders(
            &orders,
            &e["orders"],
            &e["info"],
            &self.roots.borrow(),
            self.input["label"].as_str().unwrap(),
        );
        if self.input["mutateStore"] == true {
            let o = orders.borrow()[0].clone();
            let mut o = o.borrow_mut();
            retained_mutation::set(&mut o.info.borrow_mut(), "status", json!("FILLED").into());
            retained_mutation::set(&mut o.info.borrow_mut(), "avg_price", json!("").into());
            o.fields.symbol = "changed".into();
            o.fields.filled = 9.0;
        }
        Ok(())
    }
}
impl PrivateOrderEventHost for EventHost {
    fn markets_by_id(&self) -> Result<PrivateOrderMarkets, PortfolioFailure> {
        Ok(self.markets.clone())
    }
    fn resolve_coinbase_market(
        &self,
        id: String,
        map: PrivateOrderMarkets,
    ) -> Result<Option<ReadMarket>, PortfolioFailure> {
        assert!(
            Rc::ptr_eq(&map, &self.markets),
            "resolve wrapper passes original markets root"
        );
        *self.captured_map.borrow_mut() = Some(map.clone());
        if self.input["mutateResolve"] == true {
            for raw in self.roots.borrow().iter() {
                let mut raw = raw.borrow_mut();
                retained_mutation::set(&mut raw, "cumulative_quantity", json!("99").into());
                retained_mutation::set(&mut raw, "leaves_quantity", json!("98").into());
                retained_mutation::set(&mut raw, "limit_price", json!("11").into());
            }
        }
        if let Some(raw) = self.append.borrow_mut().take() {
            let raw = Rc::new(RefCell::new(Input::from(raw)));
            self.raw_orders.borrow_mut().push(raw.clone());
            self.roots.borrow_mut().push(raw);
        }
        let alias =
            safe_cex_venue_coinbase::utils::get_coinbase_usd_equivalent_alias_product_id(&id);
        let map = map.borrow();
        let exact = map.get(&id).cloned();
        let aliased = alias.as_ref().and_then(|id| map.get(id)).cloned();
        let selected = if id.ends_with("-USD") && aliased.is_some() {
            aliased
        } else {
            exact.or(aliased)
        };
        if let Some(v) = &selected {
            self.selected.borrow_mut().push(v.clone());
        }
        Ok(selected)
    }
    fn emit_and_log_error(&self, message: String) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("emit");
        assert_eq!(
            message,
            e["message"].as_str().unwrap(),
            "native private quarantine message"
        );
        Ok(())
    }
    fn console_error(&self, message: &str, v: &str) -> Result<(), PortfolioFailure> {
        let e = self.console.next("console");
        assert_eq!(message, e["message"].as_str().unwrap());
        assert_eq!(v, e["value"].as_str().unwrap());
        Ok(())
    }
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(37.0)
    }
    fn snapshot_active(&self) -> Result<bool, PortfolioFailure> {
        Ok(self.input["snapshotActive"] == true)
    }
    fn merge_snapshot_orders(&self, v: SyncedOrders) -> Result<(), PortfolioFailure> {
        self.store("merge", v, None)
    }
    fn apply_order_snapshot_batch(
        &self,
        count: f64,
        v: SyncedOrders,
    ) -> Result<(), PortfolioFailure> {
        self.store("snapshot", v, Some(count))
    }
    fn add_or_update_orders(&self, v: SyncedOrders) -> Result<(), PortfolioFailure> {
        self.store("add", v, None)
    }
    fn emit_fill(&self, fill: PrivateOrderFill) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("fill");
        assert_eq!(e["event"], "fill");
        let v = &e["value"];
        assert_eq!(fill.id, v["id"].as_str().unwrap(), "native fill id");
        assert_eq!(
            fill.symbol,
            v["symbol"].as_str().unwrap(),
            "native fill symbol after store callback"
        );
        assert_eq!(
            fill.side,
            if v["side"] == "buy" {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            }
        );
        retained_info::assert_info(
            Some(&fill.price),
            &v["price"],
            "native fill retains avg-price scalar",
        );
        num(
            fill.amount,
            &v["amount"],
            "native fill quantity after store callback",
        );
        Ok(())
    }
}
#[test]
fn private_event_native_values_before_store_and_fill() {
    let f = fixture();
    let s = source();
    for (input, e) in f["events"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["events"].as_array().unwrap())
    {
        let raw = raw_items(&input["event"]["orders"]);
        let host = EventHost {
            input: input.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
            console: Trace::new(&e["console"], "console ordering"),
            roots: Rc::new(RefCell::new(raw.borrow().clone())),
            markets: markets(&input["markets"]),
            raw_orders: raw.clone(),
            append: RefCell::new(input.get("appendResolve").cloned()),
            captured_map: RefCell::new(None),
            selected: RefCell::new(vec![]),
        };
        let event = Rc::new(RefCell::new(PrivateOrderEvent {
            event_type: input["event"]["type"].as_str().map(str::to_owned),
            orders: raw,
        }));
        handle_user_event_orders(&host, event).unwrap();
        assert!(e["error"].is_null());
        host.trace.done();
        host.console.done();
    }
}
#[test]
fn market_resolver_retains_map_product_and_selected_root() {
    assert_eq!(
        source()["resolverProof"],
        json!({"prefersAlias":true,"missingUndefined":true})
    );
    let f = fixture();
    let input = f["events"][0].clone();
    let map = markets(&input["markets"]);
    let usd = map.borrow()["BTC-USD"].clone();
    let mut usdc = usd.borrow().clone();
    usdc.id = "BTC-USDC".into();
    usdc.symbol = "selected-USDC".into();
    let usdc = Rc::new(RefCell::new(usdc));
    map.borrow_mut().insert("BTC-USDC".into(), usdc.clone());
    let host = EventHost {
        input,
        trace: Trace::new(&json!([]), "resolver"),
        console: Trace::new(&json!([]), "resolver"),
        roots: Rc::default(),
        markets: map.clone(),
        raw_orders: Rc::default(),
        append: RefCell::new(None),
        captured_map: RefCell::new(None),
        selected: RefCell::new(vec![]),
    };
    assert!(
        Rc::ptr_eq(
            &resolve_order_market(&host, "BTC-USD".into())
                .unwrap()
                .unwrap(),
            &usdc
        ),
        "resolver returns selected USDC root"
    );
    assert!(Rc::ptr_eq(
        host.captured_map.borrow().as_ref().unwrap(),
        &map
    ));
    assert!(
        resolve_order_market(&host, "MISSING".into())
            .unwrap()
            .is_none(),
        "resolver forwards requested product"
    );
}
struct TickState {
    callbacks: RefCell<Vec<TickLoopCallback>>,
    input: Value,
    trace: Trace,
    loops: Trace,
    q: Microtasks,
    disposed: Cell<bool>,
    loaded: RefCell<Frame>,
    roots: IndexMap<String, Frame>,
    registry: RefCell<TickRegistry>,
    original: TickRegistry,
    failure: PortfolioFailure,
    loop_failure: PortfolioFailure,
    interval_failure: PortfolioFailure,
}
impl TickState {
    fn fetch(&self, key: &str) -> TickAsync<Frame> {
        let e = self.trace.next("fetch");
        assert_eq!(e["key"], key, "native sequential fetch order");
        if self.input["disposeAt"] == key {
            self.disposed.set(true)
        }
        Ok(ready(
            &self.q,
            if self.input["fail"] == key {
                Err(self.failure.clone())
            } else {
                Ok(self.roots[key].clone())
            },
        ))
    }
}
fn callback(state: Rc<TickState>, label: &'static str) -> TickCallback {
    Rc::new(move |ticker| {
        let e = state.trace.next("callback");
        assert_eq!(e["label"], label, "native handler after registry mutation");
        frame(
            Some(ticker.clone()),
            &e["ticker"],
            "native callback ticker before mutation",
        );
        let Frame::Array(root) = &state.roots["tickers"] else {
            panic!("ticker array")
        };
        assert_eq!(
            same(&ticker, &root.borrow()[0]),
            e["same"].as_bool().unwrap(),
            "callback receives original ticker root"
        );
        if state.input["fail"] == "callback" {
            return Err(state.failure.clone());
        }
        if label == "book" && state.input["mutateCallback"] == true {
            set(&ticker, "symbol", Frame::from_json(&json!("B")));
            state.original.borrow_mut().insert(
                "poll_kline.B".encode_utf16().collect(),
                Rc::new(RefCell::new(vec![callback(state.clone(), "changed-kline")])),
            );
            *state.registry.borrow_mut() = Rc::default();
        }
        if label == "book" && state.input["appendCallback"] == true {
            let handlers = state
                .original
                .borrow()
                .get(&"poll_book.A".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .clone();
            handlers
                .borrow_mut()
                .push(callback(state.clone(), "appended-handler"));
            root.borrow_mut().push(Frame::from_json(
                &json!({"symbol":"C","bid":4,"ask":5,"last":6}),
            ));
        }
        Ok(())
    })
}
struct TickAdapter(Rc<TickState>);
impl std::ops::Deref for TickAdapter {
    type Target = TickState;
    fn deref(&self) -> &TickState {
        &self.0
    }
}
impl TickHost for TickAdapter {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn fetch_tickers(&self) -> TickAsync<Frame> {
        self.fetch("tickers")
    }
    fn fetch_balance(&self) -> TickAsync<Frame> {
        self.fetch("balance")
    }
    fn fetch_positions(&self) -> TickAsync<Frame> {
        self.fetch("positions")
    }
    fn fetch_open_orders_snapshot(&self) -> TickAsync<Frame> {
        self.fetch("orders")
    }
    fn loaded(&self) -> Result<Frame, PortfolioFailure> {
        Ok(self.loaded.borrow().clone())
    }
    fn update(&self, fields: TickUpdate) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("update");
        let expected = e["fields"].as_object().unwrap();
        assert_eq!(fields.len(), 2, "independent native update field count");
        assert_eq!(fields.len(), expected.len());
        for (k, v) in &fields {
            frame(Some(v.clone()), &expected[k], "native store update values");
        }
        let key = fields.keys().next().unwrap();
        assert!(
            same(&fields[key], &self.roots[key]),
            "update retains original fetch value root"
        );
        assert_eq!(e["sameValue"], true);
        let new_loaded = fields["loaded"].clone();
        assert!(
            !same(&new_loaded, &self.loaded.borrow()),
            "loaded spread creates fresh record"
        );
        *self.loaded.borrow_mut() = new_loaded;
        Ok(())
    }
    fn public_message_handlers(&self) -> Result<TickRegistry, PortfolioFailure> {
        Ok(self.registry.borrow().clone())
    }
    fn should_poll_orders(&self) -> Result<bool, PortfolioFailure> {
        self.trace.next("poll");
        Ok(self.input["poll"] == true)
    }
    fn merge_rest_orders_with_fresh_ws_state(
        &self,
        orders: Frame,
    ) -> Result<Frame, PortfolioFailure> {
        let e = self.trace.next("merge");
        assert!(
            same(&orders, &self.roots["orders"]),
            "merge receives original snapshot root"
        );
        assert_eq!(e["same"], true);
        Ok(orders)
    }
    fn emit_orders_fetch_error(&self, error: PortfolioFailure) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("ordersError");
        assert!(
            Rc::ptr_eq(&error, &self.failure),
            "inner error carrier identity"
        );
        assert_error(&error, &e["error"], "native inner order error");
        assert_eq!(e["same"], true);
        Ok(())
    }
    fn emit_and_log_error(
        &self,
        error: PortfolioFailure,
        message: Option<Value>,
    ) -> Result<(), PortfolioFailure> {
        let e = self.trace.next("error");
        assert!(
            Rc::ptr_eq(&error, &self.failure),
            "outer error carrier identity"
        );
        assert_error(&error, &e["error"], "native outer error");
        value(
            message.as_ref(),
            &e["message"],
            "native error message projection",
        );
        assert_eq!(e["same"], true);
        Ok(())
    }
    fn get_tick_interval(&self) -> Result<f64, PortfolioFailure> {
        self.trace.next("interval");
        if self.input.get("intervalError").is_some() {
            Err(self.interval_failure.clone())
        } else {
            Ok(10000.0)
        }
    }
    fn loop_again(&self, cb: TickLoopCallback, delay: f64) -> TickAsync<()> {
        self.callbacks.borrow_mut().push(cb);
        let e = self.loops.next("loop");
        num(delay, &e["delay"], "native scheduling delay");
        Ok(ready(
            &self.q,
            if self.input.get("loopError").is_some() {
                Err(self.loop_failure.clone())
            } else {
                Ok(())
            },
        ))
    }
}
#[test]
fn polling_tick_native_roots_disposal_callbacks_and_detached_loop() {
    let f = fixture();
    let s = source();
    for (input, e) in f["ticks"]
        .as_array()
        .unwrap()
        .iter()
        .zip(s["ticks"].as_array().unwrap())
    {
        let q = Microtasks::default();
        let registry: TickRegistry = Rc::default();
        let state = Rc::new(TickState {
            callbacks: RefCell::new(vec![]),
            input: input.clone(),
            trace: Trace::new(&e["trace"], input["label"].as_str().unwrap()),
            loops: Trace::new(&e["loops"], "loop"),
            q: q.clone(),
            disposed: Cell::new(input["disposed"] == true),
            loaded: RefCell::new(Frame::from_json(&json!({"extra":true}))),
            roots: IndexMap::from([
                ("tickers".into(), Frame::from_json(&input["tickers"])),
                ("balance".into(), Frame::from_json(&json!({"total":7}))),
                (
                    "positions".into(),
                    Frame::from_json(&json!([{"symbol":"X"}])),
                ),
                ("orders".into(), Frame::from_json(&json!([{"id":"o"}]))),
            ]),
            registry: RefCell::new(registry.clone()),
            original: registry.clone(),
            failure: failure(
                json!({"name":"Error","message":format!("failed {}",input["fail"].as_str().unwrap_or("undefined"))}),
            ),
            loop_failure: failure(input.get("loopError").cloned().unwrap_or(Value::Null)),
            interval_failure: failure(input.get("intervalError").cloned().unwrap_or(Value::Null)),
        });
        for ticker in input["tickers"].as_array().unwrap() {
            let symbol = ticker["symbol"].as_str().unwrap();
            for (prefix, label) in [("poll_book.", "book"), ("poll_kline.", "kline")] {
                registry.borrow_mut().insert(
                    format!("{prefix}{symbol}").encode_utf16().collect(),
                    Rc::new(RefCell::new(vec![callback(state.clone(), label)])),
                );
            }
        }
        let r = Rc::new(TickReceiver {
            host: TickAdapter(state.clone()),
            spawn: q.spawner(),
        });
        let p = tick(r);
        q.drain();
        match completed(p) {
            Ok(()) => assert!(
                e["error"].is_null(),
                "detached loop rejection must not reject tick"
            ),
            Err(err) => {
                assert!(
                    !e["error"].is_null(),
                    "detached loop rejection must not reject tick"
                );
                assert_error(&err, &e["error"], "native tick rejection");
                assert!(
                    Rc::ptr_eq(&err, &state.interval_failure),
                    "interval throw preserves original carrier"
                )
            }
        }
        state.trace.done();
        state.loops.done();
        if input["label"] == "normal tick without order poll" {
            *state.trace.expected.borrow_mut() = e["repeatedTrace"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect();
            *state.loops.expected.borrow_mut() = e["repeatedLoops"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .collect();
            let callback = state.callbacks.borrow_mut().remove(0);
            let repeated = callback();
            q.drain();
            completed(repeated).unwrap();
            state.trace.done();
            state.loops.done();
        }
        state.callbacks.borrow_mut().clear();
        state.original.borrow_mut().clear();
    }
}
