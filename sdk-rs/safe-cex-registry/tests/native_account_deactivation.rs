//! An actual Orderly-family auth rejection through the production account host.
#![cfg(not(target_arch = "wasm32"))]
mod support;
use safe_cex_registry::native_account::{
    NativeAccount, NativeAccountSpec, native_account_call, native_account_signer_available,
    native_account_watch, open_native_account,
};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::net::TcpListener;

async fn account() -> Rc<NativeAccount> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::task::spawn_local(support::serve_http(
        listener,
        Rc::new(|request| {
            assert_eq!(request.method, "GET");
            assert!(request.path.ends_with("/v1/client/info"), "{request:?}");
            Some(
                json!({"__status":400,"body":{"success":false,"code":-1002,"msg":"API key expired"}}),
            )
        }),
        Rc::default(),
    ));
    open_native_account(NativeAccountSpec {
        hyperliquid_limiter:safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange:"woofi".into(),
        options:json!({"key":"test-orderly-key","secret":"ed25519:11111111111111111111111111111111","applicationId":"test-account","overrideProxyUrl":origin}).into(),
        account_name:Some("test-account".into()),
        sign:Some(Rc::new(|request|Box::pin(async move {Ok(safe_cex_registry::native_value::NativeValue::object([("id",request["id"].clone()),("config",request["config"].clone())]))}))),
        proxy_token:None,
    }).unwrap()
}
async fn call(account: &Rc<NativeAccount>, method: &str) -> Value {
    support::boundary_json(
        native_account_call(account.clone(), method.into(), vec![])
            .await
            .unwrap_or_else(|error| panic!("{method}: {error}")),
    )
}
#[test]
fn expired_key_posts_worker_status_without_posting_the_disposal_reset() {
    support::run_local(async {
        let account = account().await;
        let events = Rc::new(RefCell::new(vec![]));
        let stores = Rc::new(RefCell::new(vec![]));
        let captured = events.clone();
        let snapshots = stores.clone();
        let unwatch = native_account_watch(
            &account,
            Rc::new(move |event, data| {
                captured
                    .borrow_mut()
                    .push((event.to_string(), support::boundary_json(data.clone())))
            }),
            Rc::new(move |data| snapshots.borrow_mut().push(support::boundary_json(data))),
        );
        call(&account, "start").await;
        {
            let events = events.borrow();
            assert!(
                events
                    .iter()
                    .any(|(name, data)| name == "error"
                        && data.to_string().contains("API key expired")),
                "{events:?}"
            );
            assert!(
                events
                    .iter()
                    .any(|(name, data)| name == "deactivate-status" && data == &json!(true)),
                "{events:?}"
            );
        }
        assert!(
            stores.borrow().is_empty(),
            "the worker clears store delivery before disposal resets it"
        );
        unwatch();
        call(&account, "dispose").await;
    });
}
#[test]
fn expired_key_actually_disposes_the_account_and_settles_signer_waits() {
    support::run_local(async {
        let account = account().await;
        let unwatch = native_account_watch(&account, Rc::new(|_, _| {}), Rc::new(|_| {}));
        call(&account, "start").await;
        native_account_signer_available(&account, false);
        let result = tokio::time::timeout(
            Duration::from_millis(200),
            call(&account, "waitForRemoteSigner"),
        )
        .await;
        assert_eq!(
            result.expect("disposed account must not keep waiting for a signer"),
            json!(false)
        );
        unwatch();
        call(&account, "dispose").await;
    });
}
