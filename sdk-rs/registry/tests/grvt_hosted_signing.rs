#![cfg(all(feature = "grvt", feature = "hosted-wallet"))]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, IdentitySetupPolicy, KeyEncoding,
};
use safe_cex_venues::hosted_wallet::{self, Account};

fn fixture() -> (LocalSigner, safe_cex_engine::signing::Identities) {
    LocalSigner::from_setup_with_policy(
        LocalSetup {
            primary: Some(LocalIdentity {
                key: Secret::new("fixture-api-key".into()),
                secret: Secret::new("01".repeat(32)),
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
    .unwrap()
}

#[test]
fn grvt_device_codec_admits_its_own_address_request() {
    let (signer, ids) = fixture();
    let account = Account {
        key: Some("fixture-api-key".into()),
        ..Default::default()
    };
    let provider: safe_cex_engine::signing::Provider = std::sync::Arc::new(signer);
    let output = hosted_wallet::sign(
        "grvt",
        &ids,
        &account,
        &provider,
        "fixture-api-key",
        "grvt-address",
        1_700_000_000_000.,
    )
    .unwrap();
    assert_eq!(
        output.to_lowercase(),
        "0x1a642f0e3c3af545e7acbd38b07251b3990914f1"
    );
}

#[test]
fn grvt_device_rederives_exact_digest_and_refuses_other_accounts_networks_and_unknown_fields() {
    use safe_cex_engine::model::TimeInForce;
    use safe_cex_venue_grvt::provider::{FrozenOrder, OrderFields, hosted_digest};
    let fields = OrderFields {
        subaccount: "9007199254740993".into(),
        instrument: "BTC_USDT_Perp".into(),
        asset_id: [3; 32],
        base_decimals: 9,
        amount: "1.001".into(),
        limit_price: Some("50000.1".into()),
        buying: true,
        time_in_force: TimeInForce::PostOnly,
        reduce_only: false,
        nonce: 42,
        expiration_ns: 1_700_003_600_000_000_000,
        client_id: "99".into(),
        created_ns: 1_700_000_000_000_000_000,
        trigger: None,
    };
    let frozen = FrozenOrder::new(fields, false).unwrap();
    let text = frozen.statement();
    assert_eq!(
        hosted_digest(text, "9007199254740993", false, 1_700_000_000_000.).unwrap(),
        *frozen.digest()
    );
    assert!(hosted_digest(text, "9007199254740992", false, 1_700_000_000_000.).is_err());
    assert!(hosted_digest(text, "9007199254740993", true, 1_700_000_000_000.).is_err());
    assert!(hosted_digest(text, "9007199254740993", false, 1_700_003_600_000.).is_err());
    for invalid in [
        text.replace("{", "{\"unknown\":true,"),
        text.replace("\"nonce\":42", "\"nonce\":42,\"nonce\":43"),
        text.replace("\"amount\":\"1001000000\"", "\"amount\":\"01001000000\""),
    ] {
        assert!(hosted_digest(&invalid, "9007199254740993", false, 1_700_000_000_000.).is_err());
    }
}

#[test]
fn grvt_registry_preserves_public_and_api_key_only_login_construction() {
    struct Accept;
    impl safe_cex_venues::Visit for Accept {
        type Output = ();
        fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
    }
    for options in [
        "{}",
        r#"{"key":"fixture-api-key"}"#,
        r#"{"publicDataOnly":true}"#,
    ] {
        let options = serde_json::from_str(options).unwrap();
        safe_cex_venues::build(
            "grvt",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Accept,
        )
        .unwrap();
    }
}

#[test]
fn grvt_capture_clock_change_retires_old_work_and_resigns_frozen_nonce_without_renewal() {
    use safe_cex_engine::signing::{Context, Operation, PayloadPart, SignReceipt, SignRequest};
    use safe_cex_engine::signing_capture::Capture;
    let (local, ids) = fixture();
    let id = ids.primary.as_ref().unwrap();
    let text = format!(
        r#"{{"subaccount":"777","testnet":false,"assetId":"{}","amount":"1000000000","price":"100000000000","buying":true,"market":false,"tif":1,"postOnly":false,"reduceOnly":false,"nonce":42,"expiration":"1700003600000000000"}}"#,
        "03".repeat(32)
    );
    let digest =
        safe_cex_venue_grvt::provider::hosted_digest(&text, "777", false, 1_700_000_000_000.)
            .unwrap();
    let work = SignRequest {
        receipt: SignReceipt {
            expiry_ms: Some(1_700_003_600_000.),
            ..Default::default()
        },
        context: Context::default(),
        operations: vec![
            Operation::Sha256 {
                payload: vec![PayloadPart::Bytes {
                    bytes: text.as_bytes().to_vec(),
                }],
            },
            Operation::PublicAddress {
                key: id.signing_ref().unwrap().clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: id.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: digest.to_vec(),
                }],
            },
        ],
    };
    let expected = local.sign_now(&work).unwrap();
    let mut captures = Capture::default();
    let old = captures
        .prepare(work.clone(), (), 0., 1_700_000_000_000., 0, 5000)
        .unwrap();
    assert!(
        captures
            .finish(old.id, 1., 1_700_000_001_000., 1, Ok(expected.clone()))
            .is_err()
    );
    assert!(captures.is_empty());
    let fresh = captures
        .prepare(work.clone(), (), 1., 1_700_000_001_000., 1, 5000)
        .unwrap();
    let resigned = local.sign_now(&fresh.work).unwrap();
    assert_eq!(resigned.values, expected.values);
    captures
        .finish(fresh.id, 2., 1_700_000_001_000., 1, Ok(resigned))
        .unwrap();
    let provider: safe_cex_engine::signing::Provider = std::sync::Arc::new(local);
    let account = Account {
        grvt_trading_account_id: Some("777".into()),
        ..Default::default()
    };
    let first = hosted_wallet::sign(
        "grvt",
        &ids,
        &account,
        &provider,
        &text,
        "grvt-order",
        1_700_000_000_000.,
    )
    .unwrap();
    let corrected = hosted_wallet::sign(
        "grvt",
        &ids,
        &account,
        &provider,
        &text,
        "grvt-order",
        1_700_000_001_000.,
    )
    .unwrap();
    assert_eq!(first, corrected);
    assert!(captures.is_empty());
}
