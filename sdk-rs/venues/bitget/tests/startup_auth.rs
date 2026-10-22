//! Private startup until the account is ready: transient settings-probe failures
//! back off and retry forever; a documented key refusal fails startup exactly once.
use crate::support::FixtureOptions as Options;
mod support;

use safe_cex_engine::{ErrorKind, Event, ReadCredentials, Response};

use support::*;

const SETTINGS: &str = "/account/settings";

fn keyed() -> Options {
    Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        passphrase: Some("pass".into()),
        ..Options::default()
    }
}

fn expecting(options: Options) -> Harness {
    let mut h = Harness::started_expecting(options, true);
    h.load_public();
    h
}

fn has(h: &Harness, path: &str) -> bool {
    h.requests.iter().any(|r| r.url.contains(path))
}

fn private_connects(h: &Harness) -> usize {
    h.sockets
        .iter()
        .filter(|(_, u)| u.ends_with("/private"))
        .count()
}

fn fire_next(h: &mut Harness) -> bool {
    let Some((timer, at)) = h.timers.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1)) else {
        return false;
    };
    h.timers.retain(|(t, _)| *t != timer);
    h.now = at;
    h.event(Event::TimerFired { timer });
    true
}

/// Fires timers until `path` is requested; returns the elapsed time.
fn until_requested(h: &mut Harness, path: &str) -> Option<f64> {
    let from = h.now;
    for _ in 0..200 {
        if has(h, path) {
            return Some(h.now - from);
        }
        if !fire_next(h) {
            return None;
        }
    }
    None
}

/// Fires every timer within `ms`; probe reads stay unanswered.
fn run_for(h: &mut Harness, ms: f64) {
    let horizon = h.now + ms;
    while h.timers.iter().any(|(_, at)| *at <= horizon) {
        fire_next(h);
    }
}

#[test]
fn transient_probe_failures_back_off_and_the_account_still_loads() {
    let mut h = Harness::private_started();
    let failures: [(u16, &str); 7] = [
        (400, "Bad Request"),
        (404, "Not Found"),
        (
            503,
            r#"{"code":"40015","msg":"System is abnormal","data":null}"#,
        ),
        (
            200,
            r#"{"code":"00000","msg":"success","data":"not settings"}"#,
        ),
        (200, r#"{"code":"99999","msg":"unknown","data":null}"#),
        (
            400,
            r#"{"code":"40009","msg":"sign signature error","data":null}"#,
        ),
        (
            200,
            r#"{"code":"40008","msg":"Request timestamp expired","data":null}"#,
        ),
    ];
    let mut delays = Vec::new();
    for (status, body) in failures {
        h.respond(SETTINGS, status, body);
        assert!(!has(&h, SETTINGS), "no immediate resend");
        delays.push(until_requested(&mut h, SETTINGS).expect("probe retried"));
    }
    let nominal = [1_000., 2_000., 4_000., 8_000., 16_000., 30_000., 30_000.];
    assert_eq!(delays.len(), nominal.len());
    assert!(
        delays.iter().zip(nominal).all(|(d, n)| jittered(*d, n)),
        "{delays:?} not jittered from {nominal:?}"
    );
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    assert!(has(&h, "/all-position"), "classic account loads");
    assert!(h.startup_failures.is_empty());
}

#[test]
fn promoted_probe_transport_failures_retry_until_detection() {
    let mut h = Harness::classic();
    for _ in 0..20 {
        if has(&h, "/all-position") {
            break;
        }
        fire_next(&mut h);
    }
    h.respond(
        "/all-position",
        200,
        r#"{"code":"40085","msg":"Classic Account API is not supported","data":null}"#,
    );
    for _ in 0..6 {
        let probe = h.take(SETTINGS);
        h.event(Event::HttpFailed {
            request: probe.id,
            error: "temporary connection reset",
        });
        assert!(until_requested(&mut h, SETTINGS).is_some(), "probe retried");
    }
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    assert!(has(&h, "/current-position"), "UTA polling resumes");
}

#[test]
fn rejected_key_fails_startup_once_and_stops_retrying() {
    let mut h = expecting(keyed());
    h.respond(
        SETTINGS,
        400,
        r#"{"code":"40006","msg":"Invalid ACCESS_KEY","data":null}"#,
    );
    run_for(&mut h, 600_000.);
    assert!(!has(&h, SETTINGS));
    assert_eq!(private_connects(&h), 0);
    assert_eq!(h.startup_failures.len(), 1);
    let error = &h.startup_failures[0];
    assert_eq!(error.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(error.venue_code.as_deref(), Some("40006"));
    assert!(error.message.contains("Bitget rejected the API key"));
    assert_eq!(h.deactivations, 1);
}

#[test]
fn bad_signature_is_final_only_after_one_re_signed_resend() {
    let mut h = expecting(keyed());
    let sign = r#"{"code":"40009","msg":"sign signature error","data":null}"#;
    h.respond(SETTINGS, 400, sign);
    assert!(h.startup_failures.is_empty());
    until_requested(&mut h, SETTINGS).unwrap();
    h.respond(SETTINGS, 400, sign);
    run_for(&mut h, 600_000.);
    assert!(!has(&h, SETTINGS));
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].venue_code.as_deref(), Some("40009"));
}

