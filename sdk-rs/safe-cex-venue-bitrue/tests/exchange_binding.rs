#[path = "support/binding_runtime.rs"]
mod support;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, from_json, to_json},
    },
    runtime::ExchangeRuntime,
};
use safe_cex_types::{ExchangeName, ExchangeOptions, PlaceOrderOpts, UpdateOrderOpts};
use safe_cex_venue_bitrue::{BitrueDependencies, BitrueExchange};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Http(Rc<support::Runtime>, RefCell<Vec<Value>>, Cell<bool>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        let read = |k| to_json(&config.get(k).unwrap()).unwrap();
        let url = read("url");
        self.1.borrow_mut().push(json!({"url":url,"method":read("method"),"data":read("data"),"params":read("params"),"headers":read("headers"),"baseURL":read("baseURL")}));
        let body = match url.as_str().unwrap() {
            "/fapi/v1/contracts" => {
                json!([{"symbol":"E-BTC-USDT","status":1,"multiplier":0.01,"pricePrecision":1,"minOrderVolume":2,"maxMarketVolume":1234}])
            }
            "/fapi/v2/account" => {
                json!({"code":"0","data":{"account":[{"marginCoin":"USDT","totalEquity":1234,"accountNormal":1000,"accountLock":5,"sumMaintenanceMargin":7,"totalPositionNormal":10,"totalCost":12,"sumOpenRealizedAmount":3,"positionVos":[{"contractName":"E-BTC-USDT","positions":[{"side":"BUY","leverageLevel":7,"positionType":2,"volume":0.25,"openPrice":27123,"positionBalance":6780.75,"forceLiquidationPrice":23000,"reducePrice":22000}]}]}]}})
            }
            "/fapi/v2/openOrders" => {
                json!({"code":"0","data":[{"orderId":"existing-order","side":"BUY","type":"LIMIT","status":"NEW","action":"OPEN","origQty":0.4,"executedQty":0.1,"price":27234.5,"transactTime":1700000000000.0}]})
            }
            "/fapi/v2/leverageBracket" => {
                json!({"code":"0","data":[{"contractName":"E-BTC-USDT","brackets":[{"initialLeverage":17}]}]})
            }
            "/fapi/v2/order" => json!({"code":"0","data":{"orderId":"placed-order"}}),
            "/fapi/v2/cancel" | "/fapi/v2/level_edit" => json!({"code":"0","data":{}}),
            "/user_stream/api/v1/listenKey" => {
                if self.2.get() {
                    json!({"code":400,"msg":"denied"})
                } else {
                    json!({"code":200,"data":{"listenKey":"fixture-listen-key"}})
                }
            }
            other => panic!("unexpected Bitrue HTTP path {other}"),
        };
        let response = Js::object(vec![
            ("status", Js::Number(200.)),
            ("data", from_json(&body)),
            ("config", config),
        ]);
        Box::pin(async move { Ok(response) })
    }
}
fn complete<T>(rt: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut f = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..128 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        rt.pump();
        rt.advance(1000.);
    }
    panic!("Bitrue binding future did not complete")
}
#[test]
fn symbol_metadata_refresh_fetches_and_commits_the_current_leverage_bracket() {
    let runtime = Rc::new(support::Runtime::default());
    let http = Rc::new(Http(
        runtime.clone(),
        RefCell::new(vec![]),
        Cell::new(false),
    ));
    let store = Rc::new(DefaultStore::new());
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        store.clone(),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    let mut markets = complete(&runtime, venue.fetch_markets()).unwrap();
    markets[0].limits.leverage.max = 31.0;
    safe_cex_store::Store::update(
        store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    complete(&runtime, venue.update_symbol_metadata("E-BTC-USDT".into())).unwrap();
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|request| request["url"] == "/fapi/v2/leverageBracket"
                && request["params"]["contractName"] == "E-BTC-USDT"),
        "metadata refresh must fetch the symbol's current bracket"
    );
    assert_eq!(
        store.markets().unwrap()[0].limits.leverage.max,
        17.0,
        "metadata refresh must commit the returned bracket to the real Store"
    );
    venue.dispose().unwrap();
}

struct PrivateFixture {
    venue: Rc<BitrueExchange>,
    runtime: Rc<support::Runtime>,
    http: Rc<Http>,
    store: Rc<DefaultStore>,
    socket: Rc<support::Socket>,
}
impl PrivateFixture {
    fn new() -> Self {
        let runtime = Rc::new(support::Runtime::default());
        runtime.now.set(1_700_000_000_000.0);
        let http = Rc::new(Http(
            runtime.clone(),
            RefCell::new(vec![]),
            Cell::new(false),
        ));
        let store = Rc::new(DefaultStore::new());
        let venue = BitrueExchange::new(
            ExchangeName::Bitrue,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            store.clone(),
            BitrueDependencies {
                runtime: runtime.clone(),
                http: http.clone(),
                external_public_data: None,
            },
        )
        .unwrap();
        complete(&runtime, venue.start()).unwrap();
        runtime.pump();
        let socket = runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("listenKey=fixture-listen-key"))
            .map(|(_, socket)| socket.clone())
            .expect("real private listen-key connection");
        assert!(
            socket
                .sent
                .borrow()
                .iter()
                .any(|s| s.contains("user_order_update")),
            "production private subscriptions must run"
        );
        Self {
            venue,
            runtime,
            http,
            store,
            socket,
        }
    }
    fn frame(&self, frame: Value) {
        self.socket
            .events
            .borrow_mut()
            .push_back(safe_cex_exchange::websocket::WsEvent::Message(
                safe_cex_transport::socket::SocketFrame::Text(frame.to_string()),
            ));
        self.runtime.pump();
    }
    fn account_reads(&self) -> usize {
        self.http
            .1
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/fapi/v2/account")
            .count()
    }
}
impl Drop for PrivateFixture {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
    }
}

