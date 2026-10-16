use super::{
    account,
    cache::{self, N, arr, eq, object, s},
    native,
};
use safe_cex_venue_hyperliquid::{
    cancellation::*,
    helper_tasks::HelperTaskHost,
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
    pub rows: N,
    pub btc: N,
    pub markets: N,
    pub original_store: N,
    pub fresh_store: N,
    pub late_store: N,
    pub xhr: N,
    pub cross: N,
    pub marker: N,
    pub signature: N,
    pub wallet: N,
    pub post_result: N,
    pub effects: Vec<N>,
    pub sign_args: Vec<N>,
    pub posted_args: Vec<N>,
    pub mapped: Vec<N>,
    pub last_key: Option<N>,
    pub removed: Option<N>,
    pub remove_receiver: Option<N>,
    pub cleared: Option<N>,
    pub set_calls: usize,
    pub map_calls: usize,
    pub side_reads: usize,
    pub callbacks: std::collections::HashMap<String, CancelCallback<N>>,
    pub iterators: Vec<N>,
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
        if eq(v, &self.base.exchange) && ["generateNonce", "suppressCancelledOrder"].contains(&k) {
            return Ok(N::Builtin(k.into()));
        }
        if eq(v, &self.st.borrow().xhr) && k == "post" {
            return Ok(N::Builtin("cancel-post".into()));
        }
        if k == "removeOrders"
            && (eq(v, &self.st.borrow().original_store) || eq(v, &self.st.borrow().fresh_store))
        {
            return Ok(N::Builtin("removeOrders".into()));
        }
        if self.st.borrow().scene == "side-reread"
            && k == "side"
            && eq(v, &account::get(&self.st.borrow().rows, "0"))
        {
            let index = self.st.borrow().side_reads;
            self.st.borrow_mut().side_reads += 1;
            return Ok(s(if index == 0 { "buy" } else { "sell" }));
        }
        WsPingHost::get(&mut self.base, v, k)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("CANCEL callee {f:?}")
        };
        let c = self.st.borrow().callbacks.get(id).cloned();
        if let Some(c) = c {
            return run_cancel_callback(self, &c, args);
        }
        let scene = self.st.borrow().scene;
        let a = args.first().cloned().unwrap_or(N::U);
        match id.as_str() {
            "string-includes" => {
                let N::S(t) = r else {
                    panic!("string receiver")
                };
                assert_eq!(args.len(), 1, "CANCEL native HIP3 includes arity");
                let N::S(needle) = &args[0] else {
                    panic!("CANCEL native HIP3 includes string needle")
                };
                assert_eq!(needle.0, [58], "CANCEL native HIP3 includes needle");
                Ok(N::B(t.0.windows(needle.0.len()).any(|w| w == needle.0)))
            }
            "String" => {
                self.record("String", args);
                if eq(&a, &self.st.borrow().marker) {
                    return Ok(s(match scene {
                        "terminal-object" | "terminal-string" | "post-denied" => TERMINAL,
                        "terminal-error" | "object-denied" => "denied-string",
                        _ => "0",
                    }));
                }
                let t = self.template_string(&a)?;
                Ok(N::S(t))
            }
            "symbolToCoin" => {
                self.record(id, args);
                Ok(a)
            }
            "generateNonce" => {
                self.record(id, args);
                Ok(N::Num(123456.))
            }
            "createWalletFromPrivateKey" => {
                self.record(id, args);
                Ok(self.st.borrow().wallet.clone())
            }
            "signL1Action" => {
                self.record(id, args);
                self.st.borrow_mut().sign_args = args.to_vec();
                Ok(self.ready(if scene == "sign-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().signature.clone())
                }))
            }
            "cancel-post" => {
                assert!(eq(r, &self.st.borrow().xhr), "CANCEL native xhr receiver");
                self.record("post", args);
                self.st.borrow_mut().posted_args = args.to_vec();
                Ok(self.ready(
                    if [
                        "terminal-error",
                        "terminal-string",
                        "terminal-object",
                        "post-denied",
                        "object-denied",
                        "post-false",
                    ]
                    .contains(&scene)
                    {
                        Err(self.st.borrow().marker.clone())
                    } else {
                        Ok(self.st.borrow().post_result.clone())
                    },
                ))
            }
            "isOnlyHyperliquidTerminalCancelError" => {
                self.record(id, args);
                let text = if let N::S(t) = &a {
                    String::from_utf16(t.0.as_slice()).ok()
                } else {
                    None
                };
                Ok(N::B(text.as_deref().is_some_and(safe_cex_venue_hyperliquid::order_response::is_only_hyperliquid_terminal_cancel_error)))
            }
            "suppressCancelledOrder" => {
                self.record(id, args);
                if scene == "suppress-error" && eq(&a, &s("202")) {
                    return Err(self.st.borrow().marker.clone());
                }
                Ok(self.st.borrow().post_result.clone())
            }
            "getCrossCache" => {
                let key = self.st.borrow().last_key.clone().unwrap();
                let cleared = self.st.borrow().cleared.clone().unwrap();
                assert!(
                    matches!(cleared,N::Set(v)if v.borrow().iter().any(|v|eq(v,&key))),
                    "CANCEL native Set.add before cache callee"
                );
                self.record(id, args);
                Ok(self.st.borrow().cross.clone())
            }
            "clearPrePlacement" => {
                assert!(
                    eq(r, &self.st.borrow().cross),
                    "CANCEL native cross cache receiver"
                );
                self.record(id, args);
                if scene == "clear-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                Ok(self.st.borrow().post_result.clone())
            }
            "removeOrders" => {
                self.record(id, args);
                self.st.borrow_mut().remove_receiver = Some(r.clone());
                self.st.borrow_mut().removed = Some(a);
                if scene == "remove-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().post_result.clone())
                }
            }
            "has" | "add" => {
                if id == "has" {
                    self.st.borrow_mut().last_key = Some(a.clone());
                }
                self.record(if id == "add" { "set-add" } else { "set-has" }, args);
                safe_cex_venue_hyperliquid::cross_cache::CrossCacheHost::call(
                    &mut cache::Host::default(),
                    f,
                    r,
                    args,
                )
                .map_err(|e| s(&e))
            }
            "map" => {
                let index = self.st.borrow().map_calls;
                self.st.borrow_mut().map_calls += 1;
                if scene == "wire-map-error" && index == 0
                    || scene == "remove-map-error" && index == 1
                {
                    return Err(self.st.borrow().marker.clone());
                }
                if scene == "remove-callee" && index == 1 {
                    let store = self.st.borrow().late_store.clone();
                    self.set(&self.base.exchange.clone(), "store", &store)?;
                }
                let N::A(a) = r else { panic!("dense map") };
                let rows = a.borrow().clone();
                let mut mapped = vec![];
                for (i, row) in rows.iter().enumerate() {
                    mapped.push(self.call(
                        &args[0],
                        &N::U,
                        &[row.clone(), N::Num(i as f64), r.clone()],
                    )?)
                }
                let mapped = arr(mapped);
                self.st.borrow_mut().mapped.push(mapped.clone());
                Ok(mapped)
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
        Ok(if k == "ENDPOINTS" {
            object(vec![("EXCHANGE", s("/exchange"))])
        } else {
            N::Builtin(k.into())
        })
    }
    fn construct(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        if matches!(f,N::Builtin(k)if k=="Set") {
            self.st.borrow_mut().set_calls += 1;
            if self.st.borrow().scene == "set-error" {
                return Err(self.st.borrow().marker.clone());
            }
            assert!(
                args.is_empty(),
                "CANCEL native empty Set constructor arguments"
            );
            let set = N::Set(Rc::new(RefCell::new(vec![])));
            self.st.borrow_mut().cleared = Some(set.clone());
            return Ok(set);
        }
        SpotAccountHost::construct(&mut self.base, f, args)
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        if self.st.borrow().scene == "utf16-symbol"
            && eq(v, &self.st.borrow().markets)
            && matches!(k,N::S(t)if t.0==[0xd800,83])
        {
            return Ok(self.st.borrow().btc.clone());
        }
        let t = self.template_string(k)?;
        self.get(v, &String::from_utf16_lossy(&t.0))
    }
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        let f = self.base.await_value(v);
        let mut h = self.clone();
        Box::pin(async move {
            let value = f.await?;
            if h.st.borrow().scene == "fresh-store" && eq(&value, &h.st.borrow().post_result) {
                let store = h.st.borrow().fresh_store.clone();
                h.set(&h.base.exchange.clone(), "store", &store)?;
            }
            Ok(value)
        })
    }
}
pub const TERMINAL: &str = "Order was never placed, already canceled, or filled. asset=0";
pub fn hip(scene: &str) -> bool {
    [
        "hip3",
        "opposite-side",
        "terminal-error",
        "terminal-string",
        "terminal-object",
        "suppress-error",
        "clear-error",
        "next-clear-error",
        "set-error",
        "remove-error",
        "remove-map-error",
        "fresh-store",
        "side-reread",
        "drop-observer",
        "utf16-symbol",
        "vault",
        "testnet",
        "remove-callee",
    ]
    .contains(&scene)
}
impl CancellationHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v)))
    }
    fn instance_of(&mut self, v: &N, class: &N) -> Result<bool, N> {
        assert!(
            matches!(class,N::Builtin(k) if k=="Error"),
            "CANCEL native Error constructor selector"
        );
        self.record("instanceof", &[]);
        Ok(eq(v, &self.st.borrow().marker)
            && ["terminal-error", "post-denied"].contains(&self.st.borrow().scene))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        assert!(
            eq(v, &self.st.borrow().rows),
            "CANCEL native original iterable"
        );
        let N::A(a) = v else { panic!("array") };
        let i = N::Iter(Rc::new(RefCell::new((a.borrow().clone(), 0))));
        self.st.borrow_mut().iterators.push(i.clone());
        Ok(i)
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else { panic!("iterator") };
        let index = self
            .st
            .borrow()
            .iterators
            .iter()
            .position(|x| matches!(x,N::Iter(j)if Rc::ptr_eq(i,j)))
            .unwrap();
        let mut i = i.borrow_mut();
        let scene = self.st.borrow().scene;
        if i.1 == 1
            && (scene == "next-suppress-error" && index == 0
                || scene == "next-clear-error" && index == 1)
        {
            return Err(self.st.borrow().marker.clone());
        }
        let r = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(r)
    }
    fn close_after_throw(&mut self, v: &N) {
        let N::Iter(i) = v else { panic!("iterator") };
        let index = self
            .st
            .borrow()
            .iterators
            .iter()
            .position(|x| matches!(x,N::Iter(j)if Rc::ptr_eq(i,j)))
            .unwrap();
        self.record("close", &[N::Num(index as f64)])
    }
    fn cancel_callback(&mut self, c: CancelCallback<N>) -> Result<N, N> {
        let key = format!("cancel-callback-{}", self.st.borrow().callbacks.len());
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
        let is_hip = hip(scene);
        let first_symbol = if scene == "utf16-symbol" {
            N::S(JsString(vec![0xd800, 83]))
        } else {
            s("BTC/USDC")
        };
        let first = object(vec![
            (
                "id",
                match scene {
                    "invalid-id" => s("bad"),
                    "negative-zero-id" => s("-0"),
                    "infinity-id" => s("Infinity"),
                    _ => s("101"),
                },
            ),
            ("symbol", first_symbol.clone()),
            ("side", s("buy")),
        ]);
        let second = object(vec![
            ("id", s("202")),
            ("symbol", s("ETH/USDC")),
            ("side", s("sell")),
        ]);
        let third = object(vec![
            ("id", s("303")),
            ("symbol", first_symbol.clone()),
            (
                "side",
                s(if scene == "opposite-side" {
                    "sell"
                } else {
                    "buy"
                }),
            ),
        ]);
        let rows = arr(if scene == "empty" {
            vec![]
        } else {
            vec![first, second, third]
        });
        let btc = object(vec![
            ("id", s(if is_hip { "dex:BTC" } else { "BTC" })),
            ("symbol", s("market-display-BTC")),
            (
                "info",
                if scene == "missing-info" {
                    N::U
                } else {
                    object(vec![(
                        "hyperliquidId",
                        if scene == "null-asset" {
                            N::Null
                        } else {
                            N::Num(73.)
                        },
                    )])
                },
            ),
        ]);
        let eth = object(vec![
            ("id", s(if is_hip { "dex:ETH" } else { "ETH" })),
            ("symbol", s("market-display-ETH")),
            ("info", object(vec![("hyperliquidId", N::Num(91.))])),
        ]);
        // The raw UTF-16 lookup key is supplied by the native index override below.
        let markets = object(vec![
            (
                "BTC/USDC",
                if scene == "missing-market" {
                    N::U
                } else {
                    btc.clone()
                },
            ),
            ("ETH/USDC", eth),
        ]);
        let original_store = account::get(&h.exchange, "store");
        h.set(&original_store, "marketsBySymbol", &markets).unwrap();
        let fresh_store = object(vec![(
            "marketsBySymbol",
            object(vec![
                (
                    "BTC/USDC",
                    object(vec![
                        ("id", s("dex:LATER")),
                        ("symbol", s("later-market-display")),
                    ]),
                ),
                ("ETH/USDC", object(vec![("id", s("ETH"))])),
            ]),
        )]);
        let late_store = object(vec![]);
        let xhr = object(vec![]);
        h.set(&h.exchange.clone(), "xhr", &xhr).unwrap();
        let options = object(vec![
            ("secret", s("public-fixture-key")),
            (
                "vault",
                if scene == "vault" {
                    s("public-fixture-vault")
                } else {
                    N::U
                },
            ),
            ("testnet", N::B(scene == "testnet")),
        ]);
        h.set(&h.exchange.clone(), "options", &options).unwrap();
        let marker = match scene {
            "terminal-string" => s(TERMINAL),
            "terminal-object" | "object-denied" => object(vec![("message", s(TERMINAL))]),
            "post-false" => N::Num(0.),
            _ => native::error(
                "FixtureError",
                if scene == "terminal-error" {
                    TERMINAL
                } else {
                    "denied-error"
                },
            ),
        };
        let cross = object(vec![(
            "clearPrePlacement",
            N::Builtin("clearPrePlacement".into()),
        )]);
        Self {
            base,
            st: Rc::new(RefCell::new(State {
                scene,
                rows,
                btc,
                markets,
                original_store,
                fresh_store,
                late_store,
                xhr,
                cross,
                marker,
                signature: object(vec![
                    ("r", s("sig-r")),
                    ("s", s("sig-s")),
                    ("v", N::Num(28.)),
                ]),
                wallet: object(vec![("fixture", N::B(true))]),
                post_result: object(vec![("opaque", N::B(true))]),
                effects: vec![],
                sign_args: vec![],
                posted_args: vec![],
                mapped: vec![],
                last_key: None,
                removed: None,
                remove_receiver: None,
                cleared: None,
                set_calls: 0,
                map_calls: 0,
                side_reads: 0,
                callbacks: std::collections::HashMap::new(),
                iterators: vec![],
                settled: vec![],
            })),
        }
    }
}
