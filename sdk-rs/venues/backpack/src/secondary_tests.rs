use super::*;
use safe_cex_engine::{Action, ReadCredentials};
fn options() -> Options {
    Options {
        key: Some("primary-key".into()),
        secret: Some("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=".into()),
        read_credentials: Some(ReadCredentials {
            key: "read-key".into(),
            secret: "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=".into(),
            passphrase: Some("read-pass".into()),
        }),

        ..Options::default()
    }
}

#[path = "../tests/support/secondary_signing.rs"]
mod managed_probe;
use managed_probe::probe;

#[test]
fn secondary_get_keeps_primary_mutations() {
    let requests: Vec<_> = probe(Backpack::new(options()))
        .into_iter()
        .filter(|r| r.url.ends_with("/api/v1/orders") || r.url.ends_with("/api/v1/order"))
        .collect();
    assert_eq!(requests.len(), 3);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let opts = options();
        let secret = if request.method == Method::Get {
            opts.read_credentials.as_ref().unwrap().secret.as_str()
        } else {
            opts.secret.as_deref().unwrap()
        };
        let credentials = sign::Credentials::new(expected.into(), secret).unwrap();
        let instruction = match request.method {
            Method::Get => "orderQueryAll",
            Method::Post => "orderExecute",
            _ => "orderCancel",
        };
        assert_eq!(
            request.headers,
            credentials.headers(instruction, &[Default::default()], 1700000000000.0)
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-API-Key") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Backpack::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
