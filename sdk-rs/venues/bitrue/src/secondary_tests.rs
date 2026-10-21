use super::*;
use safe_cex_engine::test_support::{ProbeVenue, http_requests};
use safe_cex_engine::{Action, Engine, ReadCredentials};
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

fn probe(venue: Bitrue) -> ProbeVenue<Bitrue, impl FnMut(&mut Bitrue, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        for method in [Method::Get, Method::Post, Method::Put, Method::Delete] {
            venue.send(
                cx,
                method,
                "/secondary-test",
                None,
                None,
                Job::Account(account::Request::Listen { generation: 0 }),
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let mut engine = Engine::new(probe(Bitrue::new(options())));
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
        assert_eq!(
            request
                .headers
                .iter()
                .find(|(k, _)| *k == "X-CH-SIGN")
                .unwrap()
                .1,
            sign::signature(secret, 1700000000000, request.method, "/secondary-test", "")
        );
        assert!(
            request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-CH-APIKEY") && value == expected),
            "{:?}",
            request
        );
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let mut opts = options();
    opts.read_credentials.as_mut().unwrap().secret.clear();
    let mut engine = Engine::new(Bitrue::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn catalog_get_uses_primary_unlimited_client_identity_and_route() {
    let mut engine = Engine::new(Bitrue::new(options()));
    engine.start(1700000000000.0);
    let requests: Vec<_> = http_requests(&mut engine)
        .into_iter()
        .filter(|r| r.url.contains("/fapi/v1/contracts"))
        .collect();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]
            .headers
            .contains(&("X-CH-APIKEY", "primary-key".into()))
    );
    assert_eq!(
        requests[0].route,
        Some(safe_cex_engine::RequestRoute::Primary)
    );
}
