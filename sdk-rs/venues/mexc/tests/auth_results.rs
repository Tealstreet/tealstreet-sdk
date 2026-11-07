//! Actual private-read dispatch, credential attribution and typed envelope controls.
mod support;
use safe_cex_engine::{
    AccountReadScope, Action, AuthOutcome, CredentialSlot, Event, HttpRequest, Notice, Response,
};
use support::Options;
const MARKETS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","baseCoin":"BTC","quoteCoin":"USDT","settleCoin":"USDT","state":0,"apiAllowed":true,"contractSize":0.0001,"volUnit":1,"priceUnit":0.1,"minVol":1,"maxVol":400000,"minLeverage":1,"maxLeverage":500,"makerFeeRate":0,"takerFeeRate":0.0002}]}"#;
const TICKERS: &str = r#"{"success":true,"code":0,"data":[{"symbol":"BTC_USDT","lastPrice":60000,"fairPrice":60001,"indexPrice":59999,"bid1":59999.9,"ask1":60000,"riseFallRate":0.01,"holdVol":10000,"fundingRate":0.0001,"volume24":50000,"amount24":300000}]}"#;
fn drain(e: &mut support::TestEngine) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, 1000.)
}
fn http(actions: &[Action], path: &str) -> HttpRequest {
    actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {path}"))
}
fn reply(e: &mut support::TestEngine, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        1000.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    drain(e)
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
fn started(read: bool) -> (support::TestEngine, Vec<Action>) {
    let mut e = support::configured(Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        read_credentials: read.then(|| safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: Some("read-pass".into()),
        }),
        ..Options::default()
    });
    e.start(1000.);
    let a = drain(&mut e);
    let r = http(&a, "contract/detail");
    let a = reply(&mut e, &r, 200, MARKETS);
    assert!(auth(&a).is_empty());
    // The position mode read leaves with the ticker seed.
    let ticker = http(&a, "contract/ticker");
    let mode = http(&a, "position_mode");
    let a = reply(&mut e, &ticker, 200, TICKERS);
    assert!(auth(&a).is_empty());
    let a = reply(&mut e, &mode, 200, r#"{"success":true,"code":0,"data":2}"#);
    assert_eq!(
        auth(&a),
        vec![(
            if read {
                CredentialSlot::Secondary
            } else {
                CredentialSlot::Primary
            },
            AccountReadScope::Settings,
            AuthOutcome::Accepted
        )]
    );
    (e, a)
}
#[test]
fn actual_private_read_reports_selected_signer_once_and_public_reads_stay_silent() {
    for read in [false, true] {
        let (mut e, a) = started(read);
        assert!(
            auth(&a).is_empty()
                || matches!(
                    auth(&a).as_slice(),
                    [(_, AccountReadScope::Settings, AuthOutcome::Accepted)]
                )
        );
        let r = http(&a, "account/assets");
        assert!(
            r.headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("ApiKey")
                    && v == if read { "read-key" } else { "primary-key" })
        );
        let a = reply(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#);
        assert_eq!(
            auth(&a),
            vec![(
                if read {
                    CredentialSlot::Secondary
                } else {
                    CredentialSlot::Primary
                },
                AccountReadScope::Balance,
                AuthOutcome::Accepted
            )]
        );
        assert!(
            auth(&reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"code":0,"data":[]}"#
            ))
            .is_empty()
        );
    }
}
#[test]
fn typed_auth_rejection_on_missing_stream_list_is_observed_but_business_rate_and_malformed_are_not()
{
    for code in [401, 402, 406, 602, 701, 703] {
        let (mut e, a) = started(true);
        let r = http(&a, "account/assets");
        let body = format!(r#"{{"success":false,"code":{code}}}"#);
        assert_eq!(
            auth(&reply(&mut e, &r, 200, &body)),
            vec![(
                CredentialSlot::Secondary,
                AccountReadScope::Balance,
                AuthOutcome::Rejected
            )]
        );
    }
    for (status, body) in [
        (200, r#"{"success":false,"code":513,"message":"602"}"#),
        (429, r#"{"success":false,"code":510}"#),
        (200, r#"{"success":true,"code":0}"#),
        (401, r#"<html>unauthorized</html>"#),
    ] {
        let (mut e, a) = started(true);
        let r = http(&a, "account/assets");
        assert!(
            auth(&reply(&mut e, &r, status, body)).is_empty(),
            "{status} {body}"
        );
    }
}

#[test]
fn a_rejected_order_endpoint_is_not_reset_by_its_sibling_order_reads() {
    let (mut e, a) = started(false);
    let ok = r#"{"success":true,"code":0,"data":[]}"#;
    let refused = r#"{"success":false,"code":701,"message":"Access denied"}"#;
    let orders = |a: &[Action]| -> Vec<AuthOutcome> {
        auth(a)
            .into_iter()
            .filter(|(_, scope, _)| *scope == AccountReadScope::Orders)
            .map(|(_, _, outcome)| outcome)
            .collect()
    };
    let regular = http(&a, "order/list/open_orders");
    let plan = http(&a, "planorder/list/orders");
    let stop = http(&a, "stoporder/list/orders");
    assert_eq!(
        orders(&reply(&mut e, &regular, 200, ok)),
        [AuthOutcome::Accepted]
    );
    assert_eq!(
        orders(&reply(&mut e, &plan, 200, refused)),
        [AuthOutcome::Rejected]
    );
    // Plan orders still refuse the key, so stop orders' success is no evidence.
    assert!(orders(&reply(&mut e, &stop, 200, ok)).is_empty());
}

#[test]
fn row_failures_do_not_withhold_successful_envelope_evidence() {
    for read in [false, true] {
        for (path, scope) in [
            ("account/assets", AccountReadScope::Balance),
            ("position/open_positions", AccountReadScope::Positions),
            ("order/list/open_orders", AccountReadScope::Orders),
            ("planorder/list/orders", AccountReadScope::Orders),
            ("stoporder/list/orders", AccountReadScope::Orders),
        ] {
            let (mut e, all) = started(read);
            let r = http(&all, path);
            let a = reply(
                &mut e,
                &r,
                200,
                r#"{"success":true,"code":0,"data":[false]}"#,
            );
            assert_eq!(
                auth(&a),
                [(
                    if read {
                        CredentialSlot::Secondary
                    } else {
                        CredentialSlot::Primary
                    },
                    scope,
                    AuthOutcome::Accepted
                )],
                "{path}"
            );
        }
    }
}

#[test]
fn accepted_credentials_still_commit_good_asset_peers_and_report_bad_rows() {
    let (mut e, all) = started(true);
    let r = http(&all, "account/assets");
    let a = reply(
        &mut e,
        &r,
        200,
        r#"{"success":true,"code":0,"data":[false,{"currency":"USDT","availableBalance":40,"equity":42}]}"#,
    );
    assert_eq!(
        auth(&a),
        [(
            CredentialSlot::Secondary,
            AccountReadScope::Balance,
            AuthOutcome::Accepted
        )]
    );
    assert_eq!(e.store().balance().total, 42.0);
    assert!(a.iter().any(|a| matches!(a, Action::Notice(Notice::Event(e)) if e.code == safe_cex_engine::log::code::ROW_SKIPPED)));
}

#[test]
fn independent_depth_queries_never_emit_auth_evidence_with_private_read_credentials() {
    use safe_cex_engine::{Command, ErrorKind};
    for read in [false, true] {
        for (status, body) in [
            (
                200,
                r#"{"success":true,"code":0,"data":{"version":1,"bids":[],"asks":[]}}"#,
            ),
            (
                401,
                r#"{"success":false,"code":602,"message":"invalid api key"}"#,
            ),
            (
                403,
                r#"{"success":false,"code":701,"message":"permission denied"}"#,
            ),
            (
                200,
                r#"{"success":false,"code":602,"message":"invalid api key"}"#,
            ),
        ] {
            let (mut e, _) = started(read);
            let failures = e.health().auth_failures;
            e.handle(
                1000.,
                Event::Command(Command::FetchOrderBook {
                    command: 91,
                    symbol: "BTCUSDT".into(),
                    limit: Some(1),
                }),
            );
            let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.);
            let query = http(&actions, "contract/depth");
            assert!(auth(&actions).is_empty());
            assert!(query.headers.iter().all(|(name, _)| !matches!(
                name.to_ascii_lowercase().as_str(),
                "apikey" | "signature" | "request-time"
            )));
            let actions = reply(&mut e, &query, status, body);
            assert!(
                auth(&actions).is_empty(),
                "read={read} status={status} body={body}"
            );
            assert_eq!(e.health().auth_failures, failures);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(
                        a,
                        Action::Notice(
                            Notice::CommandDone { command: 91, .. }
                                | Notice::CommandFailed { command: 91, .. }
                        )
                    ))
                    .count(),
                1
            );
            if status == 200 && body.contains("\"success\":true") {
                assert!(
                    actions.iter().any(|a| matches!(
                        a,
                        Action::Notice(Notice::CommandDone { command: 91, .. })
                    ))
                );
            }
            if body.contains("invalid api key") || status == 403 {
                assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 91, error }) if error.kind == Some(ErrorKind::VenueUnavailable))));
            }
        }
    }
}