#[test]
fn environment_mismatch_fails_startup() {
    let mut h = expecting(keyed());
    h.respond(
        SETTINGS,
        400,
        r#"{"code":"40099","msg":"exchange environment is incorrect","data":null}"#,
    );
    run_for(&mut h, 600_000.);
    assert_eq!(h.startup_failures.len(), 1);
    assert!(!has(&h, SETTINGS));
}

#[test]
fn rejected_read_only_ip_fails_startup_without_deactivating() {
    let mut h = expecting(Options {
        read_credentials: Some(ReadCredentials {
            key: "read".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..keyed()
    });
    h.respond(
        SETTINGS,
        400,
        r#"{"code":"40018","msg":"Invalid IP","data":null}"#,
    );
    run_for(&mut h, 600_000.);
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(h.startup_failures[0].message.contains("read-only API key"));
    assert_eq!(h.deactivations, 0);
}

#[test]
fn uta_on_testnet_fails_startup_once() {
    let mut h = Harness::started_expecting(
        Options {
            testnet: true,
            ..keyed()
        },
        true,
    );
    h.respond("/contracts", 200, &CONTRACTS.replace("USDT", "SUSDT"));
    h.respond("/tickers", 200, &TICKERS.replace("USDT", "SUSDT"));
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#,
    );
    run_for(&mut h, 600_000.);
    assert!(!has(&h, SETTINGS));
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::Unsupported));
    assert!(h.startup_failures[0].message.contains("UTA"));
    assert_eq!(h.deactivations, 0);
}

#[test]
fn invalid_read_only_credentials_fail_startup() {
    let h = Harness::started_expecting(
        Options {
            read_credentials: Some(ReadCredentials {
                key: String::new(),
                secret: "read-secret".into(),
                passphrase: None,
            }),
            ..keyed()
        },
        true,
    );
    assert!(h.requests.is_empty());
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::InvalidInput));
}

#[test]
fn missing_passphrase_fails_trusted_setup_before_startup() {
    let result = try_engine(Options {
        passphrase: None,
        ..keyed()
    });
    assert!(matches!(
        result,
        Err(safe_cex_engine::signing::SignError::InvalidRequest)
    ));
}

/// Detects a Classic account and opens the private socket, without loading.
fn to_login(options: Options) -> Harness {
    let mut h = expecting(options);
    h.respond(
        SETTINGS,
        200,
        r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
    );
    let socket = h
        .sockets
        .iter()
        .find(|(_, u)| u.ends_with("/private"))
        .unwrap()
        .0;
    h.event(Event::WsOpened { socket });
    h
}

