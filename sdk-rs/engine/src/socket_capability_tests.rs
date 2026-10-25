use super::*;
use crate::socket::Config;
use crate::socket_lifecycle::{AuthMode, AuthRecovery, Invalidation, TrafficPolicy};
use std::cell::Cell;
use std::rc::Rc;

/// A handoff payload that counts its own drops.
struct Counted(Rc<Cell<u32>>);
impl Drop for Counted {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[derive(Default)]
struct Handoff {
    feed: Option<FeedId>,
    released: u32,
}
impl Venue for Handoff {
    type Pending = Counted;
    fn start(&mut self, cx: &mut Cx<'_, Counted>) {
        let feed = cx
            .register_socket_feed(FeedPolicy {
                role: SocketChannel::Public,
                supervision: Config {
                    ping_interval_ms: None,
                    idle_timeout_ms: 20_000.,
                    ..Config::default()
                },
                auth: AuthMode::None,
                recovery: AuthRecovery::Reconnect,
                permanent_refusal_group: None,
                traffic: TrafficPolicy::AnyText,
                controls: ControlPolicy::None,
                invalidation: Invalidation::None,
                subscription_interval_ms: 0.,
            })
            .unwrap();
        self.feed = Some(feed);
        cx.connect_socket(feed, "wss://fixture.invalid").unwrap();
    }
    fn on_socket_event(
        &mut self,
        _: &mut Cx<'_, Counted>,
        _: LifecycleEvent,
    ) -> Result<(), SocketError> {
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
    fn on_socket_interests_released(
        &mut self,
        _: &mut Cx<'_, Counted>,
        _: Epoch,
        _: Counted,
    ) -> Result<(), SocketError> {
        self.released += 1;
        Ok(())
    }
    fn on_open(&mut self, _: &mut Cx<'_, Counted>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, Counted>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, Counted>, _: SocketId, _: u16) {}
    fn on_response(&mut self, _: &mut Cx<'_, Counted>, _: Counted, _: Response<'_>) {}
    fn on_timer(&mut self, _: &mut Cx<'_, Counted>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, Counted>, _: Command) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, Counted>, _: Counted, _: &str) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn started() -> Engine<Handoff> {
    let mut engine = crate::test_support::seeded_engine(Handoff::default());
    engine.start(0.);
    engine.drain_actions().for_each(drop);
    let epoch = engine
        .io
        .core
        .socket_driver
        .state
        .epoch(engine.venue.feed.unwrap())
        .unwrap();
    engine.handle(
        1.,
        Event::WsOpened {
            socket: epoch.socket(),
        },
    );
    engine.drain_actions().for_each(drop);
    engine
}

fn then(
    engine: &mut Engine<Handoff>,
    values: &[&str],
    required: &[&str],
    drops: &Rc<Cell<u32>>,
) -> Result<u64, SocketError> {
    let feed = engine.venue.feed.unwrap();
    let mut cx = Cx {
        now: engine.io.core.dispatch_now,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.set_socket_interests_then(
        feed,
        values
            .iter()
            .map(|key| Interest::from_wire_args(*key))
            .collect(),
        required,
        Counted(drops.clone()),
    )
}

/// Every refusal of a handoff drops the caller's payload exactly once and
/// retains nothing: no proposal, continuation, ID, byte charge or action.
#[test]
fn refused_handoff_drops_its_payload_once_and_retains_nothing() {
    let mut engine = started();
    let drops = Rc::new(Cell::new(0));
    let ids = engine.io.core.next_id;
    let actions = engine.io.core.actions.len();
    let refusals = [
        (vec!["book"], vec!["missing"], SocketError::InvalidInterest),
        (
            vec!["book"],
            vec!["book", "book"],
            SocketError::InvalidInterest,
        ),
        (
            vec!["book", "book"],
            vec!["book"],
            SocketError::InvalidInterest,
        ),
    ];
    for (n, (values, required, error)) in refusals.into_iter().enumerate() {
        assert_eq!(then(&mut engine, &values, &required, &drops), Err(error));
        assert_eq!(drops.get(), n as u32 + 1, "refusal {n} drops once");
        assert_eq!(engine.io.core.next_id, ids);
        assert!(engine.io.socket_proposals.is_empty());
        assert!(engine.io.socket_continuations.is_empty());
        assert_eq!(engine.io.core.socket_proposal_bytes, 0);
        assert_eq!(engine.io.core.actions.len(), actions);
    }
    assert_eq!(engine.venue.released, 0);
}
