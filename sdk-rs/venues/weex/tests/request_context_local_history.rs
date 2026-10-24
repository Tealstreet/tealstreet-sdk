//! Local history terminals require genuine Read custody without changing native decisions.
mod support;

use safe_cex_engine::book::Book;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Query,
    ReadCredentials, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_weex::{Pending, WeexFutures};
use support::setup::{self, Options};

const NOW: f64 = 1_764_505_700_000.;
const DAY: f64 = 86_400_000.;
const MAX_TIME: f64 = 9_007_199_254_740_991.;
const PROBE: &str = "__retained_read_probe__";
const RETIRE: &str = "__retire_read_probe__";

// Only fixture commands create these genuine engine-issued roots. Native
// WEEX Query dispatch and its credential selection are delegated unchanged.
struct Probe {
    venue: WeexFutures,
    roots: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.venue.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.venue.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.venue.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.venue.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.venue.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.venue.on_response(cx, pending, response);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.venue.on_item(cx, pending, item);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.venue.on_request_failed(cx, pending, error);
    }
    fn on_sign_error(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: VenueError) {
        self.venue.on_sign_error(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.venue.on_timer(cx, timer);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.venue.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.venue.books()
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match &command {
            Command::FetchOrderBook { symbol, .. } if symbol == PROBE => {
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
fn private() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("passphrase".into()),
        ..Default::default()
    }
}
fn engine(options: Options) -> Engine<Probe> {
    setup::with_venue(options, |venue| Probe {
        venue,
        roots: vec![],
    })
}
#[derive(Clone, Copy, Debug)]
enum Local {
    Keyless,
    Lookup,
    Fills,
    Orders,
    Income,
    IncomeBudget,
    ListBudget,
}
const LOCALS: [Local; 7] = [
    Local::Keyless,
    Local::Lookup,
    Local::Fills,
    Local::Orders,
    Local::Income,
    Local::IncomeBudget,
    Local::ListBudget,
];
fn options(branch: Local) -> Options {
    if matches!(branch, Local::Keyless) {
        Options::default()
    } else {
        private()
    }
}
fn range(family: u8, from_ms: Option<f64>, to_ms: Option<f64>) -> Query {
    match family {
        0 => Query::Fills {
            symbol: Some("BTCUSDT".into()),
            order_id: None,
            from_ms,
            to_ms,
            limit: Some(1),
        },
        1 => Query::OrderHistory {
            symbol: Some("BTCUSDT".into()),
            from_ms,
            to_ms,
            limit: Some(1),
        },
        _ => Query::Income {
            symbol: None,
            kind: None,
            from_ms,
            to_ms,
            limit: Some(1),
        },
    }
}
fn lookup() -> Query {
    Query::OrderByClientId {
        symbol: "UNKNOWN".into(),
        client_order_id: String::new(),
    }
}
fn local_query(branch: Local) -> Query {
    match branch {
        Local::Keyless | Local::Fills => range(0, Some(2.), Some(1.)),
        Local::Lookup => lookup(),
        Local::Orders => range(1, Some(2.), Some(1.)),
        Local::Income => range(2, Some(2.), Some(1.)),
        Local::IncomeBudget => range(2, Some(0.), Some(10_001. * 100. * DAY)),
        Local::ListBudget => range(0, Some(0.), Some(1_001. * 7. * DAY)),
    }
}
fn query(e: &mut Engine<Probe>, now: f64, command: u32, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command, query }));
    support::managed::drain(e, now)
}
fn local(e: &mut Engine<Probe>, now: f64, command: u32, branch: Local) -> Vec<Action> {
    query(e, now, command, local_query(branch))
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
fn only_terminal(actions: &[Action], owner: u32) -> Result<String, VenueError> {
    assert_eq!(actions.len(), 1, "no companion effect: {actions:?}");
    let mut results = terminal(actions, owner);
    assert_eq!(results.len(), 1);
    results.pop().unwrap()
}
fn native(actions: &[Action], owner: u32, branch: Local) {
    let result = only_terminal(actions, owner);
    if matches!(branch, Local::Keyless) {
        assert_eq!(result.unwrap(), "[]");
        return;
    }
    let error = result.unwrap_err();
    assert_eq!(
        error.message,
        match branch {
            Local::Lookup => "WEEX client order lookup is not supported by the Rust engine",
            Local::Fills => "invalid WEEX fill range",
            Local::Orders => "invalid WEEX order range",
            Local::Income => "invalid WEEX income range",
            Local::IncomeBudget => "WEEX income range exceeds chunk budget",
            Local::ListBudget => "WEEX history range exceeds request budget",
            Local::Keyless => unreachable!(),
        }
    );
    assert_eq!((error.kind, error.outcome), (None, None));
}
fn refused(actions: &[Action], owner: u32, kind: ErrorKind, message: &str) {
    let error =
        only_terminal(actions, owner).expect_err("Read admission must refuse this local terminal");
    assert_eq!(error.message, message);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, None);
}
fn park(e: &mut Engine<Probe>, owner: u32) {
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: owner,
            symbol: PROBE.into(),
            limit: Some(1),
        }),
    );
    assert!(
        e.drain_actions().next().is_none(),
        "genuine retained root emits no HTTP or terminal"
    );
}
fn retire(e: &mut Engine<Probe>, owner: u32) {
    e.handle(
        NOW,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec![RETIRE.into()],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let error = only_terminal(&actions, owner).unwrap_err();
    assert_eq!(error.message, "retired fixture peer");
    assert_eq!((error.kind, error.outcome), (None, None));
}
fn no_store_effects(e: &mut Engine<Probe>) {
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
    assert!(e.take_frame(NOW).is_none());
}
#[test]
fn native_local_precedence_and_exact_metadata() {
    for branch in LOCALS {
        let mut e = engine(options(branch));
        native(&local(&mut e, NOW, 800, branch), 800, branch);
        no_store_effects(&mut e);
    }
    let secondary = Options {
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: "secret".into(),
            passphrase: Some("passphrase".into()),
        }),
        ..Default::default()
    };
    for opts in [
        Options::default(),
        Options {
            public_data_only: true,
            ..private()
        },
        secondary,
    ] {
        let mut e = engine(opts);
        for family in 0..3 {
            for (from, to) in [
                (Some(f64::NAN), None),
                (Some(2.), Some(1.)),
                (Some(0.), Some(MAX_TIME)),
            ] {
                native(
                    &query(&mut e, NOW, 801, range(family, from, to)),
                    801,
                    Local::Keyless,
                );
            }
        }
        native(&query(&mut e, NOW, 801, lookup()), 801, Local::Lookup);
        no_store_effects(&mut e);
    }
    let mut e = engine(private());
    for (from, to) in [
        (Some(f64::NAN), None),
        (None, Some(f64::INFINITY)),
        (Some(f64::NEG_INFINITY), None),
        (Some(-1.), None),
        (None, Some(-1.)),
        (Some(MAX_TIME + 1.), None),
        (None, Some(MAX_TIME + 1.)),
        (Some(2.), Some(1.)),
    ] {
        for (family, branch) in [(0, Local::Fills), (1, Local::Orders), (2, Local::Income)] {
            native(
                &query(&mut e, NOW, 802, range(family, from, to)),
                802,
                branch,
            );
        }
    }
    native(
        &query(
            &mut e,
            NOW,
            803,
            range(1, Some(0.), Some(1_001. * 90. * DAY)),
        ),
        803,
        Local::ListBudget,
    );
    no_store_effects(&mut e);
}
fn capacity(branch: Local) {
    let mut e = engine(options(branch));
    for owner in 1..=256 {
        park(&mut e, owner);
    }
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: 900,
            symbol: PROBE.into(),
            limit: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    refused(
        &actions,
        900,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    refused(
        &local(&mut e, NOW, 800, branch),
        800,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    retire(&mut e, 1);
    native(&local(&mut e, NOW, 800, branch), 800, branch);
    park(&mut e, 900);
    refused(
        &local(&mut e, NOW, 801, branch),
        801,
        ErrorKind::Internal,
        "Engine request capacity reached",
    );
    for owner in 2..=256 {
        retire(&mut e, owner);
    }
    retire(&mut e, 900);
    no_store_effects(&mut e);
}
#[test]
fn keyless_empty_requires_read_capacity() {
    capacity(Local::Keyless);
}
#[test]
fn unsupported_lookup_requires_read_capacity() {
    capacity(Local::Lookup);
}
#[test]
fn invalid_fills_requires_read_capacity() {
    capacity(Local::Fills);
}
#[test]
fn invalid_orders_requires_read_capacity() {
    capacity(Local::Orders);
}
#[test]
fn invalid_income_requires_read_capacity() {
    capacity(Local::Income);
}
#[test]
fn income_budget_requires_read_capacity() {
    capacity(Local::IncomeBudget);
}
#[test]
fn list_budget_requires_read_capacity() {
    capacity(Local::ListBudget);
}
#[test]
fn invalid_clock_refuses_all_seven_local_terminals() {
    for branch in LOCALS {
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut e = engine(options(branch));
            refused(
                &local(&mut e, now, 810, branch),
                810,
                ErrorKind::InvalidInput,
                "Invalid request context",
            );
            native(&local(&mut e, NOW, 810, branch), 810, branch);
            no_store_effects(&mut e);
        }
    }
}
#[test]
fn duplicate_cannot_complete_peer_and_local_ids_retire_without_expiry() {
    for branch in LOCALS {
        let mut e = engine(options(branch));
        park(&mut e, 820);
        let actions = local(&mut e, NOW, 820, branch);
        assert!(terminal(&actions, 820).is_empty());
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
        );
        retire(&mut e, 820);
        for _ in 0..300 {
            native(&local(&mut e, NOW, 820, branch), 820, branch);
        }
        for owner in 1..=256 {
            park(&mut e, owner);
        }
        no_store_effects(&mut e);
    }
}
#[test]
fn valid_remote_fallthrough_requires_capacity_then_keeps_native_wire() {
    for family in 0..3 {
        let budget_end = match family {
            0 => 1_000. * 7. * DAY,
            1 => 1_000. * 90. * DAY,
            _ => 10_000. * 100. * DAY,
        };
        for bounds in [
            (Some(0.), Some(0.)),
            (Some(MAX_TIME), Some(MAX_TIME)),
            (Some(0.), None),
            (None, Some(MAX_TIME)),
            (None, None),
            (Some(0.), Some(budget_end)),
        ] {
            let mut e = engine(private());
            for owner in 1..=256 {
                park(&mut e, owner);
            }
            let actions = query(&mut e, NOW, 830, range(family, bounds.0, bounds.1));
            let capacity = safe_cex_engine::request_context::Error::Capacity;
            refused(&actions, 830, ErrorKind::Internal, &capacity.to_string());
            retire(&mut e, 1);
            let one_free = query(&mut e, NOW, 830, range(family, bounds.0, bounds.1));
            assert_eq!(terminal(&one_free, 830), [Err(capacity.read_venue_error())]);
            assert!(one_free.iter().all(|action| matches!(
                action,
                Action::SetTimer { .. }
                    | Action::ClearTimer { .. }
                    | Action::Notice(Notice::CommandFailed { command: 830, .. })
            )));
            for action in &one_free {
                if let Action::SetTimer { timer, .. } = action {
                    assert!(one_free.iter().any(|action| matches!(action,
                        Action::ClearTimer { timer: cleared } if cleared == timer
                    )));
                }
            }
            retire(&mut e, 2);
            let actions = query(&mut e, NOW, 830, range(family, bounds.0, bounds.1));
            assert!(terminal(&actions, 830).is_empty());
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
            let route = match family {
                0 => "/capi/v3/userTrades",
                1 => "/capi/v3/order/history",
                _ => "/capi/v3/account/income",
            };
            assert!(r.url.contains(route), "{}", r.url);
            let path = &r.url[r.url.find("/capi/").unwrap()..];
            let stamp = format!("{NOW:.0}");
            let expected = setup::oracle(
                "key",
                "secret",
                "passphrase",
                r.method,
                path,
                r.body.as_deref().unwrap_or(""),
                &stamp,
            );
            for (name, value) in expected {
                assert!(
                    r.headers.iter().any(|(n, v)| *n == name && v == &value),
                    "missing {name}"
                );
            }
            assert_eq!(
                r.method,
                if family == 2 {
                    Method::Post
                } else {
                    Method::Get
                }
            );
            for owner in 3..=256 {
                retire(&mut e, owner);
            }
            no_store_effects(&mut e);
        }
    }
    for opts in [Options::default(), private()] {
        let mut e = engine(opts);
        for owner in 1..=256 {
            park(&mut e, owner);
        }
        let actions = query(
            &mut e,
            NOW,
            840,
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(0),
            },
        );
        refused(
            &actions,
            840,
            ErrorKind::Internal,
            &safe_cex_engine::request_context::Error::Capacity.to_string(),
        );
        retire(&mut e, 1);
        retire(&mut e, 2);
        let actions = query(
            &mut e,
            NOW,
            840,
            Query::PublicTrades {
                symbol: "BTCUSDT".into(),
                limit: Some(0),
            },
        );
        assert!(terminal(&actions, 840).is_empty());
        let request = actions
            .iter()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    Some(r)
                } else {
                    None
                }
            })
            .unwrap();
        assert!(
            request
                .url
                .ends_with("/capi/v3/market/trades?limit=1&symbol=BTCUSDT")
        );
        assert!(
            !request
                .headers
                .iter()
                .any(|(name, _)| *name == "ACCESS-SIGN")
        );
        assert_eq!(request.body, None);
        no_store_effects(&mut e);
    }
}