#[test]
fn private_order_frame_commits_to_the_production_store() {
    let f = PrivateFixture::new();
    f.frame(json!({"e":"ORDER_TRADE_UPDATE","E":1700000001000.0,"o":{"s":"E-BTC-USDT","i":"socket-order","S":"SELL","o":"LIMIT","x":"NEW","X":"NEW","p":"27300","q":"3","z":"1"}}));
    let orders = f.store.orders().unwrap();
    let order = orders
        .iter()
        .find(|o| o.id == "socket-order")
        .expect("private order Store commit");
    assert_eq!(order.symbol, "E-BTC-USDT");
    assert_eq!(order.side, safe_cex_types::OrderSide::Sell);
    assert_eq!(order.price, 27300.0);
    assert_eq!(order.filled, 1.0);
    assert_eq!(order.remaining, 2.0);
}

#[test]
fn private_order_frames_map_native_types_to_source_limit() {
    let f = PrivateFixture::new();
    f.frame(json!({"e":"ORDER_TRADE_UPDATE","E":1700000001000.0,"o":{"s":"E-BTC-USDT","i":"existing-order","S":"SELL","o":"LIMIT","x":"NEW","X":"NEW","p":"27300","q":"3","z":"1"}}));
    for kind in ["IOC", "FOK", "POST_ONLY", "STOP_LIMIT"] {
        let id = format!("native-{kind}");
        f.frame(json!({"e":"ORDER_TRADE_UPDATE","E":1700000001000.0,"o":{"s":"E-BTC-USDT","i":id,"S":"SELL","o":kind,"x":"NEW","X":"NEW","p":"27300","q":"3","z":"1"}}));
        let orders = f.store.orders().unwrap();
        let order = orders
            .iter()
            .find(|order| order.id == id)
            .expect("native private order reaches Store");
        assert_eq!(
            order.r#type.value(),
            Some(&safe_cex_types::OrderType::Limit),
            "private parseOrderType maps {kind} to limit"
        );
        let native = safe_cex_exchange::js_runtime::value::from_typed(order).unwrap();
        assert!(native.has_own("type"));
        assert_eq!(to_json(&native.get("type").unwrap()).unwrap(), "limit");
        assert_eq!(order.filled, 1.0);
        assert_eq!(order.remaining, 2.0);
    }
    assert!(
        f.store
            .orders()
            .unwrap()
            .iter()
            .any(|order| order.id == "existing-order")
    );
    assert!(f.runtime.reports.borrow().is_empty());
}

#[test]
fn private_trade_frame_emits_incremental_fill_and_commits_order() {
    let f = PrivateFixture::new();
    let fills = Rc::new(RefCell::new(vec![]));
    let capture = fills.clone();
    f.venue.base().on_args(
        "fill",
        Rc::new(move |_, args| {
            assert_eq!(args.len(), 1);
            capture.borrow_mut().push(args[0].clone());
            Ok(())
        }),
    );
    f.frame(json!({"e":"ORDER_TRADE_UPDATE","E":1700000001000.0,"o":{"s":"E-BTC-USDT","i":"socket-fill","S":"BUY","o":"LIMIT","x":"TRADE","X":"PARTIALLY_FILLED","p":"27300","q":"3","z":"1"}}));
    assert_eq!(fills.borrow().len(), 1);
    let fill = fills.borrow()[0].clone();
    assert_eq!(fill.own_entries().unwrap().len(), 5);
    for (key, expected) in [
        ("id", "socket-fill"),
        ("symbol", "E-BTC-USDT"),
        ("side", "buy"),
    ] {
        assert!(fill.get(key).unwrap().strict_equals(&Js::str(expected)));
    }
    assert_eq!(fill.get("price").unwrap().as_f64(), Some(27300.0));
    assert_eq!(fill.get("amount").unwrap().as_f64(), Some(1.0));
    assert_eq!(
        f.store
            .orders()
            .unwrap()
            .iter()
            .find(|o| o.id == "socket-fill")
            .unwrap()
            .filled,
        1.0
    );
}

