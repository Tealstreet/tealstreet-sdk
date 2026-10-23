//! Assertion-first controls for independent login and native snapshot facts.
#![cfg(feature = "managed-sockets")]
use safe_cex_engine::socket::Config;
use safe_cex_engine::socket_lifecycle::{
    AuthEvidence, AuthMode, AuthRecovery, Control, ControlPolicy, EncodedBatch, FeedId, FeedPolicy,
    Interest, Invalidation, LifecycleEvent, SocketError, TrafficPolicy,
};
use safe_cex_engine::{Action, Cx, Engine, Event, SocketChannel, SocketId, Store, Venue};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Native {
    feed: Option<FeedId>,
    snapshot_policy: bool,
    snapshots: Arc<AtomicUsize>,
    deliveries: Arc<AtomicUsize>,
}
impl Venue for Native {
    type Pending = ();
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, cx: &mut Cx<'_, ()>, _: (), _: safe_cex_engine::Response<'_>) {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
    }
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, cx: &mut Cx<'_, ()>, _: safe_cex_engine::TimerId) {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
    }
    fn on_command(&mut self, cx: &mut Cx<'_, ()>, _: safe_cex_engine::Command) {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
    }
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
        let policy = FeedPolicy {
            role: SocketChannel::Private,
            supervision: Config {
                connect_timeout_ms: 15_000.,
                auth_timeout_ms: Some(20_000.),
                idle_timeout_ms: 12_500.,
                ping_interval_ms: Some(5_000.),
                backoff_base_ms: 1_000.,
                backoff_cap_ms: 16_000.,
                stable_ms: 30_000.,
            },
            auth: AuthMode::Required,
            recovery: AuthRecovery::Reconnect,
            permanent_refusal_group: None,
            traffic: TrafficPolicy::PongOnly,
            controls: ControlPolicy::Native,
            invalidation: Invalidation::Required,
            subscription_interval_ms: 0.,
        };
        let feed = if self.snapshot_policy {
            cx.register_socket_feed_with_snapshot(policy)
        } else {
            cx.register_socket_feed(policy)
        }
        .unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://snapshot.test").unwrap();
        cx.request(
            safe_cex_engine::Method::Get,
            "https://snapshot.test/probe".into(),
            vec![],
            None,
            (),
        );
        cx.set_timer(222.);
    }
    fn socket_control(&self, _: &Store, _: FeedId, _: bool, text: &str) -> Control {
        match text {
            "login-accepted" => Control::Auth(AuthEvidence::Accepted),
            "pong" => Control::Pong,
            _ => Control::Other,
        }
    }
    fn on_socket_event(
        &mut self,
        cx: &mut Cx<'_, ()>,
        event: LifecycleEvent,
    ) -> Result<(), SocketError> {
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
        match event {
            LifecycleEvent::Opened(epoch) => cx.send(epoch.socket(), "login"),
            LifecycleEvent::Authenticated { epoch, first: true } => {
                cx.set_socket_interests(epoch.feed(), vec![Interest::from_wire_args("aop")])?;
            }
            LifecycleEvent::Ping(epoch) => cx.send(epoch.socket(), "ping"),
            LifecycleEvent::Reconnect(feed) => {
                cx.connect_socket(feed, "wss://snapshot.test")?;
            }
            _ => {}
        }
        Ok(())
    }
    fn on_socket_interests_released(
        &mut self,
        cx: &mut Cx<'_, ()>,
        _: safe_cex_engine::socket_lifecycle::Epoch,
        _: (),
    ) -> Result<(), SocketError> {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
        Ok(())
    }
    fn encode_socket_interests(
        &self,
        _: &Store,
        _: FeedId,
        _: bool,
        interests: &[Interest],
        emit: &mut dyn FnMut(EncodedBatch) -> Result<(), SocketError>,
    ) -> Result<(), SocketError> {
        for interest in interests {
            emit(EncodedBatch {
                text: interest.wire_args.to_string(),
                covered_keys: vec![interest.key.clone()],
            })?;
        }
        Ok(())
    }
    fn on_message(&mut self, cx: &mut Cx<'_, ()>, _: SocketId, text: &str) {
        self.deliveries.fetch_add(1, Ordering::SeqCst);
        if text == "schedule-proof-probe" {
            let required = vec![Interest::from_wire_args("aop")];
            cx.set_socket_interests_then(self.feed.unwrap(), required, &["aop"], ())
                .unwrap();
        }
        if text == "forged-time" {
            let original = cx.now;
            cx.now -= 1.;
            assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::InvalidTime));
            cx.now = original;
        }
        if text == "probe-before-login" {
            assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
        }
        if text == "retire-before-completion" {
            cx.retire_socket(
                self.feed.unwrap(),
                safe_cex_engine::socket_lifecycle::RetireReason::Closed,
            );
            assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
        }
        if text == "validated-orders-snapshot" {
            if self.snapshot_policy {
                cx.complete_socket_snapshot()
                    .expect("native snapshot proof before commit");
                assert_eq!(cx.complete_socket_snapshot(), Err(SocketError::Stale));
            }
            self.snapshots.fetch_add(1, Ordering::SeqCst);
        }
    }
}
struct Harness {
    engine: Engine<Native>,
    socket: SocketId,
    snapshots: Arc<AtomicUsize>,
    deliveries: Arc<AtomicUsize>,
    output: Vec<Action>,
}
impl Harness {
    fn new() -> Self {
        Self::with_policy(true)
    }
    fn with_policy(snapshot_policy: bool) -> Self {
        let snapshots = Arc::new(AtomicUsize::new(0));
        let deliveries = Arc::new(AtomicUsize::new(0));
        let mut engine = safe_cex_engine::test_support::seeded_engine(Native {
            feed: None,
            snapshot_policy,
            snapshots: snapshots.clone(),
            deliveries: deliveries.clone(),
        });
        engine.start(0.);
        let output: Vec<_> = engine.drain_actions().collect();
        let socket = output
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        Self {
            engine,
            socket,
            snapshots,
            deliveries,
            output,
        }
    }
    fn event(&mut self, now: f64, event: Event<'_>) {
        self.engine.handle(now, event);
        self.output.extend(self.engine.drain_actions());
    }
    fn message(&mut self, now: f64, text: &str) {
        self.event(
            now,
            Event::WsMessage {
                socket: self.socket,
                text,
            },
        );
    }
    fn login(&mut self) {
        self.event(
            4_000.,
            Event::WsOpened {
                socket: self.socket,
            },
        );
        self.message(4_001., "login-accepted");
    }
    fn keepalive_before_bound(&mut self) {
        for at in [8_000., 12_000., 16_000., 19_999.] {
            self.message(at, "pong");
        }
    }
    fn closed(&self) -> bool {
        self.output
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket} if *socket==self.socket))
    }
}
#[test]
fn accepted_login_releases_aop_without_a_native_snapshot() {
    let mut h = Harness::new();
    h.login();
    assert_eq!(
        h.output
            .iter()
            .filter(|a| matches!(a,Action::WsSend{text,..} if text=="aop"))
            .count(),
        1
    );
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 0);
    assert!(!h.closed());
}
#[test]
fn current_validated_snapshot_reaches_the_native_commit_boundary() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "validated-orders-snapshot");
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 1);
    assert_eq!(h.deliveries.load(Ordering::SeqCst), 1);
}
#[test]
fn connect_origin_snapshot_expiry_precedes_native_publication_at_equality() {
    let mut h = Harness::new();
    h.login();
    h.keepalive_before_bound();
    h.message(20_000., "validated-orders-snapshot");
    assert_eq!(
        h.snapshots.load(Ordering::SeqCst),
        0,
        "login and delayed open cannot discharge or renew the connect-origin bound"
    );
    assert_eq!(
        h.deliveries.load(Ordering::SeqCst),
        0,
        "expiry must fence BEFORE native codec/publication"
    );
    assert!(h.closed());
}
#[test]
fn login_and_pongs_without_snapshot_do_not_discharge_the_bound() {
    let mut h = Harness::new();
    h.login();
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(
        h.closed(),
        "login ACK plus pongs must leave connect-origin snapshot expiry in force"
    );
}
#[test]
fn malformed_snapshot_does_not_discharge_the_bound() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "malformed-snapshot");
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 0);
    assert!(
        h.closed(),
        "a native rejected snapshot must not complete the barrier"
    );
}
#[test]
fn retired_epoch_snapshot_never_enters_the_native_decoder() {
    let mut h = Harness::new();
    h.login();
    h.event(
        4_002.,
        Event::WsClosed {
            socket: h.socket,
            code: 1006,
        },
    );
    h.message(4_003., "validated-orders-snapshot");
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 0);
    assert_eq!(h.deliveries.load(Ordering::SeqCst), 0);
}

