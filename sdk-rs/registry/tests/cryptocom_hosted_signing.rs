#![cfg(all(feature = "cryptocom", feature = "hosted-signing"))]

use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{Identities, Provider};
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};

struct Registered;
impl Visit for Registered {
    type Output = Identities;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> Self::Output {
        assert!(engine.signer().is_some());
        engine.identities().clone()
    }
}

#[test]
fn cryptocom_registers_opaque_remote_and_payload_custody_without_local_credentials() {
    for payload_signer in [false, true] {
        let (signer, ids) = LocalSigner::from_setup(LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("replay-key".into()),
                secret: Secret::new("replay-secret".into()),
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
        let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
        options.signing = Some(ids.clone());
        options.payload_signer = payload_signer;
        options.remote_signing = !payload_signer;
        let provider: Provider = std::sync::Arc::new(signer);
        let registered = build_with_signer(
            "cryptocom",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(provider),
            Registered,
        )
        .unwrap();
        assert_eq!(registered, ids);
    }
}

#[test]
fn cryptocom_payload_flag_without_a_provider_is_typed_invalid_input_and_never_falls_back() {
    for raw in ["{}", r#"{"key":"replay-key","secret":"replay-secret"}"#] {
        let mut options: EngineOptions = serde_json::from_str(raw).unwrap();
        options.payload_signer = true;
        options.public_data_only = false;
        let error = build(
            "cryptocom",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Registered,
        )
        .unwrap_err();
        assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
        assert_eq!(error.outcome, Some(Outcome::NotSent));
        assert!(!error.message.contains("replay-secret"));
    }
}

#[test]
fn cryptocom_owned_http_and_ws_captures_match_independent_hmac_vectors() {
    let (provider, identities) = LocalSigner::from_setup(LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new("replay-key".into()),
            secret: Secret::new("replay-secret".into()),
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let vectors: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../venues/cryptocom/tests/fixtures/hosted-signing.json"
    ))
    .unwrap();
    for v in vectors {
        let (work, kind) = hosted_signing::prepare(
            "cryptocom",
            &identities,
            Input::Http {
                payload: v["payload"].as_str().unwrap().into(),
                stamp_ms: 1_610_000_000_000.,
                revision: "0".into(),
            },
        )
        .expect("Crypto.com requires its hosted capture codec");
        let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
        assert_eq!(output.credential.as_deref(), Some("replay-key"));
        assert_eq!(output.signature.as_deref(), v["signature"].as_str());
    }
    let (work, kind) = hosted_signing::prepare(
        "cryptocom",
        &identities,
        Input::WsAuth {
            to_sign: "public/auth1610000000000replay-key1610000000000".into(),
            output_format: OutputFormat::Hex,
        },
    )
    .unwrap();
    let output = hosted_signing::finish(kind, &provider.sign_now(&work).unwrap()).unwrap();
    assert_eq!(
        output.signature.as_deref(),
        Some("57a5207e795bd876080625ee876fa750cd0134c3916bcdc346d4efe1b6f0218f")
    );
}
