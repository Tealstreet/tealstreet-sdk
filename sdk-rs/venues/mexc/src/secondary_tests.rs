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

fn probe(
    venue: MexcFutures,
) -> ProbeVenue<MexcFutures, impl FnMut(&mut MexcFutures, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            venue.signed(
                cx,
                method,
                "/secondary-test",
                String::new(),
                false,
                Pending::Identity(0),
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut engine =
        safe_cex_engine::test_support::seeded_engine(probe(MexcFutures::new(options())));
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
        let expected_headers = sign::Credentials {
            key: expected.into(),
            secret: secret.into(),
        }
        .headers(1700000000000.0, "");
        assert_eq!(request.headers, expected_headers);
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("ApiKey") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}
