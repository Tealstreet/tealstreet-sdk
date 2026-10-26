//! Local native leverage refusals acquire only genuine Read completion authority.
use safe_cex_engine::model::{Limits, MarginType, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Notice, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_blofin::{Blofin, Options};
const SYMBOL: &str = "BTCUSDT";
const COMMAND: u32 = 71;
fn policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}
struct Probe<V> {
    inner: V,
    owners: Vec<Context>,
    backgrounds: Vec<Context>,
    hold_next: bool,
}
impl<V: Venue> Venue for Probe<V> {
    type Pending = V::Pending;
    fn guard_pending_placements(&self) -> bool {
        self.inner.guard_pending_placements()
    }
    fn guard_order_mutations(&self) -> bool {
        self.inner.guard_order_mutations()
    }
    fn cancel_needs_stored_order(&self) -> bool {
        self.inner.cancel_needs_stored_order()
    }
    fn fence_uncertain_cancel_all(&self) -> bool {
        self.inner.fence_uncertain_cancel_all()
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    fn start(&mut self, cx: &mut Cx<'_, Self::Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Self::Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Self::Pending>, timer: TimerId) {
        self.inner.on_timer(cx, timer);
    }
    fn on_item(&mut self, cx: &mut Cx<'_, Self::Pending>, pending: &mut Self::Pending, item: &str) {
        self.inner.on_item(cx, pending, item);
    }
    fn on_response(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        r: Response<'_>,
    ) {
        self.inner.on_response(cx, pending, r);
    }
    fn on_request_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: &str,
    ) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_http_failed(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.inner.on_http_failed(cx, pending, error);
    }
    fn on_sign_error(
        &mut self,
        cx: &mut Cx<'_, Self::Pending>,
        pending: Self::Pending,
        error: VenueError,
    ) {
        self.inner.on_sign_error(cx, pending, error);
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Self::Pending>, command: Command) {
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("hold-next") => self.hold_next = true,
                Some("release") => {
                    for root in self.owners.drain(..) {
                        cx.done_context(&root, "\"original\"".into()).unwrap();
                    }
                }
                Some(tag @ ("backgrounds" | "near-capacity")) => {
                    let count = if tag == "backgrounds" { 256 } else { 255 };
                    for _ in 0..count {
                        self.backgrounds
                            .push(cx.http_context(Scope::Background, policy()).unwrap());
                    }
                }
                Some("release-backgrounds") => {
                    for context in self.backgrounds.drain(..) {
                        cx.finish_http_context(&context).unwrap();
                    }
                }
                _ => panic!("unknown fixture control"),
            }
            return;
        }
        if self.hold_next {
            assert!(matches!(command, Command::FetchLeverage { .. }));
            self.hold_next = false;
            self.owners.push(cx.admit_http_command(policy()).unwrap());
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

fn seeded(keyed: bool, public_only: bool) -> Engine<Probe<Blofin>> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Blofin::new(Options {
            // Keyed means trusted setup produced refs; no provider is installed.
            identities: if keyed {
                safe_cex_crypto::signer::LocalSigner::from_json(
                    r#"{"primary":{"key":"fixture-key","secret":"fixture-secret","passphrase":"fixture-passphrase"}}"#
                        .into(),
                )
                .unwrap()
                .1
            } else {
                Default::default()
            },
            public_data_only: public_only,
            ..Options::default()
        }),
        owners: vec![],
        backgrounds: vec![],
        hold_next: false,
    });
    e.store_mut().set_markets(vec![Market {
        id: "BTC-USDT".into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.1,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.1,
                max: 1000.,
            },
            leverage: MinMax { min: 1., max: 150. },
            price: None,
        },
        contract_size: Some(0.001),
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    e.store_mut().update_setting(SYMBOL, |s| {
        s.leverage = 19.;
        s.buy_leverage = Some(18.);
        s.sell_leverage = Some(17.);
        s.margin_type = Some(MarginType::Cross);
    });
    e.store_mut().set_hedged(true);
    e
}
fn command(symbol: &str) -> Command {
    Command::FetchLeverage {
        command: COMMAND,
        symbol: symbol.into(),
    }
}
fn run(e: &mut Engine<Probe<Blofin>>, command: Command) -> Vec<Action> {
    e.handle_command(100., command, Some(120_000)).unwrap();
    e.drain_actions().collect()
}
fn control(e: &mut Engine<Probe<Blofin>>, tag: &str) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: vec![tag.into()],
        },
    )
}
fn failure(a: &[Action]) -> &VenueError {
    assert!(
        matches!(
            a,
            [Action::Notice(Notice::CommandFailed {
                command: COMMAND,
                ..
            })]
        ),
        "exactly one local correlated failure without I/O: {a:?}"
    );
    match &a[0] {
        Action::Notice(Notice::CommandFailed { error, .. }) => error,
        _ => unreachable!(),
    }
}
fn native(a: &[Action], symbol: &str) {
    let message = if symbol == SYMBOL {
        "BloFin margin mode is not loaded".to_owned()
    } else {
        format!("Unknown market {symbol}")
    };
    assert_eq!(failure(a), &VenueError::new(message));
}
fn capacity(symbol: &str) {
    let mut e = seeded(false, true);
    assert!(control(&mut e, "backgrounds").is_empty());
    let before = e.store().setting(SYMBOL);
    let a = run(&mut e, command(symbol));
    let mut expected = Error::Capacity.venue_error();
    expected.outcome = None;
    assert_eq!(
        failure(&a),
        &expected,
        "shared Read admission must supersede the native refusal at capacity"
    );
    assert_eq!(failure(&a).outcome, None);
    assert_eq!(e.store().setting(SYMBOL), before);
    assert!(e.store().hedged());
    assert!(control(&mut e, "release-backgrounds").is_empty());
    native(&run(&mut e, command(symbol)), symbol);
}
#[test]
fn unknown_market_capacity_uses_shared_read_refusal() {
    capacity("UNKNOWN");
}
#[test]
fn unloaded_mode_capacity_uses_shared_read_refusal() {
    capacity(SYMBOL);
}
#[test]
fn native_messages_and_validation_precedence_stay_before_credentials() {
    for keyed in [false, true] {
        for public_only in [false, true] {
            for symbol in ["UNKNOWN", SYMBOL] {
                let mut e = seeded(keyed, public_only);
                let before = e.store().setting(SYMBOL);
                native(&run(&mut e, command(symbol)), symbol);
                assert_eq!(e.store().setting(SYMBOL), before);
                assert!(e.store().setting("UNKNOWN").is_none());
                assert!(e.store().hedged());
                assert!(e.store().orders().is_empty());
                assert!(e.store().positions().is_empty());
            }
        }
    }
}
#[test]
fn duplicate_id_preserves_native_diagnostic_and_original_owner() {
    for symbol in ["UNKNOWN", SYMBOL] {
        let mut e = seeded(false, true);
        assert!(control(&mut e, "hold-next").is_empty());
        assert!(run(&mut e, command(SYMBOL)).is_empty());
        let a = run(&mut e, command(symbol));
        assert!(
            matches!(a.as_slice(), [Action::Notice(Notice::Event(event))]
            if event.code == "command.duplicate"
            && event.message == "Command ID is already in flight"
            && event.level == safe_cex_engine::Level::Error),
            "native duplicate: {a:?}"
        );
        assert!(matches!(control(&mut e, "release").as_slice(),
            [Action::Notice(Notice::CommandDone { command: COMMAND, result })]
            if result == "\"original\""));
        native(&run(&mut e, command(symbol)), symbol);
    }
}
#[test]
fn sequential_refusals_release_the_only_free_context_slot_without_outcomes() {
    for symbol in ["UNKNOWN", SYMBOL] {
        let mut e = seeded(false, true);
        assert!(control(&mut e, "near-capacity").is_empty());
        for _ in 0..300 {
            native(&run(&mut e, command(symbol)), symbol);
        }
        assert!(control(&mut e, "release-backgrounds").is_empty());
        native(&run(&mut e, command(symbol)), symbol);
    }
}
