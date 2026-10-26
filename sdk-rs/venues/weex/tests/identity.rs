//! Configuration validates the selected read signer while attribution remains primary-key scoped.
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, ReadCredentials, Response,
};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::setup::Options;

const NOW: f64 = 1_700_000_000_000.;
fn engine(options: Options) -> Engine<WeexFutures> {
    let mut e = support::setup::engine(options);
    e.store_mut().set_balance(safe_cex_engine::model::Balance {
        total: 123.,
        free: 100.,
        used: 23.,
        upnl: 7.,
    });
    e
}
fn ask(e: &mut Engine<WeexFutures>) -> Vec<Action> {
    e.handle(NOW, Event::Command(Command::FetchAccount { command: 42 }));
    safe_cex_engine::test_support::prepared_actions(e, NOW)
}
fn take(actions: Vec<Action>) -> HttpRequest {
    let mut iter = actions.into_iter().filter_map(|a| match a {
        Action::Http(r) => Some(r),
        _ => None,
    });
    let Some(r) = iter.next() else {
        panic!("identity requires exactly one HTTP action");
    };
    assert!(iter.next().is_none());
    assert_eq!(r.method, Method::Get);
    assert_eq!(r.body, None);
    assert_eq!(r.route, None);
    r
}
fn respond(e: &mut Engine<WeexFutures>, r: &HttpRequest, status: u16, body: &str) -> Vec<Action> {
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

const CONFIG: &str = r#"{"dualSidePosition":true,"canTrade":true}"#;
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("primary-secret".into()),
        passphrase: Some("primary-pass".into()),
        stable_id: Some("weex-account-7".into()),
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
fn request(e: &mut Engine<WeexFutures>) -> HttpRequest {
    let r = take(ask(e));
    assert_eq!(
        r.url,
        "https://api-contract.weex.com/capi/v3/account/accountConfig"
    );
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
fn identity_primary_hmac_and_stable_id_preserve_existing_account_settings_and_state() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    assert_eq!(header(&r, "ACCESS-KEY"), "primary-key");
    assert_eq!(header(&r, "ACCESS-PASSPHRASE"), "primary-pass");
    assert_eq!(header(&r, "ACCESS-TIMESTAMP"), "1700000000000");
    assert_eq!(
        header(&r, "ACCESS-SIGN"),
        "ZBtxEs55ZOASmsxxmx4OJPAX0FEU0PJoEdOHxtiK7Qc="
    );
    assert_eq!(
        result(respond(&mut e, &r, 200, CONFIG), false).unwrap(),
        r#"{"userId":"weex-account-7"}"#
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
    assert!(respond(&mut e, &r, 200, CONFIG).is_empty());
}
#[test]
fn identity_secondary_get_key_and_passphrase_validate_without_changing_primary_namespace() {
    let mut e = engine(Options {
        read_credentials: Some(read()),
        ..options()
    });
    let r = request(&mut e);
    assert_eq!(header(&r, "ACCESS-KEY"), "read-key");
    assert_eq!(header(&r, "ACCESS-PASSPHRASE"), "read-pass");
    assert_eq!(
        header(&r, "ACCESS-SIGN"),
        "RU+VzovsJ06hbBrNlM02oD8KDBpWRqNq7nt/qn1xc+0="
    );
    assert_eq!(
        result(respond(&mut e, &r, 200, CONFIG), false).unwrap(),
        r#"{"userId":"weex-account-7"}"#
    );
}
#[test]
fn identity_secondary_missing_passphrase_retains_existing_primary_passphrase_fallback() {
    let mut e = engine(Options {
        read_credentials: Some(ReadCredentials {
            passphrase: None,
            ..read()
        }),
        ..options()
    });
    let r = request(&mut e);
    assert_eq!(header(&r, "ACCESS-KEY"), "read-key");
    assert_eq!(header(&r, "ACCESS-PASSPHRASE"), "primary-pass");
    result(respond(&mut e, &r, 200, CONFIG), false).unwrap();
}
#[test]
fn identity_requires_trusted_primary_stable_id_without_key_hash_fallback() {
    for read in [None, Some(read())] {
        let mut e = engine(Options {
            stable_id: None,
            read_credentials: read,
            ..options()
        });
        let before = e.account_snapshot_frame(NOW).to_vec();
        let error = result(ask(&mut e), true).unwrap_err();
        assert!(error.contains("trusted primary stableId"), "{error}");
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_unavailable_is_typed_not_sent() {
    let mut e = engine(Options {
        stable_id: None,
        ..options()
    });
    let error = ask(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .unwrap();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
}
#[test]
fn read_credentials_never_stand_in_for_an_unusable_primary_identity() {
    let mut e = engine(Options {
        key: Some(" primary-key ".into()),
        secret: None,
        passphrase: None,
        read_credentials: Some(read()),
        ..options()
    });
    let error = result(ask(&mut e), true).unwrap_err();
    assert!(error.contains("non-public primary key"), "{error}");
}
#[test]
fn identity_disabled_trading_missing_or_malformed_config_and_business_errors_reject_without_state_commit()
 {
    for body in [
        r#"{"dualSidePosition":true,"canTrade":false}"#,
        "{}",
        r#"{"canTrade":true}"#,
        r#"{"dualSidePosition":false,"canTrade":"true"}"#,
        r#"{"dualSidePosition":false,"canTrade":true,"code":123}"#,
        r#"{"dualSidePosition":false,"canTrade":true,"code":false}"#,
    ] {
        let mut e = engine(options());
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        result(respond(&mut e, &r, 200, body), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_public_only_missing_primary_or_invalid_selected_auth_fails_before_io() {
    for (case, opts) in [
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
        Options {
            secret: None,
            passphrase: None,
            read_credentials: Some(ReadCredentials {
                passphrase: None,
                ..read()
            }),
            ..options()
        },
    ]
    .into_iter()
    .enumerate()
    {
        // Trusted setup refuses missing required passphrases outright; every other
        // case reaches the venue, which refuses its unusable identity before I/O.
        let refused_by_setup = case == 5;
        assert_eq!(
            opts.clone().try_build().is_err(),
            refused_by_setup,
            "case {case}"
        );
        if refused_by_setup {
            continue;
        }
        let mut e = engine(opts);
        let before = e.account_snapshot_frame(NOW).to_vec();
        result(ask(&mut e), true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
    }
}
#[test]
fn identity_network_and_http_failures_settle_once_without_reconciliation() {
    for network in [false, true] {
        let mut e = engine(options());
        let before = e.account_snapshot_frame(NOW).to_vec();
        let r = request(&mut e);
        let actions = if network {
            e.handle(
                NOW,
                Event::HttpFailed {
                    request: r.id,
                    error: "network",
                },
            );
            e.drain_actions().collect()
        } else {
            respond(&mut e, &r, 503, "unavailable")
        };
        result(actions, true).unwrap_err();
        assert_eq!(e.account_snapshot_frame(NOW), before);
        assert!(respond(&mut e, &r, 200, CONFIG).is_empty());
    }
}
#[test]
fn identity_rate_rejection_keeps_global_cooldown_and_does_not_install_private_poll_timers() {
    let mut e = engine(options());
    let before = e.account_snapshot_frame(NOW).to_vec();
    let r = request(&mut e);
    result(respond(&mut e, &r, 429, "rate limited"), true).unwrap_err();
    assert!(
        result(ask(&mut e), true)
            .unwrap_err()
            .contains("rate limited")
    );
    assert_eq!(e.account_snapshot_frame(NOW), before);
}
