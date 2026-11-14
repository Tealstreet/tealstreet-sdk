//! V1 keeps its HMAC capture; V3 uses the separate account-bound wallet codec.
#![cfg(feature = "asterdex")]

use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{Identities, Identity, KeyRef};
use safe_cex_engine::signing_capture::Capture;
use safe_cex_venues::EngineOptions;
use safe_cex_venues::hosted_signing::{self, Input, Output};

const STAMP: f64 = 1_709_209_849_123.;
const QUERY: &str = "symbol=BTCUSDT&recvWindow=20000&timestamp=1709209849123";
// Python hmac/hashlib over QUERY with the fixture secret, not the provider.
const QUERY_MAC: &str = "4c6d3d8130f58f899d84bf0f47f1277e0696ccf38256e0ab48ca3b3ba0232704";

fn fixture() -> (LocalSigner, Identities) {
    let mut signer = LocalSigner::new();
    let primary = Identity::new(
        KeyRef("owned/primary/presentation".into()),
        Some(KeyRef("owned/primary/mac".into())),
    );
    signer.insert(primary.credential.clone(), b"hosted-aster-key".to_vec());
    signer.insert(
        primary.signing_ref().unwrap().clone(),
        b"fixture-only-v1-secret".to_vec(),
    );
    (
        signer,
        Identities {
            primary: Some(primary),
            secondary: None,
        },
    )
}

fn capture(signer: &LocalSigner, ids: &Identities, input: Input) -> Output {
    let (work, kind) = hosted_signing::prepare("asterdex", ids, input).unwrap();
    let mut captures = Capture::default();
    let prepared = captures.prepare(work, kind, 100., STAMP, 7, 5_000).unwrap();
    let result = signer.sign_now(&prepared.work).unwrap();
    let (kind, result) = captures
        .finish(prepared.id, 101., STAMP + 1., 7, Ok(result))
        .unwrap();
    hosted_signing::finish(kind, &result).unwrap()
}

#[test]
fn v1_hosted_http_signs_the_exact_query_with_the_owned_primary() {
    let (signer, ids) = fixture();
    let credential = capture(&signer, &ids, Input::Credential {});
    assert_eq!(credential.credential.as_deref(), Some("hosted-aster-key"));
    let output = capture(
        &signer,
        &ids,
        Input::Http {
            payload: QUERY.into(),
            stamp_ms: STAMP,
            revision: "7".into(),
        },
    );
    assert_eq!(output.credential.as_deref(), Some("hosted-aster-key"));
    assert_eq!(output.signature.as_deref(), Some(QUERY_MAC));
}