fn assert_refusal(actions: &[Action], kind: safe_cex_engine::ErrorKind, code: i32) {
    let event = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::Event(event))
                if event.code == safe_cex_engine::log::code::AUTH_REJECTED =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("typed key refusal instead of missing-list error");
    assert!(event.fields.contains(&("kind", kind.as_str().into())));
    assert!(
        event
            .fields
            .contains(&("venueCode", code.to_string().into()))
    );
    assert!(
        event
            .fields
            .contains(&("venueMessage", "Confirming signature failed".into()))
    );
    assert!(!event.message.contains("expected list"));
    assert!(event.message.contains("Confirming signature failed"));
    assert!(
        !actions.iter().any(|a| matches!(a, Action::Http(r)
        if r.method != safe_cex_engine::Method::Get || r.url.contains("ping"))),
        "a key refusal grants no write or clock-probe resend"
    );
}

#[test]
fn contract_auth_refusals_keep_typed_fields_on_streamed_account_reads() {
    use safe_cex_engine::ErrorKind::*;
    for read in [false, true] {
        for (code, kind) in [
            (401, AuthInvalid),
            (402, AuthExpired),
            (406, AuthPermission),
            (511, AuthPermission),
            (602, AuthInvalid),
            (701, AuthPermission),
            (702, AuthPermission),
            (703, AuthPermission),
            (704, AuthPermission),
        ] {
            let (mut e, a) = started(read);
            let r = http(&a, "account/assets");
            let body = format!(
                r#"{{"success":false,"code":{code},"message":"Confirming signature failed"}}"#
            );
            let a = reply(&mut e, &r, 200, &body);
            assert_refusal(&a, kind, code);
            assert_eq!(
                auth(&a),
                [(
                    if read {
                        CredentialSlot::Secondary
                    } else {
                        CredentialSlot::Primary
                    },
                    AccountReadScope::Balance,
                    AuthOutcome::Rejected
                )]
            );
            assert!(e.store().balance().total == 0.);
        }
    }
}

