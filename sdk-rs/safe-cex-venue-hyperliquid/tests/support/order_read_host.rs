use super::{
    account,
    cache::{self, N, arr, eq, object, s},
};
use safe_cex_venue_hyperliquid::{
    cross_cache::CrossCacheHost,
    helper_tasks::HelperTaskHost,
    order_reads::{self, OrderReadCallback, OrderReadHost},
    spot_account_reads::*,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};
#[derive(Clone)]
pub struct Host {
    pub base: account::Host,
    pub st: Rc<RefCell<State>>,
}
pub struct State {
    pub scene: String,
    pub family: String,
    pub effects: Vec<N>,
    pub callbacks: HashMap<String, OrderReadCallback<N>>,
    pub promises: Vec<N>,
    pub posted: Vec<Vec<N>>,
    pub marker: N,
    pub final_error: N,
    pub core: N,
    pub dex: N,
    pub initial: N,
    pub swapped: N,
    pub fetch_result: N,
    pub flight: N,
    pub replacement: N,
    pub fetch_calls: usize,
    pub closes: usize,
    pub sets: usize,
    pub settlements: Vec<Result<N, N>>,
    pub flight_reads: usize,
    pub cancel_iterator: N,
    pub collected: N,
}
impl Host {
    pub fn e(&self) -> N {
        self.base.exchange.clone()
    }
    pub fn record(&self, k: &str, args: &[N]) {
        let mut v = vec![s(k)];
        v.extend_from_slice(args);
        self.st.borrow_mut().effects.push(arr(v))
    }
    pub fn scene(&self) -> String {
        self.st.borrow().scene.clone()
    }
    pub fn ready(&self, r: Result<N, N>) -> N {
        let v = self.base.spawn_native(Box::pin(std::future::ready(r)));
        self.st.borrow_mut().promises.push(v.clone());
        v
    }
    pub fn put(&mut self, k: &str, v: &N) {
        self.base.set(&self.e(), k, v).unwrap()
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, t: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(t)
    }
    fn settle_return(&self, r: Result<N, N>) -> AccountFuture<N, N> {
        let f = self.base.settle_return(r);
        let st = self.st.clone();
        Box::pin(async move {
            let r = f.await;
            st.borrow_mut().settlements.push(r.clone());
            r
        })
    }
}
impl OrderReadHost for Host {
    fn order_read_callback(&mut self, c: OrderReadCallback<N>) -> Result<N, N> {
        let mut st = self.st.borrow_mut();
        let id = format!("read-callback-{}", st.callbacks.len());
        st.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let values = match v {
            N::A(v) | N::Set(v) => v.borrow().clone(),
            N::Map(v) => v
                .borrow()
                .iter()
                .map(|(k, v)| arr(vec![k.clone(), v.clone()]))
                .collect(),
            _ => panic!("native iterator binding {v:?}"),
        };
        let iterator = N::Iter(Rc::new(RefCell::new((values, 0))));
        if eq(v, &account::get(&self.e(), "_cancelledOrders")) {
            self.st.borrow_mut().cancel_iterator = iterator.clone()
        }
        Ok(iterator)
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        if self.scene() == "cancel-next-error"
            && matches!((v,&self.st.borrow().cancel_iterator),(N::Iter(a),N::Iter(b))if Rc::ptr_eq(a,b))
        {
            return Err(self.st.borrow().marker.clone());
        }
        let N::Iter(i) = v else { panic!("next") };
        let mut i = i.borrow_mut();
        let value = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(value)
    }
    fn close_after_throw(&mut self, _: &N) {
        self.st.borrow_mut().closes += 1;
        self.record("close", &[])
    }
    fn first_binding(&mut self, v: &N) -> Result<N, N> {
        if self.scene() == "cancel-binding-error" {
            return Err(self.st.borrow().marker.clone());
        }
        let N::A(v) = v else { panic!("native binding") };
        Ok(v.borrow().first().cloned().unwrap_or(N::U))
    }
    fn typeof_value(&self, v: &N) -> &'static str {
        match v {
            N::U => "undefined",
            N::B(_) => "boolean",
            N::Num(_) => "number",
            N::S(_) => "string",
            N::Fn(_) | N::Builtin(_) => "function",
            _ => "object",
        }
    }
    fn argument_spread(&mut self, out: &mut Vec<N>, v: &N) -> Result<(), N> {
        let N::A(v) = v else {
            panic!("argument spread")
        };
        out.extend(v.borrow().clone());
        Ok(())
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        WsPingHost::undefined(&self.base)
    }

    fn literal(&self, t: &JsString) -> N {
        WsPingHost::literal(&self.base, t)
    }

    fn truthy(&self, v: &N) -> bool {
        WsPingHost::truthy(&self.base, v)
    }

    fn global(&mut self, k: &str) -> Result<N, N> {
        if k == "Set" || k == "Map" {
            return Ok(N::Builtin(k.into()));
        }
        self.base.global(k)
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.e()) && k == "_fetchOrdersInFlight" {
            let mut st = self.st.borrow_mut();
            st.flight_reads += 1;
            if st.scene == "finally-get-error" && st.flight_reads == 2 {
                return Err(st.final_error.clone());
            }
            if st.scene == "changed-inflight" && st.flight_reads == 2 {
                return Ok(st.replacement.clone());
            }
        }
        if self.st.borrow().promises.iter().any(|p| eq(p, v)) && matches!(k, "then" | "catch") {
            return Ok(N::Builtin(format!("promise-{k}")));
        }
        if matches!(v, N::S(_)) && k == "toLowerCase" {
            return Ok(N::Builtin("toLowerCase".into()));
        }
        if matches!(v, N::A(_))
            && !matches!(k, "length" | "map" | "push")
            && k.parse::<usize>().is_err()
        {
            return Ok(N::U);
        }
        self.base.get(v, k)
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        if eq(v, &self.e()) && k == "_fetchOrdersInFlight" {
            self.st.borrow_mut().sets += 1;
            if self.scene() == "finally-set-error" && matches!(value, N::Null) {
                return Err(self.st.borrow().final_error.clone());
            }
        }
        self.base.set(v, k, value)
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::define_data(&mut self.base, v, k, value)
    }

    fn object(&mut self) -> Result<N, N> {
        WsPingHost::object(&mut self.base)
    }

    fn callback(&mut self, capture: PingCallback<N>) -> Result<N, N> {
        WsPingHost::callback(&mut self.base, capture)
    }

    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("callee {f:?}")
        };
        let cb = self.st.borrow().callbacks.get(id).cloned();
        if let Some(c) = cb {
            return order_reads::run_order_read_callback(self, &c, args);
        }
        let first = args.first().cloned().unwrap_or(N::U);
        let scene = self.scene();
        match id.as_str() {
            "promise-then" | "promise-catch" => {
                let mut h = self.clone();
                let original = r.clone();
                let catch = id == "promise-catch";
                let out = self.base.spawn_native(Box::pin(async move {
                    match h.await_value(original).await {
                        Ok(v) => {
                            if catch {
                                Ok(v)
                            } else {
                                h.call(&first, &N::U, &[v])
                            }
                        }
                        Err(e) => {
                            if catch {
                                h.call(&first, &N::U, &[e])
                            } else {
                                Err(e)
                            }
                        }
                    }
                }));
                self.st.borrow_mut().promises.push(out.clone());
                return Ok(out);
            }
            "map" => {
                let N::A(a) = r else { panic!("map") };
                let len = a.borrow().len();
                let mut out = vec![];
                for i in 0..len {
                    let value = a.borrow()[i].clone();
                    out.push(self.call(&first, &N::U, &[value, N::Num(i as f64), r.clone()])?)
                }
                return Ok(arr(out));
            }
            "push" => {
                let N::A(a) = r else { panic!("push") };
                a.borrow_mut().extend_from_slice(args);
                let len = a.borrow().len();
                return Ok(N::Num(len as f64));
            }
            "toLowerCase" => {
                let N::S(v) = r else { panic!("lowercase") };
                return Ok(s(&String::from_utf16_lossy(&v.0).to_lowercase()));
            }
            "uniqBy" => {
                let N::A(a) = &first else { panic!("uniqBy") };
                let mut keys = vec![];
                let mut values = vec![];
                for v in a.borrow().iter() {
                    let key = self.call(&args[1], &N::U, std::slice::from_ref(v))?;
                    if !keys.iter().any(|k| eq(k, &key)) {
                        keys.push(key);
                        values.push(v.clone())
                    }
                }
                return Ok(arr(values));
            }
            "post" => {
                self.st.borrow_mut().posted.push(args.to_vec());
                let dex = account::get(&args[1], "dex");
                self.record("post", std::slice::from_ref(&dex));
                if scene == "store-swap" {
                    let store = account::get(&self.e(), "store");
                    let swapped = self.st.borrow().swapped.clone();
                    self.base.set(&store, "orders", &swapped)?
                }
                let fail = (scene == "core-error" && matches!(dex, N::U))
                    || ((scene == "dex-error" || scene == "warning-error")
                        && eq(&dex, &s("DEX-B")));
                let data = if matches!(dex, N::U) {
                    self.st.borrow().core.clone()
                } else if eq(&dex, &s("dex-a")) {
                    self.st.borrow().dex.clone()
                } else {
                    arr(vec![])
                };
                return Ok(self.ready(if fail {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(object(vec![("data", data)]))
                }));
            }
            "parseOpenOrders" => {
                assert!(eq(r, &self.e()), "native parser lexical receiver");
                let is_core = eq(&first, &self.st.borrow().core);
                self.record("parse", &[N::B(is_core)]);
                if scene == "core-parse-error"
                    || (scene == "dex-parse-error" && eq(&first, &self.st.borrow().dex))
                {
                    return Err(self.st.borrow().marker.clone());
                }
                let N::A(a) = &first else { panic!("parse") };
                let mut values = vec![];
                for v in a.borrow().iter() {
                    let row = account::get(v, "row");
                    if !matches!(row, N::U | N::Null) {
                        values.push(row)
                    }
                }
                return Ok(arr(values));
            }
            "log" => {
                assert!(eq(r, &self.e()), "native log lexical receiver");
                self.record("log", args);
                if scene == "logger-error"
                    || (scene == "warning-error" && eq(&args[1], &s("Warning")))
                {
                    return Err(self.st.borrow().marker.clone());
                }
                return Ok(N::U);
            }
            "addOrder" => {
                self.record("add", args);
                let list = account::get(r, "orders");
                let N::A(a) = list else {
                    panic!("store orders")
                };
                a.borrow_mut().push(first);
                return Ok(N::U);
            }
            "updateOrder" => {
                self.record("update", args);
                let list = account::get(r, "orders");
                let N::A(a) = list else {
                    panic!("store orders")
                };
                let id = account::get(&first, "id");
                let current = a
                    .borrow()
                    .iter()
                    .find(|o| eq(&account::get(o, "id"), &id))
                    .cloned()
                    .unwrap();
                let N::O(patch) = &args[1] else {
                    panic!("patch")
                };
                for (k, v, e) in patch.borrow().fields.clone() {
                    if e {
                        self.base.set(&current, &k, &v)?
                    }
                }
                return Ok(N::U);
            }
            "removeOrder" => {
                self.record("remove", args);
                let list = account::get(r, "orders");
                let N::A(a) = list else {
                    panic!("store orders")
                };
                let id = account::get(&first, "id");
                let kept = a
                    .borrow()
                    .iter()
                    .filter(|o| !eq(&account::get(o, "id"), &id))
                    .cloned()
                    .collect();
                self.base.set(r, "orders", &arr(kept))?;
                return Ok(N::U);
            }
            "unsuppressCancelledOrder" => {
                self.record("unsuppress", args);
                let map = account::get(&self.e(), "_cancelledOrders");
                return CrossCacheHost::call(
                    &mut cache::Host::default(),
                    &N::Builtin("delete".into()),
                    &map,
                    args,
                )
                .map_err(|e| s(&e));
            }
            "fetchOrdersInternal" => {
                self.st.borrow_mut().fetch_calls += 1;
                self.record("internal", &[]);
                if scene == "sync-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let v = self.st.borrow().fetch_result.clone();
                if scene == "replacement" {
                    let replacement = self.st.borrow().replacement.clone();
                    let mut host = self.clone();
                    let e = self.e();
                    let promise = self.ready(Ok(v));
                    let waiter = promise.clone();
                    self.base.spawn_native(Box::pin(async move {
                        host.await_value(waiter).await?;
                        host.put("_fetchOrdersInFlight", &replacement);
                        Ok(N::U)
                    }));
                    let _ = e;
                    return Ok(promise);
                }
                return Ok(self.ready(
                    if matches!(
                        scene.as_str(),
                        "reject" | "finally-get-error" | "finally-set-error"
                    ) {
                        Err(self.st.borrow().marker.clone())
                    } else {
                        Ok(v)
                    },
                ));
            }
            "fetchOrders" => {
                self.st.borrow_mut().fetch_calls += 1;
                self.record("fetchOrders", &[]);
                if scene == "sync-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                return Ok(self.ready(if scene == "reject" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().fetch_result.clone())
                }));
            }
            "get" | "add" | "has" | "delete" => {
                return CrossCacheHost::call(&mut cache::Host::default(), f, r, args)
                    .map_err(|e| s(&e));
            }
            _ => {}
        }
        self.base.call(f, r, args)
    }
}
impl SpotAccountHost for Host {
    fn null(&self) -> N {
        SpotAccountHost::null(&self.base)
    }

