//! AsterDEX admission: native trusted setup matches the TS mapping golden,
//! and the registry admits refs plus a provider while refusing raw material.
#![cfg(feature = "asterdex")]

use safe_cex_engine::signing::{
    Algorithm, Identity, Operation, PayloadPart, Provider, SignRequest,
};
use safe_cex_venues::{
    EngineOptions, build_with_signer, local_signer_from_json_for, local_signer_from_json_for_mode,
    prepare_local_signer_for,
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
        // The WASM path passes the parsed options, with TS's explicit version.
        let (ts, ts_ids) =
            local_signer_from_json_for_mode("asterdex", expected["setup"].to_string(), &options)
                .unwrap();
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

/// Key typing: V3 wallets are declared Secp256k1Digest by trusted policy from
/// the resolved version, so only they answer the wallet's own address. V1
/// declares nothing. One test per case, so every case runs.
fn golden_case(name: &str) -> Value {
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    expand(
        golden["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["name"] == name)
            .unwrap(),
    )
}
fn wallet_address(
    provider: &Provider,
    identity: &Identity,
) -> Result<String, safe_cex_engine::signing::SignError> {
    let request = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::PublicAddress {
            key: identity.signing.clone(),
        }],
    };
    provider.sign_ready(&request).unwrap().map(|result| {
        let hex: String = result.values[0]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        format!("0x{hex}")
    })
}
fn assert_v3_declared(name: &str, aliased: bool) {
    let case = golden_case(name);
    let mut options = engine_options(&case["input"]);
    let (provider, ids) = prepare_local_signer_for("asterdex", &mut options).unwrap();
    let (primary, secondary) = (ids.primary.unwrap(), ids.secondary.unwrap());
    // An aliased read agent shares the primary's one ref and declaration.
    assert_eq!(secondary.signing == primary.signing, aliased, "{name}");
    // Each slot's own wallet address, derived independently from the input:
    // `auth.privateKey` when present (the alias case), else the secret.
    let derived = |field: &Value| {
        safe_cex_crypto::ethereum::private_key_address(field.as_str().unwrap().trim())
            .unwrap()
            .to_ascii_lowercase()
    };
    let input = &case["input"];
    let wallet = if aliased {
        derived(&input["auth"]["privateKey"])
    } else {
        derived(&input["secret"])
    };
    let read = if aliased {
        wallet.clone()
    } else {
        derived(&input["secondary_secret"])
    };
    for (identity, expected) in [(&primary, wallet), (&secondary, read)] {
        assert_eq!(
            wallet_address(&provider, identity).map(|a| a.to_ascii_lowercase()),
            Ok(expected),
            "{name}"
        );
        // The Code (EIP-712) and query endpoints sign on the declared refs.
        assert_eq!(
            signatures(&provider, identity, Algorithm::Secp256k1Digest)
                .map(|values| values[1].len()),
            Ok(65),
            "{name}"
        );
        assert!(
            signatures(&provider, identity, Algorithm::HmacSha256).is_err(),
            "{name}"
        );
    }
}
#[test]
fn v3_read_agent_on_the_alias_signs_through_the_primary_declaration() {
    assert_v3_declared("v3 auth wallet key aliased by the read agent", true);
}
#[test]
fn v3_read_agent_with_its_own_wallet_is_declared_separately() {
    assert_v3_declared(
        "v3 auth addresses with the read agent's own wallet key",
        false,
    );
}
#[test]
fn v1_hmac_secrets_are_not_declared() {
    let case = golden_case("v1 HMAC with its own read agent");
    let mut options = engine_options(&case["input"]);
    let (provider, ids) = prepare_local_signer_for("asterdex", &mut options).unwrap();
    for identity in [ids.primary.unwrap(), ids.secondary.unwrap()] {
        assert!(wallet_address(&provider, &identity).is_err());
        assert!(signatures(&provider, &identity, Algorithm::HmacSha256).is_ok());
    }
}

/// Only the explicit resolved version declares: options without one (as an
/// options-free or version-less caller) declare nothing and V3 refuses.
#[test]
fn json_setup_without_an_explicit_version_declares_nothing() {
    let case = golden_case("v3 wallet key from the secret");
    let setup = case["expected"]["setup"].to_string();
    let versionless: EngineOptions = serde_json::from_value(json!({})).unwrap();
    let (provider, ids) = local_signer_from_json_for_mode("asterdex", setup, &versionless).unwrap();
    let primary = ids.primary.unwrap();
    assert!(signatures(&provider, &primary, Algorithm::Secp256k1Digest).is_err());
    assert!(wallet_address(&provider, &primary).is_err());
}
