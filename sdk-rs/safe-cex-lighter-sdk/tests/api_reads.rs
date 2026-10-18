use num_bigint::BigInt;
use safe_cex_lighter_sdk::{api_reads::*, key_runtime::KeyFuture, signature_wire::*};
use serde_json::{Value as J, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Big(BigInt),
    Array(Rc<RefCell<Vec<V>>>),
    Object(Rc<RefCell<BTreeMap<String, V>>>),
}
#[derive(Clone, Debug)]
struct E {
    name: &'static str,
    message: V,
    code: V,
}
fn text(s: &str) -> V {
    V::Text(s.encode_utf16().collect())
}
fn decode(j: &J) -> V {
    match j {
        J::Null => V::Null,
        J::Bool(b) => V::Bool(*b),
        J::Array(a) => V::Array(Rc::new(RefCell::new(a.iter().map(decode).collect()))),
        J::Object(o) if o.contains_key("$undefined") => V::Undefined,
        J::Object(o) if o.contains_key("$units") => V::Text(
            o["$units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|u| u.as_u64().unwrap() as u16)
                .collect(),
        ),
        J::Object(o) if o.contains_key("$number") => V::Number(if o["$number"] == "NaN" {
            f64::NAN
        } else {
            f64::from_bits(u64::from_str_radix(o["$number"].as_str().unwrap(), 16).unwrap())
        }),
        J::Object(o) if o.contains_key("$bigint") => {
            V::Big(o["$bigint"].as_str().unwrap().parse().unwrap())
        }
        J::Object(o) => V::Object(Rc::new(RefCell::new(
            o.iter().map(|(k, v)| (k.clone(), decode(v))).collect(),
        ))),
        _ => panic!("encoded source value"),
    }
}
fn encode(v: &V) -> J {
    match v {
        V::Undefined => json!({"$undefined":true}),
        V::Null => J::Null,
        V::Bool(b) => json!(b),
        V::Number(n) => {
            json!({"$number":if n.is_nan(){"NaN".into()}else{format!("{:016x}",n.to_bits())}})
        }
        V::Text(s) => json!({"$units":s}),
        V::Big(b) => json!({"$bigint":b.to_string()}),
        V::Array(a) => J::Array(a.borrow().iter().map(encode).collect()),
        V::Object(o) => J::Object(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), encode(v)))
                .collect(),
        ),
    }
}
fn corpus() -> &'static J {
    static C: OnceLock<J> = OnceLock::new();
    C.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/api_reads_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "SDK-READ source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
