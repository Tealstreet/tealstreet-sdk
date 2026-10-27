//! Reconnect delays after a managed feed retires, keyed by why it retired.
use super::*;
use crate::socket::{Config, Timeout};
use crate::socket_lifecycle::{AuthMode, AuthRecovery, Invalidation, TrafficPolicy};

#[derive(Default)]
struct Feed {
    feed: Option<FeedId>,
    retired: Vec<LifecycleEvent>,
}

fn policy() -> FeedPolicy {
    FeedPolicy {
        role: SocketChannel::Public,
        supervision: Config::default(),
        auth: AuthMode::None,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::None,
        invalidation: Invalidation::None,
        subscription_interval_ms: 0.,
    }
}

impl Venue for Feed {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let feed = cx.register_socket_feed(policy()).unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://floor.test").unwrap();
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        match event {
            LifecycleEvent::Reconnect(feed) => {
                cx.connect_socket(feed, "wss://floor.test")?;
            }
            LifecycleEvent::Retired { .. } => self.retired.push(event),
            _ => {}
        }
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {
        panic!("managed close reached legacy handler");
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

struct Harness {
    engine: Engine<Feed>,
    now: f64,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
}

impl Harness {
    fn new() -> Self {
        let mut h = Self {
            engine: crate::test_support::seeded_engine(Feed::default()),
            now: 0.,
            socket: None,
            timers: Vec::new(),
        };
        h.engine.start(0.);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::WsConnect { socket, .. } => self.socket = Some(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn open(&mut self) {
        let socket = self.socket.unwrap();
        self.event(Event::WsOpened { socket });
    }
    /// Feed `event` for the current socket and return the delay of the one
    /// retry timer it arms.
    fn retire_with(&mut self, event: Event<'_>) -> f64 {
        let before: Vec<TimerId> = self.timers.iter().map(|(t, _)| *t).collect();
        self.event(event);
        let fresh: Vec<f64> = self
            .timers
            .iter()
            .filter(|(t, _)| !before.contains(t))
            .map(|(_, at)| at - self.now)
            .collect();
        assert_eq!(fresh.len(), 1, "one retry timer: {fresh:?}");
        fresh[0]
    }
    fn close(&mut self, code: u16) -> f64 {
        let socket = self.socket.unwrap();
        self.retire_with(Event::WsClosed { socket, code })
    }
    /// Fire due timers through `until`, in order.
    fn advance(&mut self, until: f64) {
        while let Some(&(timer, at)) = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= until)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }
    fn reconnect(&mut self, delay: f64) {
        let old = self.socket;
        let until = self.now + delay;
        self.advance(until);
        assert_ne!(self.socket, old, "the retry reconnects");
    }
    fn last_reason(&self) -> RetireReason {
        match self.engine.venue.retired.last() {
            Some(LifecycleEvent::Retired { reason, .. }) => *reason,
            other => panic!("no retirement: {other:?}"),
        }
    }
}

fn assert_backoff(attempt: i32, delay: f64) {
    let nominal = (1_000. * 2f64.powi(attempt)).min(30_000.);
    assert!(
        (nominal * 0.75..=nominal).contains(&delay),
        "attempt {attempt}: {delay} outside {nominal} jitter"
    );
}

/// A close code the feed declares nothing for keeps the ordinary backoff,
/// and each such close counts one failure.
#[test]
fn an_undeclared_close_code_keeps_the_ordinary_backoff() {
    let mut h = Harness::new();
    for attempt in 0..4 {
        h.open();
        let delay = h.close(1013);
        assert_backoff(attempt, delay);
        assert_eq!(h.last_reason(), RetireReason::Closed);
        h.reconnect(delay);
    }
}

/// Retirements that are not a venue close frame keep today's retry: route
/// exhaustion is a Closed retirement on the ordinary backoff, an idle
/// timeout retries on the backoff, and a stopped feed never retries.
#[test]
fn route_exhaustion_timeouts_and_stops_keep_their_retry() {
    let mut h = Harness::new();
    h.open();
    let socket = h.socket.unwrap();
    let delay = h.retire_with(Event::WsRouteExhausted { socket });
    assert_backoff(0, delay);
    assert_eq!(h.last_reason(), RetireReason::Closed);
    h.reconnect(delay);

    h.open();
    let opened = h.now;
    let before: Vec<TimerId> = h.timers.iter().map(|(t, _)| *t).collect();
    h.advance(opened + 60_000.);
    assert_eq!(h.last_reason(), RetireReason::Timeout(Timeout::Idle));
    let retry: Vec<f64> = h
        .timers
        .iter()
        .filter(|(t, _)| !before.contains(t))
        .map(|(_, at)| at - h.now)
        .collect();
    assert_eq!(retry.len(), 1, "one retry timer: {retry:?}");
    assert_backoff(1, retry[0]);
    h.reconnect(retry[0]);

    h.open();
    let feed = h.engine.venue.feed.unwrap();
    let mut cx = Cx {
        now: h.now,
        store: &mut h.engine.store,
        io: &mut h.engine.io,
    };
    cx.stop_socket_feed(feed);
    h.engine.process_socket_work(h.now);
    h.collect();
    assert_eq!(h.last_reason(), RetireReason::Stopped);
    assert!(
        h.timers.is_empty(),
        "a stopped feed arms nothing: {:?}",
        h.timers
    );
}
