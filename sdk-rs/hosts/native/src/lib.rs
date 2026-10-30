//! Native host: runs an [`Engine`] on tokio for servers and the CLI. The
//! engine stays single-threaded and synchronous; this loop owns the sockets,
//! HTTP and timers, feeds their results in as events and executes the
//! actions that come out. Transports are pluggable so recorded traffic can
//! drive the same loop as the network.
#![cfg(not(target_arch = "wasm32"))]

mod managed_http;
mod routing;
mod server;
mod socket_health;
mod transport;

pub use routing::resolve;
pub use routing::{
    HttpRoute, HttpRouteMode, ProxyTokenFuture, ProxyTokenProvider, RoutingPlan, SocketChannel,
    SocketChannelRule, WsRoute, WsRoutes,
};
#[cfg(feature = "venues")]
pub use server::spawn_venue_account;
pub use server::{
    AccountHandle, BookLevels, ClientLink, FrameServer, InputRefused, Outgoing, ServerConfig,
    spawn_account,
};
pub use socket_health::{
    SOCKET_STALE_MS, SocketDiagnostics, SocketHealth, SocketStats, SocketSummary,
};
pub use transport::{
    BoundHttp, HttpPrepareError, HttpPrepareFuture, NetworkTransport, PreparedHttp, Transport,
};
#[cfg(feature = "nonce-drain")]
pub use transport::{DrainFuture, Drained};

use safe_cex_engine::{
    Action, Command, Engine, Event, Notice, RequestId, SocketId, TimerId, Venue, VenueError,
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

/// What transports and timers report back to the loop.
#[derive(Debug)]
pub enum HostEvent {
    PollScaleChanged(f64),
    /// Trusted host/provider ingress; network command clients cannot send it.
    PublicFeedUpdate(safe_cex_engine::PublicFeedUpdate),
    WsOpened(SocketId),
    WsMessage(SocketId, String),
    WsBinary(SocketId, Vec<u8>),
    WsClosed(SocketId, u16),
    /// Every route was refused for a `reject_route` connect; nothing was dialled.
    WsRouteExhausted(SocketId),
    HttpStart {
        request: RequestId,
        status: u16,
        headers: String,
    },
    HttpChunk(RequestId, Vec<u8>),
    HttpEnd(RequestId),
    HttpFailed(RequestId, String),
    HttpPrepared(
        safe_cex_engine::request_context::Preparation,
        Result<Box<dyn PreparedHttp>, HttpPrepareError>,
    ),
    HttpWake(
        safe_cex_engine::request_context::Preparation,
        safe_cex_engine::request_context::Permit,
    ),
    #[cfg(feature = "http-mutation-barriers")]
    HttpGroupWake(safe_cex_engine::request_context::GroupPermit),
    Signed(
        RequestId,
        Result<safe_cex_engine::signing::SignResult, safe_cex_engine::signing::SignError>,
    ),
    Timer(TimerId),
}

pub type Events = mpsc::UnboundedSender<HostEvent>;

/// Wall-clock milliseconds, the engine's notion of now.
pub fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Cryptographic production providers; replay hosts explicitly replace identity
/// entropy, and signing entropy only when a test deliberately requests it.
pub fn production_entropy() -> safe_cex_engine::entropy::Entropy {
    fn fill(bytes: &mut [u8]) -> Result<(), String> {
        getrandom::getrandom(bytes).map_err(|error| error.to_string())
    }
    safe_cex_engine::entropy::Entropy::new(Box::new(fill), Box::new(fill))
}

/// Only the engine's sticky resource failure changes a panic into a terminal
/// host result. Venue, transport and consumer bugs retain their panic behavior.
fn engine_turn<S, R>(
    state: &mut S,
    operation: impl FnOnce(&mut S) -> R,
    failed: impl Fn(&S) -> bool,
) -> Result<R, VenueError> {
    fn terminal() -> VenueError {
        VenueError::new("Engine resource identities exhausted or invalid; recreate the engine")
            .kind(safe_cex_engine::ErrorKind::Internal)
            .outcome(safe_cex_engine::Outcome::Unknown)
    }
    if failed(state) {
        return Err(terminal());
    }
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(state))) {
        Ok(value) if !failed(state) => Ok(value),
        Ok(_) => Err(terminal()),
        Err(_) if failed(state) => Err(terminal()),
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

pub struct Host<V: Venue, T: Transport> {
    engine: Engine<V>,
    transport: T,
    events: Events,
    inbox: mpsc::UnboundedReceiver<HostEvent>,
    sockets: HashMap<SocketId, mpsc::UnboundedSender<String>>,
    socket_health: socket_health::SocketTracker,
    timers: HashMap<TimerId, AbortHandle>,
    managed_http: managed_http::Requests,
    signing_tasks: HashMap<RequestId, managed_http::Task>,
    clock: fn() -> f64,
    terminal: Option<VenueError>,
    terminal_reported: bool,
}

#[cfg(feature = "address-nonce")]
struct SetupRunOwner<V: Venue, T: Transport> {
    host: Option<Host<V, T>>,
    cleanup: Option<std::pin::Pin<Box<safe_cex_engine::AddressSetupCleanup>>>,
}
#[cfg(feature = "address-nonce")]
impl<V: Venue, T: Transport> std::ops::Deref for SetupRunOwner<V, T> {
    type Target = Host<V, T>;
    fn deref(&self) -> &Self::Target {
        self.host.as_ref().expect("setup host owner is live")
    }
}
#[cfg(feature = "address-nonce")]
impl<V: Venue, T: Transport> std::ops::DerefMut for SetupRunOwner<V, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.host.as_mut().expect("setup host owner is live")
    }
}
#[cfg(feature = "address-nonce")]
impl<V: Venue, T: Transport> Drop for SetupRunOwner<V, T> {
    fn drop(&mut self) {
        let cleanup = self.cleanup.take().or_else(|| {
            self.host.as_mut().map(|host| {
                host.stop();
                Box::pin(host.engine.take_address_setup_cleanup())
            })
        });
        if let Some(mut cleanup) = cleanup {
            use std::future::Future;
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            match cleanup.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(Ok(())) => return,
                std::task::Poll::Ready(Err(_)) => {
                    eprintln!("Address setup cleanup failed after host cancellation");
                    return;
                }
                std::task::Poll::Pending => {}
            }
            // Cancellation transfers ownership, never retirement evidence. The
            // runtime must stay alive until this actual cleanup driver finishes.
            tokio::spawn(async move {
                if cleanup.await.is_err() {
                    eprintln!("Address setup cleanup failed after host cancellation");
                }
            });
        }
    }
}

