use super::{
    account,
    cache::{self, N, arr, eq, object, s},
    native,
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    limit_orders::*,
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
    pub market: N,
    pub later: N,
    pub marker: N,
    pub ids: N,
    pub builder: N,
    pub signature: N,
    pub wallet: N,
    pub response: N,
    pub requests: Vec<N>,
    pub wires: Vec<N>,
    pub actions: Vec<N>,
    pub payloads: Vec<N>,
    pub orders: Vec<N>,
    pub effects: Vec<N>,
    pub settled: Vec<Result<N, N>>,
    pub sign_args: Vec<N>,
    pub mints: usize,
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
        if eq(v, &self.base.exchange)
            && [
                "createLimitOrderWires",
                "getBuilderInfo",
                "generateNonce",
                "priceToPrecision",
                "resolveReduceOnly",
                "recordPlacement",
                "addToPrePlacementCache",
                "postOrderAction",
                "validateResponseAndExtractOrderIds",
                "rememberLimitOrderTimeInForce",
                "addPendingHip3Order",
                "refreshHip3OrdersWithRetry",
            ]
            .contains(&k)
        {
            return Ok(N::Builtin(k.into()));
        }
        if eq(v, &account::get(&self.base.exchange, "store")) && k == "addOrUpdateOrder" {
            return Ok(N::Builtin(k.into()));
        }
        WsPingHost::get(&mut self.base, v, k)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("LIMIT unexpected function {f:?}")
        };
        let scene = self.st.borrow().scene;
        let a = args.first().cloned().unwrap_or(N::U);
        match id.as_str() {
            "Boolean" => Ok(N::B(self.truthy(&a))),
            "push" => {
                let N::A(items) = r else {
                    panic!("LIMIT push array")
                };
                items.borrow_mut().extend_from_slice(args);
                Ok(N::Num(items.borrow().len() as f64))
            }
            "string-includes" => {
                let N::S(t) = r else {
                    panic!("string receiver")
                };
                assert_eq!(args.len(), 1, "LIMIT native HIP3 includes arity");
                let N::S(needle) = &args[0] else {
                    panic!("LIMIT native HIP3 includes string needle")
                };
                assert_eq!(needle.0, [58], "LIMIT native HIP3 includes needle");
                Ok(N::B(t.0.windows(needle.0.len()).any(|w| w == needle.0)))
            }
            "symbolToCoin" => {
                self.record(id, args);
                Ok(if scene == "utf16" {
                    N::S(JsString(vec![0xdfff, 67]))
                } else {
                    s("coin")
                })
            }
            "adjust" => {
                self.record(id, args);
                Ok(N::Num(7.125))
            }
            "orderRequestToOrderWire" => {
                assert!(matches!(r, N::U), "LIMIT wire imported receiver");
                let w = object(vec![
                    ("request", a.clone()),
                    ("assetIndex", args[1].clone()),
                ]);
                let mut st = self.st.borrow_mut();
                st.requests.push(a);
                st.wires.push(w.clone());
                Ok(w)
            }
            "priceToPrecision" => {
                assert!(eq(r, &self.base.exchange), "LIMIT precision receiver");
                self.record(id, args);
                Ok(args[1].clone())
            }
            "resolveReduceOnly" => {
                assert!(eq(r, &self.base.exchange), "LIMIT reduce receiver");
                assert!(
                    eq(&a, &self.st.borrow().market),
                    "LIMIT native reduce original market"
                );
                self.record(id, args);
                safe_cex_venue_hyperliquid::account_position_leaves::resolve_reduce_only(
                    self, &a, &args[1],
                )
            }
            "createLimitOrderWires" => {
                assert!(eq(r, &self.base.exchange), "LIMIT wires receiver");
                assert!(eq(&a, &self.st.borrow().opts), "LIMIT opts alias");
                let cloid = args.get(1).cloned().unwrap_or(N::U);
                create_limit_order_wires(self, r, &a, &cloid)
            }
            "mintHyperliquidCloid" => {
                self.record(id, args);
                let n = {
                    let mut st = self.st.borrow_mut();
                    st.mints += 1;
                    st.mints
                };
                Ok(s(&format!("0xmint{n}")))
            }
            "getBuilderInfo" => {
                self.record(id, args);
                Ok(self.ready(if scene == "builder-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().builder.clone())
                }))
            }
            "orderWiresToOrderAction" | "orderWiresToNormalTPSLAction" => {
                self.record(id, args);
                let v = object(vec![
                    ("kind", s(id)),
                    ("orders", a),
                    ("builder", args[1].clone()),
                ]);
                self.st.borrow_mut().actions.push(v.clone());
                Ok(v)
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
            "postOrderAction" => {
                self.record(id, args);
                self.st.borrow_mut().payloads.push(a);
                Ok(self.ready(if scene == "post-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().response.clone())
                }))
            }
            "validateResponseAndExtractOrderIds" => {
                assert!(eq(&a, &self.st.borrow().response), "LIMIT response alias");
                self.record(id, args);
                if scene == "validate-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().ids.clone())
                }
            }
            "addToPrePlacementCache"
            | "recordPlacement"
            | "rememberLimitOrderTimeInForce"
            | "addPendingHip3Order"
            | "refreshHip3OrdersWithRetry"
            | "addOrUpdateOrder" => {
                self.record(id, args);
                if id == "addPendingHip3Order" && scene == "pending-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                if id == "addOrUpdateOrder" {
                    self.st.borrow_mut().orders.push(a)
                }
                Ok(self.st.borrow().marker.clone())
            }
            "getCurrentTimeMs" => Ok(N::Num(987654.)),
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
        Ok(match k {
            "OrderType" => object(vec![("Limit", s("limit"))]),
            "OrderSide" => object(vec![("Buy", s("buy"))]),
            "OrderTimeInForce" => {
                object(vec![("PostOnly", s("post")), ("GoodTillCancel", s("gtc"))])
            }
            "OrderStatus" => object(vec![("Creating", s("creating"))]),
            "virtualClock" => object(vec![(
                "getCurrentTimeMs",
                N::Builtin("getCurrentTimeMs".into()),
            )]),
            _ => N::Builtin(k.into()),
        })
    }
    fn construct(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(&mut self.base, f, args)
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        let t = self.template_string(k)?;
        self.get(v, &String::from_utf16_lossy(&t.0))
    }
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        let f = self.base.await_value(v);
        let mut h = self.clone();
        Box::pin(async move {
            let result = f.await?;
            let scene = h.st.borrow().scene;
            if scene == "fresh-after-builder" && eq(&result, &h.st.borrow().builder) {
                let opts = h.st.borrow().opts.clone();
                h.set(&opts, "takeProfit", &N::Num(88.))?;
            }
            if scene == "fresh-market" && eq(&result, &h.st.borrow().signature) {
                let later = h.st.borrow().later.clone();
                let markets =
                    account::get(&account::get(&h.base.exchange, "store"), "marketsBySymbol");
                h.set(&markets, "BTC/USDC", &later)?;
            }
            if scene == "fresh-market" && eq(&result, &h.st.borrow().response) {
                let markets =
                    account::get(&account::get(&h.base.exchange, "store"), "marketsBySymbol");
                h.set(
                    &markets,
                    "BTC/USDC",
                    &object(vec![("id", s("other")), ("symbol", s("replaced-again"))]),
                )?;
            }
            Ok(result)
        })
    }
}
impl LimitOrderHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v)))
    }
    fn first_binding(&mut self, v: &N) -> Result<N, N> {
        self.record("binding", &[]);
        if self.st.borrow().scene == "binding-error" {
            return Err(self.st.borrow().marker.clone());
        }
        let N::A(a) = v else { panic!("ids array") };
        Ok(a.borrow().first().cloned().unwrap_or(N::U))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("ids array") };
        Ok(N::Iter(Rc::new(RefCell::new((a.borrow().clone(), 0)))))
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else { panic!("iterator") };
        let mut i = i.borrow_mut();
        if self.st.borrow().scene == "next-error" && i.1 == 1 {
            return Err(self.st.borrow().marker.clone());
        }
        let result = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(result)
    }
    fn close_after_throw(&mut self, _: &N) {
        self.record("close", &[])
    }
}
impl safe_cex_venue_hyperliquid::account_position_leaves::AccountPositionHost for Host {
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
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        eq(a, b)
    }
    fn greater_equal(&mut self, a: &N, b: &N) -> Result<bool, N> {
        Ok(cache::num(a) >= cache::num(b))
    }
    fn opposite_predicate(
        &mut self,
        _: safe_cex_venue_hyperliquid::account_position_leaves::OppositePredicate<N>,
    ) -> Result<N, N> {
        panic!("unused")
    }
}
impl Host {
    pub fn ready(&self, r: Result<N, N>) -> N {
        self.base.spawn_native(Box::pin(std::future::ready(r)))
    }
    pub fn record(&self, name: &str, args: &[N]) {
        let mut values = vec![s(name)];
        values.extend_from_slice(args);
        self.st.borrow_mut().effects.push(arr(values))
    }
    pub fn new(scene: &'static str) -> Self {
        let base = account::Host::new(object(vec![]), object(vec![]), object(vec![]));
        let mut h = base.clone();
        let market = object(vec![
            (
                "id",
                s(
                    if [
                        "hip3",
                        "post-only",
                        "both",
                        "empty",
                        "falsy-first",
                        "binding-error",
                        "pending-error",
                        "next-error",
                        "drop-observer",
                    ]
                    .contains(&scene)
                    {
                        "dex:BTC"
                    } else {
                        "BTC"
                    },
                ),
            ),
            (
                "symbol",
                if scene == "utf16" {
                    N::S(JsString(vec![0xd800, 83]))
                } else {
                    s("BTC/USDC")
                },
            ),
            ("info", object(vec![("hyperliquidId", N::Num(73.))])),
            ("precision", object(vec![("amount", N::Num(0.01))])),
            ("isSpot", N::B(scene.starts_with("spot"))),
        ]);
        let opts = object(vec![
            ("symbol", s("BTC/USDC")),
            (
                "type",
                s(if scene == "wrong-type" {
                    "market"
                } else {
                    "limit"
                }),
            ),
            ("side", s(if scene == "sell" { "sell" } else { "buy" })),
            ("amount", N::Num(7.5)),
            (
                "price",
                N::Num(match scene {
                    "zero" => 0.,
                    "negative-zero" => -0.,
                    "nan" => f64::NAN,
                    "infinity" => f64::INFINITY,
                    _ => 101.25,
                }),
            ),
            (
                "timeInForce",
                s(if scene == "post-only" { "post" } else { "gtc" }),
            ),
            (
                "reduceOnly",
                N::B(["reduce", "spot-reduce"].contains(&scene)),
            ),
            (
                "stopLoss",
                N::Num(if ["both", "sl", "spot-both"].contains(&scene) {
                    90.5
                } else {
                    0.
                }),
            ),
            (
                "takeProfit",
                N::Num(if ["both", "tp", "spot-both"].contains(&scene) {
                    120.75
                } else {
                    0.
                }),
            ),
        ]);
        let store = account::get(&h.exchange, "store");
        let markets = object(vec![]);
        if scene != "missing-market" {
            h.set(&markets, "BTC/USDC", &market).unwrap()
        }
        h.set(&store, "marketsBySymbol", &markets).unwrap();
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
        let ids = arr(if scene == "empty" {
            vec![]
        } else {
            vec![
                if scene == "falsy-first" {
                    s("")
                } else {
                    s("id-A")
                },
                s("id-B"),
            ]
        });
        let later = object(vec![("id", s("dex:LATER")), ("symbol", s("later-symbol"))]);
        let marker = native::error("FixtureError", "original\u{d7ff}");
        Self {
            base,
            st: Rc::new(RefCell::new(State {
                scene,
                opts,
                market,
                later,
                marker,
                ids,
                builder: object(vec![("fee", N::Num(19.))]),
                signature: object(vec![
                    ("r", s("sig-r")),
                    ("s", s("sig-s")),
                    ("v", N::Num(28.)),
                ]),
                wallet: object(vec![("fixture", N::B(true))]),
                response: object(vec![("code", N::Num(55.))]),
                requests: vec![],
                wires: vec![],
                actions: vec![],
                payloads: vec![],
                orders: vec![],
                effects: vec![],
                settled: vec![],
                sign_args: vec![],
                mints: 0,
            })),
        }
    }
}
