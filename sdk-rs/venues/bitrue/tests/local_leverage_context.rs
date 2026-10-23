//! Genuine initial callbacks for local Bitrue leverage refusals; valid account reads stay separate.
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bitrue::{Bitrue, Options, Pending};

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
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Bitrue::new(Options {
            key: keyed.then(|| "fixture-key".into()),
            secret: keyed.then(|| "fixture-secret".into()),
            ..Options::default()
        }),
        held: vec![],
    });
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
fn command(max: bool, symbol: &str) -> Command {
    if max {
        Command::FetchMaxLeverage {
            command: 71,
            symbol: symbol.into(),
        }
    } else {
        Command::FetchLeverage {
            command: 71,
            symbol: symbol.into(),
        }
    }
}
fn run(e: &mut Engine<Probe>, now: f64, max: bool, symbol: &str) -> Vec<Action> {
    e.handle(now, Event::Command(command(max, symbol)));
    e.drain_actions().collect()
}
fn failure(a: &[Action]) -> &VenueError {
    assert_eq!(a.len(), 1, "one terminal without companion/I/O: {a:?}");
    match &a[0] {
        Action::Notice(Notice::CommandFailed { command: 71, error }) => error,
        _ => panic!("expected correlated local failure: {a:?}"),
    }
}
fn symbols(keyed: bool) -> Vec<&'static str> {
    if keyed {
        vec!["UNKNOWN"]
    } else {
        vec!["UNKNOWN", "btc/usdt"]
    }
}
#[test]
fn both_local_refusals_gain_typed_pre_io_metadata_for_both_commands() {
    for max in [false, true] {
        for keyed in [false, true] {
            let mut e = seeded(keyed);
            for symbol in symbols(keyed) {
                let a = run(&mut e, 100., max, symbol);
                let er = failure(&a);
                assert_eq!(er.kind, Some(ErrorKind::InvalidInput));
                assert_eq!(er.outcome, Some(Outcome::NotSent));
            }
        }
    }
}
#[test]
fn unknown_symbol_precedes_authentication_and_preserves_both_native_messages() {
    for max in [false, true] {
        for keyed in [false, true] {
            assert_eq!(
                failure(&run(&mut seeded(keyed), 100., max, "UNKNOWN")).message,
                "Unknown Bitrue market"
            );
        }
        assert_eq!(
            failure(&run(&mut seeded(false), 100., max, "btc/usdt")).message,
            "Bitrue private credentials required"
        );
    }
}
#[test]
fn invalid_initial_clock_refuses_each_local_path_through_shared_entry() {
    for max in [false, true] {
        for keyed in [false, true] {
            let mut e = seeded(keyed);
            for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                for symbol in symbols(keyed) {
                    let a = run(&mut e, now, max, symbol);
                    let er = failure(&a);
                    assert_eq!(er.message, Error::Invalid.to_string());
                    assert_eq!(er.kind, Some(ErrorKind::InvalidInput));
                    assert_eq!(er.outcome, None);
                }
            }
            assert_eq!(
                failure(&run(&mut e, 100., max, "UNKNOWN")).message,
                "Unknown Bitrue market"
            );
        }
    }
}
#[test]
fn saturated_roots_deny_local_refusals_without_numeric_fallback_and_recover() {
    for max in [false, true] {
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
                for symbol in symbols(keyed) {
                    let a = run(&mut e, 100., max, symbol);
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
            let a = run(&mut e, 100., max, "UNKNOWN");
            assert_eq!(failure(&a).outcome, Some(Outcome::NotSent));
        }
    }
}
#[test]
fn fixed_clock_local_terminal_owners_retire_before_reuse() {
    for max in [false, true] {
        for keyed in [false, true] {
            let mut e = seeded(keyed);
            for _ in 0..270 {
                for symbol in symbols(keyed) {
                    let a = run(&mut e, 100., max, symbol);
                    assert_eq!(failure(&a).outcome, Some(Outcome::NotSent));
                }
            }
        }
    }
}
#[test]
fn local_refusals_do_not_mutate_store() {
    let mut e = seeded(false);
    let before = serde_json::to_string(&(
        e.store().markets(),
        e.store().tickers(),
        e.store().orders(),
        e.store().positions(),
        e.store().balance(),
    ))
    .unwrap();
    for max in [false, true] {
        for symbol in symbols(false) {
            run(&mut e, 100., max, symbol);
        }
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
fn valid_leverage_requests_keep_native_legacy_dispatch() {
    for (max, path) in [
        (true, "/fapi/v2/leverageBracket?contractName=E-BTC-USDT"),
        (false, "/fapi/v2/account"),
    ] {
        let mut e = seeded(true);
        let a = run(&mut e, 100., max, "btc/usdt");
        assert!(!a.iter().any(|a| matches!(
            a,
            Action::ManagedHttp(_)
                | Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
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
        assert!(r.url.ends_with(path));
        assert!(
            r.headers
                .iter()
                .any(|(name, value)| *name == "X-CH-APIKEY" && value == "fixture-key")
        );
        assert!(r.headers.iter().any(|(name, _)| *name == "X-CH-SIGN"));
    }
}
