//! HTTP routes and the `/account` upgrade. Nothing here logs a URI, header or
//! query string: the proxy token rides in the query.
use crate::accounts::{Registry, now_ms};
use crate::auth::Verifier;
use crate::config::Config;
use crate::out::{OUTBOUND_CAPACITY, Out};
use crate::session::{Close, Identity, Session};
use crate::state::{Log, Metrics};
use axum::Router;
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Grace after `exp` before the socket closes, as `PROXY_TOKEN_RECONNECT_GRACE_MS`.
const TOKEN_GRACE: Duration = Duration::from_secs(30);
const KEEPALIVE: Duration = Duration::from_secs(10);
const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// A peer that stops reading cannot hold its socket: every write has a deadline.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

pub struct App {
    pub registry: Arc<Registry>,
    pub metrics: Arc<Metrics>,
    pub verifier: Arc<Verifier>,
    pub config: Config,
    pub started: Instant,
    pub public_per_ip: Mutex<HashMap<IpAddr, usize>>,
    /// True once sockets must close: the process is stopping.
    pub closing: tokio::sync::watch::Sender<bool>,
}

/// Inside the 30 s a roll allows, with room for the close frames.
const DRAIN_DEADLINE: Duration = Duration::from_secs(25);
const CLOSE_FRAMES_DEADLINE: Duration = Duration::from_secs(2);

/// SIGTERM: settle every account while browsers can still sign, then close each
/// socket 1001 so its page reconnects to the next process.
pub async fn drain(app: &App) {
    if !app.registry.drain(DRAIN_DEADLINE).await {
        Log::warn("drain passed its deadline; orders may rest");
    }
    let _ = app.closing.send(true);
    let until = tokio::time::Instant::now() + CLOSE_FRAMES_DEADLINE;
    while app.metrics.snapshot().0 > 0 && tokio::time::Instant::now() < until {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Break-glass: stop every chase and TWAP, telling each page what stopped, then
/// move every socket. False if some account had not ended by the deadline.
pub async fn drain_now(app: &App) -> bool {
    let settled = app.registry.break_glass(DRAIN_DEADLINE).await;
    if !settled {
        Log::warn("break-glass drain passed its deadline; orders may rest");
    }
    settled
}

pub fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/health", get(health).options(preflight))
        .route("/admin/drain", post(admin_drain))
        .route("/admin/undrain", post(admin_undrain))
        .route("/capabilities", get(capabilities).options(preflight))
        .route(
            "/account/capabilities",
            get(capabilities).options(preflight),
        )
        .route("/", get(upgrade))
        .route("/account", get(upgrade))
        .with_state(app)
}

fn cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert("access-control-allow-origin", HeaderValue::from_static("*"));
    headers.insert(
        "access-control-allow-methods",
        HeaderValue::from_static("GET, OPTIONS"),
    );
    headers.insert(
        "access-control-allow-headers",
        HeaderValue::from_static("content-type"),
    );
    response
}

async fn preflight() -> Response {
    cors(StatusCode::NO_CONTENT.into_response())
}

async fn health(State(app): State<Arc<App>>) -> Response {
    let (connections, inits, disposes) = app.metrics.snapshot();
    let draining = app.registry.drain_state() != crate::accounts::Drain::Off;
    let (chases, twaps) = app.registry.running_algos();
    let body = json!({
        // A draining box takes no new users: its load balancer pool fails over.
        "ok": !draining,
        "draining": draining,
        "algos": {"chases": chases, "twaps": twaps},
        "runtime": "rust",
        "uptime": app.started.elapsed().as_secs(),
        "release": app.config.release,
        "region": app.config.region,
        "connections": connections,
        "accounts": app.registry.account_count(),
        "engineThreads": app.registry.threads().thread_count(),
        "accountStartPanics": app.registry.threads().panics(),
        "inits": inits,
        "disposes": disposes,
    });
    let status = if draining {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };
    cors((status, axum::Json(body)).into_response())
}

/// Headers an edge or proxy adds. cloudflared dials this port from loopback, so a
/// loopback peer alone does not prove the operator is on the box.
const FORWARDED_HEADERS: [&str; 6] = [
    "cf-connecting-ip",
    "cf-ray",
    "cdn-loop",
    "x-forwarded-for",
    "x-real-ip",
    "forwarded",
];