#[test]
fn private_account_frame_refreshes_positions_through_real_rest_binding() {
    let f = PrivateFixture::new();
    safe_cex_store::Store::update(
        f.store.as_ref(),
        safe_cex_store::StoreUpdate {
            positions: Some(vec![]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let before = f.account_reads();
    f.frame(json!({"e":"ACCOUNT_UPDATE","E":1700000001000.0}));
    f.runtime.advance(350.0);
    assert!(
        f.account_reads() > before,
        "private account frame must trigger the real account read"
    );
    let positions = f.store.positions().unwrap();
    let position = positions
        .iter()
        .find(|p| p.symbol == "E-BTC-USDT")
        .expect("private position refresh Store commit");
    assert_eq!(position.contracts, 0.25);
    assert_eq!(position.entry_price, 27123.0);
    assert_eq!(position.leverage, 7.0);
}

#[test]
fn private_balance_frame_refreshes_balance_through_real_rest_binding() {
    let f = PrivateFixture::new();
    safe_cex_store::Store::update(
        f.store.as_ref(),
        safe_cex_store::StoreUpdate {
            balance: Some(
                serde_json::from_value(json!({"total":0,"free":0,"used":0,"upnl":0})).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let before = f.account_reads();
    f.frame(json!({"e":"BALANCE","E":1700000001000.0}));
    f.runtime.advance(350.0);
    assert!(
        f.account_reads() > before,
        "private balance frame must trigger the real account read"
    );
    let balance = f.store.balance().unwrap();
    assert_eq!(balance.total, 1234.0);
    assert_eq!(balance.free, 1000.0);
    assert_eq!(balance.used, 234.0);
    assert_eq!(balance.upnl, 3.0);
}

#[test]
fn constructs_production_bitrue_and_inherits_source_account_rejection() {
    let runtime = Rc::new(support::Runtime::default());
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value::<ExchangeOptions>(
            serde_json::json!({"key":"","secret":"","applicationId":"fixture-app"}),
        )
        .unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: Rc::new(Http(
                runtime.clone(),
                RefCell::new(vec![]),
                Cell::new(false),
            )),
            external_public_data: None,
        },
    )
    .unwrap();
    assert_eq!(
        complete(&runtime, venue.get_account()).unwrap_err().message,
        "Not implemented"
    );
    venue.dispose().unwrap();
}

#[test]
fn sixteen_trait_methods_use_the_constructed_bitrue_and_scripted_transports() {
    let runtime = Rc::new(support::Runtime::default());
    runtime.now.set(1_700_000_000_000.);
    let http = Rc::new(Http(
        runtime.clone(),
        RefCell::new(vec![]),
        Cell::new(false),
    ));
    let store = Rc::new(DefaultStore::new());
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(
            json!({"key":"fixture-key","secret":"fixture-secret","applicationId":"fixture-app"}),
        )
        .unwrap(),
        store.clone(),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    let worker_calls = Rc::new(RefCell::new(vec![]));
    let (weak, rt, calls) = (Rc::downgrade(&venue), runtime.clone(), worker_calls.clone());
    venue.base().on(
        "callMethod",
        Rc::new(move |_, request| {
            calls.borrow_mut().push(request.clone());
            let weak = weak.clone();
            let request = request.clone();
            rt.spawn_local(Box::pin(async move {
                let owner = weak.upgrade().unwrap();
                owner
                    .base()
                    .emit(
                        "callMethod:done",
                        Js::object(vec![
                            ("requestId", request.get("requestId").unwrap()),
                            ("success", Js::Bool(true)),
                            ("result", Js::Null),
                        ]),
                    )
                    .unwrap();
            }));
            Ok(())
        }),
    );
    let api: &dyn Exchange = venue.as_ref();
    complete(&runtime, api.start()).expect("BIND start");
    assert_eq!(store.markets().unwrap().len(), 1, "BIND start markets");
    assert!(store.loaded().unwrap().markets, "BIND start loaded markets");
    assert_eq!(
        complete(&runtime, api.get_account()).unwrap_err().message,
        "Not implemented",
        "BIND getAccount inherited source rejection"
    );
    assert_eq!(
        serde_json::to_value(
            complete(&runtime, api.validate_account())
                .expect("BIND validateAccount")
                .error
        )
        .unwrap(),
        json!(""),
        "BIND validateAccount success"
    );
    let positions = complete(
        &runtime,
        api.fetch_positions(Some(vec!["E-BTC-USDT".into()])),
    )
    .expect("BIND fetchPositions");
    assert_eq!(positions.len(), 1, "BIND fetchPositions count");
    assert_eq!(
        (
            positions[0].contracts,
            positions[0].entry_price,
            positions[0].leverage
        ),
        (0.25, 27123., 7.),
        "BIND fetchPositions native fields"
    );
    let orders = complete(&runtime, api.fetch_orders(Some(vec!["E-BTC-USDT".into()])))
        .expect("BIND fetchOrders");
    assert_eq!(orders.len(), 1, "BIND fetchOrders count");
    assert_eq!(orders[0].id, "existing-order", "BIND fetchOrders id");
    let markets = complete(&runtime, api.fetch_markets()).expect("BIND fetchMarkets");
    assert_eq!(markets.len(), 1, "BIND fetchMarkets count");
    assert_eq!(
        (markets[0].precision.amount, markets[0].precision.price),
        (0.01, 0.1),
        "BIND fetchMarkets distinct precision axes"
    );
    assert_eq!(
        complete(&runtime, api.fetch_tickers()).unwrap_err().message,
        "Not implemented",
        "BIND fetchTickers inherited source rejection"
    );
    assert_eq!(
        complete(&runtime, api.fetch_symbol_max_leverage("E-BTC-USDT".into()))
            .expect("BIND fetchSymbolMaxLeverage"),
        17.
    );
    complete(&runtime, api.set_leverage("E-BTC-USDT".into(), 9., None)).expect("BIND setLeverage");
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|r| r["url"] == "/fapi/v2/level_edit" && r["data"]["leverage"] == 9.),
        "BIND setLeverage wire"
    );
    assert_eq!(
        complete(&runtime, api.change_position_mode(false, None))
            .unwrap_err()
            .message,
        "Not implemented",
        "BIND changePositionMode inherited source rejection"
    );
    assert_eq!(
        complete(
            &runtime,
            api.change_margin_type(true, Some("E-BTC-USDT".into()))
        )
        .unwrap_err()
        .message,
        "Cannot change Margin Type on Tealstreet as API does not support it. Change it on bitrue.com",
        "BIND changeMarginType source refusal"
    );
    let opts:PlaceOrderOpts=serde_json::from_value(json!({"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.37,"price":27123.49,"clientOrderId":"caller-17"})).unwrap();
    assert_eq!(
        complete(&runtime, api.base().place_order(api, opts.clone())).expect("BIND placeOrder"),
        ["placed-order"]
    );
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|r| r["url"] == "/fapi/v2/order" && r["data"]["clientOrderId"] == "caller-17"),
        "BIND placeOrder caller ID wire"
    );
    let mut first = opts.clone();
    first.client_order_id = Some("batch-a".into());
    let mut second = opts;
    second.client_order_id = Some("batch-b".into());
    let before = http.1.borrow().len();
    assert_eq!(
        complete(&runtime, api.base().place_orders(api, vec![first, second]))
            .expect("BIND batch placement"),
        ["placed-order", "placed-order"]
    );
    let written: Vec<_> = http.1.borrow()[before..]
        .iter()
        .filter(|r| r["url"] == "/fapi/v2/order")
        .map(|r| r["data"]["clientOrderId"].clone())
        .collect();
    assert_eq!(
        written,
        [json!("batch-a"), json!("batch-b")],
        "BIND batch preserves each caller ID"
    );
    let update:UpdateOrderOpts=serde_json::from_value(json!({"order":serde_json::to_value(&orders[0]).unwrap(),"update":{"price":27345.6,"amount":0.33}})).unwrap();
    assert_eq!(
        complete(&runtime, api.update_order(update)).expect("BIND updateOrder"),
        ["placed-order"]
    );
    complete(&runtime, api.cancel_orders(orders)).expect("BIND cancelOrders");
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|r| r["url"] == "/fapi/v2/cancel" && r["data"]["orderId"] == "existing-order"),
        "BIND cancelOrders wire"
    );
    assert_eq!(
        complete(&runtime, api.schedule_cancel(Some(Some(1700000010000.))))
            .unwrap_err()
            .message,
        "Not implemented",
        "BIND scheduleCancel inherited source rejection"
    );
    assert_eq!(
        complete(
            &runtime,
            api.fetch_public_trades("E-BTC-USDT".into(), Some(7.))
        )
        .unwrap_err()
        .message,
        "Not implemented",
        "BIND fetchPublicTrades inherited source rejection"
    );
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|r| r["headers"]["X-CH-APIKEY"] == "fixture-key"
                && r["headers"]["X-CH-SIGN"]
                    .as_str()
                    .is_some_and(|s| s.len() == 64)),
        "BIND native request signing"
    );
    assert!(
        worker_calls.borrow().iter().any(|r| r
            .get("method")
            .unwrap()
            .strict_equals(&Js::str("removeOrders"))
            && r.get("args")
                .unwrap()
                .get("0")
                .unwrap()
                .get("0")
                .unwrap()
                .get("id")
                .unwrap()
                .strict_equals(&Js::str("existing-order"))),
        "BIND cancelOrders worker payload"
    );
    api.dispose().unwrap();
    assert!(
        runtime.sockets.borrow().iter().all(|(_, s)| s.closed.get()),
        "BIND dispose closes both native sockets"
    );
}

