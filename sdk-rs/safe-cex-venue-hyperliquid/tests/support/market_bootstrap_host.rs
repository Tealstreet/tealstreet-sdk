use super::{
    account,
    cache::{self, N, arr, eq, object, s},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    market_bootstrap::{self, MarketBootstrapCallback, MarketBootstrapHost},
    spot_account_reads::*,
    spot_json::JsString,
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
    pub callbacks: HashMap<String, MarketBootstrapCallback<N>>,
    pub scene: String,
    pub clock: usize,
    pub marker: N,
    pub close_marker: N,
    pub dexes: N,
    pub fees: N,
    pub meta: N,
    pub assets: N,
    pub markets: N,
    pub tickers: N,
    pub settlements: Vec<Result<N, N>>,
    pub iterators: Vec<(N, usize, bool)>,
}
pub fn get(v: &N, k: &str) -> N {
    account::get(v, k)
}
impl Host {
    pub fn e(&self) -> N {
        self.base.exchange.clone()
    }
    pub fn scene(&self) -> String {
        self.state.borrow().scene.clone()
    }
    pub fn record(&self, name: &str, args: &[N]) {
        let mut values = vec![s(name)];
        values.extend_from_slice(args);
        self.state.borrow_mut().effects.push(arr(values))
    }
    pub fn ready(&self, value: Result<N, N>) -> N {
        self.base.spawn_native(Box::pin(std::future::ready(value)))
    }
    pub fn pair(prefix: &str) -> N {
        object(vec![
            ("markets", arr(vec![s(&format!("{prefix}-m"))])),
            ("tickers", arr(vec![s(&format!("{prefix}-t"))])),
        ])
    }
    pub fn time(&self) -> f64 {
        let mut st = self.state.borrow_mut();
        let times = if st.scene == "deadline-empty" {
            [0., 60., 70.]
        } else if st.scene == "deadline-after-first" {
            [0., 0., 60.]
        } else {
            [0., 0., 10.]
        };
        let value = times[st.clock.min(2)];
        st.clock += 1;
        value
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(task)
    }
    fn settle_return(&self, result: Result<N, N>) -> AccountFuture<N, N> {
        let task = self.base.settle_return(result);
        let state = self.state.clone();
        Box::pin(async move {
            let value = task.await;
            state.borrow_mut().settlements.push(value.clone());
            value
        })
    }
}
impl MarketBootstrapHost for Host {
    fn bootstrap_callback(&mut self, c: MarketBootstrapCallback<N>) -> Result<N, N> {
        let mut state = self.state.borrow_mut();
        let id = format!("bootstrap-callback-{}", state.callbacks.len());
        state.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
    fn array_binding(&mut self, v: &N, count: usize) -> Result<Vec<N>, N> {
        let N::A(a) = v else {
            panic!("native binding fixture requires array")
        };
        let a = a.borrow();
        Ok((0..count)
            .map(|i| a.get(i).cloned().unwrap_or(N::U))
            .collect())
    }
    fn spread_arguments(&mut self, args: &mut Vec<N>, v: &N) -> Result<(), N> {
        let N::A(a) = v else {
            panic!("native spread requires array")
        };
        args.extend(a.borrow().iter().cloned());
        Ok(())
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let is_dex = eq(v, &self.state.borrow().dexes);
        if is_dex {
            self.record("dex-iterator", &[])
        }
        let mut st = self.state.borrow_mut();
        let id = st.iterators.len();
        st.iterators.push((v.clone(), 0, is_dex));
        Ok(N::Num(id as f64))
    }
    fn next(&mut self, it: &N) -> Result<Option<N>, N> {
        let id = cache::num(it) as usize;
        let mut st = self.state.borrow_mut();
        if st.iterators[id].2 && st.iterators[id].1 == 1 && st.scene == "next-error" {
            return Err(st.marker.clone());
        }
        let (v, i, _) = &mut st.iterators[id];
        let N::A(a) = v else { panic!("iterator") };
        let out = a.borrow().get(*i).cloned();
        *i += 1;
        Ok(out)
    }
    fn close_after_throw(&mut self, it: &N) {
        let id = cache::num(it) as usize;
        if self.state.borrow().iterators[id].2 {
            self.record("dex-close", &[]);
            if self.scene() == "body-and-close-error" {
                let marker = self.state.borrow().close_marker.clone();
                self.record("dex-close-error", &[marker])
            }
        }
    }
    fn close_after_break(&mut self, it: &N) -> Result<(), N> {
        self.close_after_throw(it);
        if self.scene() == "body-and-close-error" {
            Err(self.state.borrow().close_marker.clone())
        } else if self.scene() == "close-error" {
            Err(self.state.borrow().marker.clone())
        } else {
            Ok(())
        }
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
            "Math" => object(vec![
                ("min", N::Builtin("min".into())),
                ("max", N::Builtin("max".into())),
                ("round", N::Builtin("round".into())),
            ]),
            _ => return self.base.global(k),
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if k == "all" {
            self.record("all-get", &[])
        }
        if eq(v, &self.e()) && k == "withStartupTimeout" {
            self.record("timeout-get", &[])
        }
        if matches!(v, N::A(_)) && ["push", "map", "filter", "join", "includes"].contains(&k) {
            return Ok(N::Builtin(k.into()));
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
        let callback = self.state.borrow().callbacks.get(id).cloned();
        if let Some(cb) = callback {
            return market_bootstrap::run_market_bootstrap_callback(self, &cb, args);
        }
        let first = args.first().cloned().unwrap_or(N::U);
        let scene = self.scene();
        match id.as_str() {
            "fetchCoreMetaAndAssetCtxsWithFallback" => {
                let info = get(&first, "infoXhr");
                let xhr = get(&first, "xhr");
                assert!(
                    eq(&info, &get(&self.e(), "infoXhr")),
                    "native core helper original info client"
                );
                assert!(
                    eq(&xhr, &get(&self.e(), "xhr")),
                    "native core helper original fallback client"
                );
                self.record("core", &[]);
                if scene == "core-logger" {
                    let cb = get(&first, "log");
                    self.call(&cb, &N::U, &[s("core diagnostic")])?;
                }
                return Ok(self.ready(if scene == "core-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(arr(vec![
                        self.state.borrow().meta.clone(),
                        self.state.borrow().assets.clone(),
                    ]))
                }));
            }
            "fetchFee" => {
                self.record("fees", &[]);
                return Ok(self.ready(if scene == "fees-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(self.state.borrow().fees.clone())
                }));
            }
            "fetchHip3Dexs" => {
                self.record("dexes", &[]);
                return Ok(self.ready(Ok(self.state.borrow().dexes.clone())));
            }
            "fetchSpotMarketsAndTickers" => {
                self.record("spot", &[]);
                let pair = if ["core-only", "zero-spot"].contains(&scene.as_str()) {
                    object(vec![("markets", arr(vec![])), ("tickers", arr(vec![]))])
                } else {
                    Self::pair("spot")
                };
                return Ok(self.ready(Ok(pair)));
            }
            "withStartupTimeout" => {
                self.record("timeout", &args[1..]);
                return Ok(first);
            }
            "all" => {
                self.record("all", &[]);
                let N::A(a) = &first else { panic!("all") };
                let values = a.borrow().clone();
                let mut h = self.clone();
                return Ok(self.base.spawn_native(Box::pin(async move {
                    let mut results = vec![];
                    for v in values {
                        results.push(h.await_value(v).await?);
                    }
                    Ok(arr(results))
                })));
            }
            "fetchMarketsHelper" => {
                let st = self.state.borrow();
                assert_eq!(args.len(), 3, "native core market helper arity");
                assert!(
                    eq(&args[0], &st.meta) && eq(&args[1], &st.assets) && eq(&args[2], &st.fees),
                    "native core market helper distinct original inputs"
                );
                let result = object(vec![
                    ("markets", st.markets.clone()),
                    ("tickers", st.tickers.clone()),
                ]);
                drop(st);
                self.record("helper", &[]);
                return Ok(result);
            }
            "fetchHip3Markets" => {
                assert_eq!(args.len(), 3, "native dex fetch arity");
                assert!(
                    eq(&args[2], &self.state.borrow().fees),
                    "native dex original fees handle"
                );
                self.record("fetch-dex", &args[..2]);
                let index = cache::num(&args[1]) as usize;
                let value = if (["first-null", "failed-then-timeout"].contains(&scene.as_str())
                    && index == 0)
                    || (scene == "second-null" && index == 1)
                    || scene == "all-null"
                {
                    N::Null
                } else if (scene == "timeout-first" && index == 0)
                    || (["timeout-second", "close-error", "failed-then-timeout"]
                        .contains(&scene.as_str())
                        && index == 1)
                {
                    s("timed-out")
                } else {
                    Self::pair(if index == 0 { "a" } else { "b" })
                };
                if ["fetch-sync-error", "body-and-close-error"].contains(&scene.as_str()) {
                    return Err(self.state.borrow().marker.clone());
                }
                return Ok(self.ready(if scene == "timeout-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(value)
                }));
            }
            "now" => {
                let t = self.time();
                self.record("now", &[N::Num(t)]);
                return Ok(N::Num(t));
            }
            "min" | "max" => {
                let a = cache::num(&first);
                let b = cache::num(&args[1]);
                return Ok(N::Num(if id == "min" { a.min(b) } else { a.max(b) }));
            }
            "round" => return Ok(N::Num((cache::num(&first) + 0.5).floor())),
            "push" => {
                let N::A(a) = r else { panic!("push") };
                a.borrow_mut().extend_from_slice(args);
                return Ok(N::Num(a.borrow().len() as f64));
            }
            "map" | "filter" => {
                let N::A(a) = r else { panic!("array method") };
                let values = a.borrow().clone();
                let mut out = vec![];
                for (i, v) in values.iter().enumerate() {
                    let result =
                        self.call(&first, &N::U, &[v.clone(), N::Num(i as f64), r.clone()])?;
                    if id == "map" {
                        out.push(result)
                    } else if self.truthy(&result) {
                        out.push(v.clone())
                    }
                }
                return Ok(arr(out));
            }
            "join" => {
                let N::A(a) = r else { panic!("join") };
                let separator = self.template_string(&first)?;
                let mut out = vec![];
                for (i, v) in a.borrow().iter().enumerate() {
                    if i > 0 {
                        out.extend(&separator.0)
                    }
                    out.extend(self.template_string(v)?.0)
                }
                return Ok(N::S(JsString(out)));
            }
            "log" => {
                self.record("log", args);
                return Ok(N::U);
            }
            _ => return self.base.call(f, r, args),
        }
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
        if k == "HyperliquidExchange" {
            return Ok(object(vec![
                ("HIP3_DEX_DISCOVERY_TIMEOUT_MS", N::Num(15.)),
                ("SPOT_MARKETS_LOAD_TIMEOUT_MS", N::Num(25.)),
                ("HIP3_MARKETS_LOAD_TIMEOUT_CAP_MS", N::Num(100.)),
                ("HIP3_MARKETS_LOAD_TIMEOUT_MS", N::Num(40.)),
                (
                    "HIP3_PER_DEX_LOAD_BUDGET_MS",
                    N::Num(if self.scene() == "capped-budget" {
                        100.
                    } else if self.scene() == "minimum-budget" {
                        10.
                    } else {
                        30.
                    }),
                ),
            ]));
        }
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
        Ok(match op {
            "+" => N::Num(cache::num(a) + cache::num(b)),
            "<=" => N::B(cache::num(a) <= cache::num(b)),
            _ => return self.base.binary(op, a, b),
        })
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
