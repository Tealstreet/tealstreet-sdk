use super::*;
use safe_cex_engine::Action;
use safe_cex_engine::test_support::{ProbeVenue, prepared_actions};
fn options() -> (Options, safe_cex_engine::signing::Provider) {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let slot = |key: &str, material: &str| LocalIdentity {
        key: Secret::new(key.into()),
        secret: Secret::new(material.into()),
        ..Default::default()
    };
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(slot(
            "primary-key",
            "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=",
        )),
        secondary: Some(slot(
            "read-key",
            "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=",
        )),
    })
    .unwrap();
    (
        Options {
            credentials: identities.primary,
            read_credentials: identities.secondary,
            ..Default::default()
        },
        std::sync::Arc::new(provider),
    )
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
                if method == Method::Get {
                    Job::Account(account::Request::Snapshot {
                        revision: 0,
                        data: Default::default(),
                    })
                } else {
                    Job::Account(account::Request::Listen { generation: 0 })
                },
            );
        }
    })
}
#[test]
fn secondary_get_keeps_primary_mutations() {
    let (options, provider) = options();
    let mut engine = safe_cex_engine::test_support::seeded_engine(probe(Bitrue::new(options)));
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1700000000000.0);
    let requests: Vec<_> = prepared_actions(&mut engine, 1700000000000.)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .filter(|r| r.url.contains("secondary-test"))
        .collect();
    assert_eq!(requests.len(), 4);
    for request in requests {
        let expected = if request.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        let secret = if request.method == Method::Get {
            "AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI="
        } else {
            "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE="
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
    let (mut opts, _) = options();
    opts.read_credentials
        .as_mut()
        .unwrap()
        .signing_ref()
        .unwrap()
        .clone()
        .0
        .clear();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Bitrue::new(opts));
    engine.start(1700000000000.0);
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::Http(_) | Action::WsConnect { .. }))
    );
}

#[test]
fn catalog_get_uses_primary_unlimited_client_identity_and_route() {
    let (options, provider) = options();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Bitrue::new(options));
    engine.set_signer(Some(provider)).unwrap();
    engine.start(1700000000000.0);
    let requests: Vec<_> = prepared_actions(&mut engine, 1700000000000.)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(r) => Some(r),
            _ => None,
        })
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