#[test]
fn client_ids_fail_closed_for_single_and_whole_batch_before_writes() {
    let runtime = Rc::new(support::Runtime::default());
    let http = Rc::new(Http(
        runtime.clone(),
        RefCell::new(vec![]),
        Cell::new(false),
    ));
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    let opts = |id: &str| {
        serde_json::from_value::<PlaceOrderOpts>(json!({"symbol":"E-BTC-USDT","type":"limit","side":"buy","amount":0.37,"price":27123.49,"clientOrderId":id})).unwrap()
    };
    let error = complete(
        &runtime,
        venue
            .base()
            .place_order(venue.as_ref(), opts(&"x".repeat(32))),
    )
    .unwrap_err();
    assert!(
        error.message.contains("clientOrderId"),
        "CLIENT-ID single {error:?}"
    );
    let error = complete(
        &runtime,
        venue
            .base()
            .place_orders(venue.as_ref(), vec![opts("valid"), opts(&"x".repeat(32))]),
    )
    .unwrap_err();
    assert!(
        error.message.contains("clientOrderId"),
        "CLIENT-ID batch {error:?}"
    );
    assert!(
        http.1.borrow().is_empty(),
        "CLIENT-ID no signed request for rejected batch"
    );
    venue.dispose().unwrap();
}

#[test]
fn private_listen_key_failure_never_dials_or_subscribes_and_public_binary_uses_plain_json() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let runtime = Rc::new(support::Runtime::default());
    let http = Rc::new(Http(runtime.clone(), RefCell::new(vec![]), Cell::new(true)));
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    complete(&runtime, venue.start()).unwrap();
    for _ in 0..8 {
        if http
            .1
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/user_stream/api/v1/listenKey")
            .count()
            == 4
        {
            break;
        }
        runtime.advance(1000.);
    }
    assert_eq!(
        http.1
            .borrow()
            .iter()
            .filter(|r| r["url"] == "/user_stream/api/v1/listenKey")
            .count(),
        4,
        "AUTH source retry count"
    );
    assert!(
        runtime
            .sockets
            .borrow()
            .iter()
            .all(|(url, _)| !url.contains("listenKey")),
        "AUTH no private dial without acquired key"
    );
    let public = runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.contains("/kline-api/ws"))
        .map(|(_, s)| s.clone())
        .expect("public socket");
    public
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Binary(
            br#"{"ping":9007199254740993}"#.to_vec(),
        )));
    runtime.pump();
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|s| s == "{\"pong\":9007199254740992}"),
        "PUBLIC binary Buffer coerces through plain jsonParse: {:?}",
        public.sent.borrow()
    );
    venue.dispose().unwrap();
}

