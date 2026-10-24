#![cfg(feature = "spot")]
//! Local Read authority through genuine callbacks; native private fallthrough stays legacy.
mod support;
use safe_cex_engine::model::{
    Limits, MarginType, Market, MinMax, Order, OrderSide, OrderStatus, OrderType, Precision,
};
use safe_cex_engine::request_context::{Context, Error, Operation, Policy, RetryPolicy, Scope};
use safe_cex_engine::{
    Action, Capabilities, Command, Cx, Engine, ErrorKind, Event, HttpRequest, Method, Notice,
    Outcome, Query, Response, SocketId, TimerId, Venue, VenueError,
};
use safe_cex_venue_kucoin::{Kucoin, spot::KucoinSpot};
use serde_json::{Value, json};
use support::credentials::Options;

const SYMBOL: &str = "BTC-USDT";
const COMMAND: u32 = 71;
const CREDENTIALS: &str = "KuCoin Spot requires credentials";
const UNSUPPORTED_TRADES: &str = "KuCoin Spot public trades are not implemented in TS";
const UNSUPPORTED_LEVERAGE: &str = "KuCoin leverage query is unsupported";

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
                        match cx.http_context(Scope::Background, policy()) {
                            Ok(root) => self.held.push(root),
                            Err(safe_cex_engine::request_context::Error::Capacity) => break,
                            other => panic!("unexpected peer admission: {other:?}"),
                        }
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
            assert!(matches!(command, Command::FetchLeverage { .. }));
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
fn seeded<V: Venue>(
    options: Options,
    constructor: impl FnOnce(safe_cex_venue_kucoin::Options) -> V,
) -> Engine<Probe<V>> {
    let (options, provider, identities) = options.prepare();
    let inner = constructor(options);
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        held: vec![],
        hold_next: false,
        original: None,
    });
    e.set_signer(Some(std::sync::Arc::new(provider))).unwrap();
    e.set_identities(identities).unwrap();
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
fn spot(keyed: bool) -> Engine<Probe<KucoinSpot>> {
    seeded(options(keyed), KucoinSpot::new)
}
fn run<V: Venue>(e: &mut Engine<Probe<V>>, now: f64, command: Command) -> Vec<Action> {
    e.handle(now, Event::Command(command));
    safe_cex_engine::test_support::prepared_actions(e, now)
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
fn query(q: Query) -> Command {
    Command::Query {
        command: COMMAND,
        query: q,
    }
}
fn lookup(symbol: &str, id: &str) -> Command {
    query(Query::OrderByClientId {
        symbol: symbol.into(),
        client_order_id: id.into(),
    })
}
fn leverage(symbol: &str) -> Command {
    Command::FetchLeverage {
        command: COMMAND,
        symbol: symbol.into(),
    }
}
fn failure(a: &[Action]) -> &VenueError {
    assert_eq!(a.len(), 1, "one terminal without I/O or companion: {a:?}");
    match &a[0] {
        Action::Notice(Notice::CommandFailed {
            command: COMMAND,
            error,
        }) => error,
        _ => panic!("expected correlated failure: {a:?}"),
    }
}
fn done(a: &[Action]) -> Value {
    assert_eq!(a.len(), 1, "one local result without opaque work: {a:?}");
    match &a[0] {
        Action::Notice(Notice::CommandDone {
            command: COMMAND,
            result,
        }) => serde_json::from_str(result).unwrap(),
        _ => panic!("expected correlated result: {a:?}"),
    }
}
fn typed(a: &[Action], message: &str, kind: ErrorKind, outcome: Option<Outcome>) {
    let error = failure(a);
    assert_eq!(error.message, message);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, outcome);
}
enum Expected {
    Done(Value),
    Failed(&'static str, ErrorKind),
}
fn cases(keyed: bool) -> Vec<(Command, Expected)> {
    let queries = vec![
        (
            query(Query::Fills {
                symbol: None,
                order_id: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            }),
            Expected::Done(json!([])),
        ),
        (
            lookup(SYMBOL, "client-1"),
            Expected::Done(json!({"found": true, "order": order()})),
        ),
        (
            query(Query::PublicTrades {
                symbol: "UNKNOWN".into(),
                limit: Some(0),
            }),
            Expected::Failed(UNSUPPORTED_TRADES, ErrorKind::Unsupported),
        ),
        (
            query(Query::OrderHistory {
                symbol: None,
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            }),
            Expected::Done(json!([])),
        ),
        (
            query(Query::Income {
                symbol: None,
                kind: Some("unknown".into()),
                from_ms: Some(f64::NAN),
                to_ms: None,
                limit: Some(0),
            }),
            Expected::Done(json!([])),
        ),
    ];
    let mut result: Vec<_> = queries
        .into_iter()
        .map(|(c, expected)| {
            (
                c,
                if keyed {
                    expected
                } else {
                    Expected::Failed(CREDENTIALS, ErrorKind::InvalidInput)
                },
            )
        })
        .collect();
    if !keyed {
        result.push((
            Command::FetchAccount { command: COMMAND },
            Expected::Failed(CREDENTIALS, ErrorKind::InvalidInput),
        ));
    }
    result.push((leverage(SYMBOL), Expected::Done(json!(1))));
    result
}
fn check(a: &[Action], expected: &Expected) {
    match expected {
        Expected::Done(value) => assert_eq!(&done(a), value),
        Expected::Failed(message, kind) => typed(a, message, *kind, Some(Outcome::NotSent)),
    }
}

#[test]
fn local_spot_errors_gain_typed_metadata_without_changing_credential_precedence() {
    for keyed in [false, true] {
        let mut e = spot(keyed);
        for (command, expected) in cases(keyed) {
            if matches!(expected, Expected::Failed(..)) {
                check(&run(&mut e, 100., command), &expected);
            }
        }
    }
}
#[test]
fn futures_leverage_remains_unsupported_with_typed_local_authority() {
    for keyed in [false, true] {
        let mut e = seeded(options(keyed), Kucoin::new);
        typed(
            &run(&mut e, 100., leverage("UNKNOWN")),
            UNSUPPORTED_LEVERAGE,
            ErrorKind::Unsupported,
            Some(Outcome::NotSent),
        );
    }
}
#[test]
fn native_local_results_keep_cache_matching_full_order_and_empty_results() {
    let mut e = spot(true);
    for id in ["client-1", "native-1"] {
        assert_eq!(
            done(&run(&mut e, 100., lookup(SYMBOL, id))),
            json!({"found": true, "order": order()})
        );
    }
    let before = e.store().orders().to_vec();
    for (command, expected) in cases(true) {
        if matches!(expected, Expected::Done(_)) {
            check(&run(&mut e, 100., command), &expected);
        }
    }
    assert_eq!(e.store().orders(), before.as_slice());
}
#[test]
fn leverage_projection_keeps_constant_sides_margin_and_unchecked_symbol_behavior() {
    for keyed in [false, true] {
        let mut e = spot(keyed);
        for symbol in [SYMBOL, "UNKNOWN", ""] {
            assert_eq!(done(&run(&mut e, 100., leverage(symbol))), json!(1));
            let s = e.store().setting(symbol).unwrap();
            assert_eq!(s.leverage, 1.);
            assert_eq!(s.buy_leverage, Some(1.));
            assert_eq!(s.sell_leverage, Some(1.));
            assert_eq!(
                s.margin_type,
                (symbol == SYMBOL).then_some(MarginType::Cross)
            );
        }
    }
}
fn invalid_clocks<V: Venue>(
    mut e: Engine<Probe<V>>,
    commands: impl Fn() -> Vec<Command>,
) -> Engine<Probe<V>> {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for command in commands() {
            let before = e.store().setting(SYMBOL);
            let orders = e.store().orders().to_vec();
            let a = run(&mut e, now, command);
            let error = failure(&a);
            assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
            assert_eq!(error.outcome, None);
            assert_eq!(e.store().setting(SYMBOL), before);
            assert_eq!(e.store().orders(), orders.as_slice());
        }
    }
    e
}
#[test]
fn nonfinite_clock_refuses_every_local_spot_path_before_projection_or_publication() {
    for keyed in [false, true] {
        let mut e = invalid_clocks(spot(keyed), || {
            cases(keyed).into_iter().map(|(c, _)| c).collect()
        });
        for (command, expected) in cases(keyed) {
            check(&run(&mut e, 100., command), &expected);
        }
    }
}
#[test]
fn nonfinite_clock_refuses_futures_unsupported_terminal() {
    let mut e = invalid_clocks(seeded(options(true), Kucoin::new), || {
        vec![leverage(SYMBOL)]
    });
    typed(
        &run(&mut e, 100., leverage(SYMBOL)),
        UNSUPPORTED_LEVERAGE,
        ErrorKind::Unsupported,
        Some(Outcome::NotSent),
    );
}
fn saturated<V: Venue>(
    mut e: Engine<Probe<V>>,
    commands: impl Fn() -> Vec<Command>,
    recovery: Command,
    expected: Expected,
) {
    assert!(control(&mut e, Some("saturate")).is_empty());
    assert_eq!(
        e.venue().held.len(),
        256,
        "real registry saturation in a preceding callback"
    );
    let setting = e.store().setting(SYMBOL);
    let orders = e.store().orders().to_vec();
    for _ in 0..270 {
        for command in commands() {
            typed(
                &run(&mut e, 100., command),
                &Error::Capacity.to_string(),
                ErrorKind::Internal,
                None,
            );
            assert_eq!(e.venue().held.len(), 256);
            assert_eq!(e.store().setting(SYMBOL), setting);
            assert_eq!(e.store().orders(), orders.as_slice());
        }
    }
    assert!(control(&mut e, None).is_empty());
    assert!(e.venue().held.is_empty());
    check(&run(&mut e, 100., recovery), &expected);
}
#[test]
fn genuine_capacity_refusal_covers_every_local_spot_path_and_same_clock_recovery() {
    for keyed in [false, true] {
        saturated(
            spot(keyed),
            || cases(keyed).into_iter().map(|(c, _)| c).collect(),
            leverage(SYMBOL),
            Expected::Done(json!(1)),
        );
    }
}
#[test]
fn genuine_capacity_refusal_covers_futures_local_terminal_and_recovery() {
    saturated(
        seeded(options(true), Kucoin::new),
        || vec![leverage(SYMBOL)],
        leverage(SYMBOL),
        Expected::Failed(UNSUPPORTED_LEVERAGE, ErrorKind::Unsupported),
    );
}
#[test]
fn saturated_leverage_read_cannot_mutate_the_native_setting() {
    let mut e = spot(false);
    assert!(control(&mut e, Some("saturate")).is_empty());
    assert_eq!(e.venue().held.len(), 256);
    let before = e.store().setting(SYMBOL).unwrap();
    assert_eq!(before.leverage, 9.);
    assert_eq!(before.buy_leverage, Some(8.));
    assert_eq!(before.sell_leverage, Some(7.));
    let a = run(&mut e, 100., leverage(SYMBOL));
    assert_eq!(
        e.store().setting(SYMBOL),
        Some(before),
        "capacity denial must precede the local projection"
    );
    typed(&a, &Error::Capacity.to_string(), ErrorKind::Internal, None);
    assert_eq!(e.venue().held.len(), 256);
    assert!(control(&mut e, None).is_empty());
    assert!(e.venue().held.is_empty());
    assert_eq!(done(&run(&mut e, 100., leverage(SYMBOL))), json!(1));
}
#[test]
fn nonfinite_clock_leverage_read_cannot_mutate_the_native_setting() {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut e = spot(true);
        let before = e.store().setting(SYMBOL).unwrap();
        assert_eq!(before.leverage, 9.);
        assert_eq!(before.buy_leverage, Some(8.));
        assert_eq!(before.sell_leverage, Some(7.));
        let a = run(&mut e, now, leverage(SYMBOL));
        assert_eq!(
            e.store().setting(SYMBOL),
            Some(before),
            "clock denial must precede the local projection"
        );
        let error = failure(&a);
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, None);
        assert_eq!(done(&run(&mut e, 100., leverage(SYMBOL))), json!(1));
    }
}
#[test]
fn saturated_cached_hit_cannot_publish_the_native_order() {
    let mut e = spot(true);
    assert!(control(&mut e, Some("saturate")).is_empty());
    assert_eq!(e.venue().held.len(), 256);
    let before = e.store().orders().to_vec();
    let a = run(&mut e, 100., lookup(SYMBOL, "client-1"));
    assert_eq!(e.store().orders(), before.as_slice());
    assert!(
        !a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { .. }))),
        "a cached hit cannot publish without admission: {a:?}"
    );
    typed(&a, &Error::Capacity.to_string(), ErrorKind::Internal, None);
    assert_eq!(e.venue().held.len(), 256);
    assert!(control(&mut e, None).is_empty());
    assert!(e.venue().held.is_empty());
    assert_eq!(
        done(&run(&mut e, 100., lookup(SYMBOL, "client-1"))),
        json!({"found": true, "order": order()})
    );
}
#[test]
fn fixed_clock_reuse_retires_all_local_owners_beyond_registry_capacity() {
    for keyed in [false, true] {
        let mut e = spot(keyed);
        for _ in 0..270 {
            for (command, expected) in cases(keyed) {
                check(&run(&mut e, 100., command), &expected);
            }
        }
    }
    let mut e = seeded(options(true), Kucoin::new);
    for _ in 0..270 {
        typed(
            &run(&mut e, 100., leverage(SYMBOL)),
            UNSUPPORTED_LEVERAGE,
            ErrorKind::Unsupported,
            Some(Outcome::NotSent),
        );
    }
}
#[test]
fn duplicate_live_original_read_cannot_project_or_steal_completion() {
    let mut e = spot(false);
    assert!(control(&mut e, Some("hold-next")).is_empty());
    assert!(run(&mut e, 100., leverage(SYMBOL)).is_empty());
    assert!(e.venue().original.is_some());
    let before = e.store().setting(SYMBOL);
    let a = run(&mut e, 100., leverage(SYMBOL));
    assert!(
        matches!(&a[..], [Action::Notice(Notice::Event(event))] if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
        "duplicate has no completion or I/O: {a:?}"
    );
    assert_eq!(e.store().setting(SYMBOL), before);
    assert!(e.venue().original.is_some());
    assert_eq!(
        done(&control(&mut e, Some("finish-original"))),
        json!("original")
    );
    assert!(e.venue().original.is_none());
    assert_eq!(done(&run(&mut e, 100., leverage(SYMBOL))), json!(1));
    assert_eq!(e.store().setting(SYMBOL).unwrap().leverage, 1.);
}
fn native_request<'a>(a: &'a [Action], path: &str) -> &'a HttpRequest {
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
    assert_eq!(requests.len(), 1, "native private request: {a:?}");
    let r = requests[0];
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.url, format!("https://api.kucoin.com{path}"));
    assert_eq!(r.body, None);
    assert!(
        r.headers
            .iter()
            .any(|(k, v)| *k == "KC-API-KEY" && v == "fixture-key")
    );
    assert!(r.headers.iter().any(|(k, _)| *k == "KC-API-SIGN"));
    r
}
fn reply<V: Venue>(
    e: &mut Engine<Probe<V>>,
    request: safe_cex_engine::RequestId,
    body: &str,
) -> Vec<Action> {
    e.handle(
        100.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, 100.)
}
#[test]
fn private_fill_and_selected_identity_keep_native_wire_and_completion() {
    for (command, path, body, expected) in [
        (
            query(Query::Fills {
                symbol: Some(SYMBOL.into()),
                order_id: Some("native-1".into()),
                from_ms: Some(1.9),
                to_ms: Some(2.9),
                limit: Some(999),
            }),
            "/api/v1/hf/fills?symbol=BTC-USDT&orderId=native-1&limit=100&startAt=1&endAt=2",
            r#"{"code":"200000","data":{"items":[]}}"#,
            json!([]),
        ),
        (
            Command::FetchAccount { command: COMMAND },
            "/api/v2/user-info",
            r#"{"code":"200000","data":{"userId":"u","subUserId":"s","referrerUserId":"a"}}"#,
            json!({"userId":"u","subId":"s","affiliateId":"a"}),
        ),
    ] {
        let managed_identity = matches!(command, Command::FetchAccount { .. });
        let mut e = spot(true);
        let a = run(&mut e, 100., command);
        let id = native_request(&a, path).id;
        assert!(control(&mut e, Some("saturate")).is_empty());
        assert_eq!(
            e.venue().held.len(),
            if managed_identity { 255 } else { 256 },
            "only the migrated Identity retains a Read root"
        );
        let terminal: Vec<_> = reply(&mut e, id, body)
            .into_iter()
            .filter(|action| !matches!(action, Action::SetTimer { .. } | Action::ClearTimer { .. }))
            .collect();
        assert_eq!(
            done(&terminal),
            expected,
            "native result bytes remain exact"
        );
        assert!(control(&mut e, None).is_empty());
        assert!(e.venue().held.is_empty());
    }
}
#[test]
fn cache_misses_and_sentinel_ids_keep_native_pager_without_new_owner() {
    for (symbol, id) in [
        (SYMBOL, ""),
        (SYMBOL, "0"),
        (SYMBOL, "undefined"),
        ("MISSING", "client-1"),
        (SYMBOL, "absent"),
        (SYMBOL, "parent-1"),
    ] {
        let mut e = spot(true);
        let mut sentinel = order();
        sentinel.id = id.into();
        sentinel.client_order_id = Some(id.into());
        sentinel.symbol = if symbol == "MISSING" || matches!(id, "" | "0" | "undefined") {
            SYMBOL.into()
        } else {
            "ETH-USDT".into()
        };
        e.store_mut().set_orders(vec![sentinel, order()]);
        let a = run(&mut e, 100., lookup(symbol, id));
        let request = native_request(
            &a,
            &format!("/api/v1/hf/orders/active/page?symbol={symbol}&pageNum=1&pageSize=50"),
        )
        .id;
        assert!(control(&mut e, Some("saturate")).is_empty());
        assert_eq!(
            e.venue().held.len(),
            256,
            "native pending pager owns no Read root"
        );
        assert_eq!(
            done(&reply(
                &mut e,
                request,
                r#"{"code":"200000","data":{"totalPage":1,"items":[]}}"#
            )),
            json!({"found": false, "definitive": false})
        );
        assert!(control(&mut e, None).is_empty());
        assert!(e.venue().held.is_empty());
    }
}
#[test]
fn spot_setting_setters_keep_native_noops_and_capability() {
    let mut e = spot(false);
    assert!(e.venue().capabilities().account_margin_type);
    let before = e.store().setting(SYMBOL);
    for command in [
        Command::SetMarginType {
            command: COMMAND,
            symbol: SYMBOL.into(),
            isolated: true,
        },
        Command::SetPositionMode {
            command: COMMAND,
            hedged: true,
            symbol: None,
        },
    ] {
        assert_eq!(done(&run(&mut e, 100., command)), json!([]));
        assert_eq!(e.store().setting(SYMBOL), before);
    }
    assert_eq!(
        done(&run(
            &mut e,
            100.,
            Command::SetLeverage {
                command: COMMAND,
                symbol: SYMBOL.into(),
                leverage: 50.,
                side: None
            }
        )),
        json!([])
    );
    let setting = e.store().setting(SYMBOL).unwrap();
    assert_eq!(setting.leverage, 1.);
    assert_eq!(setting.buy_leverage, Some(1.));
    assert_eq!(setting.sell_leverage, Some(1.));
    assert_eq!(setting.margin_type, Some(MarginType::Cross));
}