#[test]
fn default_registration_keeps_login_completion_semantics() {
    let mut h = Harness::with_policy(false);
    h.login();
    h.keepalive_before_bound();
    h.message(20_000., "validated-orders-snapshot");
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 1);
    assert!(!h.closed());
}
#[test]
fn completion_before_login_cannot_discharge_snapshot_bound() {
    let mut h = Harness::new();
    h.event(4_000., Event::WsOpened { socket: h.socket });
    h.message(4_001., "probe-before-login");
    h.message(4_002., "login-accepted");
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(h.closed());
}
#[test]
fn in_callback_retirement_revokes_completion_proof() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "retire-before-completion");
    assert!(h.closed());
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 0);
}
#[test]
fn accepted_snapshot_ends_deadline_and_duplicate_attempt_cannot_renew_it() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "validated-orders-snapshot");
    for at in [8_000., 12_000., 16_000., 20_000., 24_000.] {
        h.message(at, "pong");
    }
    assert!(!h.closed());
    assert_eq!(h.snapshots.load(Ordering::SeqCst), 1);
}
#[test]
fn genuine_command_cannot_borrow_snapshot_message_proof() {
    let mut h = Harness::new();
    h.login();
    h.event(
        4_002.,
        Event::Command(safe_cex_engine::Command::SetTickerSymbols { symbols: vec![] }),
    );
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(h.closed());
}

