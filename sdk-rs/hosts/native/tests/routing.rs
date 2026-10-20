//! Deterministic loopback checks of the native host's final network packet path.
#![cfg(not(target_arch = "wasm32"))]
// Tungstenite fixes the server callback error type to an owned HTTP response.
#![allow(clippy::result_large_err)]

use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, Method, RequestId, SocketId};
use safe_cex_native::{
    HostEvent, HttpRoute, HttpRouteMode, NetworkTransport, ProxyTokenProvider, RoutingPlan,
    SocketChannel, SocketChannelRule, Transport,
};
use serde::Deserialize;
use serde_json::value::RawValue;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::tungstenite::{
    Message,
    handshake::server::{Request, Response},
};

fn tokens() -> ProxyTokenProvider {
    Arc::new(|| Box::pin(async { Ok("account-token".into()) }))
}

fn http_request(id: u32, url: String) -> HttpRequest {
    HttpRequest {
        id: RequestId(id),
        method: Method::Post,
        route: None,
        timeout_ms: None,
        url,
        headers: vec![("Content-Type", "application/json".into())],
        body: Some(r#"{"id":18446744073709551615}"#.into()),
    }
}

async fn next(events: &mut mpsc::UnboundedReceiver<HostEvent>) -> HostEvent {
    tokio::time::timeout(Duration::from_secs(3), events.recv())
        .await
        .expect("host event deadline")
        .expect("host event channel")
}

async fn http_body(events: &mut mpsc::UnboundedReceiver<HostEvent>) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        match next(events).await {
            HostEvent::HttpStart { status, .. } => assert_eq!(status, 200),
            HostEvent::HttpChunk(_, chunk) => body.extend(chunk),
            HostEvent::HttpEnd(_) => return body,
            event => panic!("unexpected {event:?}"),
        }
    }
}

async fn listener() -> (TcpListener, String) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    (listener, format!("http://{address}"))
}

async fn request_text(stream: &mut TcpStream) -> String {
    let mut bytes = Vec::new();
    loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer).await.unwrap();
        assert!(count > 0, "request ended before headers");
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            let head = std::str::from_utf8(&bytes[..index]).unwrap();
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|length| length.parse::<usize>().ok())
                })
                .unwrap_or(0);
            if bytes.len() >= index + 4 + length {
                break;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn proxy_public_selection_headers_and_fresh_tokens_reach_the_wire() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let mut captured = Vec::new();
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await.unwrap();
            captured.push(request_text(&mut stream).await);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}")
                .await
                .unwrap();
        }
        captured
    });
    let counter = Arc::new(AtomicUsize::new(0));
    let count = counter.clone();
    let provider: ProxyTokenProvider = Arc::new(move || {
        let value = count.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move { Ok(format!("token-{value}")) })
    });
    let plan = RoutingPlan {
        http: HttpRoute {
            prefix: format!("{origin}/private"),
            authorize: true,
            authorization_header: "Tealstreet-Authorization".into(),
            headers: BTreeMap::from([("Origin".into(), "https://trade.tealstreet.io".into())]),
            ..HttpRoute::default()
        },
        public_http: Some(HttpRoute {
            prefix: format!("{origin}/public"),
            ..HttpRoute::default()
        }),
        public_header: Some("X-API-KEY".into()),
        ..RoutingPlan::default()
    };
    let transport = NetworkTransport::with_routing(plan, provider);
    let (send, mut events) = mpsc::unbounded_channel();
    for id in 0..3 {
        let mut request = http_request(
            id,
            "https://venue.invalid/v1/order?signature=original".into(),
        );
        request
            .headers
            .push(("Authorization", "venue-session".into()));
        if id > 0 {
            request.headers.push(("x-api-key", "venue-key".into()));
        }
        transport.request(request, send.clone());
        assert_eq!(http_body(&mut events).await, b"{}");
    }
    let captured = server.await.unwrap();
    assert!(captured[0].starts_with(
        "POST /public/https://venue.invalid/v1/order?signature=original HTTP/1.1\r\n"
    ));
    for (index, request) in captured.iter().enumerate() {
        assert!(request.contains("authorization: venue-session\r\n"));
        assert!(request.ends_with(r#"{"id":18446744073709551615}"#));
        if index > 0 {
            assert!(request.starts_with(
                "POST /private/https://venue.invalid/v1/order?signature=original HTTP/1.1\r\n"
            ));
            assert!(request.contains(&format!(
                "tealstreet-authorization: Bearer token-{index}\r\n"
            )));
            assert!(request.contains("origin: https://trade.tealstreet.io\r\n"));
        } else {
            assert!(!request.contains("tealstreet-authorization:"));
        }
    }
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn direct_without_a_plan_preserves_engine_websocket_headers() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/venue/private");
                assert_eq!(request.headers()["x-api-key"], "native-key");
                assert!(!request.headers().contains_key("origin"));
                Ok(response)
            })
            .await
            .unwrap();
        ws.send(Message::text("venue-message")).await.unwrap();
        let _ = ws.next().await;
    });
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = NetworkTransport::new().connect_with_headers(
        SocketId(1),
        origin.replacen("http:", "ws:", 1) + "/venue/private",
        vec![("X-API-KEY", "native-key".into())],
        send,
    );
    assert!(matches!(
        next(&mut events).await,
        HostEvent::WsOpened(SocketId(1))
    ));
    assert!(
        matches!(next(&mut events).await, HostEvent::WsMessage(_, text) if text == "venue-message")
    );
    drop(outgoing);
    server.await.unwrap();
}