impl<V: Venue, T: Transport> Host<V, T> {
    pub fn new(engine: Engine<V>, transport: T) -> Self {
        let (events, inbox) = mpsc::unbounded_channel();
        Self {
            engine,
            transport,
            events,
            inbox,
            sockets: HashMap::new(),
            socket_health: Default::default(),
            timers: HashMap::new(),
            managed_http: Default::default(),
            signing_tasks: HashMap::new(),
            clock: now_ms,
            terminal: None,
            terminal_reported: false,
        }
    }

    /// Host-owned provider handle; key material stays inside the implementation.
    pub fn with_signer(
        mut self,
        signer: safe_cex_engine::signing::Provider,
    ) -> Result<Self, safe_cex_engine::VenueError> {
        self.engine.set_signer(Some(signer))?;
        Ok(self)
    }

    pub fn with_entropy(
        mut self,
        fill: safe_cex_engine::entropy::Fill,
    ) -> Result<Self, safe_cex_engine::VenueError> {
        self.engine.set_entropy(fill)?;
        Ok(self)
    }
    pub fn with_signing_entropy(
        mut self,
        fill: safe_cex_engine::entropy::Fill,
    ) -> Result<Self, safe_cex_engine::VenueError> {
        self.engine.set_signing_entropy(fill)?;
        Ok(self)
    }

    /// Read-only metadata of the current managed signing task; see
    /// `Engine::sign_meta`. Descriptive only.
    #[cfg(feature = "sign-meta")]
    pub fn sign_meta(&self, task: RequestId) -> Option<safe_cex_engine::sign_meta::SignMeta> {
        self.engine.sign_meta(task)
    }

    #[cfg(feature = "address-nonce")]
    pub fn initialize_address_setup(
        &mut self,
    ) -> Result<safe_cex_engine::AddressSetupProgress, VenueError> {
        self.engine.initialize_address_nonce((self.clock)())
    }
    #[cfg(feature = "address-nonce")]
    pub fn poll_address_setup(&mut self, cx: &mut std::task::Context<'_>) -> bool {
        self.engine.poll_address_setup((self.clock)(), cx);
        self.engine.address_nonce_setup_ready()
    }
    /// Feed live configuration alongside transport completions.
    pub fn event_sender(&self) -> Events {
        self.events.clone()
    }

