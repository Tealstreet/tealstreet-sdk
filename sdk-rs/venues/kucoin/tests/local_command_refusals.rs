//! Local refusal admission through genuine Engine callbacks; private fallthrough stays native.
#[cfg(feature = "spot")]
use safe_cex_engine::model::StpMode;
use safe_cex_engine::model::{
    Interval, Limits, MarginType, Market, MinMax, Order, OrderSide, OrderStatus, OrderType,
    PlaceOrderOpts, Precision,
};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Response,
    SocketId, TimerId, Venue, VenueError,
};
#[cfg(feature = "spot")]
use safe_cex_venue_kucoin::spot::KucoinSpot;
use safe_cex_venue_kucoin::{Kucoin, Options};
use serde_json::json;

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

fn options(keyed: bool) -> Options {
    Options {
        key: keyed.then(|| "fixture-key".into()),
        secret: keyed.then(|| "fixture-secret".into()),
        passphrase: keyed.then(|| "fixture-passphrase".into()),
        ..Options::default()
    }
}
fn order() -> Order {
    Order {
        id: "native-1".into(),
        parent_id: Some("parent-1".into()),
        status: OrderStatus::Open,
        symbol: SYMBOL.into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 93.,
        amount: 2.,
        filled: 0.5,
        remaining: 1.5,
        reduce_only: false,
        close: Some(false),
        client_order_id: Some("client-1".into()),
        time_in_force: None,
        last_update: 12345.,
        trigger_type: None,
        take_profit: Some(105.),
        stop_loss: Some(80.),
        extra: None,
    }
}
fn seeded<V: Venue>(inner: V) -> Engine<Probe<V>> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        held: vec![],
        hold_next: false,
        original: None,
    });
    e.store_mut().set_markets(vec![Market {
        id: SYMBOL.into(),
        symbol: SYMBOL.into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            amount: 0.001,
            price: 0.1,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 1e6,
            },
            leverage: MinMax { min: 1., max: 1. },
            price: None,
        },
        contract_size: Some(1.),
        is_expiring: None,
        is_inverse: None,
        is_spot: Some(true),
        is_perpetual: None,
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }]);
    let mut other = order();
    other.id = "other-native".into();
    other.symbol = "ETH-USDT".into();
    e.store_mut().set_orders(vec![other, order()]);
    e.store_mut().update_setting(SYMBOL, |s| {
        s.leverage = 9.;
        s.buy_leverage = Some(8.);
        s.sell_leverage = Some(7.);
        s.margin_type = Some(MarginType::Cross);
    });
    e
}
#[cfg(feature = "spot")]
fn spot(keyed: bool) -> Engine<Probe<KucoinSpot>> {
    seeded(KucoinSpot::new(options(keyed)))
}
fn run<V: Venue>(e: &mut Engine<Probe<V>>, now: f64, command: Command) -> Vec<Action> {
    e.handle(now, Event::Command(command));
    e.drain_actions().collect()
}
fn control<V: Venue>(e: &mut Engine<Probe<V>>, tag: Option<&str>) -> Vec<Action> {
    run(
        e,
        100.,
        Command::SetTickerSymbols {
            symbols: tag.into_iter().map(Into::into).collect(),
        },
    )
}