#[test]
fn real_http_and_timer_callbacks_cannot_complete_the_snapshot() {
    let mut h = Harness::new();
    h.login();
    let request = h
        .output
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    let timer = h
        .output
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 222. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.event(
        4_002.,
        Event::HttpResponse {
            request,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: "ok",
            },
        },
    );
    h.event(4_003., Event::TimerFired { timer });
    assert_eq!(
        h.deliveries.load(Ordering::SeqCst),
        2,
        "both wrong-scope callbacks really executed"
    );
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(h.closed());
}
#[test]
fn released_interest_callback_cannot_borrow_its_original_message_proof() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "schedule-proof-probe");
    let timer = h
        .output
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 4_002. => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.event(4_002., Event::TimerFired { timer });
    assert_eq!(
        h.deliveries.load(Ordering::SeqCst),
        2,
        "message and actual handoff callback execute separately"
    );
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(h.closed());
}

#[test]
fn completion_cannot_substitute_a_caller_chosen_timestamp() {
    let mut h = Harness::new();
    h.login();
    h.message(4_002., "forged-time");
    h.keepalive_before_bound();
    h.message(20_000., "pong");
    assert!(h.closed());
}
#[test]
fn reconnect_uses_fresh_bound_and_old_epoch_cannot_complete_it() {
    let mut h = Harness::new();
    h.login();
    let old = h.socket;
    let before = h.output.len();
    h.event(
        4_002.,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
    );
    let (timer, at) = h.output[before..]
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > 4_002. && *at_ms <= 5_002. => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    h.event(at, Event::TimerFired { timer });
    h.socket = h
        .output
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(h.socket, old);
    h.event(at + 1., Event::WsOpened { socket: h.socket });
    h.message(at + 2., "login-accepted");
    h.event(
        at + 3.,
        Event::WsMessage {
            socket: old,
            text: "validated-orders-snapshot",
        },
    );
    assert_eq!(h.deliveries.load(Ordering::SeqCst), 0);
    for delta in [4_000., 8_000., 12_000., 16_000., 19_999.] {
        h.message(at + delta, "pong");
    }
    h.message(at + 19_999., "validated-orders-snapshot");
    assert_eq!(
        h.snapshots.load(Ordering::SeqCst),
        1,
        "fresh epoch remains valid past the original epoch bound"
    );
    assert!(!h.closed());
}
