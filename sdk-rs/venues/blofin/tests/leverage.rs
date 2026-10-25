mod support;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

fn flat() -> Harness {
    let mut h = Harness::new();
    h.seed();
    h.ws(r#"{"arg":{"channel":"positions"},"data":[]}"#);
    h.e.store_mut()
        .update_setting("BTCUSDT", |s| s.leverage = 0.0);
    h
}

fn lookup(h: &mut Harness) {
    h.handle(
        1.0,
        Event::Command(Command::FetchLeverage {
            command: 71,
            symbol: "BTCUSDT".into(),
        }),
    );
    // The managed Read releases through Prepare/Bind/Claim.
    h.collect_at(1.0);
}

fn result(h: &Harness) -> Result<serde_json::Value, String> {
    h.actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: 71,
                result,
            }) => Some(Ok(serde_json::from_str(result).unwrap())),
            Action::Notice(Notice::CommandFailed { command: 71, error }) => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .unwrap()
}

#[test]
fn flat_symbol_fetches_signed_actual_leverage_and_modes() {
    let mut h = flat();
    lookup(&mut h);
    let request = h.take("batch-leverage-info");
    assert!(request.url.ends_with("?instId=BTC-USDT&marginMode=cross"));
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "ACCESS-SIGN" && !value.is_empty())
    );
    h.respond(request, r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]}"#);
    assert_eq!(
        result(&h).unwrap(),
        serde_json::json!({
            "buyLeverage":12.0, "sellLeverage":12.0,
            "marginType":"cross", "positionMode":"oneway"
        })
    );
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 12.0);
    assert!(h.e.store().positions().is_empty());
}

#[test]
fn independent_hedge_leverages_are_returned_without_collapsing_sides() {
    let mut h = flat();
    lookup(&mut h);
    h.reply("batch-leverage-info", r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"short","leverage":"7"},{"instId":"BTC-USDT","marginMode":"cross","positionSide":"long","leverage":"13"}]}"#);
    let result = result(&h).unwrap();
    assert_eq!(result["buyLeverage"], 13.0);
    assert_eq!(result["sellLeverage"], 7.0);
    assert_eq!(result["positionMode"], "hedged");
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 13.0);
    assert_eq!(
        h.e.store().setting("BTCUSDT").unwrap().buy_leverage,
        Some(13.0)
    );
    assert_eq!(
        h.e.store().setting("BTCUSDT").unwrap().sell_leverage,
        Some(7.0)
    );
}

#[test]
fn hedge_position_refresh_preserves_other_side_and_flat_settings() {
    let mut h = flat();
    lookup(&mut h);
    h.reply("batch-leverage-info", r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"long","leverage":"13"},{"instId":"BTC-USDT","marginMode":"cross","positionSide":"short","leverage":"7"}]}"#);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USDT","positionSide":"short","marginMode":"cross","positions":"1","averagePrice":"60000","markPrice":"60000"}]}"#);
    assert_eq!(h.e.store().positions()[0].leverage, 7.0);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USDT","positionSide":"long","marginMode":"cross","positions":"1","leverage":"15","averagePrice":"60000","markPrice":"60000"}]}"#);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[]}"#);
    let setting = h.e.store().setting("BTCUSDT").unwrap();
    assert_eq!(setting.buy_leverage, Some(15.0));
    assert_eq!(setting.sell_leverage, Some(7.0));
    assert!(h.e.store().positions().is_empty());
}

#[test]
fn malformed_or_unrelated_rows_preserve_existing_setting() {
    for rows in [
        "[]",
        r#"[{"instId":"ETH-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"isolated","positionSide":"net","leverage":"12"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"unknown","leverage":"12"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"NaN"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"0"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"-5"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"},{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]"#,
        r#"[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"},{"instId":"BTC-USDT","marginMode":"cross","positionSide":"long","leverage":"12"}]"#,
    ] {
        let mut h = flat();
        h.e.store_mut()
            .update_setting("BTCUSDT", |s| s.leverage = 19.0);
        lookup(&mut h);
        h.reply(
            "batch-leverage-info",
            &format!("{{\"code\":\"0\",\"data\":{rows}}}"),
        );
        assert!(result(&h).is_err(), "{rows}");
        assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 19.0);
    }
}

#[test]
fn failed_envelope_or_transport_preserves_setting() {
    let mut h = flat();
    lookup(&mut h);
    h.reply(
        "batch-leverage-info",
        r#"{"code":"152401","msg":"denied","data":[]}"#,
    );
    assert!(result(&h).is_err());
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 0.0);
    lookup(&mut h);
    let request = h.take("batch-leverage-info");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: request.id,
            error: "network failure",
        },
    );
    h.collect();
    assert!(result(&h).is_err());
    assert!(h.pending.is_empty());
}

