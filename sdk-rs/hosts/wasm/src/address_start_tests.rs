use super::*;
use crate::address_start_fixture::*;
use safe_cex_engine::AddressSetupSlots;
use safe_cex_engine::nonce::{ASTERDEX_V3, HYPERLIQUID_AGENT};
use std::sync::{Arc, Mutex};

#[test]
fn wasm_start_initializes_real_local_provider_before_venue_start_once() {
    for domain in [Some(&HYPERLIQUID_AGENT), Some(&ASTERDEX_V3), None] {
        let (provider, mut identities) = safe_cex_venues::local_signer_from_json_for_mode("asterdex", format!(
            r#"{{"primary":{{"key":"opaque-credential","secret":"{}01","secretEncoding":"hex"}}}}"#,
            "00".repeat(31),
        ), &serde_json::from_str(r#"{"asterdex":{"apiVersion":"v3"}}"#).unwrap()).unwrap();
        identities.secondary = identities.primary.clone();
        let observations = Arc::new(Mutex::new(Vec::new()));
        let inner = engine(
            Probe {
                observations: observations.clone(),
                domain,
                slots: AddressSetupSlots::Both,
            },
            Some(provider),
            identities,
        );
        let mut engine = WasmEngine {
            dispatch_signing: true,
            external_signer: false,
            construction: "{}".into(),
            hosted_identity: None,
            hosted_captures: Default::default(),
            #[cfg(feature = "hosted-wallet")]
            hosted_wallet: None,
            #[cfg(feature = "algos")]
            algos: Default::default(),
            inner: Box::new(inner),
        };
        let output = engine.start(100.);
        let actions: Vec<serde_json::Value> = if output.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&output).unwrap()
        };
        assert!(
            !actions
                .iter()
                .any(|a| a["op"] == "sign" || a["op"] == "retireAddressSetup")
        );
        assert_eq!(
            *observations.lock().unwrap(),
            vec![vec![Ok(ADDRESS), Ok(ADDRESS)]]
        );
        assert_eq!(engine.start(101.), "");
        assert_eq!(observations.lock().unwrap().len(), 1);
    }
}