#[tokio::test]
async fn explicit_socket_channel_wins_and_merges_engine_and_upgrade_headers() {
    let (listener, origin) = listener().await;
    let routed = origin.replacen("http:", "ws:", 1) + "/proxy/private";
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/proxy/private");
                assert_eq!(request.headers()["x-auth"], "venue-auth");
                assert_eq!(request.headers()["origin"], "https://preview.tealstreet.io");
                assert_eq!(request.headers()["user-agent"], "routing-test-agent");
                Ok(response)
            })
            .await
            .unwrap();
        ws.send(Message::text("routed")).await.unwrap();
        let _ = ws.next().await;
    });
    let mut plan = RoutingPlan::default();
    plan.ws.private.host = Some(routed);
    plan.ws_headers = BTreeMap::from([
        ("Origin".into(), "https://preview.tealstreet.io".into()),
        ("User-Agent".into(), "routing-test-agent".into()),
    ]);
    plan.socket_channels.push(SocketChannelRule {
        contains: "shared".into(),
        channel: SocketChannel::Public,
    });
    let transport = NetworkTransport::with_routing(plan, tokens());
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = transport.connect_channel(
        SocketId(2),
        "wss://venue.invalid/shared".into(),
        vec![("X-Auth", "venue-auth".into())],
        Some(safe_cex_engine::SocketChannel::Private),
        send,
    );
    assert!(matches!(
        next(&mut events).await,
        HostEvent::WsOpened(SocketId(2))
    ));
    assert!(matches!(next(&mut events).await, HostEvent::WsMessage(_, text) if text == "routed"));
    drop(outgoing);
    server.await.unwrap();
}

#[tokio::test]
async fn two_silent_connects_choose_passthrough_and_ignore_cosmetic_query_changes() {
    let (listener, origin) = listener().await;
    let origin = origin.replacen("http:", "ws:", 1);
    let server = tokio::spawn(async move {
        for attempt in 0..3 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &Request, response: Response| {
                    assert_eq!(
                        request.uri().path(),
                        if attempt < 2 { "/direct" } else { "/fallback" }
                    );
                    assert_eq!(request.headers().contains_key("origin"), attempt == 2);
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let mut plan = RoutingPlan::default();
    plan.ws.public.fallback = Some(format!("{origin}/fallback"));
    plan.ws_headers
        .insert("Origin".into(), "https://preview.tealstreet.io".into());
    let transport = NetworkTransport::with_routing(plan, tokens());
    for attempt in 0..3 {
        let (send, mut events) = mpsc::unbounded_channel();
        let outgoing = transport.connect(
            SocketId(attempt),
            format!("{origin}/direct?requestId={attempt}"),
            send,
        );
        assert!(matches!(next(&mut events).await, HostEvent::WsOpened(_)));
        assert!(matches!(next(&mut events).await, HostEvent::WsClosed(_, _)));
        drop(outgoing);
    }
    server.await.unwrap();
}

#[derive(Deserialize)]
struct Rpc {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    upstream: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    arg: Option<Argument>,
    #[serde(default)]
    rpc_version: Option<u8>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    accept_binary: Option<bool>,
}
#[derive(Deserialize)]
struct Argument {
    url: String,
    method: String,
    headers: BTreeMap<String, String>,
    data: Box<RawValue>,
}

async fn rpc<S>(ws: &mut tokio_tungstenite::WebSocketStream<S>) -> Rpc
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        if let Some(Ok(Message::Text(text))) = ws.next().await {
            let value: Rpc = serde_json::from_str(&text).unwrap();
            if value.kind != "rpc.ping" {
                return value;
            }
        } else {
            panic!("expected RPC text");
        }
    }
}

#[tokio::test]
async fn relay_hides_auth_and_delays_open_until_the_upstream_is_ready() {
    let (listener, origin) = listener().await;
    let tunnel = origin.replacen("http:", "ws:", 1);
    let (ready, proceed) = oneshot::channel();
    let (seen, seen_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                assert_eq!(
                    request.uri().path_and_query().unwrap().as_str(),
                    "/tunnel?test=1"
                );
                assert_eq!(request.headers()["origin"], "https://preview.tealstreet.io");
                Ok(response)
            })
            .await
            .unwrap();
        ws.send(Message::text(
            r#"{"type":"serverTime","serverTime":1700000000000}"#,
        ))
        .await
        .unwrap();
        let auth = rpc(&mut ws).await;
        assert_eq!(auth.kind, "rpc.auth");
        assert_eq!(auth.token.as_deref(), Some("account-token"));
        ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
            .await
            .unwrap();
        let relay = rpc(&mut ws).await;
        assert_eq!(relay.kind, "rpc.relay");
        assert_eq!(
            relay.upstream.as_deref(),
            Some("wss://venue.invalid/v5/private")
        );
        seen.send(()).unwrap();
        proceed.await.unwrap();
        ws.send(Message::text(r#"{"type":"rpc.relay","status":"open"}"#))
            .await
            .unwrap();
        ws.send(Message::text("venue-auth-response")).await.unwrap();
        assert_eq!(
            ws.next().await.unwrap().unwrap().into_text().unwrap(),
            "venue-login"
        );
    });
    let mut plan = RoutingPlan::default();
    plan.ws.private.relay = Some(format!(
        "{tunnel}/tunnel?test=1&ts-relay=wss%3A%2F%2Fvenue.invalid%2Fv5%2Fprivate"
    ));
    plan.ws_headers
        .insert("Origin".into(), "https://preview.tealstreet.io".into());
    let transport = NetworkTransport::with_routing(plan, tokens());
    let url = "ws://127.0.0.1:1/v5/private".to_owned();
    for attempt in 0..2 {
        let (send, mut events) = mpsc::unbounded_channel();
        let outgoing = transport.connect_channel(
            SocketId(attempt),
            url.clone(),
            vec![],
            Some(safe_cex_engine::SocketChannel::Private),
            send,
        );
        assert!(matches!(next(&mut events).await, HostEvent::WsClosed(_, _)));
        drop(outgoing);
    }
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = transport.connect_channel(
        SocketId(3),
        url,
        vec![],
        Some(safe_cex_engine::SocketChannel::Private),
        send,
    );
    seen_rx.await.unwrap();
    assert!(
        events.try_recv().is_err(),
        "tunnel chatter must not open the engine socket"
    );
    ready.send(()).unwrap();
    assert!(matches!(
        next(&mut events).await,
        HostEvent::WsOpened(SocketId(3))
    ));
    outgoing.send("venue-login".into()).unwrap();
    assert!(
        matches!(next(&mut events).await, HostEvent::WsMessage(_, text) if text == "venue-auth-response")
    );
    server.await.unwrap();
}

