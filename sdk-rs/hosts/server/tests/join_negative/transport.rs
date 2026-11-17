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
        Bybit::http(method, uri, headers, body, seen)
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
