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
    snapshots: Arc<AtomicUsize>,
    deliveries: Arc<AtomicUsize>,
}
impl Venue for Native {
    type Pending = ();
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: safe_cex_engine::Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: safe_cex_engine::Command) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let feed = cx
            .register_socket_feed_with_snapshot(FeedPolicy {
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
            })
            .unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://snapshot.test").unwrap();
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
        if text == "validated-orders-snapshot" {
            cx.complete_socket_snapshot()
                .expect("native snapshot proof before commit");
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
        let snapshots = Arc::new(AtomicUsize::new(0));
        let deliveries = Arc::new(AtomicUsize::new(0));
        let mut engine = safe_cex_engine::test_support::seeded_engine(Native {
            feed: None,
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
