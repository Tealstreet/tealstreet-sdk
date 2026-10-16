use num_bigint::BigInt;
use safe_cex_venue_lighter::{
    transaction_builders::TransactionBuildError,
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{AuthTokenEncoding, HostThrownValue, TransactionSigningError as E},
    ws_private_feed_helpers as f,
};
use serde_json::Value as J;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    process::Command,
    rc::Rc,
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
        V::Array(v) if k == "length" => V::Num(v.borrow().len() as f64),
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
fn set(value: &V, key: &str, x: V) {
    let V::Obj(value) = value else {
        panic!("object")
    };
    let mut fields = value.fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == key) {
        *v = x
    } else {
        fields.push((key.into(), x))
    }
}
#[derive(Default)]
struct Engine {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Engine {
    fn enqueue_microtask(&self, callback: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(callback)
    }
    fn now_millis(&self) -> f64 {
        panic!("clock capability")
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        panic!("no signing")
    }
}
fn drain(e: &Engine) {
    for _ in 0..100 {
        let callback = e.queue.borrow_mut().pop_front();
        match callback {
            Some(callback) => callback(),
            None => return,
        }
    }
    panic!("feed::microtask floor")
}
#[derive(Debug)]
struct Injected {
    stage: String,
}
struct Timer {
    callback: Option<Box<dyn FnOnce() -> NativePromise<V>>>,
    delay: V,
    handle: V,
}
struct State {
    row: J,
    engine: Rc<Engine>,
    owner: V,
    parent: V,
    alternate: V,
    store: V,
    raw: V,
    opts: V,
    positions: V,
    fetched: V,
    events: Vec<String>,
    calls: Vec<V>,
    updates: Vec<V>,
    console_errors: Vec<V>,
    timers: Vec<Timer>,
    cancelled: Vec<V>,
    fetch_release: Option<PromiseResolver<V>>,
    skip_count: usize,
    task_count: usize,
    errors: BTreeMap<String, HostThrownValue>,
}
type H = Rc<RefCell<State>>;
type R = f::LighterPrivateFeedRuntime<H, V>;
fn injected(h: &H, stage: &str) -> E {
    let e = HostThrownValue::new(Injected {
        stage: stage.into(),
    });
    h.borrow_mut().errors.insert(stage.into(), e.clone());
    e.into()
}
fn message(e: &E) -> V {
    match e {
        E::Generated(e) => V::Text(e.message.clone()),
        E::Thrown(e) => text(&format!(
            "injected:{}",
            e.value().downcast_ref::<Injected>().unwrap().stage
        )),
    }
}
fn receipt(h: &H, name: &str, args: Vec<V>) {
    let mut state = h.borrow_mut();
    state.events.push(name.into());
    state
        .calls
        .push(obj(vec![("name", text(name)), ("args", array(args))]));
}
fn stringify(value: &V) -> Result<Option<Vec<u16>>, E> {
    let ascii = |s: &str| Some(s.encode_utf16().collect::<Vec<_>>());
    match value {
        V::U => Ok(None),
        V::Null => Ok(ascii("null")),
        V::Bool(v) => Ok(ascii(if *v { "true" } else { "false" })),
        V::Num(n) => Ok(ascii(&if n.is_finite() {
            safe_cex_num::js_number_string(*n)
        } else {
            "null".into()
        })),
        V::Big(_) => Err(error("TypeError", "Do not know how to serialize a BigInt")),
        V::Text(units) => {
            let mut out = vec![34];
            for c in char::decode_utf16(units.iter().copied()) {
                match c {
                    Err(e) => {
                        out.extend(format!("\\u{:04x}", e.unpaired_surrogate()).encode_utf16())
                    }
                    Ok(c) => {
                        let quoted = serde_json::to_string(&c.to_string()).unwrap();
                        out.extend(quoted[1..quoted.len() - 1].encode_utf16());
                    }
                }
            }
            out.push(34);
            Ok(Some(out))
        }
        V::Array(values) => {
            let values = values.borrow().clone();
            let mut out = vec![91];
            for (i, v) in values.iter().enumerate() {
                if i > 0 {
                    out.push(44)
                }
                out.extend(match v {
                    Some(v) => stringify(v)?.unwrap_or_else(|| "null".encode_utf16().collect()),
                    None => "null".encode_utf16().collect(),
                });
            }
            out.push(93);
            Ok(Some(out))
        }
        V::Obj(value) => {
            let fields = value.fields.borrow().clone();
            let keys = safe_cex_num::js_own_keys(fields.iter().map(|(k, _)| k.as_str()));
            let mut out = vec![123];
            let mut added = false;
            for key in keys {
                let v = &fields.iter().find(|(k, _)| *k == key).unwrap().1;
                if let Some(v) = stringify(v)? {
                    if added {
                        out.push(44)
                    }
                    added = true;
                    out.extend(stringify(&text(&key))?.unwrap());
                    out.push(58);
                    out.extend(v)
                }
            }
            out.push(125);
            Ok(Some(out))
        }
        _ => panic!("data stringify"),
    }
}
fn to_string(value: V) -> Vec<u16> {
    match value {
        V::Text(v) => v,
        V::Obj(_) | V::Array(_) => safe_cex_num::js_string(Some(&json(&value)))
            .unwrap()
            .encode_utf16()
            .collect(),
        _ => string(&value).encode_utf16().collect(),
    }
}
fn upper_trim(units: Vec<u16>) -> Vec<u16> {
    let whitespace = |c: &u16| {
        char::from_u32(*c as u32).is_some_and(|c| safe_cex_num::js_trim(&c.to_string()).is_empty())
    };
    let start = units
        .iter()
        .position(|c| !whitespace(c))
        .unwrap_or(units.len());
    let end = units
        .iter()
        .rposition(|c| !whitespace(c))
        .map(|i| i + 1)
        .unwrap_or(start);
    let mut out = vec![];
    for c in char::decode_utf16(units[start..end].iter().copied()) {
        match c {
            Err(e) => out.push(e.unpaired_surrogate()),
            Ok(c) => {
                for c in c.to_uppercase() {
                    out.extend(c.encode_utf16(&mut [0; 2]).iter().copied())
                }
            }
        }
    }
    out
}
fn call(h: &H, method: V, args: Vec<V>) -> Result<V, E> {
    let V::Method(name, receiver) = method else {
        panic!("method")
    };
    if name == "shouldSkipOrderReconciliationFetch" {
        assert!(
            same(&receiver, &h.borrow().parent),
            "feed::native skip receiver"
        );
        receipt(h, "skip", args);
        let mut state = h.borrow_mut();
        state.skip_count += 1;
        return Ok(V::Bool(
            state.row["mode"] == "skip-before"
                || state.row["mode"] == "skip-after" && state.skip_count > 1,
        ));
    }
    let operation = match name {
        "sendWithLog" => "send",
        other => other,
    };
    if name == "log" {
        assert!(
            same(&receiver, &h.borrow().parent),
            "feed::native log receiver"
        )
    }
    if name == "sendWithLog" {
        assert!(
            same(&receiver, &h.borrow().owner),
            "feed::native send receiver"
        )
    }
    if matches!(name, "addOrUpdateOrdersFull" | "addOrUpdatePositionsFull") {
        assert!(
            same(&receiver, &h.borrow().store),
            "feed::native store receiver"
        );
        receipt(h, operation, args.clone());
        if h.borrow().row["mode"] == "store-throw" {
            return Err(injected(h, "store"));
        }
        let identity = same(&args[0], &h.borrow().fetched);
        h.borrow_mut().updates.push(obj(vec![
            ("name", text(name)),
            ("value", args[0].clone()),
            ("identity", V::Bool(identity)),
        ]));
    } else {
        receipt(h, operation, args)
    }
    Ok(V::U)
}
fn runtime(h: H) -> Rc<R> {
    Rc::new(R {
        promise_runtime: h.borrow().engine.clone(),
        host: h.clone(),
        get: |_, value, key| {
            if matches!(value, V::U | V::Null) {
                return Err(error("TypeError", "Cannot read properties of undefined"));
            }
            Ok(raw(value, &String::from_utf16(key).unwrap()))
        },
        define: |_, value, key, v| {
            set(value, &String::from_utf16(key).unwrap(), v);
            Ok(())
        },
        method: |h, receiver, name| {
            if name == "scheduleThrottledTask" && h.borrow().row["mode"] == "getter-throw" {
                receipt(h, "scheduler-get", vec![]);
                return Err(injected(h, "scheduler"));
            }
            let name = if name == "scheduleThrottledTask" && h.borrow().row["mode"] == "override" {
                "scheduler-override"
            } else {
                name
            };
            Ok(V::Method(name, Box::new(receiver)))
        },
        call,
        call_async: |h, method, args| {
            let V::Method(name, receiver) = method else {
                panic!("method")
            };
            assert!(
                same(&receiver, &h.borrow().parent),
                "feed::native fetch receiver"
            );
            receipt(h, name, args);
            if h.borrow().row["mode"] == "call-throw" {
                return Err(injected(h, "fetch"));
            }
            let engine = h.borrow().engine.clone();
            let (promise, out) = NativePromise::pending(engine);
            h.borrow_mut().fetch_release = Some(out);
            Ok(promise)
        },
        clear_timeout: |h, handle| {
            assert!(
                h.borrow().timers.iter().any(|t| same(&t.handle, &handle)),
                "feed::native cancelled handle identity"
            );
            receipt(h, "clearTimeout", vec![handle.clone()]);
            h.borrow_mut().cancelled.push(handle);
            Ok(V::U)
        },
        set_timeout: |h, callback, delay| {
            receipt(h, "setTimeout", vec![delay.clone()]);
            let handle = obj(vec![(
                "timer",
                V::Num(h.borrow().timers.len() as f64 + 1.0),
            )]);
            h.borrow_mut().timers.push(Timer {
                callback: Some(callback),
                delay,
                handle: handle.clone(),
            });
            Ok(handle)
        },
        schedule_call: |r, callee, key, delay, task, prefix| {
            let V::Method(name, receiver) = callee else {
                panic!("scheduler")
            };
            assert!(
                same(&receiver, &r.host.borrow().owner),
                "feed::native scheduler receiver"
            );
            if name == "scheduler-override" {
                receipt(&r.host, name, vec![text(key), delay, prefix]);
                Ok(V::Num(99.0))
            } else {
                assert_eq!(name, "scheduleThrottledTask");
                f::schedule_throttled_task(r, &receiver, key, delay, task, prefix)
            }
        },
        console_error: |h, prefix, e| {
            receipt(h, "console", vec![]);
            let original = match e {
                E::Thrown(e) => {
                    let stage = &e.value().downcast_ref::<Injected>().unwrap().stage;
                    assert!(
                        e.ptr_eq(&h.borrow().errors[stage]),
                        "feed::native logged error identity"
                    );
                    text(stage)
                }
                _ => V::Null,
            };
            h.borrow_mut().console_errors.push(obj(vec![
                ("prefix", prefix),
                ("original", original),
                ("message", message(e)),
            ]));
            Ok(V::U)
        },
        stringify: |_, value| Ok(stringify(&value)?.map(V::Text).unwrap_or(V::U)),
        to_string: |_, value| Ok(to_string(value)),
        trim_upper: |_, units| Ok(upper_trim(units)),
        to_number: |_, value| {
            use safe_cex_num::JsPrimitive;
            let p = match value {
                V::U => JsPrimitive::Undefined,
                V::Null => JsPrimitive::Null,
                V::Bool(v) => JsPrimitive::Boolean(v),
                V::Num(v) => JsPrimitive::Number(v),
                V::Big(v) => JsPrimitive::BigInt(v),
                V::Text(v) => JsPrimitive::String(String::from_utf16_lossy(&v)),
                _ => panic!("sign input"),
            };
            safe_cex_num::js_number_primitive(&p).map_err(|e| error("TypeError", &e.message))
        },
        parse_float: |_, value| Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(&value))),
        copy: |_, value| {
            let V::Obj(value) = value else {
                panic!("position")
            };
            let fields = value.fields.borrow().clone();
            Ok(V::Obj(Rc::new(Obj {
                fields: RefCell::new(fields),
            })))
        },
        filter: |_, method, callback| {
            let V::Method(name, receiver) = method else {
                panic!("filter")
            };
            assert_eq!(name, "filter");
            let V::Array(values) = *receiver else {
                panic!("positions")
            };
            let values = values.borrow().clone();
            let mut out = vec![];
            for value in values.into_iter().flatten() {
                if callback(value.clone())? {
                    out.push(value)
                }
            }
            Ok(array(out))
        },
        map: |_, method, callback| {
            let V::Method(name, receiver) = method else {
                panic!("map")
            };
            assert_eq!(name, "map");
            let V::Array(values) = *receiver else {
                panic!("targets")
            };
            let values = values.borrow().clone();
            let mut out = vec![];
            for value in values.into_iter().flatten() {
                out.push(callback(value)?)
            }
            Ok(array(out))
        },
        now: |h| {
            receipt(h, "now", vec![]);
            if h.borrow().row["mode"] == "clock-parent-swap" {
                let owner = h.borrow().owner.clone();
                let alternate = h.borrow().alternate.clone();
                set(&owner, "parent", alternate)
            }
            Ok(12345.25)
        },
        number: V::Num,
        text,
        utf16: V::Text,
        boolean: V::Bool,
        undefined: || V::U,
        object: obj,
        array,
        truthy,
        nullish: |v| matches!(v, V::U | V::Null),
        is_undefined: |v| matches!(v, V::U),
        strict_same: |a, b| match (a, b) {
            (V::Num(a), V::Num(b)) => a == b,
            _ => same(a, b),
        },
    })
}
fn source() -> &'static J {
    static DATA: std::sync::OnceLock<J> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/private-feed-helpers-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "feed::oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: J = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(data["count"], 75, "feed::independent total floor");
        data
    })
}
fn host(row: &J) -> H {
    let positions = decode(&row["positions"]);
    let store = obj(vec![
        ("positions", positions.clone()),
        (
            "marketsById",
            obj(vec![
                ("7", obj(vec![("symbol", text("BTC"))])),
                ("null", obj(vec![("symbol", text("BTC"))])),
            ]),
        ),
    ]);
    let parent = obj(vec![
        ("store", store.clone()),
        ("isDisposed", V::Bool(false)),
        ("lastWebsocketPositionsUpdate", V::Num(17.0)),
    ]);
    let alternate = obj(vec![
        ("store", store.clone()),
        ("isDisposed", V::Bool(false)),
        ("lastWebsocketPositionsUpdate", V::Num(29.0)),
    ]);
    let owner = obj(vec![
        ("parent", parent.clone()),
        ("store", store.clone()),
        ("isDisposed", V::Bool(false)),
        ("ordersThrottleTimeout", V::U),
        ("positionsThrottleTimeout", V::U),
        ("balanceThrottleTimeout", V::U),
    ]);
    Rc::new(RefCell::new(State {
        row: row.clone(),
        engine: Default::default(),
        owner,
        parent,
        alternate,
        store,
        positions,
        raw: decode(&row["value"]),
        opts: decode(&row["opts"]),
        fetched: array(vec![obj(vec![
            ("id", text("fetched")),
            ("price", V::Num(7.0)),
        ])]),
        events: vec![],
        calls: vec![],
        updates: vec![],
        console_errors: vec![],
        timers: vec![],
        cancelled: vec![],
        fetch_release: None,
        skip_count: 0,
        task_count: 0,
        errors: Default::default(),
    }))
}
fn invoke(r: &Rc<R>, h: &H) -> Result<V, E> {
    let (state, owner, raw, opts) = {
        let s = h.borrow();
        (
            s.row.clone(),
            s.owner.clone(),
            s.raw.clone(),
            s.opts.clone(),
        )
    };
    let delay = if state["mode"] == "zero" {
        V::Num(0.0)
    } else {
        V::U
    };
    match state["method"].as_str().unwrap() {
        "normalizeOrderStatus" => f::normalize_order_status(r, &raw),
        "isUserCancelledReason" => f::is_user_cancelled_reason(r, raw).map(V::Bool),
        "resolveExplicitlyClosedPositionUpdates" => {
            f::resolve_explicitly_closed_position_updates(r, &owner, &raw)
        }
        "sendSubscribePayload" => {
            f::send_subscribe_payload(r, &owner, raw, state["auth"].as_bool().unwrap())
        }
        "throttleFetchOrders" => f::throttle_fetch_orders(r, &owner, delay, opts),
        "throttleFetchPositions" => f::throttle_fetch_positions(r, &owner, delay),
        "throttleFetchBalance" => f::throttle_fetch_balance(r, &owner, delay),
        "scheduleThrottledTask" => {
            let h = h.clone();
            f::schedule_throttled_task(
                r,
                &owner,
                "ordersThrottleTimeout",
                V::Num(12.5),
                Box::new(move || {
                    h.borrow_mut().task_count += 1;
                    receipt(&h, "task", vec![]);
                    let mode = h.borrow().row["mode"].as_str().unwrap().to_owned();
                    if mode == "task-throw" {
                        return Err(injected(&h, "task"));
                    }
                    let result = if mode == "task-reject" {
                        Err(injected(&h, "task"))
                    } else {
                        Ok(V::U)
                    };
                    let engine = h.borrow().engine.clone();
                    Ok(NativePromise::settled(engine, result))
                }),
                text("custom-prefix"),
            )
        }
        _ => panic!("method"),
    }
}
fn outcome(expected: &J, actual: Result<V, E>, h: &H, label: &str) {
    assert_eq!(
        actual.is_err(),
        expected["kind"] == "error",
        "feed::native outcome {label}"
    );
    match actual {
        Ok(value) => assert_eq!(
            value,
            decode(&expected["value"]),
            "feed::native result {label}"
        ),
        Err(e) => {
            assert_eq!(
                message(&e),
                decode(&expected["message"]),
                "feed::native error {label}"
            );
            let name = match &e {
                E::Generated(e) => e.name,
                _ => "Error",
            };
            assert_eq!(
                name,
                expected["name"].as_str().unwrap(),
                "feed::native error name {label}"
            );
            if let Some(stage) = expected["original"].as_str() {
                let E::Thrown(e) = e else {
                    panic!("feed::native original {label}")
                };
                assert!(
                    e.ptr_eq(&h.borrow().errors[stage]),
                    "feed::native original {label}"
                )
            }
        }
    }
}
fn timer_state(owner: &V) -> V {
    obj(vec![
        ("ordersThrottleTimeout", raw(owner, "ordersThrottleTimeout")),
        (
            "positionsThrottleTimeout",
            raw(owner, "positionsThrottleTimeout"),
        ),
        (
            "balanceThrottleTimeout",
            raw(owner, "balanceThrottleTimeout"),
        ),
    ])
}
fn state(h: &H) -> V {
    let s = h.borrow();
    obj(vec![
        ("isDisposed", raw(&s.owner, "isDisposed")),
        (
            "ordersThrottleTimeout",
            raw(&s.owner, "ordersThrottleTimeout"),
        ),
        (
            "positionsThrottleTimeout",
            raw(&s.owner, "positionsThrottleTimeout"),
        ),
        (
            "balanceThrottleTimeout",
            raw(&s.owner, "balanceThrottleTimeout"),
        ),
        ("parentDisposed", raw(&s.parent, "isDisposed")),
        (
            "parentUpdate",
            raw(&s.parent, "lastWebsocketPositionsUpdate"),
        ),
        (
            "alternateUpdate",
            raw(&s.alternate, "lastWebsocketPositionsUpdate"),
        ),
    ])
}
fn aliases(row: &J, h: &H, value: &V) {
    if row["method"] != "resolveExplicitlyClosedPositionUpdates" {
        return;
    }
    let V::Array(pairs) = value else {
        panic!("pairs")
    };
    let expected = row["aliases"].as_array().unwrap();
    assert_eq!(
        pairs.borrow().len(),
        expected.len(),
        "feed::native alias count {}",
        row["id"]
    );
    let s = h.borrow();
    let V::Array(positions) = &s.positions else {
        panic!("positions")
    };
    for (i, pair) in pairs.borrow().iter().enumerate() {
        let pair = pair.as_ref().unwrap();
        let old = raw(pair, "0");
        let updated = raw(pair, "1");
        let index = expected[i]["oldIndex"].as_u64().unwrap() as usize;
        assert!(
            same(&old, positions.borrow()[index].as_ref().unwrap()),
            "feed::native original position identity"
        );
        assert_eq!(
            same(&raw(&updated, "info"), &s.raw),
            expected[i]["infoIdentity"].as_bool().unwrap(),
            "feed::native info identity"
        );
        assert_eq!(
            !same(&updated, &old),
            expected[i]["distinct"].as_bool().unwrap(),
            "feed::native copied position identity"
        );
    }
}
fn run(row: &J, detached: bool) {
    let h = host(row);
    let r = runtime(h.clone());
    let engine = h.borrow().engine.clone();
    let owner = h.borrow().owner.clone();
    let id = row["id"].as_str().unwrap();
    let mut result = invoke(&r, &h);
    if row["mode"] == "replace" {
        let opts = h.borrow().opts.clone();
        let method = row["method"].as_str().unwrap();
        result = match method {
            "throttleFetchOrders" => f::throttle_fetch_orders(&r, &owner, V::Num(0.0), opts),
            "throttleFetchPositions" => f::throttle_fetch_positions(&r, &owner, V::Num(0.0)),
            _ => f::throttle_fetch_balance(&r, &owner, V::Num(0.0)),
        };
    }
    if let Ok(value) = &result {
        aliases(row, &h, value)
    }
    outcome(&row["result"], result, &h, id);
    assert_eq!(
        h.borrow().timers.len(),
        row["timers"]["items"].as_array().unwrap().len(),
        "feed::native scheduled timer count {id}"
    );
    let callback_result = Rc::new(RefCell::new(None));
    if !h.borrow().timers.is_empty() {
        if row["mode"] == "disposed-before" {
            set(&owner, "isDisposed", V::Bool(true))
        }
        let callback = h
            .borrow_mut()
            .timers
            .last_mut()
            .unwrap()
            .callback
            .take()
            .unwrap();
        let p = callback();
        if !detached {
            let captured = callback_result.clone();
            p.subscribe(move |value| *captured.borrow_mut() = Some(value));
        }
        drop(p);
        assert_eq!(
            timer_state(&owner),
            decode(&row["beforeFetch"]["state"]),
            "feed::native timer cleared {id}"
        );
        assert_eq!(
            array(h.borrow().updates.clone()),
            decode(&row["beforeFetch"]["updates"]),
            "feed::native updates before await {id}"
        );
        assert_eq!(
            h.borrow().fetch_release.is_some(),
            row["beforeFetch"]["fetchPending"].as_bool().unwrap(),
            "feed::native fetch suspension {id}"
        );
        if row["mode"] == "disposed-after" {
            set(&owner, "isDisposed", V::Bool(true))
        }
        if row["mode"] == "parent-disposed-after" {
            let parent = h.borrow().parent.clone();
            set(&parent, "isDisposed", V::Bool(true))
        }
        let release = h.borrow_mut().fetch_release.take();
        if let Some(out) = release {
            let result = if row["mode"] == "reject" {
                Err(injected(&h, "fetch"))
            } else {
                Ok(h.borrow().fetched.clone())
            };
            out.settle(result)
        }
        drain(&engine);
        if !detached {
            let value = callback_result.borrow_mut().take();
            assert!(value.is_some(), "feed::native callback settled {id}");
            outcome(
                &row["callbackResult"],
                value.unwrap().map(|v| (*v).clone()),
                &h,
                &format!("callback:{id}"),
            );
        }
    }
    assert_eq!(state(&h), decode(&row["state"]), "feed::native state {id}");
    let s = h.borrow();
    assert_eq!(
        array(s.updates.clone()),
        decode(&row["updates"]),
        "feed::native full store updates {id}"
    );
    assert_eq!(
        array(s.console_errors.clone()),
        decode(&row["consoleErrors"]),
        "feed::native visible errors {id}"
    );
    assert_eq!(
        s.task_count,
        row["taskCount"].as_u64().unwrap() as usize,
        "feed::native task receipts {id}"
    );
    assert_eq!(
        array(
            s.timers
                .iter()
                .map(|t| obj(vec![
                    ("delay", t.delay.clone()),
                    ("handle", t.handle.clone())
                ]))
                .collect()
        ),
        decode(&row["timers"]),
        "feed::native timer arguments {id}"
    );
    assert_eq!(
        array(s.cancelled.clone()),
        decode(&row["cancelled"]),
        "feed::native cancellation {id}"
    );
    let expected_calls = decode(&row["calls"]);
    let V::Array(expected) = expected_calls else {
        panic!("calls")
    };
    let expected = expected.borrow();
    let native = |v: &V| string(&raw(v, "name")) != "send";
    assert_eq!(
        array(s.calls.iter().filter(|v| native(v)).cloned().collect()),
        array(
            expected
                .iter()
                .flatten()
                .filter(|v| native(v))
                .cloned()
                .collect()
        ),
        "feed::native calls {id}"
    );
    assert_eq!(
        array(s.calls.iter().filter(|v| !native(v)).cloned().collect()),
        array(
            expected
                .iter()
                .flatten()
                .filter(|v| !native(v))
                .cloned()
                .collect()
        ),
        "feed::wire subscribe payload {id}"
    );
    let trace: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
    assert_eq!(s.events, trace, "feed::event trace {id}");
}
fn family(methods: &[&str], floor: usize) {
    let rows: Vec<_> = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| methods.contains(&r["method"].as_str().unwrap()))
        .collect();
    assert_eq!(rows.len(), floor, "feed::independent family floor");
    for row in rows {
        run(row, false)
    }
}
#[test]
fn private_feed_normalization() {
    family(
        &[
            "normalizeOrderStatus",
            "isUserCancelledReason",
            "resolveExplicitlyClosedPositionUpdates",
        ],
        34,
    )
}
#[test]
fn private_feed_throttles() {
    family(
        &[
            "throttleFetchOrders",
            "throttleFetchPositions",
            "throttleFetchBalance",
            "scheduleThrottledTask",
        ],
        35,
    )
}
#[test]
fn private_feed_subscribe() {
    family(&["sendSubscribePayload"], 6)
}
#[test]
fn private_feed_detached() {
    let row = source()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "throttleFetchOrders-default")
        .unwrap();
    run(row, true)
}
