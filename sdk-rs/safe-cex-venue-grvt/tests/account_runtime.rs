#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_venue_grvt::{
    account_runtime::*,
    builder_workflow::{JsPromise, JsResult},
    session_host::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
include!("support/account_values.rs");
struct Host {
    me: RefCell<std::rc::Weak<Host>>,
    group: String,
    name: String,
    ex: N,
    events: RefCell<Vec<N>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    loops: RefCell<Vec<Rc<dyn Fn() -> JsResult<N>>>>,
    failure: N,
    order: N,
    position: N,
    positions: N,
    builders: RefCell<N>,
    aggregate: N,
    post_attempts: Cell<u32>,
    log_failures: Cell<u32>,
    virtual_time: Cell<f64>,
    prototype: N,
    position_rows: RefCell<N>,
}
impl Host {
    fn new(group: &str, name: &str) -> Rc<Self> {
        let market = N::from(json!({"id":"BTC_USDT_Perp","symbol":"BTC/USDT"}));
        let store = N::from(
            json!({"loaded":{"markets":false,"tickers":false,"balance":false,"positions":false,"orders":false},"positions":[{"old":true}],"balance":{"total":0},"orders":[],"fetchedLeverage":{"BTC/USDT":{"buyLeverage":7,"sellLeverage":8,"isFetching":name=="fetching"}}}),
        );
        store.set("markets", N::array(vec![market.clone()]));
        store.set(
            "marketsById",
            N::obj(vec![("BTC_USDT_Perp", market.clone())]),
        );
        let ex = N::from(
            json!({"options":{"key":if name=="no-key"{""}else{"key"},"testnet":name=="testnet"},"apiExtras":{"sessionCookie":if name=="no-cookie"||group=="start"{""}else{"cookie"},"accountId":"account"},"isDisposed":name=="disposed","isPublicDataOnly":name=="public"||name=="external-public","defaultLeverage":10}),
        );
        ex.set("store", store);
        ex.set(
            "grvtTradingAccountId",
            if group == "balance" {
                if name == "no-auto" {
                    N::S("kept".into())
                } else {
                    N::U
                }
            } else if name == "no-account" || name == "detect-account" || name == "fake-reject" {
                N::U
            } else {
                N::S("trading".into())
            },
        );
        ex.set(
            "_builderAuthState",
            N::obj(vec![
                (
                    "status",
                    N::S(
                        if group == "builderInfo" {
                            if name == "authorized" || name == "testnet" {
                                "authorized"
                            } else if name == "low-cap" {
                                "cap-too-low"
                            } else if name.starts_with("unknown") || name == "fetch-error" {
                                "unknown"
                            } else {
                                "unauthorized"
                            }
                        } else {
                            "unknown"
                        }
                        .into(),
                    ),
                ),
                (
                    "authorizedRaw",
                    if name == "low-cap" {
                        N::Num(100.)
                    } else {
                        N::U
                    },
                ),
            ]),
        );
        let mut order = N::from(
            json!({"order_id":"0x00","legs":[{"instrument":"BTC_USDT_Perp","is_buying_asset":true,"size":"5","limit_price":"101.5"}],"state":{"status":"OPEN","book_size":["3.25"],"traded_size":["2"],"update_time":"1700000000000999999"},"metadata":{"client_order_id":"42","create_time":"1690000000000000000"},"time_in_force":"GOOD_TILL_TIME","reduce_only":false}),
        );
        if name == "lite" {
            order = N::from(
                json!({"oi":"real","l":[{"i":"BTC_USDT_Perp","ib":false,"s":"5","lp":"101.5"}],"s1":{"s":"FILLED","bs":["3.25"],"ts":["2"],"ut":"1700000000000999999"},"m":{"co":"42"},"ti":"GOOD_TILL_TIME"}),
            );
        }
        let leg = order.get("legs").get("0");
        if name == "missing-instrument" {
            Self::delete(&leg, "instrument");
        }
        if name == "unknown-market" {
            leg.set("instrument", N::S("unknown".into()));
        }
        if name == "numeric-market-hit" || name == "numeric-market-miss" {
            leg.set("instrument", N::Num(7.));
            if name == "numeric-market-hit" {
                ex.get("store").get("marketsById").set("7", market);
            }
        }
        if name == "nan" {
            leg.set("size", N::S("bad".into()));
        }
        if name == "trigger-zero" || name == "take-profit" {
            order.get("metadata").set("trigger",N::from(if name=="trigger-zero"{json!({"trigger_type":"STOP_LOSS","tpsl":{"trigger_price":0,"trigger_by":"MARK","close_position":false}})}else{json!({"trigger_type":"TAKE_PROFIT","tpsl":{"trigger_price":"99","trigger_by":"LAST","close_position":true}})}));
        }
        if name == "false-aliases" {
            order.set("is_market", N::B(false));
            order.set("im", N::B(true));
            order.set("ro", N::B(true));
            leg.set("is_buying_asset", N::B(false));
            leg.set("ib", N::B(true));
        }
        if name == "missing-time" {
            Self::delete(&order.get("state"), "update_time");
        }
        if name == "raw-id" {
            order.set("metadata", N::obj(vec![]));
        }
        let position = N::from(if name == "full" {
            json!({"instrument":"BTC_USDT_Perp","size":"2","entry_price":"101","unrealized_pnl":"-3","realized_pnl":"1","notional":"202","leverage":"4","est_liquidation_price":"50"})
        } else {
            json!({"i":"BTC_USDT_Perp","s":"-2","ep":"101","up":"-3","rp":"1","n":"-202","l":"0","el":"50"})
        });
        if name == "zero" {
            position.set("s", N::S("0".into()));
        }
        if name == "unknown-market" {
            position.set("i", N::S("unknown".into()));
        }
        if name == "invalid-size" {
            position.set("s", N::S("bad".into()));
        }
        let builder = N::from(
            json!({"builder_account_id":"0xEE8F3559F298FFFA79B8253437969D15B5DCCFF3","max_futures_fee_rate":"0.03"}),
        );
        if name == "low-cap" {
            builder.set("max_futures_fee_rate", N::S("0.01".into()));
        }
        if name == "invalid-cap" {
            builder.set("max_futures_fee_rate", N::S("bad".into()));
        }
        if name == "missing-cap" {
            Self::delete(&builder, "max_futures_fee_rate");
        }
        if name == "unmatched" {
            builder.set("builder_account_id", N::S("else".into()));
        }
        let builders = if name == "duplicate-first" {
            N::array(vec![
                N::from(
                    json!({"builder_account_id":"0xEE8F3559F298FFFA79B8253437969D15B5DCCFF3","max_futures_fee_rate":"0.01"}),
                ),
                builder,
            ])
        } else if name == "lite" {
            N::from(json!([{"ba":"0xEE8F3559F298FFFA79B8253437969D15B5DCCFF3","mfr":"0.03"}]))
        } else {
            N::array(vec![builder])
        };
        let aggregate = N::from(if name == "full" {
            json!({"spot_balances":[{"currency":"USDT","balance":"5"}],"total_equity":"100","total_sub_account_available_balance":"30","total_upnl":"-2","sub_account_summaries":[{"sub_account_id":"full-account"}]})
        } else {
            json!({"sb":[{"c":"USDT","b":"5"},{"currency":"USDC","balance":"2"}],"te":"100","ts2":"30","tu":"-2","sas":[{"si":17}]})
        });
        if name == "zero-upnl" {
            aggregate.set("tu", N::Num(0.));
            aggregate.set("total_upnl", N::Num(99.));
        }
        if name == "invalid-upnl" {
            aggregate.set("tu", N::S("bad".into()));
        }
        if name == "absent-upnl" {
            Self::delete(&aggregate, "tu");
        }
        if name == "negative-available" {
            aggregate.set("ts2", N::S("-5".into()));
        }
        if name == "nan-equity" {
            aggregate.set("te", N::S("bad".into()));
        }
        if name == "duplicate-coins" {
            Self::push(&aggregate.get("sb"), N::from(json!({"c":"USDT","b":"3"})));
        }
        if name == "prototype-currency" {
            aggregate.set("sb", N::from(json!([{"c":"constructor","b":"5"}])));
        }
        let h = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            group: group.into(),
            name: name.into(),
            ex,
            events: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
            loops: RefCell::new(vec![]),
            failure: error("Error", "io-fail"),
            order,
            position,
            positions: N::from(
                json!([{"fake":true,"symbol":"BTC/USDT"},{"fake":true,"symbol":"ETH/USDT"}]),
            ),
            builders: RefCell::new(builders),
            aggregate,
            post_attempts: Cell::new(0),
            log_failures: Cell::new(0),
            virtual_time: Cell::new(5000.),
            prototype: N::obj(vec![]),
            position_rows: RefCell::new(N::U),
        });
        *h.me.borrow_mut() = Rc::downgrade(&h);
        h.install();
        if group == "positions" && name == "object-symbol" {
            let symbol = N::from(json!({"object":"symbol"}));
            h.ex.get("store")
                .get("markets")
                .get("0")
                .set("symbol", symbol.clone());
            h.positions.get("0").set("symbol", symbol);
        }
        h
    }
    fn delete(v: &N, key: &str) {
        if let N::O(v) = v {
            v.borrow_mut().shift_remove(key);
        }
    }
    fn push(array: &N, value: N) {
        let N::A(v) = array else {
            panic!("fixture array")
        };
        v.borrow_mut().push(value);
    }
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn event(&self, args: Vec<N>) {
        self.events
            .borrow_mut()
            .push(N::array(args.into_iter().map(|v| v.snapshot()).collect()));
    }
    fn f(&self, callback: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) -> N {
        N::F(Rc::new(callback))
    }
    fn poll_job(&self, j: &Rc<Job>) {
        let Some(mut f) = j.future.borrow_mut().take() else {
            return;
        };
        let mut cx = Context::from_waker(Waker::noop());
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => *j.result.borrow_mut() = Some(v),
            Poll::Pending => *j.future.borrow_mut() = Some(f),
        }
    }
    fn drain(&self) {
        for _ in 0..64 {
            let jobs = self.jobs.borrow().clone();
            for job in jobs {
                self.poll_job(&job);
            }
            if self
                .jobs
                .borrow()
                .iter()
                .all(|j| j.result.borrow().is_some())
            {
                return;
            }
        }
        panic!("account jobs did not settle within 64 passes");
    }
    fn async_value(&self, callback: impl FnOnce() -> JsResult<N>) -> N {
        N::P(Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(callback())),
        }))
    }
    fn install(&self) {
        let h = self.self_rc();
        self.ex.set(
            "log",
            self.f(move |receiver, args| {
                assert!(receiver.same(&h.ex), "logger receiver");
                let msg = args[0].text();
                h.event(vec![h.string("log"), args[0].clone(), args[1].clone()]);
                if h.group == "authenticate"
                    && h.name == "log-mutation"
                    && msg == "Successfully authenticated with GRVT"
                {
                    h.ex.set(
                        "apiExtras",
                        N::from(json!({"sessionCookie":"mutated","accountId":"mutated-account"})),
                    );
                }
                if h.name == "catch-log-error"
                    && msg.starts_with(if h.group == "tick" {
                        "Error in tick:"
                    } else {
                        "Authentication failed:"
                    })
                {
                    return Err(error("Error", "catch-log-fail"));
                }
                if (h.name == "success-log-error" && msg == "Balance refreshed via tick()")
                    || (h.name == "auto-log-error" && msg.starts_with("Auto-detected"))
                    || (h.name == "state-log-error"
                        && msg.starts_with("Builder authorization status:"))
                    || (h.name == "last-log-error" && msg == "GRVT exchange started successfully")
                {
                    return Err(error(
                        "Error",
                        match h.name.as_str() {
                            "success-log-error" => "success-log-fail",
                            "auto-log-error" => "auto-log-fail",
                            "state-log-error" => "state-log-fail",
                            _ => "last-log-fail",
                        },
                    ));
                }
                h.log_failures.set(h.log_failures.get() + 1);
                if h.name == "log-error" && h.log_failures.get() == 1 {
                    return Err(error("Error", "log-fail"));
                }
                Ok(N::U)
            }),
        );
        let store = self.ex.get("store");
        let expected = store.clone();
        let h = self.self_rc();
        store.set(
            "update",
            self.f(move |receiver, args| {
                assert!(receiver.same(&expected), "store receiver");
                h.event(vec![h.string("store"), args[0].clone()]);
                if let N::O(v) = &args[0] {
                    for (k, v) in v.borrow().iter() {
                        expected.set(k, v.clone());
                    }
                }
                if h.name == "store-error" {
                    Err(error("Error", "store-fail"))
                } else {
                    Ok(N::U)
                }
            }),
        );
        let h = self.self_rc();
        self.ex.set(
            "syncBalanceUpnlFromPositions",
            self.f(move |receiver, args| {
                assert!(receiver.same(&h.ex), "sync receiver");
                let mut event = vec![h.string("sync")];
                event.extend(args);
                h.event(event);
                if h.name == "sync-error" {
                    Err(error("Error", "sync-fail"))
                } else {
                    Ok(N::U)
                }
            }),
        );
        let h = self.self_rc();
        self.ex.set(
            "emitAndLogError",
            self.f(move |_, args| {
                h.event(vec![h.string("emit"), args[0].clone()]);
                if h.name == "emit-error" {
                    Err(error("Error", "emit-fail"))
                } else {
                    Ok(N::U)
                }
            }),
        );
        for (kind, label) in [
            ("publicWebsocket", "public-connect"),
            ("privateWebsocket", "private-connect"),
        ] {
            if kind == "privateWebsocket" && self.name == "no-private" {
                self.ex.set(kind, N::U);
                continue;
            }
            let socket = N::obj(vec![(
                "isConnected",
                N::B(self.name == "private-connected"),
            )]);
            let expected = socket.clone();
            let h = self.self_rc();
            socket.set(
                "connectAndSubscribe",
                self.f(move |receiver, _| {
                    assert!(receiver.same(&expected), "websocket receiver");
                    h.event(vec![h.string(label)]);
                    Ok(N::U)
                }),
            );
            self.ex.set(kind, socket);
        }
        for (kind, field) in [("edge", "edgeXhr"), ("trading", "tradingXhr")] {
            let xhr = N::obj(vec![]);
            let expected = xhr.clone();
            let h = self.self_rc();
            xhr.set(
                "post",
                self.f(move |receiver, args| {
                    assert!(receiver.same(&expected), "post receiver");
                    let mut events = vec![h.string("post"), h.string(kind)];
                    events.extend(args);
                    h.event(events);
                    let host = h.clone();
                    Ok(h.async_value(move || {
                        if kind == "edge" {
                            if host.name == "post-error"
                                || host.name == "null-error"
                                || host.name == "catch-log-error"
                            {
                                return Err(if host.name == "null-error" {
                                    N::Null
                                } else {
                                    host.failure.clone()
                                });
                            }
                            if host.name == "missing-cookie" {
                                host.ex
                                    .get("apiExtras")
                                    .set("sessionCookie", host.string(""));
                            }
                            if host.name == "missing-account" {
                                host.ex.get("apiExtras").set("accountId", host.string(""));
                            }
                            return Ok(N::from(json!({"ignored":true})));
                        }
                        if host.group == "builders" {
                            host.post_attempts.set(host.post_attempts.get() + 1);
                            if host.name == "exhausted"
                                || host.name == "sleep-error"
                                || host.name == "retry" && host.post_attempts.get() < 3
                            {
                                return Err(host.failure.clone());
                            }
                            return Ok(N::obj(vec![(
                                "data",
                                N::obj(vec![(
                                    if host.name == "lite" { "r" } else { "results" },
                                    host.builders.borrow().clone(),
                                )]),
                            )]));
                        }
                        if host.name == "post-error" {
                            return Err(host.failure.clone());
                        }
                        Ok(N::obj(vec![(
                            "data",
                            if host.name == "missing" {
                                N::obj(vec![])
                            } else {
                                N::obj(vec![(
                                    if host.name == "full" { "result" } else { "r" },
                                    host.aggregate.clone(),
                                )])
                            },
                        )]))
                    }))
                }),
            );
            self.ex.set(field, xhr);
        }
        let client = N::obj(vec![("cookie", N::from(json!({"cached":true})))]);
        let h = self.self_rc();
        client.set(
            "authenticatedEndpoint",
            self.f(move |receiver, _| {
                assert!(receiver.same(&h.ex.get("grvtClient")), "endpoint receiver");
                h.event(vec![h.string("endpoint")]);
                if h.name == "endpoint-mutation" {
                    h.ex.set("grvtTradingAccountId", h.string("changed"));
                    h.ex.get("grvtClient").get("tdgClient").set(
                        "positions",
                        h.f(|_, _| Err(error("Error", "replaced method"))),
                    );
                }
                Ok(h.async_value(|| Ok(N::from(json!({"token":"endpoint"})))))
            }),
        );
        let h = self.self_rc();
        client.set(
            "getFundingAccountSummary",
            self.f(move |receiver, _| {
                assert!(receiver.same(&h.ex.get("grvtClient")), "funding receiver");
                h.event(vec![h.string("funding")]);
                let host = h.clone();
                Ok(h.async_value(move || {
                    if host.name == "summary-error" {
                        return Err(host.failure.clone());
                    }
                    if host.name == "missing-message" {
                        return Err(N::from(json!({"message":""})));
                    }
                    Ok(if host.name == "empty" {
                        N::Null
                    } else {
                        N::from(json!({"balance":3}))
                    })
                }))
            }),
        );
        let tdg = N::obj(vec![]);
        let orders = N::array(vec![self.order.clone()]);
        for (name, event) in [("positions", "positions"), ("openOrders", "orders")] {
            let expected = tdg.clone();
            let h = self.self_rc();
            let orders = orders.clone();
            tdg.set(
                name,
                self.f(move |receiver, args| {
                    assert!(receiver.same(&expected), "TDG receiver");
                    h.event(vec![h.string(event), args[0].clone(), args[1].clone()]);
                    let host = h.clone();
                    let orders = orders.clone();
                    Ok(h.async_value(move || {
                        if host.name == "post-error" {
                            return Err(host.failure.clone());
                        }
                        if host.name == "missing" {
                            return Ok(N::obj(vec![]));
                        }
                        let data = if host.name == "empty" {
                            N::array(vec![])
                        } else if name == "positions" {
                            N::array(vec![host.position.clone()])
                        } else {
                            orders
                        };
                        if name == "positions" && host.name == "iterator-append" {
                            *host.position_rows.borrow_mut() = data.clone();
                        }
                        Ok(N::obj(vec![(
                            // Two independent host/field pairs that happen to
                            // use the venue's full "result" envelope key; the
                            // arms stay separate so one changing is one edit.
                            #[allow(clippy::if_same_then_else)]
                            if name == "positions" && host.name == "full" {
                                "result"
                            } else if name == "openOrders" && host.name == "lite" {
                                "result"
                            } else {
                                "r"
                            },
                            data,
                        )]))
                    }))
                }),
            );
        }
        client.set("tdgClient", tdg);
        self.ex.set(
            "grvtClient",
            if self.name == "no-client" {
                N::U
            } else {
                client
            },
        );
        if self.group == "builders" && self.name == "iterator-append" {
            *self.builders.borrow_mut() =
                N::from(json!([{"builder_account_id":"different","max_futures_fee_rate":"0.03"}]));
        }
        if self.group == "builders"
            && (self.name == "iterator-close" || self.name == "iterator-error")
        {
            let first = if self.name == "iterator-error" {
                N::obj(vec![("__throw_address", N::B(true))])
            } else {
                self.builders.borrow().get("0")
            };
            *self.builders.borrow_mut() = N::obj(vec![
                ("length", N::Num(1.)),
                ("__iterator", N::B(true)),
                ("first", first),
            ]);
        }
        for name in [
            "authenticate",
            "fetchBuilderAuthorization",
            "tick",
            "fetchBalance",
            "fetchPositions",
            "fetchOpenOrders",
            "initExternalPublicData",
            "fetchMarkets",
            "fetchTickers",
            "updateMarketLeverageLimits",
            "getFakePositions",
            "getPositionMode",
            "symbolToInstrumentId",
            "calculateUpnlFromPositions",
            "getOrderCreationTimeMs",
            "recordNonce",
            "mapOrderStatus",
            "mapOrder",
        ] {
            let real =
                matches!(
                    (self.group.as_str(), name),
                    ("authenticate", "authenticate")
                        | ("builders", "fetchBuilderAuthorization")
                        | ("tick", "tick")
                        | ("balance", "fetchBalance")
                        | ("positions", "fetchPositions")
                        | ("orders", "fetchOpenOrders")
                        | ("mapOrder", "mapOrder")
                ) || (self.group == "start" && self.name == "last-log-error" && name == "tick");
            if real {
                if name == "tick" {
                    let h = self.self_rc();
                    self.ex.set(
                        name,
                        self.f(move |receiver, _| {
                            assert!(receiver.same(&h.ex), "live tick receiver");
                            Ok(h.promise(tick(h.clone())))
                        }),
                    );
                }
                continue;
            }
            let h = self.self_rc();
            let orders = orders.clone();
            self.ex.set(
                name,
                self.f(move |receiver, args| {
                    assert!(receiver.same(&h.ex), "own method receiver");
                    let mut events = vec![h.string("own"), h.string(name)];
                    events.extend(args.clone());
                    h.event(events);
                    match name {
                        "authenticate"
                        | "fetchBuilderAuthorization"
                        | "fetchBalance"
                        | "fetchPositions"
                        | "fetchOpenOrders"
                        | "initExternalPublicData"
                        | "fetchMarkets"
                        | "fetchTickers"
                        | "updateMarketLeverageLimits"
                        | "getPositionMode" => {
                            let host = h.clone();
                            Ok(h.async_value(move || host.own_result(name, args)))
                        }
                        "tick" => Ok(h.async_value(|| Ok(N::U))),
                        "getFakePositions" => {
                            if h.name == "fake-reject" {
                                return Ok(h.reject(h.failure.clone()));
                            }
                            if h.name == "fake-duplicate" {
                                let N::A(v) = &h.positions else {
                                    unreachable!()
                                };
                                let mut values = v.borrow().clone();
                                values.push(values[1].clone());
                                Ok(N::array(values))
                            } else {
                                Ok(h.positions.clone())
                            }
                        }
                        "symbolToInstrumentId" => {
                            let s = args[0].text();
                            let instrument = if s.to_uppercase().ends_with("_PERP") {
                                format!("{}_Perp", &s[..s.len() - 5])
                            } else {
                                s
                            };
                            Ok(h.string(&instrument))
                        }
                        "calculateUpnlFromPositions" => Ok(N::Num(9.)),
                        "getOrderCreationTimeMs" => Ok(N::Num(1234.)),
                        "recordNonce" => {
                            if h.name == "nonce-error" {
                                Err(h.failure.clone())
                            } else {
                                Ok(N::U)
                            }
                        }
                        "mapOrderStatus" => {
                            if h.name == "status-mutation" {
                                h.order.set("time_in_force", h.string("FILL_OR_KILL"));
                            }
                            Ok(h.string(match args[0].text().as_str() {
                                "FILLED" => "closed",
                                "REJECTED" | "CANCELLED" => "canceled",
                                _ => "open",
                            }))
                        }
                        "mapOrder" => {
                            if h.name == "map-error" {
                                return Err(h.failure.clone());
                            }
                            if h.name == "map-append" {
                                Self::push(&orders, args[0].clone());
                            }
                            Ok(N::obj(vec![
                                ("id", h.string("mapped")),
                                ("info", args[0].clone()),
                            ]))
                        }
                        _ => unreachable!(),
                    }
                }),
            );
        }
    }
    fn own_result(&self, name: &str, args: Vec<N>) -> JsResult<N> {
        match name {
            "authenticate" => {
                if self.name == "auth-error" {
                    return Err(self.failure.clone());
                }
                self.ex.set(
                    "apiExtras",
                    N::from(json!({"sessionCookie":"fresh","accountId":"fresh-account"})),
                );
                Ok(N::B(self.name != "auth-false"))
            }
            "fetchBuilderAuthorization" => {
                if self.name == "builder-error" || self.name == "fetch-error" {
                    return Err(self.failure.clone());
                }
                if self.name == "unknown-authorized" {
                    self.ex
                        .set("_builderAuthState", N::from(json!({"status":"authorized"})));
                }
                Ok(N::U)
            }
            "fetchBalance" => {
                if ["fetch-error", "balance-error", "catch-log-error"].contains(&self.name.as_str())
                {
                    return Err(self.failure.clone());
                }
                if self.name == "dispose-await" {
                    self.ex.set("isDisposed", N::B(true));
                }
                if self.name == "detect-account" {
                    self.ex.set("grvtTradingAccountId", self.string("detected"));
                }
                Ok(N::from(json!({"total":100})))
            }
            "fetchPositions" => {
                if self.name == "positions-error" {
                    Err(self.failure.clone())
                } else {
                    Ok(N::from(json!([{"symbol":"BTC/USDT"}])))
                }
            }
            "fetchOpenOrders" => {
                if self.name == "orders-error" {
                    Err(self.failure.clone())
                } else {
                    Ok(N::from(json!([{"id":"new-order"}])))
                }
            }
            "initExternalPublicData" => {
                if self.name == "freeze-public" {
                    self.ex.set("isPublicDataOnly", N::B(true));
                }
                Ok(N::B(
                    self.name == "external" || self.name == "external-public",
                ))
            }
            "fetchMarkets" => Ok(N::from(json!([{"id":"market"}]))),
            "fetchTickers" => Ok(N::from(json!([{"symbol":"BTC/USDT"}]))),
            "updateMarketLeverageLimits" => {
                if self.name == "leverage-error" {
                    Err(self.failure.clone())
                } else {
                    Ok(args[0].clone())
                }
            }
            "getPositionMode" => Ok(self.string("one_way")),
            _ => unreachable!(),
        }
    }
}
impl SessionHost for Host {
    type Value = N;
    fn get(&self, v: &N, key: &str) -> JsResult<N> {
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!("Cannot read properties of {} (reading '{key}')", v.text()),
            ));
        }
        if self.group == "authenticate" && self.name == "post-getter" {
            if v.same(&self.ex.get("edgeXhr")) && key == "post" {
                self.event(vec![self.string("post-get")]);
            }
            if v.same(&self.ex.get("options")) && key == "key" {
                self.event(vec![self.string("key-get")]);
                self.ex.get("edgeXhr").set(
                    "post",
                    self.f(|_, _| Err(error("Error", "replacement-post"))),
                );
            }
        }
        if self.group == "balance"
            && self.name == "iterator-append"
            && key == "b"
            && v.same(&self.aggregate.get("sb").get("0"))
        {
            self.event(vec![self.string("append-spot")]);
            Self::push(
                &self.aggregate.get("sb"),
                N::from(json!({"c":"SOL","b":"4"})),
            );
        }
        if self.group == "builders"
            && self.name == "iterator-append"
            && key == "builder_account_id"
            && v.same(&self.builders.borrow().get("0"))
        {
            self.event(vec![self.string("append-builder")]);
            Self::push(
                &self.builders.borrow(),
                N::from(
                    json!({"builder_account_id":"0xee8f3559f298fffa79b8253437969d15b5dccff3","max_futures_fee_rate":"0.03"}),
                ),
            );
        }
        if self.name == "update-getter" && key == "update" && v.same(&self.ex.get("store")) {
            self.event(vec![self.string("update-get")]);
            self.ex.get("store").get("loaded").set("extra", N::B(true));
        }
        if self.group == "builders"
            && self.name == "iterator-error"
            && key == "builder_account_id"
            && self.truthy(&v.get("__throw_address"))
        {
            return Err(self.failure.clone());
        }
        if self.group == "positions"
            && self.name == "iterator-append"
            && key == "i"
            && v.same(&self.position)
        {
            self.event(vec![self.string("append-position")]);
            Self::push(
                &self.position_rows.borrow(),
                N::from(json!({"i":"missing","s":"1"})),
            );
        }
        let own = v.get(key);
        if !matches!(own, N::U) {
            return Ok(own);
        }
        if key == "constructor" && matches!(v, N::O(_)) {
            return Ok(self.prototype.clone());
        }
        if let N::A(v) = v {
            if key == "length" {
                return Ok(N::Num(v.borrow().len() as f64));
            }
        }
        match (v, key) {
            (N::S(_), "toUpperCase" | "toLowerCase") => {
                let upper = key == "toUpperCase";
                Ok(self.f(move |receiver, _| {
                    let N::S(v) = receiver else {
                        panic!("string receiver")
                    };
                    Ok(N::S(if upper {
                        v.to_uppercase()
                    } else {
                        v.to_lowercase()
                    }))
                }))
            }
            (N::A(_), "push" | "map" | "filter" | "find") => {
                let h = self.self_rc();
                let key = key.to_owned();
                Ok(self.f(move |receiver, args| {
                    let N::A(values) = &receiver else {
                        panic!("array receiver")
                    };
                    if key == "push" {
                        for arg in args {
                            values.borrow_mut().push(arg);
                        }
                        return Ok(N::Num(values.borrow().len() as f64));
                    }
                    let len = values.borrow().len();
                    let mut result = vec![];
                    for i in 0..len {
                        let value = values.borrow().get(i).cloned().unwrap_or(N::U);
                        let mapped = h.call(args[0].clone(), N::U, vec![value.clone()])?;
                        if key == "map" {
                            result.push(mapped);
                        } else if h.truthy(&mapped) {
                            if key == "find" {
                                return Ok(value);
                            }
                            result.push(value);
                        }
                    }
                    Ok(if key == "find" {
                        N::U
                    } else {
                        N::array(result)
                    })
                }))
            }
            _ => Ok(N::U),
        }
    }
    fn set(&self, v: &N, k: &str, field: N) -> JsResult<N, ()> {
        v.set(k, field);
        Ok(())
    }
    fn call(&self, f: N, receiver: N, args: Vec<N>) -> JsResult<N> {
        match f {
            N::F(f) => f(receiver, args),
            _ => Err(error(
                "TypeError",
                "instrument.toUpperCase is not a function",
            )),
        }
    }
    fn function(&self, f: Callback<N>) -> N {
        self.f(move |_, args| f(args.first().cloned().unwrap_or(N::U)))
    }
    fn object(&self, fields: Vec<(&str, N)>) -> N {
        N::obj(fields)
    }
    fn define(&self, v: &N, key: &str, field: N) -> JsResult<N, ()> {
        v.set(key, field);
        Ok(())
    }
    fn array(&self, values: Vec<N>) -> N {
        N::array(values)
    }
    fn spread(&self, target: &N, source: N) -> JsResult<N, ()> {
        if let N::O(v) = source {
            for (k, v) in v.borrow().iter() {
                target.set(k, v.clone());
            }
        }
        Ok(())
    }
    fn string(&self, v: &str) -> N {
        N::S(v.into())
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn null(&self) -> N {
        N::Null
    }
    fn truthy(&self, v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(v) => *v,
            N::Num(v) => *v != 0. && !v.is_nan(),
            N::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn is_array(&self, v: &N) -> bool {
        matches!(v, N::A(_))
    }
    fn is_string(&self, v: &N) -> bool {
        matches!(v, N::S(_))
    }
    fn is_function(&self, v: &N) -> bool {
        matches!(v, N::F(_))
    }
    fn equals_number(&self, v: &N, n: f64) -> JsResult<N, bool> {
        Ok(self.number_value(v.clone())? == n)
    }
    fn template(&self, values: Vec<N>) -> JsResult<N> {
        Ok(N::S(values.iter().map(N::text).collect()))
    }
    fn error(&self, message: N) -> JsResult<N> {
        Ok(error("Error", &message.text()))
    }
    fn reject(&self, error: N) -> N {
        N::P(Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(Err(error))),
        }))
    }
    fn start(&self, task: JsPromise<N>) -> JsPromise<N> {
        let wrapped = Box::pin(async move {
            let mut value = task.await?;
            loop {
                if let N::P(job) = value {
                    value = Handle(job).await?;
                } else {
                    return Ok(value);
                }
            }
        });
        let job = Rc::new(Job {
            future: RefCell::new(Some(wrapped)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        self.poll_job(&job);
        Box::pin(Handle(job))
    }
    fn promise(&self, task: JsPromise<N>) -> N {
        let job = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        self.poll_job(&job);
        N::P(job)
    }
    fn await_value(&self, value: N) -> JsPromise<N> {
        Box::pin(async move {
            Later {
                first: false,
                callback: Some(Box::new(|| Ok(N::U))),
            }
            .await?;
            match value {
                N::P(job) => Handle(job).await,
                value => Ok(value),
            }
        })
    }
}
impl AccountHost for Host {
    fn binding(&self) -> Rc<Self> {
        self.self_rc()
    }
    fn exchange(&self) -> N {
        self.ex.clone()
    }
    fn endpoint(&self, name: &str) -> N {
        self.string(match name {
            "AUTH" => "/auth/api_key/login",
            "GET_AUTHORIZED_BUILDERS" => "/full/v1/get_authorized_builders",
            _ => panic!("endpoint"),
        })
    }
    fn severity(&self, s: LogSeverity) -> N {
        self.string(match s {
            LogSeverity::Debug => "debug",
            LogSeverity::Info => "info",
            LogSeverity::Warning => "warning",
            LogSeverity::Error => "error",
        })
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        match (a, b) {
            (N::U, N::U) | (N::Null, N::Null) => true,
            (N::S(a), N::S(b)) => a == b,
            (N::Num(a), N::Num(b)) => a == b,
            (N::B(a), N::B(b)) => a == b,
            _ => a.same(b),
        }
    }
    fn indexed(&self, v: &N, k: &N) -> JsResult<N> {
        self.get(v, &k.text())
    }
    fn set_indexed(&self, v: &N, k: N, value: N) -> JsResult<N, ()> {
        self.set(v, &k.text(), value)
    }
    fn number_value(&self, v: N) -> JsResult<N, f64> {
        Ok(match v {
            N::Num(v) => v,
            N::U => f64::NAN,
            N::Null => 0.,
            N::B(v) => {
                if v {
                    1.
                } else {
                    0.
                }
            }
            N::S(v) => {
                if v.is_empty() {
                    0.
                } else {
                    v.parse().unwrap_or(f64::NAN)
                }
            }
            _ => f64::NAN,
        })
    }
    fn greater_than_zero(&self, v: N) -> JsResult<N, bool> {
        Ok(self.number_value(v)? > 0.)
    }
    fn parse_float(&self, v: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_float(&v.text()))
    }
    fn parse_int(&self, v: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_int(&v.text(), None))
    }
    fn js_string(&self, v: N) -> JsResult<N> {
        Ok(self.string(&v.text()))
    }
    fn add(&self, a: N, b: N) -> JsResult<N> {
        Ok(if matches!(a, N::S(_)) || matches!(b, N::S(_)) {
            self.string(&format!("{}{}", a.text(), b.text()))
        } else {
            self.number(self.number_value(a)? + self.number_value(b)?)
        })
    }
    fn now(&self) -> JsResult<N, f64> {
        self.event(vec![self.string("now")]);
        if self.name == "clock-error" {
            Err(error("Error", "clock-fail"))
        } else {
            Ok(100000.)
        }
    }
    fn virtual_now(&self) -> JsResult<N, f64> {
        self.event(vec![self.string("virtual")]);
        let time = self.virtual_time.get();
        self.virtual_time.set(time + 1.);
        Ok(time)
    }
    fn for_of(&self, array: N, visitor: &mut Iteration<'_, N>) -> JsResult<N, ()> {
        if matches!(array, N::O(_)) && self.truthy(&array.get("__iterator")) {
            let result = visitor(array.get("first"));
            self.event(vec![self.string("iterator-close")]);
            result?;
            return Ok(());
        }
        let N::A(values) = array else {
            return Err(error("TypeError", "value is not iterable"));
        };
        let mut index = 0;
        while index < values.borrow().len() {
            let value = values.borrow()[index].clone();
            if visitor(value)? {
                break;
            }
            index += 1;
        }
        Ok(())
    }
    fn loop_after(&self, callback: Rc<dyn Fn() -> JsResult<N>>, delay: f64) -> JsResult<N, ()> {
        self.event(vec![self.string("loop"), self.number(delay)]);
        self.loops.borrow_mut().push(callback);
        Ok(())
    }
    fn sleep(&self, delay: f64) -> JsResult<N, JsPromise<N>> {
        self.event(vec![self.string("sleep"), self.number(delay)]);
        let name = self.name.clone();
        Ok(Box::pin(Later {
            first: false,
            callback: Some(Box::new(move || {
                if name == "sleep-error" {
                    Err(error("Error", "sleep-fail"))
                } else {
                    Ok(N::U)
                }
            })),
        }))
    }
    fn builder_address(&self, testnet: N) -> JsResult<N> {
        Ok(
            self.string(safe_cex_venue_grvt::builder::get_grvt_builder_address(
                self.truthy(&testnet),
            )),
        )
    }
    fn fee_to_raw(&self, fee: N) -> JsResult<N, f64> {
        safe_cex_venue_grvt::builder::grvt_builder_fee_rate_to_raw(&fee.text())
            .map_err(|message| error("Error", &message))
    }
    fn builder_info(&self, testnet: N) -> JsResult<N> {
        let value = safe_cex_venue_grvt::builder::create_grvt_builder_info(
            Some(self.truthy(&testnet)),
            None,
            None,
        )
        .unwrap();
        Ok(N::obj(vec![
            ("builder", self.string(&value.builder)),
            ("builderFee", self.string(&value.builder_fee)),
            ("builderFeeInt", self.number(value.builder_fee_int)),
        ]))
    }
    fn builder_error(&self, opts: N) -> JsResult<N> {
        let authorized = opts.get("authorizedRaw");
        let value = safe_cex_venue_grvt::builder::GrvtBuilderUnauthorizedError::new(
            if opts.get("reason").text() == "cap-too-low" {
                safe_cex_venue_grvt::types::GrvtBuilderUnauthorizedReason::CapTooLow
            } else {
                safe_cex_venue_grvt::types::GrvtBuilderUnauthorizedReason::Unauthorized
            },
            self.number_value(opts.get("requiredRaw"))?,
            if matches!(authorized, N::U) {
                None
            } else {
                Some(self.number_value(authorized.clone())?)
            },
            None,
        );
        let e = error(value.name, &value.message);
        e.set("code", self.string(value.code));
        e.set("reason", opts.get("reason"));
        e.set("requiredRaw", opts.get("requiredRaw"));
        e.set("authorizedRaw", authorized);
        Ok(e)
    }
    fn position_id(&self, symbol: N) -> JsResult<N> {
        Ok(symbol)
    }
    fn trigger_metadata(&self, order: N) -> JsResult<N> {
        let metadata = self.get(&order, "metadata")?;
        let metadata = if self.nullish(&metadata) {
            self.get(&order, "m")?
        } else {
            metadata
        };
        let trigger = if self.nullish(&metadata) {
            N::U
        } else {
            let t = self.get(&metadata, "trigger")?;
            if self.nullish(&t) {
                self.get(&metadata, "t")?
            } else {
                t
            }
        };
        if self.nullish(&trigger) {
            return Ok(N::U);
        }
        let kind = {
            let full = self.get(&trigger, "trigger_type")?;
            if self.nullish(&full) {
                self.get(&trigger, "tt")?
            } else {
                full
            }
        };
        let t = if kind.text() == "STOP_LOSS" {
            self.string("STOP_LOSS")
        } else if kind.text() == "TAKE_PROFIT" {
            self.string("TAKE_PROFIT")
        } else {
            return Ok(N::U);
        };
        let tpsl = {
            let full = self.get(&trigger, "tpsl")?;
            if self.nullish(&full) {
                self.get(&trigger, "t")?
            } else {
                full
            }
        };
        let raw = if self.nullish(&tpsl) {
            N::U
        } else {
            let full = self.get(&tpsl, "trigger_price")?;
            if self.nullish(&full) {
                self.get(&tpsl, "tp")?
            } else {
                full
            }
        };
        let n = self.number_value(raw)?;
        let by = if self.nullish(&tpsl) {
            N::U
        } else {
            let full = self.get(&tpsl, "trigger_by")?;
            if self.nullish(&full) {
                self.get(&tpsl, "tb")?
            } else {
                full
            }
        };
        let result = N::obj(vec![
            ("type", t),
            (
                "triggerPrice",
                if n.is_finite() { self.number(n) } else { N::U },
            ),
            (
                "triggerType",
                match by.text().as_str() {
                    "MARK" => self.string("mark_price"),
                    "LAST" => self.string("last_price"),
                    "INDEX" => self.string("index_price"),
                    _ => N::U,
                },
            ),
        ]);
        let close = if self.nullish(&tpsl) {
            N::U
        } else {
            let full = self.get(&tpsl, "close_position")?;
            if self.nullish(&full) {
                self.get(&tpsl, "cp")?
            } else {
                full
            }
        };
        if matches!(close, N::B(_)) {
            result.set("close", close);
        }
        Ok(result)
    }
    fn time_in_force(&self, order: N) -> JsResult<N> {
        let post = self.get(&order, "post_only")?;
        let post = if self.nullish(&post) {
            self.get(&order, "po")?
        } else {
            post
        };
        if !self.is_undefined(&post) && !matches!(post, N::B(_)) {
            return Ok(N::U);
        }
        let value = self.get(&order, "time_in_force")?;
        let value = if self.nullish(&value) {
            self.get(&order, "ti")?
        } else {
            value
        };
        Ok(match value.text().as_str() {
            "GOOD_TILL_TIME" => self.string(if self.truthy(&post) {
                "PostOnly"
            } else {
                "GoodTillCancel"
            }),
            "IMMEDIATE_OR_CANCEL" if !self.truthy(&post) => self.string("ImmediateOrCancel"),
            "FILL_OR_KILL" if !self.truthy(&post) => self.string("FillOrKill"),
            _ => N::U,
        })
    }
    fn keys_join(&self, value: N) -> JsResult<N> {
        let N::O(v) = value else {
            return Ok(self.string(""));
        };
        Ok(self.string(&v.borrow().keys().cloned().collect::<Vec<_>>().join(", ")))
    }
    fn symbol_set(&self, values: N) -> JsResult<N> {
        Ok(values)
    }
    fn has_symbol(&self, set: &N, value: N) -> JsResult<N, bool> {
        let N::A(v) = set else { panic!("symbol set") };
        Ok(v.borrow().iter().any(|other| {
            self.equal(other, &value)
                || matches!((&value,other),(N::Num(a),N::Num(b)) if a.is_nan()&&b.is_nan())
        }))
    }
    fn append_iterable(&self, target: &N, values: N) -> JsResult<N, ()> {
        self.for_of(values, &mut |value| {
            Self::push(target, value);
            Ok(false)
        })
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/account-runtime-source.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .expect("Node account source oracle");
        assert!(
            output.status.success(),
            "account source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("account source JSON")
    })
}
fn run_group(group: &str, count: usize) {
    let rows = oracle()[group].as_array().expect(group);
    assert_eq!(rows.len(), count, "independent {group} source case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let h = Host::new(group, name);
        let label = format!("{group}/{name}");
        let mut immediate_error = None;
        let task = match group {
            "tick" => tick(h.clone()),
            "authenticate" => authenticate(h.clone()),
            "validate" => validate_account(h.clone()),
            "builders" => fetch_builder_authorization(h.clone()),
            "builderInfo" => get_builder_info(h.clone()),
            "start" => start(h.clone()),
            "balance" => fetch_balance(h.clone()),
            "positions" => fetch_positions(
                h.clone(),
                if name == "symbols" {
                    N::from(json!(["BTC_USDT_PERP"]))
                } else {
                    N::U
                },
            ),
            "orders" => fetch_open_orders(
                h.clone(),
                if name == "symbol" {
                    h.string("BTC_USDT_PERP")
                } else {
                    N::U
                },
            ),
            "mapOrder" => {
                let result = map_order(h.as_ref(), h.order.clone());
                if let Err(error) = &result {
                    immediate_error = Some(error.clone());
                }
                Box::pin(async move { result })
            }
            _ => panic!("group"),
        };
        let job = h.promise(task);
        let initial = N::array(h.events.borrow().clone());
        let initial_state = N::obj(vec![
            ("extras", h.ex.get("apiExtras").snapshot()),
            ("cookie", h.ex.get("grvtClient").get("cookie").snapshot()),
            ("account", h.ex.get("grvtTradingAccountId")),
            ("authState", h.ex.get("_builderAuthState").snapshot()),
            ("loaded", h.ex.get("store").get("loaded").snapshot()),
            ("disposed", h.ex.get("isDisposed")),
        ]);
        h.drain();
        let N::P(job) = job else { unreachable!() };
        let result = job.result.borrow().clone().expect("account result settled");
        let (value, error, original) = match result {
            Ok(value) => (value, N::Null, false),
            Err(error) => {
                let original = error.same(&h.failure);
                (N::U, error, original)
            }
        };
        if group == "tick" && name == "loop-fire" {
            h.ex.set("isDisposed", N::B(true));
            let host = h.clone();
            h.ex.set(
                "tick",
                h.f(move |receiver, _| {
                    assert!(receiver.same(&host.ex), "replacement tick receiver");
                    host.event(vec![host.string("loop-tick")]);
                    Ok(host.async_value(|| Ok(N::U)))
                }),
            );
            let callback = h.loops.borrow_mut().remove(0);
            callback().unwrap_or_else(|_| panic!("loop callback"));
            h.drain();
        }
        let infos = if !matches!(value.get("info"), N::U) {
            vec![N::B(value.get("info").same(&h.order))]
        } else if let N::A(v) = &value {
            v.borrow()
                .iter()
                .filter(|v| !matches!(v.get("info"), N::U))
                .map(|v| N::B(v.get("info").same(&h.order) || v.get("info").same(&h.position)))
                .collect()
        } else {
            vec![]
        };
        let fake_array_identity = group == "positions" && value.same(&h.positions);
        let fake_identities = if group == "positions" {
            if let N::A(values) = &value {
                let N::A(fakes) = &h.positions else {
                    unreachable!()
                };
                values
                    .borrow()
                    .iter()
                    .map(|value| N::B(fakes.borrow().iter().any(|fake| value.same(fake))))
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };
        let duplicate_alias = group == "positions"
            && name == "fake-duplicate"
            && value.get("1").same(&value.get("2"));
        let symbol_alias = group == "positions"
            && name == "object-symbol"
            && value.get("0").get("id").same(&value.get("0").get("symbol"));
        assert_eq!(
            fake_array_identity, row["fakeArrayIdentity"],
            "{label} native fake array identity"
        );
        assert_native(
            &N::array(fake_identities),
            &row["fakeIdentities"],
            &format!("{label} native fake element identity"),
        );
        assert_eq!(
            duplicate_alias, row["duplicateAlias"],
            "{label} native duplicate alias"
        );
        assert_eq!(
            symbol_alias, row["symbolAlias"],
            "{label} native symbol alias"
        );
        assert_native(&value, &row["result"], &format!("{label} result"));
        assert_native(&error, &row["error"], &format!("{label} error"));
        assert_eq!(original, row["original"], "{label} raw exception identity");
        assert_native(
            &N::array(infos),
            &row["infos"],
            &format!("{label} info identity"),
        );
        assert_native(
            &initial_state,
            &row["initialState"],
            &format!("{label} eager state"),
        );
        assert_native(&initial, &row["initial"], &format!("{label} eager effects"));
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} effects"),
        );
        for (native, key) in [
            (h.ex.get("apiExtras"), "extras"),
            (h.ex.get("grvtClient").get("cookie"), "cookie"),
            (h.ex.get("grvtTradingAccountId"), "account"),
            (h.ex.get("_builderAuthState"), "authState"),
            (h.ex.get("isDisposed"), "disposed"),
            (h.ex.get("isPublicDataOnly"), "public"),
            (h.ex.get("store"), "store"),
        ] {
            assert_native(&native, &row[key], &format!("{label} {key}"));
        }
        assert_eq!(
            h.loops.borrow().len(),
            row["loops"].as_u64().unwrap() as usize,
            "{label} retained loop count"
        );
        drop(immediate_error);
    }
}
#[test]
fn tick_source() {
    run_group("tick", 11)
}
#[test]
fn authenticate_source() {
    run_group("authenticate", 10)
}
#[test]
fn validate_source() {
    run_group("validate", 8)
}
#[test]
fn builders_source() {
    run_group("builders", 18)
}
#[test]
fn builder_info_source() {
    run_group("builderInfo", 7)
}
#[test]
fn start_source() {
    run_group("start", 18)
}
#[test]
fn balance_source() {
    run_group("balance", 14)
}
#[test]
fn positions_source() {
    run_group("positions", 19)
}
#[test]
fn orders_source() {
    run_group("orders", 10)
}
#[test]
fn map_order_source() {
    run_group("mapOrder", 14)
}
