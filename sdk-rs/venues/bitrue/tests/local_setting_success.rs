//! Native fixed settings through real command receipts and shared LocalSetting admission.
use safe_cex_engine::model::MarginType;
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, Notice, Outcome, Response, SocketId, TimerId, Venue,
    VenueError,
};
use safe_cex_venue_bitrue::{Bitrue, Options};
const SYMBOL: &str = "E-BTC-USDT";
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

fn seeded(keyed: bool, public_only: bool) -> Engine<Probe<Bitrue>> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Bitrue::new(Options {
            key: keyed.then(|| "fixture-key".into()),
            secret: keyed.then(|| "fixture-secret".into()),
            public_data_only: public_only,
            ..Options::default()
        }),
        owners: vec![],
        backgrounds: vec![],
        hold_next: false,
        delay: 0.,
    });
    e.store_mut().update_setting(SYMBOL, |s| {
        s.leverage = 9.;
        s.buy_leverage = Some(8.);
        s.sell_leverage = Some(7.);
        s.margin_type = Some(MarginType::Cross);
    });
    e.store_mut().set_hedged(true);
    e
}
fn setting(hedged: bool) -> Command {
    Command::SetPositionMode {
        command: COMMAND,
        symbol: Some("UNKNOWN".into()),
        hedged,
    }
}
fn run(e: &mut Engine<Probe<Bitrue>>, command: Command) -> Vec<Action> {
    e.handle_command(100., command, Some(120_000)).unwrap();
    e.drain_actions().collect()
}
fn control(e: &mut Engine<Probe<Bitrue>>, tag: &str) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: vec![tag.into()],
        },
    )
}
fn hold(e: &mut Engine<Probe<Bitrue>>, id: u32) {
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
fn native_one_way_result_precedes_credentials_in_every_configuration() {
    for keyed in [false, true] {
        for public_only in [false, true] {
            let mut e = seeded(keyed, public_only);
            let before = e.store().setting(SYMBOL);
            success(&run(&mut e, setting(false)));
            assert!(!e.store().hedged());
            assert_eq!(e.store().setting(SYMBOL), before);
            assert!(e.store().setting("UNKNOWN").is_none());
            success(&run(&mut e, setting(false)));
        }
    }
}
#[test]
fn hedged_true_keeps_the_existing_pre_credential_refusal() {
    for keyed in [false, true] {
        for public_only in [false, true] {
            let mut e = seeded(keyed, public_only);
            let a = run(&mut e, setting(true));
            assert!(
                matches!(a.as_slice(), [Action::Notice(Notice::CommandFailed { command: COMMAND, error })]
                if error.message == "Bitrue supports one-way positions only"
                && error.kind == Some(safe_cex_engine::ErrorKind::Unsupported)
                && error.outcome == Some(Outcome::NotSent)),
                "unchanged native refusal: {a:?}"
            );
            assert!(e.store().hedged());
        }
    }
}
#[test]
fn command_capacity_refuses_before_hedged_projection_and_recovers() {
    let mut e = seeded(false, true);
    for id in 1000..1256 {
        hold(&mut e, id);
    }
    refusal(&run(&mut e, setting(false)), Error::Capacity);
    assert!(e.store().hedged());
    let released = control(&mut e, "release");
    assert_eq!(released.len(), 256);
    assert!(released.iter().all(|a| matches!(a,
        Action::Notice(Notice::CommandDone { result, .. }) if result == "\"original\"")));
    success(&run(&mut e, setting(false)));
    assert!(!e.store().hedged());
}
#[test]
fn background_context_capacity_does_not_block_local_setting_success() {
    let mut e = seeded(false, true);
    assert!(control(&mut e, "backgrounds").is_empty());
    success(&run(&mut e, setting(false)));
    assert!(!e.store().hedged());
}
#[test]
fn duplicate_id_keeps_hedged_and_original_owner_unchanged() {
    let mut e = seeded(false, true);
    hold(&mut e, COMMAND);
    let a = run(&mut e, setting(false));
    assert!(
        matches!(a.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == "command.duplicate" && event.message == "Command ID is already in flight"
        && event.level == safe_cex_engine::Level::Error),
        "unchanged duplicate: {a:?}"
    );
    assert!(e.store().hedged());
    assert!(matches!(control(&mut e, "release").as_slice(),
        [Action::Notice(Notice::CommandDone { command: COMMAND, result })]
        if result == "\"original\""));
}
#[test]
fn expired_and_sub_ms_receipts_refuse_before_hedged_projection() {
    for tag in ["sub-ms", "expired"] {
        let mut e = seeded(false, true);
        assert!(control(&mut e, tag).is_empty());
        refusal(&run(&mut e, setting(false)), Error::TimedOut);
        assert!(e.store().hedged());
    }
}

#[test]
fn absent_symbol_keeps_native_one_way_projection() {
    let mut e = seeded(false, true);
    success(&run(
        &mut e,
        Command::SetPositionMode {
            command: COMMAND,
            hedged: false,
            symbol: None,
        },
    ));
    assert!(!e.store().hedged());
}
