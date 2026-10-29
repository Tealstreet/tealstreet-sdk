//! Account-scoped binary frame streams. Account engines are provisioned by the
//! embedding application; clients can neither create accounts nor set credentials.
use crate::{Host, Transport};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::command::HostInput;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Command, Event, Level, Notice, Value, Venue, VenueError};
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

#[cfg(feature = "address-nonce")]
type AccountResult = io::Result<()>;
#[cfg(not(feature = "address-nonce"))]
type AccountResult = ();

/// Call `add_account` and `serve` inside a tokio `LocalSet`. Each engine remains
/// single-threaded, including venues whose pending jobs are not `Send`.
pub struct FrameServer {
    config: ServerConfig,
    accounts: HashMap<String, AccountAccess>,
    tasks: Vec<JoinHandle<AccountResult>>,
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

    /// Registers an account on `venue`, constructed from the same options JSON
    /// the wasm host accepts; construction rules reject before any I/O.
    #[cfg(feature = "venues")]
    pub fn add_venue<T: Transport + 'static>(
        &mut self,
        id: &str,
        token: &str,
        venue: &str,
        options_json: &str,
        transport: T,
    ) -> io::Result<()> {
        self.register_venue(id, token, venue, options_json, transport, None)
    }

    /// Trusted account setup with opaque credential refs in options. The provider
    /// is deployment-owned and never accepted from the client command wire.
    #[cfg(feature = "venues")]
    pub fn add_venue_with_signer<T: Transport + 'static>(
        &mut self,
        id: &str,
        token: &str,
        venue: &str,
        options_json: &str,
        transport: T,
        signer: safe_cex_engine::signing::Provider,
    ) -> io::Result<()> {
        self.register_venue(id, token, venue, options_json, transport, Some(signer))
    }

    #[cfg(feature = "venues")]
    fn register_venue<T: Transport + 'static>(
        &mut self,
        id: &str,
        token: &str,
        venue: &str,
        options_json: &str,
        transport: T,
        signer: Option<safe_cex_engine::signing::Provider>,
    ) -> io::Result<()> {
        struct Register<'a, T> {
            server: &'a mut FrameServer,
            id: &'a str,
            token: &'a str,
            transport: T,
        }
        impl<T: Transport + 'static> safe_cex_venues::Visit for Register<'_, T> {
            type Output = io::Result<()>;
            fn visit<V: Venue + 'static>(self, engine: safe_cex_engine::Engine<V>) -> Self::Output {
                self.server
                    .add_account(self.id, self.token, Host::new(engine, self.transport))
            }
        }
        let options = serde_json::from_str(options_json)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        let register = Register {
            server: self,
            id,
            token,
            transport,
        };
        let result = if let Some(signer) = signer {
            safe_cex_venues::build_with_signer(
                venue,
                options,
                Some(crate::production_entropy()),
                Some(signer),
                register,
            )
        } else {
            safe_cex_venues::build(venue, options, Some(crate::production_entropy()), register)
        };
        result.map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.message))?
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
        #[cfg(feature = "address-nonce")]
        let mut result = result;
        for task in self.tasks.drain(..) {
            #[cfg(not(feature = "address-nonce"))]
            let _ = task.await;
            #[cfg(feature = "address-nonce")]
            match task.await {
                Ok(Ok(())) => {}
                Ok(Err(error)) if result.is_ok() => result = Err(error),
                Err(_) if result.is_ok() => {
                    result = Err(io::Error::other("Account task failed during shutdown"));
                }
                _ => {}
            }
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

/// One account actor reached in-process, for hosts that own their client wire.
/// Spawn it inside a tokio `LocalSet`; the handle is `Send` and may cross threads.
#[derive(Clone)]
pub struct AccountHandle {
    commands: mpsc::Sender<Control>,
    stop: Arc<watch::Sender<bool>>,
    config: ServerConfig,
    capabilities: safe_cex_engine::Capabilities,
}

/// A joined client's stream: a full snapshot frame first, then ordered flush
/// frames and JSON notices exactly as the frame-server wire sends them.
pub struct ClientLink {
    pub outgoing: mpsc::Receiver<Outgoing>,
    /// Becomes true when the account drops this client (slow peer, oversized frame, dispose).
    pub canceled: watch::Receiver<bool>,
}

/// Starts an account actor without a frame-server listener. The task ends after
/// `dispose` or when every handle is dropped; await it to observe teardown.
pub fn spawn_account<V: Venue + 'static, T: Transport + 'static>(
    host: Host<V, T>,
    config: ServerConfig,
) -> io::Result<(AccountHandle, JoinHandle<io::Result<()>>)> {
    config.validate()?;
    if host.engine.has_received_commands() {
        return Err(invalid(
            "account engine must not receive commands before server registration",
        ));
    }
    let capabilities = host.engine.capabilities();
    let (commands, inbox) = mpsc::channel(256);
    let (stop, stopped) = watch::channel(false);
    let actor = Account::new(host, config.clone());
    let task = tokio::task::spawn_local(async move {
        #[cfg(not(feature = "address-nonce"))]
        {
            actor.run(inbox, stopped).await;
            Ok(())
        }
        #[cfg(feature = "address-nonce")]
        actor.run(inbox, stopped).await
    });
    let handle = AccountHandle {
        commands,
        stop: Arc::new(stop),
        config,
        capabilities,
    };
    Ok((handle, task))
}

/// `spawn_account` for a venue named as in `FrameServer::add_venue`, with an
/// optional deployment-owned signer.
#[cfg(feature = "venues")]
pub fn spawn_venue_account<T: Transport + 'static>(
    venue: &str,
    options_json: &str,
    transport: T,
    signer: Option<safe_cex_engine::signing::Provider>,
    config: ServerConfig,
) -> io::Result<(AccountHandle, JoinHandle<io::Result<()>>)> {
    struct Spawn<T> {
        transport: T,
        config: ServerConfig,
    }
    impl<T: Transport + 'static> safe_cex_venues::Visit for Spawn<T> {
        type Output = io::Result<(AccountHandle, JoinHandle<io::Result<()>>)>;
        fn visit<V: Venue + 'static>(self, engine: safe_cex_engine::Engine<V>) -> Self::Output {
            spawn_account(Host::new(engine, self.transport), self.config)
        }
    }
    let options = serde_json::from_str(options_json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let spawn = Spawn { transport, config };
    let entropy = Some(crate::production_entropy());
    let result = match signer {
        Some(signer) => {
            safe_cex_venues::build_with_signer(venue, options, entropy, Some(signer), spawn)
        }
        None => safe_cex_venues::build(venue, options, entropy, spawn),
    };
    result.map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error.message))?
}

/// Why `AccountHandle::input` refused a client message; `notice` is the
/// frame-server error notice to send back.
#[derive(Debug)]
pub struct InputRefused {
    pub notice: Arc<str>,
}

impl AccountHandle {
    /// The venue's engine capabilities, fixed at construction.
    pub fn capabilities(&self) -> &safe_cex_engine::Capabilities {
        &self.capabilities
    }