#[tokio::test]
async fn how_preserves_signed_url_headers_and_integer_body_and_streams_raw_response() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws =
            tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
                assert_eq!(request.uri().path(), "/tunnel-staging");
                let query =
                    reqwest::Url::parse(&format!("http://localhost{}", request.uri())).unwrap();
                let pairs: Vec<_> = query.query_pairs().collect();
                assert_eq!(pairs.iter().filter(|(name, _)| name == "v").count(), 1);
                assert!(
                    pairs
                        .iter()
                        .any(|(name, value)| name == "v" && value == "2")
                );
                assert!(
                    pairs
                        .iter()
                        .any(|(name, value)| name == "t" && value.parse::<u64>().unwrap() > 0)
                );
                assert_eq!(
                    pairs.iter().filter(|(name, _)| name == "preserve").count(),
                    2
                );
                assert_eq!(request.headers()["origin"], "https://preview.tealstreet.io");
                Ok(response)
            })
            .await
            .unwrap();
        ws.send(Message::text(
            r#"{"type":"serverTime","serverTime":1700000000000}"#,
        ))
        .await
        .unwrap();
        let auth = rpc(&mut ws).await;
        assert_eq!(auth.kind, "rpc.auth");
        assert_eq!(auth.token.as_deref(), Some("account-token"));
        ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
            .await
            .unwrap();
        let request = rpc(&mut ws).await;
        assert_eq!(request.kind, "rpc.http");
        assert_eq!(request.rpc_version, Some(4));
        assert_eq!(request.format.as_deref(), Some("raw"));
        assert_eq!(request.accept_binary, Some(true));
        let arg = request.arg.unwrap();
        assert_eq!(
            arg.url,
            "https://custom-proxy.invalid/https://venue.invalid/v1/order?signature=exact"
        );
        assert_eq!(arg.method, "POST");
        assert_eq!(arg.headers["X-Signature"], "unchanged");
        assert_eq!(
            serde_json::from_str::<String>(arg.data.get()).unwrap(),
            r#"{"id":18446744073709551615}"#
        );
        let nonce = request.nonce.unwrap();
        let response = format!(
            r#"{{"type":"rpc.http","nonce":"{nonce}","data":{{"status":200,"headers":{{"x-venue":"ok"}},"data_raw":"{{\"orderId\":18446744073709551615}}"}}}}"#
        );
        ws.send(Message::text(response)).await.unwrap();
        let _ = ws.next().await;
    });
    let mut plan = RoutingPlan {
        http: HttpRoute {
            mode: HttpRouteMode::How,
            prefix: "https://custom-proxy.invalid".into(),
            tunnel_url: Some(
                origin.replacen("http:", "ws:", 1)
                    + "/tunnel-staging?preserve=one&preserve=two&v=old&t=old",
            ),
            ..HttpRoute::default()
        },
        ..RoutingPlan::default()
    };
    plan.ws_headers
        .insert("Origin".into(), "https://preview.tealstreet.io".into());
    let (send, mut events) = mpsc::unbounded_channel();
    let mut request = http_request(10, "https://venue.invalid/v1/order?signature=exact".into());
    request.headers.push(("X-Signature", "unchanged".into()));
    NetworkTransport::with_routing(plan, tokens()).request(request, send);
    assert_eq!(
        http_body(&mut events).await,
        br#"{"orderId":18446744073709551615}"#
    );
    server.await.unwrap();
}

