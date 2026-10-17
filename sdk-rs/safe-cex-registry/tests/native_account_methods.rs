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
        options: json!({"key":"test-key","secret":"test-secret","overrideProxyUrl":origin}),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap()
}
async fn call(account: &Rc<NativeAccount>, method: &str, args: Value) -> Value {
    native_account_call(
        account.clone(),
        method.into(),
        args.as_array().unwrap().clone(),
    )
    .await
    .unwrap_or_else(|error| panic!("{method}: {error}"))
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
            Rc::new(move |data| captured.borrow_mut().push(data)),
        );
        call(&account, "fetchAndSetOrders", json!([null])).await;
        assert_eq!(stores.borrow().last().unwrap()["orders"][0]["id"], "77");
        assert_eq!(
            call(&account, "removeOrders", json!([[{"id":"77"}]])).await,
            Value::Null
        );
        assert_eq!(stores.borrow().last().unwrap()["orders"], json!([]));
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
