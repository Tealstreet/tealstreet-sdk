use safe_cex_exchange_server::accounts::{engine_options, prepare};
use safe_cex_exchange_server::delegated::{DelegatedPair, prepare_delegated};
use safe_cex_exchange_server::envelope::InitPayload;
use safe_cex_remote_signer::{SignerLink, supports_remote_signing};
use serde_json::{Value, json};
use std::sync::Arc;

fn init(venue: &str) -> InitPayload {
    InitPayload {
        account_name: "synthetic-main".into(),
        exchange_name: venue.into(),
        key: "synthetic-key".into(),
        ..Default::default()
    }
}
fn pair() -> DelegatedPair {
    DelegatedPair::new(
        "synthetic-key".into(),
        "synthetic-signing-fixture".into(),
        None,
    )
}

#[test]
fn delegated_uses_hosted_support_and_requires_a_server_origin() {
    assert!(prepare_delegated(&init("binance"), pair()).is_ok());
    // HyroTrader declares remote signing, but has no server origin configured.
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    assert_eq!(
        rules["venues"]["hyrotrader"]["remoteSigning"]["enabled"],
        true
    );
    assert!(!safe_cex_exchange_server::delegated::enabled("hyrotrader"));
    assert!(prepare_delegated(&init("hyrotrader"), pair()).is_err());
    assert!(!safe_cex_exchange_server::delegated::enabled("unknown"));
}

#[test]
fn delegated_support_matches_hosted_signing_and_server_origins() {
    // Sam's 2026-11-13 ruling removes the former per-venue DELEGATED list.
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    for (venue, rule) in rules["venues"].as_object().unwrap() {
        assert!(rule.get("delegated").is_none(), "{venue}");
        let hosted = supports_remote_signing(venue)
            && safe_cex_exchange_server::accounts::refuse_unhosted(venue).is_ok()
            && safe_cex_native::resolve::rest_base_url(venue, false).is_ok();
        assert_eq!(
            safe_cex_exchange_server::delegated::enabled(venue),
            hosted,
            "{venue}"
        );
    }
}

#[test]
fn bybit_delegated_prepares_injected_private_provider_options() {
    let prepared =
        prepare_delegated(&init("bybit"), pair()).unwrap_or_else(|_| panic!("Bybit prepares"));
    let options: Value = serde_json::from_str(&prepared.options).unwrap();
    assert!(options["signing"]["primary"].is_object());
    assert_eq!(options["publicDataOnly"], false);
    assert_eq!(options["remoteSigning"], false);
    assert!(options.get("key").is_none());
    assert!(options.get("secret").is_none());
    assert!(options.get("passphrase").is_none());
}

#[test]
fn delegated_is_refused_for_unhosted_venues_and_public_accounts() {
    for venue in ["hyrotrader", "binanceoptions", "kucoin", "unknown"] {
        assert!(prepare_delegated(&init(venue), pair()).is_err());
    }
    let mut public = init("bybit");
    public.public_only = Some(true);
    assert!(prepare_delegated(&public, pair()).is_err());
    public.public_only = None;
    public.account_name = "__pub_bybit".into();
    assert!(prepare_delegated(&public, pair()).is_err());
}

#[test]
fn cosigned_and_public_options_keep_their_original_bytes() {
    let private = init("bybit");
    assert_eq!(
        engine_options(&private, false).to_string(),
        r#"{"brokerAttribution":true,"key":"synthetic-key"}"#
    );
    assert_eq!(
        engine_options(&private, true).to_string(),
        r#"{"brokerAttribution":true,"publicDataOnly":true}"#
    );
    let prepared = prepare(
        &private,
        false,
        Some(SignerLink::new(1, Arc::new(|_| true))),
    )
    .unwrap();
    let mut options: Value = serde_json::from_str(&prepared.options).unwrap();
    options["signing"] = json!("opaque-identities");
    assert_eq!(
        options,
        json!({"brokerAttribution":true,"publicDataOnly":false,"remoteSigning":true,"payloadSigner":false,"signing":"opaque-identities","publicMode":{"requested":null,"remoteSigner":true,"payloadSigner":false,"apiKeyIndexPresent":false}})
    );
    assert!(prepared.remote.is_some());
}

#[test]
fn delegated_and_local_providers_return_identical_credential_and_signature_bytes() {
    use safe_cex_engine::signing::{
        Algorithm, Context, Identities, Operation, PayloadPart, SignRequest,
    };
    let prepared =
        prepare_delegated(&init("bybit"), pair()).unwrap_or_else(|_| panic!("Bybit prepares"));
    let options: Value = serde_json::from_str(&prepared.options).unwrap();
    let delegated_ids: Identities = serde_json::from_value(options["signing"].clone()).unwrap();
    let mut local: safe_cex_venues::EngineOptions = serde_json::from_value(
        json!({"key":"synthetic-key", "secret":"synthetic-signing-fixture"}),
    )
    .unwrap();
    let (local, local_ids) =
        safe_cex_venues::prepare_local_signer_for("bybit", &mut local).unwrap();
    assert_eq!(
        serde_json::to_value(&local_ids).unwrap(),
        options["signing"]
    );
    let request = |ids: &Identities| {
        let primary = ids.primary.as_ref().unwrap();
        SignRequest {
            receipt: Default::default(),
            context: Context::default(),
            operations: vec![
                Operation::Present {
                    key: primary.credential.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key: primary.signing_ref().unwrap().clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: b"synthetic-canonical-request".to_vec(),
                    }],
                },
            ],
        }
    };
    let delegated = prepared
        .resident
        .provider()
        .sign_ready(&request(&delegated_ids))
        .unwrap()
        .unwrap();
    let local = local.sign_ready(&request(&local_ids)).unwrap().unwrap();
    // Compare without formatting credential values into assertion output.
    assert!(delegated.values == local.values);
    assert_eq!(delegated.receipt, local.receipt);
    assert!(delegated.headers == local.headers);
}

#[test]
fn delegated_pair_binding_and_empty_material_refuse_with_fixed_errors() {
    for (key, material) in [
        ("different-synthetic-key", "synthetic-signing-fixture"),
        ("synthetic-key", ""),
    ] {
        let result = prepare_delegated(
            &init("bybit"),
            DelegatedPair::new(key.into(), material.into(), None),
        );
        assert!(matches!(result, Err(ref e) if e == "Invalid DELEGATED account setup"));
    }
}

#[test]
fn prepared_delegated_options_build_a_real_bybit_engine_with_the_local_provider() {
    struct Inspect;
    impl safe_cex_venues::Visit for Inspect {
        type Output = bool;
        fn visit<V: safe_cex_engine::Venue + 'static>(
            self,
            engine: safe_cex_engine::Engine<V>,
        ) -> bool {
            engine.signer().is_some()
                && engine
                    .identities()
                    .primary
                    .as_ref()
                    .is_some_and(|identity| identity.signing_ref().is_ok())
        }
    }
    let prepared =
        prepare_delegated(&init("bybit"), pair()).unwrap_or_else(|_| panic!("Bybit prepares"));
    let options = serde_json::from_str(&prepared.options).unwrap();
    let built = safe_cex_venues::build_with_signer(
        "bybit",
        options,
        Some(safe_cex_native::production_entropy()),
        Some(prepared.resident.provider()),
        Inspect,
    );
    assert!(matches!(built, Ok(true)));
}