    /// `client` must be unique among this account's live clients.
    pub async fn join(&self, client: u64, account_auth: bool) -> Result<ClientLink, &'static str> {
        let (outgoing, frames) = mpsc::channel(self.config.client_queue_capacity);
        let (cancel, canceled) = watch::channel(false);
        let (ready, joined) = oneshot::channel();
        let join = Control::Join {
            client,
            outgoing,
            cancel,
            account_auth,
            ready,
        };
        self.commands
            .send(join)
            .await
            .map_err(|_| "account stopped")?;
        joined.await.map_err(|_| "account stopped")??;
        Ok(ClientLink {
            outgoing: frames,
            canceled,
        })
    }

    /// Decodes one JSON command exactly as the frame-server wire does.
    /// `Err` carries the notice to return; `Ok(false)` means the account's
    /// queue is full or it stopped, which the frame server treats as a disconnect.
    pub fn input(&self, client: u64, text: &str) -> Result<bool, InputRefused> {
        let control = match safe_cex_engine::command::decode_host_input(text) {
            Ok(HostInput::PublicFeedUpdate(_)) => {
                return Err(InputRefused {
                    notice: encode_notice(ServerNotice::Error {
                        command: None,
                        error: "public feed updates require a trusted host provider",
                    }),
                });
            }
            Ok(HostInput::Command { command, .. }) => Control::Command { client, command },
            Ok(HostInput::PollScaleChanged { scale, ceiling_ms }) => Control::PollScaleChanged {
                client,
                scale,
                ceiling_ms,
            },
            Err(_) => {
                return Err(InputRefused {
                    notice: encode_notice(ServerNotice::Error {
                        command: None,
                        error: "invalid command",
                    }),
                });
            }
        };
        Ok(self.commands.try_send(control).is_ok())
    }

    /// Runs `read` against the engine store on the account's thread, between turns.
    pub async fn read<R: Send + 'static>(
        &self,
        read: impl FnOnce(&safe_cex_engine::Store) -> R + Send + 'static,
    ) -> Option<R> {
        let (reply, result) = oneshot::channel();
        let read = Box::new(move |store: &safe_cex_engine::Store| {
            let _ = reply.send(read(store));
        });
        self.commands.send(Control::Read(read)).await.ok()?;
        result.await.ok()
    }

    pub fn leave(&self, client: u64) {
        let _ = self.commands.try_send(Control::Leave(client));
    }

    /// Stops the actor: clients are canceled, venue sockets close, timers end.
    pub fn dispose(&self) {
        self.stop.send_replace(true);
    }
}

/// What an account sends a client: SCXF frames, or JSON notices (`type: done | error | event | ...`).
#[derive(Clone, Debug)]
pub enum Outgoing {
    Frame(Arc<[u8]>),
    Text(Arc<str>),
}
impl Outgoing {
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
        account_auth: bool,
        ready: oneshot::Sender<Result<(), &'static str>>,
    },
    Command {
        client: u64,
        command: Command,
    },
    PollScaleChanged {
        client: u64,
        scale: f64,
        ceiling_ms: Option<f64>,
    },
    Leave(u64),
    Read(Box<dyn FnOnce(&safe_cex_engine::Store) + Send>),
}

/// Subprotocol a client offers beside `safe-cex-frame.v1` to receive
/// `accountAuth` notices; the negotiated protocol stays v1.
pub const ACCOUNT_AUTH_PROTOCOL: &str = "safe-cex-frame.account-auth";

struct Peer {
    outgoing: mpsc::Sender<Outgoing>,
    cancel: watch::Sender<bool>,
    books: HashSet<String>,
    candles: HashSet<(String, Interval)>,
    tickers: Option<HashSet<String>>,
    /// Offered ACCOUNT_AUTH_PROTOCOL: older v1 clients reject unknown notices.
    account_auth: bool,
}
struct Pending {
    client: Option<u64>,
    original: u32,
}
struct Account<V: Venue, T: Transport> {
    #[cfg(feature = "address-nonce")]
    host: crate::SetupRunOwner<V, T>,
    #[cfg(not(feature = "address-nonce"))]
    host: Host<V, T>,
    config: ServerConfig,
    peers: HashMap<u64, Peer>,
    books: HashMap<String, usize>,
    candles: HashMap<(String, Interval), usize>,
    pending: HashMap<u32, Pending>,
    next_command: u32,
    /// Replayed to clients that join after startup failed, so none waits on it.
    startup_failure: Option<Arc<str>>,
}

impl<V: Venue, T: Transport> Account<V, T> {
    fn new(mut host: Host<V, T>, config: ServerConfig) -> Self {
        host.engine.set_book_deltas(true);
        Self {
            #[cfg(feature = "address-nonce")]
            host: crate::SetupRunOwner {
                host: Some(host),
                cleanup: None,
            },
            #[cfg(not(feature = "address-nonce"))]
            host,
            config,
            peers: HashMap::new(),
            books: HashMap::new(),
            candles: HashMap::new(),
            pending: HashMap::new(),
            next_command: 0,
            startup_failure: None,
        }
    }

    async fn run(
        mut self,
        mut inbox: mpsc::Receiver<Control>,
        mut shutdown: watch::Receiver<bool>,
    ) -> AccountResult {
        #[cfg(not(feature = "address-nonce"))]
        self.drive(&mut inbox, &mut shutdown).await;
        #[cfg(feature = "address-nonce")]
        {
            use futures_util::FutureExt;
            let result = std::panic::AssertUnwindSafe(self.drive(&mut inbox, &mut shutdown))
                .catch_unwind()
                .await;
            self.host.stop();
            let cleanup = self
                .host
                .host
                .as_mut()
                .unwrap()
                .engine
                .take_address_setup_cleanup();
            self.host.cleanup = Some(Box::pin(cleanup));
            let cleanup = self.host.cleanup.as_mut().unwrap().await;
            self.host.cleanup = None;
            if let Err(panic) = result {
                std::panic::resume_unwind(panic);
            }
            cleanup.map_err(|_| io::Error::other("Address setup cleanup failed"))
        }
    }

