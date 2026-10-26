#![cfg(feature = "bitrue")]
use safe_cex_engine::{Action, Engine, HttpRequest, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
struct Catalogue;
impl Visit for Catalogue {
    type Output = Vec<HttpRequest>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        engine.start(1234.9);
        safe_cex_engine::test_support::prepared_actions(&mut engine, 1234.9)
            .into_iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            })
            .collect()
    }
}
/// The routing intent each catalogue read declares at preparation: managed
/// requests carry their route there, not in `HttpRequest::route`.
struct Intents;
impl Visit for Intents {
    type Output = Vec<safe_cex_engine::request_context::RoutingIntent>;
    fn visit<V: Venue + 'static>(self, mut engine: Engine<V>) -> Self::Output {
        use safe_cex_engine::request_context::HttpAction;
        engine.start(1234.9);
        engine.prepare_dispatch(1234.9);
        engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::ManagedHttp(managed) => match *managed {
                    HttpAction::Prepare {
                        intent, request, ..
                    } if request.url.ends_with("/fapi/v1/contracts") => Some(intent),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }
}
const PRIMARY: [safe_cex_engine::request_context::RoutingIntent; 1] =
    [safe_cex_engine::request_context::RoutingIntent::Primary];
fn entropy() -> Option<safe_cex_engine::entropy::Entropy> {
    Some(safe_cex_engine::test_support::seeded_entropy(17))
}
#[test]
fn trusted_compatibility_setup_signs_catalogue_with_primary_and_normalizes_only_window() {
    for (window, expected) in [(None, "20000"), (Some(0), "1"), (Some(100_000), "60000")] {
        let options = || {
            let mut options: EngineOptions = serde_json::from_str(r#"{"key":"fixture-primary-key","secret":"fixture-primary-secret","readCredentials":{"key":"fixture-read-key","secret":"fixture-read-secret"},"publicDataOnly":true}"#).unwrap();
            options.recv_window = window.map(f64::from);
            options
        };
        let requests = build("bitrue", options(), entropy(), Catalogue).unwrap();
        assert_eq!(
            build("bitrue", options(), entropy(), Intents).unwrap(),
            PRIMARY
        );
        assert_eq!(requests.len(), 1);
        let r = &requests[0];
        assert_eq!(r.url, "https://fapi.bitrue.com/fapi/v1/contracts");
        assert!(
            r.headers
                .contains(&("X-CH-APIKEY", "fixture-primary-key".into()))
        );
        assert!(r.headers.contains(&("X-CH-TS", "1234".into())));
        assert!(r.headers.contains(&("recvWindow", expected.into())));
    }
}
#[test]
fn provider_injection_requires_no_raw_rest_fallback() {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-key".into()),
            secret: Secret::new("fixture-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let mut options: EngineOptions = serde_json::from_str(r#"{"publicDataOnly":true}"#).unwrap();
    options.signing = Some(identities.clone());
    let requests = build_with_signer(
        "bitrue",
        options,
        entropy(),
        Some(std::sync::Arc::new(provider)),
        Catalogue,
    )
    .unwrap();
    assert!(
        requests[0]
            .headers
            .contains(&("X-CH-APIKEY", "fixture-key".into()))
    );
    let mut options: EngineOptions = serde_json::from_str(r#"{"publicDataOnly":true}"#).unwrap();
    options.signing = Some(identities);
    options.key = Some("must-not-reach-venue".into());
    let error = build_with_signer("bitrue", options, entropy(), None, Catalogue).unwrap_err();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    assert_eq!(
        error.message,
        "Bitrue credentials belong in signer setup, not engine options"
    );
}
#[test]
fn keyless_catalogue_and_hosted_admission_remain_distinct() {
    let requests = build(
        "bitrue",
        serde_json::from_str("{}").unwrap(),
        entropy(),
        Catalogue,
    )
    .unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        build(
            "bitrue",
            serde_json::from_str("{}").unwrap(),
            entropy(),
            Intents
        )
        .unwrap(),
        PRIMARY
    );
    assert_eq!(
        requests[0]
            .headers
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        ["Content-Type", "X-Referer"]
    );
    assert!(!safe_cex_venues::remote_signing_policy("bitrue").enabled);
}

#[test]
fn explicit_empty_primary_keeps_the_native_unsigned_catalogue() {
    for input in [
        r#"{"key":"","secret":"","publicDataOnly":true}"#,
        r#"{"key":"fixture-key","secret":"","publicDataOnly":true}"#,
        r#"{"key":"","secret":"fixture-material","publicDataOnly":true}"#,
    ] {
        let requests = build(
            "bitrue",
            serde_json::from_str(input).unwrap(),
            entropy(),
            Catalogue,
        )
        .unwrap();
        assert_eq!(requests.len(), 1, "unsigned catalogue for {input}");
        assert_eq!(
            build(
                "bitrue",
                serde_json::from_str(input).unwrap(),
                entropy(),
                Intents
            )
            .unwrap(),
            PRIMARY,
            "{input}"
        );
        assert_eq!(
            requests[0]
                .headers
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            ["Content-Type", "X-Referer"]
        );
    }
}

#[test]
fn malformed_complete_primary_is_refused_instead_of_made_anonymous() {
    use safe_cex_crypto::signer::LocalSigner;
    let (provider, identities) = LocalSigner::from_json(
        r#"{"primary":{"key":"fixture-key","secret":"fixture-material"}}"#.into(),
    )
    .unwrap();
    let provider = std::sync::Arc::new(provider);
    for invalid_stable_id in [true, false] {
        let mut identities = identities.clone();
        let primary = identities.primary.as_mut().unwrap();
        if invalid_stable_id {
            primary.stable_id = Some("\n".into());
        } else {
            primary.set_signing_ref(Some(safe_cex_engine::signing::KeyRef("x".repeat(257))));
        }
        let mut options: EngineOptions =
            serde_json::from_str(r#"{"publicDataOnly":true}"#).unwrap();
        options.signing = Some(identities);
        let error = build_with_signer(
            "bitrue",
            options,
            entropy(),
            Some(provider.clone()),
            Catalogue,
        )
        .unwrap_err();
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
    }
}
