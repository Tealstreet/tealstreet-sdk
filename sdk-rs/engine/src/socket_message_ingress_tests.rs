//! Text-frame ingress on managed feeds: expiry, idle refresh, pending work,
//! venue retirement inside delivery, and native control transitions.
use super::*;
use crate::socket::{Config, Timeout};
use crate::socket_lifecycle::{AuthEvidence, AuthMode, AuthRecovery, Invalidation, TrafficPolicy};
use std::cell::RefCell;
use std::collections::VecDeque;

const IDLE: f64 = 20_000.;

fn policy(controls: ControlPolicy, auth: AuthMode, traffic: TrafficPolicy) -> FeedPolicy {
    FeedPolicy {
        role: SocketChannel::Public,
        supervision: Config {
            idle_timeout_ms: IDLE,
            auth_timeout_ms: (auth == AuthMode::Required).then_some(5_000.),
            ..Config::default()
        },
        auth,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic,
        controls,
        invalidation: Invalidation::None,
        subscription_interval_ms: 0.,
        close_floors: &[],
    }
}

fn control_free() -> FeedPolicy {
    policy(ControlPolicy::None, AuthMode::None, TrafficPolicy::AnyText)
}

struct Probe {
    declared: FeedPolicy,
    registered: Option<Result<FeedId, SocketError>>,
    feed: Option<FeedId>,
    script: RefCell<VecDeque<Control>>,
    control_calls: std::cell::Cell<usize>,
    log: Vec<String>,
    proposals_at_delivery: Vec<usize>,
}

impl Probe {
    fn new(declared: FeedPolicy) -> Self {
        Self {
            declared,
            registered: None,
            feed: None,
            script: RefCell::default(),
            control_calls: std::cell::Cell::new(0),
            log: Vec::new(),
            proposals_at_delivery: Vec::new(),
        }
    }
}

impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let registered = cx.register_socket_feed(self.declared.clone());
        self.registered = Some(registered);
        let Ok(feed) = registered else {
            return;
        };
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://ingress.test").unwrap();
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, _: &str) -> Control {
        self.control_calls.set(self.control_calls.get() + 1);
        self.script
            .borrow_mut()
            .pop_front()
            .unwrap_or(Control::Other)
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        self.log.push(match &event {
            LifecycleEvent::Ready { reopened, .. } => format!("ready reopened={reopened}"),
            LifecycleEvent::Retired {
                reason, close_code, ..
            } => format!("retired {reason:?} {close_code:?}"),
            LifecycleEvent::Reconnect(_) => "reconnect".into(),
            other => format!("{other:?}")
                .split([' ', '(', '{'])
                .next()
                .unwrap()
                .to_owned(),
        });
        if let LifecycleEvent::Reconnect(feed) = event {
            cx.connect_socket(feed, "wss://ingress.test")?;
        }
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &Store,
        _: FeedId,
        subscribe: bool,
        items: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for item in items {
            emit(EncodedBatch {
                text: format!("{subscribe}:{}", item.wire_args),
                covered_keys: vec![item.key.clone()],
            })?;
        }
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {
        panic!("managed open reached legacy handler");
    }
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.log.push(format!("message {text}"));
        self.proposals_at_delivery
            .push(cx.io.socket_proposals.len());
        if text == "retire" {
            cx.retire_socket(self.feed.unwrap(), RetireReason::Closed);
        }
    }
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
    engine: Engine<Probe>,
    socket: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    closed: Vec<SocketId>,
    sent: Vec<String>,
}

