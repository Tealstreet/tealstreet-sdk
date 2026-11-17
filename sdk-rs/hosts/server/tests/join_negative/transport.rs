use super::{bybit_fixture::Bybit, join_negative::*};
use axum::http::{HeaderMap, Method, Uri};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::signing::Algorithm;
use safe_cex_exchange_server::envelope::InitPayload;
use serde_json::{Value, json};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

struct WelcomeAndRaw;
impl Venue for WelcomeAndRaw {
    const NAME: &'static str = Bybit::NAME;
    fn algorithm() -> Algorithm {
        Bybit::algorithm()
    }
    fn init() -> InitPayload {
        Bybit::init()
    }
    fn credentials(other: bool) -> Value {
        Bybit::credentials(other)
    }
    fn signature(frame: &Value, other: bool) -> (String, Vec<u8>, Value) {
        Bybit::signature(frame, other)
    }
    fn http(
        method: &Method,
        uri: &Uri,
        headers: &HeaderMap,
        body: &Value,
        seen: &mut Observed,
    ) -> Value {
        if uri.path() == "/refused" {
            return json!({"code":-2015,"msg":"synthetic key refusal"});
        }
        Bybit::http(method, uri, headers, body, seen)
    }
    fn http_status(reply: &Value) -> axum::http::StatusCode {
        if reply["code"] == -2015 {
            axum::http::StatusCode::UNAUTHORIZED
        } else {
            axum::http::StatusCode::OK
        }
    }
    fn websocket(request: &Value, seen: &mut Observed) -> Value {
        Bybit::websocket(request, seen)
    }
    fn initial_websocket_frames() -> Vec<String> {
        vec![json!({"type":"welcome"}).to_string()]
    }
    fn websocket_text(text: &str, seen: &mut Observed) -> Option<String> {
        match text {
            "Ping" => Some("Pong".into()),
            "Pong" => None,
            _ => Some(Self::websocket(&serde_json::from_str(text).unwrap(), seen).to_string()),
        }
    }
}

#[tokio::test]
async fn websocket_hook_sends_welcome_before_any_client_frame() {
    assert_eq!(
        WelcomeAndRaw::initial_websocket_frames(),
        vec![json!({"type":"welcome"}).to_string()]
    );
    let f = Fixture::new::<WelcomeAndRaw>().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(&f.websocket_url)
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .expect("server-first fixture welcome is missing")
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(value, json!({"type":"welcome"}));
    socket.close(None).await.unwrap();
    assert_eq!(f.seen.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn websocket_hook_accepts_raw_ping_and_retains_json_messages() {
    assert_eq!(
        WelcomeAndRaw::websocket_text("Ping", &mut Observed::default()),
        Some("Pong".into())
    );
    let f = Fixture::new::<WelcomeAndRaw>().await;
    let (mut socket, _) = tokio_tungstenite::connect_async(&f.websocket_url)
        .await
        .unwrap();
    socket.send(Message::text("Ping")).await.unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            let frame = socket
                .next()
                .await
                .expect("raw Ping closed the fixture")
                .unwrap();
            if frame.to_text().unwrap() == "Pong" {
                break;
            }
        }
    })
    .await
    .expect("raw Ping requires raw Pong");
    socket.send(Message::text("Pong")).await.unwrap();
    socket
        .send(Message::text(json!({"op":"ping"}).to_string()))
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(1), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
    assert_eq!(value["op"], "ping");
    assert_eq!(value["success"], true);
    socket.close(None).await.unwrap();
    assert_eq!(f.seen.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn http_hook_preserves_success_and_returns_actual_refusal_status() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    assert_eq!(
        Bybit::http_status(&json!({"code":-2015})),
        axum::http::StatusCode::OK
    );
    let f = Fixture::new::<WelcomeAndRaw>().await;
    let base = f
        .websocket_url
        .strip_suffix("/ws")
        .unwrap()
        .replacen("ws:", "http:", 1);
    assert!(base.starts_with("http://127.0.0.1:"));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let success = client
        .get(format!("{base}/v5/market/tickers"))
        .send()
        .await
        .unwrap();
    assert_eq!(success.status(), axum::http::StatusCode::OK);
    assert_eq!(success.json::<Value>().await.unwrap()["retCode"], 0);
    let refusal = client.get(format!("{base}/refused")).send().await.unwrap();
    assert_eq!(refusal.status(), axum::http::StatusCode::UNAUTHORIZED);
    assert_eq!(
        refusal.json::<Value>().await.unwrap(),
        json!({"code":-2015,"msg":"synthetic key refusal"})
    );
    assert_eq!(f.seen.lock().unwrap().writes, 0);
}

#[tokio::test]
async fn bybit_private_reads_require_an_api_key_and_public_discovery_stays_unsigned() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let f = Fixture::new::<Bybit>().await;
    let base = f
        .websocket_url
        .strip_suffix("/ws")
        .unwrap()
        .replacen("ws:", "http:", 1);
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    for path in [
        "/v5/account/info",
        "/v5/account/wallet-balance",
        "/v5/position/list",
        "/v5/order/realtime",
    ] {
        let response = client.get(format!("{base}{path}")).send().await.unwrap();
        assert_eq!(response.status(), axum::http::StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(
            body["retCode"], 10003,
            "private read without an API-key header succeeded: {path}"
        );
        assert_eq!(body["result"], json!({}));
    }
    let credentials = Bybit::credentials(false);
    let key = credentials["key"].as_str().unwrap();
    let secret = credentials["secret"].as_str().unwrap();
    for missing in [
        "x-bapi-api-key",
        "x-bapi-timestamp",
        "x-bapi-recv-window",
        "x-bapi-sign",
    ] {
        let stamp = if missing == "x-bapi-timestamp" {
            ""
        } else {
            "10000"
        };
        let window = if missing == "x-bapi-recv-window" {
            ""
        } else {
            "5000"
        };
        let signature = safe_cex_crypto::hmac_sha256(
            secret.as_bytes(),
            format!("{stamp}{key}{window}").as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        let mut request = client.get(format!("{base}/v5/account/info"));
        for (header, value) in [
            ("x-bapi-api-key", key),
            ("x-bapi-timestamp", stamp),
            ("x-bapi-recv-window", window),
            ("x-bapi-sign", signature.as_str()),
        ] {
            if header != missing {
                request = request.header(header, value);
            }
        }
        let body: Value = request.send().await.unwrap().json().await.unwrap();
        assert_eq!(
            body["retCode"], 10003,
            "private read missing {missing} succeeded"
        );
    }
    for path in ["/v5/market/instruments-info", "/v5/market/tickers"] {
        let body: Value = client
            .get(format!("{base}{path}"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(
            body["retCode"], 0,
            "unsigned public discovery refused: {path}"
        );
    }
    let seen = f.seen.lock().unwrap();
    assert_eq!(seen.rejections, 8);
    assert_eq!(seen.writes, 0);
}
