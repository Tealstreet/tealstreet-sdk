//! Account-scoped binary frame streams. Account engines are provisioned by the
//! embedding application; clients can neither create accounts nor set credentials.
use crate::{Host, Transport};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::command::HostInput;
use safe_cex_engine::{Command, Event, LogLevel, Notice, Venue};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::tungstenite::http::StatusCode;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

/// Limits apply independently to every account except `max_connections`.
#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub flush_every: Duration,
    pub client_queue_capacity: usize,
    pub max_clients_per_account: usize,
    pub max_connections: usize,
    pub max_subscriptions_per_client: usize,
    pub max_subscriptions_per_account: usize,
    pub max_pending_per_client: usize,
    pub max_pending_per_account: usize,
    pub max_message_bytes: usize,
    pub max_frame_bytes: usize,
    pub io_timeout: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            flush_every: Duration::from_millis(60),
            client_queue_capacity: 32,
            max_clients_per_account: 128,
            max_connections: 256,
            max_subscriptions_per_client: 256,
            max_subscriptions_per_account: 4096,
            max_pending_per_client: 128,
            max_pending_per_account: 4096,
            max_message_bytes: 64 * 1024,
            max_frame_bytes: 16 * 1024 * 1024,
            io_timeout: Duration::from_secs(5),
        }
    }
}

impl ServerConfig {
    fn validate(&self) -> io::Result<()> {
        if self.flush_every.is_zero()
            || self.io_timeout.is_zero()
            || self.client_queue_capacity == 0
            || self.max_clients_per_account == 0
            || self.max_connections == 0
            || self.max_subscriptions_per_client == 0
            || self.max_subscriptions_per_account == 0
            || self.max_pending_per_client == 0
            || self.max_pending_per_account == 0
            || self.max_message_bytes == 0
            || self.max_frame_bytes == 0
        {
            return Err(invalid("server limits must be nonzero"));
        }
        Ok(())
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[derive(Clone)]
struct AccountAccess {
    token: Arc<str>,
    commands: mpsc::Sender<Control>,
}

/// Call `add_account` and `serve` inside a tokio `LocalSet`. Each engine remains
/// single-threaded, including venues whose pending jobs are not `Send`.
pub struct FrameServer {
    config: ServerConfig,
    accounts: HashMap<String, AccountAccess>,
    tasks: Vec<JoinHandle<()>>,
    shutdown: watch::Sender<bool>,
}

impl FrameServer {
    pub fn new(config: ServerConfig) -> io::Result<Self> {
        config.validate()?;
        Ok(Self {
            config,
            accounts: HashMap::new(),
            tasks: Vec::new(),
            shutdown: watch::channel(false).0,
        })
    }

    /// Tokens grant full access to one preconfigured account. They must be
    /// distinct, nonempty opaque strings without whitespace. Never put them in URLs.
    /// Engines must not have received commands; startup and Store seeding are allowed.
    pub fn add_account<V: Venue + 'static, T: Transport + 'static>(
        &mut self,
        id: &str,
        token: &str,
        host: Host<V, T>,
    ) -> io::Result<()> {
        if host.engine.has_received_commands() {
            return Err(invalid(
                "account engine must not receive commands before server registration",
            ));
        }
        if id.is_empty()
            || id.len() > 128
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.accounts.contains_key(id)
        {
            return Err(invalid("invalid or duplicate account ID"));
        }
        if token.is_empty()
            || !token.bytes().all(protocol_token)
            || self
                .accounts
                .values()
                .any(|account| account.token.as_ref() == token)
        {
            return Err(invalid("invalid or duplicate account token"));
        }
        let (commands, inbox) = mpsc::channel(256);
        let actor = Account::new(host, self.config.clone());
        self.tasks.push(tokio::task::spawn_local(
            actor.run(inbox, self.shutdown.subscribe()),
        ));
        self.accounts.insert(
            id.to_owned(),
            AccountAccess {
                token: token.into(),
                commands,
            },
        );
        Ok(())
    }

    /// Stop accepting, close clients, then join all account actors. TLS and
    /// application session authorization belong to the embedding deployment.
    pub async fn serve(
        mut self,
        listener: TcpListener,
        shutdown: impl Future<Output = ()>,
    ) -> io::Result<()> {
        let accounts = Arc::new(self.accounts.clone());
        let mut connections = JoinSet::new();
        let mut next_client = 0u64;
        tokio::pin!(shutdown);
        let result = loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => break Ok(()),
                Some(_) = connections.join_next(), if !connections.is_empty() => {},
                incoming = listener.accept() => match incoming {
                    Ok((stream, _)) => {
                        if connections.len() >= self.config.max_connections { drop(stream); continue; }
                        let Some(client) = next_client.checked_add(1) else { break Err(io::Error::other("client IDs exhausted")); };
                        next_client = client;
                        connections.spawn_local(connection(stream, client, accounts.clone(), self.config.clone(), self.shutdown.subscribe()));
                    }
                    Err(error) => break Err(error),
                }
            }
        };
        self.shutdown.send_replace(true);
        while connections.join_next().await.is_some() {}
        for task in self.tasks.drain(..) {
            let _ = task.await;
        }
        result
    }
}

