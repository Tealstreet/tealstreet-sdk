//! Original controls execute the native venue through a genuine engine entry.
use super::super::{AsterDex, Pending};
use crate::fixture::Options;
use crate::sign::Version;
use safe_cex_engine::book::Book;
use safe_cex_engine::model::*;
use safe_cex_engine::request_context::{Context, Operation, Policy, RetryPolicy};
use safe_cex_engine::{
    Action, Command, Cx, Engine, ErrorKind, Event, Method, Notice, Outcome, Response, SocketId,
    TimerId, Venue,
};

const NOW: f64 = 1_700_000_000_000.;
const OWNER: u32 = 700;
const PEER: &str = "__local_refusal_peer__";

struct Probe {
    inner: AsterDex,
    peers: Vec<(Context, TimerId)>,
    before: usize,
    after: usize,
}
impl Probe {
    fn live(&self, cx: &Cx<'_, Pending>) -> usize {
        self.peers
            .iter()
            .filter(|(root, _)| cx.validate_read_root_context(root).is_ok())
            .count()
    }
}
impl Venue for Probe {
    type Pending = Pending;
    fn start(&mut self, cx: &mut Cx<'_, Pending>) {
        self.inner.start(cx);
    }
    fn on_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        self.inner.on_open(cx, socket);
    }
    fn on_message(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, text: &str) {
        self.inner.on_message(cx, socket, text);
    }
    fn on_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId, code: u16) {
        self.inner.on_close(cx, socket, code);
    }
    fn on_response(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, response: Response<'_>) {
        self.inner.on_response(cx, pending, response);
    }
    fn on_request_failed(&mut self, cx: &mut Cx<'_, Pending>, pending: Pending, error: &str) {
        self.inner.on_request_failed(cx, pending, error);
    }
    fn on_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        if let Some(i) = self.peers.iter().position(|(_, id)| *id == timer) {
            let (root, _) = self.peers.remove(i);
            cx.done_context(&root, "[]".into()).unwrap();
        } else {
            self.inner.on_timer(cx, timer);
        }
    }
    fn on_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        self.before = self.live(cx);
        if matches!(&command, Command::FetchOrderBook { symbol, .. } if symbol == PEER) {
            let policy = Policy {
                operation: Operation::Read,
                credential: None,
                timeout_ms: 20_000,
                pacing: None,
                retry: RetryPolicy::Never,
            };
            if let Ok(root) = cx.admit_http_command(policy) {
                let timer = cx.set_timer(1000.);
                self.peers.push((root, timer));
            }
        } else {
            self.inner.on_command(cx, command);
        }
        self.after = self.live(cx);
    }
    fn book(&self, symbol: &str) -> Option<&Book> {
        self.inner.book(symbol)
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &Book)> + '_> {
        self.inner.books()
    }
}