impl Harness {
    fn new(declared: FeedPolicy) -> Self {
        let mut h = Self {
            engine: crate::test_support::seeded_engine(Probe::new(declared)),
            socket: None,
            timers: Vec::new(),
            closed: Vec::new(),
            sent: Vec::new(),
        };
        h.engine.start(0.);
        h.collect();
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::WsConnect { socket, .. } => self.socket = Some(socket),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::WsSend { text, .. } => self.sent.push(text),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, now: f64, event: Event<'_>) {
        self.engine.handle(now, event);
        self.collect();
    }
    fn open(&mut self, now: f64) -> SocketId {
        let socket = self.socket.unwrap();
        self.event(now, Event::WsOpened { socket });
        socket
    }
    /// Deliver a frame without firing any due watchdog first, so expiry must
    /// be decided at ingress.
    fn frame(&mut self, now: f64, socket: SocketId, text: &str) {
        self.event(now, Event::WsMessage { socket, text });
    }
    fn delivered(&self) -> Vec<&str> {
        self.engine
            .venue
            .log
            .iter()
            .filter_map(|l| l.strip_prefix("message "))
            .collect()
    }
    fn timer_ids(&self) -> Vec<TimerId> {
        self.timers.iter().map(|(t, _)| *t).collect()
    }
    fn armed_since(&self, before: &[TimerId]) -> bool {
        self.timers.iter().any(|(t, _)| !before.contains(t))
    }
    fn fire_due(&mut self, now: f64) {
        while let Some(&(timer, at)) = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= now)
            .min_by(|a, b| a.1.total_cmp(&b.1))
        {
            self.timers.retain(|(t, _)| *t != timer);
            self.event(at, Event::TimerFired { timer });
        }
    }
}

/// A frame one millisecond before the idle deadline is read and refreshes the
/// deadline; a frame exactly at it is not read and retires the socket with an
/// idle timeout, on control-free and native feeds alike.
#[test]
fn a_frame_at_the_idle_deadline_retires_and_one_before_it_refreshes() {
    for controls in [ControlPolicy::None, ControlPolicy::Native] {
        let mut h = Harness::new(policy(controls, AuthMode::None, TrafficPolicy::AnyText));
        let socket = h.open(0.);
        h.frame(IDLE - 1., socket, "early");
        h.frame(2. * IDLE - 2., socket, "refreshed");
        assert!(h.closed.is_empty(), "{controls:?}");
        let armed = h.timer_ids();
        h.frame(3. * IDLE - 2., socket, "late");
        assert_eq!(h.delivered(), ["early", "refreshed"], "{controls:?}");
        assert_eq!(h.closed, [socket], "{controls:?}");
        assert_eq!(
            h.engine.venue.log.last().unwrap(),
            &format!("retired {:?} None", RetireReason::Timeout(Timeout::Idle))
        );
        assert!(h.armed_since(&armed), "the retirement arms a retry");
        h.frame(3. * IDLE - 1., socket, "retired");
        assert_eq!(h.delivered(), ["early", "refreshed"], "{controls:?}");
        let calls = usize::from(controls == ControlPolicy::Native) * 2;
        assert_eq!(h.engine.venue.control_calls.get(), calls, "{controls:?}");
    }
}

/// Validation keeps every control-free feed unauthenticated and on any-text
/// liveness, so no auth, snapshot or first-pong deadline reaches message
/// admission. Before open the connect deadline belongs to its timer: a frame
/// there is refused without retiring the socket.
#[test]
fn control_free_feeds_meet_only_the_idle_deadline_at_message_admission() {
    for (auth, traffic) in [
        (AuthMode::Required, TrafficPolicy::AnyText),
        (AuthMode::None, TrafficPolicy::PongOnly),
        (AuthMode::None, TrafficPolicy::TextAfterFirstPong),
    ] {
        let h = Harness::new(policy(ControlPolicy::None, auth, traffic));
        assert_eq!(
            h.engine.venue.registered,
            Some(Err(SocketError::InvalidPolicy)),
            "{auth:?} {traffic:?}"
        );
    }
    let mut h = Harness::new(control_free());
    let socket = h.socket.unwrap();
    let connect = Config::default().connect_timeout_ms;
    h.frame(connect - 1., socket, "unopened");
    h.frame(connect, socket, "at-connect-deadline");
    assert!(h.delivered().is_empty());
    assert!(
        h.closed.is_empty(),
        "message admission never expires connect"
    );
    h.fire_due(connect);
    assert_eq!(h.closed, [socket]);
    assert_eq!(
        h.engine.venue.log.last().unwrap(),
        &format!("retired {:?} None", RetireReason::Timeout(Timeout::Connect))
    );
}

/// Steady control-free traffic keeps one socket alive across many idle
/// windows.
#[test]
fn every_control_free_frame_refreshes_the_idle_deadline() {
    let mut h = Harness::new(control_free());
    let socket = h.open(0.);
    for step in 1..=6 {
        h.frame(f64::from(step) * (IDLE - 1.), socket, "tick");
    }
    assert_eq!(h.delivered().len(), 6);
    assert!(h.closed.is_empty());
    assert_eq!(h.engine.venue.control_calls.get(), 0);
}