#[test]
#[expect(clippy::arc_with_non_send_sync)]
fn source_routing_authorizes_signed_requests_and_installs_bigint_only_for_tunnel() {
    use safe_cex_transport::{
        DirectTransport, PrivateProxy, ResolvedRouting, RestAdapter, RestAdapterContext,
        RestAdapterVariant, RestAuthorizer, RestFuture, RestRequest, RestResponse,
        RestResponseBody, RestTransport, RestTransportContext, RoutingOptions, TransportError,
        TransportFuture, TransportProvider, WsConnection,
    };
    use std::sync::Arc;
    struct Authorize;
    impl RestAuthorizer for Authorize {
        fn authorize<'a>(&'a self, request: &'a mut RestRequest) -> TransportFuture<'a, ()> {
            Box::pin(async move {
                assert!(
                    request
                        .headers
                        .iter()
                        .any(|(k, v)| k.eq_ignore_ascii_case("X-CH-SIGN") && v.len() == 64),
                    "ROUTING venue signs before proxy authorization"
                );
                request
                    .headers
                    .insert("Proxy-Authorization".into(), "Bearer routed".into());
                Ok(())
            })
        }
    }
    struct Terminal(Rc<RefCell<Vec<RestRequest>>>);
    impl RestAdapter for Terminal {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.0.borrow_mut().push(request);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            json!({"code":"0","data":{}}),
                        )
                        .unwrap(),
                    ),
                })
            })
        }
    }
    struct Provider {
        variants: Rc<RefCell<Vec<RestAdapterVariant>>>,
        sent: Rc<RefCell<Vec<RestRequest>>>,
    }
    impl TransportProvider for Provider {
        fn name(&self) -> &str {
            "binding-routing"
        }
        fn rest(&self, c: RestTransportContext<'_>) -> Result<RestTransport, TransportError> {
            assert_eq!(
                c.exchange,
                &ExchangeName::Bitrue,
                "ROUTING exchange selection"
            );
            assert_eq!(
                c.config.base_url.as_deref(),
                Some("https://fapi.bitrue.com"),
                "ROUTING original base"
            );
            Ok(RestTransport {
                base_url: Some("https://routing-origin".into()),
                proxied: true,
                authorize: Some(Arc::new(Authorize)),
            })
        }
        fn rest_adapter(&self, c: RestAdapterContext<'_>) -> Option<Arc<dyn RestAdapter>> {
            self.variants.borrow_mut().push(c.variant);
            Some(Arc::new(Terminal(self.sent.clone())))
        }
        fn proxy_prefix(&self, _: RestTransportContext<'_>) -> String {
            String::new()
        }
        fn public_cors_proxy(&self) -> Option<String> {
            None
        }
        fn private_proxy(&self, _: &ExchangeName) -> Option<Arc<dyn PrivateProxy>> {
            None
        }
        fn require_proxy(&self, options: ExchangeOptions) -> ExchangeOptions {
            options
        }
        fn server_proxy_url(&self, _: &ExchangeName) -> Option<String> {
            None
        }
        fn ws_routing(&self, name: &ExchangeName, options: &RoutingOptions) -> ResolvedRouting {
            DirectTransport.ws_routing(name, options)
        }
        fn ws_connection(&self, _: &str) -> Option<WsConnection> {
            None
        }
    }
    for tunnel in [false, true] {
        let runtime = Rc::new(support::Runtime::default());
        let variants = Rc::new(RefCell::new(vec![]));
        let sent = Rc::new(RefCell::new(vec![]));
        *runtime.provider.borrow_mut() = Some(Arc::new(Provider {
            variants: variants.clone(),
            sent: sent.clone(),
        }));
        let http = Rc::new(Http(
            runtime.clone(),
            RefCell::new(vec![]),
            Cell::new(false),
        ));
        let venue=BitrueExchange::new(ExchangeName::Bitrue,serde_json::from_value(json!({"key":"key","secret":"secret","tealstreetTunnel":tunnel,"extra":{"recvWindow":12345}})).unwrap(),Rc::new(DefaultStore::new()),BitrueDependencies {runtime:runtime.clone(),http:http.clone(),external_public_data:None}).expect("ROUTING constructed with provider");
        assert_eq!(
            serde_json::to_value(complete(&runtime, venue.validate_account()).unwrap().error)
                .unwrap(),
            json!(""),
            "ROUTING validation"
        );
        if tunnel {
            assert_eq!(
                *variants.borrow(),
                [RestAdapterVariant::BigInt, RestAdapterVariant::BigInt],
                "ROUTING both source clients use bigInt adapter"
            );
            assert!(
                http.1.borrow().is_empty(),
                "ROUTING installed adapter replaces terminal host"
            );
            let requests = sent.borrow();
            assert_eq!(
                requests.len(),
                2,
                "ROUTING source validation then snapshot reads"
            );
            assert_eq!(
                requests[0].base_url.as_deref(),
                Some("https://routing-origin")
            );
            assert_eq!(
                requests[0].timeout_ms,
                Some(20_000),
                "ROUTING source factory overrides recvWindow timeout"
            );
            assert!(requests[0].headers.iter().any(|(k, v)| {
                k.eq_ignore_ascii_case("Proxy-Authorization") && v == "Bearer routed"
            }));
        } else {
            assert!(
                variants.borrow().is_empty(),
                "ROUTING source does not install tunnel adapter without flag"
            );
            let requests = http.1.borrow();
            assert_eq!(
                requests.len(),
                2,
                "ROUTING source validation then snapshot reads"
            );
            assert_eq!(
                requests[0]["baseURL"], "https://routing-origin",
                "ROUTING selected factory base"
            );
            assert_eq!(
                requests[0]["headers"]["Proxy-Authorization"],
                "Bearer routed"
            );
        }
        venue.dispose().unwrap();
    }
}