#[tokio::test]
async fn how_honors_ip_auth_without_fetching_a_token_and_never_replays_a_lost_write() {
    let (listener, origin) = listener().await;
    let writes = Arc::new(AtomicUsize::new(0));
    let count = writes.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::text(
            r#"{"type":"serverTime","serverTime":1700000000000}"#,
        ))
        .await
        .unwrap();
        ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
            .await
            .unwrap();
        assert_eq!(rpc(&mut ws).await.kind, "rpc.http");
        count.fetch_add(1, Ordering::SeqCst);
        ws.close(None).await.unwrap();
    });
    let provider: ProxyTokenProvider = Arc::new(|| {
        Box::pin(async { panic!("IP-authenticated connections must not request a token") })
    });
    let mut plan = RoutingPlan::default();
    plan.http.mode = HttpRouteMode::How;
    plan.http.tunnel_url = Some(origin.replacen("http:", "ws:", 1) + "/tunnel");
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::with_routing(plan, provider)
        .request(http_request(11, "https://venue.invalid/order".into()), send);
    assert!(matches!(
        next(&mut events).await,
        HostEvent::HttpFailed(RequestId(11), _)
    ));
    server.await.unwrap();
    assert_eq!(writes.load(Ordering::SeqCst), 1);
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn how_accepts_binary_gzip_base64_raw_and_legacy_responses() {
    use base64::Engine as _;
    use std::io::Write;
    let (listener, origin) = listener().await;
    let expected = br#"{"id":18446744073709551615}"#.to_vec();
    let body = expected.clone();
    let server = tokio::spawn(async move {
        for encoding in 0..4 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
                .await
                .unwrap();
            let request = rpc(&mut ws).await;
            let nonce = request.nonce.unwrap();
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            encoder.write_all(&body).unwrap();
            let gzip = encoder.finish().unwrap();
            let data = match encoding {
                0 => format!(
                    r#""data_encoding":"gzip+base64","data_encoded":"{}""#,
                    base64::engine::general_purpose::STANDARD.encode(&gzip)
                ),
                1 => r#""data_encoding":"gzip""#.into(),
                2 => format!(
                    r#""data_raw":{}"#,
                    serde_json::to_string(std::str::from_utf8(&body).unwrap()).unwrap()
                ),
                _ => format!(
                    r#""data":{}"#,
                    serde_json::to_string(std::str::from_utf8(&body).unwrap()).unwrap()
                ),
            };
            let header = format!(
                r#"{{"type":"rpc.http","nonce":"{nonce}","data":{{"status":200,"headers":{{}},{data}}}}}"#
            );
            if encoding == 1 {
                let mut frame = b"TSB1".to_vec();
                frame.extend_from_slice(&(header.len() as u32).to_be_bytes());
                frame.extend_from_slice(header.as_bytes());
                frame.extend_from_slice(&gzip);
                ws.send(Message::binary(frame)).await.unwrap();
            } else {
                ws.send(Message::text(header)).await.unwrap();
            }
            let _ = ws.next().await;
        }
    });
    let mut plan = RoutingPlan::default();
    plan.http.mode = HttpRouteMode::How;
    plan.http.tunnel_url = Some(origin.replacen("http:", "ws:", 1) + "/tunnel");
    let transport = NetworkTransport::with_routing(plan, tokens());
    for id in 0..4 {
        let (send, mut events) = mpsc::unbounded_channel();
        transport.request(
            http_request(
                id,
                "https://venue.invalid/read?duplicate=one&duplicate=two".into(),
            ),
            send,
        );
        assert_eq!(http_body(&mut events).await, expected);
    }
    server.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn proxy_token_acquisition_is_inside_the_request_deadline() {
    let (listener, origin) = listener().await;
    let provider: ProxyTokenProvider = Arc::new(|| {
        Box::pin(async {
            std::future::pending::<()>().await;
            unreachable!()
        })
    });
    let mut plan = RoutingPlan::default();
    plan.http.authorize = true;
    plan.http.prefix = origin;
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::with_routing(plan, provider)
        .request(http_request(20, "https://venue.invalid/order".into()), send);
    tokio::task::yield_now().await;
    tokio::time::advance(Duration::from_secs(21)).await;
    assert!(
        matches!(events.recv().await, Some(HostEvent::HttpFailed(RequestId(20), error)) if error == "transport request timed out")
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(1), listener.accept())
            .await
            .is_err(),
        "an expired operation must never reach the proxy"
    );
}

#[tokio::test]
async fn stalled_relay_authentication_is_bounded_and_emits_no_open_or_messages() {
    let (listener, origin) = listener().await;
    let (accepted, accepted_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        assert_eq!(rpc(&mut ws).await.kind, "rpc.auth");
        accepted.send(()).unwrap();
        let _ = ws.next().await;
    });
    let mut plan = RoutingPlan::default();
    // A relay can also be an explicit selected host in a pre-resolved plan.
    plan.ws.private.host = Some(
        origin.replacen("http:", "ws:", 1) + "/tunnel?ts-relay=wss%3A%2F%2Fvenue.invalid%2Fprivate",
    );
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = NetworkTransport::with_routing(plan, tokens()).connect_channel(
        SocketId(30),
        "wss://venue.invalid/private".into(),
        vec![],
        Some(safe_cex_engine::SocketChannel::Private),
        send,
    );
    accepted_rx.await.unwrap();
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(16)).await;
    assert!(matches!(
        events.recv().await,
        Some(HostEvent::WsClosed(SocketId(30), 1006))
    ));
    assert!(events.try_recv().is_err());
    drop(outgoing);
    server.await.unwrap();
}

