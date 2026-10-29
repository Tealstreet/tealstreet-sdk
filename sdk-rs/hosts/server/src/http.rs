//! HTTP routes and the `/account` upgrade. Nothing here logs a URI, header or
//! query string: the proxy token rides in the query.
use crate::accounts::{Registry, now_ms};
use crate::auth::Verifier;
use crate::config::Config;
use crate::session::{Identity, Session};
use crate::state::{Log, Metrics};
use axum::Router;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

/// Grace after `exp` before the socket closes, as `PROXY_TOKEN_RECONNECT_GRACE_MS`.
const TOKEN_GRACE: Duration = Duration::from_secs(30);
const KEEPALIVE: Duration = Duration::from_secs(10);
const MAX_MESSAGE_BYTES: usize = 256 * 1024;

pub struct App {
    pub registry: Arc<Registry>,
    pub metrics: Arc<Metrics>,
    pub verifier: Arc<Verifier>,
    pub config: Config,
    pub started: Instant,
    pub public_per_ip: Mutex<HashMap<IpAddr, usize>>,
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/health", get(health).options(preflight))
        .route("/capabilities", get(capabilities).options(preflight))
        .route("/account/capabilities", get(capabilities).options(preflight))
        .route("/", get(upgrade))
        .route("/account", get(upgrade))
        .with_state(app)
}

fn cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    headers.insert("access-control-allow-methods", HeaderValue::from_static("GET, OPTIONS"));
    headers.insert("access-control-allow-headers", HeaderValue::from_static("content-type"));
    response
}

async fn preflight() -> Response {
    cors(StatusCode::NO_CONTENT.into_response())
}

async fn health(State(app): State<Arc<App>>) -> Response {
    let (connections, inits, disposes) = app.metrics.snapshot();
    let body = json!({
        "ok": true,
        "runtime": "rust",
        "uptime": app.started.elapsed().as_secs(),
        "release": app.config.release,
        "region": app.config.region,
        "connections": connections,
        "accounts": app.registry.len(),
        "engineThreads": app.registry.threads().len(),
        "inits": inits,
        "disposes": disposes,
    });
    cors(axum::Json(body).into_response())
}

pub fn capabilities_json() -> serde_json::Value {
    json!({
        "service": "exchange-server",
        "version": 1,
        "runtime": "rust",
        "transports": {"accountWorkerWebSocket": {
            "edgePath": "/account",
            "rawPath": "/",
            "auth": "Tealstreet proxy JWT in the proxyToken query parameter",
            "privateSigning": "browser-delegated remote signer; private keys never enter exchange-server",
        }},
        "publicData": {"unauthenticated": {
            "exchange": crate::accounts::PUBLIC_EXCHANGE,
            "accountName": crate::accounts::PUBLIC_ACCOUNT,
            "operations": ["sendSnapshot", "subscribeOHLCV", "subscribeOrderBook", "unsubscribeOHLCV", "unsubscribeOrderBook"],
        }},
        "storeData": "SCXF frames in msg op 'frame'",
    })
}

async fn capabilities() -> Response {
    cors(axum::Json(capabilities_json()).into_response())
}

#[derive(Deserialize)]
struct UpgradeQuery {
    #[serde(rename = "proxyToken")]
    proxy_token: Option<String>,
    token: Option<String>,
    resume: Option<String>,
    resumable: Option<String>,
}

/// The edge's client address when it is in front; the peer otherwise.
fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    headers
        .get("cf-connecting-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer.ip())
}

async fn upgrade(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<UpgradeQuery>,
    upgrade: Option<WebSocketUpgrade>,
) -> Response {
    let Some(upgrade) = upgrade else {
        return (StatusCode::UPGRADE_REQUIRED, "Expected WebSocket").into_response();
    };
    if app.metrics.connections.load(Ordering::Relaxed) >= app.config.max_connections {
        return (StatusCode::SERVICE_UNAVAILABLE, "Too many connections").into_response();
    }
    let token = query
        .proxy_token
        .filter(|token| !token.is_empty())
        .or(query.token.filter(|token| !token.is_empty()));
    let identity = match token {
        Some(token) => {
            // A resume re-verifies here too: an expired token never takes a session back.
            let Some(verified) = app.verifier.verify(&token, now_ms()) else {
                return (StatusCode::UNAUTHORIZED, "Valid Tealstreet proxy token required").into_response();
            };
            Identity {
                user: Some(verified.user_id),
                token: Some((token, verified.expires_at_ms)),
                resume_from: query.resume,
                resumable: query.resumable.as_deref() == Some("1"),
            }
        }
        None => Identity {
            user: None,
            token: None,
            resume_from: None,
            resumable: false,
        },
    };
    let public_ip = identity.user.is_none().then(|| client_ip(&headers, peer));
    if let Some(ip) = public_ip {
        let mut counts = app.public_per_ip.lock().unwrap();
        let count = counts.entry(ip).or_default();
        if *count >= app.config.max_public_per_ip {
            return (StatusCode::TOO_MANY_REQUESTS, "Too many public connections").into_response();
        }
        *count += 1;
    }
    let mut response = upgrade
        .max_message_size(MAX_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            serve(app.clone(), identity, socket).await;
            if let Some(ip) = public_ip {
                let mut counts = app.public_per_ip.lock().unwrap();
                if let Some(count) = counts.get_mut(&ip) {
                    *count = count.saturating_sub(1);
                    if *count == 0 {
                        counts.remove(&ip);
                    }
                }
            }
        });
    response.headers_mut().remove(header::SERVER);
    response
}

async fn serve(app: Arc<App>, identity: Identity, socket: WebSocket) {
    app.metrics.connections.fetch_add(1, Ordering::Relaxed);
    let (mut sink, mut stream) = socket.split();
    let (out, mut outgoing) = mpsc::unbounded_channel::<String>();
    let expires = identity
        .token
        .as_ref()
        .map(|(_, expires_at_ms)| *expires_at_ms);
    let mut session = Session::new(app.registry.clone(), app.metrics.clone(), identity, out);
    session.open().await;
    let close_at = expires.map(|expires_at_ms| {
        let remaining = Duration::from_millis(expires_at_ms.saturating_sub(now_ms()));
        tokio::time::Instant::now() + remaining + TOKEN_GRACE
    });
    let expiry = async {
        match close_at {
            Some(deadline) => tokio::time::sleep_until(deadline).await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(expiry);
    let mut keepalive = tokio::time::interval(KEEPALIVE);
    let mut last_seen = tokio::time::Instant::now();
    let idle = app.config.idle_timeout;
    let close = |code: u16, reason: &'static str| {
        Message::Close(Some(CloseFrame {
            code,
            reason: reason.into(),
        }))
    };
    loop {
        tokio::select! {
            biased;
            _ = &mut expiry => {
                let _ = sink.send(close(1008, "Tealstreet proxy token expired")).await;
                break;
            }
            text = outgoing.recv() => match text {
                Some(text) => if sink.send(Message::Text(text.into())).await.is_err() { break },
                None => break,
            },
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                last_seen = tokio::time::Instant::now();
                match message {
                    Message::Text(text) => {
                        if let Err(violation) = session.handle(text.as_str()).await {
                            let _ = sink.send(close(1008, violation.0)).await;
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = keepalive.tick() => {
                if last_seen.elapsed() > idle {
                    Log::warn("idle socket dropped");
                    break;
                }
                if sink.send(Message::Ping(Vec::new().into())).await.is_err() { break }
            }
        }
    }
    session.close();
    app.metrics.connections.fetch_sub(1, Ordering::Relaxed);
}
