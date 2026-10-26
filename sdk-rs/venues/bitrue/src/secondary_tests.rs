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

/// One account read and the listen key's create and keepalive: the key each
/// signs with follows its credential role, never its HTTP method.
fn probe(venue: Bitrue) -> ProbeVenue<Bitrue, impl FnMut(&mut Bitrue, &mut Cx<'_, Pending>)> {
    ProbeVenue::new(venue, |venue, cx| {
        let jobs = [
            (
                Method::Get,
                Job::Account(account::Request::Snapshot {
                    revision: 0,
                    data: Default::default(),
                }),
            ),
            (
                Method::Post,
                Job::Account(account::Request::Listen { generation: 0 }),
            ),
            (
                Method::Put,
                Job::Account(account::Request::Keepalive { generation: 0 }),
            ),
        ];
        for (method, job) in jobs {
            venue.send(cx, method, "/secondary-test", None, None, job);
        }
    })
}
#[test]
fn the_read_key_signs_account_reads_and_the_primary_signs_sessions() {
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
    assert_eq!(requests.len(), 3);
    for http in requests {
        let expected = if http.method == Method::Get {
            "read-key"
        } else {
            "primary-key"
        };
        assert!(
            http.headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case("X-CH-APIKEY") && value == expected),
            "{:?}",
            http
        );
        assert!(http.headers.iter().any(|(name, _)| *name == "X-CH-SIGN"));
    }
}

#[test]
fn invalid_secondary_identity_stops_startup_before_io() {
    let (mut opts, _) = options();
    opts.read_credentials
        .as_mut()
        .unwrap()
        .set_signing_ref(Some(safe_cex_engine::signing::KeyRef(String::new())));
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
    // A managed catalog read declares its Primary route at preparation.
    let (fresh, _) = self::options();
    let mut engine = safe_cex_engine::test_support::seeded_engine(Bitrue::new(fresh));
    engine.start(1700000000000.0);
    engine.prepare_dispatch(1700000000000.0);
    let intents: Vec<_> = engine
        .drain_actions()
        .filter_map(|action| match action {
            Action::ManagedHttp(managed) => match *managed {
                safe_cex_engine::request_context::HttpAction::Prepare {
                    intent, request, ..
                } if request.url.contains("/fapi/v1/contracts") => Some(intent),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        intents,
        [safe_cex_engine::request_context::RoutingIntent::Primary]
    );
}
