//! Loopback tests of the routes, auth and unauthenticated policy; no venue I/O.
mod support;

use futures_util::{SinkExt, StreamExt};
use safe_cex_exchange_server::auth::{PROXY_PUBLIC_KEY, Verifier};
use safe_cex_exchange_server::http::{App, drain};
use std::sync::Arc;
use std::time::Duration;
use support::{get, next_close, post};
use tokio_tungstenite::tungstenite::Message;

async fn server() -> String {
    server_app().await.0
}

async fn server_app() -> (String, Arc<App>) {
    let app = support::app(
        support::config(Duration::from_secs(60)),
        Verifier::new(PROXY_PUBLIC_KEY).unwrap(),
    );
    (support::serve(app.clone()).await, app)
}

#[tokio::test]
async fn a_drain_closes_sockets_1001() {
    let (address, app) = server_app().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}/account"))
        .await
        .unwrap();
    drain(&app).await;
    let closed = loop {
        match tokio::time::timeout(Duration::from_secs(2), socket.next()).await {
            Ok(Some(Ok(Message::Close(frame)))) => break frame,
            Ok(Some(Ok(_))) => {}
            other => panic!("no close frame: {other:?}"),
        }
    };
    assert_eq!(u16::from(closed.unwrap().code), 1001);
}

const PUBLIC_INIT: &str =
    r#"{"t":"init","d":{"accountName":"__pub_bybit","exchangeName":"bybit","publicOnly":true}}"#;

