//! A key the venue definitively rejects must fail startup instead of leaving
//! `start()` waiting for an account that never loads.
mod support;

use safe_cex_engine::{ErrorKind, Event, Method};
use safe_cex_venue_bybit::{Endpoints, Variant};
use support::Harness;
use support::Options;

fn private() -> Harness {
    Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    })
}

fn with_read_key() -> Harness {
    Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        read_credentials: Some(safe_cex_engine::ReadCredentials {
            key: "read-key".into(),
            secret: "read-secret".into(),
            passphrase: None,
        }),
        ..Options::default()
    })
}

fn rejection(code: i64, message: &str) -> String {
    format!(
        r#"{{"retCode":{code},"retMsg":"{message}","result":{{}},"retExtInfo":{{}},"time":1676360412576}}"#
    )
}

#[test]
fn definitive_key_rejections_fail_startup_and_deactivate() {
    for (code, kind) in [
        (10003, ErrorKind::AuthInvalid),
        (33004, ErrorKind::AuthExpired),
    ] {
        let mut h = private();
        h.respond(
            "/v5/account/fee-rate",
            200,
            &rejection(code, "API key is invalid."),
        );
        if code == 10003 {
            h.respond(
                "/v5/user/query-api",
                200,
                &rejection(10003, "invalid on both hosts"),
            );
        }
        let [failure] = h.startup_failures.as_slice() else {
            panic!(
                "{code}: one startup failure expected, got {:?}; logs {:?}",
                h.startup_failures, h.logs
            );
        };
        assert_eq!(failure.kind, Some(kind), "{code}");
        assert_eq!(
            failure.venue_code.as_deref(),
            Some(code.to_string().as_str())
        );
        assert_eq!(
            failure.venue_message.as_deref(),
            Some("API key is invalid.")
        );
        assert_eq!(
            failure.message,
            format!("Bybit rejected the API key ({code}): API key is invalid.")
        );
        assert!(h.deactivated, "{code}");
        assert!(h.logs.contains(&failure.message), "{code}: {:?}", h.logs);
    }
}

#[test]
fn a_bad_signature_fails_only_after_the_resigned_resend() {
    let mut h = private();
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(10004, "error sign!"),
    );
    assert!(h.startup_failures.is_empty());
    assert!(!h.deactivated);
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(10004, "error sign!"),
    );
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthInvalid));
    assert!(h.deactivated);
}

#[test]
fn permission_and_transient_fee_failures_keep_starting() {
    let mut h = private();
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(10005, "Permission denied"),
    );
    assert!(h.startup_failures.is_empty());
    assert!(!h.deactivated);
    assert!(
        h.logs.iter().any(|log| log.starts_with("Bybit fee rates:")),
        "{:?}",
        h.logs
    );

    let mut h = private();
    let fees = h.take("/v5/account/fee-rate");
    h.event(Event::HttpFailed {
        request: fees.id,
        error: "connection reset",
    });
    let mut h2 = private();
    h2.respond("/v5/account/fee-rate", 503, "Service Unavailable");
    for h in [h, h2] {
        assert!(h.startup_failures.is_empty());
        assert!(!h.deactivated);
    }
}

#[test]
fn an_ip_mismatch_fails_startup_without_deactivating_the_key() {
    let mut h = private();
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(
            10010,
            "Unmatched IP, please check your API key's bound IP addresses.",
        ),
    );
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthPermission));
    assert!(!h.deactivated);
}

#[test]
fn a_rejected_read_only_key_is_named_and_leaves_the_trading_key_active() {
    let mut h = with_read_key();
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(10003, "API key is invalid."),
    );
    h.respond(
        "/v5/user/query-api",
        200,
        &rejection(10003, "invalid on both hosts"),
    );
    let [failure] = h.startup_failures.as_slice() else {
        panic!("one startup failure expected: {:?}", h.startup_failures);
    };
    assert_eq!(
        failure.message,
        "Bybit rejected the read-only API key (10003): API key is invalid."
    );
    assert!(!h.deactivated);
}

