//! One HOW tunnel socket per account route, shared by its requests.
//!
//! Every `rpc.http` carries its own nonce, so requests multiplex on one socket and
//! answers may come in any order. A dropped socket fails its in-flight requests with
//! a transport error (the engine retries them); the next request reopens it, after a
//! jittered backoff when opening failed. Opens are capped per process, so a stalled
//! edge or a mass reconnect cannot turn into an upgrade storm.
use crate::Events;
use crate::routing::{
    ProxyTokenProvider, Socket, TunnelMessage, authenticate_how, decode_binary, http_response,
    ping, report_response,
};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::RequestId;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;

/// A failed open waits this long before the next, doubling to MAX_BACKOFF, with jitter.
const BASE_BACKOFF: Duration = Duration::from_millis(500);
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// A socket that drops sooner than this after opening counts as a failed open.
const SHORT_LIVED: Duration = Duration::from_secs(10);
/// One write to an open socket (a request, a pong, a ping, a close) may take this
/// long; a stalled one is a drop, so it cannot wedge every request on the route.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
/// Opening and authenticating one socket may take this long.
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);
/// Tunnel opens per process: a burst of this many, refilled at this rate per second.
const OPEN_BURST: f64 = 120.0;
const OPENS_PER_SECOND: f64 = 2.0;

/// The process-wide budget of tunnel opens.
pub(crate) struct OpenCap {
    burst: f64,
    per_second: f64,
    state: Mutex<(f64, Instant)>,
    logged: Mutex<Option<Instant>>,
}

impl OpenCap {
    pub(crate) fn new(burst: f64, per_second: f64) -> Self {
        Self {
            burst,
            per_second,
            state: Mutex::new((burst, Instant::now())),
            logged: Mutex::new(None),
        }
    }

    fn process() -> Arc<Self> {
        static CAP: std::sync::OnceLock<Arc<OpenCap>> = std::sync::OnceLock::new();
        CAP.get_or_init(|| Arc::new(Self::new(OPEN_BURST, OPENS_PER_SECOND)))
            .clone()
    }

    /// How long until an open is allowed, taking it when the answer is zero.
    fn take(&self, now: Instant) -> Duration {
        let mut state = self.state.lock().unwrap();
        let refilled =
            (state.0 + now.duration_since(state.1).as_secs_f64() * self.per_second).min(self.burst);
        *state = (refilled, now);
        if refilled >= 1.0 {
            state.0 -= 1.0;
            return Duration::ZERO;
        }
        Duration::from_secs_f64((1.0 - refilled) / self.per_second)
    }

    async fn acquire(&self, url: &str) {
        loop {
            let wait = self.take(Instant::now());
            if wait.is_zero() {
                return;
            }
            self.log_wait(url, wait);
            tokio::time::sleep(wait).await;
        }
    }

    /// A request waiting on the cap is logged, at most every 10 s per process.
    fn log_wait(&self, url: &str, wait: Duration) {
        let now = Instant::now();
        let mut logged = self.logged.lock().unwrap();
        if logged.is_some_and(|at| now.duration_since(at) < Duration::from_secs(10)) {
            return;
        }
        *logged = Some(now);
        eprintln!(
            "{}",
            serde_json::json!({
                "level": "warn",
                "source": "safe-cex-native",
                "msg": format!(
                    "HTTP tunnel opens capped at {}/s: {} waits {}ms to open",
                    self.per_second,
                    crate::transport::host_of(url),
                    wait.as_millis()
                ),
            })
        );
    }
}

/// The jittered wait before the next open after `failures` failed opens in a row.
fn backoff(base: Duration, failures: u32) -> Duration {
    let doubled = base.saturating_mul(1 << failures.saturating_sub(1).min(16));
    let capped = doubled.min(MAX_BACKOFF);
    let mut bytes = [0u8; 2];
    let _ = getrandom::getrandom(&mut bytes);
    let fraction = f64::from(u16::from_le_bytes(bytes)) / f64::from(u16::MAX);
    capped.mul_f64(0.5 + fraction / 2.0)
}

struct Job {
    nonce: String,
    packet: String,
    request: RequestId,
    events: Events,
    done: oneshot::Sender<Result<(), String>>,
}

enum Command {
    /// Answered once the socket is open and authenticated, or with why it is not.
    Ready(oneshot::Sender<Result<(), String>>),
    Send(Job),
}

/// One route's socket, as its requests see it.
#[derive(Clone)]
pub(crate) struct HowConnection {
    commands: mpsc::UnboundedSender<Command>,
}