impl Drop for FrameServer {
    fn drop(&mut self) {
        self.shutdown.send_replace(true);
        // Dropping a serving future must not leave network/engine tasks alive.
        for task in &self.tasks {
            task.abort();
        }
    }
}

#[derive(Clone)]
enum Outgoing {
    Frame(Arc<[u8]>),
    Text(Arc<str>),
}
impl Outgoing {
    fn len(&self) -> usize {
        match self {
            Self::Frame(bytes) => bytes.len(),
            Self::Text(text) => text.len(),
        }
    }

    fn message(self) -> Message {
        match self {
            Self::Frame(bytes) => Message::binary(bytes.to_vec()),
            Self::Text(text) => Message::text(text.to_string()),
        }
    }
}

enum Control {
    Join {
        client: u64,
        outgoing: mpsc::Sender<Outgoing>,
        cancel: watch::Sender<bool>,
        ready: oneshot::Sender<Result<(), &'static str>>,
    },
    Command {
        client: u64,
        command: Command,
    },
    TickIntervalChanged {
        client: u64,
        interval_ms: f64,
    },
    Leave(u64),
}

struct Peer {
    outgoing: mpsc::Sender<Outgoing>,
    cancel: watch::Sender<bool>,
    books: HashSet<String>,
    candles: HashSet<(String, String)>,
    tickers: Option<HashSet<String>>,
}
struct Pending {
    client: Option<u64>,
    original: u32,
}
struct Account<V: Venue, T: Transport> {
    host: Host<V, T>,
    config: ServerConfig,
    peers: HashMap<u64, Peer>,
    books: HashMap<String, usize>,
    candles: HashMap<(String, String), usize>,
    pending: HashMap<u32, Pending>,
    next_command: u32,
    control_now: Option<f64>,
}

impl<V: Venue, T: Transport> Account<V, T> {
    fn new(mut host: Host<V, T>, config: ServerConfig) -> Self {
        host.engine.set_book_deltas(true);
        Self {
            host,
            config,
            peers: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            pending: HashMap::new(),
            next_command: 0,
            control_now: None,
        }
    }