#[test]
fn a_rejected_key_is_reported_once_and_never_reread() {
    let mut h = private();
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(10003, "API key is invalid."),
    );
    h.respond(
        "/v5/user/query-api",
        200,
        &rejection(10003, "invalid on both hosts"),
    );
    h.respond("/v5/market/tickers", 503, "Service Unavailable");
    h.advance(60_000.0);
    assert!(
        h.has_request("/v5/market/tickers"),
        "startup retried: {:?}",
        h.paths()
    );
    assert!(!h.has_request("/v5/account/fee-rate"), "{:?}", h.paths());
    assert_eq!(h.startup_failures.len(), 1);
}

#[test]
fn trading_commands_fail_instead_of_hanging_when_options_are_invalid() {
    use safe_cex_engine::{Command, Outcome};
    // Custom endpoints are rejected for non-HyroTrader accounts, so no request can be sent.
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        endpoints: Some(safe_cex_venue_bybit::Endpoints {
            rest: "https://example.invalid".into(),
            public_ws: "wss://example.invalid".into(),
            private_ws: "wss://example.invalid".into(),
        }),
        ..Options::default()
    });
    h.event(Event::Command(Command::CancelOrders {
        command: 5,
        ids: vec!["1".into()],
    }));
    assert!(h.requests.is_empty());
    assert_eq!(h.outcomes, [(5, Some(Outcome::NotSent))]);
}

const INVALID_KEY: &str = "API key is invalid.";
const LIVE_HINT: &str = "Bybit API key environment mismatch: this key is valid for Bybit Demo Trading, but this account is configured for live Bybit. Create a live Bybit API key or switch the account to Bybit Demo Trading.";
const DEMO_HINT: &str = "Bybit API key environment mismatch: this key is valid for live Bybit, but this account is configured for Bybit Demo Trading. Create a Demo Trading API key or switch the account to live Bybit.";

fn local_options() -> Options {
    Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    }
}

fn rejected_fee(h: &mut Harness) {
    h.respond("/v5/account/fee-rate", 200, &rejection(10003, INVALID_KEY));
}

fn assert_original_failure(h: &Harness, read_key: bool) {
    let [failure] = h.startup_failures.as_slice() else {
        panic!("one original failure expected: {:?}", h.startup_failures);
    };
    let which = if read_key {
        "read-only API key"
    } else {
        "API key"
    };
    assert_eq!(
        failure.message,
        format!("Bybit rejected the {which} (10003): {INVALID_KEY}")
    );
    assert_eq!(failure.kind, Some(ErrorKind::AuthInvalid));
    assert_eq!(failure.venue_code.as_deref(), Some("10003"));
    assert_eq!(failure.venue_message.as_deref(), Some(INVALID_KEY));
    assert_eq!(h.deactivated, !read_key);
}

