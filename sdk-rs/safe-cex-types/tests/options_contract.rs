// Compile the isolated slice without changing the integrator-owned crate root.
pub use contract::*;
pub use safe_cex_types::{
    ExchangeName, PayloadSignerCapability, PublicDataSourceCapability, RemoteSignerCapability,
    TransportCapability,
};
#[path = "../src/strict_decode.rs"]
mod strict_decode;
fn deserialize_present<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    d: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
#[path = "../src/options.rs"]
mod contract;
use ts_rs::TS;

#[test]
fn signing_mode_is_optional_closed_and_preserved_in_both_models() {
    for mode in [None, Some("local"), Some("cosigned"), Some("delegated")] {
        for (account, mut wire) in [
            (false, serde_json::json!({"key":"", "secret":""})),
            (
                true,
                serde_json::json!({"exchange":"bybit", "name":"main", "apiKey":"", "apiSecret":""}),
            ),
        ] {
            if let Some(mode) = mode {
                wire["signingMode"] = mode.into();
            }
            let output = if account {
                serde_json::to_value(
                    serde_json::from_value::<contract::ExchangeAccountConfig>(wire.clone())
                        .unwrap(),
                )
                .unwrap()
            } else {
                serde_json::to_value(
                    serde_json::from_value::<contract::ExchangeOptions>(wire.clone()).unwrap(),
                )
                .unwrap()
            };
            assert_eq!(output.get("signingMode"), wire.get("signingMode"));
        }
    }
    for invalid in [
        serde_json::Value::Null,
        serde_json::json!("DELEGATED"),
        serde_json::json!(7),
        serde_json::json!({"local":null}),
    ] {
        assert!(serde_json::from_value::<contract::ExchangeAccountConfig>(serde_json::json!({
            "exchange":"bybit", "name":"main", "apiKey":"", "apiSecret":"", "signingMode":invalid
        })).is_err());
        assert!(
            serde_json::from_value::<contract::ExchangeOptions>(serde_json::json!({
                "key":"", "secret":"", "signingMode":invalid
            }))
            .is_err()
        );
    }
}

#[test]
fn account_identity_retains_absent_null_and_empty_strings() {
    for value in [
        None,
        Some(None),
        Some(Some(String::new())),
        Some(Some("0".into())),
    ] {
        let account = contract::ExchangeAccount {
            user_id: String::new(),
            sub_id: value.clone(),
            affiliate_id: value,
        };
        let wire = serde_json::to_value(&account).unwrap();
        assert_eq!(wire["userId"], "");
        match &account.sub_id {
            None => assert!(wire.get("subId").is_none()),
            Some(None) => assert!(wire["subId"].is_null()),
            Some(Some(value)) => assert_eq!(wire["subId"], *value),
        }
        let old: safe_cex_types::ExchangeAccount = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(old).unwrap(), wire);
        let generated: contract::ExchangeAccount = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(generated).unwrap(), wire);
    }
}

