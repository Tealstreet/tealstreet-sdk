//! Native host: runs an [`Engine`] on tokio for servers and the CLI. The
//! engine stays single-threaded and synchronous; this loop owns the sockets,
//! HTTP and timers, feeds their results in as events and executes the
//! actions that come out. Transports are pluggable so recorded traffic can
//! drive the same loop as the network.
#![cfg(not(target_arch = "wasm32"))]

mod routing;
mod server;
#[cfg(test)]
mod tests;
mod transport;

pub use routing::{
    HttpRoute, HttpRouteMode, ProxyTokenFuture, ProxyTokenProvider, RoutingPlan, SocketChannel,
    SocketChannelRule, WsRoute, WsRoutes,
};
pub use server::{FrameServer, ServerConfig};
pub use transport::{
    NetworkTransport, PayloadSigner, PayloadSignerFuture, PayloadSignerResult, Transport,
};

use safe_cex_engine::{
    Action, Command, Engine, Event, Notice, RequestId, SocketId, TimerId, Venue,
};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;

/// What transports and timers report back to the loop.
#[derive(Debug)]
pub enum HostEvent {
    TickIntervalChanged(f64),
    /// Trusted host/provider ingress; network command clients cannot send it.
    PublicFeedUpdate(safe_cex_engine::PublicFeedUpdate),
    WsOpened(SocketId),
    WsMessage(SocketId, String),
    WsBinary(SocketId, Vec<u8>),
    WsClosed(SocketId, u16),
    HttpStart {
        request: RequestId,
        status: u16,
        headers: String,
    },
    HttpChunk(RequestId, Vec<u8>),
    HttpEnd(RequestId),
    HttpFailed(RequestId, String),
    PayloadSigned(RequestId, PayloadSignerResult),
    PayloadSignFailed(RequestId, String),
    Timer(TimerId),
}

const MAX_BATCH_MESSAGES: usize = 256;
const MAX_BATCH_BYTES: usize = 1024 * 1024;

enum QueuedInput {
    Ready(HostEvent),
    Deferred(Box<dyn FnOnce() -> Option<HostEvent> + Send>),
}

struct ReceivedEvent {
    event: HostEvent,
    received_at: f64,
    local_owner: Option<mpsc::WeakUnboundedSender<String>>,
}

struct QueuedEvent {
    event: QueuedInput,
    received_at: f64,
    local_owner: Option<mpsc::WeakUnboundedSender<String>>,
}

impl QueuedEvent {
    fn ready(event: HostEvent, received_at: f64) -> Self {
        Self {
            event: QueuedInput::Ready(event),
            received_at,
            local_owner: None,
        }
    }

    fn resolve(self) -> Option<ReceivedEvent> {
        if self
            .local_owner
            .as_ref()
            .is_some_and(|owner| owner.upgrade().is_none())
        {
            return None;
        }
        let event = match self.event {
            QueuedInput::Ready(event) => Some(event),
            QueuedInput::Deferred(decode) => decode(),
        }?;
        Some(ReceivedEvent {
            event,
            received_at: self.received_at,
            local_owner: self.local_owner,
        })
    }
}

/// A transport's event sender. Events retain their arrival time, including
/// when the host is busy processing an earlier burst. The existing inbox is
/// unbounded; batching borrows its messages without an extra payload buffer.
#[derive(Clone)]
pub struct Events {
    sender: mpsc::UnboundedSender<QueuedEvent>,
    clock: fn() -> f64,
}

/// Receives transport events when observing a standalone transport. The host
/// keeps these same events' arrival timestamps internally for engine dispatch.
pub struct EventReceiver {
    receiver: mpsc::UnboundedReceiver<QueuedEvent>,
}

impl EventReceiver {
    /// Stop new transport events while permitting already queued events to drain.
    pub fn close(&mut self) {
        self.receiver.close();
    }

    pub async fn recv(&mut self) -> Option<HostEvent> {
        while let Some(queued) = self.receiver.recv().await {
            if let Some(received) = queued.resolve() {
                return Some(received.event);
            }
        }
        None
    }

