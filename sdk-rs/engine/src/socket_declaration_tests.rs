use super::*;
use crate::socket::Config;
use crate::socket_lifecycle::{AuthMode, AuthRecovery, Invalidation, TrafficPolicy};

/// A venue that does not declare managed sockets but tries to register one.
#[derive(Default)]
struct Undeclared {
    registered: Option<Result<FeedId, SocketError>>,
    snapshot: Option<Result<FeedId, SocketError>>,
}

fn policy() -> FeedPolicy {
    FeedPolicy {
        role: SocketChannel::Private,
        supervision: Config {
            ping_interval_ms: None,
            idle_timeout_ms: 20_000.,
            auth_timeout_ms: Some(10_000.),
            ..Config::default()
        },
        auth: AuthMode::Required,
        recovery: AuthRecovery::Reconnect,
        permanent_refusal_group: None,
        traffic: TrafficPolicy::AnyText,
        controls: ControlPolicy::Native,
        invalidation: Invalidation::None,
        subscription_interval_ms: 0.,
    }
}

impl Venue for Undeclared {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.registered = Some(cx.register_socket_feed(policy()));
        self.snapshot = Some(cx.register_socket_feed_with_snapshot(policy()));
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: u16) {}
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

/// Registration refuses before any work when the venue did not declare
/// managed sockets: no feed, resource ID, timer or action is spent.
#[test]
fn undeclared_venue_registration_is_refused_before_any_spend() {
    assert!(!<Undeclared as Venue>::managed_sockets());
    let mut engine = crate::test_support::seeded_engine(Undeclared::default());
    let ids = engine.io.core.next_id;
    engine.start(0.);
    assert_eq!(
        engine.venue.registered,
        Some(Err(SocketError::NotPermitted))
    );
    assert_eq!(engine.venue.snapshot, Some(Err(SocketError::NotPermitted)));
    assert_eq!(engine.io.core.next_id, ids);
    assert!(!engine.io.core.managed_sockets);
    assert!(!engine.io.core.managed_sockets_declared);
    assert!(engine.io.core.socket_driver.state.feeds().is_empty());
    assert!(
        engine
            .drain_actions()
            .all(|action| !matches!(action, Action::WsConnect { .. } | Action::SetTimer { .. }))
    );
}