    async fn run(
        mut self,
        mut inbox: mpsc::Receiver<Control>,
        mut shutdown: watch::Receiver<bool>,
    ) {
        let started_at = (self.host.clock)();
        self.host.last_dispatched_at = started_at;
        self.host.engine.start(started_at);
        self.execute();
        let mut flush = tokio::time::interval(self.config.flush_every);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                control = inbox.recv() => {
                    let selected_at = (self.host.clock)();
                    self.drain_ready_prefix().await;
                    self.control_now = Some(selected_at);
                    match control {
                    Some(Control::Join { client, outgoing, cancel, ready }) => {
                        if self.peers.len() >= self.config.max_clients_per_account {
                            let _ = ready.send(Err("account client limit reached"));
                            continue;
                        }
                        // Snapshot and registration share this actor turn. Existing peers
                        // establish the same baseline before a new peer takes its snapshot.
                        self.flush();
                        let snapshot: Arc<[u8]> = self.host.engine.snapshot_frame(selected_at).into();
                        if snapshot.len() > self.config.max_frame_bytes || outgoing.try_send(Outgoing::Frame(snapshot)).is_err() {
                            let _ = ready.send(Err("snapshot exceeds connection limits"));
                            continue;
                        }
                        self.peers.insert(client, Peer { outgoing, cancel, books: HashSet::new(), candles: HashSet::new(), tickers: None });
                        let _ = ready.send(Ok(()));
                    }
                    Some(Control::Command { client, command }) => self.command(client, command),
                    Some(Control::TickIntervalChanged { client, interval_ms }) => {
                        if self.peers.contains_key(&client) {
                            self.host.engine.handle(selected_at, Event::TickIntervalChanged { interval_ms });
                        }
                    }
                    Some(Control::Leave(client)) => self.leave(client),
                    None => break,
                    }
                },
                event = async {
                    if self.host.pending.is_some() { None } else { self.host.inbox.recv().await }
                } => {
                    self.control_now = None;
                    if let Some(event) = self.host.pending.take().or(event) {
                        self.host.dispatch_ready(event, crate::MAX_BATCH_MESSAGES);
                    }
                },
                _ = flush.tick() => {
                    let selected_at = (self.host.clock)();
                    self.drain_ready_prefix().await;
                    self.control_now = Some(selected_at);
                    let gone: Vec<_> = self.peers.iter().filter(|(_, peer)| peer.outgoing.is_closed()).map(|(id, _)| *id).collect();
                    for client in gone { self.leave(client); }
                    self.flush();
                }
            }
            self.execute();
            tokio::task::yield_now().await;
        }
        for (_, peer) in self.peers.drain() {
            peer.cancel.send_replace(true);
        }
        self.host.stop();
    }

    fn now(&self) -> f64 {
        self.control_now.unwrap_or(self.host.last_dispatched_at)
    }

    async fn drain_ready_prefix(&mut self) {
        // Controls and flushes share the finite prefix ready when selected.
        // Action-triggered arrivals stay behind that boundary without starving it.
        self.control_now = None;
        let mut remaining = self.host.inbox.len() + usize::from(self.host.pending.is_some());
        while remaining > 0 {
            let event = self.host.pending.take().unwrap_or_else(|| {
                self.host
                    .inbox
                    .try_recv()
                    .expect("the captured account prefix is still queued")
            });
            remaining -= self
                .host
                .dispatch_ready(event, remaining.min(crate::MAX_BATCH_MESSAGES));
            self.execute();
            if remaining > 0 {
                tokio::task::yield_now().await;
            }
        }
    }

    fn execute(&mut self) {
        let mut notices = Vec::new();
        self.host.execute(&mut |notice| notices.push(notice));
        for notice in notices {
            match notice {
                Notice::CommandDone { command, result } => {
                    if let Some(pending) = self.pending.remove(&command)
                        && let Some(client) = pending.client
                    {
                        let text = encode_notice(ServerNotice::Done {
                            command: pending.original,
                            result: result.as_deref().ok(),
                            error: result.as_ref().err().map(String::as_str),
                        });
                        self.send(client, Outgoing::Text(text));
                    }
                }
                Notice::Deactivate => {
                    self.broadcast(Outgoing::Text(encode_notice(ServerNotice::Deactivate)))
                }
                Notice::Log { level, message } => {
                    self.broadcast(Outgoing::Text(encode_notice(ServerNotice::Log {
                        level: match level {
                            LogLevel::Info => "info",
                            LogLevel::Warning => "warning",
                            LogLevel::Error => "error",
                        },
                        message: &message,
                    })))
                }
            }
        }
    }

    fn flush(&mut self) {
        let now = self.now();
        if let Some(frame) = self.host.engine.take_frame(now) {
            let frame: Arc<[u8]> = frame.into();
            if frame.len() > self.config.max_frame_bytes {
                let clients: Vec<_> = self.peers.keys().copied().collect();
                for client in clients {
                    self.leave(client);
                }
            } else {
                self.broadcast(Outgoing::Frame(frame));
            }
        }
    }

    fn send(&mut self, client: u64, message: Outgoing) {
        if message.len() > self.config.max_frame_bytes {
            self.leave(client);
            return;
        }
        if let Some(peer) = self.peers.get(&client)
            && peer.outgoing.try_send(message).is_err()
        {
            self.leave(client);
        }
    }

    fn broadcast(&mut self, message: Outgoing) {
        if message.len() > self.config.max_frame_bytes {
            let clients: Vec<_> = self.peers.keys().copied().collect();
            for client in clients {
                self.leave(client);
            }
            return;
        }
        let gone: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(client, peer)| {
                peer.outgoing
                    .try_send(message.clone())
                    .err()
                    .map(|_| *client)
            })
            .collect();
        for client in gone {
            self.leave(client);
        }
    }

    fn leave(&mut self, client: u64) {
        let Some(peer) = self.peers.remove(&client) else {
            return;
        };
        peer.cancel.send_replace(true);
        for pending in self.pending.values_mut() {
            if pending.client == Some(client) {
                pending.client = None;
            }
        }
        for symbol in peer.books {
            if release(&mut self.books, &symbol) {
                self.handle(Command::UnsubscribeOrderBook { symbol });
            }
        }
        for (symbol, interval) in peer.candles {
            if release(&mut self.candles, &(symbol.clone(), interval.clone())) {
                self.handle(Command::UnsubscribeCandles { symbol, interval });
            }
        }
        if peer.tickers.is_some() {
            self.tickers();
        }
    }

    fn handle(&mut self, command: Command) {
        let now = self.now();
        self.host.engine.handle(now, Event::Command(command));
    }

    fn command(&mut self, client: u64, mut command: Command) {
        if !self.subscription_allowed(client, &command) {
            self.send(
                client,
                Outgoing::Text(encode_notice(ServerNotice::Error {
                    command: None,
                    error: "subscription limit exceeded or invalid symbol",
                })),
            );
            return;
        }
        let Some(peer) = self.peers.get_mut(&client) else {
            return;
        };
        match &command {
            Command::SubscribeOrderBook { symbol } => {
                if peer.books.insert(symbol.clone()) {
                    let count = self.books.entry(symbol.clone()).or_default();
                    *count += 1;
                    if *count == 1 {
                        self.handle(command);
                    } else {
                        self.host.engine.request_book_resync(symbol);
                    }
                } else {
                    self.host.engine.request_book_resync(symbol);
                }
                return;
            }
            Command::UnsubscribeOrderBook { symbol } => {
                if peer.books.remove(symbol) && release(&mut self.books, symbol) {
                    self.handle(command);
                }
                return;
            }
            Command::SubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if peer.candles.insert(key.clone()) {
                    let count = self.candles.entry(key).or_default();
                    *count += 1;
                    if *count == 1 {
                        self.handle(command);
                    }
                }
                return;
            }
            Command::UnsubscribeCandles { symbol, interval } => {
                let key = (symbol.clone(), interval.clone());
                if peer.candles.remove(&key) && release(&mut self.candles, &key) {
                    self.handle(command);
                }
                return;
            }
            Command::SetTickerSymbols { symbols } => {
                peer.tickers = Some(symbols.iter().cloned().collect());
                self.tickers();
                return;
            }
            _ => {}
        }
        let id = command_id(&mut command).expect("all non-subscription commands have an ID");
        let original = *id;
        let owned = self
            .pending
            .values()
            .filter(|pending| pending.client == Some(client));
        let duplicate = owned.clone().any(|pending| pending.original == original);
        if duplicate
            || owned.count() >= self.config.max_pending_per_client
            || self.pending.len() >= self.config.max_pending_per_account
        {
            self.send(
                client,
                Outgoing::Text(encode_notice(ServerNotice::Error {
                    command: Some(original),
                    error: if duplicate {
                        "command ID already pending"
                    } else {
                        "pending command limit reached"
                    },
                })),
            );
            return;
        }
        let Some(internal) = self.next_command.checked_add(1) else {
            self.send(
                client,
                Outgoing::Text(encode_notice(ServerNotice::Error {
                    command: Some(original),
                    error: "account command IDs exhausted",
                })),
            );
            return;
        };
        self.next_command = internal;
        *id = internal;
        self.pending.insert(
            internal,
            Pending {
                client: Some(client),
                original,
            },
        );
        self.handle(command);
    }

    fn subscription_allowed(&self, client: u64, command: &Command) -> bool {
        let Some(peer) = self.peers.get(&client) else {
            return false;
        };
        let mut per_client =
            peer.books.len() + peer.candles.len() + peer.tickers.as_ref().map_or(0, HashSet::len);
        let mut per_account = self.books.len() + self.candles.len();
        let mut tickers = HashSet::<&str>::new();
        for (id, other) in &self.peers {
            if *id != client
                && let Some(symbols) = &other.tickers
            {
                tickers.extend(symbols.iter().map(String::as_str));
            }
        }
        let wanted = match command {
            Command::SetTickerSymbols { symbols } => {
                if symbols.iter().any(|symbol| !valid_symbol(symbol)) {
                    return false;
                }
                let unique: HashSet<_> = symbols.iter().map(String::as_str).collect();
                per_client =
                    per_client - peer.tickers.as_ref().map_or(0, HashSet::len) + unique.len();
                tickers.extend(unique);
                None
            }
            _ => peer.tickers.as_ref(),
        };
        if let Some(wanted) = wanted {
            tickers.extend(wanted.iter().map(String::as_str));
        }
        per_account += tickers.len();
        match command {
            Command::SubscribeOrderBook { symbol } => {
                if !valid_symbol(symbol) {
                    return false;
                }
                per_client += usize::from(!peer.books.contains(symbol));
                per_account += usize::from(!self.books.contains_key(symbol));
            }
            Command::SubscribeCandles { symbol, interval } => {
                if !valid_symbol(symbol) || interval.is_empty() || interval.len() > 64 {
                    return false;
                }
                let key = (symbol.clone(), interval.clone());
                per_client += usize::from(!peer.candles.contains(&key));
                per_account += usize::from(!self.candles.contains_key(&key));
            }
            Command::SetTickerSymbols { .. } => {}
            _ => return true,
        }
        per_client <= self.config.max_subscriptions_per_client
            && per_account <= self.config.max_subscriptions_per_account
    }

    fn tickers(&mut self) {
        let mut symbols = HashSet::new();
        for peer in self.peers.values() {
            if let Some(wanted) = &peer.tickers {
                if wanted.is_empty() {
                    symbols.clear();
                    break;
                }
                symbols.extend(wanted.iter().cloned());
            }
        }
        let mut symbols: Vec<_> = symbols.into_iter().collect();
        symbols.sort();
        self.handle(Command::SetTickerSymbols { symbols });
    }
}