#[tokio::test]
async fn closing_a_connecting_relay_cancels_its_pending_token_and_socket() {
    let (listener, origin) = listener().await;
    let (started, mut started_rx) = mpsc::unbounded_channel();
    let provider: ProxyTokenProvider = Arc::new(move || {
        started.send(()).unwrap();
        Box::pin(async { std::future::pending::<Result<String, String>>().await })
    });
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        let message = ws.next().await;
        assert!(
            !matches!(message, Some(Ok(Message::Text(_)))),
            "cancelled relay must never authenticate"
        );
    });
    let mut plan = RoutingPlan::default();
    plan.ws.private.host = Some(
        origin.replacen("http:", "ws:", 1) + "/tunnel?ts-relay=wss%3A%2F%2Fvenue.invalid%2Fprivate",
    );
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = NetworkTransport::with_routing(plan, provider).connect_channel(
        SocketId(40),
        "wss://venue.invalid/private".into(),
        vec![],
        Some(safe_cex_engine::SocketChannel::Private),
        send,
    );
    started_rx.recv().await.unwrap();
    drop(outgoing);
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn how_rejects_a_parsed_response_instead_of_rounding_native_ids() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
            .await
            .unwrap();
        let nonce = rpc(&mut ws).await.nonce.unwrap();
        ws.send(Message::text(format!(r#"{{"type":"rpc.http","nonce":"{nonce}","data":{{"status":200,"data":{{"id":18446744073709551615}}}}}}"#))).await.unwrap();
    });
    let mut plan = RoutingPlan::default();
    plan.http.mode = HttpRouteMode::How;
    plan.http.tunnel_url = Some(origin.replacen("http:", "ws:", 1) + "/tunnel");
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::with_routing(plan, tokens())
        .request(http_request(50, "https://venue.invalid/read".into()), send);
    assert!(
        matches!(next(&mut events).await, HostEvent::HttpFailed(RequestId(50), error) if error == "tunnel response has no raw body")
    );
    server.await.unwrap();
}

fn delayed_tokens() -> (
    ProxyTokenProvider,
    mpsc::UnboundedReceiver<()>,
    oneshot::Sender<String>,
) {
    let (started, started_rx) = mpsc::unbounded_channel();
    let (released, released_rx) = oneshot::channel();
    let receiver = std::sync::Mutex::new(Some(released_rx));
    let provider: ProxyTokenProvider = Arc::new(move || {
        let receiver = receiver.lock().unwrap().take().unwrap();
        let started = started.clone();
        Box::pin(async move {
            let _ = started.send(());
            receiver.await.map_err(|error| error.to_string())
        })
    });
    (provider, started_rx, released)
}

#[tokio::test]
async fn dropping_the_host_cancels_proxy_authorization_before_any_write() {
    for mode in [HttpRouteMode::Direct, HttpRouteMode::How] {
        let (listener, origin) = listener().await;
        let (provider, mut started, mut token) = delayed_tokens();
        let plan = RoutingPlan {
            http: HttpRoute {
                mode,
                prefix: origin.clone(),
                authorize: true,
                tunnel_url: Some(origin.replacen("http:", "ws:", 1) + "/tunnel"),
                ..HttpRoute::default()
            },
            ..RoutingPlan::default()
        };
        let (send, events) = mpsc::unbounded_channel();
        NetworkTransport::with_routing(plan, provider)
            .request(http_request(60, "https://venue.invalid/order".into()), send);
        started.recv().await.unwrap();
        drop(events);
        tokio::time::timeout(Duration::from_secs(2), token.closed())
            .await
            .expect("host disposal must drop pending token acquisition");
        assert!(token.send("token-after-disposal".into()).is_err());
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err(),
            "a disposed host must not connect to the proxy or HOW tunnel"
        );
    }
}

#[tokio::test]
async fn dropping_the_host_cancels_how_authentication_without_sending_a_write() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::text(
            r#"{"type":"serverTime","serverTime":1700000000000}"#,
        ))
        .await
        .unwrap();
        assert!(
            !matches!(ws.next().await, Some(Ok(Message::Text(_)))),
            "a disposed host must not send proxy credentials or an HTTP write"
        );
    });
    let (provider, mut started, mut token) = delayed_tokens();
    let mut plan = RoutingPlan::default();
    plan.http.mode = HttpRouteMode::How;
    plan.http.tunnel_url = Some(origin.replacen("http:", "ws:", 1) + "/tunnel");
    let (send, events) = mpsc::unbounded_channel();
    NetworkTransport::with_routing(plan, provider)
        .request(http_request(61, "https://venue.invalid/order".into()), send);
    started.recv().await.unwrap();
    drop(events);
    tokio::time::timeout(Duration::from_secs(2), token.closed())
        .await
        .expect("host disposal must drop pending tunnel token acquisition");
    assert!(token.send("token-after-disposal".into()).is_err());
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .expect("host disposal must close the pending tunnel")
        .unwrap();
}

