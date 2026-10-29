use super::*;
use crate::socket::{Config, Timeout};
use crate::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Invalidation, NativeTiming, StableAnchor, TrafficPolicy,
};

struct Probe {
    policy: FeedPolicy,
    timing: Option<NativeTiming>,
    snapshot: bool,
    feed: Option<FeedId>,
    registration: Option<Result<FeedId, SocketError>>,
    events: Vec<LifecycleEvent>,
    messages: Vec<String>,
    completions: Vec<(&'static str, Result<(), SocketError>, bool)>,
    timer: Option<TimerId>,
    http: Option<RequestId>,
    lifecycle_completion: bool,
}
impl Default for Probe {
    fn default() -> Self {
        Self {
            policy: FeedPolicy {
                role: SocketChannel::Private,
                supervision: Config {
                    connect_timeout_ms: 300_000.,
                    auth_timeout_ms: Some(15_000.),
                    idle_timeout_ms: 60_000.,
                    ping_interval_ms: None,
                    ..Config::default()
                },
                auth: AuthMode::Required,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::Declared,
                controls: ControlPolicy::Native,
                invalidation: Invalidation::None,
                subscription_interval_ms: 0.,
                close_floors: &[],
            },
            timing: Some(NativeTiming::default()),
            snapshot: false,
            feed: None,
            registration: None,
            events: Vec::new(),
            messages: Vec::new(),
            completions: Vec::new(),
            timer: None,
            http: None,
            lifecycle_completion: false,
        }
    }
}
impl Probe {
    fn complete(&mut self, cx: &mut Cx<'_, ()>, scope: &'static str) {
        let result = cx.complete_socket_initial_payload();
        let authenticated = self.feed.is_some_and(|feed| cx.socket_authenticated(feed));
        self.completions.push((scope, result, authenticated));
    }
}
impl Venue for Probe {
    fn managed_sockets() -> bool {
        true
    }
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let result = if self.snapshot {
            cx.register_socket_feed_with_snapshot(self.policy.clone())
        } else if let Some(timing) = self.timing {
            cx.register_socket_feed_with_native_timing(self.policy.clone(), timing)
        } else {
            cx.register_socket_feed(self.policy.clone())
        };
        self.registration = Some(result);
        if let Ok(feed) = result {
            self.feed = Some(feed);
            cx.connect_socket(feed, "wss://fixture.invalid/native")
                .unwrap();
        }
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, text: &str) -> Control {
        match text {
            "traffic" | "{}" => Control::Traffic,
            "pong" => Control::Pong,
            "challenge" | "partial-ack" => Control::Auth(AuthEvidence::Progress),
            "accepted" => Control::Auth(AuthEvidence::Accepted),
            "malformed-auth" => Control::Auth(AuthEvidence::Malformed),
            _ => Control::Other,
        }
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        if let LifecycleEvent::Reconnect(feed) = event {
            cx.connect_socket(feed, "wss://fixture.invalid/native")?;
        }
        if self.lifecycle_completion {
            self.complete(cx, "lifecycle");
        }
        self.events.push(event);
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.messages.push(text.into());
        if text == "complete" {
            self.complete(cx, "text");
        } else if text == "complete-old" {
            let result = cx.complete_socket_snapshot();
            self.completions
                .push(("old", result, cx.socket_authenticated(self.feed.unwrap())));
        }
    }
    fn on_binary(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, _: &[u8]) {
        self.complete(cx, "binary");
    }
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: Response<'_>) {
        self.complete(cx, "http");
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, timer: TimerId) {
        assert_eq!(Some(timer), self.timer);
        self.complete(cx, "timer");
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: Command) {
        self.complete(cx, "command");
        self.timer = Some(cx.set_timer(100.));
        self.http = Some(cx.request(
            Method::Get,
            "https://fixture.invalid/scope".into(),
            Vec::new(),
            None,
            (),
        ));
    }
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
    timers: std::collections::BTreeMap<u64, f64>,
    actions: Vec<Action>,
    epoch: Epoch,
}
impl Harness {
    fn new(probe: Probe, opened_at: f64) -> Self {
        let mut engine = crate::test_support::seeded_engine(probe);
        engine.start(0.);
        let epoch = engine
            .io
            .core
            .socket_driver
            .state
            .epoch(engine.venue.feed.unwrap())
            .unwrap();
        let mut h = Self {
            engine,
            timers: std::collections::BTreeMap::default(),
            actions: Vec::new(),
            epoch,
        };
        h.collect();
        h.event(
            opened_at,
            Event::WsOpened {
                socket: epoch.socket(),
            },
        );
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    self.timers.insert(timer.0, at_ms);
                }
                Action::ClearTimer { timer } => {
                    self.timers.remove(&timer.0);
                }
                _ => {}
            }
            self.actions.push(action);
        }
    }
    fn event(&mut self, now: f64, event: Event<'_>) {
        self.engine.handle(now, event);
        self.collect();
    }
    fn text(&mut self, now: f64, text: &str) {
        self.advance(now);
        self.event(
            now,
            Event::WsMessage {
                socket: self.epoch.socket(),
                text,
            },
        );
    }
    fn advance(&mut self, until: f64) {
        while let Some((timer, at)) = self
            .timers
            .iter()
            .filter(|(_, at)| **at <= until)
            .min_by(|a, b| a.1.total_cmp(b.1))
            .map(|(id, at)| (TimerId(*id), *at))
        {
            self.timers.remove(&timer.0);
            self.event(at, Event::TimerFired { timer });
        }
    }
    fn current(&self) -> bool {
        self.engine.io.core.socket_driver.state.current(self.epoch)
    }
    fn timeout(&self, timeout: Timeout) -> bool {
        self.engine.venue.events.iter().any(|event| {
            matches!(event,
            LifecycleEvent::Retired { epoch, reason: RetireReason::Timeout(kind), .. }
            if *epoch == self.epoch && *kind == timeout)
        })
    }
}
fn initial_probe() -> Probe {
    Probe {
        timing: Some(NativeTiming {
            initial_payload_timeout_ms: Some(180_000.),
            stable_anchor: StableAnchor::Open,
        }),
        ..Probe::default()
    }
}