#[test]
fn stale_lookup_cannot_overwrite_a_confirmed_setting_change() {
    let mut h = flat();
    lookup(&mut h);
    let old = h.take("batch-leverage-info");
    h.handle(
        2.0,
        Event::Command(Command::SetLeverage {
            command: 72,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
            side: None,
        }),
    );
    h.collect();
    h.reply("set-leverage", r#"{"code":"0","data":{"leverage":"30"}}"#);
    h.respond(old, r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]}"#);
    assert!(result(&h).is_err());
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 30.0);
}

#[test]
fn newer_position_push_supersedes_a_pending_lookup() {
    let mut h = flat();
    lookup(&mut h);
    h.ws(r#"{"arg":{"channel":"positions"},"data":[{"instId":"BTC-USDT","positionSide":"net","marginMode":"cross","positions":"1","leverage":"25","averagePrice":"60000","markPrice":"60000"}]}"#);
    h.reply("batch-leverage-info", r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]}"#);
    assert!(result(&h).is_err());
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 25.0);
}

#[test]
fn unknown_symbol_does_not_issue_a_request() {
    let mut h = flat();
    h.handle(
        1.0,
        Event::Command(Command::FetchLeverage {
            command: 71,
            symbol: "UNKNOWN".into(),
        }),
    );
    h.collect();
    assert!(result(&h).is_err());
    assert!(h.pending.is_empty());
}

fn typed_failure(h: &Harness) -> &safe_cex_engine::VenueError {
    assert_eq!(h.failures.len(), 1, "one terminal");
    &h.failures[0]
}

fn late_success_is_inert(h: &mut Harness, request: safe_cex_engine::HttpRequest, now: f64) {
    let before = h.actions.len();
    h.respond_at(request, r#"{"code":"0","data":[{"instId":"BTC-USDT","marginMode":"cross","positionSide":"net","leverage":"12"}]}"#, now);
    assert!(
        !h.actions[before..].iter().any(|a| matches!(
            a,
            Action::Notice(
                Notice::CommandDone { command: 71, .. } | Notice::CommandFailed { command: 71, .. }
            )
        )),
        "a late duplicate settles nothing"
    );
    assert_eq!(h.e.store().setting("BTCUSDT").unwrap().leverage, 0.0);
}

#[test]
fn released_network_failure_keeps_its_typed_kind_and_native_text() {
    let mut h = flat();
    lookup(&mut h);
    let request = h.take("batch-leverage-info");
    h.handle(
        2.0,
        Event::HttpFailed {
            request: request.id,
            error: "network failure",
        },
    );
    h.collect();
    let error = typed_failure(&h);
    assert_eq!(error.message, "network failure");
    assert_eq!(
        (error.kind, error.outcome),
        (Some(safe_cex_engine::ErrorKind::Network), None)
    );
    late_success_is_inert(&mut h, request, 3.0);
}

#[test]
fn exact_read_deadline_is_a_typed_timeout() {
    let mut h = flat();
    lookup(&mut h);
    let request = h.take("batch-leverage-info");
    let timer = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 20_001.0 => Some(*timer),
            _ => None,
        })
        .expect("the Read's 20s deadline");
    h.handle(20_001.0, Event::TimerFired { timer });
    h.collect();
    let error = typed_failure(&h);
    assert_eq!(
        (error.kind, error.outcome),
        (Some(safe_cex_engine::ErrorKind::Timeout), None)
    );
    late_success_is_inert(&mut h, request, 20_002.0);
}

/// Every signed read selects the same identity, so the settings reads refuse
/// with the native text before any work, and leverage never gets that far.
fn refuses_every_signed_read(h: &mut Harness, text: &str) {
    assert!(
        !h.pending.iter().any(|r| r.url.contains("/account/")),
        "no signed request"
    );
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a, Action::Notice(_)) && format!("{a:?}").contains(text)),
        "the settings read refusal is visible with the native text"
    );
    lookup(h);
    assert_eq!(result(h).unwrap_err(), "BloFin margin mode is not loaded");
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::ManagedHttp(_) | Action::Sign { .. }))
    );
}

#[test]
fn a_selected_identity_without_a_passphrase_ref_refuses_natively_before_any_work() {
    // Trusted setup omits the ref wherever native `headers` would refuse.
    let mut h = Harness::with_identities(|ids| ids.primary.as_mut().unwrap().passphrase = None);
    refuses_every_signed_read(&mut h, "BloFin requires a passphrase");
}

#[test]
fn no_selectable_identity_refuses_with_the_native_credentials_text() {
    let mut h = Harness::with_identities(|ids| *ids = Default::default());
    refuses_every_signed_read(&mut h, "BloFin requires API credentials");
}
