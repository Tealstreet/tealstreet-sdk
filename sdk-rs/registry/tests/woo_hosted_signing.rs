#![cfg(all(feature = "woo", feature = "hosted-signing"))]

use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_engine::{Engine, Venue};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};

fn fixture() -> (LocalSigner, Identities) {
    LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("fixture-public".into()),
            secret: Secret::new("fixture-signing-material".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn woo_hosted_capture_uses_the_owned_primary_for_http_and_ws() {
    let (provider, identities) = fixture();
    for input in [
        Input::Http {
            payload: "symbol=PERP_BTC_USDT|1700000000000".into(),
            stamp_ms: 1_700_000_000_000.,
            revision: "0".into(),
        },
        Input::WsAuth {
            to_sign: "|1700000000000".into(),
            output_format: OutputFormat::Hex,
        },
    ] {
        let (work, kind) = hosted_signing::prepare("woo", &identities, input)
            .expect("Woo requires a hosted-signing capture codec before admission");
        work.validate().unwrap();
        let result = provider.sign_now(&work).unwrap();
        work.validate_result(&result).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        assert_eq!(output.signature.as_ref().unwrap().len(), 64);
    }
}

struct Registered;
impl Visit for Registered {
    type Output = (Identities, bool);
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        (engine.identities().clone(), engine.signer().is_some())
    }
}
#[test]
fn woo_registers_an_opaque_provider_without_raw_secrets() {
    let (provider, identities) = fixture();
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities.clone());
    let provider: Provider = std::sync::Arc::new(provider);
    let (registered, present) = build_with_signer(
        "woo",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Some(provider),
        Registered,
    )
    .expect("Woo must consume opaque signer setup");
    assert!(present);
    assert_eq!(registered, identities);
}

#[test]
fn incomplete_legacy_woo_credentials_keep_public_construction() {
    for json in [
        r#"{"key":"fixture-public"}"#,
        r#"{"key":"fixture-public","secret":""}"#,
        r#"{"publicDataOnly":true}"#,
    ] {
        let (identities, _) = build(
            "woo",
            serde_json::from_str(json).unwrap(),
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Registered,
        )
        .unwrap();
        assert!(identities.primary.is_none());
    }
}

#[test]
fn opaque_woo_credentials_require_the_provider_before_venue_construction() {
    let (_, identities) = fixture();
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities);
    let error = build_with_signer(
        "woo",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        None,
        Registered,
    )
    .unwrap_err();
    assert!(error.message.contains("provider"));
}