#[derive(Clone, Copy, Debug)]
enum Case {
    PlanWrite,
    KeyWrite,
    CapacityWrite,
    PlanRead,
    KeyRead,
    FieldsWrite,
    FieldsRead,
}
impl Case {
    fn read(self) -> bool {
        matches!(self, Self::PlanRead | Self::KeyRead | Self::FieldsRead)
    }
    fn command(self, command: u32) -> Command {
        match self {
            Self::PlanRead | Self::KeyRead | Self::FieldsRead => Command::FetchStpMode { command },
            Self::PlanWrite => Command::SetStpMode {
                command,
                mode: StpMode::ExpireTaker,
            },
            Self::KeyWrite | Self::FieldsWrite => Command::SetPositionMode {
                command,
                symbol: None,
                hedged: true,
            },
            Self::CapacityWrite => Command::PlaceOrders {
                command,
                orders: vec![placement(); 10_001],
            },
        }
    }
    fn error(self) -> (&'static str, Option<ErrorKind>) {
        match self {
            Self::PlanRead | Self::PlanWrite => (
                "Aster self-trade prevention requires V3",
                Some(ErrorKind::InvalidInput),
            ),
            Self::KeyRead | Self::KeyWrite => (
                "Missing required Aster credentials",
                Some(ErrorKind::AuthInvalid),
            ),
            Self::FieldsRead | Self::FieldsWrite => (
                "Missing required Aster v3 auth fields: Main Wallet Address, API Wallet Address, and Generated Private Key",
                Some(ErrorKind::AuthInvalid),
            ),
            Self::CapacityWrite => (
                "Aster placement routing capacity reached; wait for order reconciliation or cancel existing acknowledged orders",
                None,
            ),
        }
    }
}
fn placement() -> PlaceOrderOpts {
    serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":9300}"#,
    )
    .unwrap()
}
fn market() -> Market {
    Market {
        id: "BTCUSDT".into(),
        symbol: "BTCUSDT".into(),
        pretty_symbol: None,
        base: "BTC".into(),
        quote: "USDT".into(),
        active: true,
        precision: Precision {
            price: 0.1,
            amount: 0.001,
        },
        limits: Limits {
            amount: MinMax {
                min: 0.001,
                max: 2.,
            },
            leverage: MinMax { min: 1., max: 125. },
            price: None,
        },
        contract_size: None,
        is_expiring: None,
        is_inverse: None,
        is_spot: None,
        is_perpetual: Some(true),
        is_read_only: None,
        taker_fee: None,
        maker_fee: None,
    }
}
fn engine(case: Case) -> Engine<Probe> {
    configured(case, false)
}
fn configured(case: Case, seed_ack: bool) -> Engine<Probe> {
    let keyed = matches!(
        case,
        Case::CapacityWrite | Case::FieldsRead | Case::FieldsWrite
    );
    let v3 = matches!(case, Case::KeyRead | Case::FieldsRead | Case::FieldsWrite);
    let options = Options {
        api_version: Some(if v3 { Version::V3 } else { Version::V1 }),
        key: keyed.then(|| "local-refusal-fixture".into()),
        secret: keyed.then(|| {
            if v3 {
                "01".repeat(32)
            } else {
                "local-refusal-fixture".into()
            }
        }),
        ..Options::default()
    };
    with_options(options, seed_ack)
}
fn with_options(options: Options, seed_ack: bool) -> Engine<Probe> {
    let (mut inner, provider) = options.build();
    if seed_ack {
        inner
            .trading
            .acknowledged
            .insert("retired".into(), "BTCUSDT".into());
        inner
            .trading
            .acknowledged
            .insert("live".into(), "BTCUSDT".into());
    }
    let mut e = safe_cex_engine::test_support::seeded_engine(Probe {
        inner,
        peers: Vec::new(),
        before: 0,
        after: 0,
    });
    e.set_signer(Some(provider)).unwrap();
    e.store_mut().set_markets(vec![market()]);
    e
}
fn act(e: &mut Engine<Probe>, at: f64, case: Case, command: u32) -> Vec<Action> {
    e.handle(at, Event::Command(case.command(command)));
    e.drain_actions().collect()
}
fn native(out: &[Action], case: Case) {
    let (message, kind) = case.error();
    assert_eq!(out.len(), 2, "native diagnostic then failure: {out:?}");
    assert!(
        matches!(&out[0], Action::Notice(Notice::CommandError { command: OWNER, message: got }) if got == message)
    );
    let Action::Notice(Notice::CommandFailed { command, error }) = &out[1] else {
        panic!("native failure: {out:?}");
    };
    assert_eq!(*command, OWNER);
    assert_eq!(error.message, message);
    assert_eq!(error.kind, kind);
    if !case.read() {
        assert_eq!(error.outcome, Some(Outcome::NotSent));
    }
}
fn refused(out: &[Action], case: Case, kind: ErrorKind) {
    assert_eq!(
        out.len(),
        1,
        "no diagnostic or effects before admission: {out:?}"
    );
    let Action::Notice(Notice::CommandFailed { command, error }) = &out[0] else {
        panic!("typed refusal: {out:?}");
    };
    assert_eq!(*command, OWNER);
    assert_eq!(error.kind, Some(kind));
    assert_eq!(error.outcome, (!case.read()).then_some(Outcome::NotSent));
}
fn fill(e: &mut Engine<Probe>) {
    for command in 1..=256 {
        e.handle(
            NOW,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: PEER.into(),
                limit: None,
            }),
        );
        assert!(
            e.drain_actions()
                .all(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
    assert_eq!(e.venue().peers.len(), 256);
}
fn invalid(case: Case) {
    for now in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        let mut e = engine(case);
        let before = e.snapshot_frame(NOW).to_vec();
        refused(
            &act(&mut e, now, case, OWNER),
            case,
            ErrorKind::InvalidInput,
        );
        assert_eq!(before, e.snapshot_frame(NOW));
        native(&act(&mut e, NOW, case, OWNER), case);
    }
}
fn capacity(case: Case) {
    let mut e = engine(case);
    fill(&mut e);
    let before = e.snapshot_frame(NOW).to_vec();
    refused(&act(&mut e, NOW, case, OWNER), case, ErrorKind::Internal);
    assert_eq!(e.venue().before, 256);
    assert_eq!(e.venue().after, 256);
    assert_eq!(before, e.snapshot_frame(NOW));
    let timer = e.venue().peers[0].1;
    e.handle(NOW + 1000., Event::TimerFired { timer });
    let out: Vec<_> = e.drain_actions().collect();
    assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, result }) if result == "[]")));
    native(&act(&mut e, NOW + 1001., case, OWNER), case);
    assert_eq!(e.venue().before, 255);
    assert_eq!(e.venue().after, 255);
}
macro_rules! pair {
    ($invalid:ident, $capacity:ident, $case:ident) => {
        #[test]
        fn $invalid() {
            invalid(Case::$case);
        }
        #[test]
        fn $capacity() {
            capacity(Case::$case);
        }
    };
}
pair!(invalid_plan_write, capacity_plan_write, PlanWrite);
pair!(invalid_key_write, capacity_key_write, KeyWrite);
pair!(
    invalid_routing_capacity,
    capacity_routing_capacity,
    CapacityWrite
);
pair!(invalid_plan_read, capacity_plan_read, PlanRead);
pair!(invalid_key_read, capacity_key_read, KeyRead);
pair!(invalid_fields_write, capacity_fields_write, FieldsWrite);
pair!(invalid_fields_read, capacity_fields_read, FieldsRead);