#[test]
fn environment_probe_uses_the_rejected_get_credential_and_exact_ts_hint_in_both_directions() {
    for variant in [Variant::Linear, Variant::Demo] {
        for read_key in [false, true] {
            let mut options = local_options();
            options.variant = variant;
            options.recv_window = Some(5_000);
            if read_key {
                options.read_credentials = Some(safe_cex_engine::ReadCredentials {
                    key: "read-key".into(),
                    secret: "read-secret".into(),
                    passphrase: None,
                });
            }
            let mut h = Harness::started(options);
            let original = h.take("/v5/account/fee-rate");
            h.reply(original.id, 200, &rejection(10003, INVALID_KEY));
            assert!(h.startup_failures.is_empty());
            assert!(!h.deactivated);
            let probe = h.take("/v5/user/query-api");
            assert_eq!(probe.method, Method::Get);
            assert_eq!(probe.body, None);
            assert_eq!(probe.timeout_ms, Some(2_000));
            let origin = if variant == Variant::Demo {
                "https://api.bybit.com"
            } else {
                "https://api-demo.bybit.com"
            };
            assert_eq!(probe.url, format!("{origin}/v5/user/query-api"));
            assert_eq!(
                Harness::header(&probe, "X-BAPI-API-KEY"),
                Harness::header(&original, "X-BAPI-API-KEY")
            );
            let key = if read_key { "read-key" } else { "api-key" };
            let secret = if read_key { "read-secret" } else { "secret" };
            let timestamp = Harness::header(&probe, "X-BAPI-TIMESTAMP").unwrap();
            let window = Harness::header(&probe, "X-BAPI-RECV-WINDOW").unwrap();
            assert_eq!(window, "5000");
            let signature = safe_cex_crypto::hmac_sha256(
                secret.as_bytes(),
                format!("{timestamp}{key}{window}").as_bytes(),
                safe_cex_crypto::Encoding::Hex,
            );
            assert_eq!(Harness::header(&probe, "X-BAPI-SIGN"), Some(signature));
            h.reply(
                probe.id,
                200,
                r#"{"retCode":0,"retMsg":"OK","result":{"apiKey":"must-not-log-probe-key"}}"#,
            );
            let [failure] = h.startup_failures.as_slice() else {
                panic!("one failure");
            };
            let which = if read_key {
                "read-only API key"
            } else {
                "API key"
            };
            let hint = if variant == Variant::Demo {
                DEMO_HINT
            } else {
                LIVE_HINT
            };
            assert_eq!(
                failure.message,
                format!("Bybit rejected the {which} (10003): {hint} [{INVALID_KEY}]")
            );
            assert_eq!(failure.kind, Some(ErrorKind::AuthInvalid));
            assert_eq!(failure.venue_code.as_deref(), Some("10003"));
            assert_eq!(failure.venue_message.as_deref(), Some(INVALID_KEY));
            assert_eq!(h.deactivated, !read_key);
            assert!(!h.has_request("/v5/account/fee-rate"));
            assert!(!h.has_request("/v5/user/query-api"));
            assert!(
                h.requests
                    .iter()
                    .all(|request| request.method == Method::Get)
            );
            assert!(
                !h.logs
                    .iter()
                    .any(|log| log.contains("must-not-log-probe-key") || log.contains(secret))
            );
        }
    }
}