#[test]
fn a_602_refusal_reaches_the_original_caller_once_without_resending() {
    let (mut e, _) = started(false);
    e.handle(
        1000.,
        Event::Command(safe_cex_engine::Command::FetchAccount { command: 902 }),
    );
    let a = drain(&mut e);
    let r = http(&a, "account/assets");
    let a = reply(
        &mut e,
        &r,
        200,
        r#"{"success":false,"code":602,"message":"Confirming signature failed"}"#,
    );
    let failed: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 902,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(
        failed[0].kind,
        Some(safe_cex_engine::ErrorKind::AuthInvalid)
    );
    assert_eq!(failed[0].venue_code.as_deref(), Some("602"));
    assert_eq!(
        failed[0].venue_message.as_deref(),
        Some("Confirming signature failed")
    );
    assert!(!failed[0].message.contains("expected list"));
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        reply(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#)
            .iter()
            .all(|a| !matches!(
                a,
                Action::Notice(
                    Notice::CommandDone { command: 902, .. }
                        | Notice::CommandFailed { command: 902, .. }
                )
            ))
    );
}

#[test]
fn clock_window_513_gets_one_unsigned_probe_and_one_exact_selected_get_replay() {
    for read in [false, true] {
        for caller in [false, true] {
            let (mut e, a) = started(read);
            let original = if caller {
                e.handle(
                    1000.,
                    Event::Command(safe_cex_engine::Command::FetchAccount { command: 903 }),
                );
                http(&drain(&mut e), "account/assets")
            } else {
                http(&a, "account/assets")
            };
            let a = reply(
                &mut e,
                &original,
                200,
                r#"{"success":false,"code":513,"message":"Invalid request"}"#,
            );
            let clock = http(&a, "/api/v1/contract/ping");
            assert!(clock.headers.iter().all(|(k, _)| !matches!(
                k.to_lowercase().as_str(),
                "apikey" | "request-time" | "signature"
            )));
            assert!(clock.body.is_none());
            assert!(!a.iter().any(|a| matches!(
                a,
                Action::Notice(
                    Notice::CommandFailed { command: 903, .. }
                        | Notice::CommandDone { command: 903, .. }
                )
            )));
            let a = reply(
                &mut e,
                &clock,
                200,
                r#"{"success":true,"code":0,"data":301000}"#,
            );
            let replay = http(&a, "account/assets");
            assert_ne!(original.id, replay.id);
            assert_eq!(original.url, replay.url);
            assert_eq!(original.method, replay.method);
            assert_eq!(original.body, replay.body);
            for header in ["ApiKey", "Recv-Window"] {
                assert_eq!(
                    original.headers.iter().find(|(k, _)| *k == header),
                    replay.headers.iter().find(|(k, _)| *k == header)
                );
            }
            assert!(
                replay
                    .headers
                    .iter()
                    .any(|(k, v)| *k == "Request-Time" && v == "301000")
            );
            let a = reply(
                &mut e,
                &replay,
                200,
                r#"{"success":false,"code":513,"message":"Invalid request"}"#,
            );
            assert!(
                !a.iter().any(|a| matches!(a, Action::Http(_))),
                "second 513 cannot loop"
            );
            if caller {
                let failures: Vec<_> = a
                    .iter()
                    .filter_map(|a| match a {
                        Action::Notice(Notice::CommandFailed {
                            command: 903,
                            error,
                        }) => Some(error),
                        _ => None,
                    })
                    .collect();
                assert_eq!(failures.len(), 1);
                assert_eq!(failures[0].venue_code.as_deref(), Some("513"));
                assert_eq!(
                    failures[0].kind,
                    Some(safe_cex_engine::ErrorKind::InvalidInput)
                );
            }
        }
    }
}