impl<V: Venue, T: Transport> Drop for Account<V, T> {
    fn drop(&mut self) {
        self.host.stop();
    }
}

fn release<K: std::hash::Hash + Eq>(counts: &mut HashMap<K, usize>, key: &K) -> bool {
    let Some(count) = counts.get_mut(key) else {
        return false;
    };
    *count -= 1;
    if *count == 0 {
        counts.remove(key);
        true
    } else {
        false
    }
}

fn command_id(command: &mut Command) -> Option<&mut u32> {
    match command {
        Command::FetchOptionChain { command, .. }
        | Command::FetchOptionExpiries { command, .. }
        | Command::FetchAccountGreeks { command, .. }
        | Command::PlaceOrders { command, .. }
        | Command::CancelOrders { command, .. }
        | Command::CancelOrdersWithSymbols { command, .. }
        | Command::AmendOrder { command, .. }
        | Command::AmendOrderWithSymbol { command, .. }
        | Command::CancelSymbolOrders { command, .. }
        | Command::SetLeverage { command, .. }
        | Command::SetMarginType { command, .. }
        | Command::SetPositionMode { command, .. }
        | Command::FetchStpMode { command }
        | Command::FetchAccount { command }
        | Command::FetchOrderBook { command, .. }
        | Command::SetStpMode { command, .. }
        | Command::FetchLeverage { command, .. }
        | Command::Query { command, .. }
        | Command::FetchCandles { command, .. } => Some(command),
        Command::SetTickerSymbols { .. }
        | Command::SubscribeOrderBook { .. }
        | Command::UnsubscribeOrderBook { .. }
        | Command::SubscribeCandles { .. }
        | Command::UnsubscribeCandles { .. } => None,
    }
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerNotice<'a> {
    Done {
        command: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<&'a str>,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<u32>,
        error: &'a str,
    },
    Log {
        level: &'a str,
        message: &'a str,
    },
    Deactivate,
}
fn encode_notice(notice: ServerNotice<'_>) -> Arc<str> {
    serde_json::to_string(&notice)
        .expect("notice contains only strings and integers")
        .into()
}

