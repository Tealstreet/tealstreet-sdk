#[path = "support/binding_runtime.rs"]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, from_json, to_json},
    },
    runtime::ExchangeRuntime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, ExchangeOptions, PlaceOrderOpts, UpdateOrderOpts};
use safe_cex_venue_bitrue::{BitrueDependencies, BitrueExchange};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Http(Rc<support::Runtime>, RefCell<Vec<Value>>);
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
                json!({"code":200,"data":{"listenKey":"fixture-listen-key"}})
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
            http: Rc::new(Http(runtime.clone(), RefCell::new(vec![]))),
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
    let http = Rc::new(Http(runtime.clone(), RefCell::new(vec![])));
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
                        json!({"requestId":request["requestId"],"success":true,"result":null}),
                    )
                    .unwrap();
            }));
            Ok(())
        }),
    );
    let api: &dyn Exchange = venue.as_ref();
    complete(&runtime, api.start()).expect("BIND start");
    assert_eq!(store.markets().len(), 1, "BIND start markets");
    assert!(store.loaded().markets, "BIND start loaded markets");
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
        complete(&runtime, api.base().place_order(api, opts)).expect("BIND placeOrder"),
        ["placed-order"]
    );
    assert!(
        http.1
            .borrow()
            .iter()
            .any(|r| r["url"] == "/fapi/v2/order" && r["data"]["clientOrderId"] == "caller-17"),
        "BIND placeOrder caller ID wire"
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
        worker_calls
            .borrow()
            .iter()
            .any(|r| r["method"] == "removeOrders" && r["args"][0][0]["id"] == "existing-order"),
        "BIND cancelOrders worker payload"
    );
    api.dispose().unwrap();
    assert!(
        runtime.sockets.borrow().iter().all(|(_, s)| s.closed.get()),
        "BIND dispose closes both native sockets"
    );
}
