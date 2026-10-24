//! Public identity lookup never initializes signer, private sockets or account state.
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Method, Notice, Response};
use safe_cex_venue_lighter::{Lighter, Options};

const NOW: f64 = 1_700_000_000_000.;
fn engine(options: Options) -> Engine<Lighter> {
    let mut e = safe_cex_engine::test_support::seeded_engine(Lighter::new(options));
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 100.,
        used: 23.,
        upnl: 7.,
    });
    e
}
fn ask(e: &mut Engine<Lighter>) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    native_actions(e)
}
fn native_actions(e: &mut Engine<Lighter>) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_actions(e, NOW)
        .into_iter()
        .filter(|action| match action {
            Action::SetTimer { at_ms, .. } => {
                assert!(
                    *at_ms == NOW + 20_000.
                        || *at_ms == NOW + f64::from(safe_cex_engine::DEFAULT_COMMAND_WAIT_MS)
                );
                false
            }
            Action::ClearTimer { .. } => false,
            _ => true,
        })
        .collect()
}
fn take(actions: Vec<Action>) -> HttpRequest {
    let mut iter = actions.into_iter();
    let Some(Action::Http(r)) = iter.next() else {
        panic!("identity requires exactly one HTTP action");
    };
    assert!(iter.next().is_none());
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    r
}
fn respond(e: &mut Engine<Lighter>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    e.handle(
        NOW,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    native_actions(e)
}

fn result(actions: Vec<Action>, rejected: bool) -> Result<String, String> {
    let mut done = vec![];
    let mut errors = 0;
    for action in actions {
        match action {
            Action::Notice(Notice::CommandDone { command, result }) => {
                let result: Result<String, String> = Ok(result);
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandFailed { command, error }) => {
                let result: Result<String, String> = Err(error.legacy_text());
                assert_eq!(command, 42);
                done.push(result);
            }
            Action::Notice(Notice::CommandError { command, .. }) => {
                assert_eq!(command, 42);
                assert!(done.is_empty());
                errors += 1;
            }
            Action::Notice(Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error,
                ..
            })) => panic!("identity error must remain scoped"),
            Action::Http(_)
            | Action::SignPayload { .. }
            | Action::WsConnect { .. }
            | Action::WsSend { .. }
            | Action::SetTimer { .. } => {
                panic!("identity completion must not start another lifecycle")
            }
            _ => {}
        }
    }
    assert_eq!(done.len(), 1);
    assert_eq!(errors, usize::from(rejected));
    let out = done.remove(0);
    assert_eq!(out.is_err(), rejected);
    out
}

