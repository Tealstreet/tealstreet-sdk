//! Genuine initial callbacks for local Bitrue history refusals; private fills stay separate.
mod support;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Query,
    Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bitrue::{Bitrue, Pending};
use support::Options;

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
    inner: Bitrue,
    held: Vec<Context>,
}
impl Venue for Probe {
    type Pending = Pending;
    fn guard_pending_placements(&self) -> bool {
        self.inner.guard_pending_placements()
    }
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
    let (venue, provider, identities) = (Options {
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        ..Options::default()
    })
    .build();
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: venue,
        held: vec![],
    });
    e.set_signer(Some(provider)).unwrap();
    e.set_identities(identities).unwrap();
    // Exercise the real query callback without starting unrelated feeds.
    e.store_mut().set_markets(vec![Market {
        id: "E-BTC-USDT".into(),
        symbol: "E-BTC-USDT".into(),
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
fn orders() -> Query {
    Query::OrderHistory {
        symbol: Some("UNKNOWN".into()),
        from_ms: Some(f64::NAN),
        to_ms: None,
        limit: None,
    }
}
fn income() -> Query {
    Query::Income {
        symbol: None,
        kind: None,
        from_ms: Some(f64::NAN),
        to_ms: None,
        limit: None,
    }
}
fn fills(symbol: Option<&str>, from: Option<f64>, to: Option<f64>, limit: Option<u32>) -> Query {
    Query::Fills {
        symbol: symbol.map(Into::into),
        order_id: None,
        from_ms: from,
        to_ms: to,
        limit,
    }
}
fn local_queries(keyed: bool) -> Vec<Query> {
    let mut q = vec![orders(), income()];
    if keyed {
        q.extend([
            fills(Some("UNKNOWN"), Some(f64::NAN), None, None),
            fills(Some("UNKNOWN"), None, None, None),
            fills(None, None, None, None),
        ]);
    } else {
        q.push(fills(Some("UNKNOWN"), Some(f64::NAN), None, None));
    }
    q
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
        "exactly one terminal, no opaque work/companion: {a:?}"
    );
    match &a[0] {
        Action::Notice(Notice::CommandFailed { command: 71, error }) => error,
        _ => panic!("expected correlated failure: {a:?}"),
    }
}
#[test]
fn unsupported_complete_history_kinds_gain_typed_local_authority() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for (q, text) in [
            (
                orders(),
                "Bitrue v2 has only single-order lookup, not complete order history",
            ),
            (
                income(),
                "Bitrue v2 transfer and force-order histories are not a complete PnL/funding ledger",
            ),
        ] {
            let a = query(&mut e, 100., q);
            let er = failure(&a);
            assert_eq!(er.message, text);
            assert_eq!(er.kind, Some(ErrorKind::Unsupported));
            assert_eq!(er.outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn fill_preflights_gain_typed_not_sent_metadata() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for q in local_queries(keyed).into_iter().skip(2) {
            let a = query(&mut e, 100., q);
            let er = failure(&a);
            assert_eq!(er.kind, Some(ErrorKind::InvalidInput));
            assert_eq!(er.outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn auth_then_range_then_symbol_messages_keep_native_precedence() {
    for (keyed, q, text) in [
        (
            false,
            fills(Some("UNKNOWN"), Some(f64::NAN), None, None),
            "Bitrue fills require authenticated futures credentials",
        ),
        (
            true,
            fills(Some("UNKNOWN"), Some(f64::NAN), None, None),
            "Invalid Bitrue fill time window",
        ),
        (
            true,
            fills(Some("UNKNOWN"), None, None, None),
            "Bitrue fills require a known symbol",
        ),
        (
            true,
            fills(None, None, None, None),
            "Bitrue fills require a known symbol",
        ),
    ] {
        assert_eq!(failure(&query(&mut seeded(keyed), 100., q)).message, text);
    }
    for (from, to) in [
        (f64::INFINITY, 2.),
        (f64::NEG_INFINITY, 2.),
        (-1., 2.),
        (3., 2.),
        (1., f64::NAN),
    ] {
        assert_eq!(
            failure(&query(
                &mut seeded(true),
                100.,
                fills(Some("UNKNOWN"), Some(from), Some(to), None)
            ))
            .message,
            "Invalid Bitrue fill time window"
        );
    }
}
#[test]
fn invalid_initial_clock_is_a_shared_refusal_on_each_local_path() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for q in local_queries(keyed) {
                let a = query(&mut e, now, q);
                let er = failure(&a);
                assert_eq!(er.message, Error::Invalid.to_string());
                assert_eq!(er.kind, Some(ErrorKind::InvalidInput));
                assert_eq!(er.outcome, None);
            }
        }
        assert_eq!(
            failure(&query(&mut e, 100., orders())).message,
            "Bitrue v2 has only single-order lookup, not complete order history"
        );
    }
}
#[test]
fn saturated_genuine_roots_refuse_local_work_without_numeric_fallback_and_recover() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        e.handle(
            100.,
            Event::Command(Command::SetTickerSymbols {
                symbols: vec!["saturate".into()],
            }),
        );
        assert!(e.drain_actions().next().is_none());
        assert_eq!(e.venue().held.len(), 256);
        for _ in 0..270 {
            for q in local_queries(keyed) {
                let a = query(&mut e, 100., q);
                let er = failure(&a);
                assert_eq!(er.message, Error::Capacity.to_string());
                assert_eq!(er.kind, Some(ErrorKind::Internal));
                assert_eq!(er.outcome, None);
                assert_eq!(e.venue().held.len(), 256);
            }
        }
        e.handle(
            100.,
            Event::Command(Command::SetTickerSymbols { symbols: vec![] }),
        );
        assert!(e.drain_actions().next().is_none());
        assert!(e.venue().held.is_empty());
        let a = query(&mut e, 100., orders());
        assert_eq!(failure(&a).kind, Some(ErrorKind::Unsupported));
    }
}
#[test]
fn fixed_clock_local_terminal_owners_are_retired_before_reuse() {
    for keyed in [false, true] {
        let mut e = seeded(keyed);
        for _ in 0..270 {
            for q in local_queries(keyed) {
                let a = query(&mut e, 100., q);
                assert_eq!(failure(&a).outcome, Some(Outcome::NotSent));
            }
        }
    }
}
#[test]
fn local_refusals_leave_store_unchanged() {
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
fn valid_signed_fills_preserve_aliases_flooring_and_clamped_native_limit() {
    for (limit, n) in [(None, 200), (Some(0), 1), (Some(9999), 1000)] {
        let mut e = seeded(true);
        let a = query(
            &mut e,
            100.,
            fills(Some("btc/usdt"), Some(1.5), Some(2.9), limit),
        );
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
        assert!(r.url.ends_with(&format!(
            "/fapi/v2/myTrades?contractName=E-BTC-USDT&limit={n}&startTime=1&endTime=2"
        )));
        assert!(
            r.headers
                .iter()
                .any(|(name, value)| *name == "X-CH-APIKEY" && value == "fixture-key")
        );
        assert!(r.headers.iter().any(|(name, _)| *name == "X-CH-SIGN"));
        e.handle(
            100.,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"code":"0","data":[]}"#,
                },
            },
        );
        let a = e.drain_actions().collect::<Vec<_>>();
        assert_eq!(a.len(), 1);
        assert!(
            matches!(&a[0],Action::Notice(Notice::CommandDone{command:71,result})if result=="[]")
        );
    }
}
