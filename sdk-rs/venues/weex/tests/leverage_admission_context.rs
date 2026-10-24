//! Initial leverage Read admission must preserve the shared refusal cause.
mod support;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
    VenueError,
};
use safe_cex_venue_weex::WeexFutures;
use support::setup::{self, Options};

const NOW: f64 = 1_000.;
const COMMAND: u32 = 42;
const CONFIG: &str = r#"[{"symbol":"BTCUSDT","marginType":"ISOLATED","separatedType":"SEPARATED","crossLeverage":"20","isolatedLongLeverage":"7","isolatedShortLeverage":"9"}]"#;
fn primary() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        stable_id: Some("original-owner".into()),
        ..Default::default()
    }
}
fn read() -> ReadCredentials {
    ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: Some("read-pass".into()),
    }
}
fn seed(e: &mut Engine<WeexFutures>) {
    e.store_mut().set_markets(vec![Market {
        id: "BTCUSDT".into(),
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
                max: 100.,
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
    }]);
    e.store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 13.);
}
fn engine(options: Options) -> Engine<WeexFutures> {
    let mut e = setup::engine(options);
    seed(&mut e);
    e
}
fn command(command: u32) -> Command {
    Command::FetchLeverage {
        command,
        symbol: "BTCUSDT".into(),
    }
}
fn ask(e: &mut Engine<WeexFutures>, now: f64, command: u32) -> Vec<Action> {
    e.handle(now, Event::Command(self::command(command)));
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn terminal_error(actions: &[Action], owner: u32) -> &VenueError {
    let errors: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command, error }) if *command == owner => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(errors.len(), 1, "one correlated failure: {actions:?}");
    assert!(!actions.iter().any(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command, .. }) if *command == owner)));
    errors[0]
}
fn entry_error(actions: &[Action], owner: u32) -> &VenueError {
    let error = terminal_error(actions, owner);
    assert!(
        matches!(actions, [Action::Notice(Notice::CommandFailed { command, .. })] if *command == owner),
        "initial refusal has no transport, timer, signing or auth effects: {actions:?}"
    );
    error
}
fn request(actions: &[Action]) -> &HttpRequest {
    let requests: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 1, "one native request: {actions:?}");
    requests[0]
}
fn reply(
    e: &mut Engine<WeexFutures>,
    now: f64,
    id: safe_cex_engine::RequestId,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, now)
}
fn accepted(e: &mut Engine<WeexFutures>, now: f64, id: safe_cex_engine::RequestId, owner: u32) {
    let actions = reply(e, now, id, CONFIG);
    assert_eq!(actions.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command, result }) if *command == owner && result == r#"{"buyLeverage":7.0,"sellLeverage":9.0,"marginType":"isolated"}"#)).count(), 1);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    let s = e.store().setting("BTCUSDT").unwrap();
    assert_eq!(s.buy_leverage, Some(7.));
    assert_eq!(s.sell_leverage, Some(9.));
    assert_eq!(
        s.margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
}
fn fill(e: &mut Engine<WeexFutures>) -> safe_cex_engine::RequestId {
    let mut ids = Vec::new();
    for command in 1_000..1_256 {
        e.handle(NOW, Event::Command(Command::FetchAccount { command }));
        let a = safe_cex_engine::test_support::prepared_actions(e, NOW);
        let r = request(&a);
        assert_eq!(r.method, Method::Get);
        assert!(r.url.ends_with("/capi/v3/account/accountConfig"));
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| *k == "ACCESS-KEY" && v == "primary-key")
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
        );
        ids.push(r.id);
    }
    assert_eq!(ids.len(), 256, "real managed native identity owners");
    ids[0]
}
fn overflow(e: &mut Engine<WeexFutures>) -> VenueError {
    e.handle(
        NOW,
        Event::Command(Command::FetchAccount { command: 2_000 }),
    );
    let a = safe_cex_engine::test_support::prepared_actions(e, NOW);
    let error = entry_error(&a, 2_000).clone();
    assert_eq!(
        error,
        safe_cex_engine::request_context::Error::Capacity.read_venue_error()
    );
    error
}
fn finish_identity(e: &mut Engine<WeexFutures>, id: safe_cex_engine::RequestId) {
    let a = reply(
        e,
        NOW + 1.,
        id,
        r#"{"canTrade":true,"dualSidePosition":true}"#,
    );
    assert_eq!(a.iter().filter(|a| matches!(a,
        Action::Notice(Notice::CommandDone { command: 1_000, result }) if result == r#"{"userId":"original-owner"}"#)).count(), 1);
}
#[test]
fn leverage_capacity_refusal_preserves_the_shared_read_cause() {
    let mut e = engine(primary());
    let first = fill(&mut e);
    let expected = overflow(&mut e);
    let before = e.store().setting("BTCUSDT");
    let a = ask(&mut e, NOW, COMMAND);
    assert_eq!(entry_error(&a, COMMAND), &expected);
    assert_eq!(e.store().setting("BTCUSDT"), before);
    finish_identity(&mut e, first);
    let a = ask(&mut e, NOW + 2., COMMAND);
    let id = request(&a).id;
    accepted(&mut e, NOW + 3., id, COMMAND);
}
fn invalid_clock(now: f64) {
    let mut e = engine(primary());
    let before = e.store().setting("BTCUSDT");
    let a = ask(&mut e, now, COMMAND);
    assert_eq!(
        entry_error(&a, COMMAND),
        &safe_cex_engine::request_context::Error::Revoked.read_venue_error()
    );
    assert_eq!(e.store().setting("BTCUSDT"), before);
    // MAX cannot be followed by an advancing finite clock on the same engine.
    let mut e = if now == f64::MAX {
        engine(primary())
    } else {
        e
    };
    let a = ask(&mut e, NOW, COMMAND);
    let id = request(&a).id;
    accepted(&mut e, NOW + 1., id, COMMAND);
}
#[test]
fn leverage_nan_receipt_refusal_keeps_the_shared_read_cause() {
    invalid_clock(f64::NAN);
}
#[test]
fn leverage_nonadvancing_receipt_refusal_keeps_the_shared_read_cause() {
    invalid_clock(f64::MAX);
}
#[test]
fn genuine_capacity_control_preserves_native_identity_and_freed_slot() {
    let mut e = engine(primary());
    let first = fill(&mut e);
    overflow(&mut e);
    finish_identity(&mut e, first);
    let a = ask(&mut e, NOW + 2., COMMAND);
    let id = request(&a).id;
    accepted(&mut e, NOW + 3., id, COMMAND);
}
fn signed_positive(options: Options, key: &str, secret: &str, pass: &str) {
    let mut e = engine(options);
    let a = ask(&mut e, NOW, COMMAND);
    let r = request(&a);
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(
        r.url,
        "https://api-contract.weex.com/capi/v3/account/symbolConfig?symbol=BTCUSDT"
    );
    assert_eq!(r.timeout_ms, Some(20_000));
    assert_eq!(
        r.headers,
        setup::oracle(
            key,
            secret,
            pass,
            Method::Get,
            "/capi/v3/account/symbolConfig?symbol=BTCUSDT",
            "",
            "1000"
        )
    );
    let id = r.id;
    accepted(&mut e, NOW + 1., id, COMMAND);
}
#[test]
fn primary_wire_signature_and_settings_are_native() {
    signed_positive(primary(), "primary-key", "primary-secret", "primary-pass");
}
#[test]
fn secondary_wire_signature_and_settings_are_native() {
    signed_positive(
        Options {
            read_credentials: Some(read()),
            ..primary()
        },
        "read-key",
        "read-secret",
        "read-pass",
    );
}
#[test]
fn secondary_only_wire_signature_and_settings_are_native() {
    signed_positive(
        Options {
            read_credentials: Some(read()),
            ..Default::default()
        },
        "read-key",
        "read-secret",
        "read-pass",
    );
}
#[test]
fn short_host_wait_keeps_the_native_twenty_second_physical_bound() {
    let mut e = engine(primary());
    e.handle_command(NOW, command(COMMAND), Some(1_234))
        .unwrap();
    let a = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    let r = request(&a);
    assert_eq!(r.timeout_ms, Some(20_000));
    let id = r.id;
    accepted(&mut e, NOW + 1., id, COMMAND);
}
#[test]
fn provider_refusal_retains_its_typed_not_sent_fields_without_auth_evidence() {
    let mut e = setup::engine_with_provider(primary(), |inner| {
        std::sync::Arc::new(setup::Gated::new(inner, &[false]))
    });
    seed(&mut e);
    let before = e.store().setting("BTCUSDT");
    let a = ask(&mut e, NOW, COMMAND);
    assert_eq!(
        terminal_error(&a, COMMAND),
        &safe_cex_engine::signing::SignError::Refused.venue_error()
    );
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::Notice(Notice::AccountAuth { .. } | Notice::Deactivate)
    )));
    assert_eq!(e.store().setting("BTCUSDT"), before);
}
#[test]
fn duplicate_native_leverage_owner_is_preserved_and_id_reuse_is_fresh() {
    let mut e = engine(primary());
    let a = ask(&mut e, NOW, COMMAND);
    let id = request(&a).id;
    let duplicate = ask(&mut e, NOW, COMMAND);
    assert!(
        matches!(duplicate.as_slice(), [Action::Notice(Notice::Event(event))]
        if event.code == safe_cex_engine::log::code::COMMAND_DUPLICATE)
    );
    accepted(&mut e, NOW + 1., id, COMMAND);
    let a = ask(&mut e, NOW + 2., COMMAND);
    let next = request(&a).id;
    assert_ne!(id, next);
    accepted(&mut e, NOW + 3., next, COMMAND);
}