#[tokio::test]
async fn malformed_binary_connections_advance_to_fallback_without_emitting_messages() {
    for malformed in [vec![0x1f, 0x8b, 0], vec![0xff, 0xfe]] {
        let (listener, origin) = listener().await;
        let origin = origin.replacen("http:", "ws:", 1);
        let server = tokio::spawn(async move {
            for attempt in 0..3 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_hdr_async(
                    stream,
                    move |request: &Request, response: Response| {
                        assert_eq!(
                            request.uri().path(),
                            if attempt < 2 { "/direct" } else { "/fallback" }
                        );
                        Ok(response)
                    },
                )
                .await
                .unwrap();
                if attempt < 2 {
                    ws.send(Message::binary(malformed.clone())).await.unwrap();
                    let _ = ws.next().await;
                } else {
                    ws.send(Message::text("fallback-ready")).await.unwrap();
                    ws.close(None).await.unwrap();
                }
            }
        });
        let mut plan = RoutingPlan::default();
        plan.ws.private.fallback = Some(format!("{origin}/fallback"));
        let transport = NetworkTransport::with_routing(plan, tokens());
        for attempt in 0..3 {
            let (send, mut events) = mpsc::unbounded_channel();
            let outgoing = transport.connect_channel(
                SocketId(attempt),
                format!("{origin}/direct"),
                vec![],
                Some(safe_cex_engine::SocketChannel::Private),
                send,
            );
            assert!(matches!(next(&mut events).await, HostEvent::WsOpened(_)));
            if attempt < 2 {
                assert!(matches!(
                    next(&mut events).await,
                    HostEvent::WsClosed(_, 1007)
                ));
                assert!(
                    events.try_recv().is_err(),
                    "invalid bytes must never become a venue message"
                );
            } else {
                assert!(
                    matches!(next(&mut events).await, HostEvent::WsMessage(_, text) if text == "fallback-ready")
                );
                assert!(matches!(next(&mut events).await, HostEvent::WsClosed(_, _)));
            }
            drop(outgoing);
        }
        server.await.unwrap();
    }
}

#[tokio::test]
async fn how_response_header_values_match_js_string_coercion_on_the_wire() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
        ws.send(Message::text(r#"{"type":"rpc.auth","status":"success"}"#))
            .await
            .unwrap();
        let request = rpc(&mut ws).await;
        let nonce = request.nonce.unwrap();
        let response = format!(
            r#"{{"type":"rpc.http","nonce":"{nonce}","data":{{"status":200,"headers":{{"set-cookie":["session=a","csrf=b"],"x-nested":["a",null,["b","c"],false,{{}},1e21,1e-7,-0.0],"x-number":7.0,"x-null":null,"x-object":{{"ignored":"member"}},"x-bool":true}},"data_raw":"{{}}"}}}}"#
        );
        ws.send(Message::text(response)).await.unwrap();
        let _ = ws.next().await;
    });
    let mut plan = RoutingPlan::default();
    plan.http.mode = HttpRouteMode::How;
    plan.http.tunnel_url = Some(origin.replacen("http:", "ws:", 1) + "/tunnel");
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::with_routing(plan, tokens()).request(
        http_request(91, "https://venue.invalid/headers".into()),
        send,
    );
    let HostEvent::HttpStart {
        status, headers, ..
    } = next(&mut events).await
    else {
        panic!("expected response headers")
    };
    assert_eq!(status, 200);
    let headers: BTreeMap<_, _> = headers
        .lines()
        .map(|line| line.split_once(": ").unwrap())
        .collect();
    assert_eq!(
        headers,
        BTreeMap::from([
            ("set-cookie", "session=a,csrf=b"),
            ("x-nested", "a,,b,c,false,[object Object],1e+21,1e-7,0"),
            ("x-number", "7"),
            ("x-null", "null"),
            ("x-object", "[object Object]"),
            ("x-bool", "true"),
        ])
    );
    assert!(
        matches!(next(&mut events).await, HostEvent::HttpChunk(RequestId(91), bytes) if bytes == b"{}")
    );
    assert!(matches!(
        next(&mut events).await,
        HostEvent::HttpEnd(RequestId(91))
    ));
    server.await.unwrap();
}

async fn feed_connection(
    transport: &NetworkTransport,
    url: String,
    channel: safe_cex_engine::SocketChannel,
    feed: Option<&str>,
    recover: bool,
) -> Vec<String> {
    let (send, mut events) = mpsc::unbounded_channel();
    let outgoing = transport.connect_context(
        SocketId(100),
        url,
        vec![],
        Some(channel),
        feed.map(|feed| safe_cex_engine::SocketContext {
            feed: feed.into(),
            recover,
        }),
        send,
    );
    assert!(matches!(next(&mut events).await, HostEvent::WsOpened(_)));
    let mut messages = Vec::new();
    loop {
        match next(&mut events).await {
            HostEvent::WsMessage(_, text) => messages.push(text),
            HostEvent::WsClosed(_, _) => break,
            event => panic!("unexpected feed event {event:?}"),
        }
    }
    drop(outgoing);
    messages
}