fn options() -> Options {
    Options {
        application_id: Some("0".into()),
        ..Default::default()
    }
}
#[test]
fn identity_unsigned_public_only_supports_zero_and_deployment_hosts_without_lifecycle_mutation() {
    for (robinhood, testnet, host) in [
        (false, false, "https://mainnet.zklighter.elliot.ai"),
        (false, true, "https://testnet-api.lighter.xyz"),
        (true, false, "https://api.rh.lighter.xyz"),
        (true, true, "https://api.rh-testnet.lighter.xyz"),
    ] {
        let mut e = engine(Options {
            public_data_only: true,
            robinhood,
            testnet,
            ..options()
        });
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = take(ask(&mut e));
        assert_eq!(r.url, format!("{host}/api/v1/account?by=index&value=0"));
        assert!(r.headers.is_empty());
        let body = r#"{"code":200,"accounts":[{"account_index":0,"index":123,"referrer":9007199254740993},{"account_index":2,"referrer":"wrong-row"}]}"#;
        assert_eq!(
            result(respond(&mut e, &r, 200, body), false).unwrap(),
            r#"{"userId":"0","affiliateId":"9007199254740993"}"#
        );
        assert_eq!(e.account_snapshot_frame(NOW), before);
        assert!(respond(&mut e, &r, 200, body).is_empty());
    }
}
#[test]
fn identity_preserves_large_current_and_legacy_indices_without_requiring_signer_configuration() {
    for body in [
        r#"{"code":200,"accounts":[{"index":9007199254740993,"referrer":"exact"}]}"#,
        r#"{"success":true,"data":{"accountIndex":"9007199254740993","referrer":"exact"}}"#,
    ] {
        let mut e = engine(Options {
            secret: Some("invalid-unused-signer-secret".into()),
            api_key_index: Some("not-a-key-index".into()),
            application_id: Some("9007199254740993".into()),
            ..Default::default()
        });
        let r = take(ask(&mut e));
        assert!(r.url.ends_with("?by=index&value=9007199254740993"));
        assert_eq!(
            result(respond(&mut e, &r, 200, body), false).unwrap(),
            r#"{"userId":"9007199254740993","affiliateId":"exact"}"#
        );
    }
}
#[test]
fn identity_fallback_order_and_same_row_referrer_survive_failed_candidates() {
    let addr = "0x1111111111111111111111111111111111111111";
    let mut e = engine(Options {
        application_id: Some("10".into()),
        key: Some(format!("embedded {addr} key")),
        api_key_index: Some("7".into()),
        ..Default::default()
    });
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = take(ask(&mut e));
    assert!(r.url.ends_with("account?by=index&value=10"));
    let r = take(respond(
        &mut e,
        &r,
        200,
        r#"{"code":200,"accounts":[{"referrer":"must-not-leak"}]}"#,
    ));
    assert!(
        r.url
            .ends_with(&format!("account?by=l1_address&value={addr}"))
    );
    // The API key index names a key slot, not this account, so a failed
    // address read never falls back to it.
    let r = take(respond(&mut e, &r, 200, r#"{"success":false}"#));
    assert!(
        r.url
            .ends_with(&format!("accountsByL1Address?l1_address={addr}"))
    );
    assert_eq!(result(respond(&mut e,&r,200,r#"{"sub_accounts":[{"index":"12"}],"accounts":[{"index":"13","referrer":"wrong-row"}]}"#),false).unwrap(),r#"{"userId":"12"}"#);
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn identity_l1_fallback_accepts_nested_account_arrays_and_their_own_referrer() {
    for body in [
        r#"{"data":{"sub_accounts":[{"index":"5","referrer":"own"}]}}"#,
        r#"{"data":{"accounts":[{"account_index":5,"referrer":"own"}]}}"#,
    ] {
        let mut e = engine(Options {
            application_id: Some("0x2222222222222222222222222222222222222222".into()),
            ..Default::default()
        });
        let r = take(ask(&mut e));
        let r = take(respond(&mut e, &r, 404, r#"{"code":404}"#));
        assert!(r.url.contains("accountsByL1Address"));
        assert_eq!(
            result(respond(&mut e, &r, 200, body), false).unwrap(),
            r#"{"userId":"5","affiliateId":"own"}"#
        );
    }
}
#[test]
fn identity_never_resolves_from_the_api_key_index_alone() {
    let mut e = engine(Options {
        api_key_index: Some("7".into()),
        ..Default::default()
    });
    // `result` itself rejects any HTTP action, so this also proves no lookup ran.
    result(ask(&mut e), true).unwrap_err();
}
#[test]
fn identity_missing_selector_fails_before_io_including_public_only() {
    for public_data_only in [false, true] {
        let mut e = engine(Options {
            public_data_only,
            ..Default::default()
        });
        let before = e.account_snapshot_frame(NOW).to_vec();
        result(ask(&mut e), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_malformed_missing_false_negative_fractional_and_rejected_rows_fail_once() {
    for (status, body) in [
        (200, "{}"),
        (200, r#"{"code":200,"accounts":[]}"#),
        (200, r#"{"code":200,"accounts":[{"index":false}]}"#),
        (200, r#"{"code":200,"accounts":[{"index":""}]}"#),
        (200, r#"{"code":200,"accounts":[{"index":-1}]}"#),
        (200, r#"{"code":200,"accounts":[{"index":1.5}]}"#),
        (200, r#"{"code":400,"accounts":[{"index":1}]}"#),
        (200, r#"{"success":false,"data":{"accountIndex":1}}"#),
        (503, "unavailable"),
    ] {
        let mut e = engine(options());
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = take(ask(&mut e));
        result(respond(&mut e, &r, status, body), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_network_failure_exhausts_only_its_own_candidates_and_duplicate_settlement_is_ignored() {
    let mut e = engine(options());
    let r = take(ask(&mut e));
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "connection lost",
        },
    );
    result(native_actions(&mut e), true).unwrap_err();
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "duplicate",
        },
    );
    assert!(e.drain_actions().next().is_none());
}
