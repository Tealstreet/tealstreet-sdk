//! Loopback servers and their HTTP and WebSocket clients, shared by the
//! integration tests. Each test crate uses only some of it.
#![allow(dead_code)]
use futures_util::{SinkExt, StreamExt};
use safe_cex_exchange_server::accounts::Registry;
use safe_cex_exchange_server::auth::Verifier;
use safe_cex_exchange_server::config::Config;
use safe_cex_exchange_server::envelope::InitPayload;
use safe_cex_exchange_server::http::{App, router};
use safe_cex_exchange_server::state::Metrics;
use safe_cex_exchange_server::threads::EngineThreads;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub fn config(resume_grace: Duration) -> Config {
    Config {
        port: 0,
        region: Some("tyo".into()),
        release: Some("test".into()),
        engine_threads: 1,
        max_connections: 16,
        max_public_per_ip: 2,
        idle_timeout: Duration::from_secs(30),
        resume_grace,
        account_flush: Duration::from_millis(60),
        access: None,
        router_lease: Default::default(),
        idle_release_ttl: Duration::from_millis(
            safe_cex_exchange_server::user_drain::DEFAULT_IDLE_RELEASE_TTL_MS,
        ),
    }
}

pub fn app(config: Config, verifier: Verifier) -> Arc<App> {
    let mut registry = Registry::new(
        EngineThreads::start(1).unwrap(),
        config.account_flush,
        config.resume_grace,
    )
    .with_router_lease(config.router_lease.clone());
    safe_cex_exchange_server::accounts::testing::set_join_admission(&mut registry, false);
    Arc::new(App {
        registry: Arc::new(registry),
        metrics: Arc::new(Metrics::default()),
        verifier: Arc::new(verifier),
        config,
        started: std::time::Instant::now(),
        public_per_ip: Default::default(),
        closing: tokio::sync::watch::channel(false).0,
    })
}

/// Serves `app` on a fresh localhost port, as `main` does; returns its address.
pub async fn serve(app: Arc<App>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            router(app).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    address.to_string()
}

pub type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// The next close frame, or a panic naming what came instead.
pub async fn next_close(socket: &mut Socket) -> (u16, String) {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), socket.next()).await {
            Ok(Some(Ok(Message::Close(Some(frame))))) => {
                break (u16::from(frame.code), frame.reason.to_string());
            }
            Ok(Some(Ok(_))) => {}
            other => panic!("no close frame: {other:?}"),
        }
    }
}

pub async fn post(address: &str, path: &str, headers: &str) -> (u16, String) {
    request(address, "POST", path, headers).await
}

pub async fn get(address: &str, path: &str) -> (u16, String) {
    request(address, "GET", path, "").await
}

pub async fn request(address: &str, method: &str, path: &str, headers: &str) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: 0\r\n{headers}Connection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, body)
}

/// `GET /health` as JSON, with its status.
pub async fn health(address: &str) -> (u16, Value) {
    let (status, body) = get(address, "/health").await;
    (status, serde_json::from_str(&body).unwrap())
}

/// How a page's socket ended, if it did.
#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
    Closed(u16, String),
    /// The connection went away with no close frame: the box died.
    Dropped,
    StillOpen,
}

/// A browser page's resumable socket for one user, reading every envelope.
pub struct Page {
    socket: Socket,
    pub seen: Vec<Value>,
    pub resume_id: Option<String>,
}

impl Page {
    /// `token` is the user's proxy token; `resume` a session id to resume.
    pub async fn open(address: &str, token: &str, resume: Option<&str>) -> Self {
        let resume = resume.map(|id| format!("&resume={id}")).unwrap_or_default();
        let url = format!("ws://{address}/account?proxyToken={token}&resumable=1{resume}");
        let (socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let mut page = Self {
            socket,
            seen: Vec::new(),
            resume_id: None,
        };
        let session = page.until("session", |_| true).await;
        page.resume_id = session["resumeId"].as_str().map(str::to_owned);
        page
    }

    pub async fn send(&mut self, envelope: Value) {
        self.socket
            .send(Message::text(envelope.to_string()))
            .await
            .unwrap();
    }

    pub async fn init(&mut self, init: &InitPayload) {
        self.send(json!({"t": "init", "d": init})).await;
    }

    /// An account op, as `exchangeAccountController` posts it.
    pub async fn op(&mut self, account: &str, op: &str, data: Value) {
        self.send(json!({"t": "msg", "d": {"op": op, "accountName": account, "data": data}}))
            .await;
    }

    /// The `d` of the next envelope of type `t` (or `msg` op `t`) that matches.
    pub async fn until(&mut self, t: &str, matches: impl Fn(&Value) -> bool) -> Value {
        let found = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let Some(Ok(message)) = self.socket.next().await else {
                    return None;
                };
                let Message::Text(text) = message else {
                    continue;
                };
                let envelope: Value = serde_json::from_str(text.as_str()).unwrap();
                let d = envelope["d"].clone();
                self.seen.push(envelope.clone());
                let kind = if envelope["t"] == "msg" {
                    &d["op"]
                } else {
                    &envelope["t"]
                };
                if kind == t && matches(&d) {
                    return Some(d);
                }
            }
        })
        .await;
        match found {
            Ok(Some(d)) => d,
            other => panic!("no {t} ({other:?}); saw {:?}", self.seen),
        }
    }

    /// Reads until the socket ends or `within` passes.
    pub async fn ending(&mut self, within: Duration) -> Ending {
        let ended = tokio::time::timeout(within, async {
            loop {
                match self.socket.next().await {
                    Some(Ok(Message::Close(Some(frame)))) => {
                        return Ending::Closed(u16::from(frame.code), frame.reason.to_string());
                    }
                    Some(Ok(Message::Close(None))) => return Ending::Closed(1005, String::new()),
                    Some(Err(_)) | None => return Ending::Dropped,
                    Some(Ok(Message::Text(text))) => {
                        self.seen.push(serde_json::from_str(text.as_str()).unwrap());
                    }
                    Some(Ok(_)) => {}
                }
            }
        })
        .await;
        ended.unwrap_or(Ending::StillOpen)
    }

    /// The page closes its tab: a close frame, then the socket is gone.
    pub async fn close(mut self) {
        self.socket.close(None).await.unwrap();
        while let Some(Ok(_)) = self.socket.next().await {}
    }

    /// Every `msg` op `op` this page has been sent so far.
    pub fn ops(&self, op: &str) -> Vec<&Value> {
        self.seen
            .iter()
            .filter(|envelope| envelope["t"] == "msg" && envelope["d"]["op"] == op)
            .map(|envelope| &envelope["d"])
            .collect()
    }
}
