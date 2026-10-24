//! Genuine command admission for initial Bitrue refusals; queued private work stays legacy.
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Response,
    SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_bitrue::{Bitrue, Options};
use serde_json::json;
const COMMAND: u32 = 71;
const SYMBOL: &str = "E-BTC-USDT";
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
    held: Vec<Context>,
    hold_next: bool,
    original: Option<Context>,
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
        // Allocate background peers in a preceding no-ID callback, never in the tested entry.
        if let Command::SetTickerSymbols { symbols } = &command {
            match symbols.first().map(String::as_str) {
                Some("saturate") => {
                    assert!(self.held.is_empty());
                    for _ in 0..256 {
                        self.held
                            .push(cx.http_context(Scope::Background, policy()).unwrap());
                    }
                }
                Some("hold-next") => self.hold_next = true,
                Some("finish-original") => {
                    let original = self.original.take().unwrap();
                    cx.done_context(&original, "\"original\"".into()).unwrap();
                }
                None => {
                    for root in self.held.drain(..) {
                        cx.finish_http_context(&root).unwrap();
                    }
                }
                _ => panic!("unknown fixture control"),
            }
            return;
        }
        if self.hold_next {
            self.hold_next = false;
            assert!(self.original.is_none());
            self.original = Some(cx.admit_http_command(policy()).unwrap());
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

fn seeded(keyed: bool, public_only: bool) -> Engine<Probe<Bitrue>> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner: Bitrue::new(Options {
            key: keyed.then(|| "fixture-key".into()),
            secret: keyed.then(|| "fixture-secret".into()),
            public_data_only: public_only,
            ..Options::default()
        }),
        held: vec![],
        hold_next: false,
        original: None,
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

#[derive(Clone, Copy, Debug)]
enum Case {
    Margin,
    Hedged,
    Planning,
}
impl Case {
    fn command(self) -> Command {
        match self {
            Self::Margin => Command::SetMarginType {
                command: COMMAND,
                symbol: SYMBOL.into(),
                isolated: true,
            },
            Self::Hedged => Command::SetPositionMode {
                command: COMMAND,
                symbol: Some(SYMBOL.into()),
                hedged: true,
            },
            Self::Planning => leverage(0.),
        }
    }
    fn expected(self, keyed: bool, public_only: bool) -> VenueError {
        match self {
            Self::Margin => VenueError::new("Bitrue margin mode cannot be changed through the API")
                .kind(ErrorKind::Unsupported),
            Self::Hedged => VenueError::new("Bitrue supports one-way positions only")
                .kind(ErrorKind::Unsupported),
            Self::Planning => VenueError::new(if keyed && !public_only {
                "Bitrue leverage must be positive"
            } else {
                "Bitrue trading requires API credentials"
            }),
        }
        .outcome(Outcome::NotSent)
    }
}
fn leverage(leverage: f64) -> Command {
    Command::SetLeverage {
        command: COMMAND,
        symbol: SYMBOL.into(),
        leverage,
        side: None,
    }
}
fn run(e: &mut Engine<Probe<Bitrue>>, command: Command) -> Vec<Action> {
    e.handle(100., Event::Command(command));
    e.drain_actions().collect()
}
fn control(e: &mut Engine<Probe<Bitrue>>, tag: Option<&str>) -> Vec<Action> {
    run(
        e,
        Command::SetTickerSymbols {
            symbols: tag.into_iter().map(Into::into).collect(),
        },
    )
}
fn failure(actions: &[Action]) -> &VenueError {
    assert_eq!(
        actions.len(),
        1,
        "one correlated local terminal without I/O: {actions:?}"
    );
    match &actions[0] {
        Action::Notice(Notice::CommandFailed {
            command: COMMAND,
            error,
        }) => error,
        _ => panic!("{actions:?}"),
    }
}
fn snapshot(e: &Engine<Probe<Bitrue>>) -> serde_json::Value {
    json!({"orders":e.store().orders(),"setting":format!("{:?}",e.store().setting(SYMBOL)),"hedged":e.store().hedged()})
}
fn capacity(case: Case) {
    for (keyed, public_only) in [(false, false), (true, false), (true, true)] {
        let mut e = seeded(keyed, public_only);
        e.store_mut().set_hedged(true);
        let before = snapshot(&e);
        assert!(control(&mut e, Some("saturate")).is_empty());
        let actions = run(&mut e, case.command());
        assert_eq!(
            failure(&actions),
            &Error::Capacity.venue_error(),
            "{case:?} must admit the genuine entry"
        );
        assert_eq!(snapshot(&e), before);
        assert!(control(&mut e, None).is_empty());
        assert_eq!(
            failure(&run(&mut e, case.command())),
            &case.expected(keyed, public_only)
        );
        assert_eq!(snapshot(&e), before);
    }
}
#[test]
fn margin_refusal_gains_genuine_capacity_denial() {
    capacity(Case::Margin);
}
#[test]
fn hedged_refusal_gains_genuine_capacity_denial() {
    capacity(Case::Hedged);
}
#[test]
fn initial_planning_refusal_gains_genuine_capacity_denial() {
    capacity(Case::Planning);
}
#[test]
fn native_errors_preserve_credential_order_and_store() {
    for (keyed, public_only) in [(false, false), (true, false), (true, true)] {
        let mut e = seeded(keyed, public_only);
        e.store_mut().set_hedged(true);
        let before = snapshot(&e);
        for case in [Case::Margin, Case::Hedged, Case::Planning] {
            assert_eq!(
                failure(&run(&mut e, case.command())),
                &case.expected(keyed, public_only)
            );
            assert_eq!(snapshot(&e), before);
        }
    }
}
#[test]
fn native_one_way_setting_stays_a_keyless_success() {
    for (keyed, public_only) in [(false, false), (true, false), (true, true)] {
        let mut e = seeded(keyed, public_only);
        e.store_mut().set_hedged(true);
        assert!(control(&mut e, Some("saturate")).is_empty());
        let actions = run(
            &mut e,
            Command::SetPositionMode {
                command: COMMAND,
                symbol: None,
                hedged: false,
            },
        );
        assert!(
            matches!(&actions[..],[Action::Notice(Notice::CommandDone{command:COMMAND,result})] if result=="[]"),
            "{actions:?}"
        );
        assert!(!e.store().hedged());
        assert_eq!(e.venue().held.len(), 256);
    }
}
#[test]
fn valid_private_wire_does_not_admit_a_local_refusal_owner() {
    let mut e = seeded(true, false);
    assert!(control(&mut e, Some("saturate")).is_empty());
    let actions = run(&mut e, leverage(2.));
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "{actions:?}");
    let r = requests[0];
    assert_eq!(r.method, Method::Post);
    assert!(r.url.ends_with("/fapi/v2/level_edit"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(r.body.as_deref().unwrap()).unwrap(),
        json!({"contractName":SYMBOL,"leverage":2.})
    );
    assert_eq!(
        r.headers.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        [
            "Content-Type",
            "X-Referer",
            "X-CH-APIKEY",
            "X-CH-TS",
            "recvWindow",
            "X-CH-SIGN"
        ]
    );
    assert_eq!(r.headers[2].1, "fixture-key");
    assert!(!r.headers[5].1.is_empty());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    assert_eq!(e.venue().held.len(), 256);
}

#[test]
fn duplicate_owner_and_fixed_clock_id_reuse_keep_one_terminal() {
    for (keyed, public_only) in [(false, false), (true, false), (true, true)] {
        let mut e = seeded(keyed, public_only);
        e.store_mut().set_hedged(true);
        let before = snapshot(&e);
        assert!(control(&mut e, Some("hold-next")).is_empty());
        assert!(
            run(
                &mut e,
                Command::FetchLeverage {
                    command: COMMAND,
                    symbol: SYMBOL.into()
                }
            )
            .is_empty()
        );
        for case in [Case::Margin, Case::Hedged, Case::Planning] {
            let actions = run(&mut e, case.command());
            assert!(
                matches!(&actions[..], [Action::Notice(Notice::Event(event))]
                if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
                "{actions:?}"
            );
            assert_eq!(snapshot(&e), before);
            assert!(e.venue().original.is_some());
        }
        let actions = control(&mut e, Some("finish-original"));
        assert!(
            matches!(&actions[..], [Action::Notice(Notice::CommandDone { command: COMMAND, result })]
            if result == "\"original\""),
            "{actions:?}"
        );
        assert!(e.venue().original.is_none());
        for _ in 0..270 {
            for case in [Case::Margin, Case::Hedged, Case::Planning] {
                assert_eq!(
                    failure(&run(&mut e, case.command())),
                    &case.expected(keyed, public_only)
                );
            }
        }
        assert_eq!(snapshot(&e), before);
    }
}