async fn connection(
    stream: TcpStream,
    client: u64,
    accounts: Arc<HashMap<String, AccountAccess>>,
    config: ServerConfig,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut selected = None;
    // Tungstenite requires an unboxed HTTP error response in this callback.
    #[allow(clippy::result_large_err)]
    let callback = |request: &Request, mut response: Response| {
        let protocols = request
            .headers()
            .get("sec-websocket-protocol")
            .and_then(|header| header.to_str().ok())
            .unwrap_or("");
        let version = protocols
            .split(',')
            .any(|protocol| protocol.trim() == "safe-cex-frame.v1");
        let mut bearer = protocols
            .split(',')
            .filter_map(|protocol| protocol.trim().strip_prefix("bearer."));
        let browser_token = bearer.next();
        let malformed = bearer.next().is_some() || (browser_token.is_some() && !version);
        let authorized = !malformed
            && request.uri().query().is_none()
            && request
                .uri()
                .path()
                .strip_prefix("/accounts/")
                .and_then(|id| accounts.get(id))
                .is_some_and(|account| {
                    let authorization =
                        request.headers().get("authorization").map(|h| h.as_bytes());
                    let valid = if let Some(token) = browser_token {
                        equal_token(token.as_bytes(), account.token.as_bytes())
                            && authorization.is_none_or(|header| {
                                equal_token(header, format!("Bearer {}", account.token).as_bytes())
                            })
                    } else {
                        authorization.is_some_and(|header| {
                            equal_token(header, format!("Bearer {}", account.token).as_bytes())
                        })
                    };
                    if valid {
                        selected = Some(account.clone());
                    }
                    valid
                });
        if authorized {
            if version {
                response.headers_mut().insert(
                    "sec-websocket-protocol",
                    "safe-cex-frame.v1".parse().unwrap(),
                );
            }
            Ok(response)
        } else {
            let mut response = ErrorResponse::new(Some("unauthorized".to_owned()));
            *response.status_mut() = StatusCode::UNAUTHORIZED;
            Err(response)
        }
    };
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(config.max_message_bytes))
        .max_frame_size(Some(config.max_message_bytes));
    let mut socket = tokio::select! {
        biased;
        _ = shutdown.changed() => return,
        result = tokio::time::timeout(config.io_timeout, tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(ws_config))) => match result { Ok(Ok(socket)) => socket, _ => return },
    };
    let Some(account) = selected else {
        return;
    };
    let (outgoing, mut frames) = mpsc::channel(config.client_queue_capacity);
    let (cancel, mut canceled) = watch::channel(false);
    let (ready, joined) = oneshot::channel();
    let join = Control::Join {
        client,
        outgoing,
        cancel,
        ready,
    };
    let joined = async {
        account.commands.send(join).await.map_err(|_| ())?;
        joined.await.map_err(|_| ())?.map_err(|_| ())
    };
    let accepted = tokio::select! {
        biased;
        _ = shutdown.changed() => false,
        result = tokio::time::timeout(config.io_timeout, joined) => matches!(result, Ok(Ok(()))),
    };
    if accepted {
        loop {
            let message = tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = canceled.changed() => break,
                message = frames.recv() => match message { Some(message) => message.message(), None => break },
                incoming = socket.next() => match incoming {
                    Some(Ok(Message::Text(text))) => match safe_cex_engine::command::decode_host_input(&text) {
                        Ok(HostInput::PublicFeedUpdate(_)) => Message::text(encode_notice(ServerNotice::Error {
                            command: None, error: "public feed updates require a trusted host provider",
                        }).to_string()),
                        Ok(input) => {
                            let control = match input {
                                HostInput::PublicFeedUpdate(_) => unreachable!("provider controls are rejected before command dispatch"),
                                HostInput::Command(command) => Control::Command { client, command },
                                HostInput::TickIntervalChanged { interval_ms } => Control::TickIntervalChanged { client, interval_ms },
                            };
                            if account.commands.try_send(control).is_err() { break; }
                            continue;
                        }
                        Err(_) => Message::text(encode_notice(ServerNotice::Error { command: None, error: "invalid command" }).to_string()),
                    },
                    Some(Ok(Message::Ping(_))) => {
                        let flushed = tokio::select! {
                            biased;
                            _ = shutdown.changed() => false,
                            _ = canceled.changed() => false,
                            result = tokio::time::timeout(config.io_timeout, socket.flush()) => matches!(result, Ok(Ok(()))),
                        };
                        if !flushed { break; }
                        continue;
                    }
                    Some(Ok(Message::Pong(_))) => continue,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => Message::text(encode_notice(ServerNotice::Error { command: None, error: "commands must be JSON text" }).to_string()),
                }
            };
            let sent = tokio::select! {
                biased;
                _ = shutdown.changed() => false,
                _ = canceled.changed() => false,
                result = tokio::time::timeout(config.io_timeout, socket.send(message)) => matches!(result, Ok(Ok(()))),
            };
            if !sent {
                break;
            }
        }
    }
    drop(frames);
    // A full control queue cannot leak subscriptions: the actor also prunes
    // closed outbound receivers at every flush.
    let _ = account.commands.try_send(Control::Leave(client));
    let _ = tokio::time::timeout(config.io_timeout, socket.close(None)).await;
}

