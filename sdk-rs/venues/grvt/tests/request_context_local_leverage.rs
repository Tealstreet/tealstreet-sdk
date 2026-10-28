//! Original controls for GRVT's two synchronous FetchLeverage Read refusals.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Notice, Response, SocketId,
    TimerId, Venue, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options, Pending};

const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
const KNOWN: &str = "BTC_USDT_Perp";

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
        // Seed only the Store prerequisite; no account login or tested terminal is simulated.
        cx.store.set_markets(vec![market()]);
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

fn market() -> Market {
    Market {
        id: KNOWN.into(),
        symbol: KNOWN.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: Some(false),
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn boot(options: Options) -> Engine<Probe> {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Grvt::new(options),
        peers: vec![],
        validated: 0,
    });
    engine.start(NOW);
    assert!(engine.drain_actions().next().is_none());
    unchanged(&engine);
    engine.take_frame(NOW);
    engine
}
#[derive(Clone, Copy)]
enum Case {
    UnknownMarket,
    MissingAccount,
}
impl Case {
    fn command(self, command: u32) -> Command {
        Command::FetchLeverage {
            command,
            symbol: match self {
                Self::UnknownMarket => "UNKNOWN",
                Self::MissingAccount => KNOWN,
            }
            .into(),
        }
    }
    fn message(self) -> &'static str {
        match self {
            Self::UnknownMarket => "Unknown GRVT leverage market",
            Self::MissingAccount => "GRVT private account is not authenticated",
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
            | Action::WsSend { .. }
            | Action::WsConnect { .. }
    )));
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandError { command: OWNER, .. })
    )));
}
fn unchanged(engine: &Engine<Probe>) {
    assert_eq!(engine.store().markets(), &[market()]);
    assert!(engine.store().hedged());
}
fn native(engine: &Engine<Probe>, actions: &[Action], case: Case) {
    no_io(actions);
    let result = outcomes(actions, OWNER);
    assert_eq!(result.len(), 1, "one exact native refusal");
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.message, case.message());
    assert_eq!(error.kind, None);
    assert_eq!(error.outcome, None);
    assert_eq!(error.venue_code, None);
    assert_eq!(error.retry_after_ms, None);
    unchanged(engine);
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
    let mut engine = boot(Options::default());
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
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    unchanged(&engine);
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
            let mut engine = boot(Options::default());
            engine
                .handle_command(NOW, Case::$case.command(OWNER), Some(60_000))
                .unwrap();
            let actions = engine.drain_actions().collect::<Vec<_>>();
            native(&engine, &actions, Case::$case);
        }
    };
}
paired_controls!(
    capacity_precedes_unknown_market,
    unknown_market_precedes_missing_account,
    UnknownMarket
);
paired_controls!(
    capacity_precedes_missing_account,
    known_market_without_account_keeps_native_failure,
    MissingAccount
);
fn invalid_clock(case: Case) {
    let mut engine = boot(Options::default());
    engine.handle(f64::NAN, Event::Command(case.command(OWNER)));
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    let result = outcomes(&actions, OWNER);
    assert_eq!(result.len(), 1);
    let error = result[0].as_ref().unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, None);
    unchanged(&engine);
}
#[test]
fn invalid_clock_precedes_market_diagnostic() {
    invalid_clock(Case::UnknownMarket);
}
#[test]
fn invalid_clock_precedes_account_diagnostic() {
    invalid_clock(Case::MissingAccount);
}
#[test]
fn public_only_keeps_market_before_account_precedence() {
    for case in [Case::UnknownMarket, Case::MissingAccount] {
        let mut engine = boot(Options {
            public_data_only: true,
            ..Options::default()
        });
        engine
            .handle_command(NOW, case.command(OWNER), Some(60_000))
            .unwrap();
        let actions = engine.drain_actions().collect::<Vec<_>>();
        native(&engine, &actions, case);
    }
}
#[test]
fn configured_credentials_do_not_bypass_the_native_local_refusals() {
    for case in [Case::UnknownMarket, Case::MissingAccount] {
        let mut engine = boot(Options {
            key: Some("fixture-api-key".into()),
            secret: Some(format!("0x{}", "01".repeat(32))),
            trading_account_id: Some("777".into()),
            ..Options::default()
        });
        engine
            .handle_command(NOW, case.command(OWNER), Some(60_000))
            .unwrap();
        let actions = engine.drain_actions().collect::<Vec<_>>();
        native(&engine, &actions, case);
    }
}
fn zero_wait(case: Case) {
    let mut engine = boot(Options::default());
    assert_eq!(
        engine.handle_command(NOW, case.command(OWNER), Some(0)),
        Err(Error::Invalid)
    );
    assert!(engine.drain_actions().next().is_none());
    unchanged(&engine);
}
#[test]
fn zero_wait_precedes_market_diagnostic() {
    zero_wait(Case::UnknownMarket);
}
#[test]
fn zero_wait_precedes_account_diagnostic() {
    zero_wait(Case::MissingAccount);
}
fn duplicate(case: Case) {
    let mut engine = boot(Options::default());
    peer(&mut engine, OWNER);
    engine
        .handle_command(NOW, case.command(OWNER), Some(60_000))
        .unwrap();
    let actions = engine.drain_actions().collect::<Vec<_>>();
    no_io(&actions);
    assert!(outcomes(&actions, OWNER).is_empty());
    assert!(
        matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    unchanged(&engine);
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
fn duplicate_unknown_market_keeps_the_original_read_owner() {
    duplicate(Case::UnknownMarket);
}
#[test]
fn duplicate_missing_account_keeps_the_original_read_owner() {
    duplicate(Case::MissingAccount);
}