#[test]
fn unproven_opposite_environment_answers_preserve_the_original_rejection_without_retries() {
    for (status, body) in [
        (200, r#"{"retCode":10003,"retMsg":"probe-key-echo"}"#),
        (
            200,
            r#"{"retCode":10004,"retMsg":"error sign! probe-key-echo"}"#,
        ),
        (200, r#"{"retCode":10005,"retMsg":"probe-key-echo"}"#),
        (401, r#"{"retCode":0,"retMsg":"OK"}"#),
        (403, r#"{"retCode":0,"retMsg":"OK"}"#),
        (503, "probe-key-echo"),
        (200, "{}"),
        (200, "not JSON probe-key-echo"),
    ] {
        let mut h = private();
        rejected_fee(&mut h);
        h.respond("/v5/user/query-api", status, body);
        assert_original_failure(&h, false);
        assert!(!h.has_request("/v5/user/query-api"));
        assert!(!h.has_request("/v5/account/fee-rate"));
        assert!(!h.logs.iter().any(|log| log.contains("probe-key-echo")));
    }
}

#[test]
fn noninteger_probe_codes_preserve_the_original_rejected_credential() {
    for read_key in [false, true] {
        for code in [
            "0.5",
            "-0.5",
            "0.0000000000000000001",
            "-0.0000000000000000001",
            "1e-400",
            "-1e-400",
            "0.0",
            r#""0""#,
        ] {
            let mut h = if read_key { with_read_key() } else { private() };
            rejected_fee(&mut h);
            h.respond(
                "/v5/user/query-api",
                200,
                &format!(r#"{{"retCode":{code},"retMsg":"OK"}}"#),
            );
            assert_original_failure(&h, read_key);
            assert!(!h.has_request("/v5/user/query-api"));
            assert!(!h.has_request("/v5/account/fee-rate"));
        }
    }
}

#[test]
fn probe_failure_and_timeout_finish_once_and_ignore_late_opposite_host_success() {
    for transport_failure in [false, true] {
        let mut h = with_read_key();
        rejected_fee(&mut h);
        let probe = h.take("/v5/user/query-api");
        if transport_failure {
            h.event(Event::HttpFailed {
                request: probe.id,
                error: "https://api-demo.bybit.com/key-sensitive-probe-url",
            });
        } else {
            h.advance(2_000.0);
        }
        assert_original_failure(&h, true);
        h.reply(
            probe.id,
            200,
            r#"{"retCode":0,"retMsg":"OK","result":{"apiKey":"probe-key-echo"}}"#,
        );
        h.advance(5_000.0);
        assert_original_failure(&h, true);
        assert!(!h.has_request("/v5/user/query-api"));
        assert!(!h.has_request("/v5/account/fee-rate"));
        assert!(
            !h.logs
                .iter()
                .any(|log| log.contains("sensitive-probe-url") || log.contains("probe-key-echo"))
        );
    }
}

#[test]
fn concurrent_product_fee_rejections_share_one_probe_and_one_terminal_failure() {
    let mut options = local_options();
    options.enable_inverse_markets = true;
    let mut h = Harness::started(options);
    let mut fees = Vec::new();
    while h.has_request("/v5/account/fee-rate") {
        fees.push(h.take("/v5/account/fee-rate").id);
    }
    assert_eq!(fees.len(), 2);
    for fee in fees {
        h.reply(fee, 200, &rejection(10003, INVALID_KEY));
    }
    assert!(h.startup_failures.is_empty());
    assert_eq!(
        h.requests
            .iter()
            .filter(|request| request.url.contains("/v5/user/query-api"))
            .count(),
        1
    );
    h.respond(
        "/v5/user/query-api",
        200,
        r#"{"retCode":0,"retMsg":"OK","result":{}}"#,
    );
    assert_eq!(h.startup_failures.len(), 1);
    assert!(h.startup_failures[0].message.contains(LIVE_HINT));
    h.advance(5_000.0);
    assert_eq!(h.startup_failures.len(), 1);
    assert!(!h.has_request("/v5/account/fee-rate"));
}

#[test]
fn another_definitive_failure_retires_the_probe_before_its_late_success() {
    let mut options = local_options();
    options.enable_inverse_markets = true;
    let mut h = Harness::started(options);
    rejected_fee(&mut h);
    let probe = h.take("/v5/user/query-api");
    h.respond(
        "/v5/account/fee-rate",
        200,
        &rejection(33004, "key expired"),
    );
    h.reply(probe.id, 200, r#"{"retCode":0,"retMsg":"OK","result":{}}"#);
    h.advance(2_000.0);
    assert_eq!(h.startup_failures.len(), 1);
    assert_eq!(h.startup_failures[0].kind, Some(ErrorKind::AuthExpired));
    assert_eq!(h.startup_failures[0].venue_code.as_deref(), Some("33004"));
    assert!(
        !h.startup_failures[0]
            .message
            .contains("environment mismatch")
    );
}

#[test]
fn environment_probe_never_runs_for_excluded_networks_products_or_signing_contexts() {
    let mut excluded = vec![];
    for variant in [Variant::Linear, Variant::Demo] {
        let mut options = local_options();
        options.variant = variant;
        options.testnet = true;
        excluded.push(options);
    }
    let mut options = local_options();
    options.bybit_site_id = Some("BRA_BTL".into());
    excluded.push(options);
    let mut options = local_options();
    options.remote_signing = true;
    excluded.push(options);
    let mut options = local_options();
    options.secret = Some(String::new());
    excluded.push(options);
    let mut options = local_options();
    options.public_data_only = true;
    excluded.push(options);
    excluded.push(Options::default());
    for variant in [
        Variant::Inverse,
        Variant::Spot,
        Variant::Hyrotrader,
        Variant::Linear,
    ] {
        let mut options = local_options();
        options.variant = variant;
        if matches!(variant, Variant::Hyrotrader | Variant::Linear) {
            options.endpoints = Some(Endpoints {
                rest: "https://custom.example".into(),
                public_ws: "wss://custom.example/public".into(),
                private_ws: "wss://custom.example/private".into(),
            });
        }
        excluded.push(options);
    }
    for options in excluded {
        let mut h = Harness::started(options);
        if h.has_request("/v5/account/fee-rate") {
            rejected_fee(&mut h);
        }
        assert!(!h.has_request("/v5/user/query-api"));
    }
}

/// Every error-level event's (code, message).
fn errors(h: &Harness) -> Vec<(&'static str, String)> {
    h.notices
        .iter()
        .filter_map(|notice| match notice {
            safe_cex_engine::Notice::Event(event)
                if event.level == safe_cex_engine::Level::Error =>
            {
                Some((event.code, event.message.clone()))
            }
            _ => None,
        })
        .collect()
}

const UNMATCHED_IP: &str = "Unmatched IP, please check your API key's bound IP addresses.";

#[test]
fn an_account_info_ip_refusal_fails_startup_visibly_with_the_allowlist_hint() {
    let mut h = private();
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, &rejection(10010, UNMATCHED_IP));
    let [failure] = h.startup_failures.as_slice() else {
        panic!(
            "one startup failure expected: {:?}; logs {:?}",
            h.startup_failures, h.logs
        );
    };
    assert_eq!(failure.kind, Some(ErrorKind::AuthPermission));
    assert!(
        failure.message.contains("IP allowlist"),
        "{}",
        failure.message
    );
    assert!(
        failure.message.contains(UNMATCHED_IP),
        "{}",
        failure.message
    );
    assert!(
        errors(&h)
            .iter()
            .any(|(code, message)| *code == "auth.rejected" && message == &failure.message),
        "an error-level auth.rejected event is required: {:?}",
        errors(&h)
    );
    // An IP mismatch is not a dead key.
    assert!(!h.deactivated);
}

#[test]
fn any_other_account_info_failure_is_an_error_not_a_warning() {
    let mut h = private();
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 503, "Service Unavailable");
    assert!(h.startup_failures.is_empty());
    assert!(
        errors(&h)
            .iter()
            .any(|(_, message)| message.contains("the private API was rejected")),
        "{:?}",
        h.logs
    );
}

#[test]
fn an_account_read_refused_for_the_key_is_an_auth_rejected_error() {
    let mut h = private();
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, support::ACCOUNT_INFO);
    h.load_public();
    h.respond(
        "/v5/account/wallet-balance",
        200,
        &rejection(10010, UNMATCHED_IP),
    );
    assert!(
        errors(&h)
            .iter()
            .any(|(code, message)| *code == "auth.rejected" && message.contains(UNMATCHED_IP)),
        "{:?}",
        errors(&h)
    );
}

#[test]
fn a_refused_key_is_never_used_for_private_reads_or_the_private_feed() {
    let mut h = private();
    h.respond("/v5/account/fee-rate", 200, support::FEE_RATES);
    h.respond("/v5/account/info", 200, &rejection(10010, UNMATCHED_IP));
    h.load_public();
    h.advance(60_000.0);
    for path in [
        "/v5/order/realtime",
        "/v5/position/list",
        "/v5/account/wallet-balance",
    ] {
        assert!(!h.has_request(path), "{path}: {:?}", h.paths());
    }
    assert!(
        h.sockets
            .iter()
            .all(|(_, url)| !url.contains("/v5/private")),
        "{:?}",
        h.sockets
    );
}
