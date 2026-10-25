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

/// One Pending type behind both declarations: `HANDOFF` is the only difference.
#[derive(Default)]
struct Handoff<const HANDOFF: bool> {
    feed: Option<FeedId>,
    released: u32,
}
type Full = Handoff<true>;
type Narrow = Handoff<false>;
impl<const HANDOFF: bool> Venue for Handoff<HANDOFF> {
    type Pending = Counted;
    const SOCKET_CAPABILITIES: crate::socket_lifecycle::SocketCapabilities =
        crate::socket_lifecycle::SocketCapabilities { handoff: HANDOFF };
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

fn started<const HANDOFF: bool>() -> Engine<Handoff<HANDOFF>> {
    let mut engine = crate::test_support::seeded_engine(Handoff::<HANDOFF>::default());
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

fn then<const HANDOFF: bool>(
    engine: &mut Engine<Handoff<HANDOFF>>,
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
    let mut engine = started::<true>();
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

fn interests<const HANDOFF: bool>(engine: &mut Engine<Handoff<HANDOFF>>, values: &[&str]) {
    let feed = engine.venue.feed.unwrap();
    let mut cx = Cx {
        now: engine.io.core.dispatch_now,
        store: &mut engine.store,
        io: &mut engine.io,
    };
    cx.set_socket_interests(
        feed,
        values
            .iter()
            .map(|key| Interest::from_wire_args(*key))
            .collect(),
    )
    .unwrap();
}

fn sent(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::WsSend { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

#[test]
fn construction_installs_the_declared_capabilities() {
    assert!(started::<true>().io.core.socket_capabilities.handoff);
    assert!(!started::<false>().io.core.socket_capabilities.handoff);
    assert_eq!(
        started::<true>().io.core.socket_capabilities,
        <Full as Venue>::SOCKET_CAPABILITIES
    );
    assert_eq!(
        started::<false>().io.core.socket_capabilities,
        <Narrow as Venue>::SOCKET_CAPABILITIES
    );
}

/// A venue without handoff is refused before any work: the refusal is the
/// first statement, ahead of entry activity, validation, IDs and retention.
#[test]
fn undeclared_handoff_is_refused_before_any_work() {
    let mut engine = started::<false>();
    let drops = Rc::new(Cell::new(0));
    let ids = engine.io.core.next_id;
    let actions = engine.io.core.actions.len();
    // Valid arguments that a FULL venue admits.
    assert_eq!(
        then(&mut engine, &["book"], &["book"], &drops),
        Err(SocketError::NotPermitted)
    );
    assert_eq!(drops.get(), 1);
    assert_eq!(engine.io.core.next_id, ids);
    assert!(engine.io.socket_proposals.is_empty());
    assert!(engine.io.socket_continuations.is_empty());
    assert_eq!(engine.io.core.socket_proposal_bytes, 0);
    assert_eq!(engine.io.core.actions.len(), actions);
    assert_eq!(engine.venue.released, 0);
}

#[test]
fn declared_handoff_admits_the_same_request() {
    let mut engine = started::<true>();
    let drops = Rc::new(Cell::new(0));
    then(&mut engine, &["book"], &["book"], &drops).unwrap();
    assert_eq!(drops.get(), 0, "the admitted payload is retained");
    assert_eq!(engine.io.socket_proposals.len(), 1);
}

/// Plain interests, their release and a stop work identically without handoff.
#[test]
fn plain_interests_release_and_stop_are_unchanged_without_handoff() {
    fn run<const HANDOFF: bool>() -> (Vec<String>, bool) {
        let mut engine = started::<HANDOFF>();
        interests(&mut engine, &["book", "trades"]);
        let actions: Vec<_> = engine.drain_actions().collect();
        let frames = sent(&actions).into_iter().map(str::to_owned).collect();
        let feed = engine.venue.feed.unwrap();
        let mut cx = Cx {
            now: engine.io.core.dispatch_now,
            store: &mut engine.store,
            io: &mut engine.io,
        };
        cx.stop_socket_feed(feed);
        let stopped = engine
            .drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }));
        (frames, stopped)
    }
    let full = run::<true>();
    assert!(!full.0.is_empty(), "plain interests release frames");
    assert!(full.1, "stop closes the socket");
    assert_eq!(run::<false>(), full);
}
