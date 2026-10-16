use super::{
    account,
    cache::{N, arr, eq, object, s},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    hip3_position_refresh::{self, PositionRefreshCallback, PositionRefreshHost},
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
    pub callbacks: HashMap<String, PositionRefreshCallback<N>>,
    pub promises: Vec<N>,
    pub timer: N,
    pub snapshot: N,
    pub pending: N,
    pub marker: N,
    pub closes: usize,
    pub timer_reads: usize,
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
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, t: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(t)
    }
    fn settle_return(&self, r: Result<N, N>) -> AccountFuture<N, N> {
        self.base.settle_return(r)
    }
}
impl PositionRefreshHost for Host {
    fn position_refresh_callback(&mut self, c: PositionRefreshCallback<N>) -> Result<N, N> {
        let mut st = self.st.borrow_mut();
        let id = format!("position-callback-{}", st.callbacks.len());
        st.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let N::Set(v) = v else { panic!("iterator") };
        Ok(N::Iter(Rc::new(RefCell::new((v.borrow().clone(), 0)))))
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else { panic!("next") };
        let mut i = i.borrow_mut();
        if self.scene() == "next-error" && i.1 == 1 {
            return Err(self.st.borrow().marker.clone());
        }
        let v = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(v)
    }
    fn close_after_throw(&mut self, _: &N) {
        self.st.borrow_mut().closes += 1;
        self.record("close", &[])
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
        Ok(match k {
            "Promise" => object(vec![("all", N::Builtin("all".into()))]),
            "Array" => object(vec![("from", N::Builtin("from".into()))]),
            "Set" | "setTimeout" | "clearTimeout" => N::Builtin(k.into()),
            _ => return self.base.global(k),
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.e()) && k == "_hip3PositionRefreshDebounceTimer" {
            let mut st = self.st.borrow_mut();
            st.timer_reads += 1;
            if st.scene == "fresh-timer" && st.timer_reads == 2 {
                return Ok(N::Num(72.));
            }
        }
        if self.st.borrow().promises.iter().any(|p| eq(p, v)) && k == "catch" {
            return Ok(N::Builtin("promise-catch".into()));
        }
        self.base.get(v, k)
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::set(&mut self.base, v, k, value)
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
            return hip3_position_refresh::run_position_refresh_callback(self, &c, args);
        }
        let first = args.first().cloned().unwrap_or(N::U);
        let scene = self.scene();
        match id.as_str() {
            "map" => {
                let N::A(a) = r else { panic!("map receiver") };
                let len = a.borrow().len();
                let mut out = vec![];
                for index in 0..len {
                    let value = a.borrow().get(index).cloned().unwrap_or(N::U);
                    out.push(self.call(&first, &N::U, &[value, N::Num(index as f64), r.clone()])?)
                }
                return Ok(arr(out));
            }

            "promise-catch" => {
                let mut h = self.clone();
                let original = r.clone();
                let out = self.base.spawn_native(Box::pin(async move {
                    match h.await_value(original).await {
                        Ok(v) => Ok(v),
                        Err(e) => h.call(&first, &N::U, &[e]),
                    }
                }));
                self.st.borrow_mut().promises.push(out.clone());
                return Ok(out);
            }
            "all" => {
                let N::A(a) = first else { panic!("all") };
                let values = a.borrow().clone();
                let mut h = self.clone();
                let out = self.base.spawn_native(Box::pin(async move {
                    let mut out = vec![];
                    for v in values {
                        out.push(h.await_value(v).await?)
                    }
                    Ok(arr(out))
                }));
                self.st.borrow_mut().promises.push(out.clone());
                return Ok(out);
            }
            "from" => {
                let N::Set(a) = first else { panic!("from") };
                return Ok(arr(a.borrow().clone()));
            }
            "fetchHip3Positions" => {
                assert!(eq(r, &self.e()), "native fetch receiver");
                self.record("fetch", args);
                if scene == "sync-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let reject =
                    (scene == "reject-one" || scene == "log-error") && eq(&first, &s("dex-b"));
                return Ok(self.ready(if reject {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(N::U)
                }));
            }
            "log" => {
                assert!(eq(r, &self.e()), "native warning lexical receiver");
                self.record("log", args);
                if scene == "log-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                return Ok(self.st.borrow().marker.clone());
            }
            "add" => {
                if scene == "add-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let N::Set(v) = r else { panic!("add") };
                if !v.borrow().iter().any(|p| eq(p, &first)) {
                    v.borrow_mut().push(first)
                }
                return Ok(r.clone());
            }
            "clear" => {
                let N::Set(v) = r else { panic!("clear") };
                self.record("clear", &[]);
                v.borrow_mut().clear();
                return Ok(N::U);
            }
            "clearTimeout" => {
                self.record("cancel", args);
                return Ok(N::U);
            }
            "setTimeout" => {
                self.record("timer", &args[1..]);
                self.st.borrow_mut().timer = first;
                if scene == "set-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                return Ok(N::Num(99.));
            }
            "refreshHip3PositionsAsync" => {
                assert!(eq(r, &self.e()), "native refresh receiver");
                assert!(
                    matches!(
                        account::get(&self.e(), "_hip3PositionRefreshDebounceTimer"),
                        N::Null
                    ),
                    "native timer null before refresh observation"
                );
                self.st.borrow_mut().snapshot = first.clone();
                let N::Set(v) = &first else {
                    panic!("snapshot")
                };
                self.record("refresh", &[arr(v.borrow().clone())]);
                assert!(
                    !eq(&first, &self.st.borrow().pending),
                    "native snapshot distinct Set"
                );
                let N::Set(p) = &self.st.borrow().pending else {
                    panic!("pending")
                };
                assert!(
                    p.borrow().is_empty(),
                    "native pending cleared before refresh"
                );
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
        if matches!(c,N::Builtin(k)if k=="Set") {
            let N::Set(v) = &args[0] else { panic!("Set") };
            return Ok(N::Set(Rc::new(RefCell::new(v.borrow().clone()))));
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
