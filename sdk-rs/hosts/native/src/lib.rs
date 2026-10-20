//! Native host: runs an [`Engine`] on tokio for servers and the CLI. The
//! engine stays single-threaded and synchronous; this loop owns the sockets,
//! HTTP and timers, feeds their results in as events and executes the
//! actions that come out. Transports are pluggable so recorded traffic can
//! drive the same loop as the network.
#![cfg(not(target_arch = "wasm32"))]

mod routing;
#[cfg(test)]
mod tests;
mod transport;

pub use routing::{
    HttpRoute, HttpRouteMode, ProxyTokenFuture, ProxyTokenProvider, RoutingPlan, SocketChannel,
    SocketChannelRule, WsRoute, WsRoutes,
};
pub use transport::{NetworkTransport, Transport};

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
    Timer(TimerId),
}

const MAX_BATCH_MESSAGES: usize = 256;
const MAX_BATCH_BYTES: usize = 1024 * 1024;

struct QueuedEvent {
    event: HostEvent,
    received_at: f64,
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
    pub async fn recv(&mut self) -> Option<HostEvent> {
        self.receiver.recv().await.map(|queued| queued.event)
    }

    pub fn try_recv(&mut self) -> Result<HostEvent, mpsc::error::TryRecvError> {
        self.receiver.try_recv().map(|queued| queued.event)
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

    /// Enqueue a transport event without blocking, preserving arrival order.
    pub fn send(&self, event: HostEvent) -> Result<(), mpsc::error::SendError<HostEvent>> {
        self.send_at(event, (self.clock)())
    }

    /// Preserve the raw arrival time when decoding a received frame takes time.
    pub fn send_at(
        &self,
        event: HostEvent,
        received_at: f64,
    ) -> Result<(), mpsc::error::SendError<HostEvent>> {
        self.sender
            .send(QueuedEvent { event, received_at })
            .map_err(|error| mpsc::error::SendError(error.0.event))
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
            tokio::select! {
                event = async {
                    if self.pending.is_some() { None } else { self.inbox.recv().await }
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
        for timer in self.timers.values() {
            timer.abort();
        }
        self.engine
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
        if !matches!(
            first.event,
            HostEvent::WsMessage(..) | HostEvent::WsBinary(..)
        ) {
            self.dispatch(first);
            return 1;
        }
        let mut next = first;
        let mut count = 0;
        let mut bytes = 0;
        loop {
            let size = match &next.event {
                HostEvent::WsMessage(_, text) => text.len(),
                HostEvent::WsBinary(_, data) => data.len(),
                _ => unreachable!("only websocket messages enter this burst"),
            };
            if count > 0 && bytes + size > MAX_BATCH_BYTES {
                self.pending = Some(next);
                break;
            }
            bytes += size;
            self.dispatch(next);
            count += 1;
            if count >= max_messages || bytes >= MAX_BATCH_BYTES {
                break;
            }
            match self.inbox.try_recv() {
                Ok(event)
                    if matches!(
                        event.event,
                        HostEvent::WsMessage(..) | HostEvent::WsBinary(..)
                    ) =>
                {
                    next = event;
                }
                Ok(event) => {
                    self.pending = Some(event);
                    break;
                }
                Err(_) => break,
            }
        }
        count
    }

    fn dispatch(&mut self, queued: QueuedEvent) {
        let now = queued.received_at;
        let engine = &mut self.engine;
        match queued.event {
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
                Action::SetTimer { timer, at_ms } => {
                    let delay = (at_ms - (self.clock)()).max(0.0);
                    let events = self.events.clone();
                    let handle = tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs_f64(delay / 1000.0)).await;
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