#[tokio::test]
async fn independent_feeds_on_one_url_keep_their_own_silent_fallback_history() {
    let (listener, origin) = listener().await;
    let origin = origin.replacen("http:", "ws:", 1);
    let server = tokio::spawn(async move {
        for expected in [
            "/direct",
            "/fallback",
            "/direct",
            "/direct",
            "/fallback",
            "/fallback",
        ] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &Request, response: Response| {
                    assert_eq!(request.uri().path(), expected);
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let mut plan = RoutingPlan::default();
    plan.ws.public.initial_silent_connects = Some(1);
    plan.ws.public.fallback = Some(format!("{origin}/fallback"));
    let transport = NetworkTransport::with_routing(plan, tokens());
    // Empty and absent feed names denote the legacy feed; named feeds stay independent.
    for feed in [
        Some("primary"),
        Some("primary"),
        Some("secondary"),
        None,
        Some(""),
        Some("primary"),
    ] {
        feed_connection(
            &transport,
            format!("{origin}/direct"),
            safe_cex_engine::SocketChannel::Public,
            feed,
            false,
        )
        .await;
    }
    server.await.unwrap();
}

#[tokio::test]
async fn initial_silence_and_ignored_private_pongs_select_the_second_connect_fallback() {
    let (listener, origin) = listener().await;
    let origin = origin.replacen("http:", "ws:", 1);
    let server = tokio::spawn(async move {
        for expected in ["/direct", "/fallback", "/fallback", "/direct", "/direct"] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &Request, response: Response| {
                    assert_eq!(request.uri().path(), expected);
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.send(Message::text("pong")).await.unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let mut plan = RoutingPlan::default();
    plan.ws.private.initial_silent_connects = Some(1);
    plan.ws.private.ignored_messages = vec!["pong".into()];
    plan.ws.private.fallback = Some(format!("{origin}/fallback"));
    plan.ws.public.initial_silent_connects = Some(1);
    plan.ws.public.fallback = Some(format!("{origin}/fallback"));
    let transport = NetworkTransport::with_routing(plan, tokens());
    for channel in [
        safe_cex_engine::SocketChannel::Private,
        safe_cex_engine::SocketChannel::Private,
        safe_cex_engine::SocketChannel::Private,
        safe_cex_engine::SocketChannel::Public,
        safe_cex_engine::SocketChannel::Public,
    ] {
        let messages = feed_connection(
            &transport,
            format!("{origin}/direct"),
            channel,
            Some("same-feed-name"),
            false,
        )
        .await;
        assert_eq!(
            messages,
            ["pong"],
            "ignored liveness frames must still reach the venue"
        );
    }
    server.await.unwrap();
}

#[tokio::test]
async fn recovery_sticks_to_the_final_declared_fallback_after_a_healthy_connection() {
    let (listener, origin) = listener().await;
    let origin = origin.replacen("http:", "ws:", 1);
    let server = tokio::spawn(async move {
        for expected in ["/direct", "/fallback", "/fallback", "/ordinary-proxy"] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &Request, response: Response| {
                    assert_eq!(request.uri().path(), expected);
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.send(Message::text("authenticated-payload"))
                .await
                .unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let mut plan = RoutingPlan::default();
    plan.ws.private.relay =
        Some("ws://127.0.0.1:1/unwanted-relay?ts-relay=wss%3A%2F%2Fvenue.invalid%2Fprivate".into());
    plan.ws.private.fallback = Some(format!("{origin}/fallback"));
    let transport = NetworkTransport::with_routing(plan, tokens());
    for recover in [false, true, false] {
        assert_eq!(
            feed_connection(
                &transport,
                format!("{origin}/direct"),
                safe_cex_engine::SocketChannel::Private,
                Some("private"),
                recover
            )
            .await,
            ["authenticated-payload"]
        );
    }
    let mut no_fallback = RoutingPlan::default();
    no_fallback.ws.private.host = Some(format!("{origin}/ordinary-proxy"));
    // A recovery hint with no declared fallback preserves the provider's normal route.
    let transport = NetworkTransport::with_routing(no_fallback, tokens());
    feed_connection(
        &transport,
        format!("{origin}/direct"),
        safe_cex_engine::SocketChannel::Private,
        Some("private"),
        true,
    )
    .await;
    server.await.unwrap();
}

#[tokio::test]
async fn explicit_direct_urls_bypass_recovery_and_do_not_mutate_query_variant_counters() {
    let (listener, origin) = listener().await;
    let origin = origin.replacen("http:", "ws:", 1);
    let server = tokio::spawn(async move {
        for expected in ["/v3/private", "/v3/private", "/v3/private", "/proxy"] {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_hdr_async(
                stream,
                move |request: &Request, response: Response| {
                    assert_eq!(request.uri().path(), expected);
                    assert_eq!(
                        request.headers().contains_key("origin"),
                        expected == "/proxy"
                    );
                    Ok(response)
                },
            )
            .await
            .unwrap();
            ws.close(None).await.unwrap();
        }
    });
    let direct = format!("{origin}/v3/private");
    let mut plan = RoutingPlan::default();
    plan.ws.private.host = Some(format!("{origin}/proxy"));
    plan.ws.private.relay = Some("ws://127.0.0.1:1/unused-relay".into());
    plan.ws.private.fallback = Some(format!("{origin}/fallback"));
    plan.ws.private.initial_silent_connects = Some(1);
    plan.ws.private.direct_urls = vec![direct.clone()];
    plan.ws_headers
        .insert("Origin".into(), "https://preview.tealstreet.io".into());
    let transport = NetworkTransport::with_routing(plan, tokens());
    for recover in [true, false, false] {
        feed_connection(
            &transport,
            direct.clone(),
            safe_cex_engine::SocketChannel::Private,
            Some("private"),
            recover,
        )
        .await;
    }
    feed_connection(
        &transport,
        format!("{direct}?not-an-exact-match=1"),
        safe_cex_engine::SocketChannel::Private,
        Some("private"),
        false,
    )
    .await;
    server.await.unwrap();
}

#[tokio::test]
async fn explicit_public_routes_reach_direct_and_cors_without_account_authorization() {
    let (listener, origin) = listener().await;
    let server = tokio::spawn(async move {
        let mut captured = Vec::new();
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().await.unwrap();
            captured.push(request_text(&mut stream).await);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\n{}")
                .await
                .unwrap();
        }
        captured
    });
    let tokens_used = Arc::new(AtomicUsize::new(0));
    let used = tokens_used.clone();
    let provider: ProxyTokenProvider = Arc::new(move || {
        used.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok("must-not-be-used".into()) })
    });
    let transport = NetworkTransport::with_routing(
        RoutingPlan {
            http: HttpRoute {
                mode: HttpRouteMode::How,
                prefix: "https://account-proxy.invalid".into(),
                authorize: true,
                tunnel_url: Some("wss://account-tunnel.invalid".into()),
                ..HttpRoute::default()
            },
            public_cors_http: Some(HttpRoute {
                prefix: format!("{origin}/cors"),
                ..HttpRoute::default()
            }),
            ..RoutingPlan::default()
        },
        provider,
    );
    let (send, mut events) = mpsc::unbounded_channel();
    for (id, route, url) in [
        (
            1,
            safe_cex_engine::RequestRoute::Direct,
            format!("{origin}/fapi/v3/time?bytes=one%2Btwo"),
        ),
        (
            2,
            safe_cex_engine::RequestRoute::PublicCors,
            "https://venue.invalid/fapi/v1/time".into(),
        ),
    ] {
        transport.request(
            HttpRequest {
                id: RequestId(id),
                method: Method::Get,
                route: Some(route),
                timeout_ms: None,
                url,
                headers: vec![],
                body: None,
            },
            send.clone(),
        );
        assert_eq!(http_body(&mut events).await, b"{}");
    }
    let captured = server.await.unwrap();
    assert!(captured[0].starts_with("GET /fapi/v3/time?bytes=one%2Btwo HTTP/1.1\r\n"));
    assert!(captured[1].starts_with("GET /cors/https://venue.invalid/fapi/v1/time HTTP/1.1\r\n"));
    assert!(captured.iter().all(
        |request| !request.to_ascii_lowercase().contains("authorization:")
            && !request.to_ascii_lowercase().contains("x-mbx-apikey:")
            && !request.contains("publicCors")
    ));
    assert_eq!(tokens_used.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn raw_native_transport_rejects_unconfigured_public_cors_route() {
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::default().request(
        HttpRequest {
            id: RequestId(200),
            method: Method::Get,
            route: Some(safe_cex_engine::RequestRoute::PublicCors),
            timeout_ms: None,
            url: "https://venue.invalid/time".into(),
            headers: vec![],
            body: None,
        },
        send,
    );
    assert!(
        matches!(next(&mut events).await,HostEvent::HttpFailed(RequestId(200),error) if error.contains("not configured"))
    );
}

#[tokio::test]
async fn typed_clock_deadline_stays_active_while_native_response_body_stalls() {
    let (listener, origin) = listener().await;
    let (release, held) = oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _ = request_text(&mut stream).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\n\r\nx")
            .await
            .unwrap();
        let _ = held.await;
    });
    let (send, mut events) = mpsc::unbounded_channel();
    NetworkTransport::default().request(
        HttpRequest {
            id: RequestId(201),
            method: Method::Get,
            route: Some(safe_cex_engine::RequestRoute::Direct),
            timeout_ms: Some(5_000),
            url: origin,
            headers: vec![],
            body: None,
        },
        send,
    );
    assert!(matches!(
        next(&mut events).await,
        HostEvent::HttpStart {
            request: RequestId(201),
            status: 200,
            ..
        }
    ));
    assert!(
        matches!(next(&mut events).await,HostEvent::HttpChunk(RequestId(201),chunk) if chunk==b"x")
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(4)).await;
    assert!(events.try_recv().is_err());
    tokio::time::advance(Duration::from_millis(1_100)).await;
    assert!(matches!(
        next(&mut events).await,
        HostEvent::HttpFailed(RequestId(201), _)
    ));
    let _ = release.send(());
    server.await.unwrap();
}
