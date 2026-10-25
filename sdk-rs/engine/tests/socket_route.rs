//! Route rejection seam (`docs/plans/RUST_BITGET_MANAGED_SOCKET_ROUTING.md`):
//! `SocketContext.reject_route` is wire-neutral when false, and a host-only
//! `WsRouteExhausted` reaches the venue once, defaulting to today's close.
mod support;
use safe_cex_engine::book::Book;
use safe_cex_engine::{
    Action, Command, Cx, Engine, Event, ROUTE_EXHAUSTED_CLOSE, Response, SocketChannel, SocketId,
    SocketSpec, TimerId, Venue,
};

#[derive(Default)]
struct Feed {
    reject: bool,
    socket: Option<SocketId>,
    closes: Vec<(SocketId, u16)>,
    exhausted: Vec<SocketId>,
}

impl Venue for Feed {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        let mut spec = SocketSpec::new("wss://feed.test/private")
            .channel(SocketChannel::Private)
            .feed("private", true);
        if self.reject {
            spec = spec.reject_route();
        }
        self.socket = Some(cx.connect(spec));
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, socket: SocketId, code: u16) {
        self.closes.push((socket, code));
    }
    fn on_route_exhausted(&mut self, _: &mut Cx<'_, ()>, socket: SocketId) {
        self.exhausted.push(socket);
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

/// The default hook, exactly as a venue that does not opt in inherits it.
#[derive(Default)]
struct Default_ {
    socket: Option<SocketId>,
    closes: Vec<(SocketId, u16)>,
}

impl Venue for Default_ {
    type Pending = ();
    fn start(&mut self, cx: &mut Cx<'_, ()>) {
        self.socket = Some(
            cx.connect(
                SocketSpec::new("wss://feed.test/private")
                    .channel(SocketChannel::Private)
                    .feed("private", false)
                    .reject_route(),
            ),
        );
    }
    fn on_open(&mut self, _: &mut Cx<'_, ()>, _: SocketId) {}
    fn on_message(&mut self, _: &mut Cx<'_, ()>, _: SocketId, _: &str) {}
    fn on_close(&mut self, _: &mut Cx<'_, ()>, socket: SocketId, code: u16) {
        self.closes.push((socket, code));
    }
    fn on_response(&mut self, _: &mut Cx<'_, ()>, _: (), _: Response<'_>) {}
    fn on_request_failed(&mut self, _: &mut Cx<'_, ()>, _: (), _: &str) {}
    fn on_timer(&mut self, _: &mut Cx<'_, ()>, _: TimerId) {}
    fn on_command(&mut self, _: &mut Cx<'_, ()>, _: Command) {}
    fn book(&self, _: &str) -> Option<&Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        Box::new(std::iter::empty())
    }
}

fn contexts<V: Venue>(engine: &mut Engine<V>) -> Vec<String> {
    engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::WsConnect { context, .. } => {
                Some(serde_json::to_string(&context.expect("feed context")).unwrap())
            }
            _ => None,
        })
        .collect()
}

#[test]
fn reject_route_is_on_the_wire_only_when_set() {
    let mut plain = support::seeded_engine(Feed::default());
    plain.start(1.);
    assert_eq!(
        contexts(&mut plain),
        [r#"{"feed":"private","recover":true}"#]
    );
    let mut rejecting = support::seeded_engine(Feed {
        reject: true,
        ..Feed::default()
    });
    rejecting.start(1.);
    assert_eq!(
        contexts(&mut rejecting),
        [r#"{"feed":"private","recover":true,"rejectRoute":true}"#],
        "reject_route composes with recover; both are carried"
    );
}

#[test]
fn route_exhaustion_reaches_an_opted_in_venue_once_and_is_not_a_close() {
    let mut engine = support::seeded_engine(Feed {
        reject: true,
        ..Feed::default()
    });
    engine.start(1.);
    engine.drain_actions().for_each(drop);
    let socket = engine.venue().socket.unwrap();
    engine.handle(2., Event::WsRouteExhausted { socket });
    assert_eq!(engine.venue().exhausted, [socket]);
    assert!(
        engine.venue().closes.is_empty(),
        "exhaustion is not a close"
    );
}

#[test]
fn a_venue_that_does_not_opt_in_keeps_todays_close_handling() {
    let mut engine = support::seeded_engine(Default_::default());
    engine.start(1.);
    engine.drain_actions().for_each(drop);
    let socket = engine.venue().socket.unwrap();
    engine.handle(2., Event::WsRouteExhausted { socket });
    assert_eq!(engine.venue().closes, [(socket, ROUTE_EXHAUSTED_CLOSE)]);
}