#[test]
fn new_options_keep_zero_false_and_reject_json_runtime_capabilities() {
    let options = contract::ExchangeOptions {
        key: "fixture".into(),
        hyperliquid_fast_book: Some(false),
        worker_mirror: Some(false),
        caller_reports_placement_failures: Some(false),
        engine: Some(contract::ExchangeOptionsEngine::Rust),
        ..Default::default()
    };
    let wire = serde_json::to_value(options).unwrap();
    assert_eq!(wire["hyperliquidFastBook"], false);
    assert_eq!(wire["workerMirror"], false);
    assert_eq!(wire["callerReportsPlacementFailures"], false);
    assert_eq!(wire["engine"], "rust");
    assert!(wire.get("localSigner").is_none());
    assert!(wire.get("publicFeed").is_none());
    for value in ["null", "{}", "[]", "false"] {
        assert!(serde_json::from_str::<contract::LocalSignerMetadataCapability>(value).is_err());
        assert!(serde_json::from_str::<contract::RustPublicFeedCapability>(value).is_err());
    }
    assert!(serde_json::from_str::<contract::ExchangeOptionsEngine>(r#"{"rust":null}"#).is_err());
    let extra: contract::ExchangeOptionsExtra =
        serde_json::from_str(r#"{"rustCommandTimeoutMs":0,"tickInterval":0}"#).unwrap();
    assert_eq!(extra.rust_command_timeout_ms, Some(0.));
    assert_eq!(extra.tick_interval, Some(0.));
    assert!(
        serde_json::from_str::<contract::ExchangeOptionsExtra>(r#"{"rustCommandTimeoutMs":null}"#)
            .is_err()
    );
    let options: contract::ExchangeOptions = serde_json::from_str(r#"{"key":"fixture","secret":"","workerMirror":false,"engine":"rust","polymarket":{"marketSlugs":["fixture"],"signerType":0}}"#).unwrap();
    assert_eq!(options.worker_mirror, Some(false));
    let prediction = options.polymarket.unwrap();
    assert_eq!(prediction.market_slugs, Some(vec!["fixture".into()]));
    assert_eq!(prediction.signer_type, Some(0.));
}

#[test]
fn export_options_slice() {
    if let Ok(destination) = std::env::var("SAFE_CEX_OPTIONS_OUTPUT") {
        std::fs::create_dir_all(&destination).unwrap();
        let config = ts_rs::Config::new();
        for (name, text) in [
            ("ExchangeOptions", contract::ExchangeOptions::decl(&config)),
            ("ExchangeAccount", contract::ExchangeAccount::decl(&config)),
            (
                "ExchangeAccountConfig",
                contract::ExchangeAccountConfig::decl(&config),
            ),
        ] {
            std::fs::write(
                std::path::Path::new(&destination).join(format!("{name}.ts")),
                text,
            )
            .unwrap();
        }
        let identities: Vec<_> = [
            None,
            Some(None),
            Some(Some(String::new())),
            Some(Some("000000000000000017".into())),
        ]
        .into_iter()
        .map(|value| contract::ExchangeAccount {
            user_id: "900719925474099312345".into(),
            sub_id: value.clone(),
            affiliate_id: value,
        })
        .collect();
        std::fs::write(
            std::path::Path::new(&destination).join("account-fixtures.json"),
            serde_json::to_vec(&identities).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn coinbase_jwt_algorithm_survives_canonical_options_and_account_config() {
    for algorithm in ["es256", "edDsa"] {
        let options =
            format!(r#"{{"key":"fixture","secret":"","coinbaseJwtAlgorithm":"{algorithm}"}}"#);
        let options: contract::ExchangeOptions = serde_json::from_str(&options).unwrap();
        assert_eq!(
            serde_json::to_value(options).unwrap()["coinbaseJwtAlgorithm"],
            algorithm
        );
        let account = format!(
            r#"{{"exchange":"coinbase","name":"fixture","apiKey":"","apiSecret":"","coinbaseJwtAlgorithm":"{algorithm}"}}"#
        );
        let account: contract::ExchangeAccountConfig = serde_json::from_str(&account).unwrap();
        assert_eq!(
            serde_json::to_value(account).unwrap()["coinbaseJwtAlgorithm"],
            algorithm
        );
    }
    for invalid in ["null", "17", r#""ed25519""#] {
        let options =
            format!(r#"{{"key":"fixture","secret":"","coinbaseJwtAlgorithm":{invalid}}}"#);
        assert!(serde_json::from_str::<contract::ExchangeOptions>(&options).is_err());
        let account = format!(
            r#"{{"exchange":"coinbase","name":"fixture","apiKey":"","apiSecret":"","coinbaseJwtAlgorithm":{invalid}}}"#
        );
        assert!(serde_json::from_str::<contract::ExchangeAccountConfig>(&account).is_err());
    }
}
