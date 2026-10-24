//! Compiling original controls for GRVT's three fixed local-setting terminals.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Notice, Outcome, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options, Pending};

const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;

struct Probe {
    inner: Grvt,
    peers: Vec<Context>,
    validated: usize,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        // Fixed settings have no startup dependency; seed only their observable Store flag.
        cx.store.set_hedged(true);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchCandles { symbol, .. } if symbol == "__peer" => {
                self.peers.push(
                    cx.admit_http_command(Policy {
                        operation: Operation::Read,
                        credential: None,
                        timeout_ms: 20_000,
                        pacing: None,
                        retry: RetryPolicy::Never,
                    })
                    .expect("genuine peer admission"),
                );
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["validate_peers"] => {
                for peer in &self.peers {
                    cx.validate_http_context(peer).unwrap();
                }
                self.validated = self.peers.len();
            }
            _ => self.inner.on_command(cx, command),
        }
    }
}

fn boot(public_only: bool) -> Engine<Probe> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Grvt::new(Options {
            public_data_only: public_only,
            ..Options::default()
        }),
        peers: vec![],
        validated: 0,
    });
    engine.start(NOW);
    assert!(engine.drain_actions().next().is_none());
    assert!(engine.store().hedged());
    engine.take_frame(NOW);
    engine
}

#[derive(Clone, Copy)]
enum Case {
    Cross,
    Isolated,
    Hedged,
    OneWay,
}
impl Case {
    fn command(self, command: u32) -> Command {
        match self {
            Self::Cross | Self::Isolated => Command::SetMarginType {
                command,
                symbol: "UNKNOWN".into(),
                isolated: matches!(self, Self::Isolated),
            },
            Self::Hedged | Self::OneWay => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: matches!(self, Self::Hedged),
            },
        }
    }
    fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Cross | Self::Isolated => {
                Some("GRVT margin mode cannot be changed through this integration")
            }
            Self::Hedged => Some("GRVT supports one-way positions only"),
            Self::OneWay => None,
        }
    }
}
fn outcomes(actions: &[Action], command: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: owner,
                result,
            }) if *owner == command => Some(Ok(result.clone())),
            Action::Notice(Notice::CommandFailed {
                command: owner,
                error,
            }) if *owner == command => Some(Err(error.clone())),
            _ => None,
        })
        .collect()
}
fn no_io(actions: &[Action]) {
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_)
            | Action::ManagedHttp(_)
            | Action::SignedOutbound { .. }
            | Action::Sign { .. }
            | Action::SignPayload { .. }
            | Action::WsSend { .. }
            | Action::WsConnect { .. }
    )));
}
fn native(engine: &Engine<Probe>, actions: &[Action], case: Case) {
    no_io(actions);
    let result = outcomes(actions, OWNER);
    assert_eq!(result.len(), 1, "one native setting terminal");
    if let Some(message) = case.refusal() {
        let error = result[0].as_ref().unwrap_err();
        assert_eq!(error.message, message);
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    } else {
        assert_eq!(result[0].as_ref().unwrap(), "[]");
    }
    assert_eq!(engine.store().hedged(), !matches!(case, Case::OneWay));
}
fn peer(engine: &mut Engine<Probe>, command: u32) {
    engine
        .handle_command(
            NOW,
            Command::FetchCandles {
                command,
                symbol: "__peer".into(),
                interval: Interval::M1,
                limit: None,
                from_ms: None,
                to_ms: None,
            },
            Some(60_000),
        )
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, command).is_empty());
}
fn capacity(case: Case) {
    let mut engine = boot(false);
    for command in 1000..1256 {
        peer(&mut engine, command);
    }
    assert_eq!(engine.venue().peers.len(), 256);
    engine
        .handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let result = outcomes(&actions, OWNER);
    assert_eq!(result.len(), 1, "exactly one shared refusal");
    let error = result[0]
        .as_ref()
        .expect_err("full genuine command pool must refuse the fixed native setting");
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandError { command: OWNER, .. })
    )));
    assert!(engine.store().hedged(), "refusal must not project one-way");
    engine.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let retired = outcomes(&actions, 1000);
    assert_eq!(retired.len(), 1);
    assert_eq!(
        retired[0].as_ref().unwrap_err().message,
        "original peer retired"
    );
    engine
        .handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    native(&engine, &actions, case);
    engine.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["validate_peers".into()],
        }),
    );
    assert!(engine.drain_actions().next().is_none());
    assert_eq!(engine.venue().validated, 255);
}
macro_rules! paired_controls {
    ($capacity:ident, $native:ident, $case:ident) => {
        #[test]
        fn $capacity() {
            capacity(Case::$case);
        }
        #[test]
        fn $native() {
            let mut engine = boot(false);
            engine
                .handle_command(NOW, Case::$case.command(OWNER), Some(60_000))
                .unwrap();
            let actions = engine.drain_actions().collect::<Vec<_>>();
            native(&engine, &actions, Case::$case);
        }
    };
}
paired_controls!(capacity_cross_refusal, native_cross_refusal, Cross);
paired_controls!(capacity_isolated_refusal, native_isolated_refusal, Isolated);
paired_controls!(capacity_hedged_refusal, native_hedged_refusal, Hedged);
paired_controls!(capacity_one_way, native_one_way, OneWay);

fn invalid_clock(case: Case) {
    let mut engine = boot(false);
    engine.handle(f64::NAN, Event::Command(case.command(OWNER)));
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let result = outcomes(&actions, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0]
        .as_ref()
        .expect_err("invalid external-event clock must refuse");
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(engine.store().hedged());
}
#[test]
fn invalid_clock_precedes_one_way_projection() {
    invalid_clock(Case::OneWay);
}
#[test]
fn invalid_clock_precedes_native_hedged_refusal() {
    invalid_clock(Case::Hedged);
}
#[test]
fn public_only_keeps_all_fixed_settings_before_credentials() {
    for case in [Case::Cross, Case::Isolated, Case::Hedged, Case::OneWay] {
        let mut engine = boot(true);
        engine
            .handle_command(NOW, case.command(OWNER), Some(60_000))
            .unwrap();
        let actions = engine.drain_actions().collect::<Vec<_>>();
        native(&engine, &actions, case);
    }
}
fn zero_wait(case: Case) {
    let mut engine = boot(false);
    assert_eq!(
        engine.handle_command(NOW, case.command(OWNER), Some(0)),
        Err(Error::Invalid)
    );
    assert!(engine.drain_actions().next().is_none());
    assert!(engine.store().hedged());
}
#[test]
fn zero_wait_precedes_one_way_projection() {
    zero_wait(Case::OneWay);
}
#[test]
fn zero_wait_precedes_native_refusal() {
    zero_wait(Case::Hedged);
}
fn duplicate(case: Case) {
    let mut engine = boot(false);
    peer(&mut engine, OWNER);
    engine
        .handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, OWNER).is_empty());
    assert!(engine.store().hedged());
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    engine.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["retire".into()],
        }),
    );
    let actions = engine.drain_actions().collect::<Vec<_>>();
    let original = outcomes(&actions, OWNER);
    assert_eq!(original.len(), 1);
    assert_eq!(
        original[0].as_ref().unwrap_err().message,
        "original peer retired"
    );
}
#[test]
fn duplicate_one_way_preserves_the_original_read_owner() {
    duplicate(Case::OneWay);
}
#[test]
fn duplicate_refusal_preserves_the_original_read_owner() {
    duplicate(Case::Hedged);
}