#[test]
fn rejected_socket_login_fails_startup_and_stops_reconnecting() {
    let mut h = to_login(keyed());
    h.private_message(r#"{"event":"error","code":30011,"msg":"Invalid ACCESS_KEY"}"#);
    run_for(&mut h, 600_000.);
    assert_eq!(private_connects(&h), 1, "no reconnect");
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].venue_code.as_deref(), Some("30011"));
    assert_eq!(h.deactivations, 1);
}

#[test]
fn generic_socket_login_failure_reconnects() {
    let mut h = to_login(keyed());
    h.private_message(r#"{"event":"login","code":30005,"msg":"Login failed"}"#);
    for _ in 0..50 {
        if private_connects(&h) > 1 {
            break;
        }
        fire_next(&mut h);
    }
    assert!(private_connects(&h) > 1, "reconnects");
    assert!(h.startup_failures.is_empty());
}

#[test]
fn rate_limited_probe_waits_out_the_rest_pause() {
    // The probe shares the private REST pause: Retry-After, else 30 s.
    let limited = r#"{"code":"429","msg":"Too Many Requests","data":null}"#;
    for (status, headers, body, wait) in [
        (429, "Retry-After: 120", limited, 120_000.),
        (418, "", "I'm a teapot", 30_000.),
        (429, "", limited, 30_000.),
    ] {
        let mut h = Harness::private_started();
        let probe = h.take(SETTINGS);
        h.event(Event::HttpResponse {
            request: probe.id,
            response: Response {
                status,
                headers,
                body,
            },
        });
        let from = h.now;
        run_for(&mut h, wait - 1.);
        assert!(!has(&h, SETTINGS), "{status} {headers:?}: retried early");
        until_requested(&mut h, SETTINGS).expect("probe retried");
        assert_eq!(h.now - from, wait, "{status} {headers:?}");
        assert!(h.startup_failures.is_empty());
    }
}

#[test]
fn only_exact_documented_bitget_key_and_passphrase_codes_end_startup() {
    for code in ["30011", "30012"] {
        for encoded in [code.to_owned(), format!(r#""{code}""#)] {
            for event in ["error", "login"] {
                let mut h = to_login(keyed());
                h.private_message(&format!(
                    r#"{{"event":"{event}","code":{encoded},"msg":"refused"}}"#
                ));
                assert_eq!(h.startup_failures.len(), 1, "{event} {encoded}");
                assert_eq!(h.startup_failures[0].venue_code.as_deref(), Some(code));
                assert_eq!(h.deactivations, 1);
            }
        }
    }
    for code in [
        "30011.5", "30012.5", "30005", "30006", "30007", "30013", "30014", "30015", "30033",
    ] {
        let mut h = to_login(keyed());
        h.private_message(&format!(
            r#"{{"event":"error","code":{code},"msg":"retryable or unknown"}}"#
        ));
        for _ in 0..50 {
            if private_connects(&h) > 1 {
                break;
            }
            fire_next(&mut h);
        }
        assert!(private_connects(&h) > 1, "{code}");
        assert!(h.startup_failures.is_empty(), "{code}");
        assert_eq!(h.deactivations, 0);
    }
}

#[test]
fn fractional_socket_codes_that_round_to_terminal_codes_still_reconnect() {
    for code in [
        "30010.999999999999999",
        "30011.000000000000001",
        "30011.999999999999999",
        "30012.000000000000001",
    ] {
        for encoded in [code.to_owned(), format!(r#""{code}""#)] {
            for event in ["error", "login"] {
                let mut h = to_login(keyed());
                h.private_message(&format!(
                    r#"{{"event":"{event}","code":{encoded},"msg":"retryable or unknown"}}"#
                ));
                for _ in 0..50 {
                    if private_connects(&h) > 1 {
                        break;
                    }
                    fire_next(&mut h);
                }
                assert!(private_connects(&h) > 1, "{event} {encoded}");
                assert!(h.startup_failures.is_empty(), "{event} {encoded}");
                assert_eq!(h.deactivations, 0, "{event} {encoded}");
            }
        }
    }
}
