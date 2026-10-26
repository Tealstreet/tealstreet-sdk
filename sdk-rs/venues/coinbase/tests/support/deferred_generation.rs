use crate::{Coinbase, Options, Pending, ProductKind};
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::{Action, Command, Cx, Event, Notice, Response, SocketId, TimerId, Venue};

// Exercise the real private-ready generation transition without credentials or
// changing the venue's production API. Native callbacks still use Coinbase.
struct RestartCatalogue(Coinbase);
impl Venue for RestartCatalogue {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.0.start(cx);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if matches!(command, Command::SetTickerSymbols { .. }) {
            self.0.private_ready(ProductKind::Spot, cx);
        } else {
            self.0.on_command(cx, command);
        }
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.0.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.0.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.0.on_message(cx, socket, text);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.0.on_timer(cx, timer);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.0.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.0.on_request_failed(cx, pending, error);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.0.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.0.books()
    }
}

#[test]
fn stale_product_failure_cannot_settle_the_current_generation_waiter() {
    let venue = Coinbase::new(Options {
        public_data_only: true,
        spot_only: true,
        ..Default::default()
    })
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(RestartCatalogue(venue));
    engine.start(0.);
    let initial: Vec<_> = engine.drain_actions().collect();
    let old = initial
        .iter()
        .find_map(|action| match action {
            Action::Http(packet) => Some(packet.clone()),
            _ => None,
        })
        .unwrap();
    engine.handle(
        10.,
        Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
    );
    engine.handle(
        20.,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "BTC-USD".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
    let queued: Vec<_> = engine.drain_actions().collect();
    let pacing = queued
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == 334. => Some(*timer),
            _ => None,
        })
        .expect("native REST pacing timer");
    engine.handle(
        100.,
        Event::HttpFailed {
            request: old.id,
            error: "old catalogue failed",
        },
    );
    assert!(
        !engine.drain_actions().any(|action| matches!(
            action,
            Action::Notice(
                Notice::CommandFailed { command: 7, .. } | Notice::CommandDone { command: 7, .. }
            )
        )),
        "old generation must not terminate a current waiter"
    );
    engine.handle(334., Event::TimerFired { timer: pacing });
    let current = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(packet) => Some(packet),
            _ => None,
        })
        .unwrap();
    engine.handle(
        400.,
        Event::HttpFailed {
            request: current.id,
            error: "current catalogue failed",
        },
    );
    assert_eq!(
        engine
            .drain_actions()
            .filter(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 7, .. })
            ))
            .count(),
        1
    );
}
