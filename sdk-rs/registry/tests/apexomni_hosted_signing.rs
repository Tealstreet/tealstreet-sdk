#![cfg(all(feature = "apexomni", feature = "hosted-signing"))]
use safe_cex_crypto::signer::{
    LocalExtra, LocalExtras, LocalIdentity, LocalSetup, LocalSigner, Secret,
};
use safe_cex_engine::signing::{
    CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, IdentitySetupPolicy,
    KeyEncoding, SignError,
};
use safe_cex_venues::hosted_signing::{self, Input, OutputFormat};

fn fixture() -> (LocalSigner, safe_cex_engine::signing::Identities) {
    let mut extra = LocalExtras::default();
    extra.0.insert(
        ExtraKey::ZklinkSeed,
        LocalExtra {
            value: Secret::new(format!("{}1b", "01".repeat(64))),
            encoding: KeyEncoding::Hex,
        },
    );
    LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("fixture-public".into()),
                secret: Secret::new("fixture-material".into()),
                passphrase: Some(Secret::new("fixture-pass".into())),
                extra,
                ..Default::default()
            }),
            secondary: None,
        },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                secret_encoding: Some(KeyEncoding::Base64Text),
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::ZklinkSeed,
                    field: ExtraFieldPolicy::RequiredForPrivate,
                    encoding: Some(KeyEncoding::Hex),
                    trim: false,
                }),
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap()
}

#[test]
fn apexomni_hosted_ws_and_order_batches_match_local_custody_and_independent_hmac() {
    let (signer, ids) = fixture();
    for order in [false, true] {
        let contract = safe_cex_venue_apexomni::sign::contract_payload(
            "fixture-client",
            "1",
            1,
            "0.01",
            "90500",
            "0.0002",
            "0.0005",
            true,
        )
        .unwrap();
        let payload = if order {
            serde_json::json!([{ "bytes": b"1700000000123POST/api/v3/order{signature=".to_vec() }, { "contract":0 }, { "bytes": b"}".to_vec() }])
        } else {
            serde_json::json!([{ "bytes": b"1700000000123GET/ws/accounts".to_vec() }])
        };
        let descriptor = serde_json::json!({"contracts":if order { vec![contract.to_vec()] } else {vec![]},"payload":payload}).to_string();
        let format: OutputFormat = serde_json::from_str("\"apexomni-batch\"").unwrap();
        let (work, kind) = hosted_signing::prepare(
            "apexomni",
            &ids,
            Input::WsAuth {
                to_sign: descriptor,
                output_format: format,
            },
        )
        .unwrap();
        let result = signer.sign_now(&work).unwrap();
        let output = hosted_signing::finish(kind, &result).unwrap();
        let batch: serde_json::Value =
            serde_json::from_str(output.signature.as_deref().unwrap()).unwrap();
        assert_eq!(batch["credential"], "fixture-public");
        let text = if order {
            format!(
                "1700000000123POST/api/v3/order{{signature={}}}",
                safe_cex_crypto::hex(&result.values[0])
            )
        } else {
            "1700000000123GET/ws/accounts".into()
        };
        assert_eq!(
            batch["signature"],
            safe_cex_crypto::hmac_sha256(
                b"Zml4dHVyZS1tYXRlcmlhbA==",
                text.as_bytes(),
                safe_cex_crypto::Encoding::Base64
            )
        );
        assert_eq!(
            batch["contracts"].as_array().unwrap().len(),
            usize::from(order)
        );
        let wire = output.signature.unwrap();
        assert!(!wire.contains("fixture-pass") && !wire.contains("fixture-material"));
    }
}

#[test]
fn apexomni_capture_refuses_unknown_fields_foreign_slots_and_non_contract_zk_work() {
    let (_, ids) = fixture();
    for descriptor in [
        r#"{"contracts":[],"payload":[],"extra":"no"}"#,
        r#"{"slot":"secondary","contracts":[],"payload":[{"bytes":[1]}]}"#,
        r#"{"contracts":[[0]],"payload":[{"contract":0}]}"#,
        r#"{"contracts":[],"payload":[{"contract":0}]}"#,
    ] {
        let format: OutputFormat = serde_json::from_str("\"apexomni-batch\"").unwrap();
        assert!(matches!(
            hosted_signing::prepare(
                "apexomni",
                &ids,
                Input::WsAuth {
                    to_sign: descriptor.into(),
                    output_format: format
                }
            ),
            Err(SignError::InvalidRequest)
        ));
    }
}

#[test]
fn apexomni_capture_fences_clock_changes_and_rejects_partial_or_oversized_results() {
    use safe_cex_engine::signing_capture::Capture;
    let (signer, ids) = fixture();
    let stamp = 1_700_000_000_123.;
    let input = || {
        Input::Http {
        payload: serde_json::json!({"contracts":[],"payload":[{"bytes":b"1700000000123GET/api/v3/account".to_vec()}]}).to_string(),
        stamp_ms: stamp,
        revision: "0".into(),
    }
    };
    let (work, kind) = hosted_signing::prepare("apexomni", &ids, input()).unwrap();
    let mut capture = Capture::default();
    let token = capture.prepare(work, kind, 0., stamp, 0, 5000).unwrap();
    let result = signer.sign_now(&token.work).unwrap();
    assert!(
        capture
            .finish(token.id, 1., stamp + 1000., 1, Ok(result))
            .is_err()
    );
    assert!(capture.is_empty());
    let (work, kind) = hosted_signing::prepare("apexomni", &ids, input()).unwrap();
    let mut result = signer.sign_now(&work).unwrap();
    result.values.pop();
    assert!(hosted_signing::finish(kind, &result).is_err());
    assert!(
        hosted_signing::finish(
            hosted_signing::OutputKind::ApexOmni {
                contracts: usize::MAX,
                ws: true,
            },
            &result
        )
        .is_err()
    );
}
