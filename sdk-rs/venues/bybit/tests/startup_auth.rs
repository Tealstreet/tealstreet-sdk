//! A key the venue definitively rejects must fail startup instead of leaving
//! `start()` waiting for an account that never loads.
mod support;

use safe_cex_engine::{ErrorKind, Event};
use safe_cex_venue_bybit::Options;
use support::Harness;

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