    pub fn try_recv(&mut self) -> Result<HostEvent, mpsc::error::TryRecvError> {
        loop {
            if let Some(received) = self.receiver.try_recv()?.resolve() {
                return Ok(received.event);
            }
        }
    }
}

impl Events {
    /// Create an event channel for a standalone transport observer.
    pub fn channel() -> (Self, EventReceiver) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (
            Self {
                sender,
                clock: now_ms,
            },
            EventReceiver { receiver },
        )
    }

    /// Complete when the host inbox is gone so in-flight requests can cancel.
    pub async fn closed(&self) {
        self.sender.closed().await;
    }

    /// Whether the host inbox has closed or its receiver has been dropped.
    pub fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }

    /// Enqueue a transport event without blocking, preserving arrival order.
    pub fn send(&self, event: HostEvent) -> Result<(), mpsc::error::SendError<HostEvent>> {
        self.send_at(event, (self.clock)())
    }

    // Network frames reserve a FIFO slot before decoding. Ownership remains on
    // the queued slot even after its decoded result is cached for the next burst.
    fn send_deferred_at(
        &self,
        received_at: f64,
        local_owner: Option<mpsc::WeakUnboundedSender<String>>,
        decode: impl FnOnce() -> Option<HostEvent> + Send + 'static,
    ) -> bool {
        self.sender
            .send(QueuedEvent {
                event: QueuedInput::Deferred(Box::new(decode)),
                received_at,
                local_owner,
            })
            .is_ok()
    }

    /// Preserve a transport's arrival timestamp. Custom transports should enqueue
    /// before costly preprocessing; timestamps do not reorder racing producers.
    pub fn send_at(
        &self,
        event: HostEvent,
        received_at: f64,
    ) -> Result<(), mpsc::error::SendError<HostEvent>> {
        self.sender
            .send(QueuedEvent::ready(event, received_at))
            .map_err(|error| match error.0.event {
                QueuedInput::Ready(event) => mpsc::error::SendError(event),
                QueuedInput::Deferred(_) => unreachable!("send_at queues ready events"),
            })
    }
}

/// Wall-clock milliseconds, the engine's notion of now.
pub fn now_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

pub struct Host<V: Venue, T: Transport> {
    engine: Engine<V>,
    transport: T,
    events: Events,
    inbox: mpsc::UnboundedReceiver<QueuedEvent>,
    pending: Option<QueuedEvent>,
    sockets: HashMap<SocketId, mpsc::UnboundedSender<String>>,
    timers: HashMap<TimerId, AbortHandle>,
    clock: fn() -> f64,
    actions: Vec<Action>,
    last_dispatched_at: f64,
}

impl<V: Venue, T: Transport> Host<V, T> {
    pub fn new(engine: Engine<V>, transport: T) -> Self {
        let (sender, inbox) = mpsc::unbounded_channel();
        let events = Events {
            sender,
            clock: now_ms,
        };
        Self {
            engine,
            transport,
            events,
            inbox,
            pending: None,
            sockets: HashMap::new(),
            timers: HashMap::new(),
            clock: now_ms,
            actions: Vec::new(),
            last_dispatched_at: now_ms(),
        }
    }

    /// Feed live configuration alongside transport completions.
    pub fn event_sender(&self) -> Events {
        self.events.clone()
    }

