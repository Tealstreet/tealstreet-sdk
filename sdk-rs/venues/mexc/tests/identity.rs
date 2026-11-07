//! Private validation uses the selected read signer; persisted identity is Primary metadata.
mod support;
use safe_cex_engine::{
    Action, Command, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
};
use support::Options;

const NOW: f64 = 1_700_000_000_000.;
fn engine(options: Options) -> support::TestEngine {
    let mut e = support::configured(options);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 100.,
        used: 23.,
        upnl: 7.,
    });
    e
}
fn ask(e: &mut support::TestEngine) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    let actions = support::prepared(e, NOW);
    let mut timers = std::collections::HashSet::new();
    let mut retained = Vec::new();
    for action in actions {
        match action {
            Action::SetTimer { timer, at_ms } => {
                assert!([NOW + 20_000., NOW + 120_000.].contains(&at_ms));
                assert!(timers.insert(timer));
            }
            Action::ClearTimer { timer } => {
                assert!(timers.remove(&timer));
            }
            action => retained.push(action),
        }
    }
    retained
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
fn respond(e: &mut support::TestEngine, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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
    e.drain_actions().collect()
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
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        ..Default::default()
    }
}
fn read() -> ReadCredentials {
    ReadCredentials {
        key: "read-key".into(),
        secret: "read-secret".into(),
        passphrase: None,
    }
}
fn request(e: &mut support::TestEngine) -> HttpRequest {
    let r = take(ask(e));
    assert_eq!(r.url, "https://api.mexc.com/api/v1/private/account/assets");
    r
}
fn header<'a>(r: &'a HttpRequest, name: &str) -> &'a str {
    r.headers
        .iter()
        .find(|(n, _)| *n == name)
        .unwrap()
        .1
        .as_str()
}
#[test]
fn identity_primary_hmac_and_fixed_hash_vector_accept_valid_empty_assets_without_store_changes() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert_eq!(header(&r, "ApiKey"), "primary-key");
    assert_eq!(header(&r, "Request-Time"), "1700000000000");
    assert_eq!(
        header(&r, "Signature"),
        "7aab839b166e3a062398132a47634cced8ff9e1f5a7e31028e986e33c04549e4"
    );
    let body = r#"{"success":true,"code":0,"data":[]}"#;
    assert_eq!(
        result(respond(&mut e, &r, 200, body), false).unwrap(),
        r#"{"userId":"mexc:64a04c6ee432d0c9370920d0"}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(respond(&mut e, &r, 200, body).is_empty());
}
#[test]
fn identity_secondary_get_signer_does_not_replace_primary_key_fingerprint() {
    let mut e = engine(Options {
        read_credentials: Some(read()),
        ..options()
    });
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert_eq!(header(&r, "ApiKey"), "read-key");
    assert_eq!(
        header(&r, "Signature"),
        "1504972030dc652723afa2f090b303172cdd621de85ab3f304bc291e467287ec"
    );
    assert_eq!(result(respond(&mut e,&r,200,r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":"999","equity":"1000","unrealized":"2"}]}"#),false).unwrap(),r#"{"userId":"mexc:64a04c6ee432d0c9370920d0"}"#);
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
#[test]
fn identity_hashes_raw_options_key_even_when_native_primary_signer_trims_it() {
    let mut e = engine(Options {
        key: Some(" primary-key ".into()),
        ..options()
    });
    let r = request(&mut e);
    assert_eq!(header(&r, "ApiKey"), "primary-key");
    assert_eq!(
        result(
            respond(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#),
            false
        )
        .unwrap(),
        r#"{"userId":"mexc:8103f888c531840bd2978455"}"#
    );
}
#[test]
fn identity_complete_read_credentials_can_validate_when_primary_secret_is_unavailable() {
    let mut e = engine(Options {
        secret: None,
        read_credentials: Some(read()),
        ..options()
    });
    let r = request(&mut e);
    assert_eq!(header(&r, "ApiKey"), "read-key");
    assert_eq!(
        result(
            respond(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#),
            false
        )
        .unwrap(),
        r#"{"userId":"mexc:64a04c6ee432d0c9370920d0"}"#
    );
}
#[test]
fn identity_public_only_missing_primary_key_or_invalid_selected_credentials_fail_before_io() {
    for opts in [
        Options {
            public_data_only: true,
            read_credentials: Some(read()),
            ..options()
        },
        Options {
            key: None,
            read_credentials: Some(read()),
            ..options()
        },
        Options {
            secret: None,
            ..options()
        },
        Options {
            read_credentials: Some(ReadCredentials {
                key: "".into(),
                ..read()
            }),
            ..options()
        },
        Options {
            read_credentials: Some(ReadCredentials {
                secret: "".into(),
                ..read()
            }),
            ..options()
        },
    ] {
        let mut e = engine(opts);
        let before = e.account_snapshot_frame(NOW).to_vec();
        result(ask(&mut e), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_malformed_assets_and_business_http_errors_reject_once_without_financial_commit() {
    for (status, body) in [
        (200, "{}"),
        (200, r#"{"success":false,"code":1001,"data":[]}"#),
        (200, r#"{"success":true,"code":0}"#),
        (200, r#"{"success":true,"code":0,"data":false}"#),
        (200, r#"{"success":true,"code":0,"data":[{}]}"#),
        (
            200,
            r#"{"success":true,"code":0,"data":[{"currency":"USDT","availableBalance":"invalid"}]}"#,
        ),
        (503, "unavailable"),
    ] {
        let mut e = engine(options());
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        result(respond(&mut e, &r, status, body), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_network_failure_is_scoped_and_duplicate_completion_is_consumed_once() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    e.handle(
        NOW,
        Event::HttpFailed {
            request: r.id,
            error: "network",
        },
    );
    result(e.drain_actions().collect(), true).unwrap_err();
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(respond(&mut e, &r, 200, r#"{"success":true,"code":0,"data":[]}"#).is_empty());
}
#[test]
fn identity_rate_rejection_preserves_shared_cooldown_without_starting_reconciliation() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    result(
        respond(&mut e, &r, 429, r#"{"success":false,"code":429}"#),
        true,
    )
    .unwrap_err();
    assert!(
        result(ask(&mut e), true)
            .unwrap_err()
            .contains("rate limited")
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