#[test]
fn only_resolved_v1_accounts_are_admitted_to_hosted_signing() {
    let admitted = |json: &str| {
        let options: EngineOptions = serde_json::from_str(json).unwrap();
        hosted_signing::admitted("asterdex", &options)
    };
    assert!(admitted(r#"{"asterdex":{"apiVersion":"v1"}}"#));
    assert!(!admitted(r#"{"asterdex":{"apiVersion":"v3"}}"#));
    // Only an explicit, setup-resolved V1 qualifies; inference cannot see
    // material held by the provider setup.
    assert!(!admitted(r#"{"key":"api-key","secret":"api-secret"}"#));
    assert!(!admitted(
        r#"{"applicationId":"0x2222222222222222222222222222222222222222","asterdex":{"signerAddress":"0x1111111111111111111111111111111111111111"}}"#
    ));
}

#[test]
fn ws_auth_is_refused_because_aster_uses_a_listen_key() {
    let (_, ids) = fixture();
    let input = Input::WsAuth {
        to_sign: "GET/realtime1709209800000".into(),
        output_format: hosted_signing::OutputFormat::Hex,
    };
    assert!(matches!(
        hosted_signing::prepare("asterdex", &ids, input),
        Err(safe_cex_engine::signing::SignError::InvalidRequest)
    ));
}

#[cfg(feature = "hosted-wallet")]
mod wallet {
    use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
    use safe_cex_engine::signing::{
        CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding, Provider,
    };
    use safe_cex_venues::hosted_wallet::{self, Account};

    #[test]
    fn aster_device_owns_v1_hmac_batch_and_refuses_unknown_descriptor_fields() {
        let (local, ids) = LocalSigner::from_setup_with_policy(
            LocalSetup {
                primary: Some(LocalIdentity {
                    key: Secret::new("fixture-key".into()),
                    secret: Secret::new("fixture-material".into()),
                    ..Default::default()
                }),
                secondary: None,
            },
            CredentialSetupPolicy {
                primary: Some(IdentitySetupPolicy {
                    secret_encoding: Some(KeyEncoding::Utf8),
                    ..Default::default()
                }),
                secondary: None,
            },
        )
        .unwrap();
        let provider: Provider = std::sync::Arc::new(local);
        let text = r#"{"slot":"primary","payload":"timestamp=1700000000123"}"#;
        let output = hosted_wallet::sign(
            "asterdex",
            &ids,
            &Account::default(),
            &provider,
            text,
            "asterdex-hmac",
            1_700_000_000_123.,
        )
        .unwrap();
        let output: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(output["credential"], "fixture-key");
        assert_eq!(
            output["signature"],
            safe_cex_crypto::hmac_sha256(
                b"fixture-material",
                b"timestamp=1700000000123",
                safe_cex_crypto::Encoding::Hex
            )
        );
        assert!(
            hosted_wallet::sign(
                "asterdex",
                &ids,
                &Account::default(),
                &provider,
                &text.replace('{', "{\"unknown\":true,"),
                "asterdex-hmac",
                1_700_000_000_123.
            )
            .is_err()
        );
    }

    #[test]
    fn v3_device_matches_independent_ethers_vectors_and_refuses_foreign_or_stale_statements() {
        use safe_cex_engine::signing::Algorithm;
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../venues/asterdex/tests/fixtures/signing.json"
        ))
        .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let (local, ids) = LocalSigner::from_setup_with_policy(
                LocalSetup {
                    primary: Some(LocalIdentity {
                        key: Secret::new("fixture-key".into()),
                        secret: Secret::new(case["privateKey"].as_str().unwrap().into()),
                        ..Default::default()
                    }),
                    secondary: None,
                },
                CredentialSetupPolicy {
                    primary: Some(IdentitySetupPolicy {
                        secret_encoding: Some(KeyEncoding::Hex),
                        signing_algorithm: Some(Algorithm::Secp256k1Digest),
                        ..Default::default()
                    }),
                    secondary: None,
                },
            )
            .unwrap();
            let provider: Provider = std::sync::Arc::new(local);
            let account = Account {
                asterdex_v3: true,
                asterdex_user_address: Some(case["user"].as_str().unwrap().into()),
                testnet: case["testnet"].as_bool().unwrap(),
                ..Default::default()
            };
            let name = case["name"].as_str().unwrap();
            let keys: &[&str] = if name.starts_with("documented") {
                &["symbol", "type", "side", "timeInForce", "quantity", "price"]
            } else if name.starts_with("decimal") {
                &["symbol", "quantity", "price", "reduceOnly", "note"]
            } else if name.starts_with("query") {
                &["symbol", "note"]
            } else {
                &[]
            };
            let query: Vec<_> = keys
                .iter()
                .map(|key| (*key, case["params"][*key].as_str().unwrap()))
                .collect();
            let now = case["nowMs"].as_f64().unwrap();
            for (method, expected) in [("GET", "signature"), ("POST", "codeSignature")] {
                let statement = serde_json::json!({ "method": method, "path": "/fapi/v3/order", "query": query,
                "user": case["user"], "signer": case["signer"], "nonce": case["nonce"], "stampMs": case["nowMs"], "testnet": case["testnet"] });
                let descriptor = |payload: &serde_json::Value| {
                    serde_json::json!({ "slot": "primary", "payload": payload.to_string() })
                        .to_string()
                };
                let result = hosted_wallet::sign(
                    "asterdex",
                    &ids,
                    &account,
                    &provider,
                    &descriptor(&statement),
                    "asterdex-wallet",
                    now,
                )
                .unwrap_or_else(|error| panic!("{name} {method}: {error:?}"));
                let result: serde_json::Value = serde_json::from_str(&result).unwrap();
                let raw = result["signature"].as_str().unwrap();
                let v = u8::from_str_radix(&raw[128..], 16).unwrap() + 27;
                assert_eq!(format!("0x{}{:02x}", &raw[..128], v), case[expected]);
                for (field, value) in [
                    (
                        "user",
                        serde_json::json!("0x1111111111111111111111111111111111111111"),
                    ),
                    (
                        "signer",
                        serde_json::json!("0x2222222222222222222222222222222222222222"),
                    ),
                    ("testnet", serde_json::json!(!account.testnet)),
                    ("stampMs", serde_json::json!(now as u64 - 30_001)),
                    ("unknown", serde_json::json!(true)),
                    ("path", serde_json::json!("/fapi/v3/withdraw")),
                    ("nonce", serde_json::json!("01")),
                ] {
                    let mut invalid = statement.clone();
                    invalid[field] = value;
                    assert!(
                        hosted_wallet::sign(
                            "asterdex",
                            &ids,
                            &account,
                            &provider,
                            &descriptor(&invalid),
                            "asterdex-wallet",
                            now
                        )
                        .is_err(),
                        "{field}"
                    );
                }
            }
            assert!(
                hosted_wallet::sign(
                    "asterdex",
                    &ids,
                    &account,
                    &provider,
                    r#"{"slot":"primary","payload":"opaque digest"}"#,
                    "asterdex-hmac",
                    now
                )
                .is_err()
            );
        }
    }
}
