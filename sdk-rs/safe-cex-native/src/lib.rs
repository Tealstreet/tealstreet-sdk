//! Native host: runs an [`Engine`] on tokio for servers and the CLI. The
//! engine stays single-threaded and synchronous; this loop owns the sockets,
//! HTTP and timers, feeds their results in as events and executes the
//! actions that come out. Transports are pluggable so recorded traffic can
//! drive the same loop as the network.
#![cfg(not(target_arch = "wasm32"))]

mod transport;

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
    WsOpened(SocketId),
    WsMessage(SocketId, String),
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

pub type Events = mpsc::UnboundedSender<HostEvent>;

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
    inbox: mpsc::UnboundedReceiver<HostEvent>,
    sockets: HashMap<SocketId, mpsc::UnboundedSender<String>>,
    timers: HashMap<TimerId, AbortHandle>,
    clock: fn() -> f64,
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
            timers: HashMap::new(),
            clock: now_ms,
        }
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
                Some(event) = self.inbox.recv() => self.dispatch(event),
                command = commands.recv() => match command {
                    Some(command) => self.engine.handle((self.clock)(), Event::Command(command)),
                    None => break,
                },
                _ = flush.tick() => {
                    if let Some(frame) = self.engine.take_frame((self.clock)()) {
                        on_frame(frame);
                    }
                }
            }
            self.execute(&mut on_notice);
        }
        if let Some(frame) = self.engine.take_frame((self.clock)()) {
            on_frame(frame);
        }
        for timer in self.timers.values() {
            timer.abort();
        }
        self.engine
    }

    fn dispatch(&mut self, event: HostEvent) {
        let now = (self.clock)();
        let engine = &mut self.engine;
        match event {
            HostEvent::WsOpened(socket) => engine.handle(now, Event::WsOpened { socket }),
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
        let actions: Vec<Action> = self.engine.drain_actions().collect();
        for action in actions {
            match action {
                Action::WsConnect { socket, url } => {
                    let outgoing = self.transport.connect(socket, url, self.events.clone());
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
                    self.timers.insert(timer, handle.abort_handle());
                }
                Action::ClearTimer { timer } => {
                    if let Some(handle) = self.timers.remove(&timer) {
                        handle.abort();
                    }
                }
                Action::Notice(notice) => on_notice(notice),
            }
        }
    }
}