fn equal_token(supplied: &[u8], expected: &[u8]) -> bool {
    if supplied.len() != expected.len() {
        return false;
    }
    supplied
        .iter()
        .zip(expected)
        .fold(0u8, |different, (a, b)| different | (a ^ b))
        == 0
}

fn protocol_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn valid_symbol(symbol: &str) -> bool {
    !symbol.is_empty() && symbol.len() <= 256
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Events, HostEvent};
    use safe_cex_engine::book::Book;
    use safe_cex_engine::{Cx, Engine, HttpRequest, Response, SocketId, TimerId};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Stub(Rc<RefCell<Vec<Command>>>, Rc<RefCell<Vec<(f64, String)>>>);
    impl Venue for Stub {
        type Pending = ();
        fn start(&mut self, _: &mut Cx<'_, ()>) {}
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
            self.1.borrow_mut().push((cx.now, text.into()));
            cx.store.set_balance(safe_cex_engine::model::Balance {
                total: cx.now,
                ..Default::default()
            });
        }
        fn on_binary(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, data: &[u8]) {
            self.1
                .borrow_mut()
                .push((cx.now, format!("binary:{data:?}")));
        }
        fn on_tick_interval(&mut self, cx: &mut Cx<'_, ()>, _: f64) {
            self.1.borrow_mut().push((cx.now, "tick".into()));
        }
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
            self.1.borrow_mut().push((cx.now, "command".into()));
            self.0.borrow_mut().push(command);
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    struct NoTransport;
    impl Transport for NoTransport {
        fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
            mpsc::unbounded_channel().0
        }
        fn request(&self, _: HttpRequest, _: Events) {}
    }

    #[tokio::test]
    async fn account_controls_and_snapshot_follow_queued_mixed_arrivals() {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut host = Host::new(Engine::new(Stub(Rc::default(), trace.clone())), NoTransport);
        host.clock = || 50.0;
        for (event, at) in [
            (HostEvent::WsMessage(SocketId(1), "first".into()), 11.0),
            (HostEvent::WsBinary(SocketId(1), vec![255]), 12.0),
            (HostEvent::TickIntervalChanged(f64::INFINITY), 13.0),
            (HostEvent::WsMessage(SocketId(1), "last".into()), 14.0),
        ] {
            host.event_sender().send_at(event, at).unwrap();
        }
        let actor = Account::new(host, ServerConfig::default());
        let (commands, inbox) = mpsc::channel(4);
        let (outgoing, mut frames) = mpsc::channel(8);
        let (cancel, _canceled) = watch::channel(false);
        let (ready, joined) = oneshot::channel();
        commands
            .send(Control::Join {
                client: 1,
                outgoing,
                cancel,
                ready,
            })
            .await
            .unwrap();
        commands
            .send(Control::Command {
                client: 1,
                command: Command::SetTickerSymbols {
                    symbols: vec!["BTCUSDT".into()],
                },
            })
            .await
            .unwrap();
        drop(commands);
        let (_shutdown, shutdown) = watch::channel(false);
        actor.run(inbox, shutdown).await;
        assert_eq!(joined.await.unwrap(), Ok(()));
        assert_eq!(
            *trace.borrow(),
            [
                (11.0, "first".into()),
                (12.0, "binary:[255]".into()),
                (13.0, "tick".into()),
                (14.0, "last".into()),
                (50.0, "command".into()),
            ]
        );
        let Some(Outgoing::Frame(frame)) = frames.recv().await else {
            panic!("first account message must be its snapshot");
        };
        assert_eq!(f64::from_le_bytes(frame[8..16].try_into().unwrap()), 50.0);
        let balance = safe_cex_engine::wire::sections(&frame)
            .find(|(kind, _)| *kind == safe_cex_engine::wire::section::BALANCE)
            .unwrap()
            .1;
        assert_eq!(
            f64::from_le_bytes(balance[16..24].try_into().unwrap()),
            14.0
        );
    }

    #[tokio::test]
    async fn full_peer_queue_cancels_and_releases_interest_without_dropping_a_delta() {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let mut actor = Account::new(
            Host::new(
                Engine::new(Stub(commands.clone(), Rc::default())),
                NoTransport,
            ),
            ServerConfig::default(),
        );
        let (outgoing, mut frames) = mpsc::channel(1);
        let (cancel, canceled) = watch::channel(false);
        actor.peers.insert(
            1,
            Peer {
                outgoing,
                cancel,
                books: HashSet::new(),
                candles: HashSet::new(),
                tickers: None,
            },
        );
        actor.command(
            1,
            Command::SubscribeOrderBook {
                symbol: "BTCUSDT".into(),
            },
        );
        actor.broadcast(Outgoing::Frame(vec![1].into()));
        actor.broadcast(Outgoing::Frame(vec![2].into()));
        assert!(
            *canceled.borrow(),
            "slow peer must reconnect and resnapshot"
        );
        assert!(actor.peers.is_empty() && actor.books.is_empty());
        assert!(
            matches!(frames.recv().await, Some(Outgoing::Frame(frame)) if frame.as_ref() == [1])
        );
        assert!(
            frames.recv().await.is_none(),
            "never resume with a skipped delta"
        );
        assert!(matches!(
            commands.borrow().last(),
            Some(Command::UnsubscribeOrderBook { .. })
        ));
    }

    #[tokio::test]
    async fn dropping_account_aborts_timers_and_closes_transport_input() {
        let mut actor = Account::new(
            Host::new(Engine::new(Stub(Rc::default(), Rc::default())), NoTransport),
            ServerConfig::default(),
        );
        let timer = tokio::spawn(std::future::pending::<()>());
        actor.host.timers.insert(TimerId(1), timer.abort_handle());
        let events = actor.host.events.clone();
        drop(actor);
        assert!(events.send(HostEvent::Timer(TimerId(1))).is_err());
        assert!(timer.await.unwrap_err().is_cancelled());
    }

    #[tokio::test]
    async fn invalid_configuration_is_rejected_before_starting_accounts() {
        tokio::task::LocalSet::new()
            .run_until(async {
                assert!(
                    FrameServer::new(ServerConfig {
                        max_subscriptions_per_client: 0,
                        ..ServerConfig::default()
                    })
                    .is_err()
                );
                let mut server = FrameServer::new(ServerConfig::default()).unwrap();
                let host =
                    || Host::new(Engine::new(Stub(Rc::default(), Rc::default())), NoTransport);
                for (id, token) in [
                    ("", "valid"),
                    ("../bad", "valid"),
                    ("a", ""),
                    ("a", "space token"),
                    ("a", "unsafe,token"),
                ] {
                    assert!(server.add_account(id, token, host()).is_err());
                }
                server.add_account("a", "token-a", host()).unwrap();
                assert!(server.add_account("a", "different", host()).is_err());
                assert!(server.add_account("b", "token-a", host()).is_err());
            })
            .await;
    }
}