#[test]
fn history_clock_refusal_replays_the_same_page_and_finishes_once() {
    let (mut e, _) = started(false);
    e.handle(
        1000.,
        Event::Command(safe_cex_engine::Command::Query {
            command: 904,
            query: safe_cex_engine::Query::Fills {
                symbol: Some("BTCUSDT".into()),
                order_id: None,
                from_ms: None,
                to_ms: None,
                limit: Some(2),
            },
        }),
    );
    let original = http(&drain(&mut e), "order_deals");
    let a = reply(
        &mut e,
        &original,
        200,
        r#"{"success":false,"code":513,"message":"Invalid request"}"#,
    );
    let clock = http(&a, "/contract/ping");
    let a = reply(
        &mut e,
        &clock,
        200,
        r#"{"success":true,"code":0,"data":999000}"#,
    );
    let replay = http(&a, "order_deals");
    assert_eq!(replay.url, original.url);
    assert!(
        replay
            .headers
            .iter()
            .any(|(k, v)| *k == "Request-Time" && v == "999000")
    );
    let a = reply(
        &mut e,
        &replay,
        200,
        r#"{"success":true,"code":0,"data":[]}"#,
    );
    assert_eq!(
        a.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 904, .. })))
            .count(),
        1
    );
    assert!(!a.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 904, .. })
    )));
}

#[test]
fn malformed_failed_or_expired_clock_samples_never_replay_or_poison_signing() {
    for body in [
        r#"{"success":false,"code":602,"message":"Confirming signature failed"}"#,
        r#"{"success":true,"code":0,"data":0}"#,
        r#"{"success":true,"code":0,"data":-1}"#,
        r#"{"success":true,"code":0,"data":9007199254740992}"#,
        r#"{"success":true,"code":0,"data":999000.5}"#,
        r#"{"success":true,"code":0,"data":"999000"}"#,
        r#"{"success":true,"code":0,"data":[]}"#,
        "<html>gateway failure</html>",
    ] {
        let (mut e, _) = started(false);
        e.handle(
            1000.,
            Event::Command(safe_cex_engine::Command::FetchAccount { command: 905 }),
        );
        let original = http(&drain(&mut e), "account/assets");
        let a = reply(&mut e, &original, 200, r#"{"success":false,"code":513}"#);
        let clock = http(&a, "/contract/ping");
        let a = reply(&mut e, &clock, 200, body);
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))), "{body}");
        assert_eq!(
            a.iter()
                .filter(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 905, .. })
                ))
                .count(),
            1
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 905, .. })))
        );
        e.handle(
            1000.,
            Event::Command(safe_cex_engine::Command::FetchAccount { command: 906 }),
        );
        let fresh = http(&drain(&mut e), "account/assets");
        assert!(
            fresh
                .headers
                .iter()
                .any(|(k, v)| *k == "Request-Time" && v == "1000")
        );
    }
    let (mut e, _) = started(false);
    e.handle(
        1000.,
        Event::Command(safe_cex_engine::Command::FetchAccount { command: 907 }),
    );
    let original = http(&drain(&mut e), "account/assets");
    let a = reply(&mut e, &original, 200, r#"{"success":false,"code":513}"#);
    let clock = http(&a, "/contract/ping");
    e.handle(
        7000.,
        Event::HttpResponse {
            request: clock.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"success":true,"code":0,"data":999000}"#,
            },
        },
    );
    let a = support::prepared(&mut e, 7000.);
    assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(
        a.iter()
            .filter(|a| matches!(
                a,
                Action::Notice(Notice::CommandFailed { command: 907, .. })
            ))
            .count(),
        1
    );
}