#[test]
fn connected_candle_history_resolves_the_real_promise_executor_and_caches_the_response() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let runtime = Rc::new(support::Runtime::default());
    let http = Rc::new(Http(
        runtime.clone(),
        RefCell::new(vec![]),
        Cell::new(false),
    ));
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http,
            external_public_data: None,
        },
    )
    .unwrap();
    complete(&runtime, venue.start()).unwrap();
    let public_socket = || {
        runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("/kline-api/ws"))
            .map(|(_, s)| s.clone())
    };
    for _ in 0..16 {
        if public_socket().is_some() {
            break;
        }
        runtime.advance(1000.);
    }
    let public = public_socket().expect("actual public socket");
    runtime.pump();
    let opts = || serde_json::from_value(json!({"symbol":"E-BTC-USDT","interval":"15m"})).unwrap();
    let mut future = venue.fetch_ohlcv(opts());
    let mut cx = Context::from_waker(Waker::noop());
    assert!(
        future.as_mut().poll(&mut cx).is_pending(),
        "real Promise executor must return a pending candle request"
    );
    let topic = "market_e_btcusdt_kline_15min";
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|s| s.contains("\"event\":\"req\"") && s.contains(topic)),
        "Promise executor sends the actual request synchronously"
    );
    public
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({
                "event_rep":"rep","channel":topic,"data":[]
            })
            .to_string(),
        )));
    runtime.pump();
    let candles =
        complete(&runtime, future).expect("connected fetchOHLCV resolves server response");
    assert!(
        candles.is_empty(),
        "empty server history resolves without inventing rows"
    );
    let before = public.sent.borrow().len();
    let cached = complete(&runtime, venue.fetch_ohlcv(opts())).unwrap();
    assert!(cached.is_empty());
    assert_eq!(
        public.sent.borrow().len(),
        before,
        "source ten-second request cache avoids another request"
    );
    runtime.advance(11000.);
    let mut timeout = venue.fetch_ohlcv(opts());
    assert!(timeout.as_mut().poll(&mut cx).is_pending());
    runtime.advance(5000.);
    let error =
        complete(&runtime, timeout).expect_err("request timeout rejects the constructor promise");
    assert_eq!(error.message, "Request timeout");
    assert!(
        runtime.reports.borrow().is_empty(),
        "resolved request clears its timeout"
    );
    venue.dispose().unwrap();
}

