//! Rare identity reads preserve JWT queue behavior without touching financial state.
mod support;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice};
use safe_cex_venue_paradex::Options;
use support::*;
fn options() -> Options {
    Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Default::default()
    }
}
fn seed_with(options: Options) -> Harness {
    let mut h = Harness::empty(options);
    h.reply("/markets", MARKETS);
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h
}
fn loaded() -> Harness {
    let mut h = seed_with(options());
    h.reply("/auth/", r#"{"jwt_token":"jwt-first"}"#);
    h.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":20}]}"#,
    );
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"800","initial_margin_requirement":"200"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":20}]}"#,
    );
    h.actions.clear();
    h
}
fn ask(h: &mut Harness, command: u32) {
    h.command(Command::FetchAccount { command });
}
fn take(h: &mut Harness) -> HttpRequest {
    let i = h
        .pending
        .iter()
        .position(|r| r.url.ends_with("/account"))
        .unwrap_or_else(|| panic!("missing identity request: {:?}", h.pending));
    let r = h.pending.remove(i);
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.url, "https://api.prod.paradex.trade/v1/account");
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    r
}
fn output(h: &mut Harness, id: u32, rejected: bool) -> Result<String, String> {
    result(std::mem::take(&mut h.actions), id, rejected)
}

fn result(actions: Vec<Action>, command_id: u32, rejected: bool) -> Result<String, String> {
    let mut done = Vec::new();
    let mut errors = 0;
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) if command == command_id => {
                let result: Result<String, String> = Ok(result);
                done.push(result)
            }
            Action::Notice(Notice::CommandFailed { command, error }) if command == command_id => {
                let result: Result<String, String> = Err(error.legacy_text());
                done.push(result)
            }
            Action::Notice(Notice::CommandError { command, .. }) if command == command_id => {
                assert!(
                    done.is_empty(),
                    "command diagnostic must precede completion"
                );
                errors += 1;
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity failures must be command-scoped"),
            _ => {}
        }
    }
    assert_eq!(done.len(), 1, "settles exactly once");
    assert_eq!(errors, usize::from(rejected));
    let out = done.remove(0);
    assert_eq!(out.is_err(), rejected);
    out
}

