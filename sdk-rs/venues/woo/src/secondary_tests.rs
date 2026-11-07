use super::*;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
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

fn probe(venue: Woo) -> ProbeVenue<Woo, impl FnMut(&mut Woo, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            venue
                .signed(
                    cx,
                    method,
                    "/v1/secondary-test",
                    vec![],
                    None,
                    Pending::SigningProbe,
                )
                .unwrap();
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Woo::new(options())));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine)
        .into_iter()
        .filter(|r| r.url.contains("secondary-test"))
        .collect();
    assert_eq!(requests.len(), 4);
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
        let signed = sign::request(
            expected,
            secret,
            1700000000000.0,
            request.method,
            "/v1/secondary-test",
            &[],
            None,
        );
        assert_eq!(request.headers, signed.headers);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("x-api-key") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Woo::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