#[derive(Clone, Copy, Debug)]
enum Case {
    CandleRange,
    CandleInterval,
    FuturesValidation,
    FuturesCredentials,
    #[cfg(feature = "spot")]
    SpotAmendTarget,
    #[cfg(feature = "spot")]
    SpotUnsupportedRead,
    #[cfg(feature = "spot")]
    SpotUnsupportedBook,
    #[cfg(feature = "spot")]
    SpotUnsupportedWrite,
    #[cfg(feature = "spot")]
    SpotCredentials,
    #[cfg(feature = "spot")]
    SpotBuild,
}
fn placement(client: Option<&str>, price: Option<f64>) -> PlaceOrderOpts {
    let mut value = json!({"symbol":SYMBOL,"type":"limit","side":"buy","amount":1.,"price":price});
    if let Some(client) = client {
        value["clientOrderId"] = client.into();
    }
    serde_json::from_value(value).unwrap()
}
impl Case {
    fn command(self) -> Command {
        match self {
            Self::CandleRange | Self::CandleInterval => Command::FetchCandles {
                command: COMMAND,
                symbol: SYMBOL.into(),
                interval: if matches!(self, Self::CandleInterval) {
                    Interval::S1
                } else {
                    Interval::M1
                },
                limit: None,
                from_ms: Some(-1.),
                to_ms: Some(100_000.),
            },
            Self::FuturesValidation => Command::PlaceOrders {
                command: COMMAND,
                orders: vec![placement(Some(""), Some(90.))],
            },
            Self::FuturesCredentials => Command::CancelOrders {
                command: COMMAND,
                ids: vec!["native-1".into()],
            },
            #[cfg(feature = "spot")]
            Self::SpotAmendTarget => Command::AmendOrder {
                command: COMMAND,
                id: "missing-native".into(),
                price: Some(90.),
                amount: None,
            },
            #[cfg(feature = "spot")]
            Self::SpotUnsupportedRead => Command::FetchStpMode { command: COMMAND },
            #[cfg(feature = "spot")]
            Self::SpotUnsupportedBook => Command::FetchOrderBook {
                command: COMMAND,
                symbol: SYMBOL.into(),
                limit: Some(10),
            },
            #[cfg(feature = "spot")]
            Self::SpotUnsupportedWrite => Command::SetStpMode {
                command: COMMAND,
                mode: StpMode::ExpireTaker,
            },
            #[cfg(feature = "spot")]
            Self::SpotCredentials => Command::CancelSymbolOrders {
                command: COMMAND,
                symbol: SYMBOL.into(),
            },
            #[cfg(feature = "spot")]
            Self::SpotBuild => Command::AmendOrderWithSymbol {
                command: COMMAND,
                symbol: SYMBOL.into(),
                id: "native-1".into(),
                price: None,
                amount: None,
            },
        }
    }
    fn mutation(self) -> bool {
        !matches!(self, Self::CandleRange | Self::CandleInterval) && {
            #[cfg(feature = "spot")]
            {
                !matches!(self, Self::SpotUnsupportedRead | Self::SpotUnsupportedBook)
            }
            #[cfg(not(feature = "spot"))]
            {
                true
            }
        }
    }
    fn expected(self) -> VenueError {
        let error = match self {
            Self::CandleRange => VenueError::new("invalid KuCoin candle range"),
            Self::CandleInterval => VenueError::new(Interval::S1.unsupported("KuCoin")),
            Self::FuturesValidation => {
                VenueError::new("KuCoin clientOrderId must match /^[A-Za-z0-9_-]{1,40}$/")
            }
            Self::FuturesCredentials => {
                VenueError::new("KuCoin credentials are required").kind(ErrorKind::AuthInvalid)
            }
            #[cfg(feature = "spot")]
            Self::SpotAmendTarget => VenueError::new("KuCoin Spot amend order not found"),
            #[cfg(feature = "spot")]
            Self::SpotUnsupportedRead | Self::SpotUnsupportedBook | Self::SpotUnsupportedWrite => {
                VenueError::new("unsupported KuCoin Spot command").kind(ErrorKind::Unsupported)
            }
            #[cfg(feature = "spot")]
            Self::SpotCredentials => {
                VenueError::new("KuCoin Spot requires credentials").kind(ErrorKind::AuthInvalid)
            }
            #[cfg(feature = "spot")]
            Self::SpotBuild => VenueError::new("KuCoin Spot amend has no changes"),
        };
        // The existing unsupported Read errors already carry NotSent; preserve native behavior.
        #[cfg(feature = "spot")]
        if matches!(self, Self::SpotUnsupportedRead | Self::SpotUnsupportedBook) {
            return error.outcome(Outcome::NotSent);
        }
        if self.mutation() {
            error.outcome(Outcome::NotSent)
        } else {
            error
        }
    }
}
fn failure(actions: &[Action]) -> &VenueError {
    assert_eq!(
        actions.len(),
        1,
        "exactly one local failure without opaque I/O: {actions:?}"
    );
    match &actions[0] {
        Action::Notice(Notice::CommandFailed {
            command: COMMAND,
            error,
        }) => error,
        _ => panic!("expected correlated failure: {actions:?}"),
    }
}
fn snapshot<V: Venue>(e: &Engine<Probe<V>>) -> serde_json::Value {
    json!({"orders":e.store().orders(), "setting":format!("{:?}", e.store().setting(SYMBOL)), "hedged":e.store().hedged()})
}
fn capacity<V: Venue>(mut e: Engine<Probe<V>>, case: Case) {
    let before = snapshot(&e);
    assert!(control(&mut e, Some("saturate")).is_empty());
    let actions = run(&mut e, 101., case.command());
    let mut expected = Error::Capacity.venue_error();
    expected.outcome = case.mutation().then_some(Outcome::NotSent);
    assert_eq!(
        failure(&actions),
        &expected,
        "{case:?} must admit its genuine entry"
    );
    assert_eq!(snapshot(&e), before);
    assert!(control(&mut e, None).is_empty());
    assert_eq!(
        failure(&run(&mut e, 102., case.command())),
        &case.expected()
    );
    assert_eq!(snapshot(&e), before);
}
fn native<V: Venue>(mut e: Engine<Probe<V>>, cases: &[Case]) {
    let before = snapshot(&e);
    for &case in cases {
        assert_eq!(
            failure(&run(&mut e, 101., case.command())),
            &case.expected(),
            "{case:?}"
        );
        assert_eq!(snapshot(&e), before);
    }
}
macro_rules! capacity_test {
    ($name:ident, $venue:expr, $case:ident) => {
        #[test]
        fn $name() {
            capacity($venue, Case::$case);
        }
    };
}
capacity_test!(
    futures_candle_range_capacity,
    seeded(Kucoin::new(options(false))),
    CandleRange
);
capacity_test!(
    futures_candle_interval_capacity,
    seeded(Kucoin::new(options(false))),
    CandleInterval
);
capacity_test!(
    futures_validation_capacity,
    seeded(Kucoin::new(options(false))),
    FuturesValidation
);
capacity_test!(
    futures_missing_credentials_capacity,
    seeded(Kucoin::new(options(false))),
    FuturesCredentials
);
#[cfg(feature = "spot")]
capacity_test!(spot_amend_target_capacity, spot(false), SpotAmendTarget);
#[cfg(feature = "spot")]
capacity_test!(
    spot_unsupported_read_capacity,
    spot(false),
    SpotUnsupportedRead
);
#[cfg(feature = "spot")]
capacity_test!(
    spot_unsupported_book_capacity,
    spot(false),
    SpotUnsupportedBook
);
#[cfg(feature = "spot")]
capacity_test!(
    spot_unsupported_write_capacity,
    spot(false),
    SpotUnsupportedWrite
);
#[cfg(feature = "spot")]
capacity_test!(
    spot_missing_credentials_capacity,
    spot(false),
    SpotCredentials
);
#[cfg(feature = "spot")]
capacity_test!(spot_builder_capacity, spot(true), SpotBuild);
#[test]
fn native_futures_errors_precede_credentials_and_preserve_store() {
    native(
        seeded(Kucoin::new(options(false))),
        &[
            Case::CandleRange,
            Case::CandleInterval,
            Case::FuturesValidation,
            Case::FuturesCredentials,
        ],
    );
    native(
        seeded(Kucoin::new(options(true))),
        &[Case::FuturesValidation],
    );
}
#[cfg(feature = "spot")]
#[test]
fn native_spot_errors_preserve_order_and_store() {
    native(
        spot(false),
        &[
            Case::SpotAmendTarget,
            Case::SpotUnsupportedRead,
            Case::SpotUnsupportedBook,
            Case::SpotUnsupportedWrite,
            Case::SpotCredentials,
        ],
    );
    native(
        spot(true),
        &[
            Case::SpotAmendTarget,
            Case::SpotUnsupportedRead,
            Case::SpotUnsupportedBook,
            Case::SpotUnsupportedWrite,
            Case::SpotBuild,
        ],
    );
    let mut e = spot(false);
    let error = run(&mut e, 101., Case::SpotBuild.command());
    assert_eq!(
        failure(&error),
        &Case::SpotCredentials.expected(),
        "credentials precede a completed builder error"
    );
}
#[cfg(feature = "spot")]
#[test]
fn native_spot_uuid_draws_precede_local_credentials_and_later_builder_failures() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    for keyed in [false, true] {
        for supplied in [false, true] {
            for batch in [false, true] {
                let mut e = spot(keyed);
                let draws = Arc::new(AtomicUsize::new(0));
                let count = draws.clone();
                e.set_entropy(Box::new(move |bytes| {
                    count.fetch_add(1, Ordering::SeqCst);
                    bytes.fill(7);
                    Ok(())
                }))
                .unwrap();
                e.set_signing_entropy(Box::new(|_| {
                    panic!("local refusal must not invoke signing entropy")
                }))
                .unwrap();
                let client = supplied.then_some("caller-exact");
                let mut orders = vec![placement(client, if batch { Some(90.) } else { None })];
                if batch {
                    orders.push(placement(supplied.then_some("caller-second"), None));
                }
                let actions = run(
                    &mut e,
                    101.,
                    Command::PlaceOrders {
                        command: COMMAND,
                        orders,
                    },
                );
                let expected = if keyed {
                    VenueError::new("KuCoin Spot limit order needs a price")
                        .outcome(Outcome::NotSent)
                } else {
                    Case::SpotCredentials.expected()
                };
                assert_eq!(failure(&actions), &expected);
                assert_eq!(
                    draws.load(Ordering::SeqCst),
                    if supplied {
                        0
                    } else if batch {
                        2
                    } else {
                        1
                    }
                );
            }
        }
    }
}
#[test]
fn valid_futures_candles_keep_public_wire_fallthrough() {
    let mut e = seeded(Kucoin::new(options(false)));
    let actions = run(
        &mut e,
        101.,
        Command::FetchCandles {
            command: COMMAND,
            symbol: SYMBOL.into(),
            interval: Interval::M1,
            limit: None,
            from_ms: Some(0.),
            to_ms: Some(60_000.),
        },
    );
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
    assert_eq!(requests.len(), 1, "{actions:?}");
    assert_eq!(requests[0].method, Method::Get);
    assert!(
        requests[0]
            .url
            .ends_with("/api/v1/kline/query?symbol=BTC-USDT&granularity=1&from=0&to=60000")
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}

fn owner_lifetime<V: Venue>(mut e: Engine<Probe<V>>, cases: &[Case]) {
    let before = snapshot(&e);
    assert!(control(&mut e, Some("hold-next")).is_empty());
    assert!(run(&mut e, 100., cases[0].command()).is_empty());
    for &case in cases {
        let actions = run(&mut e, 100., case.command());
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
    // More than the registry bound at one clock proves each local terminal retires its owner.
    for _ in 0..270 {
        for &case in cases {
            assert_eq!(
                failure(&run(&mut e, 100., case.command())),
                &case.expected()
            );
        }
    }
    assert_eq!(snapshot(&e), before);
}
#[test]
fn futures_duplicate_and_fixed_clock_reuse_keep_original_owner() {
    owner_lifetime(
        seeded(Kucoin::new(options(false))),
        &[
            Case::CandleRange,
            Case::CandleInterval,
            Case::FuturesValidation,
            Case::FuturesCredentials,
        ],
    );
}
#[cfg(feature = "spot")]
#[test]
fn spot_duplicate_and_fixed_clock_reuse_keep_original_owner() {
    owner_lifetime(
        spot(false),
        &[
            Case::SpotAmendTarget,
            Case::SpotUnsupportedRead,
            Case::SpotUnsupportedBook,
            Case::SpotUnsupportedWrite,
            Case::SpotCredentials,
        ],
    );
    owner_lifetime(spot(true), &[Case::SpotBuild]);
}

fn private_fallthrough<V: Venue>(mut e: Engine<Probe<V>>, command: Command, path: &str) {
    assert!(control(&mut e, Some("saturate")).is_empty());
    let actions = run(&mut e, 101., command);
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(
        requests.len(),
        1,
        "valid private path must not acquire a local owner: {actions:?}"
    );
    let request = requests[0];
    assert_eq!(request.method, Method::Delete);
    assert!(request.url.ends_with(path), "{}", request.url);
    assert!(request.body.is_none());
    assert_eq!(
        request
            .headers
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        [
            "KC-API-KEY",
            "KC-API-TIMESTAMP",
            "KC-API-KEY-VERSION",
            "KC-API-SIGN",
            "KC-API-PASSPHRASE",
            "Content-Type",
            "X-CHANNEL-API-CODE"
        ]
    );
    assert_eq!(request.headers[0].1, "fixture-key");
    assert!(!request.headers[3].1.is_empty());
    assert!(!request.headers[4].1.is_empty());
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    assert_eq!(e.venue().held.len(), 256);
}
#[test]
fn valid_futures_private_wire_ignores_local_refusal_capacity() {
    private_fallthrough(
        seeded(Kucoin::new(options(true))),
        Case::FuturesCredentials.command(),
        "/api/v1/orders/native-1?symbol=BTC-USDT",
    );
}
#[cfg(feature = "spot")]
#[test]
fn valid_spot_private_wire_ignores_local_refusal_capacity() {
    private_fallthrough(
        spot(true),
        Case::SpotCredentials.command(),
        "/api/v1/hf/orders?symbol=BTC-USDT",
    );
}
