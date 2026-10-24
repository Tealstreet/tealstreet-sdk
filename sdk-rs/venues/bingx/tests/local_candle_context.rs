//! Local candle refusals must use the genuine command's Read authority.
use safe_cex_engine::{
    Action, Command, Engine, ErrorKind, Event, Notice, RequestId, Response, VenueError,
    model::{Interval, Limits, Market, MinMax, Precision},
};
use safe_cex_venue_bingx::Bingx;
mod support;

const NOW: f64 = 1_000.;
const COMMAND: u32 = 42;

#[derive(Clone, Copy)]
enum Refusal {
    UnknownMarket,
    UnsupportedInterval,
    InvalidRange,
}
impl Refusal {
    fn command(self, command: u32) -> Command {
        Command::FetchCandles {
            command,
            symbol: match self {
                Self::UnknownMarket => "MISSING",
                _ => "BTCUSDT",
            }
            .into(),
            interval: match self {
                Self::InvalidRange => Interval::M1,
                _ => Interval::S1,
            },
            limit: None,
            from_ms: Some(200.),
            to_ms: Some(100.),
        }
    }
    fn error(self) -> VenueError {
        match self {
            Self::UnknownMarket => VenueError::new("unknown market MISSING"),
            Self::UnsupportedInterval => Interval::S1.unsupported("BingX").into(),
            Self::InvalidRange => VenueError::new("invalid candle time range"),
        }
    }
}
fn market() -> Market {
    Market {
        id: "BTC-USDT".into(),
        symbol: "BTCUSDT".into(),
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
                max: 10.,
            },
            leverage: MinMax { min: 1., max: 100. },
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
fn engine(private: bool) -> Engine<Bingx> {
    let mut engine = support::engine(support::Options {
        public_data_only: !private,
        key: private.then(|| "key".into()),
        secret: private.then(|| "secret".into()),
        ..Default::default()
    });
    engine.store_mut().set_markets(vec![market()]);
    engine
}
fn ask(engine: &mut Engine<Bingx>, now: f64, refusal: Refusal) -> Vec<Action> {
    engine.handle(now, Event::Command(refusal.command(COMMAND)));
    safe_cex_engine::test_support::prepared_actions(engine, now)
}
fn failure(actions: &[Action], command: u32) -> &VenueError {
    let terminals: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(Err(error))
            }
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if *id == command => Some(Ok(result)),
            _ => None,
        })
        .collect();
    assert_eq!(terminals.len(), 1, "one terminal: {actions:?}");
    assert!(
        actions.iter().all(|a| matches!(a,
            Action::Notice(Notice::CommandFailed { command: id, .. }) if *id == command)),
        "local refusal must emit only its failure, without transport, signer, timer or auth effects: {actions:?}"
    );
    terminals[0].expect_err("a local refusal must fail")
}
fn native(refusal: Refusal) {
    for private in [false, true] {
        let mut e = engine(private);
        let actions = ask(&mut e, NOW, refusal);
        assert_eq!(failure(&actions, COMMAND), &refusal.error());
        for _ in 0..3 {
            let actions = ask(&mut e, NOW, refusal);
            assert_eq!(failure(&actions, COMMAND), &refusal.error());
        }
    }
}
#[test]
fn unknown_market_precedes_interval_and_range_and_keeps_native_error() {
    native(Refusal::UnknownMarket);
}
#[test]
fn unsupported_interval_precedes_range_and_keeps_native_error() {
    native(Refusal::UnsupportedInterval);
}
#[test]
fn invalid_range_keeps_native_error_without_credentials() {
    native(Refusal::InvalidRange);
}
fn invalid_clock(refusal: Refusal) {
    let mut e = engine(false);
    let actions = ask(&mut e, f64::NAN, refusal);
    assert_eq!(
        failure(&actions, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
}
#[test]
fn unknown_market_cannot_bypass_invalid_receipt_clock() {
    invalid_clock(Refusal::UnknownMarket);
}
#[test]
fn unsupported_interval_cannot_bypass_invalid_receipt_clock() {
    invalid_clock(Refusal::UnsupportedInterval);
}
#[test]
fn invalid_range_cannot_bypass_invalid_receipt_clock() {
    invalid_clock(Refusal::InvalidRange);
}
fn fill_capacity(engine: &mut Engine<Bingx>) -> RequestId {
    let mut requests = Vec::new();
    for command in 1_000..1_256 {
        engine.handle(NOW, Event::Command(Command::FetchAccount { command }));
        for action in safe_cex_engine::test_support::prepared_actions(engine, NOW) {
            match action {
                Action::Http(request) => {
                    assert!(request.url.contains("/openApi/swap/v3/user/balance"));
                    requests.push(request.id);
                }
                Action::Notice(Notice::CommandFailed { error, .. }) => {
                    panic!("capacity setup refused: {error:?}")
                }
                _ => {}
            }
        }
    }
    assert_eq!(requests.len(), 256, "256 real released managed owners");
    requests[0]
}
fn overflow_error(engine: &mut Engine<Bingx>) -> VenueError {
    engine.handle(
        NOW,
        Event::Command(Command::FetchAccount { command: 2_000 }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(engine, NOW);
    let error = failure(&actions, 2_000).clone();
    assert_eq!(error.kind, Some(ErrorKind::Internal));
    assert_eq!(error.outcome, None);
    assert_eq!(
        error,
        safe_cex_engine::request_context::Error::Capacity.read_venue_error()
    );
    error
}
fn complete_original(engine: &mut Engine<Bingx>, request: RequestId) {
    engine.handle(
        NOW + 1.,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":0,"data":[{"asset":"USDT","userId":"original-owner"}]}"#,
            },
        },
    );
    let actions = safe_cex_engine::test_support::prepared_actions(engine, NOW + 1.);
    assert_eq!(
        actions
            .iter()
            .filter(|action| matches!(action,
            Action::Notice(Notice::CommandDone { command: 1_000, result })
            if result == r#"{"userId":"original-owner"}"#))
            .count(),
        1,
        "original owner survives local refusal: {actions:?}"
    );
}
fn saturated(refusal: Refusal) {
    let mut e = engine(true);
    let occupied = fill_capacity(&mut e);
    let expected = overflow_error(&mut e);
    let actions = ask(&mut e, NOW, refusal);
    assert_eq!(failure(&actions, COMMAND), &expected);
    complete_original(&mut e, occupied);
    let actions = ask(&mut e, NOW + 2., refusal);
    assert_eq!(failure(&actions, COMMAND), &refusal.error());
}
#[test]
fn unknown_market_cannot_bypass_full_owner_capacity() {
    saturated(Refusal::UnknownMarket);
}
#[test]
fn unsupported_interval_cannot_bypass_full_owner_capacity() {
    saturated(Refusal::UnsupportedInterval);
}
#[test]
fn invalid_range_cannot_bypass_full_owner_capacity() {
    saturated(Refusal::InvalidRange);
}
#[test]
fn genuine_capacity_refusal_preserves_original_owner_and_releases_a_slot() {
    let mut e = engine(true);
    let occupied = fill_capacity(&mut e);
    overflow_error(&mut e);
    complete_original(&mut e, occupied);
    e.handle(
        NOW + 2.,
        Event::Command(Command::FetchAccount { command: 2_000 }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 2.);
    assert_eq!(
        actions
            .iter()
            .filter(|a| matches!(a, Action::Http(_)))
            .count(),
        1,
        "the released slot is reusable: {actions:?}"
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
}
#[test]
fn duplicate_local_refusal_cannot_replace_a_live_account_owner() {
    let mut e = engine(true);
    e.handle(
        NOW,
        Event::Command(Command::FetchAccount { command: 1_000 }),
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request.id),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1);
    for refusal in [
        Refusal::UnknownMarket,
        Refusal::UnsupportedInterval,
        Refusal::InvalidRange,
    ] {
        e.handle(NOW, Event::Command(refusal.command(1_000)));
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            matches!(actions.as_slice(), [Action::Notice(Notice::Event(event))]
            if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE),
            "duplicate cannot settle the original: {actions:?}"
        );
    }
    complete_original(&mut e, requests[0]);
}

fn private_invalid_clock(refusal: Refusal) {
    let mut e = engine(true);
    let actions = ask(&mut e, f64::NAN, refusal);
    assert_eq!(
        failure(&actions, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
    let actions = ask(&mut e, NOW, refusal);
    assert_eq!(failure(&actions, COMMAND), &refusal.error());
}
#[test]
fn private_unknown_market_refuses_invalid_clock_then_allows_fresh_id_reuse() {
    private_invalid_clock(Refusal::UnknownMarket);
}
#[test]
fn private_unsupported_interval_refuses_invalid_clock_then_allows_fresh_id_reuse() {
    private_invalid_clock(Refusal::UnsupportedInterval);
}
#[test]
fn private_invalid_range_refuses_invalid_clock_then_allows_fresh_id_reuse() {
    private_invalid_clock(Refusal::InvalidRange);
}
#[test]
fn nonadvancing_clock_cannot_fabricate_a_receipt_for_a_local_refusal() {
    let mut e = engine(false);
    let actions = ask(&mut e, f64::MAX, Refusal::UnknownMarket);
    assert_eq!(
        failure(&actions, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
}
