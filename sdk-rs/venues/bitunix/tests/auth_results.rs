//! Current signed account evidence and bounded private-read recovery.
mod support;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Event, HttpRequest, Notice, Response,
};
use safe_cex_venue_bitunix::{BitunixFutures, Options};
use support::*;
fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn answer(
    e: &mut TestEngine,
    r: &HttpRequest,
    now: f64,
    status: u16,
    headers: &str,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    e.drain_actions().collect()
}
fn auth(actions: &[Action]) -> Vec<(CredentialSlot, AccountReadScope, AuthOutcome)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::AccountAuth {
                credential,
                scope,
                outcome,
            }) => Some((*credential, *scope, *outcome)),
            _ => None,
        })
        .collect()
}
fn started(secondary: bool) -> (TestEngine, Vec<Action>) {
    let mut e = safe_cex_engine::test_support::seeded_engine(BitunixFutures::new(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: secondary.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    }));
    e.start(0.0);
    let actions: Vec<_> = e.drain_actions().collect();
    let markets = http(&actions, "/trading_pairs");
    let actions = answer(&mut e, &markets, 0.0, 200, "", PAIRS);
    assert!(
        auth(&actions).is_empty(),
        "public catalogue is unsigned auth evidence"
    );
    (e, actions)
}
#[test]
fn actual_account_requests_report_selected_signer_and_valid_required_shapes_once() {
    for secondary in [false, true] {
        let slot = if secondary {
            CredentialSlot::Secondary
        } else {
            CredentialSlot::Primary
        };
        let (mut e, actions) = started(secondary);
        for (path, body, scope) in [
            ("/futures/account?", ACCOUNT, AccountReadScope::Balance),
            (
                "get_pending_positions",
                POSITIONS,
                AccountReadScope::Positions,
            ),
            (
                "/trade/get_pending_orders",
                r#"{"code":0,"data":{"orderList":[]}}"#,
                AccountReadScope::Orders,
            ),
        ] {
            let r = http(&actions, path);
            assert!(
                r.headers.iter().any(|(k, v)| *k == "api-key"
                    && v == if secondary { "read-key" } else { "primary-key" })
            );
            let result = answer(&mut e, &r, 100.0, 200, "", body);
            assert_eq!(auth(&result), vec![(slot, scope, AuthOutcome::Accepted)]);
            assert!(
                auth(&answer(&mut e, &r, 101.0, 200, "", body)).is_empty(),
                "duplicate callback has no evidence"
            );
        }
    }
}
#[test]
fn only_strict_documented_native_refusals_prove_account_credential_rejection() {
    for native in ["403", "10003", "10004", "\"403\"", "\"10003\"", "\"10004\""] {
        let (mut e, actions) = started(true);
        let r = http(&actions, "get_pending_positions");
        let body = format!(r#"{{"code":{native},"msg":"refused"}}"#);
        assert_eq!(
            auth(&answer(&mut e, &r, 100.0, 200, "", &body)),
            vec![(
                CredentialSlot::Secondary,
                AccountReadScope::Positions,
                AuthOutcome::Rejected
            )],
            "{native}"
        );
    }
}
#[test]
fn ambiguous_codes_statuses_and_malformed_success_shapes_remain_auth_inert() {
    for (status, body) in [
        (401, r#"{"code":10003}"#),
        (403, r#"{"code":10004}"#),
        (500, r#"{"code":403}"#),
        (200, r#"{"code":10007}"#),
        (200, r#"{"code":10010}"#),
        (200, r#"{"code":10011}"#),
        (200, r#"{"code":403.1}"#),
        (200, r#"{"code":"403.0"}"#),
        (200, r#"{"code":null}"#),
        (200, r#"{"code":true}"#),
        (200, r#"{"msg":"invalid api key"}"#),
        (200, r#"{"code":0,"data":null}"#),
        (200, r#"{"code":0,"data":{}}"#),
        (200, r#"{"code":0,"data":[]"#),
    ] {
        let (mut e, actions) = started(true);
        let r = http(&actions, "get_pending_positions");
        assert!(
            auth(&answer(&mut e, &r, 100.0, status, "", body)).is_empty(),
            "{status} {body}"
        );
    }
}
#[test]
fn private_missing_list_diagnostics_never_copy_the_raw_account_body() {
    let (mut e, actions) = started(false);
    let r = http(&actions, "get_pending_positions");
    let result = answer(
        &mut e,
        &r,
        100.0,
        200,
        "",
        r#"{"code":10004,"msg":"IP whitelist refused","privateAccount":"DO-NOT-LOG-ACCOUNT-PAYLOAD"}"#,
    );
    for action in result {
        if let Action::Notice(Notice::Event(event)) = action {
            assert!(!event.message.contains("DO-NOT-LOG-ACCOUNT-PAYLOAD"));
        }
    }
}
#[test]
fn a_private_rate_pause_cannot_be_bypassed_by_the_balance_tick() {
    let (mut e, actions) = started(false);
    let r = http(&actions, "/futures/account?");
    let mut pending = answer(
        &mut e,
        &r,
        100.0,
        429,
        "Retry-After: 1\r\n",
        r#"{"code":10006,"msg":"too frequent"}"#,
    );
    let mut timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 500.0 => Some(*timer),
            _ => None,
        })
        .expect("balance tick");
    for now in [500.0, 1000.0, 1500.0, 2000.0] {
        e.handle(now, Event::TimerFired { timer });
        let current: Vec<_> = e.drain_actions().collect();
        assert!(
            !current
                .iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.contains("/futures/account?"))),
            "tick bypassed rate pause at {now}"
        );
        timer = current
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + 500.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
        pending.extend(current);
    }
    assert!(
        pending
            .iter()
            .any(|a| matches!(a,Action::SetTimer{at_ms,..}if *at_ms>=60100.0)),
        "rate pause must floor short Retry-After at60s"
    );
}

fn retry(actions: &[Action], now: f64) -> (safe_cex_engine::TimerId, f64) {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms }
                if *at_ms >= now + 750.0 && *at_ms <= now + 1000.0 =>
            {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("actual jittered retry")
}
#[test]
fn secondary_refusal_then_current_recovery_has_only_its_scoped_evidence() {
    let (mut e, actions) = started(true);
    let r = http(&actions, "get_pending_positions");
    let failed = answer(
        &mut e,
        &r,
        100.0,
        200,
        "",
        r#"{"code":10004,"msg":"IP refused"}"#,
    );
    assert_eq!(
        auth(&failed),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Positions,
            AuthOutcome::Rejected
        )]
    );
    assert!(!failed.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::AUTH_REJECTED)),"legacy auth diagnostic would invent primary/account failure in SDK");
    let (timer, at) = retry(&failed, 100.0);
    e.handle(at, Event::TimerFired { timer });
    let current: Vec<_> = e.drain_actions().collect();
    let r = http(&current, "get_pending_positions");
    let recovered = answer(&mut e, &r, at + 1.0, 200, "", POSITIONS);
    assert_eq!(
        auth(&recovered),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Positions,
            AuthOutcome::Accepted
        )]
    );
    assert!(!recovered.iter().any(|a|matches!(a,Action::Notice(Notice::Event(event))if event.code==safe_cex_engine::log::code::AUTH_REJECTED)));
}
fn settings(e: &mut TestEngine, now: f64, symbol: &str) -> HttpRequest {
    e.handle(
        now,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: symbol.into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    http(&actions, "/get_leverage_margin_mode")
}
#[test]
fn settings_symbols_do_not_heal_each_other_and_empty_margin_mode_preserves_existing_fallback() {
    let (mut e, _) = started(true);
    let mut markets = e.store().markets().to_vec();
    let mut eth = markets[0].clone();
    eth.symbol = "ETHUSDT".into();
    eth.id = "ETH/USDT".into();
    eth.base = "ETH".into();
    markets.push(eth);
    e.store_mut().set_markets(markets);
    let btc = settings(&mut e, 100.0, "BTCUSDT");
    let failed = answer(
        &mut e,
        &btc,
        200.0,
        200,
        "",
        r#"{"code":403,"msg":"not permitted"}"#,
    );
    assert_eq!(
        auth(&failed),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Settings,
            AuthOutcome::Rejected
        )]
    );
    let eth = settings(&mut e, 300.0, "ETHUSDT");
    assert!(
        auth(&answer(
            &mut e,
            &eth,
            400.0,
            200,
            "",
            r#"{"code":0,"data":{"leverage":30,"marginMode":"CROSS"}}"#
        ))
        .is_empty()
    );
    let (timer, at) = retry(&failed, 200.0);
    e.handle(at, Event::TimerFired { timer });
    let current: Vec<_> = e.drain_actions().collect();
    let btc = http(&current, "/get_leverage_margin_mode");
    let recovered = answer(
        &mut e,
        &btc,
        at + 1.0,
        200,
        "",
        r#"{"code":0,"data":{"leverage":20,"marginMode":""}}"#,
    );
    assert_eq!(
        auth(&recovered),
        vec![(
            CredentialSlot::Secondary,
            AccountReadScope::Settings,
            AuthOutcome::Accepted
        )]
    );
    assert_eq!(
        e.store().setting("BTCUSDT").unwrap().margin_type,
        Some(safe_cex_engine::model::MarginType::Cross)
    );
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
    e.handle(
        at + 2.0,
        Event::Command(safe_cex_engine::Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::Http(r)if r.url.contains("/get_leverage_margin_mode")))
    );
}
