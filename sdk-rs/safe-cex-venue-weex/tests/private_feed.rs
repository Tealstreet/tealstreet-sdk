use safe_cex_types::OrderStatus;
use safe_cex_venue_weex::private_feed::*;
use serde_json::Value as Json;
use std::{
    cell::RefCell,
    future::Future,
    path::Path,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[path = "support/source_profile.rs"]
mod source_profile;
#[derive(Debug, Clone)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Vec<u16>),
    Object(Rc<RefCell<Vec<(String, V)>>>),
    Array(Rc<RefCell<Vec<Option<V>>>>),
    Iterator(Rc<RefCell<(V, usize)>>),
}
impl PartialEq for V {
    fn eq(&self, b: &Self) -> bool {
        match (self, b) {
            (Self::Undefined, Self::Undefined) | (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a.to_bits() == b.to_bits(),
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => *a.borrow() == *b.borrow(),
            (Self::Array(a), Self::Array(b)) => *a.borrow() == *b.borrow(),
            _ => false,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
struct Error {
    name: &'static str,
    units: Vec<u16>,
    header: bool,
}
type H = RefCell<State>;
type RT = WeexPrivateFeedRuntime<H, V, usize, u64, Error>;
struct State {
    input: Json,
    disposed: bool,
    id: f64,
    events: Vec<String>,
    json: V,
    stored: V,
    provider: Option<fn(&H) -> Result<V, Error>>,
    nonce: f64,
    created: Vec<(Vec<u16>, Option<Vec<String>>, bool)>,
    listeners: Vec<String>,
    headers: Vec<V>,
    warnings: Vec<Vec<u16>>,
    orders: Vec<V>,
    statuses: Vec<(V, V)>,
    positions: Vec<V>,
    fills: Vec<WeexPrivateFill<V>>,
    timers: Vec<(WeexRefreshKind, u64, u64)>,
    handles: [Option<u64>; 3],
    next_timer: u64,
    applied: Vec<(String, V)>,
}
fn units(v: &Json) -> Vec<u16> {
    serde_json::from_value(v.clone()).unwrap()
}
fn bits(v: &Json) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}
fn text(s: &str) -> V {
    V::String(s.encode_utf16().collect())
}
fn object(fields: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(RefCell::new(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn from_json(j: &Json) -> V {
    match j {
        Json::Null => V::Null,
        Json::Bool(b) => V::Bool(*b),
        Json::Number(n) => V::Number(n.as_f64().unwrap()),
        Json::String(s) => text(s),
        Json::Array(a) => V::Array(Rc::new(RefCell::new(
            a.iter().map(|x| Some(from_json(x))).collect(),
        ))),
        Json::Object(o) => V::Object(Rc::new(RefCell::new(
            o.iter().map(|(k, v)| (k.clone(), from_json(v))).collect(),
        ))),
    }
}
fn decode(j: &Json) -> V {
    match j["kind"].as_str().unwrap() {
        "undefined" => V::Undefined,
        "null" => V::Null,
        "boolean" => V::Bool(j["value"] == true),
        "number" => V::Number(bits(&j["bits"])),
        "string" => V::String(units(&j["units"])),
        "array" => V::Array(Rc::new(RefCell::new(
            j["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    if v["kind"] == "hole" {
                        None
                    } else {
                        Some(decode(v))
                    }
                })
                .collect(),
        ))),
        "object" => V::Object(Rc::new(RefCell::new(
            j["fields"]
                .as_array()
                .unwrap()
                .iter()
                .map(|f| (f[0].as_str().unwrap().into(), decode(&f[1])))
                .collect(),
        ))),
        other => panic!("unknown {other}"),
    }
}
fn kind(s: &str) -> WeexRefreshKind {
    match s {
        "orders" => WeexRefreshKind::Orders,
        "positions" => WeexRefreshKind::Positions,
        "balance" => WeexRefreshKind::Balance,
        _ => panic!("unknown kind"),
    }
}
fn label(k: WeexRefreshKind) -> &'static str {
    match k {
        WeexRefreshKind::Orders => "orders",
        WeexRefreshKind::Positions => "positions",
        WeexRefreshKind::Balance => "balance",
    }
}
fn index(k: WeexRefreshKind) -> usize {
    match k {
        WeexRefreshKind::Orders => 0,
        WeexRefreshKind::Positions => 1,
        WeexRefreshKind::Balance => 2,
    }
}
fn check(h: &H, event: &str) -> Result<(), Error> {
    if h.borrow().input["fail"] == event {
        Err(Error {
            name: "RangeError",
            units: format!("fail:{event}").encode_utf16().collect(),
            header: false,
        })
    } else {
        Ok(())
    }
}
fn record(h: &H, event: &str) -> Result<(), Error> {
    h.borrow_mut().events.push(event.into());
    check(h, event)
}
fn property(_: &H, v: &V, key: &str) -> Result<V, Error> {
    match v {
        V::Object(o) => Ok(o
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Undefined)),
        V::Null | V::Undefined => Err(Error {
            name: "TypeError",
            units: format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v, V::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )
            .encode_utf16()
            .collect(),
            header: false,
        }),
        _ => Ok(V::Undefined),
    }
}
fn truthy(v: &V) -> bool {
    match v {
        V::Undefined | V::Null => false,
        V::Bool(b) => *b,
        V::Number(n) => *n != 0.0 && !n.is_nan(),
        V::String(s) => !s.is_empty(),
        _ => true,
    }
}
// `h` is threaded through every helper in this fixture and several call sites
// are closures whose signature supplies it, so the parameter stays even though
// only the recursive call reads it here.
#[allow(clippy::only_used_in_recursion)]
fn stringify(h: &H, v: &V) -> Result<Vec<u16>, Error> {
    Ok(match v {
        V::Undefined => "undefined".encode_utf16().collect(),
        V::Null => "null".encode_utf16().collect(),
        V::Bool(b) => b.to_string().encode_utf16().collect(),
        V::Number(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
        V::String(s) => s.clone(),
        V::Object(_) => "[object Object]".encode_utf16().collect(),
        V::Array(a) => {
            let mut result = Vec::new();
            for (i, v) in a.borrow().iter().enumerate() {
                if i > 0 {
                    result.push(44);
                }
                if let Some(v) = v {
                    if !matches!(v, V::Null | V::Undefined) {
                        result.extend(stringify(h, v)?);
                    }
                }
            }
            result
        }
        _ => panic!("iterator string"),
    })
}
fn number(h: &H, v: &V) -> Result<f64, Error> {
    if matches!(v, V::Null | V::Undefined) || matches!(v,V::String(s)if s.is_empty()) {
        return Ok(0.0);
    }
    let s = String::from_utf16_lossy(&stringify(h, v)?);
    let n = safe_cex_num::parse_float(&s);
    Ok(if n.is_finite() { n } else { 0.0 })
}
fn timestamp(h: &H, v: &V) -> Result<f64, Error> {
    let s = String::from_utf16_lossy(&stringify(h, v)?);
    let n = s.parse::<f64>().unwrap_or(0.0);
    Ok(if n > 0.0 && n.is_finite() {
        let ms = if n >= 1e18 {
            (n / 1e6).floor()
        } else if n >= 1e15 {
            (n / 1e3).floor()
        } else if n >= 1e12 {
            n.floor()
        } else {
            (n * 1000.0).floor()
        };
        (ms / 1000.0).floor()
    } else {
        0.0
    })
}
fn uppercase(_: &H, s: &[u16]) -> Result<Vec<u16>, Error> {
    let mut out = Vec::new();
    for c in char::decode_utf16(s.iter().copied()) {
        match c {
            Ok(c) => {
                for upper in c.to_uppercase() {
                    let mut buffer = [0u16; 2];
                    out.extend_from_slice(upper.encode_utf16(&mut buffer));
                }
            }
            Err(e) => out.push(e.unpaired_surrogate()),
        }
    }
    Ok(out)
}
fn array_length(_: &H, v: &V) -> Result<usize, Error> {
    match v {
        V::Array(a) => Ok(a.borrow().len()),
        _ => Err(Error {
            name: "TypeError",
            units: "array method unavailable".encode_utf16().collect(),
            header: false,
        }),
    }
}
fn element(_: &H, v: &V, i: usize) -> Result<Option<V>, Error> {
    match v {
        V::Array(a) => Ok(a.borrow().get(i).cloned().flatten()),
        _ => panic!("not array"),
    }
}
fn iterator(_: &H, v: &V) -> Result<V, Error> {
    Ok(V::Iterator(Rc::new(RefCell::new((v.clone(), 0)))))
}
fn next(h: &H, v: &V) -> Result<Option<V>, Error> {
    let V::Iterator(it) = v else {
        panic!("not iterator")
    };
    let mut it = it.borrow_mut();
    if it.1 >= array_length(h, &it.0)? {
        return Ok(None);
    }
    let value = element(h, &it.0, it.1)?.unwrap_or(V::Undefined);
    it.1 += 1;
    Ok(Some(value))
}
fn close(h: &H, _: &V) -> Result<(), Error> {
    if h.borrow().input["customIterator"] == true {
        record(h, "iteratorClose")?;
    }
    Ok(())
}
fn strict(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        (V::Number(a), V::Number(b)) => a == b,
        _ => a == b,
    }
}
fn disposed(h: &H) -> Result<bool, Error> {
    record(h, "disposed")?;
    Ok(h.borrow().disposed)
}
fn supported(h: &H) -> Result<bool, Error> {
    record(h, "supported")?;
    Ok(h.borrow().input["supported"] == true)
}
fn credential(h: &H, key: &str) -> Result<V, Error> {
    record(h, &format!("credential:{key}"))?;
    Ok(h.borrow().input["credentials"]
        .get(key)
        .map(from_json)
        .unwrap_or(V::Undefined))
}
fn auth(h: &H) -> Result<V, Error> {
    record(h, "auth")?;
    let v = object(vec![("nonce", V::Number(h.borrow().nonce))]);
    h.borrow_mut().headers.push(v.clone());
    Ok(v)
}
fn create(
    h: &H,
    url: &'static str,
    protocols: Option<&[&str]>,
    provider: fn(&H) -> Result<V, Error>,
    required: bool,
) -> Result<usize, Error> {
    record_raw(h, "create");
    h.borrow_mut().created.push((
        url.encode_utf16().collect(),
        protocols.map(|a| a.iter().map(|s| s.to_string()).collect()),
        required,
    ));
    h.borrow_mut().provider = Some(provider);
    if h.borrow().input["headerError"] == true {
        return Err(Error {
            name: "Error",
            units: "header".encode_utf16().collect(),
            header: true,
        });
    }
    check(h, "create")?;
    Ok(1)
}
fn record_raw(h: &H, s: &str) {
    h.borrow_mut().events.push(s.into());
}
fn set_socket(h: &H, _: usize) -> Result<(), Error> {
    record(h, "socket:set")
}
fn socket(h: &H) -> Result<usize, Error> {
    record(h, "socket")?;
    Ok(1)
}
fn listener(h: &H, _: &usize, event: &'static str) -> Result<(), Error> {
    record(h, &format!("listener:{event}"))?;
    h.borrow_mut().listeners.push(event.into());
    Ok(())
}
fn unsupported(h: &H, reason: Option<&str>) -> Result<(), Error> {
    record(h, "unsupported")?;
    h.borrow_mut()
        .warnings
        .push(reason.unwrap_or("default").encode_utf16().collect());
    Ok(())
}
fn markets(h: &H) -> Result<V, Error> {
    record(h, "markets")?;
    Ok(object(vec![]))
}
fn market(h: &H, _: &V, _: &V) -> Result<V, Error> {
    record(h, "market")?;
    Ok(object(vec![]))
}
fn normalize_order(h: &H, row: &V, _: &V) -> Result<V, Error> {
    record(h, "normalizeOrder")?;
    property(h, row, "normalized")
}
fn normalize_position(h: &H, row: &V, _: &V) -> Result<V, Error> {
    record(h, "normalizePosition")?;
    property(h, row, "normalized")
}
fn change_status(v: &V) {
    if let V::Object(o) = v {
        for (k, v) in o.borrow_mut().iter_mut() {
            if k == "status" {
                *v = text("closed");
            }
        }
    }
}
// The signature is fixed by the host contract: this is installed as a fn
// pointer whose declared type is `&mut Vec<V>`, so &mut [V] does not coerce.
#[allow(clippy::ptr_arg)]
fn add_orders(h: &H, orders: &mut Vec<V>) -> Result<(), Error> {
    record(h, "addOrders")?;
    if h.borrow().input["afterAddClosed"] == true {
        change_status(&orders[0]);
    }
    h.borrow_mut().orders.push(V::Array(Rc::new(RefCell::new(
        orders.iter().cloned().map(Some).collect(),
    ))));
    Ok(())
}
fn update_status(h: &H, id: V, status: V) -> Result<(), Error> {
    record(h, "updateStatus")?;
    h.borrow_mut().statuses.push((id, status));
    Ok(())
}
fn add_positions(h: &H, p: Vec<V>) -> Result<(), Error> {
    record(h, "addPositions")?;
    h.borrow_mut().positions.push(V::Array(Rc::new(RefCell::new(
        p.into_iter().map(Some).collect(),
    ))));
    Ok(())
}
fn emit_fill(h: &H, fill: WeexPrivateFill<V>) -> Result<(), Error> {
    record(h, "emitFill")?;
    h.borrow_mut().fills.push(fill);
    if h.borrow().input["appendFill"] == true && h.borrow().fills.len() == 1 {
        let array = property(h, &h.borrow().json, "d")?;
        if let V::Array(a) = array {
            let mut a = a.borrow_mut();
            let v = a[0].as_ref().unwrap().clone();
            let mut fields = if let V::Object(o) = v {
                o.borrow().clone()
            } else {
                panic!("not object")
            };
            for (k, v) in &mut fields {
                if k == "id" {
                    *v = text("APPENDED");
                }
            }
            a.push(Some(V::Object(Rc::new(RefCell::new(fields)))));
        }
    }
    Ok(())
}
fn track(h: &H) -> Result<(), Error> {
    record(h, "track")?;
    if h.borrow().input["disposeOnTrack"] == true {
        h.borrow_mut().disposed = true;
    }
    Ok(())
}
fn send_pong(h: &H, id: f64) -> Result<(), Error> {
    h.borrow_mut()
        .applied
        .push(("rawPong".into(), V::Number(id)));
    record(h, "sendPong")
}
fn handle_pong(h: &H, interval: f64) -> Result<(), Error> {
    record(h, "pong")?;
    h.borrow_mut()
        .applied
        .push(("pong".into(), V::Number(interval)));
    Ok(())
}
fn warning(h: &H, s: Vec<u16>) -> Result<(), Error> {
    record(h, "warning")?;
    h.borrow_mut().warnings.push(s);
    Ok(())
}
fn stats(h: &H, v: &V) -> Result<(), Error> {
    record(h, "stats")?;
    h.borrow_mut().applied.push(("stats".into(), v.clone()));
    Ok(())
}
fn timer(h: &H, k: WeexRefreshKind) -> Result<Option<u64>, Error> {
    record(h, &format!("timer:{}", label(k)))?;
    Ok(h.borrow().handles[index(k)])
}
fn clear(h: &H, t: Option<&u64>) -> Result<(), Error> {
    record(
        h,
        &format!(
            "clear:{}",
            t.map_or_else(|| "undefined".into(), |v| v.to_string())
        ),
    )
}
fn schedule(h: &H, k: WeexRefreshKind, delay: u64) -> Result<u64, Error> {
    record_raw(h, &format!("schedule:{}", label(k)));
    check(h, "schedule")?;
    let mut s = h.borrow_mut();
    let id = s.next_timer;
    s.next_timer += 1;
    s.timers.push((k, delay, id));
    Ok(id)
}
fn assign(h: &H, k: WeexRefreshKind, t: u64) -> Result<(), Error> {
    record(h, &format!("assign:{}", label(k)))?;
    h.borrow_mut().handles[index(k)] = Some(t);
    Ok(())
}
struct Fetch<'a> {
    host: &'a H,
    kind: WeexRefreshKind,
    pending: bool,
}
impl Future for Fetch<'_> {
    type Output = Result<V, Error>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.pending {
            self.pending = false;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        record_raw(self.host, &format!("resolved:{}", label(self.kind)));
        if self.host.borrow().input["disposeDuringFetch"] == true {
            self.host.borrow_mut().disposed = true;
        }
        Poll::Ready(Ok(object(vec![("refreshed", text(label(self.kind)))])))
    }
}
fn fetch(
    h: &H,
    k: WeexRefreshKind,
) -> Result<Pin<Box<dyn Future<Output = Result<V, Error>> + '_>>, Error> {
    record(h, &format!("fetch:{}", label(k)))?;
    Ok(Box::pin(Fetch {
        host: h,
        kind: k,
        pending: true,
    }))
}
fn apply(h: &H, k: WeexRefreshKind, v: V) -> Result<(), Error> {
    record(h, &format!("apply:{}", label(k)))?;
    h.borrow_mut().applied.push((label(k).into(), v));
    Ok(())
}
fn base_dispose(h: &H) -> Result<(), Error> {
    record(h, "baseDispose")?;
    h.borrow_mut().disposed = true;
    Ok(())
}
fn parse(h: &H, s: &[u16]) -> Result<V, Error> {
    record(h, "parse")?;
    let source = String::from_utf16_lossy(s);
    let j: Json = match serde_json::from_str(&source) {
        Ok(j) => j,
        Err(_) => return Ok(V::Null),
    };
    fn native(j: &Json, nested: bool) -> V {
        match j {
            Json::Number(n) => {
                let s = n.to_string();
                if nested
                    && s.trim_start_matches('-').len() >= 16
                    && s.trim_start_matches('-')
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                {
                    text(&s)
                } else {
                    from_json(j)
                }
            }
            Json::Array(a) => V::Array(Rc::new(RefCell::new(
                a.iter().map(|v| Some(native(v, true))).collect(),
            ))),
            Json::Object(o) => V::Object(Rc::new(RefCell::new(
                o.iter()
                    .map(|(k, v)| (k.clone(), native(v, true)))
                    .collect(),
            ))),
            _ => from_json(j),
        }
    }
    Ok(native(&j, false))
}
fn runtime() -> RT {
    RT {
        disposed,
        supported_runtime: supported,
        credential,
        truthy,
        undefined: |v| matches!(v, V::Undefined),
        string_equals: |v, s| *v == text(s),
        is_false: |v| matches!(v, V::Bool(false)),
        make_string: |_, s| Ok(text(s)),
        stringify,
        template_string: stringify,
        property,
        is_array: |_, v| Ok(matches!(v, V::Array(_))),
        make_string_units: |_, s| Ok(V::String(s)),
        make_undefined: |_| Ok(V::Undefined),
        uppercase,
        map_length: array_length,
        find_length: array_length,
        array_element: element,
        empty_array: |_| Ok(V::Array(Rc::new(RefCell::new(vec![])))),
        iterator,
        iterator_next: next,
        iterator_close: close,
        parse,
        number,
        timestamp_seconds: timestamp,
        markets_by_symbol: markets,
        market,
        normalize_order,
        normalize_position,
        stored_orders: |h| Ok(h.borrow().stored.clone()),
        strict_equal: strict,
        add_orders,
        order_status: |v| {
            if *v == text("closed") {
                Some(OrderStatus::Closed)
            } else if *v == text("canceled") {
                Some(OrderStatus::Canceled)
            } else {
                None
            }
        },
        update_status,
        add_positions,
        emit_fill,
        track_received: track,
        subscription_id: |h| {
            record(h, "id:get")?;
            Ok(h.borrow().id)
        },
        set_subscription_id: |h, id| {
            record(h, "id:set")?;
            h.borrow_mut().id = id;
            Ok(())
        },
        send_pong,
        handle_pong,
        warning,
        record_stats: stats,
        log_unsupported: unsupported,
        auth_headers: auth,
        create_socket: create,
        set_socket,
        socket,
        add_listener: listener,
        header_unavailable: |e| e.header,
        timer,
        clear_timeout: clear,
        set_timeout: schedule,
        assign_timer: assign,
        fetch_refresh: fetch,
        apply_refresh: apply,
        base_dispose,
    }
}
fn state(input: &Json) -> State {
    let json = from_json(&input["json"]);
    if let V::Array(a) = property(&RefCell::new(State::empty()), &json, "d").unwrap_or(V::Undefined)
    {
        for hole in input["holes"].as_array().into_iter().flatten() {
            a.borrow_mut()[hole.as_u64().unwrap() as usize] = None;
        }
    }
    State {
        input: input.clone(),
        disposed: input["disposed"] == true,
        id: bits(&input["subscriptionId"]),
        events: vec![],
        json,
        stored: from_json(&input["stored"]),
        provider: None,
        nonce: 1.0,
        created: vec![],
        listeners: vec![],
        headers: vec![],
        warnings: vec![],
        orders: vec![],
        statuses: vec![],
        positions: vec![],
        fills: vec![],
        timers: vec![],
        handles: [0, 1, 2].map(|i| input["oldTimers"].get(i).and_then(Json::as_u64)),
        next_timer: 100,
        applied: vec![],
    }
}
impl State {
    fn empty() -> Self {
        Self {
            input: Json::Null,
            disposed: false,
            id: 0.0,
            events: vec![],
            json: V::Null,
            stored: V::Null,
            provider: None,
            nonce: 0.0,
            created: vec![],
            listeners: vec![],
            headers: vec![],
            warnings: vec![],
            orders: vec![],
            statuses: vec![],
            positions: vec![],
            fills: vec![],
            timers: vec![],
            handles: [None, None, None],
            next_timer: 0,
            applied: vec![],
        }
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    for _ in 0..8 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("refresh poll floor exceeded")
}

fn ready_refresh_waits(kind: WeexRefreshKind) {
    for case in ["success", "disposed", "rejected", "apply-error"] {
        let h = RefCell::new(State::empty());
        if case == "apply-error" {
            h.borrow_mut().input = serde_json::json!({"fail":format!("apply:{}", label(kind))});
        }
        let mut rt = runtime();
        rt.fetch_refresh = |h, kind| {
            record_raw(h, &format!("fetch:{}", label(kind)));
            Ok(Box::pin(std::future::ready(Ok(text("ready")))))
        };
        if case == "rejected" {
            rt.fetch_refresh = |h, kind| {
                record_raw(h, &format!("fetch:{}", label(kind)));
                Ok(Box::pin(std::future::ready(Err(Error {
                    name: "RangeError",
                    units: "ready rejection".encode_utf16().collect(),
                    header: false,
                }))))
            };
        }
        let mut future = run_refresh_timer(&h, &rt, kind);
        assert_eq!(h.borrow().events, [format!("fetch:{}", label(kind))]);
        let waker = Waker::noop();
        let mut context = Context::from_waker(waker);
        assert!(
            future.as_mut().poll(&mut context).is_pending(),
            "WEEX ready refresh reaction waits {}:{case}",
            label(kind)
        );
        assert!(
            h.borrow().applied.is_empty(),
            "WEEX no inline refresh update"
        );
        if case == "disposed" {
            h.borrow_mut().disposed = true;
        }
        assert_eq!(run(future), Ok(()));
        let state = h.borrow();
        assert_eq!(state.applied.len(), usize::from(case == "success"));
        if case == "success" {
            assert_eq!(state.applied[0], (label(kind).to_owned(), text("ready")));
        }
    }
}

#[test]
fn ready_orders_refresh_waits_for_reaction() {
    ready_refresh_waits(WeexRefreshKind::Orders);
    for observation in source_profile::observe(
        "tests/divergence-observations.cjs",
        "refresh-turn",
        "WEEX SOURCE ready refresh defers continuation",
    ) {
        let cases = observation["cases"].as_array().unwrap();
        let expected = ["orders", "positions", "balance"]
            .into_iter()
            .flat_map(|kind| {
                ["success", "disposed", "rejected", "apply-error"]
                    .into_iter()
                    .map(move |case| format!("{kind}:{case}"))
            })
            .collect::<Vec<_>>();
        assert_eq!(cases.len(), 12, "WEEX source refresh case floor");
        assert_eq!(
            cases
                .iter()
                .map(|case| case["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn ready_positions_refresh_waits_for_reaction() {
    ready_refresh_waits(WeexRefreshKind::Positions);
}

#[test]
fn ready_balance_refresh_waits_for_reaction() {
    ready_refresh_waits(WeexRefreshKind::Balance);
}

#[test]
fn native_private_feed_matches_source() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/private-feed-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Json> = serde_json::from_slice(&output.stdout).unwrap();
    let ids = [
        "connect_basic",
        "connect_disposed",
        "connect_browser",
        "connect_key_missing",
        "connect_secret_missing",
        "connect_pass_missing",
        "connect_testnet",
        "connect_header_failure",
        "connect_other_failure",
        "connect_listener_failure",
        "message_disposed",
        "message_invalid",
        "message_null",
        "message_ping",
        "message_ping_send_failure",
        "message_result_false",
        "message_result_default",
        "message_result_null",
        "message_missing_event",
        "message_unknown",
        "message_account",
        "message_track_disposes",
        "message_large_fill",
        "orders_closed",
        "orders_null_normalizer",
        "orders_non_array",
        "orders_add_changes_status",
        "orders_add_failure",
        "orders_update_failure",
        "orders_sparse",
        "positions_basic",
        "positions_non_array",
        "positions_add_failure",
        "fills_basic",
        "fills_order_side",
        "fills_zero_amount",
        "fills_zero_value",
        "fills_fallback_fields",
        "fills_unicode_side",
        "fills_infinity",
        "fills_live_append",
        "fills_sparse",
        "fills_emit_failure_close",
        "fills_non_array",
        "orders_schedule",
        "orders_refresh",
        "orders_disposed_after_fetch",
        "orders_fetch_failure",
        "orders_apply_failure",
        "positions_schedule",
        "positions_refresh",
        "positions_disposed_after_fetch",
        "positions_fetch_failure",
        "positions_apply_failure",
        "balance_schedule",
        "balance_refresh",
        "balance_disposed_after_fetch",
        "balance_fetch_failure",
        "balance_apply_failure",
        "orders_missing_timer",
        "dispose_missing_timers",
        "dispose_basic",
        "dispose_clear_failure",
    ];
    assert_eq!(
        cases
            .iter()
            .map(|c| c["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ids,
        "native_case_floor"
    );
    let rt = runtime();
    let mut traces = Vec::new();
    for c in &cases {
        let id = c["id"].as_str().unwrap();
        let input = &c["input"];
        let h = RefCell::new(state(input));
        let json = h.borrow().json.clone();
        let k = kind(input["timerKind"].as_str().unwrap());
        let result = match input["method"].as_str().unwrap() {
            "connect" => {
                let r = connect_and_subscribe(&h, &rt);
                if r.is_ok() && input["callHeaders"] == true {
                    let p = h.borrow().provider;
                    if let Some(p) = p {
                        h.borrow_mut().nonce = 2.0;
                        p(&h).map(|_| ())
                    } else {
                        r
                    }
                } else {
                    r
                }
            }
            "message" => on_message(&h, &rt, &from_json(&input["data"])),
            "orders" => handle_orders(&h, &rt, &json),
            "positions" => handle_positions(&h, &rt, &json),
            "fills" => handle_fills(&h, &rt, &json),
            "dispose" => dispose(&h, &rt),
            "timer" => {
                let r = match k {
                    WeexRefreshKind::Orders => throttled_fetch_orders(&h, &rt),
                    WeexRefreshKind::Positions => throttled_fetch_positions(&h, &rt),
                    WeexRefreshKind::Balance => throttled_fetch_balance(&h, &rt),
                };
                if r.is_ok() && input["fire"] == true {
                    let f = run_refresh_timer(&h, &rt, k);
                    record_raw(&h, "callbackStarted");
                    assert!(
                        h.try_borrow_mut().is_ok(),
                        "native_host_guard_released::{id}"
                    );
                    run(f)
                } else {
                    r
                }
            }
            _ => panic!("unknown method"),
        };
        let s = h.into_inner();
        if c["error"].is_null() {
            assert_eq!(result, Ok(()), "native_result::{id}");
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.name,
                c["error"]["name"].as_str().unwrap(),
                "native_error_name::{id}"
            );
            assert_eq!(
                error.units,
                units(&c["error"]["units"]),
                "native_error_message::{id}"
            );
        }
        assert_eq!(s.disposed, c["disposed"] == true, "native_disposed::{id}");
        assert_eq!(
            s.id.to_bits(),
            bits(&c["subscriptionId"]).to_bits(),
            "native_subscription_id::{id}"
        );
        assert_eq!(
            s.created,
            c["created"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (units(&v["url"]), None, v["headersRequired"] == true))
                .collect::<Vec<_>>(),
            "native_socket_options::{id}"
        );
        assert_eq!(
            s.listeners,
            c["listeners"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "native_listeners::{id}"
        );
        assert_eq!(
            s.headers,
            c["headers"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>(),
            "native_live_headers::{id}"
        );
        assert_eq!(
            s.warnings,
            c["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .map(units)
                .collect::<Vec<_>>(),
            "native_warnings::{id}"
        );
        assert_eq!(
            s.orders,
            c["addedOrders"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>(),
            "native_orders::{id}"
        );
        assert_eq!(
            s.statuses,
            c["statuses"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (decode(&v["id"]), decode(&v["status"])))
                .collect::<Vec<_>>(),
            "native_statuses::{id}"
        );
        assert_eq!(
            s.positions,
            c["addedPositions"]
                .as_array()
                .unwrap()
                .iter()
                .map(decode)
                .collect::<Vec<_>>(),
            "native_positions::{id}"
        );
        let expected_fill_count = match id {
            "message_large_fill"
            | "fills_basic"
            | "fills_order_side"
            | "fills_zero_value"
            | "fills_fallback_fields"
            | "fills_unicode_side"
            | "fills_infinity" => 1,
            "fills_live_append" => 2,
            _ => 0,
        };
        assert_eq!(
            s.fills.len(),
            expected_fill_count,
            "native_fill_count::{id}"
        );
        assert_eq!(
            c["fills"].as_array().unwrap().len(),
            expected_fill_count,
            "source_fill_count::{id}"
        );
        assert_eq!(
            s.applied
                .iter()
                .filter(|(k, _)| k == "rawPong")
                .map(|(_, v)| v.clone())
                .collect::<Vec<_>>(),
            c["rawPongs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| V::Number(bits(n)))
                .collect::<Vec<_>>(),
            "native_raw_pong::{id}"
        );
        for (actual, expected) in s.fills.iter().zip(c["fills"].as_array().unwrap()) {
            assert_eq!(actual.id, units(&expected["id"]), "native_fill_id::{id}");
            assert_eq!(
                actual.order_id,
                units(&expected["orderId"]),
                "native_fill_order_id::{id}"
            );
            assert_eq!(
                actual.side,
                decode(&expected["side"]),
                "native_fill_side::{id}"
            );
            assert_eq!(
                actual.symbol,
                decode(&expected["symbol"]),
                "native_fill_symbol::{id}"
            );
            for (name, n) in [
                ("price", actual.price),
                ("amount", actual.amount),
                ("timestamp", actual.timestamp),
                ("fee", actual.fee),
                ("realizedPnl", actual.realized_pnl),
            ] {
                assert_eq!(
                    n.to_bits(),
                    bits(&expected[name]).to_bits(),
                    "native_fill_{name}::{id}"
                );
            }
        }
        assert_eq!(
            s.timers,
            c["timers"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (
                    kind(v["kind"].as_str().unwrap()),
                    v["delay"].as_u64().unwrap(),
                    v["id"].as_u64().unwrap()
                ))
                .collect::<Vec<_>>(),
            "native_timers::{id}"
        );
        let expected_applied = c["applied"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v["kind"].as_str().unwrap().to_string(), decode(&v["value"])))
            .collect::<Vec<_>>();
        assert_eq!(
            s.applied
                .iter()
                .filter(|(k, _)| k != "rawPong")
                .cloned()
                .collect::<Vec<_>>(),
            expected_applied,
            "native_refresh_updates::{id}"
        );
        if id == "fills_live_append" {
            assert_eq!(s.fills.len(), 2, "native_live_fill_floor");
        }
        if id == "orders_closed" {
            assert_eq!(s.statuses.len(), 2, "native_closed_order_floor");
        }
        traces.push((
            id.to_string(),
            s.events.into_iter().collect::<Vec<_>>(),
            c["events"].clone(),
        ));
    }
    for (id, events, expected) in traces {
        assert_eq!(
            events,
            expected
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect::<Vec<_>>(),
            "private_feed_effect_trace::{id}"
        );
    }
}