    /// Run until `commands` closes, handing each flush frame to `on_frame`
    /// and every log line or command result to `on_notice`. Returns the
    /// engine for inspection.
    // This must construct the cleanup owner before even the first future poll.
    #[allow(clippy::manual_async_fn)]
    pub fn run(
        self,
        commands: mpsc::UnboundedReceiver<Command>,
        flush_every: Duration,
        mut on_frame: impl FnMut(&[u8]),
        on_notice: impl FnMut(Notice),
    ) -> impl std::future::Future<Output = Engine<V>> {
        self.run_with_store(
            commands,
            flush_every,
            move |frame, _| on_frame(frame),
            on_notice,
        )
    }

    /// [`Self::run`], also handing each flush frame's callback the engine store
    /// it was taken from, for embedders that read typed state (markets) directly.
    #[allow(clippy::manual_async_fn)]
    pub fn run_with_store(
        self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        flush_every: Duration,
        mut on_frame: impl FnMut(&[u8], &safe_cex_engine::Store),
        mut on_notice: impl FnMut(Notice),
    ) -> impl std::future::Future<Output = Engine<V>> {
        #[cfg(feature = "address-nonce")]
        let mut retirement = SetupRunOwner {
            host: Some(self),
            cleanup: None,
        };
        async move {
            #[cfg(feature = "address-nonce")]
            {
                use futures_util::FutureExt;
                let result = std::panic::AssertUnwindSafe(retirement.host.as_mut().unwrap().drive(
                    &mut commands,
                    flush_every,
                    &mut on_frame,
                    &mut on_notice,
                ))
                .catch_unwind()
                .await;
                retirement.host.as_mut().unwrap().stop();
                retirement.cleanup = Some(Box::pin(
                    retirement
                        .host
                        .as_mut()
                        .unwrap()
                        .engine
                        .take_address_setup_cleanup(),
                ));
                let cleanup = retirement.cleanup.as_mut().unwrap().await;
                retirement.cleanup = None;
                let mut host = retirement.host.take().unwrap();
                if let Err(panic) = result {
                    std::panic::resume_unwind(panic);
                }
                host.report_terminal(&mut on_notice);
                if cleanup.is_err() {
                    on_notice(Notice::StartupFailed {
                        error: VenueError::new("Address setup cleanup failed")
                            .kind(safe_cex_engine::ErrorKind::Internal),
                    });
                }
                host.engine
            }
            #[cfg(not(feature = "address-nonce"))]
            {
                let mut host = self;
                host.drive(&mut commands, flush_every, &mut on_frame, &mut on_notice)
                    .await;
                host.report_terminal(&mut on_notice);
                host.engine
            }
        }
    }