    async fn drive(
        &mut self,
        inbox: &mut mpsc::Receiver<Control>,
        shutdown: &mut watch::Receiver<bool>,
    ) {
        self.host.turn(|host| {
            let now = (host.clock)();
            #[cfg(feature = "address-nonce")]
            if !host.engine.has_started()
                && let Err(error) = host.engine.initialize_address_nonce(now)
            {
                host.terminal = Some(error);
                return;
            }
            host.engine.start(now);
        });
        self.execute();
        let mut flush = tokio::time::interval(self.config.flush_every);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                control = inbox.recv() => match control {
                    Some(Control::Join { client, outgoing, cancel, account_auth, ready }) => {
                        let _ = ready.send(self.join(client, outgoing, cancel, account_auth));
                    }
                    Some(Control::Command { client, command }) => self.command(client, command),
                    Some(Control::PollScaleChanged { client, scale, ceiling_ms }) => {
                        if self.peers.contains_key(&client) {
                            self.host.turn(|host| {
                                let now = (host.clock)();
                                host.engine.handle(now, Event::PollCeilingChanged { ceiling_ms });
                                host.engine.handle(now, Event::PollScaleChanged { scale });
                            });
                        }
                    }
                    Some(Control::Leave(client)) => self.leave(client),
                    Some(Control::Read(read)) => read(self.host.engine.store()),
                    None => break,
                },
                Some(event) = self.host.inbox.recv() => self.host.dispatch(event),
                _ = flush.tick() => {
                    let gone: Vec<_> = self.peers.iter().filter(|(_, peer)| peer.outgoing.is_closed()).map(|(id, _)| *id).collect();
                    for client in gone { self.leave(client); }
                    self.flush();
                }
            }
            self.execute();
        }
        for (_, peer) in self.peers.drain() {
            peer.cancel.send_replace(true);
        }
        self.host.stop();
    }

    fn join(
        &mut self,
        client: u64,
        outgoing: mpsc::Sender<Outgoing>,
        cancel: watch::Sender<bool>,
        account_auth: bool,
    ) -> Result<(), &'static str> {
        if self.peers.contains_key(&client) {
            return Err("duplicate client ID");
        }
        if self.peers.len() >= self.config.max_clients_per_account {
            return Err("account client limit reached");
        }
        // Snapshot and registration share this actor turn. Existing peers
        // establish the same baseline before a new peer takes its snapshot.
        self.flush();
        let snapshot = self
            .host
            .turn(|host| Arc::<[u8]>::from(host.engine.snapshot_frame((host.clock)())));
        self.execute();
        if let Some(snapshot) = snapshot
            && self.host.terminal.is_none()
            && (snapshot.len() > self.config.max_frame_bytes
                || outgoing.try_send(Outgoing::Frame(snapshot)).is_err())
        {
            return Err("snapshot exceeds connection limits");
        }
        if let Some(failure) = &self.startup_failure {
            let _ = outgoing.try_send(Outgoing::Text(failure.clone()));
        }
        self.peers.insert(
            client,
            Peer {
                outgoing,
                cancel,
                books: HashSet::new(),
                candles: HashSet::new(),
                tickers: None,
                account_auth,
            },
        );
        Ok(())
    }

    fn execute(&mut self) {
        let mut notices = Vec::new();
        self.host.execute(&mut |notice| notices.push(notice));
        self.host
            .report_terminal(&mut |notice| notices.push(notice));
        for notice in notices {
            match notice {
                Notice::CommandDone { command, result } => {
                    if let Some(pending) = self.pending.remove(&command)
                        && let Some(client) = pending.client
                    {
                        let text = encode_notice(ServerNotice::Done {
                            command: pending.original,
                            result: Some(&result),
                            error: None,
                            detail: None,
                        });
                        let text = self.bounded_done(pending.original, text);
                        self.send(client, Outgoing::Text(text));
                    }
                }
                Notice::CommandFailed { command, error } => {
                    if let Some(pending) = self.pending.remove(&command)
                        && let Some(client) = pending.client
                    {
                        let text = encode_notice(ServerNotice::Done {
                            command: pending.original,
                            result: None,
                            error: Some(error.legacy_text().into()),
                            detail: Some(&error),
                        });
                        let text = self.bounded_done(pending.original, text);
                        self.send(client, Outgoing::Text(text));
                    }
                }
                Notice::CommandProgress { command, receipts } => {
                    if let Some(pending) = self.pending.get(&command)
                        && let Some(client) = pending.client
                    {
                        let text = encode_notice(ServerNotice::CommandProgress {
                            command: pending.original,
                            receipts: &receipts,
                        });
                        self.send(client, Outgoing::Text(text));
                    }
                }
                Notice::AccountAuth {
                    credential,
                    scope,
                    outcome,
                } => self.broadcast_to(
                    Outgoing::Text(encode_notice(ServerNotice::AccountAuth {
                        credential,
                        scope,
                        outcome,
                    })),
                    |peer| peer.account_auth,
                ),
                Notice::StartupFailed { error } => {
                    let mut text = encode_notice(ServerNotice::StartupFailed { detail: &error });
                    if text.len() > self.config.max_frame_bytes {
                        // Late peers replay this notice, so keep a deliverable failure.
                        // Classification survives so clients can still branch on it.
                        let mut bounded = VenueError::new(
                            "Startup failed; its diagnostic exceeded the native message limit",
                        );
                        bounded.kind = error.kind;
                        bounded.outcome = error.outcome;
                        text = encode_notice(ServerNotice::StartupFailed { detail: &bounded });
                    }
                    self.startup_failure = Some(text.clone());
                    self.broadcast(Outgoing::Text(text))
                }
                Notice::Deactivate => {
                    self.broadcast(Outgoing::Text(encode_notice(ServerNotice::Deactivate)))
                }
                Notice::CommandError { command, message } => {
                    if let Some(pending) = self.pending.get(&command)
                        && let Some(client) = pending.client
                    {
                        self.send(
                            client,
                            Outgoing::Text(encode_notice(ServerNotice::CommandError {
                                command: pending.original,
                                message: &message,
                            })),
                        );
                    }
                }
                Notice::Event(mut event) => {
                    let owner = if let Some(index) =
                        event.fields.iter().position(|(key, _)| *key == "command")
                    {
                        // Reject ambiguous fields and values that cannot name a native u32 command.
                        if event.fields[index + 1..]
                            .iter()
                            .any(|(key, _)| *key == "command")
                        {
                            continue;
                        }
                        let Value::Num(command) = &event.fields[index].1 else {
                            continue;
                        };
                        if !command.is_finite()
                            || command.fract() != 0.0
                            || *command < 0.0
                            || *command > f64::from(u32::MAX)
                        {
                            continue;
                        }
                        let Some(pending) = self.pending.get(&(*command as u32)) else {
                            continue;
                        };
                        let Some(client) = pending
                            .client
                            .filter(|client| self.peers.contains_key(client))
                        else {
                            continue;
                        };
                        event.fields[index].1 = Value::from(pending.original);
                        Some(client)
                    } else {
                        // A coverage marker without an owner is not a background diagnostic.
                        if event.code == safe_cex_engine::log::code::HISTORY_INCOMPLETE {
                            continue;
                        }
                        None
                    };
                    let level = event.level.as_str();
                    self.route_log(
                        owner,
                        level,
                        event.level >= Level::Warn,
                        |message| {
                            encode_notice(ServerNotice::Event {
                                level,
                                code: event.code,
                                message,
                                fields: NoticeFields(&event.fields),
                            })
                        },
                        &event.message,
                        Some(event.code),
                    );
                }
            }
        }
        if let Some(error) = self.host.terminal.clone() {
            // A released command may have reached the venue, even when the
            // fatal callback's output was withheld. Settle every owned waiter
            // once without fabricating a NotSent result or a new engine turn.
            let pending: Vec<_> = self.pending.drain().map(|(_, pending)| pending).collect();
            for pending in pending {
                if let Some(client) = pending.client {
                    let text = encode_notice(ServerNotice::Done {
                        command: pending.original,
                        result: None,
                        error: Some(error.legacy_text().into()),
                        detail: Some(&error),
                    });
                    self.send(
                        client,
                        Outgoing::Text(self.bounded_done(pending.original, text)),
                    );
                }
            }
        }
    }

    /// Log notices never disconnect peers: an oversized one is cut to fit, and
    /// with no client attached Warn+ goes to stderr instead of being dropped.
    fn route_log(
        &mut self,
        owner: Option<u64>,
        level: &str,
        warn: bool,
        encode: impl Fn(&str) -> Arc<str>,
        message: &str,
        code: Option<&str>,
    ) {
        if self.peers.is_empty() {
            if warn {
                eprintln!("safe-cex {level} {}: {message}", code.unwrap_or("log"));
            }
            return;
        }
        let limit = self.config.max_frame_bytes;
        let mut text = encode(message);
        if text.len() > limit {
            // Escaping makes encoded length non-linear, so search the longest prefix that fits.
            let (mut fits, mut over) = (0, message.len());
            while over - fits > 1 {
                let mut mid = (fits + over) / 2;
                while !message.is_char_boundary(mid) {
                    mid -= 1;
                }
                if mid <= fits {
                    break;
                }
                if encode(&message[..mid]).len() <= limit {
                    fits = mid;
                } else {
                    over = mid;
                }
            }
            text = encode(&message[..fits]);
        }
        if text.len() <= limit {
            if let Some(client) = owner {
                self.send(client, Outgoing::Text(text));
            } else {
                self.broadcast(Outgoing::Text(text));
            }
        }
    }

    fn flush(&mut self) {
        let frame = self.host.turn(|host| {
            host.engine
                .take_frame((host.clock)())
                .map(Arc::<[u8]>::from)
        });
        if let Some(Some(frame)) = frame {
            self.broadcast(Outgoing::Frame(frame));
        }
    }

    /// A completion too large to deliver still settles its command, as an error
    /// whose outcome is unknown: the engine may have succeeded.
    fn bounded_done(&self, command: u32, text: Arc<str>) -> Arc<str> {
        if text.len() <= self.config.max_frame_bytes {
            return text;
        }
        let error = VenueError::new(format!(
            "Engine response of {} bytes exceeds the {}-byte native message limit",
            text.len(),
            self.config.max_frame_bytes
        ))
        .outcome(safe_cex_engine::Outcome::Unknown);
        encode_notice(ServerNotice::Done {
            command,
            result: None,
            error: Some(error.legacy_text().into()),
            detail: Some(&error),
        })
    }

    /// Replaces one undeliverable notice with a short warning, when even that fits.
    fn oversized_notice(&self, bytes: usize) -> Option<Outgoing> {
        let message = format!(
            "Dropped a {bytes}-byte notice over the {}-byte native message limit",
            self.config.max_frame_bytes
        );
        eprintln!("safe-cex native server: {message}");
        let text = encode_notice(ServerNotice::Event {
            level: "warn",
            code: "host.notice_dropped",
            fields: NoticeFields(&[]),
            message: &message,
        });
        (text.len() <= self.config.max_frame_bytes).then_some(Outgoing::Text(text))
    }

    fn send(&mut self, client: u64, message: Outgoing) {
        let message = match message {
            Outgoing::Text(text) if text.len() > self.config.max_frame_bytes => {
                match self.oversized_notice(text.len()) {
                    Some(notice) => notice,
                    None => return,
                }
            }
            Outgoing::Frame(frame) if frame.len() > self.config.max_frame_bytes => {
                self.leave(client);
                return;
            }
            message => message,
        };
        if let Some(peer) = self.peers.get(&client)
            && peer.outgoing.try_send(message).is_err()
        {
            self.leave(client);
        }
    }

    fn broadcast(&mut self, message: Outgoing) {
        match message {
            Outgoing::Text(text) if text.len() > self.config.max_frame_bytes => {
                if let Some(notice) = self.oversized_notice(text.len()) {
                    self.deliver(notice);
                }
            }
            Outgoing::Frame(frame) if frame.len() > self.config.max_frame_bytes => {
                match segment_frame(&frame, self.config.max_frame_bytes) {
                    Some(segments) => {
                        eprintln!(
                            "safe-cex native server: {}-byte frame exceeded the {}-byte limit; sent as {} frames",
                            frame.len(),
                            self.config.max_frame_bytes,
                            segments.len()
                        );
                        for segment in segments {
                            self.deliver(Outgoing::Frame(segment));
                        }
                    }
                    None => {
                        // One section alone exceeds the limit: no peer can apply this delta,
                        // and resuming without it would diverge, so every peer resnapshots.
                        eprintln!(
                            "safe-cex native server: {}-byte frame has a section over the {}-byte limit; disconnecting peers to resnapshot",
                            frame.len(),
                            self.config.max_frame_bytes
                        );
                        let clients: Vec<_> = self.peers.keys().copied().collect();
                        for client in clients {
                            self.leave(client);
                        }
                    }
                }
            }
            message => self.deliver(message),
        }
    }

    /// Account-auth notices reach only the peers that opted in to them.
    fn broadcast_to(&mut self, message: Outgoing, recipient: impl Fn(&Peer) -> bool) {
        let message = match message {
            Outgoing::Text(text) if text.len() > self.config.max_frame_bytes => {
                match self.oversized_notice(text.len()) {
                    Some(notice) => notice,
                    None => return,
                }
            }
            message => message,
        };
        self.deliver_to(message, recipient);
    }

    fn deliver(&mut self, message: Outgoing) {
        self.deliver_to(message, |_| true)
    }

    fn deliver_to(&mut self, message: Outgoing, recipient: impl Fn(&Peer) -> bool) {
        let gone: Vec<_> = self
            .peers
            .iter()
            .filter(|(_, peer)| recipient(peer))
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
            if release(&mut self.candles, &(symbol.clone(), interval)) {
                self.handle(Command::UnsubscribeCandles { symbol, interval });
            }
        }
        if peer.tickers.is_some() {
            self.tickers();
        }
    }

    fn handle(&mut self, command: Command) {
        self.host
            .turn(|host| host.engine.handle((host.clock)(), Event::Command(command)));
    }

    fn command(&mut self, client: u64, mut command: Command) {
        if let Some(error) = self.host.terminal.clone() {
            if let Some(command) = command_id(&mut command) {
                let text = encode_notice(ServerNotice::Done {
                    command: *command,
                    result: None,
                    error: Some(error.legacy_text().into()),
                    detail: Some(&error),
                });
                self.send(client, Outgoing::Text(self.bounded_done(*command, text)));
            }
            return;
        }
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
                        self.host
                            .turn(|host| host.engine.request_book_resync(symbol));
                    }
                } else {
                    self.host
                        .turn(|host| host.engine.request_book_resync(symbol));
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
                let key = (symbol.clone(), *interval);
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
                let key = (symbol.clone(), *interval);
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
                if !valid_symbol(symbol) {
                    return false;
                }
                let key = (symbol.clone(), *interval);
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
        | Command::FetchMaxLeverage { command, .. }
        | Command::Query { command, .. }
        | Command::FetchCandles { command, .. }
        | Command::Venue { command, .. } => Some(command),
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
    AccountAuth {
        credential: safe_cex_engine::CredentialSlot,
        scope: safe_cex_engine::AccountReadScope,
        outcome: safe_cex_engine::AuthOutcome,
    },
    CommandError {
        command: u32,
        message: &'a str,
    },
    CommandProgress {
        command: u32,
        receipts: &'a [safe_cex_engine::OrderIdentityReceipt],
    },
    Done {
        command: u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<std::borrow::Cow<'a, str>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<&'a VenueError>,
    },
    Error {
        #[serde(skip_serializing_if = "Option::is_none")]
        command: Option<u32>,
        error: &'a str,
    },
    Event {
        level: &'a str,
        code: &'a str,
        message: &'a str,
        #[serde(skip_serializing_if = "NoticeFields::is_empty")]
        fields: NoticeFields<'a>,
    },
    Deactivate,
    StartupFailed {
        detail: &'a VenueError,
    },
}