#[test]
fn declared_traffic_is_delivered_to_the_native_reducer() {
    let mut h = Harness::new(Probe::default(), 1.);
    h.text(2., "{}");
    assert_eq!(h.engine.venue.messages, ["{}"]);
}
#[test]
fn declared_data_refreshes_receive_while_unknown_text_does_not() {
    let mut h = Harness::new(Probe::default(), 1.);
    h.text(2., "accepted");
    h.advance(55_000.);
    h.text(55_000., "traffic");
    h.advance(65_000.);
    assert!(
        h.current(),
        "valid declared data extends native idle liveness"
    );
    h.advance(115_000.);
    assert!(h.timeout(Timeout::Idle));
}
#[test]
fn malformed_text_cannot_rescue_idle_expiry() {
    let mut h = Harness::new(Probe::default(), 1.);
    h.text(2., "accepted");
    h.advance(59_999.);
    h.text(59_999., "wrong-type");
    h.text(60_002., "pong");
    assert!(h.timeout(Timeout::Idle));
    assert!(!h.current());
}
#[test]
fn declared_progress_refreshes_receive_without_granting_ready() {
    let mut probe = Probe::default();
    probe.policy.supervision.auth_timeout_ms = Some(100_000.);
    let mut h = Harness::new(probe, 1.);
    h.advance(55_000.);
    h.text(55_000., "challenge");
    h.advance(65_000.);
    assert!(h.current());
    assert!(
        !h.engine
            .io
            .core
            .socket_driver
            .state
            .authenticated(h.epoch.feed())
    );
    assert!(
        !h.engine
            .venue
            .events
            .iter()
            .any(|event| matches!(event, LifecycleEvent::Ready { .. }))
    );
    h.text(100_000., "pong");
    h.text(140_000., "pong");
    h.advance(154_999.);
    assert!(h.current());
    h.advance(155_000.);
    assert!(h.timeout(Timeout::Auth));
}
#[test]
fn declared_partial_ack_refreshes_receive() {
    let mut probe = Probe::default();
    probe.policy.supervision.auth_timeout_ms = Some(100_000.);
    let mut h = Harness::new(probe, 1.);
    h.text(50_000., "challenge");
    h.text(100_000., "partial-ack");
    h.advance(110_000.);
    assert!(h.current());
}
#[test]
fn declared_final_ack_refreshes_receive() {
    let mut probe = Probe::default();
    probe.policy.supervision.auth_timeout_ms = Some(100_000.);
    let mut h = Harness::new(probe, 1.);
    h.text(55_000., "accepted");
    h.advance(65_000.);
    assert!(h.current());
}
#[test]
fn open_payload_deadline_survives_progress_and_final_ack() {
    let mut h = Harness::new(initial_probe(), 10_000.);
    h.text(20_000., "challenge");
    h.text(30_000., "accepted");
    for now in [60_000., 100_000., 140_000., 180_000.] {
        h.text(now, "pong");
    }
    h.text(190_000., "pong");
    assert!(
        !h.current(),
        "initial payload is still missing at exact OPEN+180s"
    );
    assert!(h.timeout(Timeout::Auth));
}
#[test]
fn delayed_open_does_not_inherit_connect_payload_origin() {
    let mut h = Harness::new(initial_probe(), 100_000.);
    h.text(100_001., "accepted");
    for now in [150_000., 200_000., 250_000., 279_999.] {
        h.text(now, "pong");
    }
    assert!(h.current());
    h.text(280_000., "pong");
    assert!(!h.current());
}
#[test]
fn completion_before_final_ack_does_not_authenticate_and_retires_only_its_bound() {
    let mut h = Harness::new(initial_probe(), 1.);
    h.text(2., "challenge");
    h.text(3., "complete");
    assert_eq!(h.engine.venue.completions, [("text", Ok(()), false)]);
    assert!(
        !h.engine
            .venue
            .events
            .iter()
            .any(|event| matches!(event, LifecycleEvent::Ready { .. }))
    );
    h.text(4., "accepted");
    for now in [50_000., 100_000., 150_000., 190_000.] {
        h.text(now, "pong");
    }
    assert!(h.current());
    h.text(190_001., "complete");
    assert_eq!(
        h.engine.venue.completions.last().unwrap().1,
        Err(SocketError::Stale)
    );
}
#[test]
fn completion_after_final_ack_is_exactly_once() {
    let mut h = Harness::new(initial_probe(), 1.);
    h.text(2., "accepted");
    h.text(3., "complete");
    h.text(4., "complete");
    assert_eq!(
        h.engine.venue.completions,
        [
            ("text", Ok(()), true),
            ("text", Err(SocketError::Stale), true)
        ]
    );
}
#[test]
fn completion_at_initial_deadline_cannot_rescue_or_reach_the_reducer() {
    let mut h = Harness::new(initial_probe(), 1.);
    h.text(2., "accepted");
    for now in [50_000., 100_000., 150_000.] {
        h.text(now, "pong");
    }
    h.text(180_001., "complete");
    assert!(h.engine.venue.completions.is_empty());
    assert!(!h.current());
}
#[test]
fn timer_command_http_binary_and_lifecycle_have_no_text_completion_proof() {
    let mut probe = initial_probe();
    probe.lifecycle_completion = true;
    let mut h = Harness::new(probe, 1.);
    h.event(
        2.,
        Event::Command(Command::SetTickerSymbols {
            symbols: Vec::new(),
        }),
    );
    let request = h.engine.venue.http.unwrap();
    h.event(
        3.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: "{}",
            },
        },
    );
    h.event(
        4.,
        Event::WsBinary {
            socket: h.epoch.socket(),
            data: b"complete",
        },
    );
    h.advance(102.);
    for scope in ["lifecycle", "command", "http", "binary", "timer"] {
        assert!(
            h.engine
                .venue
                .completions
                .iter()
                .any(|row| row.0 == scope && row.1 == Err(SocketError::Stale)),
            "missing real {scope} callback"
        );
    }
    h.text(103., "complete");
    assert_eq!(h.engine.venue.completions.last().unwrap().1, Ok(()));
}
#[test]
fn initial_completion_cannot_discharge_the_legacy_snapshot() {
    let mut probe = Probe {
        timing: None,
        snapshot: true,
        ..Probe::default()
    };
    probe.policy.traffic = TrafficPolicy::AnyText;
    let mut h = Harness::new(probe, 1.);
    h.text(2., "complete-old");
    assert_eq!(
        h.engine.venue.completions.last().unwrap().1,
        Err(SocketError::Stale)
    );
    h.text(3., "accepted");
    h.text(4., "complete");
    assert_eq!(
        h.engine.venue.completions.last().unwrap().1,
        Err(SocketError::Stale)
    );
    h.text(5., "complete-old");
    assert_eq!(h.engine.venue.completions.last().unwrap().1, Ok(()));
    h.text(15_001., "pong");
    assert!(h.current());
}
#[test]
fn legacy_snapshot_completion_cannot_discharge_the_initial_bound() {
    let mut h = Harness::new(initial_probe(), 1.);
    h.text(2., "accepted");
    h.text(3., "complete-old");
    assert_eq!(
        h.engine.venue.completions.last().unwrap().1,
        Err(SocketError::Stale)
    );
    for now in [50_000., 100_000., 150_000.] {
        h.text(now, "pong");
    }
    h.text(180_001., "pong");
    assert!(!h.current());
}
#[test]
fn declared_without_native_controls_is_refused_atomically() {
    let mut probe = Probe::default();
    probe.policy.auth = AuthMode::None;
    probe.policy.supervision.auth_timeout_ms = None;
    probe.policy.controls = ControlPolicy::None;
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    assert_eq!(
        engine.venue.registration,
        Some(Err(SocketError::InvalidPolicy))
    );
    assert!(engine.venue.feed.is_none());
    assert!(
        !engine
            .drain_actions()
            .any(|action| matches!(action, Action::WsConnect { .. }))
    );
}
fn invalid_initial_timeout(timeout: f64) {
    let mut probe = initial_probe();
    probe.timing.as_mut().unwrap().initial_payload_timeout_ms = Some(timeout);
    let mut engine = crate::test_support::seeded_engine(probe);
    engine.start(0.);
    assert_eq!(
        engine.venue.registration,
        Some(Err(SocketError::InvalidPolicy)),
        "timeout {timeout}"
    );
    assert!(engine.venue.feed.is_none());
    assert!(
        !engine
            .drain_actions()
            .any(|action| matches!(action, Action::WsConnect { .. }))
    );
}
#[test]
fn zero_initial_timeout_publishes_no_feed() {
    invalid_initial_timeout(0.);
}
#[test]
fn negative_initial_timeout_publishes_no_feed() {
    invalid_initial_timeout(-1.);
}
#[test]
fn nan_initial_timeout_publishes_no_feed() {
    invalid_initial_timeout(f64::NAN);
}
#[test]
fn infinite_initial_timeout_publishes_no_feed() {
    invalid_initial_timeout(f64::INFINITY);
}
#[test]
fn negative_infinite_initial_timeout_publishes_no_feed() {
    invalid_initial_timeout(f64::NEG_INFINITY);
}
#[test]
fn completion_requires_an_armed_initial_bound() {
    let mut h = Harness::new(Probe::default(), 1.);
    h.text(2., "accepted");
    h.text(3., "complete");
    assert_eq!(
        h.engine.venue.completions,
        [("text", Err(SocketError::Stale), true)]
    );
    assert!(h.current());
}
#[test]
fn old_any_text_policy_preserves_malformed_text_liveness() {
    let mut probe = Probe {
        timing: None,
        ..Probe::default()
    };
    probe.policy.traffic = TrafficPolicy::AnyText;
    let mut h = Harness::new(probe, 1.);
    h.text(2., "accepted");
    h.text(55_000., "wrong-type");
    h.advance(65_000.);
    assert!(h.current());
    assert_eq!(h.engine.venue.messages, ["wrong-type"]);
}
#[test]
fn malformed_auth_retains_native_retirement_without_delivery() {
    let mut h = Harness::new(Probe::default(), 1.);
    h.text(2., "malformed-auth");
    assert!(!h.current());
    assert!(h.engine.venue.messages.is_empty());
    assert!(h.engine.venue.events.iter().any(|event| matches!(
        event,
        LifecycleEvent::Retired {
            reason: RetireReason::Authentication,
            ..
        }
    )));
}
fn second_retry_delay(anchor: StableAnchor) -> f64 {
    let mut probe = Probe::default();
    probe.policy.supervision.auth_timeout_ms = Some(100_000.);
    probe.timing.as_mut().unwrap().stable_anchor = anchor;
    let mut h = Harness::new(probe, 1.);
    h.event(
        2.,
        Event::WsClosed {
            socket: h.epoch.socket(),
            code: 1006,
        },
    );
    h.advance(2_000.);
    let epoch = h
        .engine
        .io
        .core
        .socket_driver
        .state
        .epoch(h.epoch.feed())
        .unwrap();
    h.epoch = epoch;
    h.event(
        2_001.,
        Event::WsOpened {
            socket: epoch.socket(),
        },
    );
    h.text(22_001., "accepted");
    h.text(32_002., "pong");
    h.event(
        32_003.,
        Event::WsClosed {
            socket: epoch.socket(),
            code: 1006,
        },
    );
    h.actions
        .iter()
        .rev()
        .find_map(|action| match action {
            Action::SetTimer { at_ms, .. } => Some(*at_ms - 32_003.),
            _ => None,
        })
        .unwrap()
}
#[test]
fn open_stable_anchor_resets_backoff_after_delayed_auth() {
    assert!(second_retry_delay(StableAnchor::Open) < 1_500.);
}
#[test]
fn ready_stable_anchor_retains_native_delayed_auth_backoff() {
    assert!(second_retry_delay(StableAnchor::Ready) > 1_500.);
}
