//! safeCexAxiosCreate's proxy failure `error` event, through a production native
//! account on the Tealstreet transport and a loopback proxy origin.
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

/// An unexpired proxy JWT (exp 2100-01-01); the transport checks expiry, not the signature.
const TOKEN: &str =
    "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhY2N0LTEiLCJleHAiOjQxMDI0NDQ4MDB9.c2ln";

async fn proxied_account(reply: Value) -> (Rc<NativeAccount>, Rc<RefCell<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Rc::<RefCell<Vec<Seen>>>::default();
    tokio::task::spawn_local(serve_http(
        listener,
        Rc::new(move |_: &Seen| Some(reply.clone())),
        seen.clone(),
    ));
    let account = open_native_account(NativeAccountSpec {
        hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        exchange: "binance".into(),
        options: json!({"key":"test-key","secret":"test-secret","tealstreetProxy":true,
            "overrideProxyUrl":origin})
        .into(),
        account_name: None,
        sign: None,
        proxy_token: Some(Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) }))),
    })
    .unwrap();
    (account, seen)
}

async fn error_events(reply: Value) -> Vec<Value> {
    let (account, seen) = proxied_account(reply).await;
    let events = Rc::<RefCell<Vec<Value>>>::default();
    let recorded = events.clone();
    let unwatch = native_account_watch(
        &account,
        Rc::new(move |name, data| {
            if name == "error" {
                recorded
                    .borrow_mut()
                    .push(support::boundary_json(data.clone()));
            }
        }),
        Rc::new(|_| {}),
    );
    // fetchAndSetOrders catches the failure (as TS does); the event is the signal.
    native_account_call(
        account.clone(),
        "fetchAndSetOrders".into(),
        vec![Value::Null.into()],
    )
    .await
    .unwrap();
    assert!(
        seen.borrow()
            .iter()
            .any(|request| request.headers.contains_key("authorization")),
        "the request went through the authorized proxy route: {:?}",
        seen.borrow()
    );
    unwatch();
    native_account_call(account, "dispose".into(), vec![])
        .await
        .unwrap();
    events.take()
}

#[test]
fn tealstreet_proxy_auth_failure_emits_error() {
    run_local(async {
        let events = error_events(json!({"__status":401,"body":{"message":"Invalid JWT"}})).await;
        assert!(
            events
                .iter()
                .any(|data| data[0].as_str().is_some_and(|message| message
                    .starts_with("[binance] Tealstreet proxy auth failed (401): ")
                    && message.contains("Invalid JWT"))),
            "{events:?}"
        );
    });
}

#[test]
fn upstream_auth_failure_via_proxy_emits_error() {
    run_local(async {
        let events =
            error_events(json!({"__status":401,"body":{"code":-2015,"msg":"Invalid API-key"}}))
                .await;
        assert!(
            events
                .iter()
                .any(|data| data[0].as_str().is_some_and(|message| message
                    .starts_with("[binance] Upstream exchange auth failed via proxy (401): "))),
            "{events:?}"
        );
    });
}

#[test]
fn proxy_server_error_emits_error() {
    run_local(async {
        let events = error_events(json!({"__status":502,"body":"bad gateway"})).await;
        assert!(
            events.iter().any(|data| {
                data[0].as_str().is_some_and(|message| {
                    message.starts_with("[binance] Proxy server error (502): ")
                })
            }),
            "{events:?}"
        );
    });
}

#[test]
fn proxy_failures_retain_diagnostic_shape_without_credentials() {
    run_local(async {
        use safe_cex_store::venue_rest_diagnostics::{
            drain_venue_rest_failure_samples, reset_venue_rest_diagnostics,
        };
        reset_venue_rest_diagnostics();
        let (account, seen) =
            proxied_account(json!({"__status":403,"body":{"message":"Invalid JWT"}})).await;
        native_account_call(account.clone(), "validateAccount".into(), vec![])
            .await
            .unwrap();
        let failures = drain_venue_rest_failure_samples();
        assert_eq!(
            seen.borrow().len(),
            1,
            "rejected account validation sends one HTTP request"
        );
        assert_eq!(
            failures.len(),
            1,
            "the actual proxy failure reaches diagnostics"
        );
        for failure in failures {
            assert_eq!(failure.status, Some(403.0));
            assert_eq!(failure.rest_proxy_header_sent.as_deref(), Some("bearer"));
            assert!(failure.total_ms.is_some_and(|ms| ms >= 0.0));
            assert!(
                failure
                    .error_message
                    .as_deref()
                    .is_none_or(|text| !text.contains(TOKEN))
            );
            assert!(!format!("{failure:?}").contains(TOKEN));
        }
        native_account_call(account, "dispose".into(), vec![])
            .await
            .unwrap();
    });
}
