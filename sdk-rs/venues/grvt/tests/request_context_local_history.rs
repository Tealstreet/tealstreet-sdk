//! Native local history decisions publish only with genuine Read custody.
use safe_cex_engine::book::Book;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::test_support::prepared_actions;
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Query, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_grvt::{Grvt, Options, Pending};
const NOW: f64 = 1_700_000_000_000.;
const PEER: &str = "__retained_history_peer__";
const RETIRE: &str = "__retire_history_peer__";
struct Probe {
    venue: Grvt,
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
    Income,
    Session,
    Market,
    Bounds,
    Range,
    Client,
    Subaccount,
}
const LOCALS: [Local; 7] = [
    Local::Income,
    Local::Session,
    Local::Market,
    Local::Bounds,
    Local::Range,
    Local::Client,
    Local::Subaccount,
];
fn engine(branch: Local, testnet: bool) -> Engine<Probe> {
    let private = matches!(
        branch,
        Local::Bounds | Local::Range | Local::Client | Local::Subaccount
    );
    let mut engine = safe_cex_engine::test_support::seeded_engine(Probe {
        venue: Grvt::new(Options {
            testnet,
            public_data_only: !private,
            key: Some("fixture-key".into()),
            ..Default::default()
        }),
        roots: vec![],
    });
    engine.store_mut().set_markets(vec![Market {
        id: "BTC_USDT_Perp".into(),
        symbol: "BTC_USDT_PERP".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
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
    if private {
        engine.handle(
            NOW,
            Event::Command(Command::FetchAccount { command: 900001 }),
        );
        let actions = prepared_actions(&mut engine, NOW);
        let requests: Vec<_> = actions
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1, "real login precedes private query");
        let login = &requests[0];
        let host = if testnet {
            "https://edge.testnet.grvt.io"
        } else {
            "https://edge.grvt.io"
        };
        assert_eq!(login.url, format!("{host}/auth/api_key/login"));
        assert_eq!(login.method, Method::Post);
        assert_eq!(login.body.as_deref(), Some(r#"{"api_key":"fixture-key"}"#));
        let body = if matches!(branch, Local::Subaccount) {
            r#"{"status":"success"}"#
        } else {
            r#"{"status":"success","sub_account_id":"777"}"#
        };
        engine.handle(
            NOW,
            Event::HttpResponse {
                request: login.id,
                response: Response {
                    status: 200,
                    body,
                    headers: "x-ts-grvt-session: fixture-session\nx-grvt-account-id: 0xaccount",
                },
            },
        );
        let requests = prepared_actions(&mut engine, NOW);
        assert!(
            requests
                .iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/account_summary"))),
            "native login installed the session and released identity"
        );
    }
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
        Local::Income => Query::Income {
            symbol: Some("UNKNOWN".into()),
            kind: Some("interest".into()),
            from_ms: Some(2.),
            to_ms: Some(1.),
            limit: None,
        },
        Local::Session => fills("UNKNOWN", Some(""), Some(-1.), Some(1.)),
        Local::Market => Query::PublicTrades {
            symbol: "UNKNOWN".into(),
            limit: None,
        },
        Local::Bounds => fills("BTC_USDT_PERP", Some(""), Some(-1.), Some(1.)),
        Local::Range => fills("BTC_USDT_PERP", None, Some(2.), Some(1.)),
        Local::Client => Query::OrderByClientId {
            symbol: "BTC_USDT_PERP".into(),
            client_order_id: "01".into(),
        },
        Local::Subaccount => fills("BTC_USDT_PERP", None, Some(0.), Some(NOW)),
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
    let error = one(actions, owner).unwrap_err();
    assert_eq!(
        error.message,
        match branch {
            Local::Income => "Unsupported GRVT income category",
            Local::Session => "GRVT query requires an authenticated session",
            Local::Market => "Unknown GRVT query market",
            Local::Bounds => "Invalid GRVT history bounds",
            Local::Range => "Invalid GRVT query range or order ID",
            Local::Client => "Invalid GRVT client order ID",
            Local::Subaccount => "GRVT query requires a subaccount",
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
    for testnet in [false, true] {
        let mut e = engine(branch, testnet);
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
        native(&ask(&mut e, NOW + 1., 700, branch), 700, branch);
        assert!(e.take_frame(NOW + 1.).is_none());
    }
}
fn clock(branch: Local) {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = engine(branch, false);
        // Lookup has no explicit to bound: an invalid clock reaches bounds first.
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
fn income_capacity() {
    capacity(Local::Income)
}
#[test]
fn income_clock() {
    clock(Local::Income)
}
#[test]
fn session_capacity() {
    capacity(Local::Session)
}
#[test]
fn session_clock() {
    clock(Local::Session)
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
fn bounds_capacity() {
    capacity(Local::Bounds)
}
#[test]
fn bounds_clock() {
    clock(Local::Bounds)
}
#[test]
fn range_capacity() {
    capacity(Local::Range)
}
#[test]
fn range_clock() {
    clock(Local::Range)
}
#[test]
fn client_capacity() {
    capacity(Local::Client)
}
#[test]
fn client_clock() {
    clock(Local::Client)
}
#[test]
fn subaccount_capacity() {
    capacity(Local::Subaccount)
}
#[test]
fn subaccount_clock() {
    clock(Local::Subaccount)
}
#[test]
fn finite_native_precedence_in_both_deployments() {
    for testnet in [false, true] {
        for branch in LOCALS {
            let mut e = engine(branch, testnet);
            native(&ask(&mut e, NOW, 700, branch), 700, branch);
            assert!(e.take_frame(NOW).is_none());
        }
    }
}
#[test]
fn native_bound_errors_and_range_before_lookup_syntax_are_preserved() {
    let mut authenticated = engine(Local::Bounds, false);
    native(
        &ask_query(
            &mut authenticated,
            NOW,
            699,
            fills("UNKNOWN", Some(""), Some(-1.), Some(1.)),
        ),
        699,
        Local::Market,
    );
    for value in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -1.,
        u64::MAX as f64,
    ] {
        let mut e = engine(Local::Bounds, false);
        native(
            &ask_query(
                &mut e,
                NOW,
                700,
                fills("BTC_USDT_PERP", Some(""), Some(value), Some(1.)),
            ),
            700,
            Local::Bounds,
        );
    }
    let mut e = engine(Local::Bounds, false);
    let error = one(
        &ask_query(
            &mut e,
            NOW,
            700,
            fills("BTC_USDT_PERP", None, Some(0.0000001), Some(1.)),
        ),
        700,
    )
    .unwrap_err();
    assert_eq!(error.message, "GRVT decimal exceeds native precision");
    assert_eq!((error.kind, error.outcome), (None, None));
    native(
        &ask_query(
            &mut e,
            NOW,
            701,
            Query::OrderByClientId {
                symbol: "BTC_USDT_PERP".into(),
                client_order_id: "".into(),
            },
        ),
        701,
        Local::Range,
    );
    for id in ["0", "01", "bad", "18446744073709551616"] {
        native(
            &ask_query(
                &mut e,
                NOW,
                702,
                Query::OrderByClientId {
                    symbol: "BTC_USDT_PERP".into(),
                    client_order_id: id.into(),
                },
            ),
            702,
            Local::Client,
        );
    }
}
#[test]
fn immediate_terminals_do_not_leak_the_owner_bound() {
    for branch in LOCALS {
        let mut e = engine(branch, false);
        for id in 1000..1400 {
            native(&ask(&mut e, NOW, id, branch), id, branch);
        }
        assert!(e.take_frame(NOW).is_none());
    }
}
#[test]
fn duplicate_id_cannot_consume_a_retained_peer() {
    for branch in LOCALS {
        let mut e = engine(branch, false);
        peer(&mut e, 700);
        assert!(terminal(&ask(&mut e, NOW, 700, branch), 700).is_empty());
        assert_eq!(
            one(&retire(&mut e, NOW + 1.), 700).unwrap_err().message,
            "retired fixture peer"
        );
        native(&ask(&mut e, NOW + 1., 700, branch), 700, branch);
    }
}
#[test]
fn saturated_remote_public_trades_keeps_native_bytes_and_no_unused_root() {
    let mut after = Vec::new();
    for testnet in [false, true] {
        let mut e = engine(Local::Market, testnet);
        for id in 1..=256 {
            peer(&mut e, id);
        }
        let actions = ask_query(
            &mut e,
            NOW,
            800,
            Query::PublicTrades {
                symbol: "BTC_USDT_PERP".into(),
                limit: Some(9000),
            },
        );
        assert_eq!(actions.len(), 1);
        let Action::Http(request) = &actions[0] else {
            panic!("remote history remains legacy")
        };
        let host = if testnet {
            "https://market-data.testnet.grvt.io"
        } else {
            "https://market-data.grvt.io"
        };
        assert_eq!(request.url, format!("{host}/lite/v1/trade_history"));
        assert_eq!(request.method, Method::Post);
        assert_eq!(
            request.headers,
            vec![("Content-Type", "application/json".into())]
        );
        assert_eq!(
            request.body.as_deref(),
            Some(r#"{"i":"BTC_USDT_Perp","et":"1700000000000000000","l":1000,"c":""}"#)
        );
        e.handle(
            NOW,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"r":[{"et":"1700000000000000000","i":"BTC_USDT_Perp","ti":"0x000123","it":false,"s":"2","p":"100"}],"n":""}"#,
                },
            },
        );
        let result = one(&e.drain_actions().collect::<Vec<_>>(), 800).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&result).unwrap(),
            serde_json::json!([
                {"id":"0x000123","symbol":"BTC_USDT_PERP","price":100.,"qty":2.,
                 "quoteQty":200.,"timestamp":NOW,"side":"sell"}
            ])
        );
        after.push(ask(&mut e, NOW, 801, Local::Market));
    }
    for actions in after {
        refused(
            &actions,
            801,
            ErrorKind::Internal,
            "Engine request capacity reached",
        );
    }
}