impl HowConnection {
    /// Waits until the socket is open and authenticated, opening it if it is not.
    pub(crate) async fn ready(&self) -> Result<(), String> {
        let (answer, answered) = oneshot::channel();
        self.commands
            .send(Command::Ready(answer))
            .map_err(|_| "HTTP tunnel connection failed".to_owned())?;
        answered
            .await
            .unwrap_or_else(|_| Err("HTTP tunnel connection failed".into()))
    }

    /// Sends one `rpc.http` and reports its answer to `events`.
    pub(crate) async fn request(
        &self,
        request: RequestId,
        packet: String,
        events: Events,
    ) -> Result<(), String> {
        let (done, finished) = oneshot::channel();
        let job = Job {
            nonce: format!("native-{}", request.0),
            packet,
            request,
            events,
            done,
        };
        self.commands
            .send(Command::Send(job))
            .map_err(|_| "HTTP tunnel handoff failed".to_owned())?;
        finished
            .await
            .unwrap_or_else(|_| Err("HTTP tunnel closed".into()))
    }
}

/// An account's HOW sockets, one per tunnel URL. Dropping it closes them.
#[derive(Default)]
pub(crate) struct HowPool {
    connections: Mutex<HashMap<String, HowConnection>>,
}

impl HowPool {
    pub(crate) fn connection(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        tokens: ProxyTokenProvider,
    ) -> HowConnection {
        self.connection_with(url, headers, tokens, OpenCap::process(), BASE_BACKOFF)
    }

    fn connection_with(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        tokens: ProxyTokenProvider,
        cap: Arc<OpenCap>,
        base_backoff: Duration,
    ) -> HowConnection {
        let mut connections = self.connections.lock().unwrap();
        if let Some(connection) = connections.get(url)
            && !connection.commands.is_closed()
        {
            return connection.clone();
        }
        let (commands, received) = mpsc::unbounded_channel();
        let route = Route {
            url: url.to_owned(),
            headers,
            tokens,
            cap,
            base_backoff,
        };
        tokio::spawn(run(route, received));
        let connection = HowConnection { commands };
        connections.insert(url.to_owned(), connection.clone());
        connection
    }
}

struct Route {
    url: String,
    headers: Vec<(String, String)>,
    tokens: ProxyTokenProvider,
    cap: Arc<OpenCap>,
    base_backoff: Duration,
}

impl Route {
    /// The tunnel URL with a fresh `t`, as the browser's adapter dials it.
    fn dial_url(&self) -> Result<String, String> {
        let mut url = reqwest::Url::parse(&self.url).map_err(|_| "Invalid HTTP tunnel URL")?;
        let retained: Vec<_> = url
            .query_pairs()
            .filter(|(name, _)| name != "v" && name != "t")
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        url.set_query(None);
        url.query_pairs_mut()
            .extend_pairs(retained)
            .append_pair("v", "2")
            .append_pair("t", &format!("{:.0}", crate::now_ms()));
        Ok(url.into())
    }

    async fn open(&self) -> Result<Socket, String> {
        self.cap.acquire(&self.url).await;
        let url = self.dial_url()?;
        let opening = async {
            let mut socket = crate::transport::open_socket(&url, self.headers.clone()).await?;
            authenticate_how(&mut socket, &self.tokens).await?;
            Ok::<_, String>(socket)
        };
        match tokio::time::timeout(OPEN_TIMEOUT, opening).await {
            Ok(Ok(socket)) => Ok(socket),
            Ok(Err(error)) => {
                crate::transport::log_open_failure(&url, &error);
                Err(error)
            }
            Err(_) => {
                crate::transport::log_open_failure(&url, "timed out after 15s");
                Err("timed out".into())
            }
        }
    }
}

/// The next frame of an open socket; never resolves while there is none.
async fn next_frame(
    socket: &mut Option<Socket>,
) -> Option<Result<Message, tokio_tungstenite::tungstenite::Error>> {
    match socket {
        Some(socket) => socket.next().await,
        None => std::future::pending().await,
    }
}

/// After a drop: a long-lived socket reopens at once; a short-lived one backs off as a failure.
fn after_drop(opened_at: Instant, failures: &mut u32, next_open: &mut Instant, base: Duration) {
    if opened_at.elapsed() < SHORT_LIVED {
        *failures = failures.saturating_add(1);
        *next_open = Instant::now() + backoff(base, *failures);
    } else {
        *failures = 0;
    }
}