struct NoticeFields<'a>(&'a [(&'static str, Value)]);

impl NoticeFields<'_> {
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Serialize for NoticeFields<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in self.0 {
            match value {
                Value::Str(text) => map.serialize_entry(key, text)?,
                Value::Num(number) if number.is_finite() => map.serialize_entry(key, number)?,
                Value::Num(_) => map.serialize_entry(key, &())?,
                Value::Bool(flag) => map.serialize_entry(key, flag)?,
                Value::Redacted => map.serialize_entry(key, safe_cex_engine::log::REDACTED)?,
            }
        }
        map.end()
    }
}

/// The parent a section is only read beside, in the same frame, by frame consumers.
fn section_parent(kind: u16) -> Option<u16> {
    use safe_cex_engine::wire::section;
    match kind {
        section::PREDICTION_MARKETS
        | section::INSTRUMENT_CLASSIFICATIONS
        | section::DERIVATIVE_PRODUCTS => Some(section::MARKETS),
        section::POSITION_GREEKS | section::PREDICTION_HOLDINGS => Some(section::POSITIONS),
        _ => None,
    }
}

/// Splits a delta frame into consecutive frames under `max` bytes. The engine
/// writes sections in the order consumers apply them, so applying the segments
/// in sequence equals applying the frame, given that each carries the header and
/// full string table, dependent sections stay with their parent, and STATUS
/// (loaded and hedge flags) arrives last with the SETTINGS that read its hedge
/// flag. `None` when the frame is malformed or a unit cannot fit with the table.
fn segment_frame(frame: &[u8], max: usize) -> Option<Vec<Arc<[u8]>>> {
    use safe_cex_engine::wire::section;
    const HEADER: usize = 16;
    let count = u16::from_le_bytes(frame.get(6..8)?.try_into().ok()?) as usize;
    // A unit is a parent section and its dependents: (head kind, bytes, sections).
    let mut units: Vec<(u16, usize, Vec<&[u8]>)> = Vec::with_capacity(count);
    let mut tail: Vec<&[u8]> = Vec::new();
    let mut strings: &[u8] = &[];
    let mut at = HEADER;
    for _ in 0..count {
        let header = frame.get(at..at + 8)?;
        let kind = u16::from_le_bytes([header[0], header[1]]);
        let length = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
        let bytes = frame.get(at..at.checked_add(8 + length)?)?;
        at += bytes.len();
        match (kind, section_parent(kind)) {
            (section::STRINGS, _) => strings = bytes,
            (section::STATUS | section::SETTINGS, _) => tail.push(bytes),
            (_, Some(parent)) => {
                let unit = units.last_mut().filter(|unit| unit.0 == parent)?;
                unit.1 += bytes.len();
                unit.2.push(bytes);
            }
            _ => units.push((kind, bytes.len(), vec![bytes])),
        }
    }
    if at != frame.len() {
        return None;
    }
    if !tail.is_empty() {
        units.push((section::STATUS, tail.iter().map(|s| s.len()).sum(), tail));
    }
    let fixed = HEADER + strings.len();
    let mut groups: Vec<Vec<&[u8]>> = vec![Vec::new()];
    let mut size = fixed;
    for (_, bytes, sections) in units {
        if fixed + bytes > max {
            return None;
        }
        if size + bytes > max {
            groups.push(Vec::new());
            size = fixed;
        }
        size += bytes;
        groups.last_mut()?.extend(sections);
    }
    Some(
        groups
            .into_iter()
            .map(|group| {
                let mut out = Vec::with_capacity(max.min(frame.len()));
                out.extend_from_slice(&frame[..HEADER]);
                let sections = group.len() + usize::from(!strings.is_empty());
                out[6..8].copy_from_slice(&(sections as u16).to_le_bytes());
                for section in group {
                    out.extend_from_slice(section);
                }
                out.extend_from_slice(strings);
                out.into()
            })
            .collect(),
    )
}