struct AccountRowsHttp(Rc<Http>, Value);
impl JsHttpHost for AccountRowsHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.0.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        if config.get("url").unwrap().to_display() != "/fapi/v2/account" {
            return self.0.send(config);
        }
        let response = Js::object(vec![
            ("status", Js::Number(200.)),
            ("data", from_json(&self.1)),
            ("config", config),
        ]);
        Box::pin(async move { Ok(response) })
    }
}
#[test]
fn malformed_balance_upnl_refuses_only_its_account_row() {
    for bad in [json!("bad-pnl"), json!(null), json!({}), json!(true)] {
        let runtime = Rc::new(support::Runtime::default());
        let http = Rc::new(AccountRowsHttp(
            Rc::new(Http(
                runtime.clone(),
                RefCell::new(vec![]),
                Cell::new(false),
            )),
            json!({"code":"0","data":{"account":[
                {"marginCoin":"USDT","totalEquity":100,"accountNormal":70,"sumOpenRealizedAmount":3},
                {"marginCoin":"USDT","totalEquity":999,"accountNormal":900,"sumOpenRealizedAmount":bad},
                {"marginCoin":"USDC","totalEquity":200,"accountNormal":150,"sumOpenRealizedAmount":7}
            ]}}),
        ));
        let store = Rc::new(DefaultStore::new());
        let venue = BitrueExchange::new(
            ExchangeName::Bitrue,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            store.clone(),
            BitrueDependencies {
                runtime: runtime.clone(),
                http,
                external_public_data: None,
            },
        )
        .unwrap();
        complete(&runtime, venue.start()).expect("bad upnl must not reject the valid account rows");
        let balance = store.balance().unwrap();
        assert_eq!(
            (balance.total, balance.free, balance.used, balance.upnl),
            (100., 70., 30., 3.)
        );
        let currencies = balance.currencies.as_ref().unwrap();
        assert_eq!(currencies.len(), 2);
        let usdc = currencies.get("USDC").unwrap().as_balance().unwrap();
        assert_eq!(
            (usdc.total, usdc.free, usdc.used, usdc.upnl),
            (200., 150., 50., 7.)
        );
        assert_eq!(
            balance
                .info
                .as_ref()
                .unwrap()
                .get("data")
                .unwrap()
                .get("account")
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let warnings = runtime.console.borrow();
        let refusals: Vec<_> = warnings
            .iter()
            .filter(|line| line.contains("Balance.upnl"))
            .collect();
        assert!(!refusals.is_empty());
        assert!(
            refusals
                .iter()
                .all(|line| line.contains("USDT") && line.contains(&bad.to_string())),
            "warning must identify field, currency and original value: {warnings:?}"
        );
        assert!(runtime.reports.borrow().is_empty());
        drop(balance);
        venue.dispose().unwrap();
    }
}

#[test]
fn public_book_pairs_reject_noniterables_through_the_constructed_socket() {
    let unhandled = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/composition-streams-oracle.cjs"
        ))
        .arg("--unhandled-book-pair")
        .output()
        .unwrap();
    assert!(
        unhandled.status.success(),
        "{}",
        String::from_utf8_lossy(&unhandled.stderr)
    );
    let unhandled: Value = serde_json::from_slice(&unhandled.stdout).unwrap();
    assert_eq!(unhandled["callbacks"], 0);
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/composition-streams-oracle.cjs"
        ))
        .arg("--book-pairs")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let source = source["rows"]["book"].as_array().unwrap();
    assert_eq!(source.len(), 3);
    assert_eq!(source[2]["error"], json!({"undefined":true}));
    for (index, pair) in [json!({"0":2,"1":3}), json!(2)].into_iter().enumerate() {
        assert_eq!(
            source[index]["error"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry[0] == "name")
                .unwrap()[1],
            "TypeError"
        );
        let f = PrivateFixture::new();
        let books = Rc::new(RefCell::new(Vec::new()));
        let sink = books.clone();
        let cleanup = f
            .venue
            .listen_order_book(
                "E-BTC-USDT".into(),
                Rc::new(move |book| sink.borrow_mut().push(book)),
            )
            .unwrap();
        f.runtime.pump();
        f.runtime.advance(100.0);
        let public = f
            .runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| !url.contains("listenKey="))
            .unwrap()
            .1
            .clone();
        assert!(
            public
                .sent
                .borrow()
                .iter()
                .any(|raw| raw.contains("market_e_btcusdt_simple_depth_step0"))
        );
        let push = |buys: Value, asks: Value| {
            public.events.borrow_mut().push_back(safe_cex_exchange::websocket::WsEvent::Message(
                safe_cex_transport::socket::SocketFrame::Text(json!({
                    "channel":"market_e_btcusdt_simple_depth_step0","tick":{"buys":buys,"asks":asks}
                }).to_string())));
            f.runtime.pump();
        };
        assert!(f.runtime.exceptions.borrow().is_empty());
        push(json!([pair]), json!([]));
        assert!(
            books.borrow().is_empty(),
            "source refuses noniterable book row {index}"
        );
        {
            let reports = f.runtime.exceptions.borrow();
            assert_eq!(
                reports.len(),
                1,
                "shared unhandled async rejection must be surfaced exactly once"
            );
            let (error, context) = &reports[0];
            assert_eq!(error.name, unhandled["rejection"]["name"].as_str().unwrap());
            let expected = source[index]["error"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|entry| entry[0] == "message")
                .unwrap();
            assert_eq!(error.message, expected[1].as_str().unwrap());
            assert!(
                context
                    .get("source")
                    .is_some_and(|source| source.text_equals("unhandledRejection"))
            );
            if index == 0 {
                assert_eq!(
                    error.message,
                    unhandled["rejection"]["message"].as_str().unwrap()
                );
            }
        }
        push(json!([[2, 3]]), json!([[4, 5]]));
        let books = books.borrow();
        assert_eq!(books.len(), 1, "next valid snapshot recovers after refusal");
        assert_eq!(
            (
                books[0].bids[0].price,
                books[0].bids[0].amount,
                books[0].bids[0].total
            ),
            (2.0, 3.0, 3.0)
        );
        assert_eq!(
            (
                books[0].asks[0].price,
                books[0].asks[0].amount,
                books[0].asks[0].total
            ),
            (4.0, 5.0, 5.0)
        );
        drop(books);
        cleanup().unwrap();
        f.runtime.pump();
        assert_eq!(
            f.runtime.exceptions.borrow().len(),
            1,
            "valid recovery and cleanup must not duplicate the rejection report"
        );
    }
}

#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[test]
fn production_ticker_coercion_matches_constructed_typescript() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_transport::socket::SocketFrame;
    let source = ticker_coercion::source("bitrue");
    assert_eq!(source["cases"].as_array().unwrap().len(), 32);
    let f = PrivateFixture::new();
    let template = f.store.markets().unwrap()[0].clone();
    let tickers = ticker_coercion::seed("bitrue");
    let markets = tickers
        .iter()
        .map(|ticker| {
            let mut market = template.clone();
            market.id.clone_from(&ticker.id);
            market.symbol.clone_from(&ticker.symbol);
            market
        })
        .collect();
    Store::update(
        f.store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            tickers: Some(tickers),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    f.venue
        .base()
        .emit(
            "markets:changed",
            from_json(&json!({"added":[{"symbol":"E-ETH-USDT"},{"symbol":"E-ADA-USDT"}]})),
        )
        .unwrap();
    f.runtime.pump();
    let socket = f
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.contains("listenKey="))
        .unwrap()
        .1
        .clone();
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            f.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed("bitrue")),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        for frame in case["frames"].as_array().unwrap() {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    frame.as_str().unwrap().into(),
                )));
            f.runtime.pump();
        }
        ticker_coercion::assert_native(&f.store.tickers().unwrap(), case);
    }
    assert!(
        f.runtime.reports.borrow().is_empty(),
        "{:?}",
        f.runtime.reports.borrow()
    );
}