fn fail_all(in_flight: &mut HashMap<String, Job>, error: &str) {
    for (_, job) in in_flight.drain() {
        let _ = job.done.send(Err(error.to_owned()));
    }
}

async fn run(route: Route, mut commands: mpsc::UnboundedReceiver<Command>) {
    let mut socket: Option<Socket> = None;
    let mut opened_at = Instant::now();
    let mut waiting: Vec<oneshot::Sender<Result<(), String>>> = Vec::new();
    let mut in_flight: HashMap<String, Job> = HashMap::new();
    let mut failures = 0u32;
    let mut next_open = Instant::now();
    let mut prune = tokio::time::interval(Duration::from_secs(5));
    loop {
        if socket.is_none() && !waiting.is_empty() {
            // The backoff and the open both end early when the account is gone:
            // a disposed host must not go on to send its token.
            let (at, opener) = (next_open, &route);
            let opening = async move {
                tokio::time::sleep_until(at).await;
                opener.open().await
            };
            tokio::pin!(opening);
            let opened = loop {
                tokio::select! {
                    opened = &mut opening => break opened,
                    command = commands.recv() => match command {
                        None => return,
                        Some(Command::Ready(waiter)) => waiting.push(waiter),
                        Some(Command::Send(job)) => {
                            let _ = job.done.send(Err("HTTP tunnel handoff failed".into()));
                        }
                    },
                }
            };
            match opened {
                Ok(opened) => {
                    socket = Some(opened);
                    opened_at = Instant::now();
                    for waiter in waiting.drain(..) {
                        let _ = waiter.send(Ok(()));
                    }
                }
                Err(error) => {
                    failures = failures.saturating_add(1);
                    next_open = Instant::now() + backoff(route.base_backoff, failures);
                    let error = format!("HTTP tunnel connection failed: {error}");
                    for waiter in waiting.drain(..) {
                        let _ = waiter.send(Err(error.clone()));
                    }
                }
            }
            continue;
        }
        tokio::select! {
            command = commands.recv() => match command {
                None => break,
                Some(Command::Ready(waiter)) => {
                    if socket.is_some() {
                        let _ = waiter.send(Ok(()));
                    } else {
                        waiting.push(waiter);
                    }
                }
                Some(Command::Send(job)) => {
                    let Some(open) = socket.as_mut() else {
                        let _ = job.done.send(Err("HTTP tunnel handoff failed".into()));
                        continue;
                    };
                    let sent = tokio::time::timeout(WRITE_TIMEOUT, open.send(Message::text(job.packet.clone()))).await;
                    match sent {
                        Ok(Ok(())) => {
                            in_flight.insert(job.nonce.clone(), job);
                        }
                        Ok(Err(_)) | Err(_) => {
                            let _ = job.done.send(Err("HTTP tunnel handoff failed".into()));
                            socket = None;
                            after_drop(opened_at, &mut failures, &mut next_open, route.base_backoff);
                            fail_all(&mut in_flight, "HTTP tunnel closed");
                        }
                    }
                }
            },
            frame = next_frame(&mut socket) => {
                let message = match frame {
                    Some(Ok(Message::Text(text))) => serde_json::from_str::<TunnelMessage>(&text)
                        .map(|message| (message, None))
                        .map_err(|error| error.to_string()),
                    Some(Ok(Message::Binary(bytes))) => decode_binary(&bytes),
                    Some(Ok(Message::Ping(_))) => {
                        if let Some(open) = socket.as_mut()
                            && !matches!(tokio::time::timeout(WRITE_TIMEOUT, open.flush()).await, Ok(Ok(())))
                        {
                            socket = None;
                            after_drop(opened_at, &mut failures, &mut next_open, route.base_backoff);
                            fail_all(&mut in_flight, "HTTP tunnel closed");
                        }
                        continue;
                    }
                    Some(Ok(Message::Close(_))) | None => Err("tunnel closed".into()),
                    Some(Err(error)) => Err(error.to_string()),
                    Some(Ok(_)) => continue,
                };
                match message {
                    Ok((message, binary)) => {
                        if message.kind == "heartbeat" {
                            if let Some(open) = socket.as_mut()
                                && !matches!(tokio::time::timeout(WRITE_TIMEOUT, ping(open)).await, Ok(Ok(())))
                            {
                                socket = None;
                                after_drop(opened_at, &mut failures, &mut next_open, route.base_backoff);
                                fail_all(&mut in_flight, "HTTP tunnel closed");
                            }
                            continue;
                        }
                        if message.kind != "rpc.http" {
                            continue;
                        }
                        let Some(job) = message.nonce.as_ref().and_then(|nonce| in_flight.remove(nonce)) else {
                            continue;
                        };
                        let result = http_response(message, binary).map(|(status, headers, body)| {
                            report_response(job.request, status, headers, &body, &job.events);
                        });
                        let _ = job.done.send(result);
                    }
                    Err(error) => {
                        // A malformed frame is the socket's fault as much as a drop: start over.
                        if let Some(mut open) = socket.take() {
                            let _ = tokio::time::timeout(WRITE_TIMEOUT, open.close(None)).await;
                        }
                        after_drop(opened_at, &mut failures, &mut next_open, route.base_backoff);
                        fail_all(&mut in_flight, &format!("HTTP tunnel closed: {error}"));
                    }
                }
            }
            _ = prune.tick() => {
                // A request past its own deadline gave up; its answer, if any, goes nowhere.
                in_flight.retain(|_, job| !job.done.is_closed());
            }
        }
    }
    if let Some(mut open) = socket {
        let _ = tokio::time::timeout(WRITE_TIMEOUT, open.close(None)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HostEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::net::TcpListener;

    fn tokens() -> ProxyTokenProvider {
        Arc::new(|| Box::pin(async { Ok("token".to_owned()) }))
    }

    fn answer(nonce: &str, body: &str) -> String {
        serde_json::json!({"type": "rpc.http", "nonce": nonce,
            "data": {"status": 200, "headers": {}, "data_raw": body}})
        .to_string()
    }

    /// A tunnel that greets, accepts the token, and hands each socket to `serve`.
    async fn tunnel<F, Fut>(serve: F) -> (String, Arc<AtomicUsize>)
    where
        F: Fn(tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/tunnel", listener.local_addr().unwrap());
        let opened = Arc::new(AtomicUsize::new(0));
        let counted = opened.clone();
        let serve = Arc::new(serve);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                counted.fetch_add(1, Ordering::SeqCst);
                let serve = serve.clone();
                tokio::spawn(async move {
                    let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                        return;
                    };
                    let greeting = serde_json::json!({"type": "serverTime"}).to_string();
                    let _ = socket.send(Message::text(greeting)).await;
                    while let Some(Ok(Message::Text(text))) = socket.next().await {
                        if text.contains("rpc.auth") {
                            let _ = socket.send(Message::text(r#"{"type":"rpc.auth"}"#)).await;
                            break;
                        }
                    }
                    serve(socket).await;
                });
            }
        });
        (url, opened)
    }

    #[tokio::test]
    async fn requests_share_one_socket_and_answers_may_come_out_of_order() {
        let (url, opened) = tunnel(|mut socket| async move {
            let mut nonces = Vec::new();
            while nonces.len() < 2 {
                let Some(Ok(Message::Text(text))) = socket.next().await else {
                    return;
                };
                let packet: serde_json::Value = serde_json::from_str(&text).unwrap();
                nonces.push(packet["nonce"].as_str().unwrap().to_owned());
            }
            for nonce in nonces.iter().rev() {
                let _ = socket
                    .send(Message::text(answer(nonce, &format!("body {nonce}"))))
                    .await;
            }
            while socket.next().await.is_some() {}
        })
        .await;
        let pool = HowPool::default();
        let connection = pool.connection_with(
            &url,
            vec![],
            tokens(),
            Arc::new(OpenCap::new(10.0, 10.0)),
            BASE_BACKOFF,
        );
        connection.ready().await.unwrap();
        let (events, mut received) = mpsc::unbounded_channel();
        let packet = |id: u64| {
            serde_json::json!({"type": "rpc.http", "nonce": format!("native-{id}")}).to_string()
        };
        let (first, second) = tokio::join!(
            connection.request(RequestId(1), packet(1), events.clone()),
            connection.request(RequestId(2), packet(2), events.clone()),
        );
        assert_eq!((first, second), (Ok(()), Ok(())));
        let mut bodies = HashMap::new();
        while let Ok(event) = received.try_recv() {
            if let HostEvent::HttpChunk(id, chunk) = event {
                bodies.entry(id.0).or_insert_with(Vec::new).extend(chunk);
            }
        }
        assert_eq!(
            String::from_utf8(bodies[&1].clone()).unwrap(),
            "body native-1"
        );
        assert_eq!(
            String::from_utf8(bodies[&2].clone()).unwrap(),
            "body native-2"
        );
        assert_eq!(
            opened.load(Ordering::SeqCst),
            1,
            "one socket for both requests"
        );
        // The pool hands out that same socket for the route.
        let again = pool.connection(&url, vec![], tokens());
        again.ready().await.unwrap();
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_drop_mid_flight_fails_the_request_and_the_next_one_reopens() {
        let (url, opened) = tunnel(|mut socket| async move {
            // Takes the request, then drops without answering.
            let _ = socket.next().await;
            let _ = socket.close(None).await;
        })
        .await;
        let pool = HowPool::default();
        let connection = pool.connection_with(
            &url,
            vec![],
            tokens(),
            Arc::new(OpenCap::new(10.0, 10.0)),
            BASE_BACKOFF,
        );
        connection.ready().await.unwrap();
        let (events, _received) = mpsc::unbounded_channel();
        let packet = serde_json::json!({"type": "rpc.http", "nonce": "native-1"}).to_string();
        let failed = connection.request(RequestId(1), packet, events).await;
        assert!(failed.unwrap_err().starts_with("HTTP tunnel closed"));
        connection.ready().await.unwrap();
        assert_eq!(
            opened.load(Ordering::SeqCst),
            2,
            "reopened for the next request"
        );
    }

    #[tokio::test]
    async fn a_refused_open_backs_off_before_the_next() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/tunnel", listener.local_addr().unwrap());
        let attempts = Arc::new(Mutex::new(Vec::new()));
        let seen = attempts.clone();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            while let Ok((mut stream, _)) = listener.accept().await {
                seen.lock().unwrap().push(Instant::now());
                let mut request = [0u8; 2048];
                let _ = stream.read(&mut request).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 429 Too Many Requests\r\ncontent-length: 0\r\n\r\n")
                    .await;
            }
        });
        let pool = HowPool::default();
        let base = Duration::from_millis(400);
        let connection = pool.connection_with(
            &url,
            vec![],
            tokens(),
            Arc::new(OpenCap::new(10.0, 10.0)),
            base,
        );
        let first = connection.ready().await.unwrap_err();
        assert!(
            first.contains("HTTP tunnel connection failed") && first.contains("429"),
            "{first}"
        );
        let second = connection.ready().await;
        assert!(second.is_err());
        let attempts = attempts.lock().unwrap().clone();
        assert_eq!(attempts.len(), 2);
        assert!(
            attempts[1].duration_since(attempts[0]) >= base / 2,
            "waited {:?}",
            attempts[1].duration_since(attempts[0])
        );
    }

    #[tokio::test]
    async fn a_stalled_write_fails_its_request_as_a_drop_instead_of_wedging_the_route() {
        let (url, opened) = tunnel(|socket| async move {
            // Authenticated, then never reads again: the request's write backs up.
            tokio::time::sleep(Duration::from_secs(30)).await;
            drop(socket);
        })
        .await;
        let pool = HowPool::default();
        let connection = pool.connection_with(
            &url,
            vec![],
            tokens(),
            Arc::new(OpenCap::new(10.0, 10.0)),
            BASE_BACKOFF,
        );
        connection.ready().await.unwrap();
        let (events, _received) = mpsc::unbounded_channel();
        let huge = "x".repeat(64 * 1024 * 1024);
        let packet =
            serde_json::json!({"type": "rpc.http", "nonce": "native-1", "pad": huge}).to_string();
        let started = std::time::Instant::now();
        let failed = tokio::time::timeout(
            Duration::from_secs(15),
            connection.request(RequestId(1), packet, events),
        )
        .await
        .expect("a stalled write must not wedge the request");
        assert!(failed.unwrap_err().starts_with("HTTP tunnel"));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(opened.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_open_cap_spends_its_burst_then_paces_opens() {
        let cap = OpenCap::new(2.0, 4.0);
        let start = Instant::now();
        assert_eq!(cap.take(start), Duration::ZERO);
        assert_eq!(cap.take(start), Duration::ZERO);
        assert_eq!(cap.take(start), Duration::from_millis(250));
        assert_eq!(cap.take(start + Duration::from_millis(250)), Duration::ZERO);
    }

    #[test]
    fn backoff_doubles_to_its_ceiling_with_jitter() {
        for failures in 1..20 {
            let wait = backoff(BASE_BACKOFF, failures);
            let full = BASE_BACKOFF
                .saturating_mul(1 << (failures - 1).min(16))
                .min(MAX_BACKOFF);
            assert!(wait >= full / 2 && wait <= full, "{failures}: {wait:?}");
        }
    }
}
