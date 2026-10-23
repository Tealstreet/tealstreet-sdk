//! AsterDEX admission: native trusted setup matches the TS mapping golden,
//! and the registry admits refs plus a provider while refusing raw material.
#![cfg(feature = "asterdex")]

use safe_cex_engine::signing::{
    Algorithm, Identity, Operation, PayloadPart, Provider, SignRequest,
};
use safe_cex_venues::{
    EngineOptions, build_with_signer, local_signer_from_json_for, prepare_local_signer_for,
};
use serde_json::{Value, json};

const GOLDEN: &str =
    include_str!("../../../packages/safe-cex/src/exchanges/rust/asterdex-signer-setup.golden.json");

fn expand(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            let mut out = text.clone();
            while let Some(start) = out.find("<hex32:") {
                let end = start + out[start..].find('>').unwrap();
                let byte = out[start + 7..end].to_owned();
                out.replace_range(start..=end, &byte.repeat(32));
            }
            Value::String(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(expand).collect()),
        Value::Object(map) => map.iter().map(|(k, v)| (k.clone(), expand(v))).collect(),
        other => other.clone(),
    }
}

/// TS account fields to the native options shape.
fn engine_options(input: &Value) -> EngineOptions {
    let auth = &input["auth"];
    serde_json::from_value(json!({
        "key": input["key"],
        "secret": input["secret"],
        "applicationId": input["applicationId"],
        "passphrase": input["passphrase"],
        "asterdex": {
            "apiVersion": input["apiVersion"],
            "userAddress": auth["userAddress"],
            "signerAddress": auth["signerAddress"],
            "privateKey": auth["privateKey"],
            "readKey": input["secondary_key"],
            "readSecret": input["secondary_secret"],
        },
    }))
    .unwrap()
}

fn signatures(
    provider: &Provider,
    identity: &Identity,
    algorithm: Algorithm,
) -> Result<Vec<Vec<u8>>, safe_cex_engine::signing::SignError> {
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                algorithm,
                key: identity.signing.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: vec![0x5a; 32],
                }],
            },
        ],
    };
    // An incomplete identity (blank secret) must fail the same way on both.
    provider
        .sign_ready(&request)
        .unwrap()
        .map(|result| result.values.clone())
}

#[test]
fn native_setup_matches_the_ts_mapping_golden() {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    for case in golden["cases"].as_array().unwrap() {
        let case = expand(case);
        let name = case["name"].as_str().unwrap();
        let expected = &case["expected"];
        let mut options = engine_options(&case["input"]);
        let (native, native_ids) = prepare_local_signer_for("asterdex", &mut options).unwrap();
        // Setup consumes every raw field and leaves resolved public config.
        assert!(options.key.is_none() && options.secret.is_none(), "{name}");
        assert!(options.asterdex.private_key.is_none(), "{name}");
        assert!(options.asterdex.read_key.is_none(), "{name}");
        assert!(options.asterdex.read_secret.is_none(), "{name}");
        assert!(options.passphrase.is_none(), "{name}");
        let version = match options.asterdex.api_version {
            Some(safe_cex_venue_asterdex::sign::Version::V1) => "v1",
            Some(safe_cex_venue_asterdex::sign::Version::V3) => "v3",
            None => "unresolved",
        };
        assert_eq!(version, expected["apiVersion"], "{name}");
        let public = |value: &Option<String>| json!(value);
        assert_eq!(
            public(&options.asterdex.signer_address),
            expected["signerAddress"],
            "{name}"
        );
        assert_eq!(
            public(&options.asterdex.read_signer_address),
            expected["readSignerAddress"],
            "{name}"
        );
        let (ts, ts_ids) =
            local_signer_from_json_for("asterdex", expected["setup"].to_string()).unwrap();
        assert_eq!(native_ids, ts_ids, "{name}: refs, including any alias");
        let algorithm = if expected["apiVersion"] == "v3" {
            Algorithm::Secp256k1Digest
        } else {
            Algorithm::HmacSha256
        };
        // Field for field: refs (incl. alias) above; key bytes, secret presence
        // and signing bytes (secret and encoding) per slot below.
        for (slot, identity) in [
            ("primary", native_ids.primary.as_ref()),
            ("secondary", native_ids.secondary.as_ref()),
        ] {
            let golden = &expected["setup"][slot];
            let Some(identity) = identity else {
                assert!(golden.is_null(), "{name}: {slot}");
                continue;
            };
            let presented = Operation::Present {
                key: identity.credential.clone(),
            };
            let request = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![presented],
            };
            assert_eq!(
                native
                    .sign_ready(&request)
                    .unwrap()
                    .map(|r| r.values.clone()),
                Ok(vec![golden["key"].as_str().unwrap().as_bytes().to_vec()]),
                "{name}: {slot} key"
            );
            let signs = golden["secret"].is_string() || golden["signingFromPrimary"] == true;
            assert_eq!(
                !identity.signing.0.is_empty(),
                signs,
                "{name}: {slot} secret"
            );
        }
        for identity in [native_ids.primary.as_ref(), native_ids.secondary.as_ref()]
            .into_iter()
            .flatten()
        {
            assert_eq!(
                signatures(&native, identity, algorithm),
                signatures(&ts, identity, algorithm),
                "{name}"
            );
        }
    }
}

#[test]
fn v1_api_wallet_keys_without_a_main_wallet_are_refused_at_setup() {
    let mut options: EngineOptions = serde_json::from_value(json!({
        "key": "0x1111111111111111111111111111111111111111",
        "secret": "12".repeat(32),
        "asterdex": {"apiVersion": "v1"},
    }))
    .unwrap();
    let Err(error) = prepare_local_signer_for("asterdex", &mut options) else {
        panic!("API wallet keys under V1 need the main wallet");
    };
    assert!(
        error
            .message
            .starts_with("Aster API Wallet/vault keys require the Main Wallet Address.")
    );
}

struct Accept;
impl safe_cex_venues::Visit for Accept {
    type Output = ();
    fn visit<V: safe_cex_engine::Venue + 'static>(self, _: safe_cex_engine::Engine<V>) {}
}

#[test]
fn registry_admits_refs_with_a_provider_and_refuses_raw_material() {
    let (provider, identities) = local_signer_from_json_for(
        "asterdex",
        r#"{"primary":{"key":"api-key","secret":"api-secret"}}"#.into(),
    )
    .unwrap();
    let with_refs = |extra: Value| {
        let mut options: EngineOptions = serde_json::from_value(extra).unwrap();
        options.signing = Some(identities.clone());
        options.asterdex.api_version = Some(safe_cex_venue_asterdex::sign::Version::V1);
        options
    };
    let entropy = || Some(safe_cex_engine::test_support::seeded_entropy(17));
    build_with_signer(
        "asterdex",
        with_refs(json!({})),
        entropy(),
        Some(provider.clone()),
        Accept,
    )
    .unwrap();
    assert_eq!(
        build_with_signer("asterdex", with_refs(json!({})), entropy(), None, Accept)
            .unwrap_err()
            .message,
        "Engine signing provider is required"
    );
    for raw in [
        json!({"key": "api-key"}),
        json!({"secret": "api-secret"}),
        json!({"asterdex": {"privateKey": "wallet"}}),
        json!({"asterdex": {"readKey": "read", "readSecret": "read-secret"}}),
    ] {
        let error = build_with_signer(
            "asterdex",
            with_refs(raw.clone()),
            entropy(),
            Some(provider.clone()),
            Accept,
        )
        .unwrap_err();
        assert_eq!(
            error.message, "AsterDEX credentials belong in signer setup, not engine options",
            "{raw}"
        );
    }
}
