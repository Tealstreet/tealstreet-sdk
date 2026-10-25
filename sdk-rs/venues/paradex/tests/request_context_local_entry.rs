//! Genuine entry authority for fixed settings and pre-I/O native refusals.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::StpMode;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Notice, Outcome, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex, Pending};
const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
struct Probe {
    inner: Paradex,
    peers: Vec<Context>,
    delay: f64,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.inner.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.inner.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId) {
        self.inner.on_open(cx, s)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, c: u16) {
        self.inner.on_close(cx, s, c)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, s: SocketId, t: &str) {
        self.inner.on_message(cx, s, t)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, t: TimerId) {
        self.inner.on_timer(cx, t)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, r: Response<'_>) {
        self.inner.on_response(cx, p, r)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, p: &mut Pending, s: &str) {
        self.inner.on_item(cx, p, s)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, s: &str) {
        self.inner.on_request_failed(cx, p, s)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_http_failed(cx, p, e)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, p: Pending, e: VenueError) {
        self.inner.on_sign_error(cx, p, e)
    }
    fn book(&self, s: &str) -> Option<&Book> {
        self.inner.book(s)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == "__peer" => {
                if let Ok(c) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.peers.push(c)
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["retire"] => {
                cx.fail_context(&self.peers.remove(0), "original peer retired")
                    .unwrap();
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == ["expire"] => {
                self.delay = 120_000.
            }
            _ => {
                cx.now += std::mem::take(&mut self.delay);
                self.inner.on_command(cx, command)
            }
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Local {
    UnsupportedRead,
    UnsupportedWrite,
    MarginRefusal,
    MarginSuccess,
    ModeRefusal,
    ModeSuccess,
    MissingRead,
    MissingWrite,
}
const LOCALS: [Local; 8] = [
    Local::UnsupportedRead,
    Local::UnsupportedWrite,
    Local::MarginRefusal,
    Local::MarginSuccess,
    Local::ModeRefusal,
    Local::ModeSuccess,
    Local::MissingRead,
    Local::MissingWrite,
];
fn engine(testnet: bool) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Paradex::new(Options {
            testnet,
            ..Default::default()
        }),
        peers: vec![],
        delay: 0.,
    });
    e.take_frame(NOW);
    e
}
fn command(id: u32, local: Local) -> Command {
    match local {
        Local::UnsupportedRead => Command::FetchStpMode { command: id },
        Local::UnsupportedWrite => Command::SetStpMode {
            command: id,
            mode: StpMode::ExpireBoth,
        },
        Local::MarginRefusal | Local::MarginSuccess => Command::SetMarginType {
            command: id,
            symbol: "UNKNOWN".into(),
            isolated: matches!(local, Local::MarginRefusal),
        },
        Local::ModeRefusal | Local::ModeSuccess => Command::SetPositionMode {
            command: id,
            symbol: Some("UNKNOWN".into()),
            hedged: matches!(local, Local::ModeRefusal),
        },
        Local::MissingRead => Command::FetchLeverage {
            command: id,
            symbol: "UNKNOWN".into(),
        },
        Local::MissingWrite => Command::CancelOrders {
            command: id,
            ids: vec!["native-order".into()],
        },
    }
}
fn ask(e: &mut Engine<Probe>, now: f64, local: Local) -> Vec<Action> {
    e.handle(now, Event::Command(command(OWNER, local)));
    e.drain_actions().collect()
}
fn outcomes(actions: &[Action], id: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == id => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == id => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn one(actions: &[Action], id: u32) -> Result<String, VenueError> {
    assert_eq!(
        actions.len(),
        1,
        "no I/O, extra terminal, timer or Store effect: {actions:?}"
    );
    let mut results = outcomes(actions, id);
    assert_eq!(results.len(), 1);
    results.remove(0)
}
fn read(local: Local) -> bool {
    matches!(local, Local::UnsupportedRead | Local::MissingRead)
}
fn native(actions: &[Action], local: Local) {
    let result = one(actions, OWNER);
    if matches!(local, Local::MarginSuccess | Local::ModeSuccess) {
        assert_eq!(result.unwrap(), "[]");
        return;
    }
    let e = result.unwrap_err();
    assert_eq!(
        e.message,
        match local {
            Local::UnsupportedRead | Local::UnsupportedWrite =>
                "This query or control is not supported by Paradex",
            Local::MarginRefusal => "Paradex isolated account routing is unsupported",
            Local::ModeRefusal => "Paradex supports one-way mode only",
            Local::MissingRead | Local::MissingWrite =>
                "Paradex private account is not initialized",
            _ => unreachable!(),
        }
    );
    assert_eq!(
        e.kind,
        if matches!(local, Local::MissingRead | Local::MissingWrite) {
            None
        } else {
            Some(ErrorKind::Unsupported)
        }
    );
    assert_eq!(e.outcome, Some(Outcome::NotSent));
}
fn peer(e: &mut Engine<Probe>, id: u32) {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: id,
            symbol: "__peer".into(),
            limit: None,
        }),
    );
    assert!(e.drain_actions().next().is_none());
}
fn refused(actions: &[Action], local: Local, kind: ErrorKind, message: &str) {
    let e = one(actions, OWNER).expect_err("shared entry must refuse before native result");
    assert_eq!(e.message, message);
    assert_eq!(e.kind, Some(kind));
    assert_eq!(
        e.outcome,
        if read(local) {
            None
        } else {
            Some(Outcome::NotSent)
        }
    );
}
#[test]
fn native_local_results_on_both_networks() {
    for testnet in [false, true] {
        for local in LOCALS {
            let mut e = engine(testnet);
            native(&ask(&mut e, NOW, local), local);
            assert!(e.take_frame(NOW).is_none());
        }
    }
}
fn capacity(local: Local) {
    {
        let mut e = engine(false);
        for id in 1..=256 {
            peer(&mut e, id)
        }
        refused(
            &ask(&mut e, NOW, local),
            local,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["retire".into()],
            }),
        );
        assert_eq!(
            one(&e.drain_actions().collect::<Vec<_>>(), 1)
                .unwrap_err()
                .message,
            "original peer retired"
        );
        native(&ask(&mut e, NOW, local), local);
        assert!(e.take_frame(NOW).is_none());
    }
}
fn clock(local: Local) {
    {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut e = engine(false);
            refused(
                &ask(&mut e, now, local),
                local,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
            native(&ask(&mut e, NOW, local), local);
        }
    }
}
#[test]
fn duplicate_owner_preserves_original_root_and_publishes_no_second_terminal() {
    for local in LOCALS {
        let mut e = engine(false);
        peer(&mut e, OWNER);
        assert!(outcomes(&ask(&mut e, NOW, local), OWNER).is_empty());
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["retire".into()],
            }),
        );
        assert_eq!(
            one(&e.drain_actions().collect::<Vec<_>>(), OWNER)
                .unwrap_err()
                .message,
            "original peer retired"
        );
    }
}
fn expiry(local: Local) {
    {
        let mut e = engine(false);
        e.handle(
            NOW,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["expire".into()],
            }),
        );
        e.drain_actions().for_each(drop);
        e.handle_command(NOW, command(OWNER, local), Some(120_000))
            .unwrap();
        refused(
            &e.drain_actions().collect::<Vec<_>>(),
            local,
            ErrorKind::Timeout,
            "Request context timed out",
        );
        native(&ask(&mut e, NOW, local), local);
    }
}

macro_rules! entry_controls { ($(($capacity:ident, $clock:ident, $local:ident)),* $(,)?) => {$ (
#[test] fn $capacity(){capacity(Local::$local)}
#[test] fn $clock(){clock(Local::$local)}
)*};}
entry_controls! {
(unsupported_read_capacity,unsupported_read_clock,UnsupportedRead),
(unsupported_write_capacity,unsupported_write_clock,UnsupportedWrite),
(margin_refusal_capacity,margin_refusal_clock,MarginRefusal),
(margin_success_capacity,margin_success_clock,MarginSuccess),
(mode_refusal_capacity,mode_refusal_clock,ModeRefusal),
(mode_success_capacity,mode_success_clock,ModeSuccess),
(missing_read_capacity,missing_read_clock,MissingRead),
(missing_write_capacity,missing_write_clock,MissingWrite),
}
#[test]
fn margin_success_expired_receipt() {
    expiry(Local::MarginSuccess)
}
#[test]
fn mode_success_expired_receipt() {
    expiry(Local::ModeSuccess)
}