#[test]
fn native_local_errors_preserve_diagnostic_text_kind_and_mutation_outcome() {
    for case in [
        Case::PlanWrite,
        Case::KeyWrite,
        Case::CapacityWrite,
        Case::PlanRead,
        Case::KeyRead,
        Case::FieldsRead,
        Case::FieldsWrite,
    ] {
        native(&act(&mut engine(case), NOW, case, OWNER), case);
    }
}
fn read_outcome(case: Case) {
    let out = act(&mut engine(case), NOW, case, OWNER);
    native(&out, case);
    let Action::Notice(Notice::CommandFailed { error, .. }) = &out[1] else {
        unreachable!()
    };
    assert_eq!(error.outcome, None, "FetchStpMode is Read: {case:?}");
}
#[test]
fn local_unsupported_stp_read_has_no_money_outcome() {
    read_outcome(Case::PlanRead);
}
#[test]
fn local_missing_key_stp_read_has_no_money_outcome() {
    read_outcome(Case::KeyRead);
}
#[test]
fn local_missing_fields_stp_read_has_no_money_outcome() {
    read_outcome(Case::FieldsRead);
}

#[test]
fn credential_free_refusal_does_not_supply_auth_health_evidence() {
    let mut e = engine(Case::KeyWrite);
    native(&act(&mut e, NOW, Case::KeyWrite, OWNER), Case::KeyWrite);
    assert_eq!(e.health().auth_failures, 0);
}
#[test]
fn synchronous_refusals_release_entry_and_order_lock_custody() {
    let mut e = engine(Case::KeyWrite);
    for i in 0..300 {
        native(
            &act(&mut e, NOW + f64::from(i), Case::KeyWrite, OWNER),
            Case::KeyWrite,
        );
    }
}
#[test]
fn duplicate_owner_keeps_real_peer_and_its_single_terminal() {
    let mut e = engine(Case::PlanRead);
    e.handle(
        NOW,
        Event::Command(Command::FetchOrderBook {
            command: OWNER,
            symbol: PEER.into(),
            limit: None,
        }),
    );
    e.drain_actions().for_each(drop);
    let out = act(&mut e, NOW, Case::PlanRead, OWNER);
    assert!(!out.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
    )));
    assert_eq!(e.venue().peers.len(), 1);
    assert_eq!(out.iter().filter(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)).count(), 1);
    let timer = e.venue().peers[0].1;
    e.handle(NOW + 1000., Event::TimerFired { timer });
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone { command: OWNER, .. })
            ))
            .count(),
        1
    );
}
fn stale_order() -> Order {
    Order {
        id: "retired".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 9300.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 0.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn seeded_ack() -> Engine<Probe> {
    // A test-only native state seed, never a provider or physical-release receipt.
    let mut e = configured(Case::CapacityWrite, true);
    e.store_mut().set_orders(vec![stale_order()]);
    e
}

#[test]
fn invalid_entry_does_not_prune_acknowledged_routing_before_admission() {
    let mut e = seeded_ack();
    let before = e.venue().inner.trading.acknowledged.clone();
    let out = act(&mut e, f64::NAN, Case::CapacityWrite, OWNER);
    assert_eq!(e.venue().inner.trading.acknowledged, before);
    refused(&out, Case::CapacityWrite, ErrorKind::InvalidInput);
}
#[test]
fn full_entry_does_not_prune_acknowledged_routing_before_admission() {
    let mut e = seeded_ack();
    fill(&mut e);
    let before = e.venue().inner.trading.acknowledged.clone();
    let out = act(&mut e, NOW, Case::CapacityWrite, OWNER);
    assert_eq!(e.venue().inner.trading.acknowledged, before);
    refused(&out, Case::CapacityWrite, ErrorKind::Internal);
}
#[test]
fn native_routing_cleanup_removes_only_store_observed_acknowledgements() {
    let mut e = seeded_ack();
    native(
        &act(&mut e, NOW, Case::CapacityWrite, OWNER),
        Case::CapacityWrite,
    );
    assert!(!e.venue().inner.trading.acknowledged.contains_key("retired"));
    assert_eq!(
        e.venue()
            .inner
            .trading
            .acknowledged
            .get("live")
            .map(String::as_str),
        Some("BTCUSDT")
    );
}
#[test]
fn valid_legacy_setting_keeps_signed_post_without_a_managed_root() {
    let mut e = engine(Case::CapacityWrite);
    fill(&mut e);
    e.handle(
        NOW,
        Event::Command(Command::SetPositionMode {
            command: OWNER,
            symbol: None,
            hedged: true,
        }),
    );
    let mut out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    if let Some(id) = out.iter().find_map(|a| match a {
        Action::Http(h) if h.url.ends_with("/time") => Some(h.id),
        _ => None,
    }) {
        e.handle(
            NOW,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"serverTime":1700000000000}"#,
                },
            },
        );
        out.extend(safe_cex_engine::test_support::prepared_actions(&mut e, NOW));
    }
    let http = out
        .iter()
        .find_map(|a| match a {
            Action::Http(h) if h.url.contains("/positionSide/dual?") => Some(h),
            _ => None,
        })
        .expect("unchanged accepted native signed write at full managed capacity");
    assert_eq!(http.method, Method::Post);
    assert!(http.url.contains("dualSidePosition=true&"));
    assert!(http.url.contains("signature="));
    assert!(!e.venue().inner.trading.jobs.is_empty());
    assert_eq!(e.venue().peers.len(), 256);
    assert_eq!(e.venue().after, 256);
}