    /// Run until `commands` closes, handing each flush frame to `on_frame`
    /// and every log line or command result to `on_notice`. Returns the
    /// engine for inspection.
    pub async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        flush_every: Duration,
        mut on_frame: impl FnMut(&[u8]),
        mut on_notice: impl FnMut(Notice),
    ) -> Engine<V> {
        self.engine.start((self.clock)());
        self.execute(&mut on_notice);
        let mut flush = tokio::time::interval(flush_every);
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let has_pending = self.pending.is_some();
            tokio::select! {
                event = async {
                    if has_pending { None } else { self.inbox.recv().await }
                } => {
                    if let Some(event) = self.pending.take().or(event) {
                        self.dispatch_ready(event, MAX_BATCH_MESSAGES);
                    }
                },
                command = commands.recv() => match command {
                    Some(command) => {
                        self.dispatch_command(command, &mut on_notice).await;
                    }
                    None => {
                        self.drain_ready_prefix(&mut on_notice).await;
                        break;
                    },
                },
                _ = flush.tick() => {
                    self.drain_ready_prefix(&mut on_notice).await;
                    if let Some(frame) = self.engine.take_frame((self.clock)()) {
                        on_frame(frame);
                    }
                }
            }
            self.execute(&mut on_notice);
            // A permanently ready socket must still let the producers, timers
            // and command sender run on a single-threaded runtime.
            tokio::task::yield_now().await;
        }
        if let Some(frame) = self.engine.take_frame((self.clock)()) {
            on_frame(frame);
        }
        self.stop();
        self.engine
    }

    fn stop(&mut self) {
        self.inbox.close();
        self.sockets.clear();
        for (_, timer) in self.timers.drain() {
            timer.abort();
        }
    }

    async fn dispatch_command(&mut self, command: Command, on_notice: &mut impl FnMut(Notice)) {
        let selected_at = (self.clock)();
        self.drain_ready_prefix(on_notice).await;
        self.engine.handle(selected_at, Event::Command(command));
    }

    async fn drain_ready_prefix(&mut self, on_notice: &mut impl FnMut(Notice)) {
        // Commands have a separate channel. Their barrier is the inbox prefix
        // ready when selected; later arrivals cannot postpone them indefinitely.
        let mut remaining = self.inbox.len() + usize::from(self.pending.is_some());
        while remaining > 0 {
            let event = self.pending.take().unwrap_or_else(|| {
                self.inbox
                    .try_recv()
                    .expect("the captured prefix is still queued")
            });
            remaining -= self.dispatch_ready(event, remaining.min(MAX_BATCH_MESSAGES));
            self.execute(on_notice);
            if remaining > 0 {
                tokio::task::yield_now().await;
            }
        }
    }

    fn dispatch_ready(&mut self, first: QueuedEvent, max_messages: usize) -> usize {
        let mut next = first;
        let mut count = 0;
        let mut bytes = 0;
        loop {
            if let Some(received) = next.resolve() {
                let size = match &received.event {
                    HostEvent::WsMessage(_, text) => Some(text.len()),
                    HostEvent::WsBinary(_, data) => Some(data.len()),
                    _ => None,
                };
                if count > 0 && (size.is_none() || bytes + size.unwrap() > MAX_BATCH_BYTES) {
                    // Decoding is performed once at the inbox position. If the
                    // decoded payload cannot fit this burst, retain its result.
                    self.pending = Some(QueuedEvent {
                        event: QueuedInput::Ready(received.event),
                        received_at: received.received_at,
                        local_owner: received.local_owner,
                    });
                    break;
                }
                bytes += size.unwrap_or(0);
                self.dispatch_received(received);
                count += 1;
                if size.is_none() {
                    break;
                }
            } else {
                // Suppressed socket inputs still occupy a captured-prefix slot
                // and count toward fairness, without advancing engine time.
                count += 1;
            }
            if count >= max_messages || bytes >= MAX_BATCH_BYTES {
                break;
            }
            match self.inbox.try_recv() {
                Ok(event) => next = event,
                Err(_) => break,
            }
        }
        count
    }

    fn dispatch_received(&mut self, queued: ReceivedEvent) {
        // Signing and HTTP share a wall-clock deadline. Waiting behind earlier
        // socket work must not extend that budget; WS messages keep arrival time.
        let now = match &queued.event {
            HostEvent::PayloadSigned(..) | HostEvent::PayloadSignFailed(..) => (self.clock)(),
            _ => queued.received_at,
        };
        self.last_dispatched_at = now;
        let engine = &mut self.engine;
        match queued.event {
            HostEvent::PublicFeedUpdate(update) => {
                if let Err(error) = engine.apply_public_feed(now, update) {
                    // A configuration error is observable through the normal notice path.
                    engine.report_public_feed_error(error);
                }
            }
            HostEvent::TickIntervalChanged(interval_ms) => {
                engine.handle(now, Event::TickIntervalChanged { interval_ms })
            }
            HostEvent::WsOpened(socket) => engine.handle(now, Event::WsOpened { socket }),
            HostEvent::WsBinary(socket, data) => engine.handle(
                now,
                Event::WsBinary {
                    socket,
                    data: &data,
                },
            ),
            HostEvent::WsMessage(socket, text) => engine.handle(
                now,
                Event::WsMessage {
                    socket,
                    text: &text,
                },
            ),
            HostEvent::WsClosed(socket, code) => {
                self.sockets.remove(&socket);
                engine.handle(now, Event::WsClosed { socket, code })
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
            HostEvent::PayloadSigned(request, result) => engine.handle(
                now,
                Event::PayloadSigned {
                    request,
                    signature: &result.signature,
                    headers: result.headers,
                },
            ),
            HostEvent::PayloadSignFailed(request, error) => engine.handle(
                now,
                Event::PayloadSignFailed {
                    request,
                    error: &error,
                },
            ),
            HostEvent::Timer(timer) => {
                self.timers.remove(&timer);
                engine.handle(now, Event::TimerFired { timer })
            }
        }
    }

    fn execute(&mut self, on_notice: &mut impl FnMut(Notice)) {
        let mut actions = std::mem::take(&mut self.actions);
        actions.extend(self.engine.drain_actions());
        for action in actions.drain(..) {
            match action {
                Action::WsConnect {
                    binary,
                    socket,
                    url,
                    channel,
                    context,
                } => {
                    let outgoing = self.transport.connect_encoding(
                        binary,
                        socket,
                        url,
                        vec![],
                        channel,
                        context,
                        self.events.clone(),
                    );
                    self.sockets.insert(socket, outgoing);
                }
                Action::WsConnectWithHeaders {
                    socket,
                    url,
                    headers,
                    channel,
                    context,
                } => {
                    let outgoing = self.transport.connect_context(
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
                }
                Action::Http(request) => self.transport.request(request, self.events.clone()),
                Action::SignPayload {
                    request,
                    payload,
                    timeout_ms,
                } => self
                    .transport
                    .sign_payload(request, payload, timeout_ms, self.events.clone()),
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
        self.actions = actions;
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;
    use safe_cex_engine::{Cx, HttpRequest, Response, book::Book};

    #[derive(Default)]
    struct TimerVenue {
        timer: Option<TimerId>,
        fired: Vec<TimerId>,
    }
    impl Venue for TimerVenue {
        type Pending = ();
        fn start(&mut self, _: &mut Cx<'_, ()>) {}
        fn on_tick_interval(&mut self, cx: &mut Cx<'_, ()>, interval_ms: f64) {
            if let Some(timer) = self.timer.take() {
                cx.clear_timer(timer);
            }
            if interval_ms.is_finite() {
                self.timer = Some(cx.set_timer(interval_ms));
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
        fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
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

    #[tokio::test(start_paused = true)]
    async fn huge_future_timer_stays_alive_until_cleared_then_normal_timers_fire() {
        let mut host = Host::new(Engine::new(TimerVenue::default()), NoNetwork);
        host.clock = || 0.;
        host.engine
            .handle(0., Event::TickIntervalChanged { interval_ms: 1e308 });
        host.execute(&mut |_| {});
        let parked = host.timers.values().next().unwrap().clone();
        // Let the spawned task run: a conversion panic would finish it while
        // leaving the host alive and silently losing this venue timer.
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(!parked.is_finished(), "huge timer must remain parked");
        assert!(host.inbox.try_recv().is_err());

        host.engine.handle(
            0.,
            Event::TickIntervalChanged {
                interval_ms: f64::INFINITY,
            },
        );
        host.execute(&mut |_| {});
        assert!(host.timers.is_empty(), "ClearTimer removes the handle");
        tokio::time::sleep(Duration::from_millis(1)).await;
        assert!(parked.is_finished(), "ClearTimer aborts the parked task");

        host.engine
            .handle(0., Event::TickIntervalChanged { interval_ms: 5. });
        host.execute(&mut |_| {});
        let normal = *host.timers.keys().next().unwrap();
        let event = tokio::time::timeout(Duration::from_secs(1), host.inbox.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(event.event, QueuedInput::Ready(HostEvent::Timer(timer)) if timer == normal)
        );
        host.dispatch_ready(event, 1);
        assert_eq!(host.engine.venue().fired, [normal]);
        assert!(host.timers.is_empty());
    }
}
