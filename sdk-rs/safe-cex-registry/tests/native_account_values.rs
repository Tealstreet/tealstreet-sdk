#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, open_native_account,
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use tokio::net::TcpListener;

#[test]
fn actual_order_results_retain_nonfinite_native_numbers() {
    support::run_local(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let seen = Rc::new(RefCell::new(vec![]));
        tokio::task::spawn_local(support::serve_http(
            listener,
            Rc::new(|request| {
                assert_eq!(request.method, "GET");
                if request.path.ends_with("/openAlgoOrders") {
                    return Some(json!([]));
                }
                assert!(request.path.ends_with("/openOrders"), "{request:?}");
                Some(json!(["not-a-number", "Infinity", "-Infinity"].into_iter().enumerate().map(|(i, price)| json!({
                "orderId":i+1,"clientOrderId":format!("client-{i}"),"symbol":"BTCUSDT","side":"BUY","type":"LIMIT",
                "origQty":"1","price":price,"stopPrice":"not-a-number","status":"NEW","executedQty":"0",
                "reduceOnly":false,"positionSide":"BOTH","time":1
            })).collect::<Vec<_>>()))
            }),
            seen.clone(),
        ));
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: "binance".into(),
            options: json!({"key":"test-key","secret":"test-secret","overrideProxyUrl":origin}),
            account_name: None,
            sign: None,
            proxy_token: None,
        })
        .unwrap();
        let orders = native_account_call(account.clone(), "fetchOrders".into(), vec![json!(null)])
            .await
            .unwrap();
        assert_eq!(seen.borrow().len(), 2);
        assert!(
            orders[0]["price"].as_f64().is_some_and(f64::is_nan),
            "{orders:?}"
        );
        assert_eq!(orders[1]["price"].as_f64(), Some(f64::INFINITY));
        assert_eq!(orders[2]["price"].as_f64(), Some(f64::NEG_INFINITY));
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}