fn encode_notice(notice: ServerNotice<'_>) -> Arc<str> {
    serde_json::to_string(&notice)
        .expect("notice contains only strings, numbers and enums")
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
    let mut account_auth = false;
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
        account_auth = protocols
            .split(',')
            .any(|protocol| protocol.trim() == ACCOUNT_AUTH_PROTOCOL);
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
        account_auth,
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
                                HostInput::Command { command, .. } => Control::Command { client, command },
                                HostInput::PollScaleChanged { scale, ceiling_ms } => {
                                    Control::PollScaleChanged { client, scale, ceiling_ms }
                                }
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
    use safe_cex_engine::{Cx, HttpRequest, Response, SocketId, TimerId};
    use std::cell::RefCell;
    use std::rc::Rc;

    struct Stub(Rc<RefCell<Vec<Command>>>);
    impl Venue for Stub {
        type Pending = ();
        fn start(&mut self, _: &mut Cx<'_, ()>) {}
        fn on_open(&mut self, cx: &mut Cx<'_, ()>, socket: SocketId) {
            let command =
                u32::try_from(socket.0).expect("fixture carries a command ID in its socket slot");
            cx.done(command, "[]".into());
        }
        fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
            if text == "auth" {
                let request = cx.request(
                    safe_cex_engine::Method::Get,
                    "https://fixture/account".into(),
                    vec![],
                    None,
                    (),
                );
                cx.tag_account_read(
                    request,
                    safe_cex_engine::CredentialSlot::Secondary,
                    safe_cex_engine::AccountReadScope::Orders,
                );
                return;
            }
            if let Some(command) = text.strip_prefix("event:") {
                let fields = match command {
                    "missing" => Vec::new(),
                    "string" => vec![("command", Value::from("2"))],
                    "boolean" => vec![("command", Value::Bool(true))],
                    "duplicate" => vec![("command", 2u32.into()), ("command", 2u32.into())],
                    value => vec![("command", Value::Num(value.parse().expect("event command")))],
                };
                cx.event(
                    Level::Warn,
                    safe_cex_engine::log::code::HISTORY_INCOMPLETE,
                    "\"é".repeat(400),
                    fields,
                );
                return;
            }
            if text == "background" {
                cx.notice(
                    Level::Warn,
                    safe_cex_engine::log::code::ROW_SKIPPED,
                    "background warning",
                );
                return;
            }
            if let Some(command) = text.strip_prefix("error:") {
                let command = command.parse().expect("error command");
                cx.command_error(command, "placement rejected");
                return;
            }
            let command: u32 = text.parse().expect("receipt command");
            cx.command_progress(
                command,
                vec![safe_cex_engine::OrderIdentityReceipt {
                    symbol: "BTC-USD".into(),
                    kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
                    identity: format!("native-{command}"),
                }],
            );
        }
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn account_auth_outcome(
            &self,
            _: &(),
            _: &Response<'_>,
            _: f64,
            _store: &safe_cex_engine::Store,
        ) -> Option<safe_cex_engine::AuthOutcome> {
            Some(safe_cex_engine::AuthOutcome::Rejected)
        }
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
            if matches!(&command, Command::FetchLeverage {symbol, ..} if symbol == "fatal") {
                cx.send(
                    SocketId(safe_cex_engine::resource_ids::MAX_RESOURCE_ID + 1),
                    "unsafe",
                );
                return;
            }
            if matches!(&command, Command::CancelSymbolOrders { .. }) {
                cx.request(
                    safe_cex_engine::Method::Post,
                    "https://fixture/cancel".into(),
                    vec![],
                    None,
                    (),
                );
            }
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

    fn event_actor(
        max_frame_bytes: usize,
    ) -> (Account<Stub, NoTransport>, Vec<mpsc::Receiver<Outgoing>>) {
        event_actor_with(max_frame_bytes, NoTransport)
    }

    fn event_actor_with<T: Transport>(
        max_frame_bytes: usize,
        transport: T,
    ) -> (Account<Stub, T>, Vec<mpsc::Receiver<Outgoing>>) {
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(Rc::default())),
                transport,
            ),
            ServerConfig {
                max_frame_bytes,
                ..ServerConfig::default()
            },
        );
        let mut receivers = Vec::new();
        for client in [1, 2] {
            let (outgoing, receiver) = mpsc::channel(8);
            let (cancel, _) = watch::channel(false);
            actor.peers.insert(
                client,
                Peer {
                    outgoing,
                    cancel,
                    books: HashSet::new(),
                    candles: HashSet::new(),
                    tickers: None,
                    account_auth: false,
                },
            );
            receivers.push(receiver);
            actor.command(
                client,
                Command::FetchLeverage {
                    command: 77,
                    symbol: "BTC-USD".into(),
                },
            );
        }
        (actor, receivers)
    }

    fn emit_event(actor: &mut Account<Stub, NoTransport>, text: &str) {
        actor.host.engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(0),
                text,
            },
        );
        actor.execute();
    }

    fn receive_notice(receiver: &mut mpsc::Receiver<Outgoing>) -> serde_json::Value {
        let Ok(Outgoing::Text(message)) = receiver.try_recv() else {
            panic!("expected a text notice");
        };
        serde_json::from_str(&message).unwrap()
    }

    #[test]
    fn command_events_translate_overlapping_external_ids_before_done_without_broadcasting() {
        let (mut actor, mut receivers) = event_actor(4096);
        actor.host.engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "event:1",
            },
        );
        actor.host.engine.handle(
            1.,
            Event::WsOpened {
                socket: SocketId(1),
            },
        );
        actor.execute();
        let event = receive_notice(&mut receivers[0]);
        assert_eq!(event["code"], "history.incomplete");
        assert_eq!(event["fields"]["command"].as_f64(), Some(77.0));
        let done = receive_notice(&mut receivers[0]);
        assert_eq!(done["type"], "done");
        assert_eq!(done["command"], 77);
        assert!(receivers[1].try_recv().is_err());
        assert!(receivers[0].try_recv().is_err());
        emit_event(&mut actor, "event:2");
        assert_eq!(
            receive_notice(&mut receivers[1])["fields"]["command"].as_f64(),
            Some(77.0)
        );
        assert!(receivers[0].try_recv().is_err());
    }

    #[tokio::test]
    async fn terminal_engine_failure_settles_pending_commands_and_late_joins_without_frames() {
        struct Recorded(Rc<RefCell<Vec<safe_cex_engine::Method>>>);
        impl Transport for Recorded {
            fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
                panic!("no new sockets")
            }
            fn request(&self, request: HttpRequest, _: Events) {
                self.0.borrow_mut().push(request.method);
            }
        }
        let sent = Rc::new(RefCell::new(Vec::new()));
        let (mut actor, mut receivers) = event_actor_with(4096, Recorded(sent.clone()));
        // A real physical POST precedes the resource fault. The failure must
        // retain Unknown even though the next action batch never leaves Rust.
        actor.command(
            1,
            Command::CancelSymbolOrders {
                command: 88,
                symbol: "BTC-USD".into(),
            },
        );
        actor.execute();
        assert_eq!(*sent.borrow(), [safe_cex_engine::Method::Post]);
        let timer = tokio::spawn(std::future::pending::<()>());
        actor.host.timers.insert(TimerId(80), timer.abort_handle());
        let signing = tokio::spawn(std::future::pending::<()>());
        actor.host.signing_tasks.insert(
            safe_cex_engine::RequestId(81),
            crate::managed_http::Task(signing.abort_handle()),
        );
        let (outgoing, mut socket) = mpsc::unbounded_channel();
        actor.host.sockets.insert(SocketId(82), outgoing);
        let events = actor.host.event_sender();
        // The next callback queues an invalid resource after the POST has
        // physically left. The common engine guard must withhold its batch.
        actor.host.engine.handle(
            1.,
            Event::Command(Command::FetchLeverage {
                command: 99,
                symbol: "fatal".into(),
            }),
        );
        actor.execute();
        assert!(actor.host.engine.failed());
        assert!(events.is_closed());
        assert!(
            actor.host.timers.is_empty()
                && actor.host.signing_tasks.is_empty()
                && actor.host.sockets.is_empty()
        );
        assert!(timer.await.unwrap_err().is_cancelled());
        assert!(signing.await.unwrap_err().is_cancelled());
        assert!(socket.recv().await.is_none());
        for (index, receiver) in receivers.iter_mut().enumerate() {
            let first = receive_notice(receiver);
            assert_eq!(first["type"], "startupFailed");
            assert_eq!(first["detail"]["kind"], "internal");
            assert_eq!(first["detail"]["outcome"], "unknown");
            let expected = if index == 0 { vec![77, 88] } else { vec![77] };
            let mut commands = Vec::new();
            for _ in &expected {
                let done = receive_notice(receiver);
                assert_eq!(done["type"], "done");
                assert_eq!(done["detail"]["kind"], "internal");
                assert_eq!(done["detail"]["outcome"], "unknown");
                commands.push(done["command"].as_u64().unwrap());
            }
            commands.sort();
            assert_eq!(commands, expected);
            assert!(receiver.try_recv().is_err());
        }
        assert!(actor.pending.is_empty());
        actor.execute();
        actor.flush();
        actor.host.dispatch(HostEvent::Timer(TimerId(80)));
        assert!(
            receivers
                .iter_mut()
                .all(|receiver| receiver.try_recv().is_err())
        );
        let (outgoing, mut messages) = mpsc::channel(4);
        actor
            .join(7, outgoing, watch::channel(false).0, false)
            .unwrap();
        let failure = receive_notice(&mut messages);
        assert_eq!(failure["type"], "startupFailed");
        assert!(
            messages.try_recv().is_err(),
            "dead engine cannot generate a snapshot"
        );
        actor.command(
            7,
            Command::FetchLeverage {
                command: 55,
                symbol: "BTC-USD".into(),
            },
        );
        let done = receive_notice(&mut messages);
        assert_eq!(done["type"], "done");
        assert_eq!(done["command"], 55);
        assert_eq!(done["detail"]["outcome"], "unknown");
        assert!(actor.pending.is_empty());
        assert_eq!(*sent.borrow(), [safe_cex_engine::Method::Post]);
    }

    #[tokio::test]
    async fn terminal_failure_during_join_drain_withholds_the_saved_snapshot() {
        let (mut actor, _) = event_actor(4096);
        actor.host.engine.handle(
            1.,
            Event::Command(Command::FetchLeverage {
                command: 99,
                symbol: "fatal".into(),
            }),
        );
        assert!(
            !actor.host.engine.failed(),
            "fault is still queued before join"
        );
        let (outgoing, mut messages) = mpsc::channel(4);
        actor
            .join(7, outgoing, watch::channel(false).0, false)
            .unwrap();
        assert!(actor.host.engine.failed());
        assert_eq!(receive_notice(&mut messages)["type"], "startupFailed");
        assert!(
            messages.try_recv().is_err(),
            "a saved pre-failure frame cannot escape after drain fails"
        );
    }

    #[test]
    fn command_events_suppress_malformed_unowned_and_disconnected_ids() {
        let (mut actor, mut receivers) = event_actor(4096);
        for text in [
            "event:string",
            "event:boolean",
            "event:duplicate",
            "event:missing",
            "event:1.5",
            "event:-1",
            "event:4294967296",
            "event:NaN",
            "event:inf",
            "event:999",
        ] {
            emit_event(&mut actor, text);
            assert!(receivers[0].try_recv().is_err(), "{text}");
            assert!(receivers[1].try_recv().is_err(), "{text}");
        }
        actor.host.engine.handle(
            1.,
            Event::WsOpened {
                socket: SocketId(1),
            },
        );
        actor.execute();
        receive_notice(&mut receivers[0]);
        emit_event(&mut actor, "event:1");
        assert!(receivers[0].try_recv().is_err());
        assert!(receivers[1].try_recv().is_err());
        actor.leave(2);
        emit_event(&mut actor, "event:2");
        assert!(
            receivers[0].try_recv().is_err(),
            "departed diagnostics cannot migrate"
        );
        emit_event(&mut actor, "background");
        assert_eq!(receive_notice(&mut receivers[0])["code"], "row.skipped");
    }

    #[test]
    fn command_event_diagnostics_remain_bounded_without_disconnect_or_owner_leak() {
        let (mut actor, mut receivers) = event_actor(180);
        emit_event(&mut actor, "event:1");
        let Ok(Outgoing::Text(message)) = receivers[0].try_recv() else {
            panic!("bounded event");
        };
        assert!(message.len() <= 180);
        let event: serde_json::Value = serde_json::from_str(&message).unwrap();
        assert_eq!(event["fields"]["command"].as_f64(), Some(77.0));
        assert_eq!(event["code"], "history.incomplete");
        assert!(event["message"].as_str().unwrap().starts_with("\"é"));
        assert!(receivers[1].try_recv().is_err());
        assert_eq!(actor.peers.len(), 2);
        assert_eq!(actor.pending.len(), 2);
    }

    #[test]
    fn credential_evidence_reaches_all_account_peers_without_command_ownership_or_cross_account_leaks()
     {
        let mut actors = Vec::new();
        let mut receivers = Vec::new();
        for _ in 0..2 {
            let mut actor = Account::new(
                Host::new(
                    safe_cex_engine::test_support::seeded_engine(Stub(Rc::new(RefCell::new(
                        Vec::new(),
                    )))),
                    NoTransport,
                ),
                ServerConfig::default(),
            );
            let mut peers = Vec::new();
            for client in [1, 2] {
                let (outgoing, receiver) = mpsc::channel(8);
                let (cancel, _) = watch::channel(false);
                actor.peers.insert(
                    client,
                    Peer {
                        outgoing,
                        cancel,
                        books: HashSet::new(),
                        candles: HashSet::new(),
                        tickers: None,
                        // Client 2 is a v1 client that never offered the notice.
                        account_auth: client == 1,
                    },
                );
                peers.push(receiver);
            }
            receivers.push(peers);
            actors.push(actor);
        }
        actors[0].host.engine.handle(
            0.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "auth",
            },
        );
        actors[0].host.engine.handle(
            1.,
            Event::HttpResponse {
                request: safe_cex_engine::RequestId(1),
                response: Response {
                    status: 401,
                    headers: "",
                    body: "{}",
                },
            },
        );
        actors[0].execute();
        let Ok(Outgoing::Text(message)) = receivers[0][0].try_recv() else {
            panic!("account auth notice")
        };
        assert_eq!(
            message.as_ref(),
            r#"{"type":"accountAuth","credential":"secondary","scope":"orders","outcome":"rejected"}"#
        );
        for peer in &mut receivers[0] {
            assert!(
                peer.try_recv().is_err(),
                "only opted-in peers receive it, once"
            );
        }
        for peer in &mut receivers[1] {
            assert!(peer.try_recv().is_err());
        }
        assert!(actors.iter().all(|actor| actor.pending.is_empty()));
    }

    #[test]
    fn command_errors_keep_original_client_id_and_cannot_leak_after_completion() {
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(Rc::new(RefCell::new(
                    Vec::new(),
                )))),
                NoTransport,
            ),
            ServerConfig::default(),
        );
        let mut receivers = Vec::new();
        for client in [1, 2] {
            let (outgoing, receiver) = mpsc::channel(8);
            let (cancel, _) = watch::channel(false);
            actor.peers.insert(
                client,
                Peer {
                    outgoing,
                    cancel,
                    books: HashSet::new(),
                    candles: HashSet::new(),
                    tickers: None,
                    account_auth: false,
                },
            );
            receivers.push(receiver);
            actor.command(
                client,
                Command::FetchLeverage {
                    command: 77,
                    symbol: "BTC-USD".into(),
                },
            );
        }
        actor.host.engine.handle(
            1.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "error:1",
            },
        );
        actor.host.engine.handle(
            1.,
            Event::WsOpened {
                socket: SocketId(1),
            },
        );
        actor.execute();
        let Ok(Outgoing::Text(message)) = receivers[0].try_recv() else {
            panic!("command error")
        };
        assert_eq!(
            message.as_ref(),
            r#"{"type":"commandError","command":77,"message":"placement rejected"}"#
        );
        let Ok(Outgoing::Text(done)) = receivers[0].try_recv() else {
            panic!("command done")
        };
        assert_eq!(
            done.as_ref(),
            r#"{"type":"done","command":77,"result":"[]"}"#
        );
        assert!(receivers[1].try_recv().is_err());
        assert_eq!(actor.pending.len(), 1);
        actor.host.engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "error:1",
            },
        );
        actor.execute();
        assert!(receivers[0].try_recv().is_err());
        assert!(receivers[1].try_recv().is_err());
    }

    #[test]
    fn command_receipts_keep_owner_mapping_until_done_and_never_reach_other_peers() {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(commands)),
                NoTransport,
            ),
            ServerConfig::default(),
        );
        let mut receivers = Vec::new();
        for client in [1, 2] {
            let (outgoing, receiver) = mpsc::channel(8);
            let (cancel, _) = watch::channel(false);
            actor.peers.insert(
                client,
                Peer {
                    outgoing,
                    cancel,
                    books: HashSet::new(),
                    candles: HashSet::new(),
                    tickers: None,
                    account_auth: false,
                },
            );
            receivers.push(receiver);
            actor.command(
                client,
                Command::FetchLeverage {
                    command: 77,
                    symbol: "BTC-USD".into(),
                },
            );
        }
        assert_eq!(actor.pending.len(), 2);
        for _ in 0..2 {
            actor.host.engine.handle(
                1.,
                Event::WsMessage {
                    socket: SocketId(0),
                    text: "1",
                },
            );
            actor.execute();
            let Ok(Outgoing::Text(message)) = receivers[0].try_recv() else {
                panic!("receipt")
            };
            assert_eq!(
                message.as_ref(),
                r#"{"type":"commandProgress","command":77,"receipts":[{"symbol":"BTC-USD","kind":"clientOrderId","identity":"native-1"}]}"#
            );
            assert!(receivers[1].try_recv().is_err());
            assert_eq!(actor.pending.len(), 2, "progress must not complete routing");
        }
        actor.leave(1);
        assert_eq!(actor.pending.get(&1).unwrap().client, None);
        actor.host.engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "1",
            },
        );
        actor.execute();
        assert!(
            receivers[1].try_recv().is_err(),
            "departed owner's identity cannot migrate"
        );
        actor.host.engine.handle(
            2.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "2",
            },
        );
        actor.execute();
        let Ok(Outgoing::Text(message)) = receivers[1].try_recv() else {
            panic!("second owner receipt")
        };
        assert!(message.contains("native-2"));
        actor.host.engine.handle(
            3.,
            Event::WsOpened {
                socket: SocketId(2),
            },
        );
        actor.execute();
        let Ok(Outgoing::Text(message)) = receivers[1].try_recv() else {
            panic!("done")
        };
        assert_eq!(
            message.as_ref(),
            r#"{"type":"done","command":77,"result":"[]"}"#
        );
        assert!(!actor.pending.contains_key(&2));
        actor.host.engine.handle(
            4.,
            Event::WsMessage {
                socket: SocketId(0),
                text: "2",
            },
        );
        actor.execute();
        assert!(
            receivers[1].try_recv().is_err(),
            "completion closes receipt routing"
        );
    }

    #[tokio::test]
    async fn full_peer_queue_cancels_and_releases_interest_without_dropping_a_delta() {
        let commands = Rc::new(RefCell::new(Vec::new()));
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(commands.clone())),
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
                account_auth: false,
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

    fn sectioned_frame(payloads: &[usize]) -> Vec<u8> {
        let names: Vec<String> = (0..payloads.len()).map(|i| format!("symbol-{i}")).collect();
        let mut w = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 7.0);
        for (i, bytes) in payloads.iter().enumerate() {
            w.begin(100 + i as u16);
            let name = w.string(&names[i]);
            w.u32(name);
            w.f64s(std::iter::repeat_n(i as f64, bytes / 8));
            w.end();
        }
        w.finish()
    }

    fn sections_of(frames: &[&[u8]]) -> Vec<(u16, Vec<u8>)> {
        frames
            .iter()
            .flat_map(|frame| safe_cex_engine::wire::sections(frame))
            .filter(|(kind, _)| *kind != safe_cex_engine::wire::section::STRINGS)
            .map(|(kind, payload)| (kind, payload.to_vec()))
            .collect()
    }

    #[test]
    fn oversized_frames_split_into_ordered_segments_sharing_the_string_table() {
        let frame = sectioned_frame(&[400, 400, 400, 400]);
        let max = frame.len() / 2;
        let segments = segment_frame(&frame, max).expect("every section fits alone");
        assert!(segments.len() >= 2);
        let strings = safe_cex_engine::wire::sections(&frame)
            .find(|(kind, _)| *kind == safe_cex_engine::wire::section::STRINGS)
            .unwrap()
            .1;
        for segment in &segments {
            assert!(segment.len() <= max);
            assert_eq!(segment[..4], frame[..4]);
            assert_eq!(
                segment[8..16],
                frame[8..16],
                "segments keep the frame clock"
            );
            assert_eq!(
                safe_cex_engine::wire::sections(segment)
                    .find(|(kind, _)| *kind == safe_cex_engine::wire::section::STRINGS)
                    .unwrap()
                    .1,
                strings
            );
        }
        let parts: Vec<&[u8]> = segments.iter().map(|s| s.as_ref()).collect();
        assert_eq!(sections_of(&parts), sections_of(&[&frame]));
        let one_section = sectioned_frame(&[4000]);
        assert!(segment_frame(&one_section, one_section.len() - 1).is_none());
        assert!(
            segment_frame(&frame[..frame.len() - 1], max).is_none(),
            "malformed"
        );
    }

    fn kinded_frame(sections: &[(u16, usize)]) -> Vec<u8> {
        let mut w = safe_cex_engine::wire::FrameWriter::new(Vec::new(), 3.0);
        for (kind, bytes) in sections {
            w.begin(*kind);
            let name = w.string("BTC");
            w.u32(name);
            w.f64s(std::iter::repeat_n(1.0, bytes / 8));
            w.end();
        }
        w.finish()
    }

    #[test]
    fn segments_keep_dependent_sections_with_their_parent_and_status_last() {
        use safe_cex_engine::wire::section::*;
        let frame = kinded_frame(&[
            (STATUS, 64),
            (SETTINGS, 64),
            (MARKETS, 400),
            (PREDICTION_MARKETS, 300),
            (INSTRUMENT_CLASSIFICATIONS, 300),
            (TICKERS, 400),
            (POSITIONS, 400),
            (POSITION_GREEKS, 300),
            (PREDICTION_HOLDINGS, 300),
            (BOOK, 400),
        ]);
        let segments = segment_frame(&frame, 1_300).expect("units fit");
        assert!(segments.len() >= 3);
        let kinds: Vec<Vec<u16>> = segments
            .iter()
            .map(|segment| {
                safe_cex_engine::wire::sections(segment)
                    .map(|(kind, _)| kind)
                    .filter(|kind| *kind != STRINGS)
                    .collect()
            })
            .collect();
        let segment_of = |wanted: u16| kinds.iter().position(|k| k.contains(&wanted)).unwrap();
        for (parent, child) in [
            (MARKETS, PREDICTION_MARKETS),
            (MARKETS, INSTRUMENT_CLASSIFICATIONS),
            (POSITIONS, POSITION_GREEKS),
            (POSITIONS, PREDICTION_HOLDINGS),
        ] {
            assert_eq!(
                segment_of(parent),
                segment_of(child),
                "{child} stays with {parent}"
            );
        }
        assert_eq!(
            segment_of(STATUS),
            kinds.len() - 1,
            "loaded flags follow their data"
        );
        assert_eq!(
            segment_of(SETTINGS),
            segment_of(STATUS),
            "settings read the hedge flag"
        );
        // A unit larger than the limit, or a dependent without its parent, cannot be split.
        assert!(segment_frame(&frame, 1_000).is_none());
        let orphan = kinded_frame(&[(TICKERS, 400), (POSITION_GREEKS, 400), (BOOK, 400)]);
        assert!(segment_frame(&orphan, 900).is_none());
    }

    fn peer(
        actor: &mut Account<Stub, NoTransport>,
        client: u64,
    ) -> (mpsc::Receiver<Outgoing>, watch::Receiver<bool>) {
        let (outgoing, frames) = mpsc::channel(32);
        let (cancel, canceled) = watch::channel(false);
        actor.peers.insert(
            client,
            Peer {
                outgoing,
                cancel,
                books: HashSet::new(),
                candles: HashSet::new(),
                tickers: None,
                account_auth: false,
            },
        );
        (frames, canceled)
    }

    fn stub_account(max_frame_bytes: usize) -> Account<Stub, NoTransport> {
        Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(Rc::new(RefCell::new(
                    Vec::new(),
                )))),
                NoTransport,
            ),
            ServerConfig {
                max_frame_bytes,
                ..ServerConfig::default()
            },
        )
    }

    #[test]
    fn one_oversized_frame_reaches_every_peer_in_segments_without_disconnecting() {
        let frame = sectioned_frame(&[400, 400, 400]);
        let mut actor = stub_account(frame.len() - 100);
        let mut peers = [peer(&mut actor, 1), peer(&mut actor, 2)];
        actor.broadcast(Outgoing::Frame(frame.clone().into()));
        assert_eq!(actor.peers.len(), 2, "no peer is dropped");
        for (frames, canceled) in &mut peers {
            assert!(!*canceled.borrow());
            let mut received = Vec::new();
            while let Ok(Outgoing::Frame(segment)) = frames.try_recv() {
                received.push(segment);
            }
            assert!(received.len() >= 2);
            let parts: Vec<&[u8]> = received.iter().map(|s| s.as_ref()).collect();
            assert_eq!(sections_of(&parts), sections_of(&[&frame]));
        }
        // A section that cannot fit alone leaves no deliverable delta: peers resnapshot.
        let unsplittable = sectioned_frame(&[actor.config.max_frame_bytes]);
        actor.broadcast(Outgoing::Frame(unsplittable.into()));
        assert!(actor.peers.is_empty());
        assert!(peers.iter().all(|(_, canceled)| *canceled.borrow()));
    }

    #[test]
    fn oversized_notices_are_replaced_for_that_message_only() {
        let mut actor = stub_account(256);
        let (mut a, a_canceled) = peer(&mut actor, 1);
        let (mut b, b_canceled) = peer(&mut actor, 2);
        let huge = "x".repeat(1_000);
        actor.broadcast(Outgoing::Text(encode_notice(ServerNotice::Event {
            level: "error",
            code: "test.diagnostic",
            fields: NoticeFields(&[]),
            message: &huge,
        })));
        for frames in [&mut a, &mut b] {
            let Ok(Outgoing::Text(text)) = frames.try_recv() else {
                panic!("peer receives the replacement warning");
            };
            assert!(text.len() <= 256 && text.contains("native message limit"));
            assert!(!text.contains(&huge));
        }
        let done = actor.bounded_done(
            9,
            encode_notice(ServerNotice::Done {
                command: 9,
                result: Some(&huge),
                error: None,
                detail: None,
            }),
        );
        actor.send(1, Outgoing::Text(done));
        let Ok(Outgoing::Text(text)) = a.try_recv() else {
            panic!("the command still settles");
        };
        let done: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(done["command"], 9);
        assert!(done.get("result").is_none());
        assert!(
            done["error"]
                .as_str()
                .unwrap()
                .contains("exceeds the 256-byte")
        );
        assert_eq!(actor.peers.len(), 2);
        assert!(!*a_canceled.borrow() && !*b_canceled.borrow());
        assert!(
            b.try_recv().is_err(),
            "a per-client completion stays per-client"
        );
    }

    struct FailsStartup;
    impl Venue for FailsStartup {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            cx.startup_failed(
                VenueError::new("key rejected").kind(safe_cex_engine::ErrorKind::AuthInvalid),
            );
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }

    #[tokio::test]
    async fn a_startup_failure_reaches_clients_that_join_later() {
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(FailsStartup),
                NoTransport,
            ),
            ServerConfig::default(),
        );
        let now = (actor.host.clock)();
        actor.host.engine.start(now);
        actor.execute();
        let (outgoing, mut messages) = mpsc::channel(4);
        actor
            .join(7, outgoing, watch::channel(false).0, false)
            .unwrap();
        assert!(matches!(messages.recv().await, Some(Outgoing::Frame(_))));
        let Some(Outgoing::Text(text)) = messages.recv().await else {
            panic!("startup failure was not replayed");
        };
        assert_eq!(
            text.as_ref(),
            r#"{"type":"startupFailed","detail":{"message":"key rejected","kind":"authInvalid"}}"#
        );
    }

    #[tokio::test]
    async fn oversized_log_notice_is_truncated_instead_of_disconnecting_peers() {
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(Rc::default())),
                NoTransport,
            ),
            ServerConfig {
                max_frame_bytes: 96,
                ..ServerConfig::default()
            },
        );
        let (outgoing, mut frames) = mpsc::channel(4);
        let (cancel, canceled) = watch::channel(false);
        actor.peers.insert(
            1,
            Peer {
                outgoing,
                cancel,
                books: HashSet::new(),
                candles: HashSet::new(),
                tickers: None,
                account_auth: false,
            },
        );
        // The engine cap is larger than this client frame limit.
        actor
            .host
            .engine
            .report_public_feed_error(&"\"é".repeat(200));
        actor.execute();
        assert!(!*canceled.borrow() && actor.peers.contains_key(&1));
        let Some(Outgoing::Text(text)) = frames.recv().await else {
            panic!("expected a log notice");
        };
        assert!(text.len() <= 96, "{} bytes", text.len());
        let notice: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(notice["type"], "event");
        assert!(
            notice["message"].as_str().unwrap().starts_with("\"é"),
            "{text}"
        );
    }

    #[tokio::test]
    async fn dropping_account_aborts_timers_and_closes_transport_input() {
        let mut actor = Account::new(
            Host::new(
                safe_cex_engine::test_support::seeded_engine(Stub(Rc::default())),
                NoTransport,
            ),
            ServerConfig::default(),
        );
        let timer = tokio::spawn(std::future::pending::<()>());
        actor.host.timers.insert(TimerId(1), timer.abort_handle());
        let events = actor.host.events.clone();
        drop(actor);
        assert!(events.send(HostEvent::Timer(TimerId(1))).is_err());
        assert!(timer.await.unwrap_err().is_cancelled());
    }

    #[cfg(feature = "venues")]
    #[tokio::test]
    async fn venues_are_registered_by_name_after_their_construction_rules() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let mut server = FrameServer::new(ServerConfig::default()).unwrap();
                server
                    .add_venue(
                        "a",
                        "token-a",
                        "binance",
                        r#"{"publicDataOnly":true}"#,
                        NoTransport,
                    )
                    .unwrap();
                let (signer, identities) =
                    safe_cex_crypto::signer::LocalSigner::from_setup(Default::default()).unwrap();
                let setup = serde_json::json!({ "publicDataOnly": true, "signing": identities })
                    .to_string();
                server
                    .add_venue_with_signer(
                        "injected",
                        "token-injected",
                        "binance",
                        &setup,
                        NoTransport,
                        Arc::new(signer),
                    )
                    .unwrap();
                for (venue, options, message) in [
                    ("nope", "{}", "unsupported venue: nope"),
                    (
                        "weex",
                        r#"{"key":"k","secret":"s"}"#,
                        "Invalid local signer setup",
                    ),
                ] {
                    let error = server
                        .add_venue("b", "token-b", venue, options, NoTransport)
                        .unwrap_err();
                    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
                    assert_eq!(error.to_string(), message);
                }
                assert!(
                    server
                        .add_venue("b", "token-b", "binance", "not json", NoTransport)
                        .is_err()
                );
            })
            .await;
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
                let host = || {
                    Host::new(
                        safe_cex_engine::test_support::seeded_engine(Stub(Rc::default())),
                        NoTransport,
                    )
                };
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