#[tokio::test]
async fn a_drain_moves_an_idle_socket_and_refuses_new_users_with_4100() {
    let (address, _app) = server_app().await;
    let url = format!("ws://{address}/account");
    let (mut idle, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (status, body) = post(&address, "/admin/drain", "").await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(
        next_close(&mut idle).await,
        (4100, "draining: reconnect".to_owned())
    );
    let (mut newcomer, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    newcomer.send(Message::text(PUBLIC_INIT)).await.unwrap();
    assert_eq!(
        next_close(&mut newcomer).await,
        (4100, "draining: reconnect".to_owned())
    );
    let (status, body) = get(&address, "/health").await;
    assert_eq!(status, 503, "{body}");
    assert!(
        body.contains(r#""draining":true"#) && body.contains(r#""algos":{"chases":0,"twaps":0}"#),
        "{body}"
    );
    let (status, _) = post(&address, "/admin/undrain", "").await;
    assert_eq!(status, 200);
    let (status, body) = get(&address, "/health").await;
    assert_eq!(status, 200);
    assert!(body.contains(r#""draining":false"#), "{body}");
}

#[tokio::test]
async fn break_glass_moves_every_socket_with_4100() {
    let (address, _app) = server_app().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}/account"))
        .await
        .unwrap();
    let (status, body) = post(&address, "/admin/drain?now=1", "").await;
    assert_eq!(status, 200, "{body}");
    assert!(body.contains(r#""settled":true"#), "{body}");
    assert_eq!(
        next_close(&mut socket).await,
        (4100, "draining: reconnect".to_owned())
    );
}

#[tokio::test]
async fn admin_routes_refuse_a_request_the_edge_forwarded() {
    let (address, _app) = server_app().await;
    for path in ["/admin/drain", "/admin/drain?now=1", "/admin/undrain"] {
        let (status, _) = post(&address, path, "cf-connecting-ip: 1.2.3.4\r\n").await;
        assert_eq!(status, 403, "{path}");
    }
    let (status, body) = get(&address, "/health").await;
    assert_eq!(status, 200);
    assert!(body.contains(r#""draining":false"#), "{body}");
}

#[tokio::test]
async fn health_and_capabilities_answer_without_venue_io() {
    let address = server().await;
    let (status, body) = get(&address, "/health").await;
    assert_eq!(status, 200);
    assert!(
        body.contains(r#""ok":true"#) && body.contains(r#""region":"tyo""#),
        "{body}"
    );
    for path in ["/capabilities", "/account/capabilities"] {
        let (status, body) = get(&address, path).await;
        assert_eq!(status, 200);
        assert!(body.contains(r#""runtime":"rust""#), "{body}");
    }
    assert_eq!(
        get(&address, "/account").await.0,
        426,
        "plain GET is not an upgrade"
    );
}

#[tokio::test]
async fn capability_discovery_is_scoped_to_the_requested_account_venue() {
    let address = server().await;
    for route in ["/capabilities", "/account/capabilities"] {
        for (venue, supported) in [("binance", true), ("hyrotrader", false), ("unknown", false)] {
            let (status, body) = get(&address, &format!("{route}?exchangeName={venue}")).await;
            assert_eq!(status, 200);
            let body: serde_json::Value = serde_json::from_str(&body).unwrap();
            let transport = &body["transports"]["accountWorkerWebSocket"];
            assert_eq!(transport["delegatedVenueV1"], venue);
            assert_eq!(
                transport["delegatedActivationScopeV1"]["place"],
                supported && cfg!(feature = "delegated-order-frames")
            );
        }
        let (_, body) = get(&address, route).await;
        let body: serde_json::Value = serde_json::from_str(&body).unwrap();
        let transport = &body["transports"]["accountWorkerWebSocket"];
        assert!(transport["delegatedVenueV1"].is_null());
        assert_eq!(transport["delegatedActivationScopeV1"]["place"], false);
    }
}

#[tokio::test]
async fn invalid_tokens_are_refused_before_the_upgrade() {
    let address = server().await;
    let error =
        tokio_tungstenite::connect_async(format!("ws://{address}/account?proxyToken=not-a-jwt"))
            .await
            .unwrap_err();
    let tokio_tungstenite::tungstenite::Error::Http(response) = error else {
        panic!("{error}")
    };
    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn token_expiry_refuses_commands_but_keeps_the_thirty_second_close_grace() {
    let mut config = support::config(Duration::from_secs(60));
    config.idle_timeout = Duration::from_secs(60);
    let app = support::app(config, Verifier::unsigned());
    let address = support::serve(app).await;
    let expires = safe_cex_exchange_server::accounts::now_ms() / 1000 + 5;
    let token = safe_cex_exchange_server::auth::unsigned_token("synthetic-user", expires);
    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{address}/account?proxyToken={token}"))
            .await
            .unwrap();
    socket.send(Message::text(r#"{"t":"ping"}"#)).await.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Text(text))) if text.contains("pong") => break,
                Some(Ok(Message::Ping(_))) => {}
                other => panic!("valid command before expiry: {other:?}"),
            }
        }
    })
    .await
    .expect("initial pong");
    let expiry_ms = expires * 1000;
    tokio::time::sleep(Duration::from_millis(
        expiry_ms.saturating_sub(safe_cex_exchange_server::accounts::now_ms()) + 20,
    ))
    .await;
    socket.send(Message::text(r#"{"t":"ping"}"#)).await.unwrap();
    loop {
        match tokio::time::timeout(Duration::from_secs(2), socket.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => {
                assert!(text.contains("Tealstreet proxy token expired"), "{text}");
                break;
            }
            Ok(Some(Ok(Message::Ping(_)))) => {}
            other => panic!("command refusal during grace: {other:?}"),
        }
    }
    let close = loop {
        match tokio::time::timeout(Duration::from_secs(32), socket.next()).await {
            Ok(Some(Ok(Message::Close(Some(frame))))) => break frame,
            Ok(Some(Ok(_))) => {}
            other => panic!("expiry close: {other:?}"),
        }
    };
    let closed_at = safe_cex_exchange_server::accounts::now_ms();
    assert!(
        closed_at >= expiry_ms + 30_000,
        "close did not consume grace"
    );
    assert!(closed_at < expiry_ms + 33_000, "grace was not extended");
    assert_eq!(u16::from(close.code), 1008);
    assert_eq!(close.reason, "Tealstreet proxy token expired");
}

#[tokio::test]
async fn unauthenticated_sockets_may_only_init_the_public_account() {
    let address = server().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(format!("ws://{address}/account"))
        .await
        .unwrap();
    let init = r#"{"t":"init","d":{"accountName":"mine","exchangeName":"bybit","key":"k"}}"#;
    socket.send(Message::text(init)).await.unwrap();
    let mut closed = None;
    while let Some(Ok(message)) = socket.next().await {
        if let Message::Close(frame) = message {
            closed = frame;
            break;
        }
    }
    assert_eq!(u16::from(closed.expect("policy close").code), 1008);
}

#[tokio::test]
async fn unauthenticated_sockets_cannot_sign_and_are_capped_per_ip() {
    let address = server().await;
    let url = format!("ws://{address}/account");
    let (mut first, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (_second, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let error = tokio_tungstenite::connect_async(&url).await.unwrap_err();
    assert!(
        matches!(error, tokio_tungstenite::tungstenite::Error::Http(response) if response.status() == 429)
    );
    first
        .send(Message::text(r#"{"t":"sign-res","d":{"id":"x"}}"#))
        .await
        .unwrap();
    let mut code = None;
    while let Some(Ok(message)) = first.next().await {
        if let Message::Close(Some(frame)) = message {
            code = Some(u16::from(frame.code));
            break;
        }
    }
    assert_eq!(code, Some(1008));
}
