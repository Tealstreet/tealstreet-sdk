//! The public JSON account entrypoints, using production registry construction and
//! loopback HTTP. No replacement exchange or alternate dispatcher is involved.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccount, NativeAccountSpec, native_account_call, native_account_watch,
    open_native_account,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
use support::{Seen, run_local, serve_http};
use tokio::net::TcpListener;

async fn account() -> Rc<NativeAccount> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(|request: &Seen| {
            assert_eq!(request.method, "GET");
            if request.path.ends_with("/fapi/v1/openOrders") {
                Some(
                    json!([{"orderId":77,"clientOrderId":"lost-response","symbol":"BTCUSDT",
                "side":"BUY","type":"LIMIT","origQty":"1","price":"10","stopPrice":"0",
                "status":"NEW","executedQty":"0","reduceOnly":false,"positionSide":"BOTH","time":1}]),
                )
            } else if request.path.ends_with("/fapi/v1/openAlgoOrders") {
                Some(json!([]))
            } else {
                panic!("unexpected request: {request:?}")
            }
        }),
        Rc::default(),
    ));
    open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "binance".into(),
        options: json!({"key":"test-key","secret":"test-secret","overrideProxyUrl":origin}).into(),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap()
}
async fn call(account: &Rc<NativeAccount>, method: &str, args: Value) -> Value {
    support::boundary_json(
        native_account_call(
            account.clone(),
            method.into(),
            args.as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(Into::into)
                .collect(),
        )
        .await
        .unwrap_or_else(|error| panic!("{method}: {error:?}")),
    )
}

#[test]
fn reconciles_a_lost_placement_response_from_native_store() {
    run_local(async {
        let account = account().await;
        call(&account, "fetchAndSetOrders", json!([null])).await;
        let found = call(
            &account,
            "findOrderByClientOrderId",
            json!(["BTCUSDT", "lost-response"]),
        )
        .await;
        assert_eq!(found["found"], true);
        assert_eq!(found["order"]["id"], "77");
        assert_eq!(
            call(
                &account,
                "findOrderByClientOrderId",
                json!(["ETHUSDT", "lost-response"])
            )
            .await,
            json!({"found":false,"definitive":false})
        );
        call(&account, "dispose", json!([])).await;
    });
}

#[test]
fn removes_orders_by_id_and_posts_the_mutated_store() {
    run_local(async {
        let account = account().await;
        let stores = Rc::new(RefCell::new(vec![]));
        let captured = stores.clone();
        let unwatch = native_account_watch(
            &account,
            Rc::new(|_, _| {}),
            Rc::new(move |data| captured.borrow_mut().push(support::boundary_json(data))),
        );
        call(&account, "fetchAndSetOrders", json!([null])).await;
        assert_eq!(stores.borrow().last().unwrap()["orders"][0]["id"], "77");
        assert_eq!(
            call(&account, "removeOrders", json!([[{"id":"77"}]])).await,
            Value::Null
        );
        support::eventually("throttled order removal Store post", || {
            stores.borrow().last().unwrap()["orders"] == json!([])
        })
        .await;
        unwatch();
        call(&account, "dispose", json!([])).await;
    });
}

#[test]
fn reads_websocket_logs_when_no_sockets_exist() {
    run_local(async {
        let account = account().await;
        assert_eq!(
            call(&account, "fetchLastLogsFromWebsockets", json!([])).await,
            json!([])
        );
        call(&account, "dispose", json!([])).await;
    });
}

#[test]
fn reads_initial_active_ticker_symbols() {
    run_local(async {
        let account = account().await;
        assert_eq!(
            call(&account, "getActiveTickerSymbols", json!([])).await,
            json!([])
        );
        call(&account, "dispose", json!([])).await;
    });
}

#[test]
fn sets_and_reads_normalized_active_ticker_symbols() {
    run_local(async {
        let account = account().await;
        assert_eq!(
            call(
                &account,
                "setActiveTickerSymbols",
                json!([[" btcusdt ", "ETHUSDT", "BTCUSDT", "ETHUSDT", ""]])
            )
            .await,
            Value::Null
        );
        assert_eq!(
            call(&account, "getActiveTickerSymbols", json!([])).await,
            json!([" btcusdt ", "ETHUSDT", "BTCUSDT"])
        );
        call(&account, "dispose", json!([])).await;
    });
}

// ts: createRemoteSigningAdapter signs every config, so hosted MEXC order-deals
// (native query params) is signed through the production HostSigner and sent.
#[test]
fn hosted_signer_signs_native_query_requests() {
    run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let seen = Rc::new(RefCell::new(vec![]));
        tokio::task::spawn_local(serve_http(
            listener,
            Rc::new(|request: &Seen| {
                assert!(
                    request
                        .path
                        .ends_with("/api/v1/private/order/list/order_deals"),
                    "{request:?}"
                );
                Some(json!({"success":true,"code":0,"data":[]}))
            }),
            seen.clone(),
        ));
        let signed = Rc::new(RefCell::new(vec![]));
        let captured = signed.clone();
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: "mexc".into(),
            options: json!({"key":"hosted-key","secret":"","overrideProxyUrl":origin}).into(),
            account_name: Some("mexc-hosted".into()),
            sign: Some(Rc::new(move |wire| {
                let wire = support::boundary_json(wire);
                captured.borrow_mut().push(wire.clone());
                let mut headers = wire["config"]["headers"].clone();
                headers["X-Hosted-Signature"] = json!("host-signed");
                let response = json!({"id":wire["id"],"config":{
                    "params":wire["config"]["params"],"headers":headers}});
                Box::pin(async move { Ok(response.into()) })
            })),
            proxy_token: None,
        })
        .unwrap();
        assert_eq!(
            call(
                &account,
                "fetchHistoricalFills",
                json!([{"symbol":"BTCUSDT"}])
            )
            .await,
            json!([])
        );
        let wire = signed.borrow()[0].clone();
        assert_eq!(wire["type"], "http");
        assert_eq!(wire["exchangeName"], "mexc");
        assert_eq!(wire["accountName"], "mexc-hosted");
        assert_eq!(
            wire["config"]["params"],
            json!({"symbol":"BTC_USDT","page_num":1.0,"page_size":100.0})
        );
        let seen = seen.take();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].headers["x-hosted-signature"], "host-signed");
        let query: Vec<_> = seen[0].query.split('&').collect();
        assert!(query.contains(&"page_num=1"), "{}", seen[0].query);
        assert!(query.contains(&"page_size=100"), "{}", seen[0].query);
        assert!(
            seen[0].query.contains("symbol=BTC_USDT"),
            "{}",
            seen[0].query
        );
        call(&account, "dispose", json!([])).await;
    });
}