/// Interest work already queued when a frame arrives is processed before the
/// venue reads that frame.
#[test]
fn interest_work_pending_at_a_frame_is_processed_before_the_venue_reads_it() {
    let mut h = Harness::new(control_free());
    let socket = h.open(0.);
    h.frame(1., socket, "before");
    let feed = h.engine.venue.feed.unwrap();
    let mut cx = Cx {
        now: 2.,
        store: &mut h.engine.store,
        io: &mut h.engine.io,
    };
    cx.set_socket_interests(feed, vec![Interest::from_wire_args("topic")])
        .unwrap();
    assert_eq!(h.engine.io.socket_proposals.len(), 1);
    h.frame(3., socket, "after");
    assert_eq!(h.delivered(), ["before", "after"]);
    assert_eq!(h.engine.venue.proposals_at_delivery, [0, 0]);
    assert_eq!(h.sent, ["true:topic"]);
}

/// A venue that retires its own socket while reading a frame has that
/// retirement finished in the same event, before the host drains actions;
/// later frames are not read.
#[test]
fn a_venue_retiring_its_socket_inside_a_frame_finishes_in_that_event() {
    let mut h = Harness::new(control_free());
    let socket = h.open(0.);
    let armed = h.timer_ids();
    h.engine.handle(
        1.,
        Event::WsMessage {
            socket,
            text: "retire",
        },
    );
    assert!(h.engine.io.core.socket_effects.is_empty());
    assert_eq!(
        h.engine.venue.log.last().unwrap(),
        &format!("retired {:?} None", RetireReason::Closed)
    );
    h.collect();
    assert_eq!(h.closed, [socket]);
    assert_eq!(
        h.engine.venue.log.last().unwrap(),
        &format!("retired {:?} None", RetireReason::Closed)
    );
    assert!(h.armed_since(&armed), "the retirement arms a retry");
    h.frame(2., socket, "after");
    assert_eq!(h.delivered(), ["retire"]);
}

/// Native control frames keep their auth, pong-liveness and first-pong
/// transitions; only `Other` and auth progress reach the venue.
#[test]
fn native_control_frames_keep_their_transitions() {
    let mut h = Harness::new(policy(
        ControlPolicy::Native,
        AuthMode::Required,
        TrafficPolicy::PongOnly,
    ));
    h.engine.venue.script.borrow_mut().extend([
        Control::Auth(AuthEvidence::Accepted),
        Control::Other,
        Control::Pong,
        Control::Auth(AuthEvidence::Progress),
        Control::Other,
        Control::Other,
    ]);
    let socket = h.open(0.);
    h.frame(100., socket, "login");
    assert_eq!(
        h.engine.venue.log,
        ["Opened", "Authenticated", "ready reopened=false"]
    );
    h.frame(10_000., socket, "data");
    h.frame(15_000., socket, "pong");
    h.frame(16_000., socket, "progress");
    // Only the pong refreshed liveness: 15 s + 20 s idle.
    h.frame(34_999., socket, "alive");
    assert!(h.closed.is_empty());
    h.frame(35_000., socket, "late");
    assert_eq!(h.delivered(), ["data", "progress", "alive"]);
    assert_eq!(h.closed, [socket]);
    // The expired frame is refused before the codec sees it.
    assert_eq!(h.engine.venue.control_calls.get(), 5);
    assert_eq!(
        h.engine.venue.log.last().unwrap(),
        &format!("retired {:?} None", RetireReason::Timeout(Timeout::Idle))
    );

    let mut h = Harness::new(policy(
        ControlPolicy::Native,
        AuthMode::None,
        TrafficPolicy::TextAfterFirstPong,
    ));
    h.engine.venue.script.borrow_mut().extend([
        Control::Other,
        Control::Pong,
        Control::Other,
        Control::Other,
    ]);
    let socket = h.open(0.);
    // Idle is off until the first pong; text before it is read but proves
    // nothing.
    h.frame(30_000., socket, "before-pong");
    h.frame(31_000., socket, "pong");
    h.frame(50_999., socket, "text-after-pong");
    assert!(h.closed.is_empty());
    h.frame(70_999., socket, "late");
    assert_eq!(h.delivered(), ["before-pong", "text-after-pong"]);
    assert_eq!(h.closed, [socket]);
}
