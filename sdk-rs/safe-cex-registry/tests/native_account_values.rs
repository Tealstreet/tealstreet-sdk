#![cfg(not(target_arch = "wasm32"))]
#[path = "support/native_account.rs"]
mod account;
mod support;
use safe_cex_registry::native_account::{
    NativeAccountSpec, native_account_call, native_account_watch, open_native_account,
};
use safe_cex_registry::native_value::NativeValue;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use tokio::net::TcpListener;

#[test]
fn actual_order_results_retain_nonfinite_native_numbers() {
    account::run(async {
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
                Some(json!(["not-a-number", "Infinity", "-Infinity", "-0"].into_iter().enumerate().map(|(i, price)| json!({
                "orderId":i+1,"clientOrderId":format!("client-{i}"),"symbol":"BTCUSDT","side":"BUY","type":if i == 3 {"UNKNOWN"} else {"LIMIT"},
                "origQty":"1","price":price,"stopPrice":if i == 3 {"-0"} else {"not-a-number"},"status":"NEW","executedQty":"0",
                "reduceOnly":false,"positionSide":"BOTH","time":1
            })).collect::<Vec<_>>()))
            }),
            seen.clone(),
        ));
        let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: "binance".into(),
            options: json!({"key":"test-key","secret":"test-secret","overrideProxyUrl":origin})
                .into(),
            account_name: None,
            sign: None,
            proxy_token: None,
        })
        .unwrap();
        let stores = Rc::new(RefCell::new(vec![]));
        let captured = stores.clone();
        let unwatch = native_account_watch(
            &account,
            Rc::new(|_, _| {}),
            Rc::new(move |data| captured.borrow_mut().push(data)),
        );
        let orders = native_account_call(
            account.clone(),
            "fetchOrders".into(),
            vec![json!(null).into()],
        )
        .await
        .unwrap();
        assert_eq!(seen.borrow().len(), 2);
        assert!(
            orders[0]["price"].as_f64().is_some_and(f64::is_nan),
            "{orders:?}"
        );
        assert_eq!(orders[1]["price"].as_f64(), Some(f64::INFINITY));
        assert_eq!(orders[2]["price"].as_f64(), Some(f64::NEG_INFINITY));
        assert_eq!(
            orders[3]["price"].as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        assert!(matches!(
            orders[3].get("type"),
            Some(NativeValue::Undefined)
        ));
        assert!(orders[3].get("parentId").is_none());
        let error = native_account_call(
            account.clone(),
            "setLeverage".into(),
            vec![
                NativeValue::string("BTCUSDT"),
                NativeValue::Number(f64::NAN),
            ],
        )
        .await
        .unwrap_err();
        assert_eq!(error["message"], "Market BTCUSDT not found");
        native_account_call(account.clone(), "fetchAndSetOrders".into(), vec![])
            .await
            .unwrap();
        support::eventually("native order Store delivery", || {
            stores
                .borrow()
                .iter()
                .any(|data: &NativeValue| data["orders"][0].get("id").is_some())
        })
        .await;
        {
            let stores = stores.borrow();
            let stored = stores
                .iter()
                .find(|data| data["orders"][0].get("id").is_some())
                .unwrap();
            assert!(stored["orders"][0]["price"].as_f64().unwrap().is_nan());
            assert_eq!(stored["orders"][1]["price"].as_f64(), Some(f64::INFINITY));
            assert!(matches!(
                stored["orders"][3].get("type"),
                Some(NativeValue::Undefined)
            ));
        }
        unwatch();
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}

#[test]
fn worker_void_result_is_undefined() {
    account::run(async {
        let h = account::started().await;
        let result = native_account_call(
            h.account.clone(),
            "fetchAndSetPrivateSnapshot".into(),
            vec![],
        )
        .await
        .unwrap();
        assert!(matches!(result, NativeValue::Undefined), "{result:?}");
        h.ok("dispose", json!([])).await;
    });
}