    fn boolean(&self, v: bool) -> N {
        SpotAccountHost::boolean(&self.base, v)
    }

    fn number(&self, v: f64) -> N {
        SpotAccountHost::number(&self.base, v)
    }

    fn is_nullish(&self, v: &N) -> bool {
        SpotAccountHost::is_nullish(&self.base, v)
    }

    fn is_undefined(&self, v: &N) -> bool {
        SpotAccountHost::is_undefined(&self.base, v)
    }

    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        SpotAccountHost::is_array(&mut self.base, v)
    }

    fn strict_equal(&self, a: &N, b: &N) -> bool {
        SpotAccountHost::strict_equal(&self.base, a, b)
    }

    fn imported(&mut self, k: &str) -> Result<N, N> {
        match k {
            "INFO_TYPES" => Ok(object(vec![(
                "FRONTEND_OPEN_ORDERS",
                s("frontendOpenOrders"),
            )])),
            "OrderStatus" => Ok(account::get(&self.base.enums, "OrderStatus")),
            _ => SpotAccountHost::imported(&mut self.base, k),
        }
    }

    fn array(&mut self, v: &[N]) -> Result<N, N> {
        let array = arr(v.to_vec());
        if self.st.borrow().family == "internal" && matches!(self.st.borrow().collected, N::U) {
            self.st.borrow_mut().collected = array.clone()
        }
        Ok(array)
    }

    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::spread_array(&mut self.base, v)
    }

    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        SpotAccountHost::spread_into(&mut self.base, target, source)
    }

    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(&mut self.base, v)
    }

    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        SpotAccountHost::binary(&mut self.base, op, a, b)
    }

    fn interval_binding(&mut self, v: &N) -> Result<[N; 3], N> {
        SpotAccountHost::interval_binding(&mut self.base, v)
    }

    fn pair_binding(&mut self, v: &N) -> Result<[N; 2], N> {
        SpotAccountHost::pair_binding(&mut self.base, v)
    }

    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        SpotAccountHost::index(&mut self.base, v, k)
    }

    fn digit_regex(&mut self) -> Result<N, N> {
        SpotAccountHost::digit_regex(&mut self.base)
    }

    fn response_data(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::response_data(&mut self.base, v)
    }

    fn candle_binding(&mut self, v: &N) -> Result<[N; 6], N> {
        SpotAccountHost::candle_binding(&mut self.base, v)
    }

    fn construct(&mut self, c: &N, args: &[N]) -> Result<N, N> {
        if let N::Builtin(k) = c {
            if k == "Set" {
                let values = if args.is_empty() {
                    vec![]
                } else {
                    let N::A(a) = &args[0] else {
                        panic!("Set input")
                    };
                    a.borrow().clone()
                };
                let mut unique = vec![];
                for v in values {
                    if !unique.iter().any(|u| eq(u, &v)) {
                        unique.push(v)
                    }
                }
                return Ok(N::Set(Rc::new(RefCell::new(unique))));
            }
            if k == "Map" {
                let N::A(a) = &args[0] else {
                    panic!("Map input")
                };
                let mut out = vec![];
                for entry in a.borrow().iter() {
                    let N::A(e) = entry else { panic!("entry") };
                    let e = e.borrow();
                    if let Some((_, v)) = out.iter_mut().find(|(k, _)| eq(k, &e[0])) {
                        *v = e[1].clone()
                    } else {
                        out.push((e[0].clone(), e[1].clone()))
                    }
                }
                return Ok(N::Map(Rc::new(RefCell::new(out))));
            }
        }
        self.base.construct(c, args)
    }
    fn throw_value(&self, v: N) -> N {
        SpotAccountHost::throw_value(&self.base, v)
    }

    fn error_value(&self, e: &N) -> N {
        SpotAccountHost::error_value(&self.base, e)
    }

    fn callback(&mut self, c: AccountCallback<N>) -> Result<N, N> {
        SpotAccountHost::callback(&mut self.base, c)
    }

    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        SpotAccountHost::await_value(&mut self.base, v)
    }

    fn spawn_native(&self, body: AccountFuture<N, N>) -> N {
        SpotAccountHost::spawn_native(&self.base, body)
    }
}
