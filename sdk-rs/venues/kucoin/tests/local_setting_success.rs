#![cfg(feature = "spot")]
//! Native fixed settings through real command receipts and shared LocalSetting admission.
mod support;
use safe_cex_engine::model::MarginType;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Notice, Outcome, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_kucoin::spot::KucoinSpot;
use support::credentials::Options;
const SYMBOL: &str = "BTC-USDT";
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
    delay: f64,
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
                Some("backgrounds") => {
                    for _ in 0..256 {
                        self.backgrounds
                            .push(cx.http_context(Scope::Background, policy()).unwrap());
                    }
                }
                Some("sub-ms") => self.delay = 119_999.5,
                Some("expired") => self.delay = 120_000.,
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
        // Advance only the public callback clock after genuine receipt ingress.
        cx.now += std::mem::take(&mut self.delay);
        self.inner.on_command(cx, command);
    }
    fn book(&self, symbol: &str) -> Option<&safe_cex_engine::book::Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        self.inner.books()
    }
}

fn seeded(keyed: bool, public_only: bool) -> Engine<Probe<KucoinSpot>> {
    let (options, provider, identities) = Options {
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        passphrase: keyed.then(|| "fixture-passphrase".into()),
        public_data_only: public_only,
        ..Options::default()
    }
    .prepare();
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: KucoinSpot::new(options),
        owners: vec![],
        backgrounds: vec![],
        hold_next: false,
        delay: 0.,
    });
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
    e.store_mut().update_setting(SYMBOL, |s| {
        s.leverage = 9.;
        s.buy_leverage = Some(8.);
        s.sell_leverage = Some(7.);
        s.margin_type = Some(MarginType::Cross);
    });
    e.store_mut().set_hedged(true);
    e
}
fn setting(kind: u8, symbol: &str, flag: bool) -> Command {
    match kind {
        0 => Command::SetLeverage {
            command: COMMAND,
            symbol: symbol.into(),
            leverage: f64::NAN,
            side: None,
        },
        1 => Command::SetMarginType {
            command: COMMAND,
            symbol: symbol.into(),
            isolated: flag,
        },
        2 => Command::SetPositionMode {
            command: COMMAND,
            symbol: Some(symbol.into()),
            hedged: flag,
        },
        _ => unreachable!(),
    }
}
fn run(e: &mut Engine<Probe<KucoinSpot>>, command: Command) -> Vec<Action> {
    e.handle_command(100., command, Some(120_000)).unwrap();
    e.drain_actions().collect()
}
fn control(e: &mut Engine<Probe<KucoinSpot>>, tag: &str) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: vec![tag.into()],
        },
    )
}
fn hold(e: &mut Engine<Probe<KucoinSpot>>, id: u32) {
    assert!(control(e, "hold-next").is_empty());
    assert!(
        run(
            e,
            Command::FetchLeverage {
                command: id,
                symbol: SYMBOL.into()
            }
        )
        .is_empty()
    );
}
fn success(a: &[Action]) {
    assert!(
        matches!(a, [Action::Notice(Notice::CommandDone { command: COMMAND, result })]
        if result == "[]"),
        "exact local success without opaque work: {a:?}"
    );
}
fn refusal(a: &[Action], cause: Error) {
    let mut expected = cause.venue_error();
    expected.outcome = Some(Outcome::NotSent);
    assert!(
        matches!(a, [Action::Notice(Notice::CommandFailed { command: COMMAND, error })]
        if error == &expected),
        "one typed initial refusal without I/O: {a:?}"
    );
}
#[test]
fn native_results_and_projections_precede_credentials_in_every_configuration() {
    for keyed in [false, true] {
        for public_only in [false, true] {
            for kind in 0..3 {
                for flag in [false, true] {
                    let mut e = seeded(keyed, public_only);
                    let before = e.store().setting(SYMBOL).unwrap();
                    success(&run(&mut e, setting(kind, SYMBOL, flag)));
                    let after = e.store().setting(SYMBOL).unwrap();
                    if kind == 0 {
                        assert_eq!(after.leverage, 1.);
                        assert_eq!(after.buy_leverage, Some(1.));
                        assert_eq!(after.sell_leverage, Some(1.));
                        assert_eq!(after.margin_type, before.margin_type);
                    } else {
                        assert_eq!(after, before);
                    }
                    assert!(e.store().hedged());
                }
            }
        }
    }
}
#[test]
fn unknown_symbols_keep_native_default_insertion_only_for_leverage() {
    for kind in 0..3 {
        let mut e = seeded(false, true);
        assert!(e.store().setting("UNKNOWN").is_none());
        success(&run(&mut e, setting(kind, "UNKNOWN", true)));
        if kind == 0 {
            let s = e.store().setting("UNKNOWN").unwrap();
            assert_eq!(s.leverage, 1.);
            assert_eq!(s.buy_leverage, Some(1.));
            assert_eq!(s.sell_leverage, Some(1.));
            assert_eq!(s.margin_type, None);
        } else {
            assert!(e.store().setting("UNKNOWN").is_none());
        }
    }
}
fn capacity_refuses(kind: u8) {
    {
        let mut e = seeded(false, true);
        for id in 1000..1256 {
            hold(&mut e, id);
        }
        let before = e.store().setting(SYMBOL);
        refusal(&run(&mut e, setting(kind, SYMBOL, false)), Error::Capacity);
        assert_eq!(e.store().setting(SYMBOL), before);
        assert!(e.store().hedged());
        let released = control(&mut e, "release");
        assert_eq!(released.len(), 256);
        assert!(released.iter().all(|a| matches!(a,
            Action::Notice(Notice::CommandDone { result, .. }) if result == "\"original\"")));
        success(&run(&mut e, setting(kind, SYMBOL, false)));
    }
}
#[test]
fn background_context_capacity_does_not_block_context_free_setting_success() {
    for kind in 0..3 {
        let mut e = seeded(false, true);
        assert!(control(&mut e, "backgrounds").is_empty());
        success(&run(&mut e, setting(kind, SYMBOL, false)));
    }
}
#[test]
fn duplicate_id_keeps_store_and_original_read_owner_unchanged() {
    for kind in 0..3 {
        let mut e = seeded(false, true);
        hold(&mut e, COMMAND);
        let before = e.store().setting(SYMBOL);
        let a = run(&mut e, setting(kind, SYMBOL, false));
        assert!(
            matches!(a.as_slice(), [Action::Notice(Notice::Event(event))]
            if event.code == "command.duplicate"
            && event.message == "Command ID is already in flight"
            && event.level == safe_cex_engine::Level::Error),
            "duplicate keeps the native diagnostic without second terminal: {a:?}"
        );
        assert_eq!(e.store().setting(SYMBOL), before);
        assert!(e.store().hedged());
        assert!(matches!(control(&mut e, "release").as_slice(),
            [Action::Notice(Notice::CommandDone { command: COMMAND, result })]
            if result == "\"original\""));
    }
}
fn expired_refuses(kind: u8) {
    {
        for tag in ["sub-ms", "expired"] {
            let mut e = seeded(false, true);
            assert!(control(&mut e, tag).is_empty());
            let before = e.store().setting(SYMBOL);
            refusal(&run(&mut e, setting(kind, SYMBOL, false)), Error::TimedOut);
            assert_eq!(e.store().setting(SYMBOL), before);
            assert!(e.store().hedged());
        }
    }
}

#[test]
fn leverage_capacity_refuses_before_projection() {
    capacity_refuses(0);
}
#[test]
fn margin_capacity_refuses_before_projection() {
    capacity_refuses(1);
}
#[test]
fn mode_capacity_refuses_before_projection() {
    capacity_refuses(2);
}
#[test]
fn leverage_expired_receipt_refuses_before_projection() {
    expired_refuses(0);
}
#[test]
fn margin_expired_receipt_refuses_before_projection() {
    expired_refuses(1);
}
#[test]
fn mode_expired_receipt_refuses_before_projection() {
    expired_refuses(2);
}
