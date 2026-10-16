use super::{
    account,
    cache::{self, N, arr, eq, s},
};
use safe_cex_venue_hyperliquid::{
    cross_cache::CrossCacheHost,
    helper_tasks::HelperTaskHost,
    order_retry::{self, OrderRetryCallback, OrderRetryHost},
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
    pub effects: Vec<N>,
    pub callbacks: HashMap<String, OrderRetryCallback<N>>,
    pub promises: Vec<N>,
    pub marker: N,
    pub secondary: N,
    pub timers: Vec<(N, N)>,
    pub rows: N,
    pub attempt: N,
    pub coercions: usize,
    pub found: N,
    pub found_iterator: N,
    pub fetch_calls: usize,
    pub step_calls: usize,
    pub end_calls: usize,
    pub log_calls: usize,
    pub settlements: Vec<Result<N, N>>,
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
    pub fn coerce(&self, v: &N, hint: &str) -> N {
        if !eq(v, &self.st.borrow().attempt) || !self.scene().starts_with("stateful") {
            return v.clone();
        }
        let index = self.st.borrow().coercions;
        self.st.borrow_mut().coercions += 1;
        self.record("coerce", &[s(hint)]);
        if self.scene() == "stateful-reject" {
            match index {
                0 => N::Num(2.),
                1 => s("label"),
                2 => N::Num(2.),
                _ => s("2"),
            }
        } else {
            match index {
                0 => N::Num(2.),
                1 => N::Num(1.),
                _ => s("2"),
            }
        }
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
impl OrderRetryHost for Host {
    fn order_retry_callback(&mut self, c: OrderRetryCallback<N>) -> Result<N, N> {
        let mut st = self.st.borrow_mut();
        let id = format!("retry-callback-{}", st.callbacks.len());
        st.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        if self.scene() == "iterator-error" {
            return Err(self.st.borrow().marker.clone());
        }
        let values = match v {
            N::A(v) | N::Set(v) => v.borrow().clone(),
            _ => panic!("native iterator input"),
        };
        let it = N::Iter(Rc::new(RefCell::new((values, 0))));
        if eq(v, &self.st.borrow().found) {
            self.st.borrow_mut().found_iterator = it.clone()
        }
        Ok(it)
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else {
            panic!("iterator record")
        };
        let mut i = i.borrow_mut();
        let found = matches!(&self.st.borrow().found_iterator,N::Iter(f)if Rc::ptr_eq(f,&(match v{N::Iter(i)=>i.clone(),_=>unreachable!()})));
        if self.scene() == "next-error" && found && i.1 == 1 {
            return Err(self.st.borrow().marker.clone());
        }
        let value = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(value)
    }
    fn close_after_throw(&mut self, _: &N) {
        self.record("close", &[])
    }
    fn retry_operator(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        let a = self.coerce(a, if op == "+" { "default" } else { "number" });
        let b = self.coerce(b, if op == "+" { "default" } else { "number" });
        if op == "+" && (matches!(a, N::S(_)) || matches!(b, N::S(_))) {
            let mut text = SpotAccountHost::template_string(self, &a)?.0;
            text.extend(SpotAccountHost::template_string(self, &b)?.0);
            return Ok(N::S(JsString(text)));
        }
        let (a, b) = (cache::num(&a), cache::num(&b));
        Ok(match op {
            "-" => N::Num(a - b),
            "+" => N::Num(a + b),
            ">=" => N::B(a >= b),
            ">" => N::B(a > b),
            "<" => N::B(a < b),
            _ => panic!("operator"),
        })
    }
}
impl safe_cex_venue_hyperliquid::lifecycle_delegates::LifecycleHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn boolean(&self, b: bool) -> N {
        N::B(b)
    }
    fn number(&self, n: f64) -> N {
        N::Num(n)
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn super_get(&mut self, _: &N, _: &str) -> Result<N, N> {
        panic!("unused super binding")
    }
    fn lifecycle_callback(
        &mut self,
        _: safe_cex_venue_hyperliquid::lifecycle_delegates::LifecycleCallback<N>,
    ) -> Result<N, N> {
        panic!("unused lifecycle callback binding")
    }
    fn subtract(&mut self, a: &N, b: &N) -> Result<N, N> {
        SpotAccountHost::binary(self, "-", a, b)
    }
    fn greater_than(&mut self, a: &N, b: &N) -> Result<bool, N> {
        Ok(cache::num(a) > cache::num(b))
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(self, v)
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
        if matches!(k, "Set" | "setTimeout") {
            return Ok(N::Builtin(k.into()));
        }
        self.base.global(k)
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.e()) && k == "isDisposed" && self.scene() == "disposed-get-error" {
            return Err(self.st.borrow().marker.clone());
        }
        if matches!(v, N::A(_)) && !matches!(k, "length" | "map") && k.parse::<usize>().is_err() {
            return Ok(N::U);
        }
        self.base.get(v, k)
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        if eq(v, &self.e())
            && k == "_retryChainTimer"
            && matches!(value, N::Null)
            && self.scene() == "reset-error"
        {
            return Err(self.st.borrow().marker.clone());
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
            panic!("native callee {f:?}")
        };
        let cb = self.st.borrow().callbacks.get(id).cloned();
        if let Some(c) = cb {
            return match c {
                OrderRetryCallback::Id => order_retry::run_order_retry_callback(self, &c, args),
                OrderRetryCallback::Timer { .. } => {
                    let observer = order_retry::start_order_retry_callback(self, c);
                    let value = self.base.promise(observer);
                    self.st.borrow_mut().promises.push(value.clone());
                    Ok(value)
                }
            };
        }
        let first = args.first().cloned().unwrap_or(N::U);
        let scene = self.scene();
        match id.as_str() {
            "setTimeout" => {
                self.record("timer", &args[1..]);
                if scene == "set-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let mut st = self.st.borrow_mut();
                st.timers.push((first, args[1].clone()));
                return Ok(N::Num(10. + st.timers.len() as f64));
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
            "add" => {
                if scene == "add-error" && eq(&first, &s("b")) {
                    return Err(self.st.borrow().marker.clone());
                }
                return CrossCacheHost::call(&mut cache::Host::default(), f, r, args)
                    .map_err(|e| s(&e));
            }
            "delete" => {
                self.record("delete", args);
                if scene == "delete-error" && eq(&first, &s("b")) {
                    return Err(self.st.borrow().marker.clone());
                }
                return CrossCacheHost::call(&mut cache::Host::default(), f, r, args)
                    .map_err(|e| s(&e));
            }
            "clear" => {
                self.record("clear", &[]);
                let N::Set(v) = r else { panic!("clear") };
                v.borrow_mut().clear();
                return Ok(N::U);
            }
            "runRetryChainStep" => {
                assert!(eq(r, &self.e()), "native retry step lexical receiver");
                self.st.borrow_mut().step_calls += 1;
                self.record("next", args);
                if scene == "step-error" || scene == "start-error" {
                    let call = self.st.borrow().step_calls;
                    return Err(if call == 1 {
                        self.st.borrow().marker.clone()
                    } else {
                        self.st.borrow().secondary.clone()
                    });
                }
                if scene == "step-once-error" && self.st.borrow().step_calls == 1 {
                    return Err(self.st.borrow().marker.clone());
                }
                if scene == "full-chain" {
                    return order_retry::run_retry_chain_step(self, r, &first);
                }
                return Ok(self.st.borrow().marker.clone());
            }
            "endRetryChain" => {
                assert!(eq(r, &self.e()), "native end lexical receiver");
                assert!(
                    matches!(account::get(r, "_retryChainTimer"), N::Null),
                    "native timer null before end observation"
                );
                self.st.borrow_mut().end_calls += 1;
                self.record("end", &[]);
                if matches!(scene.as_str(), "end-error" | "disposed-end-error") {
                    let count = self.st.borrow().end_calls;
                    return Err(if count == 1 {
                        self.st.borrow().marker.clone()
                    } else {
                        self.st.borrow().secondary.clone()
                    });
                }
                safe_cex_venue_hyperliquid::lifecycle_delegates::end_retry_chain(self, r)?;
                return Ok(self.st.borrow().marker.clone());
            }
            "fetchOrders" => {
                assert!(eq(r, &self.e()), "native fetch lexical receiver");
                assert!(
                    matches!(account::get(r, "_retryChainTimer"), N::Null),
                    "native timer null before fetch observation"
                );
                self.st.borrow_mut().fetch_calls += 1;
                self.record("fetch", &[]);
                if scene == "sync-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let result =
                    if scene == "reject" || scene == "last-reject" || scene == "stateful-reject" {
                        Err(self.st.borrow().marker.clone())
                    } else {
                        Ok(self.st.borrow().rows.clone())
                    };
                return Ok(self.ready(result));
            }
            "log" => {
                assert!(eq(r, &self.e()), "native retry warning lexical receiver");
                self.st.borrow_mut().log_calls += 1;
                self.record("log", args);
                if scene == "log-error" {
                    let count = self.st.borrow().log_calls;
                    return Err(if count == 1 {
                        self.st.borrow().marker.clone()
                    } else {
                        self.st.borrow().secondary.clone()
                    });
                }
                return Ok(self.st.borrow().marker.clone());
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
        SpotAccountHost::imported(&mut self.base, k)
    }

    fn array(&mut self, v: &[N]) -> Result<N, N> {
        SpotAccountHost::array(&mut self.base, v)
    }

    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::spread_array(&mut self.base, v)
    }

    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        SpotAccountHost::spread_into(&mut self.base, target, source)
    }

    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        let value = self.coerce(v, "string");
        self.base.template_string(&value)
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
        let text = SpotAccountHost::template_string(self, k)?;
        self.get(v, &String::from_utf16_lossy(&text.0))
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
        if matches!(c,N::Builtin(k)if k=="Set") {
            let N::A(a) = &args[0] else {
                panic!("Set input")
            };
            let mut values = vec![];
            for value in a.borrow().iter() {
                if !values.iter().any(|v| eq(v, value)) {
                    values.push(value.clone())
                }
            }
            let value = N::Set(Rc::new(RefCell::new(values)));
            self.st.borrow_mut().found = value.clone();
            return Ok(value);
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