#[test]
fn valid_v3_stp_read_keeps_secondary_signed_get_at_full_managed_capacity() {
    let primary = "01".repeat(32);
    let secondary = "02".repeat(32);
    let primary_address = safe_cex_crypto::ethereum::private_key_address(&primary).unwrap();
    let secondary_address = safe_cex_crypto::ethereum::private_key_address(&secondary).unwrap();
    let mut e = with_options(
        Options {
            api_version: Some(Version::V3),
            application_id: Some(primary_address.clone()),
            key: Some(primary_address),
            secret: Some(primary),
            read_key: Some(secondary_address.clone()),
            read_secret: Some(secondary),
            ..Options::default()
        },
        false,
    );
    fill(&mut e);
    e.handle(
        NOW,
        Event::Command(Command::FetchStpMode { command: OWNER }),
    );
    let mut out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    if let Some(id) = out.iter().find_map(|a| match a {
        Action::Http(h) if h.url.ends_with("/time") => Some(h.id),
        _ => None,
    }) {
        e.handle(
            NOW,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"serverTime":1700000000000}"#,
                },
            },
        );
        out.extend(safe_cex_engine::test_support::prepared_actions(&mut e, NOW));
    }
    let http = out
        .iter()
        .find_map(|a| match a {
            Action::Http(h) if h.url.contains("/fapi/v3/stpMode?") => Some(h),
            _ => None,
        })
        .expect("native signed STP Read retains secondary selection at full capacity");
    assert_eq!(http.method, Method::Get);
    assert!(http.url.contains(&format!("signer={secondary_address}")));
    assert!(http.url.contains("signature="));
    let request = http.id;
    e.handle(
        NOW,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"stpMode":"EXPIRE_TAKER"}"#,
            },
        },
    );
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: OWNER, result }) if result == "\"EXPIRE_TAKER\"")));
    assert_eq!(e.venue().peers.len(), 256);
    assert_eq!(e.venue().after, 256);
}

#[test]
fn retired_ack_does_not_consume_native_capacity_for_an_accepted_job() {
    let mut e = seeded_ack();
    fill(&mut e);
    e.handle(
        NOW,
        Event::Command(Command::PlaceOrders {
            command: OWNER,
            orders: vec![placement(); 9_999],
        }),
    );
    let mut out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    if let Some(id) = out.iter().find_map(|a| match a {
        Action::Http(h) if h.url.ends_with("/time") => Some(h.id),
        _ => None,
    }) {
        e.handle(
            NOW,
            Event::HttpResponse {
                request: id,
                response: Response {
                    status: 200,
                    headers: "",
                    body: r#"{"serverTime":1700000000000}"#,
                },
            },
        );
        out.extend(safe_cex_engine::test_support::prepared_actions(&mut e, NOW));
    }
    assert!(out.iter().any(
        |a| matches!(a, Action::Http(h) if h.method == Method::Post && h.url.contains("/order?"))
    ));
    let inner = &e.venue().inner;
    assert!(!inner.trading.acknowledged.contains_key("retired"));
    assert!(inner.trading.acknowledged.contains_key("live"));
    assert_eq!(
        inner.trading.jobs.get(&OWNER).unwrap().reserved_places,
        9_999
    );
    assert_eq!(e.venue().peers.len(), 256);
    assert_eq!(e.venue().after, 256);
}