#[test]
fn clock_transport_failure_stays_network_and_a_failed_replay_stays_auth() {
    for network in [false, true] {
        let (mut e, _) = started(false);
        e.handle(
            1000.,
            Event::Command(safe_cex_engine::Command::FetchAccount { command: 908 }),
        );
        let original = http(&drain(&mut e), "account/assets");
        let a = reply(&mut e, &original, 200, r#"{"success":false,"code":513}"#);
        let clock = http(&a, "/contract/ping");
        let a = if network {
            e.handle(
                1000.,
                Event::HttpFailed {
                    request: clock.id,
                    error: "connection reset",
                },
            );
            drain(&mut e)
        } else {
            let a = reply(
                &mut e,
                &clock,
                200,
                r#"{"success":true,"code":0,"data":301000}"#,
            );
            let replay = http(&a, "account/assets");
            reply(
                &mut e,
                &replay,
                200,
                r#"{"success":false,"code":602,"message":"Confirming signature failed"}"#,
            )
        };
        let errors: Vec<_> = a
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed {
                    command: 908,
                    error,
                }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].kind,
            Some(if network {
                safe_cex_engine::ErrorKind::Network
            } else {
                safe_cex_engine::ErrorKind::AuthInvalid
            })
        );
        assert_eq!(
            errors[0].venue_code.as_deref(),
            if network { None } else { Some("602") }
        );
        assert!(!a.iter().any(|a| matches!(a, Action::Http(_))));
    }
}

#[test]
fn req_time_after_hidden_sleep_is_signed_at_new_dispatch_time() {
    let (mut e, _) = started(false);
    e.handle(
        301000.875,
        Event::Command(safe_cex_engine::Command::FetchAccount { command: 909 }),
    );
    let request = http(&support::prepared(&mut e, 301000.875), "account/assets");
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "Request-Time" && v == "301000")
    );
    let signature = safe_cex_crypto::hmac_sha256(
        b"primary-secret",
        b"primary-key301000",
        safe_cex_crypto::Encoding::Hex,
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| *k == "Signature" && v == &signature)
    );
}

#[test]
fn a_success_or_unproven_envelope_cannot_become_a_key_refusal_or_clock_grant() {
    for body in [
        r#"{"success":true,"code":602,"message":"Confirming signature failed"}"#,
        r#"{"code":602,"message":"Confirming signature failed"}"#,
        r#"{"success":false,"code":602.5,"message":"Confirming signature failed"}"#,
        r#"{"success":true,"code":513}"#,
        r#"{"success":false,"code":513,"data":[]}"#,
    ] {
        let (mut e, a) = started(false);
        let original = http(&a, "account/assets");
        let a = reply(&mut e, &original, 200, body);
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.contains("/contract/ping")))
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::Event(event))
            if event.code==safe_cex_engine::log::code::AUTH_REJECTED))
        );
    }
}

#[test]
fn native_auth_detail_is_bounded_and_never_includes_refusal_data_rows() {
    let (mut e, _) = started(false);
    e.handle(
        1000.,
        Event::Command(safe_cex_engine::Command::FetchAccount { command: 910 }),
    );
    let request = http(&drain(&mut e), "account/assets");
    let message = "密".repeat(500);
    let body = format!(
        r#"{{"success":false,"code":602,"message":"{message}","data":[{{"private":"do-not-publish"}}]}}"#
    );
    let a = reply(&mut e, &request, 200, &body);
    let error = a
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 910,
                error,
            }) => Some(error),
            _ => None,
        })
        .expect("original typed terminal");
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::AuthInvalid));
    assert_eq!(error.venue_message.as_ref().unwrap().chars().count(), 128);
    assert!(!error.message.contains("do-not-publish"));
    assert!(e.store().orders().is_empty());
}