/// Admin routes answer only a peer on this box (loopback, or the Docker bridge the
/// published port arrives through) whose request no proxy forwarded.
pub fn admin_peer(headers: &HeaderMap, peer: SocketAddr) -> bool {
    let on_box = match peer.ip().to_canonical() {
        std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private(),
        std::net::IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local(),
    };
    on_box
        && !FORWARDED_HEADERS
            .iter()
            .any(|name| headers.contains_key(*name))
}

#[derive(Deserialize)]
struct DrainQuery {
    now: Option<String>,
}

fn drain_status(app: &App) -> serde_json::Value {
    let (chases, twaps) = app.registry.running_algos();
    let drain = match app.registry.drain_state() {
        crate::accounts::Drain::Off => "off",
        crate::accounts::Drain::Waiting => "waiting",
        crate::accounts::Drain::Now => "now",
    };
    json!({"drain": drain, "algos": {"chases": chases, "twaps": twaps}})
}

/// `POST /admin/drain` starts a drain; `?now=1` is break-glass.
async fn admin_drain(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<DrainQuery>,
) -> Response {
    if !admin_peer(&headers, peer) {
        return (StatusCode::FORBIDDEN, "admin routes are local only").into_response();
    }
    if query.now.as_deref() == Some("1") {
        Log::info("admin: break-glass drain");
        let settled = drain_now(&app).await;
        let mut body = drain_status(&app);
        body["settled"] = settled.into();
        return axum::Json(body).into_response();
    }
    Log::info("admin: drain");
    app.registry.start_drain();
    axum::Json(drain_status(&app)).into_response()
}

