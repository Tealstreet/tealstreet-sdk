use super::{
    account,
    cache::{N, arr, eq, s},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    spot_account_reads::*,
    spot_json::JsString,
    startup_lifecycle::{self, StartupCallback, StartupLifecycleHost},
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc};
#[derive(Clone)]
pub struct Host {
    pub base: account::Host,
    pub state: Rc<RefCell<State>>,
}
pub struct State {
    pub effects: Vec<N>,
    pub callbacks: HashMap<String, StartupCallback<N>>,
    pub promises: Vec<N>,
    pub scene: String,
    pub core: N,
    pub positions: N,
    pub marker: N,
    pub drop_mode: bool,
    pub release_mode: bool,
    pub mode_reached: bool,
    pub settlements: Vec<Result<N, N>>,
}
impl Host {
    pub fn record(&self, name: &str, args: &[N]) {
        let mut v = vec![s(name)];
        v.extend_from_slice(args);
        self.state.borrow_mut().effects.push(arr(v));
    }
    pub fn scene(&self) -> String {
        self.state.borrow().scene.clone()
    }
    pub fn ready(&self, result: Result<N, N>) -> N {
        let v = self.base.spawn_native(Box::pin(std::future::ready(result)));
        self.state.borrow_mut().promises.push(v.clone());
        v
    }
    pub fn e(&self) -> N {
        self.base.exchange.clone()
    }
    pub fn put(&mut self, k: &str, v: N) {
        self.set(&self.e(), k, &v).unwrap()
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(task)
    }
    fn settle_return(&self, result: Result<N, N>) -> AccountFuture<N, N> {
        let settled = self.base.settle_return(result);
        let state = self.state.clone();
        Box::pin(async move {
            let result = settled.await;
            state.borrow_mut().settlements.push(result.clone());
            result
        })
    }
}
impl StartupLifecycleHost for Host {
    fn startup_callback(&mut self, c: StartupCallback<N>) -> Result<N, N> {
        let mut st = self.state.borrow_mut();
        let id = format!("startup-callback-{}", st.callbacks.len());
        st.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
}
impl WsPingHost for Host {
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("unexpected native callee {f:?}")
        };
        let cb = self.state.borrow().callbacks.get(id).cloned();
        if let Some(cb) = cb {
            return startup_lifecycle::run_startup_callback(self, &cb, args);
        }
        let scene = self.scene();
        let first = args.first().cloned().unwrap_or(N::U);
        if id == "promise-catch" || id == "promise-then" {
            let mut h = self.clone();
            let original = r.clone();
            let callback = first;
            let is_catch = id == "promise-catch";
            let out = self.base.spawn_native(Box::pin(async move {
                match h.await_value(original).await {
                    Ok(v) => {
                        if is_catch {
                            Ok(v)
                        } else {
                            h.call(&callback, &N::U, &[v])
                        }
                    }
                    Err(err) => {
                        if is_catch {
                            h.call(&callback, &N::U, &[err])
                        } else {
                            Err(err)
                        }
                    }
                }
            }));
            self.state.borrow_mut().promises.push(out.clone());
            return Ok(out);
        }
        match id.as_str() {
            "init" => {
                self.record("init", &[]);
                return Ok(self.ready(if scene == "init-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(N::B(scene.contains("external")))
                }));
            }
            "resolveApplicationIdFromKey" => {
                self.record(id, &[]);
                if scene == "resolve-disposed" {
                    self.put("isDisposed", N::B(true))
                } else if scene == "resolve-success" {
                    self.put("hasPrivateSessionIdentity", N::B(true))
                }
                return Ok(self.ready(Ok(N::U)));
            }
            "warnOnMismatchedApiWallet" => self.record(id, args),
            "resolveUserAbstractionMode"
            | "fetchCorePerpsState"
            | "fetchBuilderFeeApproval"
            | "refreshOrders"
            | "fetchAllHip3Positions" => {
                self.record(id, &[]);
                if scene == "sync-orders-error" && id == "refreshOrders" {
                    return Err(self.state.borrow().marker.clone());
                }
                if scene == "dispose-on-orders" && id == "refreshOrders" {
                    self.put("isDisposed", N::B(true))
                }
                if scene == "mode-disposed" && id == "resolveUserAbstractionMode" {
                    self.put("isDisposed", N::B(true))
                }
                if id == "resolveUserAbstractionMode" && self.state.borrow().drop_mode {
                    self.state.borrow_mut().mode_reached = true;
                    let state = self.state.clone();
                    let v = self
                        .base
                        .spawn_native(Box::pin(std::future::poll_fn(move |_| {
                            if state.borrow().release_mode {
                                std::task::Poll::Ready(Ok(N::U))
                            } else {
                                std::task::Poll::Pending
                            }
                        })));
                    self.state.borrow_mut().promises.push(v.clone());
                    return Ok(v);
                }
                let result = if scene == "reject-all" || scene == format!("reject-{id}") {
                    Err(self.state.borrow().marker.clone())
                } else if id == "fetchCorePerpsState" {
                    Ok(if scene == "core-null" {
                        N::Null
                    } else {
                        self.state.borrow().core.clone()
                    })
                } else {
                    Ok(N::U)
                };
                return Ok(self.ready(result));
            }
            "markets" => {
                self.record("markets", &[]);
                if scene == "markets-disposed" {
                    self.put("isDisposed", N::B(true))
                }
                return Ok(self.ready(if scene == "markets-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(arr(vec![
                        self.base.markets.clone(),
                        self.base.tickers.clone(),
                    ]))
                }));
            }
            "buildMarketsLowerCaseMap" => {
                assert!(
                    eq(&first, &self.base.markets),
                    "native startup market map original handle"
                );
                self.record(id, &[])
            }
            "fake" => {
                assert!(
                    eq(&first, &self.base.markets),
                    "native startup positions original markets"
                );
                self.record("fake", &[]);
                return Ok(self.state.borrow().positions.clone());
            }
            "update" => {
                self.record("update", args);
                let N::O(o) = &first else { panic!("update") };
                for (k, v, _) in o.borrow().fields.clone() {
                    self.set(r, &k, &v)?
                }
            }
            "public-connect" | "private-connect" => self.record(
                if id == "public-connect" {
                    "public-connect"
                } else {
                    "private-connect"
                },
                &[],
            ),
            "shouldTrackSpotHoldings" => {
                self.record(id, &[]);
                return Ok(N::B(scene != "no-track"));
            }
            "requestUnifiedSpotCollateralRefresh" => self.record(id, args),
            "applyCoreClearinghouseState" => {
                assert_eq!(args.len(), 2, "native core snapshot callback arity");
                assert!(
                    eq(&args[1], &N::B(true)),
                    "native core snapshot complete flag"
                );
                assert!(
                    eq(&first, &self.state.borrow().core),
                    "native startup core state original handle"
                );
                self.record(id, args)
            }
            "log" => {
                self.record(id, args);
                if scene == "log-error" {
                    return Err(self.state.borrow().marker.clone());
                }
            }
            "now" => return Ok(N::Num(200000.)),
            "tick" => {
                self.record("tick", &[]);
                return Ok(self.ready(Ok(N::U)));
            }
            "interval" => {
                self.record(id, &[]);
                return Ok(N::Num(2000.));
            }
            "loop" => {
                self.record("loop", &args[1..]);
                return self.base.call(f, r, args);
            }
            _ => return self.base.call(f, r, args),
        }
        Ok(N::U)
    }
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
        WsPingHost::global(&mut self.base, k)
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if self.state.borrow().promises.iter().any(|p| eq(p, v)) && matches!(k, "catch" | "then") {
            return Ok(N::Builtin(format!("promise-{k}")));
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
        self.base.imported(k)
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
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(&mut self.base, constructor, args)
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