    async fn drive(
        &mut self,
        commands: &mut mpsc::UnboundedReceiver<Command>,
        flush_every: Duration,
        on_frame: &mut impl FnMut(&[u8], &safe_cex_engine::Store),
        on_notice: &mut impl FnMut(Notice),
    ) {
        self.turn(|host| {
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
        self.execute(on_notice);
        let mut flush = tokio::time::interval(flush_every);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        while self.terminal.is_none() {
            tokio::select! {
                Some(event) = self.inbox.recv() => self.dispatch(event),
                command = commands.recv() => match command {
                    Some(command) => { self.turn(|host| host.engine.handle((host.clock)(), Event::Command(command))); },
                    None => break,
                },
                _ = flush.tick() => {
                    self.frame(on_frame);
                }
            }
            self.execute(on_notice);
        }
        if self.terminal.is_none() {
            self.frame(on_frame);
        }
        self.stop();
    }

    fn turn<R>(&mut self, operation: impl FnOnce(&mut Self) -> R) -> Option<R> {
        if self.terminal.is_some() {
            return None;
        }
        match engine_turn(self, operation, |host| host.engine.failed()) {
            Ok(value) => Some(value),
            Err(error) => {
                self.terminal = Some(error);
                self.stop();
                None
            }
        }
    }

    fn report_terminal(&mut self, on_notice: &mut impl FnMut(Notice)) {
        if !self.terminal_reported
            && let Some(error) = &self.terminal
        {
            self.terminal_reported = true;
            // Deliberately outside the panic classifier: consumer failures are
            // never disguised as another engine terminal result.
            on_notice(Notice::StartupFailed {
                error: error.clone(),
            });
        }
    }

    fn frame(&mut self, on_frame: &mut impl FnMut(&[u8], &safe_cex_engine::Store)) {
        // Own the result before calling application code, outside the guard.
        let frame = self.turn(|host| host.engine.take_frame((host.clock)()).map(Vec::from));
        if let Some(Some(frame)) = frame {
            on_frame(&frame, self.engine.store());
        }
    }

    fn stop(&mut self) {
        self.inbox.close();
        self.managed_http.clear();
        self.signing_tasks.clear();
        self.sockets.clear();
        self.socket_health.clear();
        for (_, timer) in self.timers.drain() {
            timer.abort();
        }
    }

    fn dispatch(&mut self, event: HostEvent) {
        self.turn(|host| host.dispatch_inner(event));
    }

    fn dispatch_inner(&mut self, event: HostEvent) {
        let now = (self.clock)();
        let engine = &mut self.engine;
        match event {
            HostEvent::HttpPrepared(preparation, result) => {
                self.complete_http_preparation(preparation, result);
            }
            HostEvent::HttpWake(preparation, permit) => {
                self.wake_http(preparation, permit);
            }
            #[cfg(feature = "http-mutation-barriers")]
            HostEvent::HttpGroupWake(permit) => self.claim_http_group_resources(permit),
            HostEvent::PublicFeedUpdate(update) => {
                if let Err(error) = engine.apply_public_feed(now, update) {
                    // A configuration error is observable through the normal notice path.
                    engine.report_public_feed_error(error);
                }
            }
            HostEvent::PollScaleChanged(scale) => {
                engine.handle(now, Event::PollScaleChanged { scale })
            }
            HostEvent::WsOpened(socket) => {
                self.socket_health.opened(socket);
                engine.handle(now, Event::WsOpened { socket })
            }
            HostEvent::WsBinary(socket, data) => {
                self.socket_health.received(socket);
                engine.handle(
                    now,
                    Event::WsBinary {
                        socket,
                        data: &data,
                    },
                )
            }
            HostEvent::WsMessage(socket, text) => {
                self.socket_health.received(socket);
                engine.handle(
                    now,
                    Event::WsMessage {
                        socket,
                        text: &text,
                    },
                )
            }
            HostEvent::WsClosed(socket, code) => {
                self.sockets.remove(&socket);
                self.socket_health.closed(socket, Some(code));
                engine.handle(now, Event::WsClosed { socket, code })
            }
            HostEvent::WsRouteExhausted(socket) => {
                self.sockets.remove(&socket);
                self.socket_health.closed(socket, None);
                engine.handle(now, Event::WsRouteExhausted { socket })
            }
            HostEvent::HttpStart {
                request,
                status,
                headers,
            } => engine.handle(
                now,
                Event::HttpStart {
                    request,
                    status,
                    headers: &headers,
                },
            ),
            HostEvent::HttpChunk(request, data) => engine.handle(
                now,
                Event::HttpChunk {
                    request,
                    data: &data,
                },
            ),
            HostEvent::HttpEnd(request) => engine.handle(now, Event::HttpEnd { request }),
            HostEvent::HttpFailed(request, error) => engine.handle(
                now,
                Event::HttpFailed {
                    request,
                    error: &error,
                },
            ),
            HostEvent::Signed(attempt, result) => {
                self.signing_tasks.remove(&attempt);
                engine.handle(now, Event::Signed { attempt, result })
            }
            HostEvent::Timer(timer) => {
                self.timers.remove(&timer);
                engine.handle(now, Event::TimerFired { timer })
            }
        }
    }

    fn execute(&mut self, on_notice: &mut impl FnMut(Notice)) {
        while self.execute_batch(on_notice) {}
    }

    fn execute_batch(&mut self, on_notice: &mut impl FnMut(Notice)) -> bool {
        let Some(actions) = self.turn(|host| {
            host.engine.prepare_dispatch((host.clock)());
            host.engine.drain_actions().collect::<Vec<Action>>()
        }) else {
            return false;
        };
        let had_actions = !actions.is_empty();
        for action in actions {
            if self.terminal.is_some() {
                break;
            }
            match action {
                Action::WsConnect {
                    binary,
                    socket,
                    url,
                    headers,
                    channel,
                    context,
                } => {
                    self.socket_health.connecting(socket, channel.clone());
                    let outgoing = self.transport.connect_encoding(
                        binary,
                        socket,
                        url,
                        headers,
                        channel,
                        context,
                        self.events.clone(),
                    );
                    self.sockets.insert(socket, outgoing);
                }
                Action::WsSend { socket, text } => {
                    if let Some(outgoing) = self.sockets.get(&socket) {
                        let _ = outgoing.send(text);
                    }
                }
                // Dropping the sender closes the socket.
                Action::WsClose { socket } => {
                    self.sockets.remove(&socket);
                    self.socket_health.closed(socket, None);
                }
                Action::Http(request) => self.transport.request(request, self.events.clone()),
                Action::ManagedHttp(action) => {
                    self.turn(|host| host.execute_managed_http(*action));
                }
                Action::SignedOutbound { .. } => {
                    unreachable!("engine drain unwraps signed outbound")
                }
                Action::RetireAddressSetup { .. } => {
                    self.terminal = Some(
                        VenueError::new("Unsupported external address setup retirement")
                            .kind(safe_cex_engine::ErrorKind::Internal),
                    );
                    self.stop();
                    return false;
                }
                Action::Sign {
                    attempt,
                    work,
                    timeout_ms,
                } => {
                    if !self.engine.signing_pending(attempt) {
                        continue;
                    }
                    let signer = self.engine.signer();
                    let events = self.events.clone();
                    let task = tokio::spawn(async move {
                        use safe_cex_engine::signing::SignError;
                        if events.is_closed() {
                            return;
                        }
                        let result = match signer {
                            Some(signer) => tokio::select! {
                                biased;
                                _ = events.closed() => return,
                                result = tokio::time::timeout(
                                    Duration::from_millis(u64::from(timeout_ms)),
                                    signer.sign(work),
                                ) => result.unwrap_or(Err(SignError::TimedOut)),
                            },
                            None => Err(SignError::Unavailable),
                        };
                        let _ = events.send(HostEvent::Signed(attempt, result));
                    });
                    self.signing_tasks
                        .insert(attempt, managed_http::Task(task.abort_handle()));
                }
                Action::SetTimer { timer, at_ms } => {
                    let delay = (at_ms - (self.clock)()).max(0.0);
                    let deadline = Duration::try_from_secs_f64(delay / 1000.0)
                        .ok()
                        .and_then(|duration| tokio::time::Instant::now().checked_add(duration));
                    let events = self.events.clone();
                    let handle = tokio::spawn(async move {
                        if let Some(deadline) = deadline {
                            tokio::time::sleep_until(deadline).await;
                        } else {
                            // A valid positive venue interval may exceed the OS clock.
                            std::future::pending::<()>().await;
                        }
                        let _ = events.send(HostEvent::Timer(timer));
                    });
                    if let Some(previous) = self.timers.insert(timer, handle.abort_handle()) {
                        previous.abort();
                    }
                }
                Action::ClearTimer { timer } => {
                    if let Some(handle) = self.timers.remove(&timer) {
                        handle.abort();
                    }
                }
                Action::Notice(notice) => on_notice(notice),
            }
        }
        had_actions
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;
    use safe_cex_engine::{Cx, HttpRequest, Method, Response, book::Book};

    #[derive(Default)]
    struct TimerVenue {
        timer: Option<TimerId>,
        fired: Vec<TimerId>,
    }
    impl Venue for TimerVenue {
        type Pending = ();
        fn start(&mut self, cx: &mut Cx<'_, ()>) {
            self.timer = Some(cx.set_timer(1e308));
        }
        fn on_poll_scale(&mut self, cx: &mut Cx<'_, ()>, scale: f64) {
            if let Some(timer) = self.timer.take() {
                cx.clear_timer(timer);
            }
            // A 1 s native cadence, armed as a raw venue timer.
            if scale.is_finite() {
                self.timer = Some(cx.set_timer(1_000. * scale));
            }
        }
        fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
        fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
        fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
        fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
        fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
        fn on_timer(&mut self, _: &mut Cx<'_, ()>, timer: TimerId) {
            self.fired.push(timer);
        }
        fn on_command(&mut self, cx: &mut Cx<'_, ()>, command: Command) {
            if let Command::FetchLeverage { command, symbol } = command {
                cx.command_progress(command, Vec::new());
                cx.command_progress(
                    command,
                    vec![safe_cex_engine::OrderIdentityReceipt {
                        symbol,
                        kind: safe_cex_engine::OrderIdentityKind::ClientOrderId,
                        identity: "native-receipt".into(),
                    }],
                );
                cx.request(
                    Method::Post,
                    "https://example.test/orders".into(),
                    vec![],
                    None,
                    (),
                );
            }
        }
        fn book(&self, _: &str) -> Option<&Book> {
            None
        }
        fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
            Box::new(std::iter::empty())
        }
    }
    struct NoNetwork;
    impl Transport for NoNetwork {
        fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
            panic!("timer venue never opens a socket")
        }
        fn request(&self, _: HttpRequest, _: Events) {
            panic!("timer venue never requests HTTP")
        }
    }

    #[test]
    fn native_host_observes_command_receipts_before_transport_dispatch() {
        use std::cell::RefCell;
        use std::rc::Rc;
        struct Recorded(Rc<RefCell<Vec<&'static str>>>);
        impl Transport for Recorded {
            fn connect(&self, _: SocketId, _: String, _: Events) -> mpsc::UnboundedSender<String> {
                panic!("receipt test never opens a socket")
            }
            fn request(&self, _: HttpRequest, _: Events) {
                self.0.borrow_mut().push("http");
            }
        }
        let order = Rc::new(RefCell::new(Vec::new()));
        let mut host = Host::new(
            safe_cex_engine::test_support::seeded_engine(TimerVenue::default()),
            Recorded(order.clone()),
        );
        host.engine.handle(
            0.,
            Event::Command(Command::FetchLeverage {
                command: 9,
                symbol: "BTC-USD".into(),
            }),
        );
        host.execute(&mut |notice| {
            assert!(
                matches!(notice, Notice::CommandProgress { command: 9, ref receipts }
                if receipts.len() == 1 && receipts[0].identity == "native-receipt")
            );
            order.borrow_mut().push("progress");
        });
        assert_eq!(*order.borrow(), ["progress", "http"]);
    }

    struct NeverStep;
    impl safe_cex_engine::SignStep for NeverStep {
        fn prepare(
            &mut self,
            _: &mut Action,
            _: &safe_cex_engine::DispatchContext<'_>,
        ) -> Result<safe_cex_engine::signing::SignRequest, safe_cex_engine::signing::SignError>
        {
            panic!("unsafe socket must fail before recipe preparation")
        }
        fn apply(
            &mut self,
            _: &mut Action,
            _: &safe_cex_engine::signing::SignResult,
        ) -> Result<(), safe_cex_engine::signing::SignError> {
            unreachable!()
        }
        fn freshness_ms(&self) -> f64 {
            1_000.
        }
    }

    async fn native_fatal_control(during_start: bool) {
        use std::sync::{Arc, Mutex};
        let venue =
            safe_cex_engine::test_support::ProbeVenue::new(TimerVenue::default(), move |_, cx| {
                cx.request(
                    Method::Get,
                    "https://venue.test/unsent".into(),
                    vec![],
                    None,
                    (),
                );
                let unsafe_socket = SocketId(safe_cex_engine::resource_ids::MAX_RESOURCE_ID + 1);
                if during_start {
                    cx.send_signed(unsafe_socket, "".into(), Box::new(NeverStep));
                } else {
                    cx.send(unsafe_socket, "");
                }
            });
        let host = Host::new(
            safe_cex_engine::test_support::seeded_engine(venue),
            NoNetwork,
        );
        let events = host.event_sender();
        let notices = Arc::new(Mutex::new(Vec::new()));
        let captured = notices.clone();
        let (commands, inbox) = mpsc::unbounded_channel();
        drop(commands);
        let event_check = events.clone();
        let result = tokio::spawn(async move {
            host.run(
                inbox,
                Duration::from_millis(100),
                |_| panic!("fatal engine cannot publish a frame"),
                move |notice| {
                    assert!(
                        event_check.is_closed(),
                        "resources must retire before terminal notification"
                    );
                    captured.lock().unwrap().push(notice);
                },
            )
            .await
        })
        .await;
        assert!(
            result.is_ok(),
            "sticky engine failure must use the typed terminal channel"
        );
        assert!(result.unwrap().failed());
        assert!(events.is_closed());
        let notices = notices.lock().unwrap();
        assert_eq!(notices.len(), 1);
        assert!(matches!(&notices[0], Notice::StartupFailed { error }
            if error.kind == Some(safe_cex_engine::ErrorKind::Internal)
            && error.outcome == Some(safe_cex_engine::Outcome::Unknown)));
    }

    #[tokio::test]
    async fn resource_failure_during_start_is_a_typed_terminal_native_failure() {
        native_fatal_control(true).await;
    }

    #[tokio::test]
    async fn unsafe_output_batch_is_a_typed_terminal_native_failure_without_partial_io() {
        native_fatal_control(false).await;
    }

    #[tokio::test]
    async fn ordinary_venue_startup_failure_does_not_latch_terminal_failure() {
        let venue =
            safe_cex_engine::test_support::ProbeVenue::new(TimerVenue::default(), |_, cx| {
                cx.startup_failed(safe_cex_engine::VenueError::new("fixture startup refusal"));
            });
        let host = Host::new(
            safe_cex_engine::test_support::seeded_engine(venue),
            NoNetwork,
        );
        let events = host.event_sender();
        let (commands, inbox) = mpsc::unbounded_channel();
        drop(commands);
        let mut notices = Vec::new();
        let engine = host
            .run(
                inbox,
                Duration::from_millis(100),
                |_| {},
                |notice| {
                    assert!(
                        !events.is_closed(),
                        "ordinary startup refusal preserves host lifecycle"
                    );
                    notices.push(notice);
                },
            )
            .await;
        assert!(!engine.failed());
        assert_eq!(notices.len(), 1);
        assert!(
            matches!(&notices[0], Notice::StartupFailed { error } if error.message == "fixture startup refusal")
        );
    }

    #[tokio::test]
    async fn unrelated_callback_panic_is_not_a_terminal_engine_failure() {
        let venue =
            safe_cex_engine::test_support::ProbeVenue::new(TimerVenue::default(), |_, _| {
                panic!("unrelated fixture callback panic");
            });
        let host = Host::new(
            safe_cex_engine::test_support::seeded_engine(venue),
            NoNetwork,
        );
        let (commands, inbox) = mpsc::unbounded_channel();
        drop(commands);
        let result = tokio::spawn(async move {
            host.run(
                inbox,
                Duration::from_millis(100),
                |_| {},
                |_| panic!("unrelated panic cannot become terminal notice"),
            )
            .await
        })
        .await;
        let error = result.err().expect("unrelated callback must still panic");
        assert!(error.is_panic());
        assert_eq!(
            error.into_panic().downcast_ref::<&str>(),
            Some(&"unrelated fixture callback panic")
        );
    }

    #[tokio::test(start_paused = true)]
    async fn huge_future_timer_stays_alive_until_cleared_then_normal_timers_fire() {
        let mut host = Host::new(
            safe_cex_engine::test_support::seeded_engine(TimerVenue::default()),
            NoNetwork,
        );
        host.clock = || 0.;
        // Exercise raw venue timers independently of the clamped polling control.
        host.engine.start(0.);
        host.execute(&mut |_| {});
        let parked = host.timers.values().next().unwrap().clone();
        // Let the spawned task run: a conversion panic would finish it while
        // leaving the host alive and silently losing this venue timer.
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(!parked.is_finished(), "huge timer must remain parked");
        assert!(host.inbox.try_recv().is_err());

        host.engine.handle(
            0.,
            Event::PollScaleChanged {
                scale: f64::INFINITY,
            },
        );
        host.execute(&mut |_| {});
        assert!(host.timers.is_empty(), "ClearTimer removes the handle");
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(parked.is_finished(), "ClearTimer aborts the parked task");

        host.engine
            .handle(0., Event::PollScaleChanged { scale: 1. });
        host.execute(&mut |_| {});
        let normal = *host.timers.keys().next().unwrap();
        let event = tokio::time::timeout(Duration::from_secs(2), host.inbox.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(event, HostEvent::Timer(timer) if timer == normal));
        host.dispatch(event);
        assert_eq!(host.engine.venue().fired, [normal]);
        assert!(host.timers.is_empty());
    }
}
