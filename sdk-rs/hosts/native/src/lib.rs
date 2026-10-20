//! Native host: runs an [`Engine`] on tokio for servers and the CLI. The
//! engine stays single-threaded and synchronous; this loop owns the sockets,
//! HTTP and timers, feeds their results in as events and executes the
//! actions that come out. Transports are pluggable so recorded traffic can
//! drive the same loop as the network.
#![cfg(not(target_arch = "wasm32"))]

mod routing;
mod server;
mod transport;

pub use routing::{
    HttpRoute, HttpRouteMode, ProxyTokenFuture, ProxyTokenProvider, RoutingPlan, SocketChannel,
    SocketChannelRule, WsRoute, WsRoutes,
};
pub use server::{FrameServer, ServerConfig};
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

    fn dispatch(&mut self, event: HostEvent) {
        let now = (self.clock)();
        let engine = &mut self.engine;
        match event {
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
        let actions: Vec<Action> = self.engine.drain_actions().collect();
        for action in actions {
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
        assert!(matches!(event, HostEvent::Timer(timer) if timer == normal));
        host.dispatch(event);
        assert_eq!(host.engine.venue().fired, [normal]);
        assert!(host.timers.is_empty());
    }
}
