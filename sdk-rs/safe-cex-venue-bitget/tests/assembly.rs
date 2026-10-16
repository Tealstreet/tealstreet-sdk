use safe_cex_venue_bitget::assembly::*;
use serde_json::{Value as J, json};
use std::{
    cell::RefCell,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
};
#[derive(Clone)]
enum V {
    U,
    Null,
    B(bool),
    N(f64),
    S(String),
    A(Vec<V>),
    O(Rc<RefCell<Vec<(String, V)>>>),
    F(NativeCallback<V, String>),
    Map,
    Set,
}
fn s(v: &str) -> V {
    V::S(v.into())
}
fn o(fields: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn get(v: &V, k: &str) -> V {
    if let V::O(v) = v {
        v.borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U)
    } else {
        V::U
    }
}
fn text(v: &V) -> String {
    match v {
        V::S(s) => s.clone(),
        V::N(n) => safe_cex_num::js_number_string(*n),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::B(b) => b.to_string(),
        _ => "[object Object]".into(),
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::B(b) => *b,
        V::N(n) => *n != 0. && !n.is_nan(),
        V::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a.to_bits() == b.to_bits(),
        (V::B(a), V::B(b)) => a == b,
        (V::U, V::U) | (V::Null, V::Null) => true,
        _ => false,
    }
}
fn put(v: &V, k: &str, f: V) {
    let V::O(v) = v else { panic!("native object") };
    let mut v = v.borrow_mut();
    if let Some((_, old)) = v.iter_mut().find(|(key, _)| key == k) {
        *old = f
    } else {
        v.push((k.into(), f))
    }
}
fn invoke(f: V, args: Vec<V>) -> Result<V, String> {
    let V::F(f) = f else {
        return Err("native callable required".into());
    };
    f(args)
}
fn wire(v: &V) -> J {
    match v {
        V::U => json!({"u":true}),
        V::Null => J::Null,
        V::B(b) => json!(b),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(s) => json!(s),
        V::F(_) => json!({"fn":true}),
        V::A(v) => json!({"a":v.iter().map(wire).collect::<Vec<_>>()}),
        V::Map => json!({"map":[]}),
        V::Set => json!({"set":[]}),
        V::O(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k,wire(v)])).collect::<Vec<_>>()})
        }
    }
}
#[derive(Clone, Default)]
struct H {
    state: Rc<RefCell<State>>,
}
#[derive(Default)]
struct State {
    events: Vec<String>,
    requests: Vec<V>,
    responses: Vec<V>,
    configs: Vec<V>,
    preimages: Vec<String>,
    emissions: Vec<V>,
    conditions: Vec<V>,
    fail: String,
}
impl H {
    fn event(&self, name: &str) -> Result<(), String> {
        self.state.borrow_mut().events.push(name.into());
        if self.state.borrow().fail == name {
            Err("assembly failure".into())
        } else {
            Ok(())
        }
    }
    fn client(&self) -> V {
        let request = self.clone();
        let response = self.clone();
        o(vec![(
            "interceptors",
            o(vec![
                (
                    "request",
                    o(vec![(
                        "use",
                        V::F(Rc::new(move |v| {
                            request.event("request")?;
                            request.state.borrow_mut().requests.push(v[0].clone());
                            Ok(V::U)
                        })),
                    )]),
                ),
                (
                    "response",
                    o(vec![(
                        "use",
                        V::F(Rc::new(move |v| {
                            response.event("response")?;
                            response.state.borrow_mut().responses.push(v[0].clone());
                            Ok(V::U)
                        })),
                    )]),
                ),
            ]),
        )])
    }
}
impl AssemblyHost for H {
    type Value = V;
    type Error = String;
    fn undefined(&self) -> V {
        V::U
    }
    fn null(&self) -> V {
        V::Null
    }
    fn boolean(&self, v: bool) -> V {
        V::B(v)
    }
    fn number(&self, v: f64) -> V {
        V::N(v)
    }
    fn string(&self, v: &str) -> V {
        s(v)
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn property(&self, v: &V, k: &str) -> Result<V, String> {
        Ok(get(v, k))
    }
    fn define(&self, v: &V, k: &str, f: V) -> Result<(), String> {
        put(v, k, f);
        Ok(())
    }
    fn assign(&self, v: &V, k: &str, f: V) -> Result<(), String> {
        self.define(v, k, f)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn global(&self, k: &str) -> Result<V, String> {
        if k == "BitgetClientOrderIdPattern" {
            return Ok(s(k));
        }
        if matches!(k, "GRAFANA_ENDPOINTS_EVERY" | "GRAFANA_ENDPOINTS_PERIODIC") {
            return Ok(V::A(
                if k == "GRAFANA_ENDPOINTS_EVERY" {
                    safe_cex_venue_bitget::protocol::GRAFANA_ENDPOINTS_EVERY
                } else {
                    safe_cex_venue_bitget::protocol::GRAFANA_ENDPOINTS_PERIODIC
                }
                .iter()
                .map(|v| s(v))
                .collect(),
            ));
        }
        let h = self.clone();
        let name = k.to_string();
        Ok(V::F(Rc::new(move |args| match name.as_str() {
            "BaseExchange" => {
                h.event("base")?;
                let r = o(vec![
                    ("name", args[0].clone()),
                    ("options", args[1].clone()),
                    ("store", args[2].clone()),
                ]);
                let h2 = h.clone();
                let receiver = r.clone();
                put(
                    &r,
                    "createXhrWithSecondaryKey",
                    V::F(Rc::new(move |a| {
                        h2.event("secondary-key")?;
                        let xhr = invoke(a[1].clone(), vec![a[0].clone(), receiver.clone()])?;
                        invoke(get(&a[2], "rateLimit"), vec![xhr.clone()])?;
                        Ok(xhr)
                    })),
                );
                Ok(r)
            }
            "BaseWebSocket" => {
                h.event("socket-base")?;
                Ok(o(vec![("parent", args[0].clone())]))
            }
            "BitgetSecondaryPublicWebsocket" => {
                let index = text(&args[1]);
                h.event(&format!("secondary-{index}"))?;
                Ok(o(vec![
                    ("parent", args[0].clone()),
                    ("index", args[1].clone()),
                ]))
            }
            "Map" => Ok(V::Map),
            "Set" => Ok(V::Set),
            "patternClientOrderIdRule" => Ok(s("rule")),
            "safeCexAxiosCreate" => {
                h.event("axios")?;
                h.state.borrow_mut().configs.push(args[0].clone());
                Ok(h.client())
            }
            "networkErrorOnRead" => {
                h.event("condition")?;
                Ok(V::F(Rc::new(|e| Ok(V::B(text(&e[0]) == "read")))))
            }
            "isBitgetSignatureRejection" => Ok(V::F(Rc::new(|_| Ok(V::B(false))))),
            "signatureRejectedResponse" => {
                h.event("signature-condition")?;
                let predicate = matches!(args[0], V::F(_));
                Ok(V::F(Rc::new(move |e| {
                    Ok(V::B(predicate && text(&e[0]) == "sig"))
                })))
            }
            "resendOnSignatureRejection" => {
                h.event("resend-check")?;
                assert!(
                    matches!(args[2], V::F(_)),
                    "ASSEMBLY native signature predicate"
                );
                Ok(V::U)
            }
            "typeof" => Ok(s(match &args[0] {
                V::U => "undefined",
                V::B(_) => "boolean",
                V::N(_) => "number",
                V::S(_) => "string",
                V::F(_) => "function",
                _ => "object",
            })),
            "WeakSet" => {
                let members: Rc<RefCell<Vec<V>>> = Rc::default();
                let added = members.clone();
                Ok(o(vec![
                    (
                        "has",
                        V::F(Rc::new(move |a| {
                            Ok(V::B(members.borrow().iter().any(|m| same(m, &a[0]))))
                        })),
                    ),
                    (
                        "add",
                        V::F(Rc::new(move |a| {
                            added.borrow_mut().push(a[0].clone());
                            Ok(V::U)
                        })),
                    ),
                ]))
            }
            "retry" => {
                h.event("retry")?;
                h.state
                    .borrow_mut()
                    .conditions
                    .push(get(&args[1], "retryCondition"));
                assert!(
                    same(&get(&args[1], "retries"), &V::N(3.)),
                    "ASSEMBLY native retry count"
                );
                Ok(V::U)
            }
            "rateLimit" => {
                h.event("limit")?;
                assert!(
                    same(&get(&args[1], "maxRPS"), &V::N(3.)),
                    "ASSEMBLY native rate limit"
                );
                Ok(args[0].clone())
            }
            "installTransportAdapter" => {
                h.event("tunnel")?;
                assert!(same(&args[2], &s("bitget")));
                Ok(V::U)
            }
            "virtualClock.getCurrentTimeMs" => Ok(V::N(1234.)),
            "qs.stringify" => {
                let V::O(o) = &args[0] else { panic!() };
                Ok(s(&o
                    .borrow()
                    .iter()
                    .map(|(k, v)| format!("{k}={}", text(v)))
                    .collect::<Vec<_>>()
                    .join("&")))
            }
            "signingQueryTemplate" => Ok(s(&format!("{}?{}", text(&args[0]), text(&args[1])))),
            "signingJoin" => Ok(s(&args
                .iter()
                .map(|v| {
                    if matches!(v, V::U | V::Null) {
                        String::new()
                    } else {
                        text(v)
                    }
                })
                .collect::<Vec<_>>()
                .join(""))),
            "toJsonString" => {
                let V::O(o) = &args[0] else { panic!() };
                let v: J = o
                    .borrow()
                    .iter()
                    .map(|(k, v)| (String::from(k.trim_start_matches('_')), json!(text(v))))
                    .collect();
                Ok(s(&serde_json::to_string(&v).unwrap()))
            }
            "createHmac" => {
                assert!(same(&args[0], &s("sha256")));
                assert!(same(&args[1], &s("secret")));
                let h = h.clone();
                Ok(o(vec![(
                    "update",
                    V::F(Rc::new(move |a| {
                        h.state.borrow_mut().preimages.push(text(&a[0]));
                        Ok(o(vec![(
                            "digest",
                            V::F(Rc::new(move |_| {
                                Ok(o(vec![(
                                    "toString",
                                    V::F(Rc::new(move |a| {
                                        assert!(same(&a[0], &s("base64")));
                                        Ok(s("signature"))
                                    })),
                                )]))
                            })),
                        )]))
                    })),
                )]))
            }
            "AxiosHeaders" => Ok(args[0].clone()),
            "v" => Ok(get(&args[0], &text(&args[1]))),
            "String" => Ok(s(&text(&args[0]))),
            "shouldSuppressValidationErrorEmission" => Ok(V::B(same(
                &get(&args[0], "suppressBitgetValidationErrorEmission"),
                &V::B(true),
            ))),
            "shouldSilenceBitgetResponseError" => {
                Ok(V::B(matches!(text(&args[0]).as_str(), "40084" | "40085")))
            }
            "appendHttpFailure" => Ok(s(&format!("{} [http]", text(&args[0])))),
            _ => Err(format!("unknown native import {name}")),
        })))
    }
    fn member(&self, v: &V, k: &str) -> Result<V, String> {
        if let V::S(value) = v {
            let value = value.clone();
            let key = k.to_string();
            return Ok(V::F(Rc::new(move |a| {
                Ok(match key.as_str() {
                    "replace" => s(&value.replacen(&text(&a[0]), &text(&a[1]), 1)),
                    "toUpperCase" => s(&value.to_uppercase()),
                    "includes" => V::B(value.contains(&text(&a[0]))),
                    _ => panic!("string member"),
                })
            })));
        }
        Ok(get(v, k))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, String> {
        invoke(f, args)
    }
    fn construct(&self, f: V, args: Vec<V>) -> Result<V, String> {
        invoke(f, args)
    }
    fn function(&self, body: NativeCallback<V, String>) -> V {
        V::F(body)
    }
    fn async_function(&self, body: NativeCallback<V, String>) -> V {
        V::F(body)
    }
    fn spread(&self, v: &V) -> Result<V, String> {
        if let V::O(v) = v {
            Ok(V::O(Rc::new(RefCell::new(v.borrow().clone()))))
        } else {
            Ok(o(vec![]))
        }
    }
    fn bind_mapped(&self, _: &V, _: &str, _: &str, targets: &[&str]) -> Result<V, String> {
        assert!(
            !targets.is_empty()
                && targets
                    .iter()
                    .all(|t| t.starts_with("safe-cex-") && t.contains(".rs::")),
            "ASSEMBLY real mapped target"
        );
        Ok(V::F(Rc::new(|_| {
            Err("mapped body not invoked by assembly fixture".into())
        })))
    }
}
fn source(cases: J) -> J {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/assembly-capture.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(cases.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "ASSEMBLY source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn family_construction_native_then_source() {
    let mut cases = vec![];
    let mut actual = vec![];
    for variant in ["classic", "inverse", "usdc"] {
        for testnet in [false, true] {
            let h = H::default();
            let opts = o(vec![("testnet", V::B(testnet))]);
            let store = o(vec![]);
            let r = match variant {
                "inverse" => {
                    BitgetInverseExchange::new(h.clone(), s("bitget"), opts.clone(), store.clone())
                        .unwrap()
                        .inner
                        .receiver
                }
                "usdc" => {
                    BitgetUSDCExchange::new(h.clone(), s("bitget"), opts.clone(), store.clone())
                        .unwrap()
                        .inner
                        .receiver
                }
                _ => {
                    BitgetExchange::new(h.clone(), s("bitget"), opts.clone(), store.clone())
                        .unwrap()
                        .receiver
                }
            };
            assert!(
                same(&get(&r, "options"), &opts),
                "ASSEMBLY native options alias"
            );
            assert!(
                same(&get(&r, "store"), &store),
                "ASSEMBLY native store alias"
            );
            assert!(
                same(&get(&r, "supportsUtaRouting"), &V::B(variant != "inverse")),
                "ASSEMBLY native variant routing"
            );
            for name in [
                "isUta",
                "_accountTypeDetected",
                "_suppressBitgetValidationErrorEmission",
                "privateStreamDegradedReconcileRunning",
                "isPrivateStreamDegraded",
                "hasWarnedPrivateStreamDegraded",
            ] {
                assert!(
                    same(&get(&r, name), &V::B(false)),
                    "ASSEMBLY native exchange default {name}"
                )
            }
            assert!(matches!(get(&r, "_accountTypeDetectionInflight"), V::Null));
            for key in ["_positioningCache", "_positioningInflight"] {
                let V::O(v) = get(&r, key) else { panic!() };
                assert_eq!(v.borrow().len(), 0, "ASSEMBLY native empty cache")
            }
            for key in [
                "publicWebsocket",
                "privateWebsocket",
                "secondsTickersWebsocket",
                "thirdTickersWebsocket",
            ] {
                assert!(
                    same(&get(&get(&r, key), "parent"), &r),
                    "ASSEMBLY native socket parent alias"
                )
            }
            assert!(same(
                &get(&get(&r, "secondsTickersWebsocket"), "index"),
                &V::N(1.)
            ));
            assert!(
                same(&get(&get(&r, "thirdTickersWebsocket"), "index"), &V::N(2.)),
                "ASSEMBLY native third index"
            );
            let private = get(&r, "privateWebsocket");
            for (k, n) in [
                ("MAX_ATTEMPTS", 2.),
                ("privateStreamRecoveryCooldownMs", 30000.),
                ("positionFetchThrottleMs", 500.),
                ("balanceFetchThrottleMs", 300.),
                ("orderTombstoneTtlMs", 10000.),
                ("wsOpenOrderGraceMs", 5000.),
                ("positionRestStaleGuardMs", 5000.),
            ] {
                assert!(
                    same(&get(&private, k), &V::N(n)),
                    "ASSEMBLY native private default {k}"
                )
            }
            assert!(matches!(get(&private, "terminalOrderTombstones"), V::Map));
            assert!(matches!(get(&private, "lastWsPositionSnapshotIds"), V::Set));
            assert_eq!(
                h.state.borrow().configs.len(),
                2,
                "ASSEMBLY native independent API count"
            );
            assert_eq!(
                h.state.borrow().requests.len(),
                2,
                "ASSEMBLY native independent request interceptor count"
            );
            let expected = [
                "base",
                "secondary-key",
                "axios",
                "retry",
                "request",
                "response",
                "limit",
                "axios",
                "retry",
                "request",
                "response",
                "socket-base",
                "socket-base",
                "secondary-1",
                "secondary-2",
            ];
            assert_eq!(
                h.state.borrow().events,
                expected,
                "ASSEMBLY native constructor order"
            );
            cases.push(json!({"variant":variant,"testnet":testnet}));
            actual.push(json!({"exchange":snapshot(&r),"public":snapshot(&get(&r,"publicWebsocket")),"private":snapshot(&private),"events":h.state.borrow().events}));
        }
    }
    assert_eq!(cases.len(), 6, "ASSEMBLY independent family input floor");
    let captured = source(json!({"kind":"family","cases":cases}));
    assert_eq!(
        json!(actual),
        captured,
        "ASSEMBLY source family fields and order"
    );
}
fn snapshot(v: &V) -> J {
    let V::O(v) = v else { panic!() };
    json!({"o":v.borrow().iter().filter(|(k,_)|!matches!(k.as_str(),"name"|"options"|"store"|"createXhrWithSecondaryKey"|"parent"|"xhr"|"unlimitedXHR"|"publicWebsocket"|"privateWebsocket"|"secondsTickersWebsocket"|"thirdTickersWebsocket")).map(|(k,v)|json!([k,wire(v)])).collect::<Vec<_>>()})
}
#[test]
fn api_interceptors_native_then_source() {
    let mut cases = vec![];
    let mut out = vec![];
    for scene in [
        "signed",
        "remote",
        "public",
        "kline",
        "null-window",
        "zero-window",
        "silent",
        "emit",
        "suppress",
        "tunnel",
    ] {
        let h = H::default();
        let options = o(vec![
            ("key", s(if scene == "public" { "" } else { "key" })),
            ("secret", s("secret")),
            ("passphrase", s("pass")),
            ("remoteSigner", V::B(scene == "remote")),
            ("tealstreetTunnel", V::B(scene == "tunnel")),
            (
                "extra",
                o(vec![(
                    "recvWindow",
                    if scene == "null-window" {
                        V::Null
                    } else if scene == "zero-window" {
                        V::N(0.)
                    } else {
                        V::U
                    },
                )]),
            ),
        ]);
        let emit = h.clone();
        let exchange = o(vec![
            ("name", s("bitget")),
            (
                "suppressBitgetValidationErrorEmission",
                V::B(scene == "suppress"),
            ),
            (
                "emitAndLogError",
                V::F(Rc::new(move |a| {
                    emit.state.borrow_mut().emissions.push(a[0].clone());
                    Ok(V::U)
                })),
            ),
        ]);
        let xhr = create_api(&h, options, exchange).unwrap();
        assert!(matches!(xhr, V::O(_)), "ASSEMBLY native client");
        let params = o(vec![("symbol", s("$BTC$USD")), ("limit", s("1"))]);
        let data = o(vec![("symbol", s("$BTC$USD")), ("_type", s("limit"))]);
        let input = o(vec![
            (
                "url",
                s(if scene == "kline" {
                    "/api/v2/mix/market/candles"
                } else {
                    "/orders"
                }),
            ),
            ("method", s("post")),
            ("params", params.clone()),
            ("data", data.clone()),
            ("headers", o(vec![("old", s("keep"))])),
        ]);
        // The Ref from `h.state.borrow()` lives to the end of its STATEMENT, so
        // inlining it here held a shared borrow across the whole interceptor chain -
        // and the createHmac `update` callback inside that chain borrow_mut()s the
        // same cell, so the test panicked with "RefCell already borrowed" the first
        // time it was ever able to run. Bind the clone, then invoke.
        let interceptor = h.state.borrow().requests[0].clone();
        let request = invoke(interceptor, vec![input.clone()]).unwrap();
        let signed = !matches!(scene, "remote" | "public");
        assert!(
            same(&get(&request, "data"), &data),
            "ASSEMBLY native original body alias"
        );
        assert!(
            same(&get(&request, "params"), &params),
            "ASSEMBLY native params alias"
        );
        assert!(
            same(&get(&data, "symbol"), &s("$BTC$USD")),
            "ASSEMBLY native body symbol unchanged"
        );
        assert!(
            same(
                &get(&params, "symbol"),
                &s(if signed && scene != "kline" {
                    "BTC$USD"
                } else {
                    "$BTC$USD"
                })
            ),
            "ASSEMBLY native query normalization"
        );
        assert_eq!(
            h.state.borrow().preimages.len(),
            usize::from(signed),
            "ASSEMBLY native independent signing count"
        );
        if signed {
            assert!(
                same(
                    &get(&get(&request, "headers"), "ACCESS-TIMESTAMP"),
                    &V::N(1234.)
                ),
                "ASSEMBLY native timestamp"
            );
            assert!(
                h.state.borrow().preimages[0]
                    .ends_with("{\"symbol\":\"BTC$USD\",\"type\":\"limit\"}"),
                "ASSEMBLY native signing body"
            )
        }
        let response = o(vec![
            (
                "data",
                o(vec![
                    ("code", s(if scene == "silent" { "40084" } else { "41" })),
                    ("msg", s("failure")),
                    ("data", V::U),
                ]),
            ),
            ("config", o(vec![("url", s("/orders"))])),
        ]);
        // Same temporary-Ref trap as the request interceptor above: the
        // response chain's emit callback borrow_mut()s this cell.
        let interceptor = h.state.borrow().responses[0].clone();
        let returned = invoke(interceptor, vec![response.clone()]).unwrap();
        assert!(same(&returned, &response), "ASSEMBLY native response alias");
        assert_eq!(
            h.state.borrow().emissions.len(),
            usize::from(!matches!(scene, "silent" | "suppress")),
            "ASSEMBLY native independent emission count"
        );
        assert!(
            same(
                &get(&h.state.borrow().configs[0], "timeout"),
                &V::N(if scene == "zero-window" { 0. } else { 20000. })
            ),
            "ASSEMBLY native recv window"
        );
        let interceptor = h.state.borrow().responses[0].clone();
        let repeated = invoke(interceptor, vec![response.clone()]).unwrap();
        assert!(
            same(&repeated, &response),
            "ASSEMBLY native repeated response alias"
        );
        let raw_body = s("{\"raw\":1}");
        let raw_input = o(vec![
            ("url", s("/orders")),
            ("method", s("post")),
            ("data", raw_body.clone()),
            ("headers", o(vec![])),
        ]);
        let interceptor = h.state.borrow().requests[0].clone();
        let raw = invoke(interceptor, vec![raw_input]).unwrap();
        assert!(
            same(&get(&raw, "data"), &raw_body),
            "ASSEMBLY native raw body alias"
        );
        let condition = h.state.borrow().conditions[0].clone();
        let conditions =
            ["read", "sig", "other"].map(|e| wire(&invoke(condition.clone(), vec![s(e)]).unwrap()));
        cases.push(json!({"scene":scene}));
        out.push(json!({"request":wire(&request),"preimages":h.state.borrow().preimages,"events":h.state.borrow().events,"emissions":h.state.borrow().emissions.iter().map(wire).collect::<Vec<_>>(),"conditions":conditions}));
    }
    assert_eq!(cases.len(), 10, "ASSEMBLY independent API floor");
    assert_eq!(
        json!(out),
        source(json!({"kind":"api","cases":cases})),
        "ASSEMBLY source API bodies"
    );
}
#[test]
fn construction_failure_stops_at_call_site() {
    for fault in [
        "base",
        "secondary-key",
        "axios",
        "limit",
        "socket-base",
        "secondary-1",
        "secondary-2",
    ] {
        let h = H::default();
        h.state.borrow_mut().fail = fault.into();
        let result = BitgetExchange::new(h.clone(), s("bitget"), o(vec![]), o(vec![]));
        assert!(
            matches!(result,Err(ref e) if e=="assembly failure"),
            "ASSEMBLY native constructor failure"
        );
        assert_eq!(
            h.state.borrow().events.last().unwrap(),
            fault,
            "ASSEMBLY native no later effect"
        );
    }
}