#[test]
fn identity_reuses_current_jwt_and_returns_exact_account_number_without_store_commit() {
    for (id, expected) in [
        ("9007199254740993", "9007199254740993"),
        (r#""900719925474099312345""#, "900719925474099312345"),
        (r#""0""#, "0"),
    ] {
        let mut h = loaded();
        let before = h.e.account_snapshot_frame(h.now).to_vec();
        ask(&mut h, 42);
        let r = take(&mut h);
        assert!(
            r.headers
                .iter()
                .any(|(n, v)| *n == "Authorization" && v == "Bearer jwt-first")
        );
        h.respond(r, 200, &format!(r#"{{"account_number":{id},"account":"not-the-identity","total_collateral":"99999"}}"#));
        assert_eq!(
            output(&mut h, 42, false).unwrap(),
            format!(r#"{{"userId":"{expected}","affiliateId":null}}"#)
        );
        assert_eq!(h.e.account_snapshot_frame(h.now), before);
        assert!(
            h.pending.is_empty(),
            "identity must not start a reconciliation cycle"
        );
    }
}
#[test]
fn identity_waits_for_existing_auth_then_dispatches_with_token_and_same_account_route() {
    let mut h = seed_with(options());
    ask(&mut h, 42);
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.contains("/auth/"))
            .count(),
        1
    );
    h.reply("/auth/", r#"{"jwt_token":"queued-token"}"#);
    let r = take(&mut h);
    assert!(
        r.headers
            .iter()
            .any(|(n, v)| *n == "Authorization" && v == "Bearer queued-token")
    );
    let before = h.e.account_snapshot_frame(h.now).to_vec();
    h.respond(r, 200, r#"{"account_number":"queued-account"}"#);
    assert_eq!(
        output(&mut h, 42, false).unwrap(),
        r#"{"userId":"queued-account","affiliateId":null}"#
    );
    assert_eq!(h.e.account_snapshot_frame(h.now), before);
}
#[test]
fn identity_expired_jwt_joins_single_refresh_and_preserves_authentication_route() {
    let mut h = loaded();
    h.now += 180_001.;
    ask(&mut h, 42);
    ask(&mut h, 43);
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.contains("/auth/"))
            .count(),
        1
    );
    let auth = h.take("/auth/");
    assert_eq!(auth.url, "https://api.prod.paradex.trade/v1/auth/0x456");
    assert_eq!(auth.route, Some(safe_cex_engine::RequestRoute::Direct));
    assert!(
        auth.headers
            .iter()
            .any(|(n, v)| *n == "PARADEX-STARKNET-ACCOUNT" && v == "0x0123")
    );
    h.respond(auth, 200, r#"{"jwt_token":"refreshed"}"#);
    for command in [42, 43] {
        let r = take(&mut h);
        assert!(
            r.headers
                .iter()
                .any(|(n, v)| *n == "Authorization" && v == "Bearer refreshed")
        );
        h.respond(r, 200, r#"{"account_number":"main"}"#);
        output(&mut h, command, false).unwrap();
    }
}
#[test]
fn identity_missing_false_zero_empty_and_failed_response_reject_once_without_store_commit() {
    for (status, body) in [
        (200, "{}"),
        (200, r#"{"account_number":false}"#),
        (200, r#"{"account_number":0}"#),
        (200, r#"{"account_number":" "}"#),
        (200, r#"{"account_number":1.5}"#),
        (200, r#"{"account_number":"main","error":"denied"}"#),
        (403, r#"{"account_number":"main"}"#),
    ] {
        let mut h = loaded();
        let before = h.e.account_snapshot_frame(h.now).to_vec();
        ask(&mut h, 42);
        let r = take(&mut h);
        h.respond(r, status, body);
        output(&mut h, 42, true).unwrap_err();
        assert_eq!(h.e.account_snapshot_frame(h.now), before);
        assert!(h.pending.is_empty());
    }
}
#[test]
fn identity_transport_failure_settles_once_and_keeps_financial_state() {
    let mut h = loaded();
    let before = h.e.account_snapshot_frame(h.now).to_vec();
    ask(&mut h, 42);
    let r = take(&mut h);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: r.id,
            error: "connection lost",
        },
    );
    h.collect();
    output(&mut h, 42, true).unwrap_err();
    assert_eq!(h.e.account_snapshot_frame(h.now), before);
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: r.id,
            error: "duplicate",
        },
    );
    h.collect();
    assert!(h.actions.is_empty());
}
#[test]
fn identity_auth_failure_settles_queued_queries_once_without_identity_http() {
    let mut h = loaded();
    h.now += 180_001.;
    ask(&mut h, 42);
    ask(&mut h, 43);
    let auth = h.take("/auth/");
    h.respond(auth, 403, r#"{"error":"denied"}"#);
    let actions = std::mem::take(&mut h.actions);
    let mut first = vec![];
    let mut second = vec![];
    for action in actions {
        match &action {
            Action::Notice(
                Notice::CommandDone { command, .. }
                | Notice::CommandFailed { command, .. }
                | Notice::CommandError { command, .. },
            ) if *command == 42 => first.push(action),
            Action::Notice(
                Notice::CommandDone { command, .. }
                | Notice::CommandFailed { command, .. }
                | Notice::CommandError { command, .. },
            ) if *command == 43 => second.push(action),
            _ => {}
        }
    }
    result(first, 42, true).unwrap_err();
    result(second, 43, true).unwrap_err();
    assert!(!h.pending.iter().any(|r| r.url.ends_with("/account")));
}
#[test]
fn identity_public_only_or_missing_required_credentials_fail_before_network() {
    for opts in [
        Options {
            public_data_only: true,
            ..options()
        },
        Options::default(),
        Options {
            subkey_address: None,
            ..options()
        },
    ] {
        let mut h = Harness::empty(opts);
        h.pending.clear();
        h.actions.clear();
        ask(&mut h, 42);
        assert!(h.pending.is_empty());
        output(&mut h, 42, true).unwrap_err();
    }
}
#[test]
fn identity_before_native_auth_initialization_fails_visibly_without_polling_or_store_reset() {
    let mut h = Harness::empty(options());
    h.pending.clear();
    h.actions.clear();
    let before = h.e.account_snapshot_frame(h.now).to_vec();
    ask(&mut h, 42);
    output(&mut h, 42, true).unwrap_err();
    assert!(h.pending.is_empty());
    assert_eq!(h.e.account_snapshot_frame(h.now), before);
}

#[test]
fn identity_waits_for_delegated_auth_signer_then_uses_its_jwt_without_signing_the_read() {
    let mut h = seed_with(Options {
        key: Some("opaque-public-subkey".into()),
        payload_signer: true,
        ..options()
    });
    ask(&mut h, 42);
    assert!(h.pending.is_empty());
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SignPayload { .. }))
        .unwrap();
    let Action::SignPayload {
        request, payload, ..
    } = h.actions.remove(i)
    else {
        unreachable!()
    };
    assert_eq!(payload.key, "opaque-public-subkey");
    assert_eq!(payload.scheme, "stark");
    h.e.handle(
        h.now,
        Event::PayloadSigned {
            request,
            signature: "actual-auth-signature",
            headers: vec![],
        },
    );
    h.collect();
    let auth = h.take("/auth/");
    assert_eq!(auth.route, Some(safe_cex_engine::RequestRoute::Direct));
    assert!(
        auth.headers
            .iter()
            .any(|(n, v)| *n == "PARADEX-STARKNET-SIGNATURE" && v == "actual-auth-signature")
    );
    h.respond(auth, 200, r#"{"jwt_token":"delegated-jwt"}"#);
    let read = take(&mut h);
    assert!(
        read.headers
            .iter()
            .any(|(n, v)| *n == "Authorization" && v == "Bearer delegated-jwt")
    );
    let before = h.e.account_snapshot_frame(h.now).to_vec();
    h.respond(read, 200, r#"{"account_number":"delegated-account"}"#);
    assert_eq!(
        output(&mut h, 42, false).unwrap(),
        r#"{"userId":"delegated-account","affiliateId":null}"#
    );
    assert_eq!(h.e.account_snapshot_frame(h.now), before);
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::SignPayload { .. }))
    );
}
