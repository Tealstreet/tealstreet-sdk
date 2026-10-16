use super::{
    account,
    cache::{self, N, arr, eq, object, s},
    native,
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    order_update::*,
    spot_account_reads::*,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
#[derive(Clone)]
pub struct Host {
    pub base: account::Host,
    pub st: Rc<RefCell<State>>,
}
pub struct State {
    pub scene: &'static str,
    pub opts: N,
    pub order: N,
    pub symbol: N,
    pub reduce: N,
    pub market: N,
    pub markets: N,
    pub ticker: N,
    pub tickers: N,
    pub store: N,
    pub dead: N,
    pub marker: N,
    pub cancel_result: N,
    pub result: N,
    pub effects: Vec<N>,
    pub placeholders: Vec<N>,
    pub placed: Option<N>,
    pub cancel_seen: Option<N>,
    pub candidates: Option<N>,
    pub reference: Option<N>,
    pub max: Option<N>,
    pub cancel_completed: bool,
    pub replacement_client_order_id: Option<N>,
    pub callbacks: std::collections::HashMap<String, UpdateCallback<N>>,
    pub settled: Vec<Result<N, N>>,
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(task)
    }
    fn settle_return(&self, result: Result<N, N>) -> AccountFuture<N, N> {
        let st = self.st.clone();
        let f = self.base.settle_return(result);
        Box::pin(async move {
            let r = f.await;
            st.borrow_mut().settled.push(r.clone());
            r
        })
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
    fn global(&mut self, k: &str) -> Result<N, N> {
        Ok(N::Builtin(k.into()))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if matches!(v, N::S(_)) && k == "includes" {
            return Ok(N::Builtin("string-includes".into()));
        }
        if matches!(v,N::Builtin(name)if name=="Number") && k == "isFinite" {
            return Ok(N::Builtin("Number.isFinite".into()));
        }
        if matches!(v,N::Builtin(name)if name=="Math") && ["abs", "max"].contains(&k) {
            return Ok(N::Builtin(format!("Math.{k}")));
        }
        if eq(v, &self.base.exchange) && ["log", "cancelOrders", "placeOrderWithCloid"].contains(&k) {
            return Ok(N::Builtin(k.into()));
        }
        if eq(v, &self.st.borrow().store) && k == "addOrUpdateOrder" {
            return Ok(N::Builtin(k.into()));
        }
        WsPingHost::get(&mut self.base, v, k)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("UPDATE callee {f:?}")
        };
        let callback = self.st.borrow().callbacks.get(id).cloned();
        if let Some(c) = callback {
            return run_update_callback(self, &c, args);
        }
        let scene = self.st.borrow().scene;
        let first = args.first().cloned().unwrap_or(N::U);
        match id.as_str() {
            "string-includes" => {
                let N::S(t) = r else {
                    panic!("string receiver")
                };
                assert_eq!(args.len(), 1, "UPDATE native HIP3 includes arity");
                let N::S(needle) = &args[0] else {
                    panic!("UPDATE native HIP3 includes string needle")
                };
                assert_eq!(needle.0, [58], "UPDATE native HIP3 includes needle");
                Ok(N::B(t.0.windows(needle.0.len()).any(|w| w == needle.0)))
            }
            "Number" => {
                self.record(id, args);
                Ok(N::Num(cache::num(&first)))
            }
            "Number.isFinite" => {
                assert!(
                    matches!(r,N::Builtin(k)if k=="Number"),
                    "UPDATE native Number.isFinite receiver"
                );
                self.record(id, args);
                Ok(N::B(matches!(first,N::Num(n)if n.is_finite())))
            }
            "Boolean" => Ok(N::B(self.truthy(&first))),
            "Math.abs" => {
                assert!(
                    matches!(r,N::Builtin(k)if k=="Math"),
                    "UPDATE native Math receiver"
                );
                self.record(id, args);
                Ok(N::Num(cache::num(&first).abs()))
            }
            "Math.max" => {
                assert!(
                    matches!(r,N::Builtin(k)if k=="Math"),
                    "UPDATE native Math.max receiver"
                );
                self.record(id, args);
                let (a, b) = (cache::num(&first), cache::num(&args[1]));
                Ok(N::Num(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }))
            }
            "log" => {
                assert!(eq(r, &self.base.exchange), "UPDATE native log receiver");
                self.record(id, args);
                if scene == "log-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().cancel_result.clone())
                }
            }
            "addOrUpdateOrder" => {
                assert!(
                    eq(r, &self.st.borrow().store),
                    "UPDATE native placeholder store receiver"
                );
                self.record(id, args);
                self.st.borrow_mut().placeholders.push(first);
                if scene == "store-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().cancel_result.clone())
                }
            }
            "cancelOrders" => {
                if hip(scene) {
                    assert_eq!(
                        self.st.borrow().placeholders.len(),
                        1,
                        "UPDATE native placeholder before cancellation"
                    );
                }
                assert!(eq(r, &self.base.exchange), "UPDATE native cancel receiver");
                self.record(id, args);
                let N::A(a) = &first else {
                    panic!("cancel array")
                };
                assert_eq!(
                    a.borrow().len(),
                    1,
                    "UPDATE native independent singleton cancel input"
                );
                assert!(
                    eq(&a.borrow()[0], &self.st.borrow().order),
                    "UPDATE native cancel original order alias"
                );
                self.st.borrow_mut().cancel_seen = Some(account::get(&a.borrow()[0], "id"));
                Ok(self.ready(if scene == "cancel-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().cancel_result.clone())
                }))
            }
            "add" => {
                assert!(
                    self.st.borrow().cancel_completed,
                    "UPDATE native cancellation awaited before dead add"
                );
                assert!(
                    eq(r, &self.st.borrow().dead),
                    "UPDATE native original dead Set receiver"
                );
                self.record("dead-add", args);
                if scene == "dead-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                let N::Set(keys) = r else { panic!("dead Set") };
                if !keys.borrow().iter().any(|v| eq(v, &first)) {
                    keys.borrow_mut().push(first)
                }
                Ok(self.st.borrow().cancel_result.clone())
            }
            "mintHyperliquidCloid" => {
                self.record(id, args);
                Ok(s("0xmint"))
            }
            "withReplacementClientOrderId" => {
                self.record(id, &args[1..]);
                self.st.borrow_mut().replacement_client_order_id = Some(args[1].clone());
                Ok(first)
            }
            "placeOrderWithCloid" => {
                assert!(
                    self.st.borrow().cancel_completed,
                    "UPDATE native cancellation awaited before placement"
                );
                let dead = self.st.borrow().dead.clone();
                let N::Set(keys) = dead else { unreachable!() };
                assert_eq!(
                    keys.borrow().len(),
                    1,
                    "UPDATE native dead add before placement"
                );
                assert!(eq(r, &self.base.exchange), "UPDATE native place receiver");
                self.record(id, args);
                self.st.borrow_mut().placed = Some(first);
                Ok(self.ready(if scene == "place-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().result.clone())
                }))
            }
            "getCurrentTimeMs" => Ok(N::Num(987654.)),
            "filter" => {
                let N::A(a) = r else { panic!("filter") };
                let mut result = vec![];
                for (i, v) in a.borrow().clone().iter().enumerate() {
                    let keep =
                        self.call(&first, &N::U, &[v.clone(), N::Num(i as f64), r.clone()])?;
                    if self.truthy(&keep) {
                        result.push(v.clone())
                    }
                }
                let result = arr(result);
                self.st.borrow_mut().candidates = Some(result.clone());
                Ok(result)
            }
            "reduce" => {
                let N::A(a) = r else { panic!("reduce") };
                let mut result = args[1].clone();
                for (i, v) in a.borrow().clone().iter().enumerate() {
                    result = self.call(
                        &first,
                        &N::U,
                        &[result, v.clone(), N::Num(i as f64), r.clone()],
                    )?
                }
                self.st.borrow_mut().max = Some(result.clone());
                Ok(result)
            }
            _ => WsPingHost::call(&mut self.base, f, r, args),
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
    fn digit_regex(&mut self) -> Result<N, N> {
        SpotAccountHost::digit_regex(&mut self.base)
    }
    fn response_data(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::response_data(&mut self.base, v)
    }
    fn candle_binding(&mut self, v: &N) -> Result<[N; 6], N> {
        SpotAccountHost::candle_binding(&mut self.base, v)
    }
    fn construct(&mut self, class: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(&mut self.base, class, args)
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
    fn spawn_native(&self, body: AccountFuture<N, N>) -> N {
        SpotAccountHost::spawn_native(&self.base, body)
    }
    fn imported(&mut self, k: &str) -> Result<N, N> {
        Ok(match k {
            "OrderType" => object(vec![
                ("Limit", s("limit")),
                ("TakeProfit", s("tp")),
                ("StopLoss", s("sl")),
            ]),
            "OrderSide" => object(vec![("Sell", s("sell")), ("Buy", s("buy"))]),
            "OrderStatus" => object(vec![("Updating", s("updating"))]),
            "LogSeverity" => object(vec![("Warning", s("Warning")), ("Info", s("Info"))]),
            "virtualClock" => object(vec![(
                "getCurrentTimeMs",
                N::Builtin("getCurrentTimeMs".into()),
            )]),
            _ => N::Builtin(k.into()),
        })
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        if self.st.borrow().scene == "utf16-symbol" && matches!(k,N::S(t)if t.0==[0xd800,83]) {
            if eq(v, &self.st.borrow().markets) {
                return Ok(self.st.borrow().market.clone());
            }
            if eq(v, &self.st.borrow().tickers) {
                return Ok(self.st.borrow().ticker.clone());
            }
        }
        let t = self.template_string(k)?;
        self.get(v, &String::from_utf16_lossy(&t.0))
    }
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        let f = self.base.await_value(v);
        let mut h = self.clone();
        Box::pin(async move {
            let result = f.await?;
            if eq(&result, &h.st.borrow().cancel_result) {
                h.st.borrow_mut().cancel_completed = true;
            }
            if h.st.borrow().scene == "fresh-id-after-cancel"
                && eq(&result, &h.st.borrow().cancel_result)
            {
                let order = h.st.borrow().order.clone();
                h.set(&order, "id", &s("later-id"))?;
            }
            Ok(result)
        })
    }
}
pub fn classification(scene: &str) -> bool {
    [
        "tp-sell-cross",
        "tp-buy-cross",
        "tp-sell-keep",
        "tp-buy-keep",
        "sl-sell-projected-tp",
        "sl-buy-projected-tp",
        "sl-keep",
        "equal-sell",
        "equal-buy",
        "divergence-quarter",
        "divergence-over",
        "wrong-denominator",
        "last-fallback",
        "index-only",
        "no-reference",
        "missing-ticker",
        "nan-trigger",
        "zero-trigger",
        "infinite-trigger",
        "price-undefined",
        "log-error",
        "utf16-symbol",
    ]
    .contains(&scene)
}
pub fn hip(scene: &str) -> bool {
    [
        "hip-limit",
        "zero-trigger",
        "price-undefined",
        "amount-zero",
        "raw-reduce",
        "post-only",
        "cancel-error",
        "place-error",
        "dead-error",
        "store-error",
        "fresh-id-after-cancel",
        "drop-observer",
        "utf16-symbol",
    ]
    .contains(&scene)
}
pub fn initial_type(scene: &str) -> &'static str {
    if scene.starts_with("sl-") || scene == "utf16-symbol" {
        "sl"
    } else if classification(scene) {
        "tp"
    } else {
        "limit"
    }
}
pub fn side(scene: &str) -> &'static str {
    if [
        "tp-buy-cross",
        "tp-buy-keep",
        "sl-buy-projected-tp",
        "equal-buy",
    ]
    .contains(&scene)
    {
        "buy"
    } else {
        "sell"
    }
}
pub fn raw_price(scene: &str) -> N {
    match scene {
        "tp-sell-keep" | "tp-buy-cross" | "sl-sell-projected-tp" | "utf16-symbol" => N::Num(120.),
        "equal-sell" | "equal-buy" => N::Num(100.),
        "nan-trigger" => N::Num(f64::NAN),
        "zero-trigger" => N::Num(0.),
        "infinite-trigger" => N::Num(f64::INFINITY),
        "price-undefined" => N::U,
        "inherited-update" => N::Num(95.),
        _ => N::Num(90.),
    }
}
impl OrderUpdateHost for Host {
    fn update_parameter_binding(&mut self, v: &N) -> Result<(N, N), N> {
        if matches!(v, N::Null | N::U) {
            return Err(native::error(
                "TypeError",
                if matches!(v, N::Null) {
                    "Cannot destructure property 'order' of 'object null' as it is null."
                } else {
                    "Cannot destructure property 'order' of 'undefined' as it is undefined."
                },
            ));
        }
        Ok((self.get(v, "order")?, self.get(v, "update")?))
    }
    fn has_property(&mut self, v: &N, k: &str) -> Result<bool, N> {
        let N::O(o) = v else {
            return Err(native::error(
                "TypeError",
                &format!("Cannot use 'in' operator to search for '{k}' in null"),
            ));
        };
        if o.borrow().fields.iter().any(|p| p.0 == k) {
            return Ok(true);
        }
        let p = o.borrow().prototype.clone();
        if let Some(p) = p {
            self.has_property(&p, k)
        } else {
            Ok(false)
        }
    }
    fn first_rest_binding(&mut self, v: &N) -> Result<(N, N), N> {
        let N::A(a) = v else { panic!("binding array") };
        let values = a.borrow().clone();
        assert!(
            !values.is_empty(),
            "UPDATE native nonempty reference binding"
        );
        self.st.borrow_mut().reference = Some(values[0].clone());
        Ok((values[0].clone(), arr(values[1..].to_vec())))
    }
    fn update_callback(&mut self, c: UpdateCallback<N>) -> Result<N, N> {
        let key = format!("update-callback-{}", self.st.borrow().callbacks.len());
        self.st.borrow_mut().callbacks.insert(key.clone(), c);
        Ok(N::Builtin(key))
    }
}
impl Host {
    pub fn ready(&self, r: Result<N, N>) -> N {
        self.base.spawn_native(Box::pin(std::future::ready(r)))
    }
    pub fn record(&self, name: &str, args: &[N]) {
        let mut v = vec![s(name)];
        v.extend_from_slice(args);
        self.st.borrow_mut().effects.push(arr(v))
    }
    pub fn new(scene: &'static str) -> Self {
        let base = account::Host::new(object(vec![]), object(vec![]), object(vec![]));
        let mut h = base.clone();
        let symbol = if scene == "utf16-symbol" {
            N::S(JsString(vec![0xd800, 83]))
        } else {
            s("BTC/USDC")
        };
        let reduce = if scene == "raw-reduce" {
            object(vec![("keep", N::B(true))])
        } else {
            N::U
        };
        let ordinary = object(vec![
            ("id", s("old-id")),
            ("symbol", symbol.clone()),
            ("type", s(initial_type(scene))),
            ("side", s(side(scene))),
            ("price", N::Num(110.)),
            ("amount", N::Num(7.5)),
            ("reduceOnly", reduce.clone()),
            (
                "timeInForce",
                s(if scene == "post-only" { "post" } else { "gtc" }),
            ),
        ]);
        if scene == "inherited-trigger-fields" {
            let N::O(o) = &ordinary else { unreachable!() };
            o.borrow_mut().prototype = Some(object(vec![
                ("tpTriggerType", s("last")),
                ("slTriggerType", N::U),
            ]));
        }
        let order = if scene == "order-null" {
            N::Null
        } else {
            ordinary
        };
        let update = if scene == "update-null" {
            N::Null
        } else if scene == "inherited-update" {
            let v = object(vec![]);
            let N::O(o) = &v else { unreachable!() };
            o.borrow_mut().prototype =
                Some(object(vec![("price", N::Num(95.)), ("amount", N::Num(9.))]));
            v
        } else {
            let v = object(vec![("price", raw_price(scene))]);
            if scene == "amount-zero" {
                h.set(&v, "amount", &N::Num(0.)).unwrap()
            }
            v
        };
        let opts = match scene {
            "opts-null" => N::Null,
            "opts-undefined" => N::U,
            _ => object(vec![("order", order.clone()), ("update", update)]),
        };
        let ticker = object(vec![
            (
                "mark",
                if [
                    "last-fallback",
                    "index-only",
                    "no-reference",
                    "missing-ticker",
                ]
                .contains(&scene)
                {
                    N::U
                } else {
                    N::Num(100.)
                },
            ),
            (
                "last",
                N::Num(match scene {
                    "divergence-quarter" => 125.,
                    "divergence-over" => 125.01,
                    "wrong-denominator" => 130.,
                    "index-only" | "no-reference" => 0.,
                    _ => 100.,
                }),
            ),
            (
                "index",
                N::Num(if scene == "no-reference" {
                    -1.
                } else if scene == "last-fallback" {
                    110.
                } else {
                    100.
                }),
            ),
        ]);
        let market = if scene == "missing-market" {
            N::U
        } else if scene == "missing-market-id" {
            object(vec![])
        } else {
            object(vec![("id", s(if hip(scene) { "dex:BTC" } else { "BTC" }))])
        };
        let markets = object(vec![("BTC/USDC", market.clone())]);
        let tickers = object(vec![(
            "BTC/USDC",
            if scene == "missing-ticker" {
                N::U
            } else {
                ticker.clone()
            },
        )]);
        let store = account::get(&h.exchange, "store");
        h.set(&store, "marketsBySymbol", &markets).unwrap();
        h.set(&store, "tickersBySymbol", &tickers).unwrap();
        let dead = N::Set(Rc::new(RefCell::new(vec![])));
        h.set(&h.exchange.clone(), "_deadOrders", &dead).unwrap();
        Self {
            base,
            st: Rc::new(RefCell::new(State {
                scene,
                opts,
                order,
                symbol,
                reduce,
                market,
                markets,
                ticker,
                tickers,
                store,
                dead,
                marker: native::error("FixtureError", "original-error"),
                cancel_result: object(vec![("opaque", N::B(true))]),
                result: arr(vec![s("new-A"), s("new-B")]),
                effects: vec![],
                placeholders: vec![],
                placed: None,
                cancel_seen: None,
                candidates: None,
                reference: None,
                max: None,
                cancel_completed: false,
                replacement_client_order_id: None,
                callbacks: std::collections::HashMap::new(),
                settled: vec![],
            })),
        }
    }
}