/// `POST /admin/undrain` ends a drain that did not end in a restart.
async fn admin_undrain(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !admin_peer(&headers, peer) {
        return (StatusCode::FORBIDDEN, "admin routes are local only").into_response();
    }
    if *app.closing.borrow() {
        return (StatusCode::CONFLICT, "the process is stopping").into_response();
    }
    Log::info("admin: undrain");
    app.registry.undrain();
    axum::Json(drain_status(&app)).into_response()
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

/// Cloudflare's published edge ranges (cloudflare.com/ips). Only a peer inside
/// them may name the client with `cf-connecting-ip`; anyone else is the client.
const CLOUDFLARE: [&str; 22] = [
    "173.245.48.0/20",
    "103.21.244.0/22",
    "103.22.200.0/22",
    "103.31.4.0/22",
    "141.101.64.0/18",
    "108.162.192.0/18",
    "190.93.240.0/20",
    "188.114.96.0/20",
    "197.234.240.0/22",
    "198.41.128.0/17",
    "162.158.0.0/15",
    "104.16.0.0/13",
    "104.24.0.0/14",
    "172.64.0.0/13",
    "131.0.72.0/22",
    "2400:cb00::/32",
    "2606:4700::/32",
    "2803:f800::/32",
    "2405:b500::/32",
    "2405:8100::/32",
    "2a06:98c0::/29",
    "2c0f:f248::/32",
];

fn in_cidr(ip: IpAddr, cidr: &str) -> bool {
    let Some((base, bits)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(base), Ok(bits)) = (base.parse::<IpAddr>(), bits.parse::<u32>()) else {
        return false;
    };
    match (ip, base) {
        (IpAddr::V4(ip), IpAddr::V4(base)) => {
            let mask = u32::MAX.checked_shl(32 - bits).unwrap_or(0);
            u32::from(ip) & mask == u32::from(base) & mask
        }
        (IpAddr::V6(ip), IpAddr::V6(base)) => {
            let mask = u128::MAX.checked_shl(128 - bits).unwrap_or(0);
            u128::from(ip) & mask == u128::from(base) & mask
        }
        _ => false,
    }
}

/// The client address: `cf-connecting-ip` only from a Cloudflare edge peer.
pub fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> IpAddr {
    // A dual-stack listener reports IPv4 peers as ::ffff:a.b.c.d.
    let peer_ip = peer.ip().to_canonical();
    let from_edge = CLOUDFLARE.iter().any(|cidr| in_cidr(peer_ip, cidr));
    headers
        .get("cf-connecting-ip")
        .filter(|_| from_edge)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(peer_ip)
}

async fn upgrade(
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Query(query): Query<UpgradeQuery>,
    upgrade: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let Ok(upgrade) = upgrade else {
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
                return (
                    StatusCode::UNAUTHORIZED,
                    "Valid Tealstreet proxy token required",
                )
                    .into_response();
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
    let mut response =
        upgrade
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

async fn send(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: Message,
) -> bool {
    matches!(
        tokio::time::timeout(SEND_TIMEOUT, sink.send(message)).await,
        Ok(Ok(()))
    )
}

/// What the page was already sent goes first (a break-glass stop notice), then the move.
async fn move_away(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    outgoing: &mut tokio::sync::mpsc::Receiver<String>,
    moved: Close,
) {
    while let Ok(text) = outgoing.try_recv() {
        if !send(sink, Message::Text(text.into())).await {
            return;
        }
    }
    let frame = CloseFrame {
        code: moved.code(),
        reason: moved.reason().into(),
    };
    send(sink, Message::Close(Some(frame))).await;
}

fn idle_dropped(socket: u64, user: Option<&str>) -> String {
    format!(
        "idle socket dropped: socket {socket} user {}",
        user.unwrap_or("none")
    )
}

async fn serve(app: Arc<App>, identity: Identity, socket: WebSocket) {
    app.metrics.connections.fetch_add(1, Ordering::Relaxed);
    let (mut sink, mut stream) = socket.split();
    let (out, mut outgoing, overflow) = Out::channel(OUTBOUND_CAPACITY);
    let expires = identity
        .token
        .as_ref()
        .map(|(_, expires_at_ms)| *expires_at_ms);
    let user = identity.user.clone();
    let mut session = Session::new(app.registry.clone(), app.metrics.clone(), identity, out);
    let mut loaded = session.take_loaded();
    let mut dropped = session.take_dropped();
    session.open().await;
    let taken_over = session.taken_over();
    let mut closing = app.closing.subscribe();
    let mut drain = app.registry.watch_drain();
    let mut algos_ended = app.registry.watch_algos_ended();
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
            _ = overflow.notified() => {
                Log::warn("slow client dropped: outbound queue full");
                send(&mut sink, close(1013, "Client is not reading; reconnect")).await;
                break;
            }
            _ = closing.changed() => {
                send(&mut sink, close(1001, "Server restarting")).await;
                break;
            }
            _ = &mut expiry => {
                send(&mut sink, close(1008, "Tealstreet proxy token expired")).await;
                break;
            }
            Ok(()) = drain.changed() => if let Some(moved) = session.drain_close() {
                move_away(&mut sink, &mut outgoing, moved).await;
                break;
            },
            Ok(()) = algos_ended.changed() => if let Some(moved) = session.drain_close() {
                move_away(&mut sink, &mut outgoing, moved).await;
                break;
            },
            // A resume of this session arrived on a new socket: this one is dead.
            _ = taken_over.notified() => {
                Log::info("session taken over by a resume");
                break;
            }
            Some((name, client)) = loaded.recv() => session.account_loaded(&name, client).await,
            Some((name, client)) = dropped.recv() => session.client_dropped(&name, client).await,
            text = outgoing.recv() => match text {
                Some(text) => if !send(&mut sink, Message::Text(text.into())).await { break },
                None => break,
            },
            incoming = stream.next() => {
                let Some(Ok(message)) = incoming else { break };
                last_seen = tokio::time::Instant::now();
                session.heard();
                match message {
                    Message::Text(text) => {
                        if let Err(refusal) = session.handle(text.as_str()).await {
                            if refusal == Close::Moved {
                                move_away(&mut sink, &mut outgoing, refusal).await;
                            } else {
                                send(&mut sink, close(refusal.code(), refusal.reason())).await;
                            }
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            _ = keepalive.tick() => {
                if last_seen.elapsed() > idle {
                    Log::warn(&idle_dropped(session.socket, user.as_deref()));
                    break;
                }
                if !send(&mut sink, Message::Ping(Vec::new().into())).await { break }
            }
        }
    }
    session.close();
    app.metrics.connections.fetch_sub(1, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_routes_answer_only_an_unforwarded_loopback_peer() {
        let none = HeaderMap::new();
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let mapped: SocketAddr = "[::ffff:127.0.0.1]:5000".parse().unwrap();
        assert!(admin_peer(&none, local));
        assert!(admin_peer(&none, mapped));
        assert!(admin_peer(&none, "[::1]:5000".parse().unwrap()));
        // The box's Docker bridge: the port is published on the host's loopback only.
        assert!(admin_peer(&none, "172.18.0.1:5000".parse().unwrap()));
        for remote in ["100.64.1.2:5000", "172.64.1.2:443", "[2606:4700::1]:443"] {
            assert!(!admin_peer(&none, remote.parse().unwrap()), "{remote}");
        }
        // cloudflared dials from loopback; the edge's headers give it away.
        for header in FORWARDED_HEADERS {
            let mut forwarded = HeaderMap::new();
            forwarded.insert(header, HeaderValue::from_static("1.2.3.4"));
            assert!(!admin_peer(&forwarded, local), "{header}");
        }
    }

    #[tokio::test]
    async fn health_counts_running_chases_and_twaps_and_reports_draining() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let registry = crate::accounts::tests::registry();
                let link = safe_cex_remote_signer::SignerLink::new(7, Arc::new(|_| true));
                let account =
                    crate::accounts::tests::held_account_counted(&registry, link, String::new());
                tokio::time::sleep(Duration::from_millis(200)).await;
                let op = |op: &str, data: &str| {
                    let input = crate::algos::AlgoInput::Op {
                        op: op.into(),
                        data: data.into(),
                        origin: "test".into(),
                    };
                    account.algos.as_ref().unwrap().send(input).unwrap();
                };
                op(
                    "chaser:start",
                    r#"{"symbol":"BTCUSDT","size":0.01,"side":"buy","maxAttempts":1000,"reduceOnly":false,"delay":500,"origin":"web"}"#,
                );
                let market = r#"{"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":100}},"isPerpetual":true}"#;
                op(
                    "twap:start",
                    &format!(
                        r#"{{"accountName":"acct","activeMarket":{market},"size":0.1,"duration":600,"ordersCount":10,"side":"buy","chaserDelay":500,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false,"origin":"web"}}"#
                    ),
                );
                tokio::time::sleep(Duration::from_millis(600)).await;
                let config = Config {
                    port: 0,
                    region: None,
                    release: None,
                    engine_threads: 1,
                    max_connections: 16,
                    max_public_per_ip: 2,
                    idle_timeout: Duration::from_secs(30),
                    resume_grace: Duration::from_secs(60),
                    account_flush: Duration::from_millis(60),
                };
                let app = Arc::new(App {
                    registry: registry.clone(),
                    metrics: Arc::default(),
                    verifier: Arc::new(Verifier::new(crate::auth::PROXY_PUBLIC_KEY).unwrap()),
                    config,
                    started: Instant::now(),
                    public_per_ip: Mutex::default(),
                    closing: tokio::sync::watch::channel(false).0,
                });
                let read = async |app: &Arc<App>| {
                    let response = health(State(app.clone())).await;
                    let status = response.status().as_u16();
                    let body = axum::body::to_bytes(response.into_body(), 1 << 16)
                        .await
                        .unwrap();
                    (
                        status,
                        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
                    )
                };
                let (status, body) = read(&app).await;
                assert_eq!(status, 200, "{body}");
                assert_eq!(body["ok"], true);
                assert_eq!(body["draining"], false);
                assert_eq!(body["algos"], json!({"chases": 1, "twaps": 1}));
                registry.start_drain();
                let (status, body) = read(&app).await;
                assert_eq!(status, 503, "a draining box takes no new users: {body}");
                assert_eq!(body["ok"], false);
                assert_eq!(body["draining"], true);
                assert_eq!(
                    body["algos"],
                    json!({"chases": 1, "twaps": 1}),
                    "the drain never stops them"
                );
            })
            .await;
    }

    #[test]
    fn an_idle_drop_names_its_socket_and_user() {
        assert_eq!(
            idle_dropped(7, Some("u1")),
            "idle socket dropped: socket 7 user u1"
        );
        assert_eq!(
            idle_dropped(8, None),
            "idle socket dropped: socket 8 user none"
        );
    }

    #[test]
    fn only_cloudflare_peers_may_name_the_client() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-connecting-ip", HeaderValue::from_static("9.9.9.9"));
        let edge: SocketAddr = "172.64.1.2:443".parse().unwrap();
        let direct: SocketAddr = "5.6.7.8:443".parse().unwrap();
        assert_eq!(
            client_ip(&headers, edge),
            "9.9.9.9".parse::<IpAddr>().unwrap()
        );
        assert_eq!(
            client_ip(&headers, direct),
            direct.ip(),
            "a spoofed header is ignored"
        );
        let edge6: SocketAddr = "[2606:4700::1]:443".parse().unwrap();
        assert_eq!(
            client_ip(&headers, edge6),
            "9.9.9.9".parse::<IpAddr>().unwrap()
        );
    }
}
