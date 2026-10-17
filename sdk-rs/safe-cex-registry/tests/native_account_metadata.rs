#![cfg(not(target_arch = "wasm32"))]
mod support;

use safe_cex_registry::{
    native_account::{NativeAccount, NativeAccountSpec, native_account_call, open_native_account},
    native_value::NativeValue,
};
use serde_json::json;
use std::{cell::RefCell, rc::Rc, time::Duration};
use support::{Seen, eventually, query_value, run_local, serve_http};
use tokio::net::TcpListener;

async fn account() -> (Rc<NativeAccount>, Rc<RefCell<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(|request| {
            assert_eq!(request.method, "GET");
            assert!(
                request.path.ends_with("/fapi/v2/leverageBracket"),
                "{request:?}"
            );
            Some(json!({"code":"0","data":[]}))
        }),
        seen.clone(),
    ));
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "bitrue".into(),
        options: NativeValue::encode(
            &json!({"key":"native-key","secret":"native-secret","overrideProxyUrl":origin}),
        )
        .unwrap(),
        account_name: None,
        sign: None,
        proxy_token: None,
    })
    .unwrap();
    (account, seen)
}
async fn call(account: &Rc<NativeAccount>, method: &str, args: Vec<NativeValue>) {
    let result = native_account_call(account.clone(), method.into(), args).await;
    assert!(
        matches!(result, Ok(NativeValue::Null)),
        "{method}: {result:?}"
    );
}
async fn metadata(account: &Rc<NativeAccount>, symbol: &str) {
    call(
        account,
        "updateSymbolMetadata",
        vec![NativeValue::string(symbol)],
    )
    .await;
}
fn count(seen: &Rc<RefCell<Vec<Seen>>>, symbol: &str) -> usize {
    seen.borrow()
        .iter()
        .filter(|request| query_value(&request.query, "contractName") == Some(symbol))
        .count()
}

#[test]
fn native_metadata_coalesces_per_symbol_on_the_real_http_path() {
    run_local(async {
        let (account, seen) = account().await;
        metadata(&account, "BTCUSDT").await;
        metadata(&account, "BTCUSDT").await;
        metadata(&account, "BTCUSDT").await;
        metadata(&account, "ETHUSDT").await;
        eventually("metadata leading and trailing HTTP", || {
            count(&seen, "BTCUSDT") >= 2 && count(&seen, "ETHUSDT") >= 1
        })
        .await;
        call(&account, "dispose", vec![]).await;
        assert_eq!(count(&seen, "BTCUSDT"), 2);
        assert_eq!(count(&seen, "ETHUSDT"), 1);
    });
}

#[test]
fn native_disposal_cancels_trailing_metadata_http() {
    run_local(async {
        let (account, seen) = account().await;
        metadata(&account, "BTCUSDT").await;
        metadata(&account, "BTCUSDT").await;
        eventually("first metadata HTTP", || !seen.borrow().is_empty()).await;
        call(&account, "dispose", vec![]).await;
        tokio::time::sleep(Duration::from_millis(650)).await;
        assert_eq!(seen.borrow().len(), 1);
    });
}
