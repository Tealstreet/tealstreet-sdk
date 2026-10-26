mod support;

use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Notice, Outcome, PublicFeedPolicy,
    Query, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bitunix::{BitunixFutures, Pending};

fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
struct Probe {
    inner: BitunixFutures,
    held: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Pending>, pending: &mut Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            if symbols.is_empty() {
                for root in self.held.drain(..) {
                    cx.finish_http_context(&root).unwrap();
                }
            } else {
                assert!(self.held.is_empty());
                for _ in 0..256 {
                    self.held
                        .push(cx.http_context(Scope::Background, policy()).unwrap());
                }
            }
            return;
        }
        self.inner.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.inner.books()
    }
}
fn seeded(keyed: bool) -> Engine<Probe> {
    let (inner, provider, ids) = support::Options {
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        ..support::Options::default()
    }
    .build();
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        held: vec![],
    });
    support::install(&mut e, provider, ids);
    e.set_public_feed_policy(PublicFeedPolicy::Direct).unwrap();
    e.start(0.);
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            break;
        }
        for action in actions {
            if let Action::Http(request) = action {
                let body = if request.url.ends_with("/trading_pairs") {
                    support::PAIRS
                } else if request.url.ends_with("/tickers") {
                    support::TICKERS
                } else if request.url.contains("get_pending_positions") {
                    support::POSITIONS
                } else if request.url.contains("/tpsl/") {
                    support::TPSL
                } else if request.url.contains("get_pending_orders") {
                    support::ORDERS
                } else {
                    support::ACCOUNT
                };
                e.handle(
                    0.,
                    Event::HttpResponse {
                        request: request.id,
                        response: Response {
                            status: 200,
                            headers: "",
                            body,
                        },
                    },
                );
            }
        }
    }
    assert!(e.store().market("BTCUSDT").is_some());
    e
}
fn orders(from_ms: Option<f64>) -> Query {
    Query::OrderHistory {
        symbol: None,
        from_ms,
        to_ms: None,
        limit: None,
    }
}
fn fills(symbol: Option<&str>, from_ms: Option<f64>) -> Query {
    Query::Fills {
        symbol: symbol.map(Into::into),
        order_id: None,
        from_ms,
        to_ms: None,
        limit: None,
    }
}
fn income(to_ms: Option<f64>) -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: None,
        to_ms,
        limit: None,
    }
}
fn client() -> Query {
    Query::OrderByClientId {
        symbol: "BTCUSDT".into(),
        client_order_id: "original".into(),
    }
}
fn public() -> Query {
    Query::PublicTrades {
        symbol: "BTCUSDT".into(),
        limit: None,
    }
}
fn query(e: &mut Engine<Probe>, now: f64, query: Query) -> Vec<Action> {
    e.handle(now, Event::Command(Command::Query { command: 71, query }));
    e.drain_actions().collect()
}
fn failure(actions: &[Action]) -> &VenueError {
    assert_eq!(actions.len(), 1, "one terminal, no companion or opaque I/O");
    match &actions[0] {
        Action::Notice(Notice::CommandFailed { command: 71, error }) => error,
        _ => panic!("expected one correlated failure"),
    }
}
fn empty(actions: &[Action]) {
    assert_eq!(actions.len(), 1, "one terminal, no companion or opaque I/O");
    assert!(
        matches!(&actions[0], Action::Notice(Notice::CommandDone { command: 71, result }) if result == "[]")
    );
}
#[test]
fn unsupported_queries_preserve_text_and_gain_typed_preflight() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for (q, text) in [
            (
                client(),
                "Bitunix client order lookup is not supported by the Rust engine",
            ),
            (public(), "Not implemented"),
        ] {
            let a = query(&mut e, 100., q);
            let error = failure(&a);
            assert_eq!(error.message, text);
            assert_eq!(error.kind, Some(ErrorKind::Unsupported));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn keyless_private_queries_still_complete_empty_before_time_validation() {
    let mut e = seeded(false);
    for q in [
        orders(Some(f64::NAN)),
        income(Some(f64::INFINITY)),
        fills(Some("BTCUSDT"), Some(f64::NEG_INFINITY)),
    ] {
        empty(&query(&mut e, 100., q));
    }
}
#[test]
fn missing_fill_market_still_completes_empty_before_time_validation() {
    let mut e = seeded(true);
    for symbol in [None, Some("UNKNOWN")] {
        empty(&query(&mut e, 100., fills(symbol, Some(f64::NAN))));
    }
}
#[test]
fn nonfinite_time_is_a_typed_local_refusal() {
    let mut e = seeded(true);
    for q in [
        orders(Some(f64::NAN)),
        income(Some(f64::INFINITY)),
        fills(Some("BTCUSDT"), Some(f64::NEG_INFINITY)),
    ] {
        let a = query(&mut e, 100., q);
        let error = failure(&a);
        assert_eq!(error.message, "Bitunix history time must be finite");
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
}
fn local_branches(keyed: bool) -> Vec<Query> {
    let mut q = vec![client(), public()];
    if keyed {
        q.extend([
            fills(None, Some(f64::NAN)),
            fills(Some("UNKNOWN"), Some(f64::NAN)),
            orders(Some(f64::NAN)),
        ]);
    } else {
        q.extend([orders(None), income(None), fills(Some("BTCUSDT"), None)]);
    }
    q
}
#[test]
fn initial_invalid_clock_refusal_covers_every_local_terminal() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for q in local_branches(keyed) {
            let a = query(&mut e, f64::NAN, q);
            let error = failure(&a);
            assert_eq!(error.message, Error::Invalid.to_string());
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
            assert_eq!(error.outcome, None);
        }
        empty(&query(&mut e, 100., fills(None, None)));
    }
}
#[test]
fn initial_capacity_refusal_does_not_consume_other_owners_and_recovers_at_same_clock() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        e.handle(
            100.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["roots".into()],
            }),
        );
        assert!(e.drain_actions().next().is_none());
        assert_eq!(
            e.venue().held.len(),
            256,
            "all bounded owners must really be allocated"
        );
        for _ in 0..270 {
            for q in local_branches(keyed) {
                let a = query(&mut e, 100., q);
                let error = failure(&a);
                assert_eq!(error.message, Error::Capacity.to_string());
                assert_eq!(error.kind, Some(ErrorKind::Internal));
                assert_eq!(error.outcome, None);
                assert_eq!(e.venue().held.len(), 256);
            }
        }
        e.handle(
            100.,
            Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
        );
        assert!(e.drain_actions().next().is_none());
        assert!(e.venue().held.is_empty());
        empty(&query(&mut e, 100., fills(None, None)));
    }
}
#[test]
fn fixed_clock_local_terminals_retire_their_owner_before_reuse() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for _ in 0..270 {
            for q in local_branches(keyed) {
                let unsupported = matches!(
                    q,
                    Query::OrderByClientId { .. } | Query::PublicTrades { .. }
                );
                let invalid = keyed && matches!(q, Query::OrderHistory { .. });
                let a = query(&mut e, 100., q);
                if unsupported || invalid {
                    let error = failure(&a);
                    assert_eq!(
                        error.kind,
                        Some(if unsupported {
                            ErrorKind::Unsupported
                        } else {
                            ErrorKind::InvalidInput
                        })
                    );
                    assert_eq!(error.outcome, Some(Outcome::NotSent));
                } else {
                    empty(&a);
                }
            }
        }
    }
}
#[test]
fn valid_private_history_still_uses_its_unchanged_legacy_dispatch() {
    let mut e = seeded(true);
    let a = query(&mut e, 100., orders(None));
    assert_eq!(a.iter().filter(|a| matches!(a, Action::Http(_))).count(), 1);
    assert!(!a.iter().any(|a| matches!(a, Action::ManagedHttp(_))));
    let r = a
        .iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    assert!(r.url.contains("/get_history_orders?"));
    assert!(r.url.contains("limit=100"));
    assert!(
        r.headers
            .iter()
            .any(|(name, value)| *name == "api-key" && value == "fixture-key")
    );
    e.handle(
        100.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":{"orderList":[]}}"#,
            },
        },
    );
    empty(&e.drain_actions().collect::<Vec<_>>());
}
