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

async fn rejected_snapshot_keeps_its_started_http_sibling_alive(
    exchange: &'static str,
    method: &'static str,
    expected_paths: &'static [&'static str],
    held_path: &'static str,
) {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::{Barrier, Notify};
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let paths = Rc::new(RefCell::new(Vec::new()));
    let received = Rc::new(Barrier::new(expected_paths.len()));
    let release_algo = Rc::new(Notify::new());
    let (requests, barrier, release) = (paths.clone(), received.clone(), release_algo.clone());
    tokio::task::spawn_local(async move {
        for _ in 0..expected_paths.len() {
            let (mut stream, _) = listener.accept().await.unwrap();
            let (requests, barrier, release) = (requests.clone(), barrier.clone(), release.clone());
            tokio::task::spawn_local(async move {
                let mut headers = Vec::new();
                while !headers.ends_with(b"\r\n\r\n") {
                    headers.push(stream.read_u8().await.unwrap());
                }
                let headers = String::from_utf8(headers).unwrap();
                let path = headers
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .split('?')
                    .next()
                    .unwrap()
                    .to_owned();
                let path = path
                    .split_once("://")
                    .and_then(|(_, rest)| rest.split_once('/'))
                    .map_or(path.clone(), |(_, suffix)| format!("/{suffix}"));
                let held = path == held_path;
                requests.borrow_mut().push(path);
                barrier.wait().await;
                if held {
                    release.notified().await;
                }
                let body = r#"{"message":"Invalid JWT"}"#;
                let response = format!(
                    "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            });
        }
    });
    let account = open_native_account(NativeAccountSpec {
            hyperliquid_limiter: safe_cex_registry::native_host::hyperliquid_rate_limiter(),
            exchange: exchange.into(),
            options: json!({"key":"test-key","secret":"test-secret","passphrase":"test-passphrase","tealstreetProxy":true,
                "overrideProxyUrl":origin})
            .into(),
            account_name: None,
            sign: None,
            proxy_token: Some(Rc::new(|| Box::pin(async { Ok(TOKEN.to_owned()) }))),
        })
        .unwrap();
    let errors = Rc::new(RefCell::new(Vec::new()));
    let changed = Rc::new(Notify::new());
    let expected_prefix = format!("[{exchange}] Tealstreet proxy auth failed (403):");
    let (observed, signal) = (errors.clone(), changed.clone());
    let unwatch = native_account_watch(
        &account,
        Rc::new(move |name, data| {
            let value = support::boundary_json(data.clone());
            if name == "error"
                && value[0]
                    .as_str()
                    .is_some_and(|message| message.starts_with(&expected_prefix))
            {
                observed.borrow_mut().push(value);
                signal.notify_one();
            }
        }),
        Rc::new(|_| {}),
    );
    // binance.exchange.ts starts each snapshot pair eagerly with Promise.all.
    tokio::time::timeout(
        Duration::from_secs(3),
        native_account_call(account.clone(), method.into(), vec![Value::Null.into()]),
    )
    .await
    .expect("first rejection settles without waiting for the held sibling")
    .unwrap();
    assert_eq!(
        errors.borrow().len(),
        1,
        "only the first HTTP response was released"
    );
    let mut actual_paths = paths.borrow().clone();
    actual_paths.sort();
    assert_eq!(actual_paths, expected_paths);
    release_algo.notify_one();
    let completed = tokio::time::timeout(Duration::from_secs(2), async {
        while errors.borrow().len() < expected_paths.len() {
            changed.notified().await;
        }
    })
    .await;
    unwatch();
    native_account_call(account, "dispose".into(), vec![])
        .await
        .unwrap();
    assert!(
        completed.is_ok(),
        "the already-started sibling must run its response interceptors after Promise.all rejects; observed {:?}",
        errors.borrow()
    );
    assert_eq!(errors.borrow().len(), expected_paths.len());
}

#[test]
fn rejected_order_snapshot_keeps_its_started_http_sibling_alive() {
    run_local(rejected_snapshot_keeps_its_started_http_sibling_alive(
        "binance",
        "fetchAndSetOrders",
        &["/fapi/v1/openAlgoOrders", "/fapi/v1/openOrders"],
        "/fapi/v1/openAlgoOrders",
    ));
}

#[test]
fn rejected_position_snapshot_keeps_its_started_http_sibling_alive() {
    run_local(rejected_snapshot_keeps_its_started_http_sibling_alive(
        "binance",
        "fetchPositions",
        &["/fapi/v1/symbolConfig", "/fapi/v3/positionRisk"],
        "/fapi/v1/symbolConfig",
    ));
}

#[test]
fn rejected_blofin_orders_keep_started_http_siblings_alive() {
    run_local(rejected_snapshot_keeps_its_started_http_sibling_alive(
        "blofin",
        "fetchOrders",
        &[
            "/api/v1/trade/orders-algo-pending",
            "/api/v1/trade/orders-pending",
            "/api/v1/trade/orders-tpsl-pending",
        ],
        "/api/v1/trade/orders-tpsl-pending",
    ));
}

#[test]
fn rejected_backpack_tickers_keep_started_http_sibling_alive() {
    run_local(rejected_snapshot_keeps_its_started_http_sibling_alive(
        "backpack",
        "fetchTickers",
        &["/api/v1/markPrices", "/api/v1/tickers"],
        "/api/v1/markPrices",
    ));
}
