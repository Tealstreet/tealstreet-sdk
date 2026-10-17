// Finite source-semantics host. Production import registration is tested through
// HyperliquidExchange in binding.rs; unknown names must never auto-resolve here.
use super::{
    account,
    cache::{self, N, arr, eq, object, s},
    native,
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    placement_dispatch::*,
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
    pub rows: N,
    pub marker: N,
    pub ids: N,
    pub builder: N,
    pub signature: N,
    pub wallet: N,
    pub response: N,
    pub replacement: N,
    pub requests: Vec<N>,
    pub wires: Vec<N>,
    pub actions: Vec<N>,
    pub payloads: Vec<N>,
    pub orders: Vec<N>,
    pub effects: Vec<N>,
    pub settled: Vec<Result<N, N>>,
    pub sign_args: Vec<N>,
    pub callback: Option<SerialOrderCallback<N>>,
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
        assert!(
            matches!(
                k,
                "Boolean"
                    | "Number"
                    | "String"
                    | "Error"
                    | "Array"
                    | "Set"
                    | "OrderType"
                    | "OrderSide"
                    | "OrderTimeInForce"
                    | "OrderStatus"
                    | "PositionSide"
                    | "HyperliquidExchange"
                    | "HyperliquidSpotExchange"
                    | "ENDPOINTS"
                    | "virtualClock"
                    | "resolveHyperliquidBuilder"
                    | "maskHyperliquidAddress"
                    | "HyperliquidUnreadableOrderResponseError"
                    | "signL1Action"
                    | "createWalletFromPrivateKey"
            ),
            "unreviewed placement-test global {k}"
        );
        Ok(N::Builtin(k.into()))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if matches!(v, N::S(_)) && k == "includes" {
            return Ok(N::Builtin("string-includes".into()));
        }
        if eq(v, &self.base.exchange) {
            if k == "placeOrder" {
                let actual = account::get(v, k);
                return Ok(if matches!(actual, N::U) {
                    N::Builtin(k.into())
                } else {
                    actual
                });
            }
            if [
                "resolveCallerClientOrderId",
                "assertCallerClientOrderIds",
                "getOrderBasePrice",
                "shouldUsePositionTpsl",
                "getBuilderInfo",
                "generateNonce",
                "priceToPrecision",
                "resolveReduceOnly",
                "placeLimitOrder",
                "placeOrderWithCloid",
                "recordPlacement",
                "addToPrePlacementCache",
                "postOrderAction",
                "validateResponseAndExtractOrderIds",
                "rememberLimitOrderTimeInForce",
                "addPendingHip3Order",
                "refreshHip3OrdersWithRetry",
                "responseHasDeferredOrderStatus",
                "scheduleOrderRefresh",
            ]
            .contains(&k)
            {
                return Ok(N::Builtin(k.into()));
            }
        }
        if eq(v, &account::get(&self.base.exchange, "store")) && k == "addOrUpdateOrder" {
            return Ok(N::Builtin(k.into()));
        }
        WsPingHost::get(&mut self.base, v, k)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("DISPATCH unexpected callee {f:?}")
        };
        let scene = self.st.borrow().scene;
        let first = args.first().cloned().unwrap_or(N::U);
        if id == "serial-callback" {
            let c = self.st.borrow().callback.clone().unwrap();
            return run_serial_order_callback(self, &c, args);
        }
        match id.as_str() {
            "Boolean" => Ok(N::B(self.truthy(&first))),
            "Number" => Ok(N::Num(cache::num(&first))),
            "string-includes" => {
                let N::S(t) = r else {
                    panic!("string receiver")
                };
                assert_eq!(args.len(), 1, "PLACE native HIP3 includes arity");
                let N::S(needle) = &args[0] else {
                    panic!("PLACE native HIP3 includes string needle")
                };
                assert_eq!(needle.0, [58], "PLACE native HIP3 includes needle");
                Ok(N::B(t.0.windows(needle.0.len()).any(|w| w == needle.0)))
            }
            "symbolToCoin" => {
                self.record(id, args);
                Ok(s("coin"))
            }
            "adjust" => {
                self.record(id, args);
                Ok(N::Num(7.125))
            }
            "isFinitePositiveNumber" => {
                self.record(id, args);
                let n = cache::num(&first);
                Ok(N::B(n.is_finite() && n > 0.))
            }
            "orderRequestToOrderWire" => {
                assert!(matches!(r, N::U), "DISPATCH imported receiver");
                let w = object(vec![
                    ("request", first.clone()),
                    ("assetIndex", args[1].clone()),
                ]);
                let mut st = self.st.borrow_mut();
                st.requests.push(first);
                st.wires.push(w.clone());
                Ok(w)
            }
            "priceToPrecision" => {
                assert!(eq(r, &self.base.exchange), "DISPATCH precision receiver");
                self.record(id, args);
                Ok(args[1].clone())
            }
            "resolveReduceOnly" => {
                assert!(eq(r, &self.base.exchange), "DISPATCH reduce receiver");
                self.record(id, args);
                safe_cex_venue_hyperliquid::account_position_leaves::resolve_reduce_only(
                    self, &first, &args[1],
                )
            }
            "resolveCallerClientOrderId" => {
                self.record(id, args);
                if scene == "serial-first-error"
                    || scene == "serial-second-error" && eq(&account::get(&first, "label"), &s("B"))
                {
                    return Err(self.st.borrow().marker.clone());
                }
                Ok(self.st.borrow().marker.clone())
            }
            "assertCallerClientOrderIds" => {
                self.record(id, args);
                if scene == "serial-guard-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().marker.clone())
                }
            }
            "placeOrder" => {
                self.record(id, args);
                let observer = place_order(self, r, &first);
                Ok(self.base.promise(observer))
            }
            // The actual source arrow (not a fixture): run the Rust body as the class would.
            "placeOrderWithCloid" => {
                let cloid = args.get(1).cloned().unwrap_or(N::U);
                let observer = place_order_with_cloid(self, r, &first, &cloid);
                Ok(self.base.promise(observer))
            }
            "mintHyperliquidCloid" => {
                self.record(id, args);
                Ok(s("0xmint"))
            }
            "replacement-place" => {
                self.record(id, args);
                Ok(self.ready(Ok(self.st.borrow().replacement.clone())))
            }
            "placeLimitOrder" => {
                self.record(id, args);
                if scene == "serial-replaced" {
                    self.set(
                        &self.base.exchange.clone(),
                        "placeOrder",
                        &N::Builtin("replacement-place".into()),
                    )?;
                }
                let value = account::get(&first, "ids");
                Ok(self.ready(Ok(if matches!(value, N::U) {
                    self.st.borrow().ids.clone()
                } else {
                    value
                })))
            }
            "getOrderBasePrice" => {
                self.record(id, args);
                Ok(N::Num(200.))
            }
            "shouldUsePositionTpsl" => {
                self.record(id, args);
                Ok(N::B(["tp-zero", "tp-full"].contains(&scene)))
            }
            "getBuilderInfo" => {
                self.record(id, args);
                Ok(self.ready(Ok(self.st.borrow().builder.clone())))
            }
            "orderWiresToOrderAction" | "orderWiresToPositionalTPSLAction" => {
                self.record(id, args);
                let action = object(vec![
                    ("kind", s(id)),
                    ("orders", first),
                    ("builder", args[1].clone()),
                ]);
                self.st.borrow_mut().actions.push(action.clone());
                Ok(action)
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
                self.st.borrow_mut().payloads.push(first);
                Ok(self.ready(if scene == "post-error" {
                    Err(self.st.borrow().marker.clone())
                } else {
                    Ok(self.st.borrow().response.clone())
                }))
            }
            "validateResponseAndExtractOrderIds" => {
                assert!(
                    eq(&first, &self.st.borrow().response),
                    "DISPATCH response alias"
                );
                self.record(id, args);
                Ok(self.st.borrow().ids.clone())
            }
            "responseHasDeferredOrderStatus" => {
                assert!(eq(&first, &self.st.borrow().response));
                self.record(id, args);
                Ok(N::B(scene == "tp-deferred"))
            }
            "addToPrePlacementCache"
            | "recordPlacement"
            | "rememberLimitOrderTimeInForce"
            | "addPendingHip3Order"
            | "refreshHip3OrdersWithRetry"
            | "addOrUpdateOrder"
            | "scheduleOrderRefresh" => {
                self.record(id, args);
                if id == "addPendingHip3Order" && scene == "tp-body-error" {
                    return Err(self.st.borrow().marker.clone());
                }
                if id == "addOrUpdateOrder" {
                    self.st.borrow_mut().orders.push(first)
                }
                Ok(self.st.borrow().marker.clone())
            }
            "getCurrentTimeMs" => Ok(N::Num(987654.)),
            "mapSeries" => {
                assert!(
                    eq(&first, &self.st.borrow().rows),
                    "DISPATCH serial input alias"
                );
                let N::A(a) = &first else {
                    panic!("dense array fixture")
                };
                let rows = a.borrow().clone();
                let callback = args[1].clone();
                let mut h = self.clone();
                Ok(self.base.spawn_native(Box::pin(async move {
                    let mut result = vec![];
                    for (i, row) in rows.iter().enumerate() {
                        let value = h.call(
                            &callback,
                            &N::U,
                            &[row.clone(), N::Num(i as f64), first.clone()],
                        )?;
                        result.push(h.await_value(value).await?);
                    }
                    Ok(arr(result))
                })))
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
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        SpotAccountHost::await_value(&mut self.base, v)
    }
    fn spawn_native(&self, body: AccountFuture<N, N>) -> N {
        SpotAccountHost::spawn_native(&self.base, body)
    }
    fn imported(&mut self, k: &str) -> Result<N, N> {
        Ok(match k {
            "OrderType" => object(vec![
                ("Limit", s("limit")),
                ("Market", s("market")),
                ("TakeProfit", s("tp")),
                ("StopLoss", s("sl")),
                ("TrailingStopLoss", s("trailing")),
            ]),
            "OrderSide" => object(vec![("Buy", s("buy")), ("Sell", s("sell"))]),
            "OrderTimeInForce" => {
                object(vec![("PostOnly", s("post")), ("GoodTillCancel", s("gtc"))])
            }
            "OrderStatus" => object(vec![("Creating", s("creating"))]),
            "virtualClock" => object(vec![(
                "getCurrentTimeMs",
                N::Builtin("getCurrentTimeMs".into()),
            )]),
            "symbolToCoin"
            | "adjust"
            | "isFinitePositiveNumber"
            | "orderRequestToOrderWire"
            | "orderWiresToOrderAction"
            | "orderWiresToPositionalTPSLAction"
            | "signL1Action"
            | "createWalletFromPrivateKey"
            | "mintHyperliquidCloid"
            | "mapSeries"
            | "isOnlyHyperliquidTerminalCancelError" => N::Builtin(k.into()),
            _ => panic!("unreviewed placement-test import {k}"),
        })
    }
    fn construct(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(&mut self.base, f, args)
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        let t = self.template_string(k)?;
        self.get(v, &String::from_utf16_lossy(&t.0))
    }
}
impl PlacementHost for Host {
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v)))
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("ids array") };
        Ok(N::Iter(Rc::new(RefCell::new((a.borrow().clone(), 0)))))
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else { panic!("iterator") };
        let mut i = i.borrow_mut();
        if self.st.borrow().scene == "tp-next-error" && i.1 == 1 {
            return Err(self.st.borrow().marker.clone());
        }
        let r = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(r)
    }
    fn close_after_throw(&mut self, _: &N) {
        self.record("close", &[])
    }
    fn serial_order_callback(&mut self, c: SerialOrderCallback<N>) -> Result<N, N> {
        self.st.borrow_mut().callback = Some(c);
        Ok(N::Builtin("serial-callback".into()))
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
                        "tp-hip3",
                        "tp-deferred",
                        "tp-next-error",
                        "tp-body-error",
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
                if scene == "tp-utf16" {
                    N::S(JsString(vec![0xd800, 83]))
                } else {
                    s("BTC/USDC")
                },
            ),
            ("info", object(vec![("hyperliquidId", N::Num(73.))])),
            ("precision", object(vec![("amount", N::Num(0.01))])),
            ("isSpot", N::B(scene == "market-spot")),
        ]);
        let kind = if scene == "limit" || scene.starts_with("serial-") {
            "limit"
        } else if scene == "wrong-type" {
            "unknown"
        } else if scene == "trailing" {
            "trailing"
        } else if scene.starts_with("tp") || scene == "drop-observer" {
            "tp"
        } else if scene.starts_with("sl") {
            "sl"
        } else {
            "market"
        };
        let opts = object(vec![
            ("symbol", s("BTC/USDC")),
            ("type", s(kind)),
            (
                "side",
                s(if scene == "market-sell" {
                    "sell"
                } else {
                    "buy"
                }),
            ),
            ("amount", N::Num(if scene == "tp-zero" { 0. } else { 7.5 })),
            (
                "price",
                N::Num(if ["tp-invalid", "market-zero"].contains(&scene) {
                    0.
                } else if scene == "sl-nan" {
                    f64::NAN
                } else {
                    101.25
                }),
            ),
            (
                "reduceOnly",
                N::B(["tp-full", "market-spot"].contains(&scene)),
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
                if scene == "market-vault" {
                    s("public-fixture-vault")
                } else {
                    N::U
                },
            ),
            ("testnet", N::B(scene == "market-zero")),
        ]);
        h.set(&h.exchange.clone(), "options", &options).unwrap();
        let ids = arr(if ["market-empty", "tp-deferred"].contains(&scene) {
            vec![]
        } else {
            vec![s("id-A"), s("id-B")]
        });
        let first = object(vec![
            ("symbol", s("BTC/USDC")),
            ("type", s("limit")),
            ("label", s("A")),
            (
                "ids",
                arr(if scene == "serial-nested" {
                    vec![s("A"), arr(vec![s("nested")])]
                } else {
                    vec![s("A1"), s("A2")]
                }),
            ),
        ]);
        let second = object(vec![
            ("symbol", s("BTC/USDC")),
            ("type", s("limit")),
            ("label", s("B")),
            (
                "ids",
                arr(if scene == "serial-nested" {
                    vec![]
                } else {
                    vec![s("B1"), s("B2")]
                }),
            ),
        ]);
        let rows = arr(if scene == "serial-empty" {
            vec![]
        } else if scene == "serial-one" {
            vec![first]
        } else {
            vec![first, second]
        });
        Self {
            base,
            st: Rc::new(RefCell::new(State {
                scene,
                opts,
                market,
                rows,
                marker: native::error("FixtureError", "original\u{d7ff}"),
                ids,
                builder: object(vec![("fee", N::Num(19.))]),
                signature: object(vec![
                    ("r", s("sig-r")),
                    ("s", s("sig-s")),
                    ("v", N::Num(28.)),
                ]),
                wallet: object(vec![("fixture", N::B(true))]),
                response: object(vec![("code", N::Num(55.))]),
                replacement: arr(vec![s("replacement")]),
                requests: vec![],
                wires: vec![],
                actions: vec![],
                payloads: vec![],
                orders: vec![],
                effects: vec![],
                settled: vec![],
                sign_args: vec![],
                callback: None,
            })),
        }
    }
}
