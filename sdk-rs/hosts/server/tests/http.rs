//! Loopback tests of the routes, auth and unauthenticated policy; no venue I/O.
use futures_util::{SinkExt, StreamExt};
use safe_cex_exchange_server::accounts::Registry;
use safe_cex_exchange_server::auth::{PROXY_PUBLIC_KEY, Verifier};
use safe_cex_exchange_server::config::Config;
use safe_cex_exchange_server::http::{App, router};
use safe_cex_exchange_server::state::Metrics;
use safe_cex_exchange_server::threads::EngineThreads;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

async fn server() -> String {
    let config = Config {
        port: 0,
        region: Some("tyo".into()),
        release: Some("test".into()),
        engine_threads: 1,
        max_connections: 16,
        max_public_per_ip: 2,
        idle_timeout: Duration::from_secs(30),
        resume_grace: Duration::from_secs(60),
        account_flush: Duration::from_millis(60),
    };
    let app = App {
        registry: Arc::new(Registry::new(
            EngineThreads::start(1).unwrap(),
            config.account_flush,
            config.resume_grace,
        )),
        metrics: Arc::new(Metrics::default()),
        verifier: Arc::new(Verifier::new(PROXY_PUBLIC_KEY).unwrap()),
        config,
        started: std::time::Instant::now(),
        public_per_ip: Default::default(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router(Arc::new(app)).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    address.to_string()
}

async fn get(address: &str, path: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body)
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
