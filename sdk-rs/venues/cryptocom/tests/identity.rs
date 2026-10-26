//! Actual signed identity queries are isolated from financial reconciliation.
mod support;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
};
use safe_cex_venue_cryptocom::Cryptocom;
use support::fixture::Options;
const NOW: f64 = 1_700_000_000_000.;
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Default::default()
    }
}
fn engine(options: Options) -> Engine<Cryptocom> {
    let mut e = support::fixture::engine(options);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 100.,
        used: 23.,
        upnl: 7.,
    });
    e
}
fn ask(e: &mut Engine<Cryptocom>) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    safe_cex_engine::test_support::prepared_actions(e, NOW)
}
fn take(actions: Vec<Action>) -> HttpRequest {
    let mut requests = actions.into_iter().filter_map(|a| {
        if let Action::Http(r) = a {
            Some(r)
        } else {
            None
        }
    });
    let r = requests.next().expect("identity HTTP");
    assert!(requests.next().is_none());
    assert_eq!(r.method, Method::Post);
    assert_eq!(
        r.url,
        "https://api.crypto.com/exchange/v1/private/get-accounts"
    );
    assert_eq!(r.route, None);
    assert_eq!(r.headers, vec![("Content-Type", "application/json".into())]);
    r
}
fn respond(e: &mut Engine<Cryptocom>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
    respond_at(e, NOW, r, status, body)
}
fn respond_at(
    e: &mut Engine<Cryptocom>,
    now: f64,
    r: &HttpRequest,
    status: u16,
    body: &str,
) -> Vec<Action> {
    e.handle(
        now,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status,
                headers: "",
                body,
            },
        },
    );
    safe_cex_engine::test_support::prepared_actions(e, now)
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
fn identity_primary_signed_post_retains_exact_uuid_with_secondary_configured() {
    let mut opts = options();
    opts.read_credentials = Some(ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: None,
    });
    let mut e = engine(opts);
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = take(ask(&mut e));
    let expected = r#"{"id":1700000000000,"method":"private/get-accounts","api_key":"primary-key","params":{},"sig":"a69045d3689998a494d25861de1154e06c4d2338ef0fb355451fd0af114f5014","nonce":1700000000000}"#.to_owned();
    assert_eq!(r.body.as_deref(), Some(expected.as_str()));
    let body = r#"{"code":0,"result":{"this_account":{"uuid":9007199254740993}}}"#;
    assert_eq!(
        result(respond(&mut e, &r, 200, body), 42, false).unwrap(),
        r#"{"userId":"9007199254740993","affiliateId":null}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(respond(&mut e, &r, 200, body).is_empty());
}
#[test]
fn identity_preserves_uuid_strings_including_text_zero() {
    for id in ["account-uuid", "0", "900719925474099312345"] {
        let mut e = engine(options());
        let r = take(ask(&mut e));
        let body = format!(r#"{{"code":0,"result":{{"this_account":{{"uuid":"{id}"}}}}}}"#);
        assert_eq!(
            result(respond(&mut e, &r, 200, &body), 42, false).unwrap(),
            format!(r#"{{"userId":"{id}","affiliateId":null}}"#)
        );
    }
}
#[test]
fn identity_missing_false_zero_empty_and_malformed_reject_without_store_changes() {
    for body in [
        r#"{}"#,
        r#"{"code":0}"#,
        r#"{"code":0,"result":{}}"#,
        r#"{"code":0,"result":{"this_account":{"uuid":false}}}"#,
        r#"{"code":0,"result":{"this_account":{"uuid":0}}}"#,
        r#"{"code":0,"result":{"this_account":{"uuid":" "}}}"#,
        r#"{"code":0,"result":{"this_account":{"uuid":1.5}}}"#,
    ] {
        let mut e = engine(options());
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = take(ask(&mut e));
        result(respond(&mut e, &r, 200, body), 42, true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_signature_expiry_resigns_once_with_new_clock_then_succeeds() {
    let mut e = engine(options());
    let r = take(ask(&mut e));
    e.handle(
        NOW + 1.,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":40101,"message":"invalid nonce"}"#,
            },
        },
    );
    let retry = take(safe_cex_engine::test_support::prepared_actions(
        &mut e,
        NOW + 1.,
    ));
    let expected=r#"{"id":1700000000001,"method":"private/get-accounts","api_key":"primary-key","params":{},"sig":"ef38ffbd574eb2864d5974329163c8a63a8a6c7b0a29e10c7ceb82f6f8544fbb","nonce":1700000000001}"#.to_owned();
    assert_eq!(retry.body.as_deref(), Some(expected.as_str()));
    result(
        respond_at(
            &mut e,
            NOW + 1.,
            &retry,
            200,
            r#"{"code":0,"result":{"this_account":{"uuid":"ok"}}}"#,
        ),
        42,
        false,
    )
    .unwrap();
}
#[test]
fn identity_repeated_signature_failure_is_bounded() {
    let mut e = engine(options());
    let r = take(ask(&mut e));
    let retry = take(respond(&mut e, &r, 200, r#"{"code":40101}"#));
    result(respond(&mut e, &retry, 200, r#"{"code":40101}"#), 42, true).unwrap_err();
}
#[test]
fn identity_transport_retry_budget_and_other_errors_settle_once() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let mut r = take(ask(&mut e));
    for attempt in 0..4 {
        e.handle(
            NOW,
            Event::HttpFailed {
                request: r.id,
                error: "connection lost",
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if attempt < 3 {
            r = take(actions);
        } else {
            result(actions, 42, true).unwrap_err();
        }
    }
    assert_eq!(e.account_snapshot_frame(NOW), before);
    for (status, body) in [
        (503, "unavailable"),
        (200, r#"{"code":10004,"message":"denied"}"#),
    ] {
        let mut e = engine(options());
        let r = take(ask(&mut e));
        result(respond(&mut e, &r, status, body), 42, true).unwrap_err();
    }
}
#[test]
fn identity_public_only_and_missing_primary_fail_before_io() {
    for opts in [
        Options {
            public_data_only: true,
            ..options()
        },
        Options::default(),
        Options {
            secret: None,
            ..options()
        },
    ] {
        let mut e = engine(opts);
        let actions = ask(&mut e);
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        result(actions, 42, true).unwrap_err();
    }
}
#[test]
fn identity_invalid_saved_read_credentials_do_not_fall_back_silently() {
    let mut opts = options();
    opts.read_credentials = Some(ReadCredentials {
        key: "".into(),
        secret: "read-secret".into(),
        passphrase: None,
    });
    let mut e = engine(opts);
    let actions = ask(&mut e);
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    result(actions, 42, true).unwrap_err();
}
