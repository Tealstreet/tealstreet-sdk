use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    account_tier as tier, builder_approval as b, builder_approval_reads as a, deployment,
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{AuthTokenEncoding, HostThrownValue, TransactionSigningError as E},
    utils,
};
use serde_json::Value as J;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
#[derive(Clone, Debug)]
enum V {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Big(BigInt),
    Text(Vec<u16>),
    Obj(Rc<Obj>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Method(&'static str, Box<V>),
}
#[derive(Debug)]
struct Obj {
    fields: RefCell<Vec<(String, V)>>,
    kind: Option<&'static str>,
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::U, Self::U) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Num(a), Self::Num(b)) => a.to_bits() == b.to_bits(),
            (Self::Big(a), Self::Big(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Obj(a), Self::Obj(b)) => *a.fields.borrow() == *b.fields.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
fn obj(v: Vec<(&str, V)>) -> V {
    V::Obj(Rc::new(Obj {
        fields: RefCell::new(v.into_iter().map(|(k, v)| (k.into(), v)).collect()),
        kind: None,
    }))
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v.into_iter().map(Some).collect())))
}
fn text(v: &str) -> V {
    V::Text(v.encode_utf16().collect())
}
fn string(v: &V) -> String {
    match v {
        V::Text(v) => String::from_utf16(v).unwrap(),
        V::Big(v) => v.to_string(),
        V::Num(v) => safe_cex_num::js_number_string(*v),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        _ => panic!("fixture String conversion"),
    }
}
fn raw(v: &V, k: &str) -> V {
    match v {
        V::Obj(o) => o
            .fields
            .borrow()
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        V::Array(v) => k
            .parse::<usize>()
            .ok()
            .and_then(|i| v.borrow().get(i).cloned().flatten())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => a == b,
    }
}
fn set(v: &V, k: &str, x: V) {
    let V::Obj(o) = v else {
        panic!("object setter")
    };
    let mut fields = o.fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(n, _)| n == k) {
        *v = x
    } else {
        fields.push((k.into(), x))
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::Bool(v) => *v,
        V::Num(v) => *v != 0.0 && !v.is_nan(),
        V::Big(v) => *v != BigInt::from(0),
        V::Text(v) => !v.is_empty(),
        _ => true,
    }
}
fn decode(j: &J) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::U,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"].as_bool().unwrap()),
        "number" => V::Num(f64::from_bits(
            u64::from_str_radix(j["bits"].as_str().unwrap(), 16).unwrap(),
        )),
        "bigint" => V::Big(j["decimal"].as_str().unwrap().parse().unwrap()),
        "string" => V::Text(serde_json::from_value(j["units"].clone()).unwrap()),
        "object" => V::Obj(Rc::new(Obj {
            fields: RefCell::new(
                j["fields"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                    .collect(),
            ),
            kind: None,
        })),
        "array" => V::Array(Rc::new(RefCell::new(
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
        ))),
        k => panic!("decode {k}"),
    }
}
fn json(v: &V) -> J {
    match v {
        V::U | V::Null => J::Null,
        V::Bool(v) => (*v).into(),
        V::Num(v) => serde_json::Number::from_f64(*v)
            .map(J::Number)
            .unwrap_or(J::Null),
        V::Text(v) => String::from_utf16(v).unwrap().into(),
        V::Big(v) => v.to_string().into(),
        V::Array(v) => J::Array(
            v.borrow()
                .iter()
                .map(|x| x.as_ref().map(json).unwrap_or(J::Null))
                .collect(),
        ),
        V::Obj(v) => J::Object(
            v.fields
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::U))
                .map(|(k, v)| (k.clone(), json(v)))
                .collect(),
        ),
        _ => panic!("fixture imported JSON helper"),
    }
}
fn error(name: &'static str, message: &str) -> E {
    TransactionBuildError {
        name,
        message: message.encode_utf16().collect(),
    }
    .into()
}
#[derive(Debug)]
struct Injected {
    message: String,
    is_error: bool,
}
fn message(e: &E) -> V {
    match e {
        E::Generated(e) => V::Text(e.message.clone()),
        E::Thrown(e) => text(&e.value().downcast_ref::<Injected>().unwrap().message),
    }
}
#[derive(Default)]
struct Engine {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Engine {
    fn enqueue_microtask(&self, f: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(f)
    }
    fn now_millis(&self) -> f64 {
        panic!("clock supplied at source site")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("no actual signing in read fixture")
    }
}
struct Signal(AtomicBool);
impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.0.store(true, Ordering::Relaxed)
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.store(true, Ordering::Relaxed)
    }
}
type F = Pin<Box<dyn Future<Output = Result<V, E>>>>;
struct Task {
    future: F,
    resolver: Option<PromiseResolver<V>>,
    signal: Arc<Signal>,
}
impl Task {
    fn poll(&mut self) -> bool {
        self.signal.0.store(false, Ordering::Relaxed);
        let w = Waker::from(self.signal.clone());
        if let Poll::Ready(v) = self.future.as_mut().poll(&mut Context::from_waker(&w)) {
            self.resolver.take().unwrap().settle(v);
            true
        } else {
            false
        }
    }
}
struct State {
    input: J,
    params: V,
    payload: V,
    crypto: V,
    engine: Rc<Engine>,
    tasks: VecDeque<Task>,
    events: Vec<String>,
    warnings: Vec<E>,
    init_receipts: usize,
    constructs: Vec<V>,
    clients: Vec<V>,
    calls: Vec<V>,
    sequences: BTreeMap<String, Vec<V>>,
    indices: BTreeMap<String, usize>,
    injected: HostThrownValue,
}
type H = RefCell<State>;
fn check(h: &H, n: &str) -> Result<(), E> {
    if n == "init" {
        h.borrow_mut().init_receipts += 1;
    }
    h.borrow_mut().events.push(n.into());
    if h.borrow().input["fail"] == n && h.borrow().input["reject"] != true {
        Err(h.borrow().injected.clone().into())
    } else {
        Ok(())
    }
}
fn get(h: &H, v: &V, k: &str) -> Result<V, E> {
    if matches!(v, V::U | V::Null) {
        return Err(error(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{k}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ),
        ));
    }
    if same(v, &h.borrow().params) && h.borrow().sequences.contains_key(k) {
        let mut h = h.borrow_mut();
        h.events.push(format!("param:{k}"));
        let i = *h.indices.entry(k.into()).or_default();
        h.indices.insert(k.into(), i + 1);
        let values = &h.sequences[k];
        return Ok(values[i.min(values.len() - 1)].clone());
    }
    Ok(raw(v, k))
}
fn begin(h: &H, future: F) -> NativePromise<V> {
    let (p, resolver) = NativePromise::pending(h.borrow().engine.clone());
    let mut task = Task {
        future,
        resolver: Some(resolver),
        signal: Arc::new(Signal(AtomicBool::new(true))),
    };
    if !task.poll() {
        h.borrow_mut().tasks.push_back(task)
    }
    p
}
fn drain(h: &H) {
    for _ in 0..100 {
        let task = h.borrow_mut().tasks.pop_front();
        if let Some(mut task) = task {
            assert!(
                task.signal.0.load(Ordering::Relaxed),
                "fixture task wake required"
            );
            if !task.poll() {
                h.borrow_mut().tasks.push_back(task)
            }
            continue;
        }
        let engine = h.borrow().engine.clone();
        let next = engine.queue.borrow_mut().pop_front();
        if let Some(next) = next {
            next()
        } else {
            return;
        }
    }
    panic!("independent task ceiling")
}
fn primitive(v: V) -> safe_cex_num::JsPrimitive {
    use safe_cex_num::JsPrimitive as P;
    match v {
        V::U => P::Undefined,
        V::Null => P::Null,
        V::Bool(v) => P::Boolean(v),
        V::Num(v) => P::Number(v),
        V::Big(v) => P::BigInt(v),
        V::Text(v) => P::String(String::from_utf16(&v).unwrap()),
        _ => panic!("primitive fixture"),
    }
}
fn bigint(v: V) -> Result<V, E> {
    safe_cex_num::js_bigint_primitive(&primitive(v))
        .map(V::Big)
        .map_err(|e| {
            TransactionBuildError {
                name: e.kind.name(),
                message: e.message_utf16,
            }
            .into()
        })
}
fn name(v: &V) -> Option<safe_cex_types::ExchangeName> {
    serde_json::from_value::<safe_cex_types::ExchangeName>(json(v)).ok()
}
fn runtime() -> Rc<a::LighterApprovalReadRuntime<H, V, E>> {
    Rc::new(a::LighterApprovalReadRuntime {
        run_async: begin,
        get,
        method: |_, v, n| Ok(V::Method(n, Box::new(v.clone()))),
        call: |_, f, _| {
            let V::Method(n, v) = f else { panic!("method") };
            Ok(match n {
                "trim" => text(safe_cex_num::js_trim(&string(&v))),
                "toString" => text(&string(&v)),
                _ => panic!("sync method"),
            })
        },
        call_async: |h, f, args| {
            let V::Method(n, receiver) = f else {
                panic!("method")
            };
            let V::Obj(o) = receiver.as_ref() else {
                panic!("client receiver")
            };
            assert_eq!(
                o.kind,
                Some(if n == "getAuthToken" {
                    "TransactionClient"
                } else {
                    "LighterApiClient"
                })
            );
            check(h, n)?;
            let v = {
                h.borrow_mut()
                    .calls
                    .push(obj(vec![("name", text(n)), ("args", array(args))]));
                if h.borrow().input["reject"] == true && h.borrow().input["fail"] == n {
                    Err(h.borrow().injected.clone().into())
                } else {
                    Ok(match n {
                        "getAuthToken" => text("auth-token"),
                        "get" => h.borrow().payload.clone(),
                        "getNextNonce" => V::Big("900719925474099312345".parse().unwrap()),
                        _ => panic!("async method"),
                    })
                }
            };
            Ok(Box::pin(std::future::ready(v)))
        },
        construct: |h, n, opts| {
            check(h, n)?;
            h.borrow_mut()
                .constructs
                .push(obj(vec![("name", text(n)), ("options", opts.clone())]));
            if n == "TransactionClient" {
                assert!(same(&raw(&opts, "crypto"), &h.borrow().crypto));
                if h.borrow().input["method"] != "createApprovalTransactionClient" {
                    assert!(
                        same(&raw(&opts, "httpClient"), &h.borrow().clients[0]),
                        "native HTTP client identity"
                    )
                }
            }
            let value = V::Obj(Rc::new(Obj {
                fields: RefCell::new(if n == "LighterApiClient" {
                    vec![("options".into(), opts)]
                } else {
                    vec![]
                }),
                kind: Some(n),
            }));
            h.borrow_mut().clients.push(value.clone());
            Ok(value)
        },
        object: |v| obj(v),
        array,
        undefined: || V::U,
        number: V::Num,
        text,
        truthy,
        is_number: |v| matches!(v, V::Num(_)),
        finite: |v| matches!(v,V::Num(n) if n.is_finite()),
        is_object: |v| matches!(v, V::Obj(_) | V::Array(_) | V::Null),
        is_string: |v| matches!(v, V::Text(_)),
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        nullish: |v| matches!(v, V::U | V::Null),
        is_undefined: |v| matches!(v, V::U),
        same: |a, b| a == b,
        copy: |_, v| {
            let V::Obj(o) = v else { panic!("base object") };
            Ok(V::Obj(Rc::new(Obj {
                fields: RefCell::new(o.fields.borrow().clone()),
                kind: None,
            })))
        },
        spread: |_, target, v| {
            if let V::Obj(o) = v {
                for (k, v) in o.fields.borrow().iter() {
                    set(target, k, v.clone())
                }
            }
            Ok(())
        },
        extend_array: |_, target, v| {
            let (V::Array(a), V::Array(b)) = (target, v) else {
                panic!("array spread")
            };
            for x in b.borrow().iter() {
                a.borrow_mut().push(Some(x.clone().unwrap_or(V::U)))
            }
            Ok(())
        },
        map: |_, v, f| {
            let V::Array(v) = v else { panic!("map") };
            let mut out = vec![];
            for item in v.borrow().iter() {
                out.push(match item {
                    Some(x) => Some(f(x.clone())?),
                    None => None,
                })
            }
            Ok(V::Array(Rc::new(RefCell::new(out))))
        },
        filter: |_, v, f| {
            let V::Array(v) = v else { panic!("filter") };
            let mut out = vec![];
            for item in v.borrow().iter().flatten() {
                if f(item.clone())? {
                    out.push(item.clone())
                }
            }
            Ok(array(out))
        },
        default_caps: |_| {
            let c = b::DEFAULT_LIGHTER_APPROVAL_FEE_CAPS;
            Ok(obj(vec![
                ("maxPerpsMakerFee", V::Num(c.max_perps_maker_fee)),
                ("maxPerpsTakerFee", V::Num(c.max_perps_taker_fee)),
                ("maxSpotMakerFee", V::Num(c.max_spot_maker_fee)),
                ("maxSpotTakerFee", V::Num(c.max_spot_taker_fee)),
            ]))
        },
        base_url: |_, p| {
            let ex = name(&raw(&p, "exchangeName"));
            let base = raw(&p, "overrideBaseUrl");
            let base = if matches!(base, V::U) {
                None
            } else {
                Some(string(&base))
            };
            Ok(text(&b::get_base_url(
                ex.as_ref(),
                base.as_deref(),
                truthy(&raw(&p, "testnet")),
                &Default::default(),
            )))
        },
        deployment_profile: |_, p| {
            let ex = name(&raw(&p, "exchangeName"));
            let p = deployment::resolve_lighter_deployment_profile(
                ex.as_ref(),
                Some(truthy(&raw(&p, "testnet"))),
            );
            Ok(obj(vec![(
                "signerChainId",
                V::Num(p.signer_chain_id as f64),
            )]))
        },
        wasm_ready: |h| {
            check(h, "ready")?;
            Ok(h.borrow().input["ready"] != false)
        },
        init_wasm: |h| {
            let v = check(h, "init").map(|_| V::U);
            Ok(Box::pin(std::future::ready(v)))
        },
        crypto_api: |h| {
            check(h, "crypto")?;
            Ok(h.borrow().crypto.clone())
        },
        now: |h| {
            check(h, "now")?;
            let V::Num(n) = decode(&h.borrow().input["now"]) else {
                panic!("clock")
            };
            Ok(n)
        },
        bigint: |_, v| bigint(v),
        negative_one: || V::Big((-1).into()),
        to_bigint: |_, v| {
            if matches!(v, V::U | V::Null) {
                return Ok(V::U);
            }
            Ok(bigint(text(&string(&v))).unwrap_or(V::U))
        },
        to_number: |_, v| {
            let n = safe_cex_num::js_number_primitive(&primitive(v))
                .map_err(|e| error(e.name, &e.message))?;
            Ok(if n.is_finite() { V::Num(n) } else { V::U })
        },
        positive_integer: |_, v| {
            Ok(utils::parse_lighter_positive_integer_string(Some(&json(v)))
                .unwrap()
                .map(|v| text(&v))
                .unwrap_or(V::U))
        },
        l1_address: |_, v| Ok(utils::is_lighter_l1_address(Some(&json(v))).unwrap()),
        extract_tier: |_, v| {
            Ok(tier::extract_lighter_account_tier_from_payload(Some(
                &safe_cex_exchange::js_runtime::value::from_json(&json(&v)),
            ))
            .unwrap()
            .map(|v| {
                text(match v {
                    tier::LighterAccountTier::Standard => "standard",
                    tier::LighterAccountTier::Premium => "premium",
                    tier::LighterAccountTier::Plus => "plus",
                })
            })
            .unwrap_or(V::U))
        },
        eligible_tier: |_, v| {
            Ok(
                tier::is_lighter_account_tier_eligible_for_partner_attribution(
                    serde_json::from_value(json(&v)).ok(),
                ),
            )
        },
        nonzero_fees: |_, v| {
            let n = |k| {
                if let V::Num(n) = raw(&v, k) {
                    Some(n)
                } else {
                    None
                }
            };
            Ok(tier::has_non_zero_lighter_integrator_fee(
                n("maxPerpsMakerFee"),
                n("maxPerpsTakerFee"),
            ) || tier::has_non_zero_lighter_integrator_fee(
                n("maxSpotMakerFee"),
                n("maxSpotTakerFee"),
            ))
        },
        nonnegative_bigint: |_, v| {
            Ok(if let V::Big(n) = v {
                n >= 0.into()
            } else {
                panic!("normalized index")
            })
        },
        new_error: |_, s| error("Error", s),
        is_error: |_, e| {
            Ok(match e {
                E::Generated(_) => true,
                E::Thrown(e) => e.value().downcast_ref::<Injected>().unwrap().is_error,
            })
        },
        error_value: |e| obj(vec![("message", message(e))]),
        warn_tier_lookup_failure: |h, e| {
            h.borrow_mut().warnings.push(e.clone());
        },
    })
}
fn source() -> &'static J {
    static C: std::sync::OnceLock<J> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let o = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/builder-approval-reads-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    })
}
fn host(row: &J) -> Rc<H> {
    Rc::new(RefCell::new(State {
        input: row.clone(),
        params: decode(&row["params"]),
        payload: decode(&row["payload"]),
        crypto: obj(vec![("imported", text("crypto"))]),
        engine: Default::default(),
        tasks: Default::default(),
        events: vec![],
        warnings: vec![],
        init_receipts: 0,
        constructs: vec![],
        clients: vec![],
        calls: vec![],
        sequences: row["sequences"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_array().unwrap().iter().map(decode).collect(),
                )
            })
            .collect(),
        indices: Default::default(),
        injected: HostThrownValue::new(Injected {
            message: row["errorMessage"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("injected:{}", row["fail"].as_str().unwrap_or(""))),
            is_error: row["nonerror"] != true,
        }),
    }))
}
fn run(h: &Rc<H>, rt: &Rc<a::LighterApprovalReadRuntime<H, V, E>>) -> Result<V, E> {
    let params = h.borrow().params.clone();
    let name = h.borrow().input["method"].as_str().unwrap().to_owned();
    let sync = match name.as_str() {
        "resolveApprovalFeeCaps" => Some(a::resolve_approval_fee_caps(
            h.as_ref(),
            rt.as_ref(),
            &params,
        )),
        "createApprovalClient" => Some(a::create_approval_client(h.as_ref(), rt.as_ref(), &params)),
        "createApprovalTransactionClient" => Some(a::create_approval_transaction_client(
            h.as_ref(),
            rt.as_ref(),
            &params,
        )),
        _ => None,
    };
    if let Some(result) = sync {
        if result.is_ok() {
            h.borrow_mut().events.push("after-call".into())
        }
        return result;
    }
    let p = match name.as_str() {
        "ensureWasmReady" => a::ensure_wasm_ready(h.clone(), rt.clone()),
        "fetchAccountLimitsForApproval" => {
            a::fetch_account_limits_for_approval(h.clone(), rt.clone(), params)
        }
        "assertPartnerAttributionTierEligibleForApproval" => {
            a::assert_partner_attribution_tier_eligible_for_approval(h.clone(), rt.clone(), params)
        }
        "resolveAccountIndex" => a::resolve_account_index(h.clone(), rt.clone(), params),
        "fetchAccountSnapshot" => a::fetch_account_snapshot(h.clone(), rt.clone(), params),
        "getNextNonce" => a::get_next_nonce(h.clone(), rt.clone(), params),
        _ => panic!("method"),
    };
    h.borrow_mut().events.push("after-call".into());
    let result = Rc::new(RefCell::new(None));
    let captured = result.clone();
    p.subscribe(move |v| *captured.borrow_mut() = Some(v.map(|v| (*v).clone())));
    drain(h);
    let value = result.borrow_mut().take().unwrap();
    value
}
fn check_family(methods: &[&str], floor: usize) {
    let rows = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| methods.contains(&r["method"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(
        rows.len(),
        floor,
        "approval_reads::independent family floor"
    );
    let rt = runtime();
    let mut results = vec![];
    for row in rows {
        let h = host(row);
        let result = run(&h, &rt);
        let expected = &row["result"];
        assert_eq!(
            result.is_err(),
            expected["kind"] == "error",
            "approval_reads::native outcome {}",
            row["id"]
        );
        assert_eq!(
            h.borrow().init_receipts,
            row["initReceipts"].as_u64().unwrap() as usize,
            "approval_reads::native init receipts {}",
            row["id"]
        );
        if expected["kind"] == "error" {
            let e = result.unwrap_err();
            let name = match &e {
                E::Generated(e) => e.name,
                E::Thrown(e) => {
                    if e.value().downcast_ref::<Injected>().unwrap().is_error {
                        "Error"
                    } else {
                        "thrown"
                    }
                }
            };
            assert_eq!(
                name,
                expected["name"].as_str().unwrap(),
                "approval_reads::native error name {}",
                row["id"]
            );
            assert_eq!(
                message(&e),
                decode(&expected["message"]),
                "approval_reads::native error {}",
                row["id"]
            );
            if expected["injected"] == true {
                assert!(
                    matches!(&e,E::Thrown(v) if v.ptr_eq(&h.borrow().injected)),
                    "native original thrown identity"
                )
            }
        } else {
            assert_eq!(
                result.unwrap(),
                decode(expected),
                "approval_reads::native result {}",
                row["id"]
            )
        }
        assert_eq!(
            array(h.borrow().constructs.clone()),
            decode(&row["constructs"]),
            "approval_reads::native constructor {}",
            row["id"]
        );
        assert_eq!(
            array(h.borrow().calls.clone()),
            decode(&row["calls"]),
            "approval_reads::native requests {}",
            row["id"]
        );
        let expected_warnings = usize::from(matches!(
            row["id"].as_str(),
            Some("tier-best-effort" | "tier-nonerror" | "tier-rejection")
        ));
        assert_eq!(
            h.borrow().warnings.len(),
            expected_warnings,
            "approval_reads::native tier fallback warning {}",
            row["id"]
        );
        for warning in &h.borrow().warnings {
            assert!(
                matches!(warning, E::Thrown(v) if v.ptr_eq(&h.borrow().injected)),
                "original warning error identity"
            );
        }
        results.push((h, row));
    }
    for (h, row) in results {
        let expected: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
        assert_eq!(
            h.borrow().events,
            expected,
            "approval_reads::event trace {}",
            row["id"]
        )
    }
}
#[test]
fn approval_reads_caps() {
    check_family(&["resolveApprovalFeeCaps"], 10)
}
#[test]
fn approval_reads_clients() {
    check_family(
        &[
            "createApprovalClient",
            "createApprovalTransactionClient",
            "ensureWasmReady",
        ],
        11,
    )
}
#[test]
fn approval_reads_limits_tier() {
    check_family(
        &[
            "fetchAccountLimitsForApproval",
            "assertPartnerAttributionTierEligibleForApproval",
        ],
        19,
    )
}
#[test]
fn approval_reads_accounts() {
    check_family(
        &[
            "resolveAccountIndex",
            "fetchAccountSnapshot",
            "getNextNonce",
        ],
        25,
    )
}
#[test]
fn approval_reads_detached() {
    let row = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "limits-ready")
        .unwrap();
    let h = host(row);
    let rt = runtime();
    let params = h.borrow().params.clone();
    let p = a::fetch_account_limits_for_approval(h.clone(), rt, params);
    drop(p);
    assert_eq!(h.borrow().constructs.len(), 0);
    drain(&h);
    assert_eq!(
        h.borrow().constructs.len(),
        2,
        "approval_reads::native detached construction"
    );
    assert_eq!(
        h.borrow().calls.len(),
        2,
        "approval_reads::native detached requests"
    );
}
