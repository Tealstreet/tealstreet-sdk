//! Native local history decisions publish only with genuine Read custody.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Interval, Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_paradex::{Options, Paradex, Pending};
const NOW: f64 = 1_700_000_000_000.;
const PEER: &str = "__retained_history_peer__";
const RETIRE: &str = "__retire_history_peer__";
struct Probe {
    venue: Paradex,
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
    Interval,
    Market,
    Book,
}
const LOCALS: [Local; 3] = [Local::Interval, Local::Market, Local::Book];
fn engine(testnet: bool) -> Engine<Probe> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Paradex::new(Options {
            testnet,
            public_data_only: true,
            ..Default::default()
        }),
        roots: vec![],
    });
    e.store_mut().set_markets(vec![Market {
        id: "BTC-USD-PERP".into(),
        symbol: "BTC-USD-PERP".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USD".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 100.,
            },
            leverage: MinMax { min: 1., max: 50. },
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
    e.take_frame(NOW);
    e
}
fn command(owner: u32, branch: Local) -> Command {
    match branch {
        Local::Interval | Local::Market => Command::FetchCandles {
            command: owner,
            symbol: "UNKNOWN".into(),
            interval: if matches!(branch, Local::Interval) {
                Interval::S1
            } else {
                Interval::M1
            },
            from_ms: Some(0.),
            to_ms: Some(NOW),
            limit: None,
        },
        Local::Book => Command::FetchOrderBook {
            command: owner,
            symbol: "BTC-USD-PERP".into(),
            limit: Some(7),
        },
    }
}
fn ask(e: &mut Engine<Probe>, now: f64, owner: u32, branch: Local) -> Vec<Action> {
    e.handle(now, Event::Command(command(owner, branch)));
    e.drain_actions().collect()
}
fn native(actions: &[Action], owner: u32, branch: Local) {
    let error = one(actions, owner).unwrap_err();
    assert_eq!(
        error.message,
        match branch {
            Local::Interval => "Paradex does not support 1s candles",
            Local::Market => "Unknown Paradex candle market",
            Local::Book => unreachable!(),
        }
    );
    assert_eq!((error.kind, error.outcome), (None, None));
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
    for testnet in [false, true] {
        let mut e = engine(testnet);
        for id in 1..=256 {
            peer(&mut e, id);
        }
        refused(
            &ask(&mut e, NOW, 700, branch),
            700,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
        assert_eq!(
            one(&retire(&mut e, NOW + 1.), 1).unwrap_err().message,
            "retired fixture peer"
        );
        if !matches!(branch, Local::Book) {
            native(&ask(&mut e, NOW + 1., 700, branch), 700, branch);
        } else {
            e.handle(NOW + 1., Event::Command(command(700, branch)));
            let actions = prepared_actions(&mut e, NOW + 1.);
            assert!(
                actions
                    .iter()
                    .any(|a| matches!(a,Action::Http(r) if r.url.contains("/orderbook/")))
            );
        }
        assert!(e.take_frame(NOW + 1.).is_none());
    }
}
fn clock(branch: Local) {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(false);
        refused(
            &ask(&mut e, invalid, 701, branch),
            701,
            ErrorKind::InvalidInput,
            "Invalid request context",
        );
        if matches!(branch, Local::Book) {
            e.handle(NOW, Event::Command(command(701, branch)));
            assert!(
                prepared_actions(&mut e, NOW)
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.contains("/orderbook/")))
            );
        } else {
            native(&ask(&mut e, NOW, 701, branch), 701, branch);
        }
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn interval_capacity() {
    capacity(Local::Interval)
}
#[test]
fn interval_clock() {
    clock(Local::Interval)
}
#[test]
fn market_capacity() {
    capacity(Local::Market)
}
#[test]
fn market_clock() {
    clock(Local::Market)
}
#[test]
fn book_capacity() {
    capacity(Local::Book)
}
#[test]
fn book_clock() {
    clock(Local::Book)
}
#[test]
fn native_candle_refusals_keep_interval_precedence_on_both_networks() {
    for testnet in [false, true] {
        for branch in [Local::Interval, Local::Market] {
            let mut e = engine(testnet);
            native(&ask(&mut e, NOW, 700, branch), 700, branch);
        }
    }
}
#[test]
fn duplicate_owner_is_not_consumed() {
    for branch in LOCALS {
        let mut e = engine(false);
        peer(&mut e, 700);
        assert!(terminal(&ask(&mut e, NOW, 700, branch), 700).is_empty());
        assert_eq!(
            one(&retire(&mut e, NOW + 1.), 700).unwrap_err().message,
            "retired fixture peer"
        );
    }
}
#[test]
fn book_success_keeps_native_managed_get_and_schema() {
    for testnet in [false, true] {
        let mut e = engine(testnet);
        e.handle(NOW, Event::Command(command(700, Local::Book)));
        let actions = prepared_actions(&mut e, NOW);
        let requests: Vec<_> = actions
            .iter()
            .filter_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let r = requests[0];
        let host = if testnet {
            "https://api.testnet.paradex.trade/v1"
        } else {
            "https://api.prod.paradex.trade/v1"
        };
        assert_eq!(r.url, format!("{host}/orderbook/BTC-USD-PERP?depth=7"));
        assert_eq!(r.method, Method::Get);
        assert!(r.headers.is_empty());
        assert!(r.body.is_none());
        e.handle(
            NOW + 1.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"bids":[["100","2"]],"asks":[["101","3"]]}"#,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        let result = terminal(&actions, 700).pop().unwrap().unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&result).unwrap(),
            serde_json::json!({"bids":[{"price":100.,"amount":2.,"total":200.}],"asks":[{"price":101.,"amount":3.,"total":303.}]})
        );
    }
}
#[test]
fn saturated_remote_candles_keep_native_legacy_bytes_without_unused_root() {
    for testnet in [false, true] {
        let mut e = engine(testnet);
        for id in 1..=256 {
            peer(&mut e, id);
        }
        e.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command: 800,
                symbol: "BTC-USD-PERP".into(),
                interval: Interval::M1,
                from_ms: Some(0.),
                to_ms: Some(NOW),
                limit: Some(3),
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(actions.len(), 1);
        let Action::Http(r) = &actions[0] else {
            panic!("remote candles remain legacy")
        };
        let host = if testnet {
            "https://api.testnet.paradex.trade/v1"
        } else {
            "https://api.prod.paradex.trade/v1"
        };
        assert_eq!(
            r.url,
            format!(
                "{host}/markets/klines?symbol=BTC-USD-PERP&resolution=1&start_at=0&end_at=1700000000000&price_kind=last"
            )
        );
        assert!(r.headers.is_empty());
        assert_eq!(r.method, Method::Get);
        assert!(r.body.is_none());
    }
}
