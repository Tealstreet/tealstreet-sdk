//! Native local history decisions publish only with genuine Read custody.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Notice, Query, Response, SocketId,
    TimerId, Venue, VenueError,
};
use safe_cex_venue_lighter::{Lighter, Options, Pending};
const NOW: f64 = 1_700_000_000_000.;
const PEER: &str = "__retained_history_peer__";
const RETIRE: &str = "__retire_history_peer__";
struct Probe {
    venue: Lighter,
    roots: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn guard_pending_placements(&self) -> bool {
        self.venue.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.venue.guard_order_mutations()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.venue.fence_uncertain_cancel_all()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx)
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket)
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text)
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code)
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response)
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item)
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error)
    }
    fn on_http_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_http_failed(cx, pending, error)
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error)
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer)
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == PEER => {
                if let Ok(root) = cx.admit_http_command(Policy {
                    operation: Operation::Read,
                    credential: None,
                    timeout_ms: 20_000,
                    pacing: None,
                    retry: RetryPolicy::Never,
                }) {
                    self.roots.push(root);
                }
            }
            Command::SetTickerSymbols { symbols } if symbols.as_slice() == [RETIRE] => {
                let root = self.roots.remove(0);
                cx.fail_context(&root, "retired fixture peer").unwrap();
            }
            _ => self.venue.on_command(cx, command),
        }
    }
}
#[derive(Clone, Copy, Debug)]
enum Local {
    Client,
    Income,
    Bounds,
    Index,
    Market,
    Empty,
    Unavailable,
}
const LOCALS: [Local; 7] = [
    Local::Client,
    Local::Income,
    Local::Bounds,
    Local::Index,
    Local::Market,
    Local::Empty,
    Local::Unavailable,
];
fn engine(branch: Local, robinhood: bool, testnet: bool) -> Engine<Probe> {
    let private = matches!(branch, Local::Unavailable);
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Lighter::new(Options {
            testnet,
            robinhood,
            public_data_only: !private,
            remote_signer: private,
            api_key_index: private.then(|| "0".into()),
            ..Default::default()
        }),
        roots: vec![],
    });
    engine.store_mut().set_markets(vec![Market {
        id: "1".into(),
        symbol: "ETH".into(),
        pretty_symbol: None,
        base: "ETH".into(),
        quote: "USDC".into(),
        active: true,
        precision: Precision {
            price: 0.01,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    engine.take_frame(NOW);
    engine
}
fn fills(symbol: &str, order: Option<&str>, from: Option<f64>, to: Option<f64>) -> Query {
    Query::Fills {
        symbol: Some(symbol.into()),
        order_id: order.map(Into::into),
        from_ms: from,
        to_ms: to,
        limit: Some(100),
    }
}
fn query(branch: Local) -> Query {
    match branch {
        Local::Client => Query::OrderByClientId {
            symbol: "UNKNOWN".into(),
            client_order_id: "caller".into(),
        },
        Local::Income => Query::Income {
            symbol: Some("UNKNOWN".into()),
            kind: Some("interest".into()),
            from_ms: Some(2.),
            to_ms: Some(1.),
            limit: None,
        },
        Local::Bounds => fills("UNKNOWN", Some("bad"), Some(2.), Some(1.)),
        Local::Index => fills("UNKNOWN", Some("bad"), None, None),
        Local::Market => fills("UNKNOWN", None, None, None),
        Local::Empty | Local::Unavailable => fills("ETH", None, None, None),
    }
}
fn ask_query(e: &mut Engine<Probe>, now: f64, command: u32, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command, query }));
    e.drain_actions().collect()
}
fn ask(e: &mut Engine<Probe>, now: f64, command: u32, branch: Local) -> Vec<Action> {
    ask_query(e, now, command, query(branch))
}
fn terminal(actions: &[Action], owner: u32) -> Vec<Result<String, VenueError>> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandDone { command, result }) if *command == owner => {
                Some(Ok(result.clone()))
            }
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(Err(error.clone()))
            }
            _ => None,
        })
        .collect()
}
fn one(actions: &[Action], owner: u32) -> Result<String, VenueError> {
    assert_eq!(
        actions.len(),
        1,
        "local branch has only its correlated terminal"
    );
    let mut terminals = terminal(actions, owner);
    assert_eq!(terminals.len(), 1);
    terminals.remove(0)
}
fn native(actions: &[Action], owner: u32, branch: Local) {
    let result = one(actions, owner);
    if matches!(branch, Local::Empty) {
        assert_eq!(result.unwrap(), "[]");
        return;
    }
    let error = result.unwrap_err();
    assert_eq!(
        error.message,
        match branch {
            Local::Client => "Lighter does not support caller client order IDs",
            Local::Income => "Lighter income supports funding, commission and realized PnL",
            Local::Bounds => "Invalid Lighter history time range",
            Local::Index => "Invalid Lighter native order index",
            Local::Market => "Lighter market not found",
            Local::Unavailable => "Lighter private account identity is unavailable",
            Local::Empty => unreachable!(),
        }
    );
    assert_eq!((error.kind, error.outcome), (None, None));
}
fn refused(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    let error = one(actions, owner).expect_err("local history must borrow shared Read authority");
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
    assert_eq!(error.message, message);
}
fn peer(e: &mut Engine<Probe>, owner: u32) {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: PEER.into(),
            limit: None,
        }),
    );
    assert!(e.drain_actions().next().is_none());
}
fn retire(e: &mut Engine<Probe>, now: f64) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    e.drain_actions().collect()
}
fn capacity(branch: Local) {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(branch, robinhood, testnet);
            for id in 1..=256 {
                peer(&mut e, id);
            }
            refused(
                &ask(&mut e, NOW, 700, branch),
                700,
                ErrorKind::Internal,
                "Engine request capacity reached",
            );
            let old = retire(&mut e, NOW + 1.);
            assert_eq!(one(&old, 1).unwrap_err().message, "retired fixture peer");
            native(&ask(&mut e, NOW + 1., 700, branch), 700, branch);
            assert!(e.take_frame(NOW + 1.).is_none());
        }
    }
}
fn clock(branch: Local) {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(branch, false, false);
        refused(
            &ask(&mut e, invalid, 701, branch),
            701,
            ErrorKind::InvalidInput,
            "Invalid request context",
        );
        native(&ask(&mut e, NOW, 701, branch), 701, branch);
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn client_capacity() {
    capacity(Local::Client)
}
#[test]
fn income_capacity() {
    capacity(Local::Income)
}
#[test]
fn bounds_capacity() {
    capacity(Local::Bounds)
}
#[test]
fn index_capacity() {
    capacity(Local::Index)
}
#[test]
fn market_capacity() {
    capacity(Local::Market)
}
#[test]
fn empty_capacity() {
    capacity(Local::Empty)
}
#[test]
fn unavailable_capacity() {
    capacity(Local::Unavailable)
}
#[test]
fn client_clock() {
    clock(Local::Client)
}
#[test]
fn income_clock() {
    clock(Local::Income)
}
#[test]
fn bounds_clock() {
    clock(Local::Bounds)
}
#[test]
fn index_clock() {
    clock(Local::Index)
}
#[test]
fn market_clock() {
    clock(Local::Market)
}
#[test]
fn empty_clock() {
    clock(Local::Empty)
}
#[test]
fn unavailable_clock() {
    clock(Local::Unavailable)
}
#[test]
fn finite_native_precedence_and_output_in_all_deployments() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            for branch in LOCALS {
                let mut e = engine(branch, robinhood, testnet);
                native(&ask(&mut e, NOW, 700, branch), 700, branch);
                assert!(e.take_frame(NOW).is_none());
            }
        }
    }
}
#[test]
fn invalid_bounds_keep_native_precedence_before_bad_index_and_market() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.] {
        let mut e = engine(Local::Bounds, false, false);
        let actions = ask_query(
            &mut e,
            NOW,
            700,
            fills("UNKNOWN", Some("bad"), Some(value), None),
        );
        native(&actions, 700, Local::Bounds);
    }
}
#[test]
fn immediate_terminals_do_not_leak_the_owner_bound() {
    for branch in LOCALS {
        let mut e = engine(branch, false, false);
        for id in 1000..1400 {
            native(&ask(&mut e, NOW, id, branch), id, branch);
        }
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn duplicate_id_cannot_consume_a_retained_peer() {
    for branch in LOCALS {
        let mut e = engine(branch, false, false);
        peer(&mut e, 700);
        let duplicate = ask(&mut e, NOW, 700, branch);
        assert!(terminal(&duplicate, 700).is_empty());
        let retired = retire(&mut e, NOW + 1.);
        assert_eq!(
            one(&retired, 700).unwrap_err().message,
            "retired fixture peer"
        );
        native(&ask(&mut e, NOW + 1., 700, branch), 700, branch);
    }
}
#[test]
fn saturated_remote_public_trades_refuses_before_preparation() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let mut e = engine(Local::Empty, robinhood, testnet);
            for id in 1..=256 {
                peer(&mut e, id);
            }
            let actions = ask_query(
                &mut e,
                NOW,
                800,
                Query::PublicTrades {
                    symbol: "ETH".into(),
                    limit: Some(999),
                },
            );
            assert!(
                !actions
                    .iter()
                    .any(|action| matches!(action, Action::Http(_) | Action::ManagedHttp(_)))
            );
            assert_eq!(actions.iter().filter(|action| matches!(action, Action::Notice(Notice::CommandFailed { command: 800, error }) if error.kind == Some(ErrorKind::Internal))).count(), 1);
        }
    }
}
