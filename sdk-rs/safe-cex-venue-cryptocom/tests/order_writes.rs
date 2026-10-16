use safe_cex_venue_cryptocom::order_writes::{self as b, BoxFuture, WriteHost};
use serde_json::Value as Json;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<N>;
type E = V;
#[derive(Debug)]
enum N {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Obj {
        tag: Option<String>,
        fields: RefCell<Vec<(Vec<u16>, V)>>,
        prototype: Option<V>,
    },
    Arr {
        tag: Option<String>,
        values: RefCell<Vec<V>>,
    },
    Opaque {
        tag: String,
        class: String,
    },
    CustomString {
        tag: String,
        returned: Json,
    },
}
fn text(s: &str) -> V {
    Rc::new(N::Text(s.encode_utf16().collect()))
}
fn utf16(s: Vec<u16>) -> V {
    Rc::new(N::Text(s))
}
fn undefined() -> V {
    Rc::new(N::Undefined)
}
fn opaque(tag: &str, class: &str) -> V {
    Rc::new(N::Opaque {
        tag: tag.into(),
        class: class.into(),
    })
}
fn units(v: &Json) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn tag(v: &V) -> Option<String> {
    match &**v {
        N::Obj { tag, .. } | N::Arr { tag, .. } => tag.clone(),
        N::Opaque { tag, .. } | N::CustomString { tag, .. } => Some(tag.clone()),
        _ => None,
    }
}
fn input(v: &Json, t: &str) -> V {
    if v["$undefined"] == true {
        return undefined();
    }
    if v["$units"].is_array() {
        return utf16(units(&v["$units"]));
    }
    if let Some(n) = v["$number"].as_str() {
        return Rc::new(N::Num(match n {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("fixture number"),
        }));
    }
    if let Some(t) = v["$opaque"].as_str() {
        return opaque(&format!("opaque:{t}"), "object");
    }
    if let Some(returned) = v.get("$toString") {
        return Rc::new(N::CustomString {
            tag: t.into(),
            returned: returned.clone(),
        });
    }
    Rc::new(match v {
        Json::Null => N::Null,
        Json::Bool(v) => N::Bool(*v),
        Json::Number(v) => N::Num(v.as_f64().unwrap()),
        Json::String(v) => N::Text(v.encode_utf16().collect()),
        Json::Array(v) => N::Arr {
            tag: Some(t.into()),
            values: RefCell::new(
                v.iter()
                    .enumerate()
                    .map(|(i, v)| input(v, &format!("{t}.{i}")))
                    .collect(),
            ),
        },
        Json::Object(v) => N::Obj {
            tag: Some(t.into()),
            fields: RefCell::new(
                v.iter()
                    .filter(|(k, _)| *k != "$prototype")
                    .map(|(k, v)| (k.encode_utf16().collect(), input(v, &format!("{t}.{k}"))))
                    .collect(),
            ),
            prototype: v
                .get("$prototype")
                .map(|v| input(v, &format!("{t}.prototype"))),
        },
    })
}
fn primitive_text(v: &V) -> Vec<u16> {
    match &**v {
        N::Text(v) => v.clone(),
        N::Undefined => "undefined".encode_utf16().collect(),
        N::Null => "null".encode_utf16().collect(),
        N::Bool(v) => v.to_string().encode_utf16().collect(),
        N::Num(v) => {
            let s = if v.is_nan() {
                "NaN".into()
            } else if *v == f64::INFINITY {
                "Infinity".into()
            } else if *v == f64::NEG_INFINITY {
                "-Infinity".into()
            } else if *v == 0.0 {
                "0".into()
            } else {
                v.to_string()
            };
            s.encode_utf16().collect()
        }
        _ => "[object Object]".encode_utf16().collect(),
    }
}
fn assert_native(v: &V, e: &Json, label: &str) {
    match (
        &**v,
        e["kind"]
            .as_str()
            .unwrap_or_else(|| panic!("CRYPTOCOM-WRITE-NATIVE-MISSING-EXPECTATION {label}: {e}")),
    ) {
        (N::Undefined, "undefined") | (N::Null, "null") => {}
        (N::Bool(v), "bool") => assert_eq!(
            *v,
            e["value"].as_bool().unwrap(),
            "CRYPTOCOM-WRITE-NATIVE-BOOL {label}"
        ),
        (N::Num(v), "number") => {
            let n = f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
            assert!(
                v.is_nan() && n.is_nan() || v.to_bits() == n.to_bits(),
                "CRYPTOCOM-WRITE-NATIVE-NUMBER {label}"
            );
        }
        (N::Text(v), "string") => assert_eq!(
            *v,
            units(&e["units"]),
            "CRYPTOCOM-WRITE-NATIVE-TEXT {label}"
        ),
        (N::Arr { values, .. }, "array") => {
            let values = values.borrow();
            let expected = e["values"].as_array().unwrap();
            assert_eq!(
                values.len(),
                expected.len(),
                "CRYPTOCOM-WRITE-NATIVE-ARRAY {label}"
            );
            for (i, (v, e)) in values.iter().zip(expected).enumerate() {
                assert_native(v, e, &format!("{label}[{i}]"));
            }
        }
        (N::Obj { fields, .. }, "object") => {
            let fields = fields.borrow();
            let expected = e["fields"].as_array().unwrap();
            assert_eq!(
                fields.len(),
                expected.len(),
                "CRYPTOCOM-WRITE-NATIVE-FIELDS {label}"
            );
            for field in expected {
                let key = units(&field[0]);
                let v = &fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .unwrap_or_else(|| panic!("CRYPTOCOM-WRITE-NATIVE-KEY {label} {key:?}"))
                    .1;
                assert_native(
                    v,
                    &field[1],
                    &format!("{label}.{}", String::from_utf16_lossy(&key)),
                );
            }
        }
        (_, "opaque") => {
            assert_eq!(
                tag(v).as_deref(),
                e["tag"].as_str(),
                "CRYPTOCOM-WRITE-NATIVE-REFERENCE {label}"
            );
            let class = match &**v {
                N::Opaque { class, .. } => class.as_str(),
                _ => "object",
            };
            assert_eq!(
                class,
                e["classification"].as_str().unwrap(),
                "CRYPTOCOM-WRITE-NATIVE-CLASS {label}"
            );
        }
        _ => panic!("CRYPTOCOM-WRITE-NATIVE-KIND {label}: {v:?} vs {e}"),
    }
}
struct Prepared {
    receiver: V,
    key: &'static str,
}
struct IteratorValue {
    values: V,
    index: usize,
    custom: bool,
}
#[derive(Clone)]
struct Host(Rc<State>);
impl std::ops::Deref for Host {
    type Target = State;
    fn deref(&self) -> &State {
        &self.0
    }
}
struct State {
    spec: Json,
    source: Json,
    root: V,
    store: V,
    markets: V,
    market: V,
    xhr: V,
    socket: V,
    live_socket: RefCell<V>,
    opts: V,
    order: V,
    orders: V,
    alternate_update: V,
    caller: V,
    response: V,
    order_id: V,
    callback: V,
    listener_data: V,
    listener_return: V,
    arg: V,
    phase: E,
    post_error: E,
    call_error: E,
    callback_error: E,
    close_error: E,
    type_error: E,
    constructed: RefCell<Option<(E, V)>>,
    events: RefCell<Vec<Vec<u16>>>,
    posts: RefCell<Vec<(String, V)>>,
    logs: RefCell<Vec<(V, V)>>,
    calls: RefCell<Vec<(&'static str, V)>>,
    callbacks: RefCell<Vec<V>>,
    callback_returns: RefCell<Vec<V>>,
    delegated: RefCell<Vec<(&'static str, V, V)>>,
    conversion_index: Cell<usize>,
    template_index: Cell<usize>,
    price_reads: Cell<usize>,
    type_reads: Cell<usize>,
    update_reads: Cell<usize>,
    jobs: RefCell<Vec<Rc<Job>>>,
    registry: RefCell<HashMap<String, V>>,
}
impl Host {
    fn event(&self, s: &str) -> Result<(), E> {
        self.events.borrow_mut().push(s.encode_utf16().collect());
        if self.spec["fail"] == s {
            Err(self.phase.clone())
        } else {
            Ok(())
        }
    }
    fn event_key(&self, prefix: &str, key: Vec<u16>) -> Result<(), E> {
        let units = prefix.encode_utf16().chain(key).collect::<Vec<_>>();
        self.events.borrow_mut().push(units);
        Ok(())
    }
    fn empty_tag(t: &str) -> V {
        Rc::new(N::Obj {
            tag: Some(t.into()),
            fields: RefCell::new(vec![]),
            prototype: None,
        })
    }
    fn new(spec: Json, source: Json) -> Self {
        let socket = Self::empty_tag("socket");
        let order_id = opaque("opaque:order-id", "object");
        let data = Rc::new(N::Obj {
            tag: Some("data".into()),
            fields: RefCell::new(vec![(
                "order_id".encode_utf16().collect(),
                order_id.clone(),
            )]),
            prototype: None,
        });
        let response = Rc::new(N::Obj {
            tag: Some("response".into()),
            fields: RefCell::new(vec![("data".encode_utf16().collect(), data.clone())]),
            prototype: None,
        });
        let opts = input(spec.get("opts").unwrap_or(&serde_json::json!({})), "opts");
        let order = input(spec.get("order").unwrap_or(&serde_json::json!({})), "order");
        let orders = input(
            spec.get("orders").unwrap_or(&serde_json::json!([])),
            "orders",
        );
        let market = input(
            &serde_json::json!({"id":spec.get("marketId").cloned().unwrap_or(serde_json::json!("BTCUSD-PERP"))}),
            "market",
        );
        let caller = input(
            spec.get("caller")
                .unwrap_or(&serde_json::json!({"$undefined":true})),
            "caller",
        );
        let listener_return = spec
            .get("listenerReturn")
            .map(|v| input(v, "opaque:listener-return"))
            .unwrap_or_else(|| opaque("opaque:listener-return", "function"));
        let h = Self(Rc::new(State {
            spec: spec.clone(),
            source,
            root: Self::empty_tag("root"),
            store: Self::empty_tag("store"),
            markets: Self::empty_tag("markets"),
            market,
            xhr: Self::empty_tag("xhr"),
            socket: socket.clone(),
            live_socket: RefCell::new(socket),
            opts,
            order,
            orders,
            alternate_update: input(
                &serde_json::json!({"amount":5,"price":15}),
                "opts.update-alt",
            ),
            caller,
            response,
            order_id,
            callback: opaque("opaque:callback", "function"),
            listener_data: input(&serde_json::json!({"value":42}), "listener-data"),
            listener_return,
            arg: input(spec.get("arg").unwrap_or(&Json::Null), "listener-arg"),
            phase: opaque("error:phase", "object"),
            post_error: opaque("error:post", "object"),
            call_error: opaque("error:call", "object"),
            callback_error: opaque("error:callback", "object"),
            close_error: opaque("error:close", "object"),
            type_error: opaque("error:type", "object"),
            constructed: RefCell::new(None),
            events: RefCell::new(vec![]),
            posts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            calls: RefCell::new(vec![]),
            callbacks: RefCell::new(vec![]),
            callback_returns: RefCell::new(vec![]),
            delegated: RefCell::new(vec![]),
            conversion_index: Cell::new(0),
            template_index: Cell::new(0),
            price_reads: Cell::new(0),
            type_reads: Cell::new(0),
            update_reads: Cell::new(0),
            jobs: RefCell::new(vec![]),
            registry: RefCell::new(HashMap::new()),
        }));
        for v in [
            &h.opts,
            &h.order,
            &h.orders,
            &h.caller,
            &h.order_id,
            &h.callback,
            &h.listener_data,
            &h.listener_return,
            &h.arg,
            &h.phase,
            &h.post_error,
            &h.call_error,
            &h.callback_error,
            &h.close_error,
            &h.type_error,
        ] {
            h.register(v);
        }
        h
    }
    fn register(&self, v: &V) {
        if let Some(t) = tag(v) {
            self.registry.borrow_mut().insert(t, v.clone());
        }
        match &**v {
            N::Obj {
                fields, prototype, ..
            } => {
                let values = fields
                    .borrow()
                    .iter()
                    .map(|(_, v)| v.clone())
                    .collect::<Vec<_>>();
                for v in values {
                    self.register(&v);
                }
                if let Some(v) = prototype {
                    self.register(v);
                }
            }
            N::Arr { values, .. } => {
                let values = values.borrow().clone();
                for v in values {
                    self.register(&v);
                }
            }
            _ => {}
        }
    }
    fn observed_to_value(&self, v: &Json) -> V {
        match v["kind"].as_str().unwrap() {
            "undefined" => undefined(),
            "null" => Rc::new(N::Null),
            "bool" => Rc::new(N::Bool(v["value"].as_bool().unwrap())),
            "number" => Rc::new(N::Num(f64::from_bits(
                u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap(),
            ))),
            "string" => utf16(units(&v["units"])),
            "opaque" => {
                let t = v["tag"].as_str().unwrap();
                if let Some(v) = self.registry.borrow().get(t) {
                    return v.clone();
                }
                let v = opaque(t, v["classification"].as_str().unwrap());
                self.register(&v);
                v
            }
            _ => panic!("native dependency observation"),
        }
    }
    fn raw_get(&self, v: &V, key: &str) -> V {
        let key = key.encode_utf16().collect::<Vec<_>>();
        self.raw_key(v, &key)
    }
    fn raw_key(&self, v: &V, key: &[u16]) -> V {
        match &**v {
            N::Obj {
                fields, prototype, ..
            } => {
                let found = fields
                    .borrow()
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone());
                found.unwrap_or_else(|| {
                    prototype
                        .as_ref()
                        .map(|v| self.raw_key(v, key))
                        .unwrap_or_else(undefined)
                })
            }
            _ => undefined(),
        }
    }
    fn rows(&self, v: &V) -> Result<Vec<V>, E> {
        if let N::Arr { values, .. } = &**v {
            Ok(values.borrow().clone())
        } else {
            Err(self.type_error.clone())
        }
    }
    fn resolve(&self, job: &Job) -> Result<V, E> {
        if let Some(result) = job.result.borrow().clone() {
            return result;
        }
        let result = if let Some(index) = job.call_index {
            self.event(&format!("call-resolve:{index}"))?;
            if self.spec["appendWhileWaiting"] == true && index == 0 {
                self.event("append-order")?;
                let v = input(&self.spec["orders"][0], "orders.1");
                self.register(&v);
                if let N::Arr { values, .. } = &*self.orders {
                    values.borrow_mut().push(v);
                }
            }
            if self.spec["rejectCalls"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v.as_u64() == Some(index as u64)))
            {
                Err(self.call_error.clone())
            } else {
                Ok(job.returned.clone())
            }
        } else {
            self.event("post-resolve")?;
            if self.spec["postReject"] == true {
                Err(self.post_error.clone())
            } else {
                Ok(self.response.clone())
            }
        };
        *job.result.borrow_mut() = Some(result.clone());
        result
    }
    fn background(&self) {
        let jobs = self.jobs.borrow().clone();
        for job in jobs {
            let _ = self.resolve(&job);
        }
    }
}
enum SeriesSlot {
    Done,
    Hole,
    Value(V),
}
impl Host {
    fn advance_series(&self, orders: &V, mut index: usize) -> Result<(usize, SeriesSlot), E> {
        loop {
            let slot = self.series_slot(orders, index)?;
            if matches!(slot, SeriesSlot::Hole) {
                index += 1;
            } else {
                return Ok((index, slot));
            }
        }
    }
    fn series_slot(&self, orders: &V, index: usize) -> Result<SeriesSlot, E> {
        let values = self.rows(orders)?;
        if index >= values.len() {
            return Ok(SeriesSlot::Done);
        }
        self.event(&format!("has:orders:{index}"))?;
        let hole = self.spec["holes"]
            .as_array()
            .is_some_and(|v| v.iter().any(|v| v.as_u64() == Some(index as u64)));
        let value = if hole {
            if !self.spec["inheritedSlots"].is_object() {
                return Ok(SeriesSlot::Hole);
            }
            self.event(&format!("has:orders.prototype:{index}"))?;
            let Some(value) = self.spec["inheritedSlots"].get(index.to_string()) else {
                return Ok(SeriesSlot::Hole);
            };
            self.event(&format!("get:orders.prototype:{index}"))?;
            let value = input(value, &format!("orders.prototype.{index}"));
            self.register(&value);
            value
        } else {
            values[index].clone()
        };
        if matches!(&*value, N::Obj { .. }) {
            let _ = self.get(&value, "then")?;
        }
        Ok(SeriesSlot::Value(value))
    }
}
struct Job {
    call_index: Option<usize>,
    returned: V,
    result: RefCell<Option<Result<V, E>>>,
}
struct NativeFuture {
    host: Host,
    job: Rc<Job>,
}
impl Future for NativeFuture {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.host.resolve(&self.job))
    }
}
impl WriteHost for Host {
    type Value = V;
    type Error = E;
    type Prepared = Prepared;
    type Iterator = IteratorValue;
    type PostFuture = NativeFuture;
    type CallFuture = NativeFuture;
    type SeriesFuture = BoxFuture<V, E>;
    fn root(&self) -> V {
        self.root.clone()
    }
    fn undefined(&self) -> V {
        undefined()
    }
    fn string(&self, s: &'static str) -> V {
        text(s)
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        Rc::new(N::Obj {
            tag: None,
            fields: RefCell::new(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.encode_utf16().collect(), v))
                    .collect(),
            ),
            prototype: None,
        })
    }
    fn array(&self, values: Vec<V>) -> V {
        Rc::new(N::Arr {
            tag: None,
            values: RefCell::new(values),
        })
    }
    fn get(&self, v: &V, key: &'static str) -> Result<V, E> {
        if self.nullish(v) {
            return Err(self.type_error.clone());
        }
        if let Some(t) = tag(v) {
            self.event(&format!("get:{t}:{key}"))?;
            if t == "root" {
                return Ok(match key {
                    "store" => self.store.clone(),
                    "xhr" => self.xhr.clone(),
                    "publicWebsocket" => self.live_socket.borrow().clone(),
                    _ => undefined(),
                });
            }
            if t == "store" && key == "marketsBySymbol" {
                return Ok(self.markets.clone());
            }
            if t == "opts" {
                if key == "price" && self.spec["changePrice"] == true {
                    let n = self.price_reads.get();
                    self.price_reads.set(n + 1);
                    return Ok(Rc::new(N::Num(if n == 0 { 10.0 } else { 0.0 })));
                }
                if key == "type" && self.spec["changeType"] == true {
                    let n = self.type_reads.get();
                    self.type_reads.set(n + 1);
                    return Ok(text(
                        ["limit", "stop_market", "take_profit_market", "limit"][n.min(3)],
                    ));
                }
                if key == "update" && self.spec["changeUpdate"] == true {
                    let n = self.update_reads.get();
                    self.update_reads.set(n + 1);
                    if n > 0 {
                        return Ok(self.alternate_update.clone());
                    }
                }
            }
        }
        if let N::Obj {
            fields, prototype, ..
        } = &**v
        {
            let found = fields
                .borrow()
                .iter()
                .find(|(k, _)| k.iter().copied().eq(key.encode_utf16()))
                .map(|(_, v)| v.clone());
            if let Some(v) = found {
                return Ok(v);
            }
            if let Some(proto) = prototype {
                return self.get(proto, key);
            }
        }
        Ok(undefined())
    }
    fn get_key(&self, v: V, key: V) -> Result<V, E> {
        assert!(Rc::ptr_eq(&v, &self.markets));
        self.event_key("get:markets:", primitive_text(&key))?;
        Ok(if self.spec["missingMarket"] == true {
            undefined()
        } else {
            self.market.clone()
        })
    }
    fn assign(&self, v: &V, key: &'static str, value: V) -> Result<(), E> {
        if let N::Obj { fields, .. } = &**v {
            let key = key.encode_utf16().collect::<Vec<_>>();
            let mut fields = fields.borrow_mut();
            if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == key) {
                *v = value
            } else {
                fields.push((key, value))
            };
            Ok(())
        } else {
            Err(self.type_error.clone())
        }
    }
    fn has(&self, v: V, key: &'static str) -> Result<bool, E> {
        if let Some(t) = tag(&v) {
            self.event(&format!("has:{t}:{key}"))?;
        }
        if !matches!(&*v, N::Obj { .. }) {
            return Err(self.type_error.clone());
        }
        if let N::Obj {
            fields, prototype, ..
        } = &*v
        {
            let own = fields
                .borrow()
                .iter()
                .any(|(k, _)| k.iter().copied().eq(key.encode_utf16()));
            if own {
                return Ok(true);
            }
            if let Some(proto) = prototype {
                return self.has(proto.clone(), key);
            }
        }
        Ok(false)
    }

    fn truthy(&self, v: &V) -> bool {
        match &**v {
            N::Undefined | N::Null => false,
            N::Bool(v) => *v,
            N::Num(v) => *v != 0.0 && !v.is_nan(),
            N::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(&**v, N::Undefined | N::Null)
    }
    fn is_undefined(&self, v: &V) -> bool {
        matches!(&**v, N::Undefined)
    }
    fn equal_string(&self, v: &V, s: &'static str) -> bool {
        matches!(&**v,N::Text(v)if v.iter().copied().eq(s.encode_utf16()))
    }
    fn prepare(&self, v: &V, key: &'static str) -> Result<Prepared, E> {
        if self.nullish(v) {
            return Err(self.type_error.clone());
        }
        self.event(&format!("prepare:{key}"))?;
        if key == "cancelOrder" && self.spec["failCancelGet"] == true
            || key == "placeOrder" && self.spec["failPlaceGet"] == true
        {
            return Err(self.phase.clone());
        }
        if key.starts_with("listen") && self.spec["replaceSocket"] == true {
            *self.live_socket.borrow_mut() = Self::empty_tag("replacement");
        }
        Ok(Prepared {
            receiver: v.clone(),
            key,
        })
    }
    fn call(&self, p: Prepared, args: Vec<V>) -> Result<V, E> {
        if !matches!(p.key, "log" | "listenOHLCV" | "listenOrderBook") {
            self.event(&format!("call:{}", p.key))?;
        }
        match p.key {
            "resolveCallerClientOrderId" => {
                assert!(Rc::ptr_eq(&p.receiver, &self.root));
                assert!(Rc::ptr_eq(&args[0], &self.opts));
                Ok(self.caller.clone())
            }
            "assertCallerClientOrderIds" => {
                assert!(Rc::ptr_eq(&p.receiver, &self.root));
                assert!(Rc::ptr_eq(&args[0], &self.orders));
                Ok(undefined())
            }
            "toString" => {
                let n = self.conversion_index.get();
                self.conversion_index.set(n + 1);
                let c = &self.source["conversions"][n];
                assert_native(&p.receiver, &c["input"], "toString.input");
                if let N::CustomString { returned, .. } = &*p.receiver {
                    let _ = returned;
                }
                Ok(self.observed_to_value(&c["out"]))
            }
            "mapOrderType" | "mapTimeInForce" | "mapStopTriggerType" => {
                assert!(Rc::ptr_eq(&p.receiver, &self.root));
                let arg = if let N::Text(v) = &*args[0] {
                    Some(v.as_slice())
                } else {
                    None
                };
                let value = match p.key {
                    "mapOrderType" => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_order_type(arg),
                    )
                    .unwrap(),
                    "mapTimeInForce" => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_time_in_force(arg),
                    )
                    .unwrap(),
                    _ => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_stop_trigger_type(arg),
                    )
                    .unwrap(),
                };
                Ok(text(value.as_str().unwrap()))
            }
            "log" => {
                assert!(Rc::ptr_eq(&p.receiver, &self.root));
                self.event("log")?;
                self.logs
                    .borrow_mut()
                    .push((args[0].clone(), args[1].clone()));
                Ok(undefined())
            }
            "push" => {
                if let N::Arr { values, .. } = &*p.receiver {
                    values.borrow_mut().extend(args);
                    Ok(Rc::new(N::Num(values.borrow().len() as f64)))
                } else {
                    Err(self.type_error.clone())
                }
            }
            "listenOHLCV" | "listenOrderBook" => {
                assert!(
                    Rc::ptr_eq(&p.receiver, &self.socket),
                    "CRYPTOCOM-WRITE-NATIVE-LISTENER-RECEIVER"
                );
                self.event("listener-call")?;
                self.delegated
                    .borrow_mut()
                    .push((p.key, args[0].clone(), args[1].clone()));
                assert!(
                    Rc::ptr_eq(&args[1], &self.callback),
                    "CRYPTOCOM-WRITE-NATIVE-CALLBACK-IDENTITY"
                );
                self.event("callback")?;
                self.callbacks.borrow_mut().push(self.listener_data.clone());
                if self.spec["callbackThrows"] == true {
                    Err(self.callback_error.clone())
                } else {
                    Ok(self.listener_return.clone())
                }
            }
            _ => panic!("unexpected native call {}", p.key),
        }
    }
    fn call_async(&self, p: Prepared, args: Vec<V>) -> Result<NativeFuture, E> {
        assert!(Rc::ptr_eq(&p.receiver, &self.root));
        let index = self.calls.borrow().len();
        self.calls.borrow_mut().push((p.key, args[0].clone()));
        self.event(&format!("call:{}", p.key))?;
        let returned = if p.key == "cancelOrder" {
            opaque("opaque:ignored-cancel", "object")
        } else {
            input(
                self.spec["returns"]
                    .get(index)
                    .unwrap_or(&serde_json::json!([format!("ID{index}")])),
                &format!("returned.{index}"),
            )
        };
        self.register(&returned);
        let job = Rc::new(Job {
            call_index: Some(index),
            returned,
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        Ok(NativeFuture {
            host: self.clone(),
            job,
        })
    }
    fn endpoint(&self, key: &'static str) -> Result<V, E> {
        self.event(&format!("endpoint:{key}"))?;
        Ok(text(key))
    }
    fn post(&self, p: Prepared, endpoint: V, params: V) -> Result<NativeFuture, E> {
        assert_eq!(p.key, "post");
        assert!(Rc::ptr_eq(&p.receiver, &self.xhr));
        self.posts
            .borrow_mut()
            .push((String::from_utf16_lossy(&primitive_text(&endpoint)), params));
        self.event("post")?;
        let job = Rc::new(Job {
            call_index: None,
            returned: self.response.clone(),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        Ok(NativeFuture {
            host: self.clone(),
            job,
        })
    }
    fn is_finite(&self, v: V) -> Result<bool, E> {
        self.event("finite")?;
        Ok(matches!(&*v,N::Num(n)if n.is_finite()))
    }
    fn less_equal_zero(&self, v: V) -> Result<bool, E> {
        Ok(matches!(&*v,N::Num(n)if *n<=0.0))
    }
    fn prepare_error(&self) -> Result<Prepared, E> {
        self.event("prepare-error")?;
        Ok(Prepared {
            receiver: self.root.clone(),
            key: "Error",
        })
    }
    fn template(&self, prefix: &'static str, v: V) -> Result<V, E> {
        let suffix = if tag(&v).is_some_and(|s| s.starts_with("error:")) {
            self.event("error-string")?;
            let n = self.template_index.get();
            self.template_index.set(n + 1);
            let c = &self.source["templates"][n];
            assert_native(&v, &c["input"], "template.original-error");
            units(&c["out"]["units"])
        } else {
            primitive_text(&v)
        };
        Ok(utf16(prefix.encode_utf16().chain(suffix).collect()))
    }
    fn construct_error(&self, p: Prepared, message: V) -> Result<E, E> {
        assert_eq!(p.key, "Error");
        self.event("construct-error")?;
        let error = opaque("error:constructed", "object");
        self.register(&error);
        *self.constructed.borrow_mut() = Some((error.clone(), message));
        Ok(error)
    }
    fn error_value(&self, e: &E) -> V {
        e.clone()
    }
    fn severity_error(&self) -> Result<V, E> {
        Ok(text(self.source["_severity"].as_str().unwrap()))
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        if let Some(t) = tag(&v) {
            self.event(&format!("iterator:{t}"))?;
        }
        self.rows(&v)
    }
    fn iterator(&self, v: V) -> Result<IteratorValue, E> {
        if let Some(t) = tag(&v) {
            self.event(&format!("iterator:{t}"))?;
        }
        Ok(IteratorValue {
            values: v,
            index: 0,
            custom: self.spec["customIterator"] == true,
        })
    }
    fn next(&self, i: &mut IteratorValue) -> Result<Option<V>, E> {
        if i.custom {
            self.event(&format!("next:{}", i.index))?;
        }
        let value = if let N::Arr { values, .. } = &*i.values {
            values.borrow().get(i.index).cloned()
        } else {
            return Err(self.type_error.clone());
        };
        if value.is_some() {
            i.index += 1;
        }
        Ok(value)
    }
    fn close_throw(&self, i: IteratorValue, e: E) -> E {
        if i.custom {
            let _ = self.event("close");
            if self.spec["closeThrow"] == true {
                let _ = &self.close_error;
            }
        }
        e
    }
    fn series<F>(&self, orders: V, mut callback: F) -> Result<BoxFuture<V, E>, E>
    where
        F: FnMut(V) -> BoxFuture<V, E> + 'static,
    {
        self.event("series")?;
        if self.spec["customSeries"] == true {
            let original = self.rows(&orders)?[0].clone();
            self.event("series-callback:0")?;
            let first = callback(original.clone());
            let h = self.clone();
            return Ok(Box::pin(async move {
                let v = first.await?;
                h.callback_returns.borrow_mut().push(v);
                h.event("series-callback:1")?;
                let v = callback(original).await?;
                h.callback_returns.borrow_mut().push(v);
                Ok(opaque("opaque:series-return", "object"))
            }));
        }
        let h = self.clone();
        let (first_index, first) = h.advance_series(&orders, 0)?;
        Ok(Box::pin(async move {
            let mut index = first_index;
            let mut slot = first;
            loop {
                match slot {
                    SeriesSlot::Done => return Ok(undefined()),
                    SeriesSlot::Hole => {}
                    SeriesSlot::Value(value) => {
                        h.event(&format!("series-callback:{index}"))?;
                        let returned = callback(value).await?;
                        h.callback_returns.borrow_mut().push(returned);
                    }
                }
                let next = h.advance_series(&orders, index + 1)?;
                index = next.0;
                slot = next.1;
            }
        }))
    }
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("bounded native driver pending"),
    }
}
#[test]
fn order_writes_match_actual_typescript_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_writes_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").expect("compiler required"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Json = serde_json::from_slice(&output.stdout).unwrap();
    let specs: Vec<Json> =
        serde_json::from_str(include_str!("order_writes_fixtures.json")).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    let mut count = 0;
    for (spec, source) in specs.iter().zip(rows) {
        let name = spec["name"].as_str().unwrap();
        let mut source = source.clone();
        source["_severity"] = oracle["severity"].clone();
        let h = Host::new(spec.clone(), source.clone());
        let host = Rc::new(h.clone());
        let immediate;
        macro_rules! run {
            ($future:expr) => {{
                let future = $future;
                immediate = h.events.borrow().clone();
                drive(future)
            }};
        }
        let op = spec["op"].as_str().unwrap();
        let result = match op {
            "place" => run!(b::place_order(host, h.opts.clone())),
            "cancel" => run!(b::cancel_order(host, h.order.clone())),
            "symbol" => run!(b::cancel_symbol_orders(
                host,
                input(&spec["symbol"], "symbol")
            )),
            "update" => run!(b::update_order(host, h.opts.clone())),
            "cancelBatch" => run!(b::cancel_orders(host, h.orders.clone())),
            "placeBatch" => run!(b::place_orders(host, h.orders.clone())),
            "ohlcv" => {
                let result = b::listen_ohlcv(&h, h.arg.clone(), h.callback.clone());
                immediate = h.events.borrow().clone();
                result
            }
            "book" => {
                let result = b::listen_order_book(&h, h.arg.clone(), h.callback.clone());
                immediate = h.events.borrow().clone();
                result
            }
            _ => panic!("fixture op"),
        };
        h.background();
        match result {
            Ok(value) => {
                assert!(
                    source["error"].is_null(),
                    "CRYPTOCOM-WRITE-NATIVE-ERROR {name} unexpected success"
                );
                assert_native(&value, &source["result"], name);
                if op == "place" || op == "update" {
                    let values = h.rows(&value).unwrap();
                    assert!(
                        Rc::ptr_eq(&values[0], &h.order_id),
                        "CRYPTOCOM-WRITE-NATIVE-ORDER-ID-IDENTITY {name}"
                    );
                }
                if op == "ohlcv" || op == "book" {
                    assert!(
                        Rc::ptr_eq(&value, &h.listener_return),
                        "CRYPTOCOM-WRITE-NATIVE-LISTENER-RETURN-IDENTITY {name}"
                    );
                }
            }
            Err(error) => {
                assert!(
                    !source["error"].is_null(),
                    "CRYPTOCOM-WRITE-NATIVE-ERROR {name} unexpected Rust error {error:?}"
                );
                assert_native(&error, &source["error"], &format!("{name}.error"));
                let expected = h
                    .registry
                    .borrow()
                    .get(source["error"]["tag"].as_str().unwrap())
                    .unwrap()
                    .clone();
                assert!(
                    Rc::ptr_eq(&error, &expected),
                    "CRYPTOCOM-WRITE-NATIVE-ERROR-IDENTITY {name}"
                );
            }
        }
        let posts = h.posts.borrow();
        assert_eq!(
            posts.len(),
            usize::try_from(spec["expectedPosts"].as_u64().unwrap()).unwrap(),
            "CRYPTOCOM-WRITE-NATIVE-POST-COUNT {name}"
        );
        for ((key, params), expected) in posts.iter().zip(source["posts"].as_array().unwrap()) {
            assert_eq!(
                key,
                expected["key"].as_str().unwrap(),
                "CRYPTOCOM-WRITE-NATIVE-ENDPOINT {name}"
            );
            assert_native(params, &expected["params"], &format!("{name}.request"));
            if op == "place" && !h.is_undefined(&h.caller) {
                assert!(
                    Rc::ptr_eq(&h.raw_get(params, "client_oid"), &h.caller),
                    "CRYPTOCOM-WRITE-NATIVE-CALLER-ID-IDENTITY {name}"
                );
            }
        }
        let constructed = h.constructed.borrow();
        if let Some((error, message)) = &*constructed {
            assert_native(
                error,
                &source["constructed"]["value"],
                &format!("{name}.constructed"),
            );
            assert_eq!(
                primitive_text(message),
                units(&source["constructed"]["message"]),
                "CRYPTOCOM-WRITE-NATIVE-CONSTRUCTED-MESSAGE {name}"
            );
        } else {
            assert!(source["constructed"].is_null());
        }
        let logs = h.logs.borrow();
        assert_eq!(
            logs.len(),
            source["logs"].as_array().unwrap().len(),
            "CRYPTOCOM-WRITE-NATIVE-LOG-COUNT {name}"
        );
        for ((message, severity), expected) in logs.iter().zip(source["logs"].as_array().unwrap()) {
            assert_eq!(
                primitive_text(message),
                units(&expected["message"]),
                "CRYPTOCOM-WRITE-NATIVE-LOG {name}"
            );
            assert_eq!(
                primitive_text(severity),
                expected["severity"]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .collect::<Vec<_>>()
            );
        }
        let calls = h.calls.borrow();
        if let Some(expected) = spec["expectedCalls"].as_u64() {
            assert_eq!(
                calls.len(),
                expected as usize,
                "CRYPTOCOM-WRITE-NATIVE-CALL-COUNT {name}"
            );
        }
        for ((method, arg), expected) in calls.iter().zip(source["calls"].as_array().unwrap()) {
            assert_eq!(*method, expected["name"].as_str().unwrap());
            assert_native(arg, &expected["arg"], &format!("{name}.call-arg"));
            let original = h
                .registry
                .borrow()
                .get(expected["arg"]["tag"].as_str().unwrap())
                .unwrap()
                .clone();
            assert!(
                Rc::ptr_eq(arg, &original),
                "CRYPTOCOM-WRITE-NATIVE-CALL-ARG-IDENTITY {name}"
            );
        }
        let callbacks = h.callbacks.borrow();
        if let Some(expected) = spec["expectedCallbacks"].as_u64() {
            assert_eq!(
                callbacks.len(),
                expected as usize,
                "CRYPTOCOM-WRITE-NATIVE-CALLBACK-COUNT {name}"
            );
        }
        for (actual, expected) in callbacks
            .iter()
            .zip(source["callbacks"].as_array().unwrap())
        {
            assert_native(actual, expected, &format!("{name}.callback"));
            assert!(Rc::ptr_eq(actual, &h.listener_data));
        }
        let delegated = h.delegated.borrow();
        for ((method, arg, cb), expected) in delegated
            .iter()
            .zip(source["delegated"].as_array().unwrap())
        {
            assert_eq!(*method, expected["name"].as_str().unwrap());
            assert_native(arg, &expected["arg"], &format!("{name}.arg"));
            assert_native(
                cb,
                &expected["callback"],
                &format!("{name}.callback-function"),
            );
            assert!(
                Rc::ptr_eq(arg, &h.arg),
                "CRYPTOCOM-WRITE-NATIVE-DELEGATE-ARG-IDENTITY {name}"
            );
            assert!(Rc::ptr_eq(cb, &h.callback));
        }
        let returned = h.callback_returns.borrow();
        assert_eq!(
            returned.len(),
            source["callbackReturns"].as_array().unwrap().len()
        );
        for (actual, expected) in returned
            .iter()
            .zip(source["callbackReturns"].as_array().unwrap())
        {
            assert_native(actual, expected, &format!("{name}.callback-return"));
        }
        // Direct native values, identities and independent counts precede event/inventory checks.
        let trace = |v: &Json| v.as_array().unwrap().iter().map(units).collect::<Vec<_>>();
        assert_eq!(
            immediate,
            trace(&source["immediate"]),
            "CRYPTOCOM-WRITE-NATIVE-EAGER {name}"
        );
        assert_eq!(
            *h.events.borrow(),
            trace(&source["events"]),
            "CRYPTOCOM-WRITE-NATIVE-EFFECTS {name}"
        );
        assert_eq!(
            h.conversion_index.get(),
            source["conversions"].as_array().unwrap().len()
        );
        assert_eq!(
            h.template_index.get(),
            source["templates"].as_array().unwrap().len()
        );
        count += 1;
    }
    assert_eq!(specs.len(), 91, "WRITE independent fixture floor");
    assert_eq!(rows.len(), 91, "WRITE source oracle floor");
    assert_eq!(count, 91, "WRITE native measured floor");
    assert_eq!(specs[90]["name"], "listen-book-primitive-return");
    eprintln!("WRITES native floor91 stops index90 listen-book-primitive-return");
}