#[test]
fn listen_ohlcv_maps_singapore_kline_times_through_the_real_dayjs_global() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let runtime = Rc::new(support::Runtime::default());
    let http = Rc::new(Http(
        runtime.clone(),
        RefCell::new(vec![]),
        Cell::new(false),
    ));
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: http.clone(),
            external_public_data: None,
        },
    )
    .unwrap();
    complete(&runtime, venue.start()).unwrap();
    let public_socket = || {
        runtime
            .sockets
            .borrow()
            .iter()
            .find(|(url, _)| url.contains("/kline-api/ws"))
            .map(|(_, s)| s.clone())
    };
    for _ in 0..16 {
        if public_socket().is_some() {
            break;
        }
        runtime.advance(1000.);
    }
    let public = public_socket().expect("public socket");
    let candles = Rc::new(RefCell::new(vec![]));
    let sink = candles.clone();
    let cleanup = venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"E-BTC-USDT","interval":"15m"})).unwrap(),
            Rc::new(move |c| sink.borrow_mut().push(c)),
        )
        .expect("BIND listenOHLCV");
    runtime.pump();
    let topic = "market_e_btcusdt_kline_15min";
    assert!(
        public
            .sent
            .borrow()
            .iter()
            .any(|s| s.contains("\"event\":\"sub\"") && s.contains(topic)),
        "listenOHLCV subscribes the kline topic: {:?}",
        public.sent.borrow()
    );
    // Bitrue `ds` is Singapore wall time; real dayjs+utc in a UTC host gives these seconds.
    for tick in [
        json!({"ds":"2024-03-10 08:00:00","open":1,"high":3,"low":0.5,"close":2,"vol":10,"amount":20,"id":1,"tradeId":1}),
        json!({"ds":"2024-03-10 08:15:00","open":2,"high":4,"low":1.5,"close":3,"vol":11,"amount":33,"id":2,"tradeId":2}),
    ] {
        let frame = json!({"channel":topic,"tick":tick});
        public
            .events
            .borrow_mut()
            .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
        runtime.pump();
    }
    let fields: Vec<_> = candles
        .borrow()
        .iter()
        .map(|c: &safe_cex_types::Candle| (c.timestamp, c.open, c.high, c.low, c.close, c.volume))
        .collect();
    assert_eq!(
        fields,
        vec![
            (1_710_028_800., 1., 3., 0.5, 2., 10.),
            (1_710_029_700., 2., 4., 1.5, 3., 11.)
        ],
        "reports: {:?}",
        runtime.reports.borrow()
    );
    cleanup().unwrap();
    venue.dispose().unwrap();
}

#[test]
fn public_candles_keep_singapore_time_and_distinct_fields_through_socket() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/candle-stream-source.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 4);
    let f = PrivateFixture::new();
    let core = f
        .venue
        .base()
        .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Public)
        .unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let cleanup = f
        .venue
        .listen_ohlcv(
            serde_json::from_value(json!({"symbol":"E-BTC-USDT","interval":"1m"})).unwrap(),
            Rc::new(move |c| sink.borrow_mut().push(c)),
        )
        .unwrap();
    let public = f
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| !url.contains("listenKey"))
        .unwrap()
        .1
        .clone();
    let subscription: Value = public
        .sent
        .borrow()
        .iter()
        .filter_map(|raw| serde_json::from_str::<Value>(raw).ok())
        .find(|row| {
            row["event"] == "sub"
                && row["params"]["channel"]
                    .as_str()
                    .is_some_and(|topic| topic.contains("kline"))
        })
        .expect("actual candle subscription");
    let topic = subscription["params"]["channel"].as_str().unwrap();
    for (index, case) in cases.iter().enumerate() {
        let raw = format!(
            "{{\"channel\":{},\"data\":{}}}",
            serde_json::to_string(topic).unwrap(),
            case["raw"].as_str().unwrap()
        );
        core.on_message(&safe_cex_transport::socket::SocketFrame::Text(raw))
            .unwrap();
        f.runtime.pump();
        assert_eq!(
            seen.borrow().len(),
            index + 1,
            "actual candle callback {index}; reports={:?}, console={:?}",
            f.runtime.reports.borrow(),
            f.runtime.console.borrow()
        );
        let candle = &seen.borrow()[index];
        for (field, value) in [
            ("timestamp", candle.timestamp),
            ("open", candle.open),
            ("high", candle.high),
            ("low", candle.low),
            ("close", candle.close),
            ("volume", candle.volume),
        ] {
            let expected = case["fields"][field].as_str().unwrap();
            if expected == "nan" {
                assert!(value.is_nan(), "case {index}: {field}");
            } else {
                assert_eq!(
                    format!("{:016x}", value.to_bits()),
                    expected,
                    "case {index}: {field}"
                );
            }
        }
    }
    cleanup().unwrap();
    core.on_message(&safe_cex_transport::socket::SocketFrame::Text(format!(
        "{{\"channel\":{},\"data\":{}}}",
        serde_json::to_string(topic).unwrap(),
        cases[0]["raw"].as_str().unwrap()
    )))
    .unwrap();
    f.runtime.pump();
    assert_eq!(
        seen.borrow().len(),
        4,
        "cleanup removes the candle callback"
    );
}

#[test]
fn shared_dayjs_exposes_the_utc_offset_used_by_live_candles() {
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::library::install_library::<()>(&globals, std::rc::Weak::new());
    let date = globals
        .get("dayjs")
        .unwrap()
        .call(Js::Undefined, vec![Js::str("2024-06-08 01:23:45")])
        .unwrap();
    assert!(
        date.get("utcOffset").unwrap().is_callable(),
        "shared Dayjs must expose source utcOffset"
    );
}
