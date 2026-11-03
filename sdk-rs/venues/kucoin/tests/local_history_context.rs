//! Genuine initial callbacks for local futures history terminals; private paging stays separate.
mod support;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Query,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_kucoin::{Kucoin, Pending};
use support::credentials::Options;

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
    inner: Kucoin,
    held: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
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
        // This no-ID command is a separate genuine callback, never a forged command entry.
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
    let (options, provider, identities) = Options {
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        passphrase: keyed.then(|| "fixture-passphrase".into()),
        ..Options::default()
    }
    .prepare();
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Kucoin::new(options),
        held: vec![],
    });
    // Like owned_history, exercise the real query callback without starting unrelated feeds.
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.store_mut().set_markets(vec![Market {
        id: "XBTUSDTM".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 1.,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax { min: 1., max: 1e6 },
            leverage: MinMax { min: 1., max: 100. },
            price: None,
        },
        contract_size: Some(0.001),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e
}
fn orders(symbol: Option<&str>, from: f64, to: f64, limit: u32) -> Query {
    Query::OrderHistory {
        symbol: symbol.map(Into::into),
        from_ms: Some(from),
        to_ms: Some(to),
        limit: Some(limit),
    }
}
fn funding(symbol: Option<&str>, from: f64, to: f64, limit: u32) -> Query {
    Query::Income {
        symbol: symbol.map(Into::into),
        kind: Some("FUNDING_FEE".into()),
        from_ms: Some(from),
        to_ms: Some(to),
        limit: Some(limit),
    }
}
fn fills(order_id: &str) -> Query {
    Query::Fills {
        symbol: Some("BTCUSDT".into()),
        order_id: Some(order_id.into()),
        from_ms: Some(1.),
        to_ms: Some(2.),
        limit: Some(10),
    }
}
fn client() -> Query {
    Query::OrderByClientId {
        symbol: "UNKNOWN".into(),
        client_order_id: "native".into(),
    }
}
fn query(e: &mut Engine<Probe>, now: f64, q: Query) -> Vec<Action> {
    e.handle(
        now,
        Event::Command(Command::Query {
            command: 71,
            query: q,
        }),
    );
    e.drain_actions().collect()
}
fn failure(a: &[Action]) -> &VenueError {
    assert_eq!(
        a.len(),
        1,
        "exactly one terminal, no I/O or companion: {a:?}"
    );
    match &a[0] {
        Action::Notice(Notice::CommandFailed { command: 71, error }) => error,
        _ => panic!("expected one correlated failure: {a:?}"),
    }
}
fn empty(a: &[Action]) {
    assert_eq!(a.len(), 1, "empty local result has no opaque work: {a:?}");
    assert!(
        matches!(&a[0], Action::Notice(Notice::CommandDone { command: 71, result }) if result == "[]")
    );
}
fn local_queries(keyed: bool) -> Vec<Query> {
    let mut q = vec![client(), fills("invalid/id")];
    if !keyed {
        q.extend([
            orders(Some("UNKNOWN"), f64::NAN, 2., 0),
            funding(None, f64::NAN, 2., 0),
        ]);
        return q;
    }
    q.extend([
        orders(Some("UNKNOWN"), f64::NAN, 2., 0),
        orders(Some("BTCUSDT"), 1.5, 2., 0),
        orders(Some("BTCUSDT"), 3., 2., 0),
        orders(Some("BTCUSDT"), 1., 2., 0),
        funding(None, f64::NAN, 2., 0),
        funding(Some("UNKNOWN"), f64::NAN, 2., 0),
        funding(Some("BTCUSDT"), 1.5, 2., 0),
        funding(Some("BTCUSDT"), 3., 2., 0),
        funding(Some("BTCUSDT"), 1., 2., 0),
    ]);
    q
}
#[test]
fn unsupported_client_lookup_preserves_text_and_gets_typed_local_authority() {
    for keyed in [false, true] {
        let a = query(&mut seeded(keyed), 100., client());
        let error = failure(&a);
        assert_eq!(
            error.message,
            "Client ID query is not supported by this venue"
        );
        assert_eq!(error.kind, Some(ErrorKind::Unsupported));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
}
#[test]
fn local_validation_and_credentials_gain_typed_pre_io_errors() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for q in local_queries(keyed).into_iter().skip(1) {
            let a = query(&mut e, 100., q);
            if matches!(&a[..], [Action::Notice(Notice::CommandDone { .. })]) {
                empty(&a);
                continue;
            }
            let error = failure(&a);
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
            assert_eq!(error.outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn zero_limits_remain_empty_for_history_and_funding_without_io() {
    let mut e = seeded(true);
    for q in [
        orders(Some("BTCUSDT"), 1., 2., 0),
        funding(Some("BTCUSDT"), 1., 2., 0),
    ] {
        empty(&query(&mut e, 100., q));
    }
}
#[test]
fn validation_message_and_precedence_remain_native() {
    for (keyed, q, expected) in [
        (false, fills("invalid/id"), "Invalid KuCoin order ID"),
        (
            false,
            orders(Some("UNKNOWN"), f64::NAN, 2., 0),
            "KuCoin credentials required",
        ),
        (
            false,
            funding(None, f64::NAN, 2., 0),
            "KuCoin credentials required",
        ),
        (
            true,
            orders(Some("UNKNOWN"), f64::NAN, 2., 0),
            "Unknown KuCoin market UNKNOWN",
        ),
        (
            true,
            funding(None, f64::NAN, 2., 0),
            "KuCoin funding history requires a symbol",
        ),
        (
            true,
            funding(Some(""), f64::NAN, 2., 0),
            "KuCoin funding history requires a symbol",
        ),
        (
            true,
            funding(Some("UNKNOWN"), f64::NAN, 2., 0),
            "Unknown KuCoin market UNKNOWN",
        ),
    ] {
        assert_eq!(
            failure(&query(&mut seeded(keyed), 100., q)).message,
            expected
        );
    }
}
#[test]
fn invalid_numeric_bounds_remain_local_before_zero_limit() {
    for (from, to) in [
        (-1., 2.),
        (1.5, 2.),
        (3., 2.),
        (f64::NAN, 2.),
        (1., f64::INFINITY),
        (1., f64::NEG_INFINITY),
        (1., 9_007_199_254_740_992.),
    ] {
        for (q, expected) in [
            (
                orders(Some("BTCUSDT"), from, to, 0),
                "Invalid KuCoin query time range",
            ),
            (
                funding(Some("BTCUSDT"), from, to, 0),
                "Invalid KuCoin funding time range",
            ),
        ] {
            assert_eq!(
                failure(&query(&mut seeded(true), 100., q)).message,
                expected
            );
        }
    }
}
#[test]
fn invalid_initial_clock_is_rejected_by_shared_admission_on_every_reachable_local_path() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for q in local_queries(keyed) {
                let a = query(&mut e, now, q);
                let error = failure(&a);
                assert_eq!(error.message, Error::Invalid.to_string());
                assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
                assert_eq!(error.outcome, None);
            }
        }
        assert_eq!(
            failure(&query(&mut e, 100., client())).message,
            "Client ID query is not supported by this venue"
        );
    }
}
#[test]
fn saturated_genuine_roots_deny_local_results_without_numeric_fallback_and_recover() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        e.handle(
            100.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["saturate".into()],
            }),
        );
        assert!(e.drain_actions().next().is_none());
        assert_eq!(
            e.venue().held.len(),
            256,
            "genuinely allocate the existing root bound"
        );
        for _ in 0..270 {
            for q in local_queries(keyed) {
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
        if keyed {
            empty(&query(&mut e, 100., funding(Some("BTCUSDT"), 1., 2., 0)));
        } else {
            assert_eq!(
                failure(&query(&mut e, 100., client())).message,
                "Client ID query is not supported by this venue"
            );
        }
    }
}
#[test]
fn fixed_clock_reuse_retires_each_local_owner() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for _ in 0..270 {
            for q in local_queries(keyed) {
                let a = query(&mut e, 100., q);
                if matches!(&a[..], [Action::Notice(Notice::CommandDone { .. })]) {
                    empty(&a);
                } else {
                    assert_eq!(failure(&a).outcome, Some(Outcome::NotSent));
                }
            }
        }
    }
}
#[test]
fn local_terminals_leave_store_untouched() {
    let mut e = seeded(true);
    let before = serde_json::to_string(&(
        e.store().markets(),
        e.store().tickers(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance(),
    ))
    .unwrap();
    for q in local_queries(true) {
        query(&mut e, 100., q);
    }
    let after = serde_json::to_string(&(
        e.store().markets(),
        e.store().tickers(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance(),
    ))
    .unwrap();
    assert_eq!(before, after);
}
#[test]
fn valid_signed_history_and_funding_keep_native_requests_and_empty_results() {
    for (q, path, body) in [
        (
            orders(Some("BTCUSDT"), 1., 2., 10),
            "/api/v1/orders?symbol=XBTUSDTM&status=done&startAt=1&endAt=2&currentPage=1&pageSize=50",
            r#"{"code":"200000","data":{"currentPage":1,"totalPage":1,"totalNum":0,"items":[]}}"#,
        ),
        (
            funding(Some("BTCUSDT"), 1., 2., 10),
            "/api/v1/funding-history?symbol=XBTUSDTM&startAt=1&endAt=2&reverse=true&forward=false&maxCount=50",
            r#"{"code":"200000","data":{"dataList":[],"hasMore":false}}"#,
        ),
    ] {
        let mut e = seeded(true);
        let a = query(&mut e, 100., q);
        assert!(!a.iter().any(|a| matches!(
            a,
            Action::ManagedHttp(_) | Action::Notice(Notice::CommandFailed { .. })
        )));
        let requests: Vec<_> = a
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
        assert_eq!(r.method, Method::Get);
        assert_eq!(r.url, format!("https://api-futures.kucoin.com{path}"));
        assert!(
            r.headers
                .iter()
                .any(|(name, value)| *name == "KC-API-KEY" && value == "fixture-key")
        );
        assert!(r.headers.iter().any(|(name, _)| *name == "KC-API-SIGN"));
        e.handle(
            100.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        empty(&e.drain_actions().collect::<Vec<_>>());
    }
}
#[test]
fn valid_long_range_still_dispatches_clipped_history_instead_of_a_local_refusal() {
    let mut e = seeded(true);
    let a = query(
        &mut e,
        100.,
        orders(Some("BTCUSDT"), 0., 9_007_199_254_740_991., 1),
    );
    assert!(a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::ManagedHttp(_) | Action::Notice(Notice::CommandFailed { .. })
    )));
}