struct Host {
    response: V,
    calls: RefCell<Vec<(Vec<u16>, V)>>,
    tasks: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    now_calls: RefCell<usize>,
    resolve_calls: RefCell<usize>,
    transport: Option<Rc<E>>,
}
impl Host {
    fn new(response: V) -> Self {
        Self {
            response,
            calls: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            now_calls: RefCell::new(0),
            resolve_calls: RefCell::new(0),
            transport: None,
        }
    }
    fn drain(&self) {
        for _ in 0..20 {
            let tasks = self.tasks.take();
            if tasks.is_empty() {
                return;
            }
            let mut pending = vec![];
            for mut f in tasks {
                if f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
                {
                    pending.push(f)
                }
            }
            self.tasks.borrow_mut().extend(pending);
        }
        panic!("SDK-READ scheduler settled");
    }
}
impl ReadHost for Host {
    type Value = V;
    type Error = Rc<E>;
    fn spawn(&self, t: Pin<Box<dyn Future<Output = ()>>>) {
        self.tasks.borrow_mut().push(t)
    }
    fn get(&self, v: &V, key: &str) -> Result<V, Rc<E>> {
        match v {
            V::Object(o) => Ok(o.borrow().get(key).cloned().unwrap_or(V::Undefined)),
            V::Null | V::Undefined => Err(Rc::new(E {
                name: "TypeError",
                message: text(&format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if matches!(v, V::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
                code: V::Undefined,
            })),
            _ => Ok(V::Undefined),
        }
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::Undefined | V::Null => false,
            V::Bool(b) => *b,
            V::Number(n) => *n != 0.0 && !n.is_nan(),
            V::Text(s) => !s.is_empty(),
            V::Big(n) => *n != BigInt::from(0),
            _ => true,
        }
    }
    fn undefined(&self, v: &V) -> bool {
        matches!(v, V::Undefined)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Undefined | V::Null)
    }
    fn number_200(&self, v: &V) -> bool {
        matches!(v,V::Number(n) if *n==200.0)
    }
    fn string_200(&self, v: &V) -> bool {
        matches!(v,V::Text(s) if *s=="200".encode_utf16().collect::<Vec<_>>())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::Array(_))
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, Rc<E>> {
        Ok(match v {
            V::Text(s) => s.clone(),
            V::Big(b) => b.to_string().encode_utf16().collect(),
            V::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
            V::Bool(b) => b.to_string().encode_utf16().collect(),
            V::Undefined => "undefined".encode_utf16().collect(),
            V::Null => "null".encode_utf16().collect(),
            _ => "[object Object]".encode_utf16().collect(),
        })
    }
    fn bigint(&self, v: &V) -> Result<BigInt, Rc<E>> {
        let result = match v {
            V::Big(b) => Some(b.clone()),
            V::Bool(b) => Some(BigInt::from(u8::from(*b))),
            V::Number(n) if n.is_finite() && n.fract() == 0.0 => Some(BigInt::from(*n as i64)),
            V::Text(s) => {
                let s = String::from_utf16(s).unwrap();
                if let Some(s) = s.strip_prefix("0x") {
                    BigInt::parse_bytes(s.as_bytes(), 16)
                } else {
                    s.parse().ok()
                }
            }
            _ => None,
        };
        result.ok_or_else(|| {
            Rc::new(E {
                name: "SyntaxError",
                message: text(&format!(
                    "Cannot convert {} to a BigInt",
                    String::from_utf16(&self.string(v).unwrap()).unwrap()
                )),
                code: V::Undefined,
            })
        })
    }
    fn text(&self, s: Vec<u16>) -> V {
        V::Text(s)
    }
    fn number(&self, n: f64) -> V {
        V::Number(n)
    }
    fn undefined_value(&self) -> V {
        V::Undefined
    }
    fn bigint_value(&self, b: BigInt) -> V {
        V::Big(b)
    }
    fn object(&self, f: Fields<V>) -> V {
        V::Object(Rc::new(RefCell::new(
            f.into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), v))
                .collect(),
        )))
    }
    fn empty_array(&self) -> V {
        V::Array(Rc::new(RefCell::new(vec![])))
    }
    fn copy_properties(&self, v: &V) -> Result<Fields<V>, Rc<E>> {
        Ok(if let V::Object(o) = v {
            o.borrow()
                .iter()
                .map(|(k, v)| (k.encode_utf16().collect(), v.clone()))
                .collect()
        } else {
            vec![]
        })
    }
    fn now(&self) -> Result<f64, Rc<E>> {
        *self.now_calls.borrow_mut() += 1;
        Ok(123456.0)
    }
    fn api_error(&self, m: V, c: V) -> Result<Rc<E>, Rc<E>> {
        Ok(Rc::new(E {
            name: "LighterApiError",
            message: m,
            code: c,
        }))
    }
    fn error(&self, m: V) -> Result<Rc<E>, Rc<E>> {
        Ok(Rc::new(E {
            name: "Error",
            message: m,
            code: V::Undefined,
        }))
    }
    fn http_get(
        &self,
        axios: bool,
        url: Vec<u16>,
        config: V,
    ) -> Result<KeyFuture<V, Rc<E>>, Rc<E>> {
        self.calls.borrow_mut().push((url, config));
        if let Some(e) = &self.transport {
            let e = e.clone();
            return Ok(Box::pin(async move { Err(e) }));
        }
        let v = if axios {
            self.object(vec![(
                "data".encode_utf16().collect(),
                self.response.clone(),
            )])
        } else {
            self.response.clone()
        };
        Ok(Box::pin(async move { Ok(v) }))
    }
    fn promise_resolve(&self, v: V) -> KeyFuture<V, Rc<E>> {
        *self.resolve_calls.borrow_mut() += 1;
        Box::pin(async move { Ok(v) })
    }
}
fn bigint(v: &V) -> BigInt {
    if let V::Big(b) = v {
        b.clone()
    } else {
        panic!("declared bigint")
    }
}
fn number(v: &V) -> f64 {
    if let V::Number(n) = v {
        *n
    } else {
        panic!("declared number")
    }
}
fn opt_number(v: &V) -> Option<f64> {
    if matches!(v, V::Undefined) {
        None
    } else {
        Some(number(v))
    }
}
fn opt_text(v: &V) -> Option<Vec<u16>> {
    if let V::Text(s) = v {
        Some(s.clone())
    } else {
        None
    }
}
fn ready<T>(mut f: KeyFuture<T, Rc<E>>) -> Result<T, Rc<E>> {
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("SDK-READ completed promise"),
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
#[test]
fn referral_constructor_retains_routes_replaces_and_releases_exact_client() {
    assert_eq!(
        corpus()["constructorOwnership"],
        json!({
            "sameClient":true,"enumerable":true,"writable":true,"configurable":true,
            "results":["first","second"],"deleted":true,
            "calls":[
                {"label":"first","url":"/api/v1/referral/points","index":"1"},
                {"label":"second","url":"/api/v1/referral/points","index":"2"}
            ]
        }),
        "REFERRAL constructor source property and routing evidence"
    );
    let first_data = decode(&json!({"label":encode(&text("first"))}));
    let first = Rc::new(Host::new(decode(
        &json!({"success":true,"data":{"label":encode(&text("first"))}}),
    )));
    let first_weak = Rc::downgrade(&first);
    let mut api = ReferralApi::new(first.clone());
    assert!(
        Rc::ptr_eq(&api.client, &first),
        "REFERRAL constructor exact client"
    );
    assert_eq!(
        Rc::strong_count(&first),
        2,
        "REFERRAL constructor one retained client"
    );
    assert!(
        first.calls.borrow().is_empty(),
        "REFERRAL constructor has no request"
    );
    assert!(
        first.tasks.borrow().is_empty(),
        "REFERRAL constructor has no task"
    );
    drop(first);
    let pending = api.get_referral_points(BigInt::from(1), None);
    api.client.drain();
    assert_eq!(encode(&ready(pending).unwrap()), encode(&first_data));
    assert_eq!(api.client.calls.borrow().len(), 1);
    assert_eq!(
        Rc::strong_count(&api.client),
        1,
        "REFERRAL settled request releases client"
    );

    let second = Rc::new(Host::new(decode(
        &json!({"success":true,"data":{"label":encode(&text("second"))}}),
    )));
    let second_weak = Rc::downgrade(&second);
    api.client = second.clone();
    assert!(
        first_weak.upgrade().is_none(),
        "REFERRAL replacement releases first client"
    );
    let pending = api.get_referral_points(BigInt::from(2), None);
    second.drain();
    assert_eq!(
        encode(&ready(pending).unwrap()),
        json!({"label":encode(&text("second"))})
    );
    assert_eq!(second.calls.borrow().len(), 1);
    assert_eq!(
        second.calls.borrow()[0].0,
        "/api/v1/referral/points".encode_utf16().collect::<Vec<_>>()
    );
    let config = second.calls.borrow()[0].1.clone();
    let params = second.get(&config, "params").unwrap();
    assert_eq!(
        encode(&second.get(&params, "accountIndex").unwrap()),
        encode(&text("2"))
    );
    drop(api);
    assert_eq!(
        Rc::strong_count(&second),
        1,
        "REFERRAL facade releases client"
    );
    drop(second);
    assert!(second_weak.upgrade().is_none(), "REFERRAL no leaked client");
}

#[test]
fn source_read_bodies_keep_requests_results_and_eager_ownership() {
    for (i, c) in corpus()["cases"].as_array().unwrap().iter().enumerate() {
        let kind = c["kind"].as_str().unwrap();
        let args = if let V::Array(a) = decode(&c["args"]) {
            a.borrow().clone()
        } else {
            unreachable!()
        };
        let payload = decode(&c["payload"]);
        let h = Rc::new(Host::new(payload.clone()));
        let f: KeyFuture<V, Rc<E>> = match kind {
            "referral" => ReferralApi::new(h.clone())
                .get_referral_points(bigint(&args[0]), opt_text(&args[1])),
            "nonce" => {
                let f = get_next_nonce(h.clone(), bigint(&args[0]), number(&args[1]));
                Box::pin(async move { f.await.map(V::Big) })
            }
            "keys" => get_api_keys(h.clone(), bigint(&args[0]), number(&args[1])),
            "fee" => get_transfer_fee_info(
                h.clone(),
                bigint(&args[0]),
                bigint(&args[1]),
                opt_text(&args[2]).unwrap(),
            ),
            "orders" => get_order_book_orders(
                h.clone(),
                number(&args[0]),
                opt_number(&args[1]),
                args[2].clone(),
            ),
            _ => unreachable!(),
        };
        assert_eq!(
            h.calls.borrow().len(),
            1,
            "SDK-READ eager dispatch {kind} {i}"
        );
        h.drain();
        let got = ready(f);
        let calls = J::Array(
            h.calls
                .borrow()
                .iter()
                .map(|(u, c)| json!([encode(&V::Text(u.clone())), encode(c)]))
                .collect(),
        );
        assert_eq!(calls, c["calls"], "SDK-READ request {kind} {i}");
        match got {
            Ok(v) => {
                assert_eq!(encode(&v), c["result"]["ok"], "SDK-READ result {kind} {i}");
                if c["result"]["same"] == true {
                    let expected = if kind == "keys" {
                        h.get(&h.get(&payload, "data").unwrap(), "apiKeys").unwrap()
                    } else {
                        h.get(&payload, "data").unwrap()
                    };
                    assert!(
                        same(&v, &expected)
                            || matches!((&v, &expected), (V::Undefined, V::Undefined)),
                        "SDK-READ native identity {kind} {i}"
                    );
                }
                if kind == "orders" && h.number_200(&h.get(&payload, "code").unwrap()) {
                    let V::Number(n) = h.get(&v, "marketId").unwrap() else {
                        unreachable!()
                    };
                    assert_eq!(
                        n.to_bits(),
                        number(&args[0]).to_bits(),
                        "SDK-READ native market bits"
                    );
                }
                if kind == "fee" {
                    let V::Number(n) = h.get(&v, "feePercentage").unwrap() else {
                        unreachable!()
                    };
                    assert_eq!(n.to_bits(), (-0.0f64).to_bits(), "SDK-READ native fee bits");
                    let V::Big(b) = h.get(&v, "fee").unwrap() else {
                        unreachable!()
                    };
                    assert_eq!(
                        b.to_string(),
                        "99999999999999999999999",
                        "SDK-READ native big fee"
                    );
                }
            }
            Err(e) => {
                assert_eq!(
                    json!({"name":e.name,"message":encode(&e.message),"code":encode(&e.code)}),
                    c["result"]["error"],
                    "SDK-READ error {kind} {i}"
                );
            }
        }
    }
    let h = Rc::new(Host::new(decode(
        &json!({"success":true,"data":{"live":true}}),
    )));
    let api = ReferralApi::new(h.clone());
    assert!(
        Rc::ptr_eq(&api.client, &h),
        "SDK-READ constructor client identity"
    );
    assert_eq!(Rc::strong_count(&h), 2, "SDK-READ constructor ownership");
    let promise = api.get_referral_points(BigInt::from(1), None);
    assert_eq!(
        h.calls.borrow().len(),
        1,
        "SDK-READ ignored promise eager request"
    );
    drop(promise);
    h.drain();
    assert_eq!(
        *h.resolve_calls.borrow(),
        1,
        "SDK-READ ignored promise continuation"
    );
    let failure = Rc::new(E {
        name: "opaque",
        message: text("original"),
        code: V::Undefined,
    });
    let mut host = Host::new(V::Null);
    host.transport = Some(failure.clone());
    let host = Rc::new(host);
    let f = get_api_keys(host.clone(), BigInt::from(1), 0.0);
    host.drain();
    assert!(
        Rc::ptr_eq(&ready(f).unwrap_err(), &failure),
        "SDK-READ original transport error"
    );
    assert_eq!(
        corpus()["reflection"],
        json!({"keys":["client"],"clientEnumerable":true,"methodEnumerable":false,"instanceof":true,"clientDeletable":true}),
        "SDK-READ source class divergence witness"
    );
    for witness in corpus()["overrides"].as_array().unwrap() {
        let kind = witness["kind"].as_str().unwrap();
        let calls = &witness["calls"];
        let baseline = &corpus()["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == kind)
            .unwrap()["calls"];
        assert_ne!(
            calls, baseline,
            "SDK-READ source prototype divergence {kind}"
        );
        let expected = encode(&text(if kind == "orders" {
            "patched-number"
        } else {
            "patched-bigint"
        }));
        if kind == "referral" {
            assert_eq!(
                calls[0][1]["params"]["accountIndex"], expected,
                "SDK-READ patched referral index"
            );
        } else if kind == "nonce" {
            assert_eq!(
                calls[0][1]["params"]["account_index"], expected,
                "SDK-READ patched nonce index"
            );
        } else if kind == "orders" {
            assert_eq!(
                calls[0][1]["params"]["market_id"], expected,
                "SDK-READ patched market index"
            );
        } else {
            let ending = if kind == "keys" {
                "api-keys"
            } else {
                "transfer-fee"
            };
            assert_eq!(
                calls[0][0],
                encode(&text(&format!("/api/v1/accounts/patched-bigint/{ending}"))),
                "SDK-READ patched URL receipt"
            );
        }
    }
    assert_eq!(
        corpus()["referralClass"],
        json!(["constructor", "getReferralPoints"]),
        "SDK-READ complete referral class"
    );
}
struct Encoder {
    mode: String,
    calls: RefCell<Vec<(String, Vec<u8>)>>,
}
impl Base64Host for Encoder {
    type Error = String;
    fn has_btoa(&self) -> bool {
        self.mode == "btoa" || self.mode == "throw"
    }
    fn btoa(&self, b: &[u8]) -> Result<Vec<u16>, String> {
        if self.mode == "throw" {
            return Err("encoder fault".into());
        }
        self.calls.borrow_mut().push(("btoa".into(), b.to_vec()));
        Ok(base64(b).encode_utf16().collect())
    }
    fn has_buffer(&self) -> bool {
        self.mode == "buffer"
    }
    fn buffer_base64(&self, b: &[u8]) -> Result<Vec<u16>, String> {
        self.calls.borrow_mut().push(("buffer".into(), b.to_vec()));
        Ok(base64(b).encode_utf16().collect())
    }
    fn error(&self, m: &'static str) -> String {
        m.into()
    }
}
fn base64(b: &[u8]) -> String {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        s.push(table[(n >> 18) as usize] as char);
        s.push(table[((n >> 12) & 63) as usize] as char);
        s.push(if c.len() > 1 {
            table[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        s.push(if c.len() > 2 {
            table[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    s
}
#[test]
fn source_signature_wire_keeps_parseint_bytes_and_encoder_selection() {
    for (i, c) in corpus()["wire"].as_array().unwrap().iter().enumerate() {
        let e = Encoder {
            mode: c["mode"].as_str().unwrap().into(),
            calls: RefCell::new(vec![]),
        };
        let V::Text(input) = decode(&c["input"]) else {
            unreachable!()
        };
        let got = to_base64_from_hex(&e, &input);
        let result = match got {
            Ok(s) => json!({"ok":encode(&V::Text(s))}),
            Err(s) => json!({"error":s}),
        };
        assert_eq!(result, c["result"], "SDK-READ signature result {i}");
        assert_eq!(
            json!(*e.calls.borrow()),
            c["calls"],
            "SDK-READ signature encoder {i}"
        );
    }
}

#[test]
fn signature_intrinsic_parser_divergence_is_explicit() {
    let e = Encoder {
        mode: "btoa".into(),
        calls: RefCell::new(vec![]),
    };
    let native = to_base64_from_hex(&e, &"00ff".encode_utf16().collect::<Vec<_>>()).unwrap();
    assert_eq!(
        native,
        "AP8=".encode_utf16().collect::<Vec<_>>(),
        "SDK-READ native intrinsic parser"
    );
    assert_eq!(
        corpus()["wireOverride"],
        encode(&text("ERE=")),
        "SDK-READ source patched parseInt witness"
    );
    assert_ne!(
        encode(&V::Text(native)),
        corpus()["wireOverride"],
        "SDK-READ parser divergence"
    );
}
