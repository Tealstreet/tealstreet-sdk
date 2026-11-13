#[path = "support/exchange_metadata_native.rs"]
mod identity;
use safe_cex_types::{ExchangeName, exchange_metadata::*};

#[test]
fn the_current_roster_accepts_options_and_refuses_retired_brokers() {
    let options: ExchangeName = serde_json::from_str("\"binanceoptions\"").unwrap();
    assert_eq!(serde_json::to_value(options).unwrap(), "binanceoptions");
    for retired in ["salsadex", "tealstreet-dex"] {
        assert!(serde_json::from_value::<ExchangeName>(serde_json::json!(retired)).is_err());
    }
}

#[test]
fn full_metadata_native_fields_unified_mobile_and_domains() {
    let table = exchange_metadata();
    let roster: Vec<String> =
        serde_json::from_str(include_str!("fixtures/exchange-metadata-roster.json")).unwrap();
    assert_eq!(roster.len(), 48);
    let mut native_names = table.keys().cloned().collect::<Vec<_>>();
    native_names.sort();
    assert_eq!(native_names, roster);
    let inverse = get_exchange_metadata("binanceinverse", false).unwrap();
    assert_eq!(inverse.exchange_name, ExchangeName::Binanceinverse);
    assert_eq!(inverse.pretty_name, "Binance Inverse");
    assert_eq!(inverse.default_quote, "USD");
    assert_eq!(inverse.latency_domain.as_deref(), Some("dapi.binance.com"));
    assert!(inverse.enabled);
    assert!(inverse.enabled_in_dev);
    assert!(inverse.supported_on_mobile);
    assert!(!inverse.supports_testnet);
    assert_eq!(inverse.requires_tickers_for_markets_loaded, Some(true));
    let grvt = get_exchange_metadata("grvt", false).unwrap();
    assert!(grvt.requires_cors, "native GRVT requiresCors");
    assert!(!grvt.forbid_cors, "native GRVT forbidCors");
    let bybit = get_exchange_metadata("bybit", false).unwrap();
    let unified = get_exchange_metadata("bybit", true).unwrap();
    assert_eq!(bybit.exchange_name, ExchangeName::Bybit);
    assert_eq!(bybit.pretty_name, "Bybit");
    assert!(bybit.supported_on_mobile);
    assert!(bybit.features.orders);
    assert_ne!(bybit, unified);
    assert_eq!(unified.exchange_name, ExchangeName::Bybit);
    assert_eq!(
        get_exchange_metadata("binance", true).unwrap(),
        get_exchange_metadata("binance", false).unwrap()
    );
    assert_eq!(
        get_exchange_metadata("unknown", false).unwrap_err(),
        "Unknown exchange metadata: unknown"
    );
    assert!(!is_exchange_supported_on_mobile("unknown"));
    let domains = latency_domain_to_exchanges();
    assert_eq!(
        get_unique_latency_domains(),
        domains.keys().cloned().collect::<Vec<_>>()
    );
    assert_eq!(exchange_pretty_names()["bybit"], "Bybit");
    let original = get_exchange_metadata("bybit", false).unwrap();
    let mut owned = original.clone();
    owned.pretty_name = "changed".into();
    assert_eq!(
        get_exchange_metadata("bybit", false).unwrap().pretty_name,
        "Bybit"
    );
    // Decode the live TypeScript oracle to native typed records before any wire assertion.
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/exchange_metadata_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                // CI supplies an isolated compiler; this fallback only
                // serves a local run from the repo root.
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for (name, record) in &table {
        identity::assert_exchange_metadata_native(record, &oracle["metadata"][name]);
    }
    identity::assert_exchange_metadata_native(&unified, &oracle["unified"]);
    let native: safe_cex_types::object_map::JsObjectMap<ExchangeMetadata> =
        serde_json::from_value(oracle["metadata"].clone()).unwrap();
    assert_eq!(table, native); // Behavior comparison follows the independent direct native field assertions.
    let native_unified: ExchangeMetadata =
        serde_json::from_value(oracle["unified"].clone()).unwrap();
    assert_eq!(unified, native_unified);
    let expected_domains: Vec<String> = serde_json::from_value(oracle["domains"].clone()).unwrap();
    assert_eq!(get_unique_latency_domains(), expected_domains);
    let expected_groups: Vec<(String, Vec<String>)> =
        serde_json::from_value(oracle["groups"].clone()).unwrap();
    assert_eq!(domains.into_iter().collect::<Vec<_>>(), expected_groups);
}

#[test]
fn bingx_spot_metadata_and_symbol_conversion_stay_separate_and_disabled() {
    let spot: ExchangeName = serde_json::from_str("\"bingxspot\"").unwrap();
    assert_eq!(serde_json::to_value(&spot).unwrap(), "bingxspot");
    let metadata = get_exchange_metadata("bingxspot", false).unwrap();
    assert!(!metadata.enabled);
    assert!(!metadata.enabled_in_dev);
    assert_eq!(metadata.enable_in_preview, Some(false));
    assert_eq!(
        safe_cex_types::symbol_converter::to_binance("BTCUSDT", &spot),
        "BTCUSDT"
    );
    assert_eq!(
        safe_cex_types::symbol_converter::from_binance("BTCUSDT", &spot),
        "BTCUSDT"
    );
}

#[test]
fn bitget_tpsl_context_matches_live_source_and_keeps_static_metadata_conservative() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/bitget_metadata_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
            }),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 96);
    for case in cases {
        let exchange = case["exchange"].as_str().unwrap();
        let options = &case["options"];
        let native = get_exchange_metadata_with_options(
            exchange,
            ExchangeMetadataOptions {
                is_unified: options["isUnified"].as_bool().unwrap(),
                bitget_account_mode: options["bitgetAccountMode"].as_str(),
                tpsl_order_path: options["tpslOrderPath"].as_str(),
            },
        )
        .unwrap();
        identity::assert_exchange_metadata_native(&native, &case["metadata"]);
        let expected: ExchangeMetadata = serde_json::from_value(case["metadata"].clone()).unwrap();
        assert_eq!(native, expected, "{exchange}: {options}");
    }
    // A path-specific resolution must never advertise choices on the static table.
    let table = exchange_metadata();
    for exchange in ["bitget", "bitgetusdc", "bitgetinverse"] {
        let metadata = &table[exchange];
        assert!(metadata.supported_tpsl_trigger_types.is_empty());
        assert_eq!(metadata.default_tp_trigger_type, None);
        assert_eq!(metadata.default_sl_trigger_type, None);
        assert_eq!(get_exchange_metadata(exchange, true).unwrap(), *metadata);
    }
}

#[test]
fn bitget_context_does_not_change_other_venues_or_unknown_name_refusals() {
    for exchange in exchange_metadata().keys() {
        if matches!(exchange.as_str(), "bitget" | "bitgetusdc" | "bitgetinverse") {
            continue;
        }
        for is_unified in [false, true] {
            let metadata = get_exchange_metadata_with_options(
                exchange,
                ExchangeMetadataOptions {
                    is_unified,
                    bitget_account_mode: Some("uta"),
                    tpsl_order_path: Some("position"),
                },
            )
            .unwrap();
            assert_eq!(
                metadata,
                get_exchange_metadata(exchange, is_unified).unwrap()
            );
        }
    }
    assert_eq!(
        get_exchange_metadata_with_options(
            "unknown",
            ExchangeMetadataOptions {
                bitget_account_mode: Some("uta"),
                tpsl_order_path: Some("position"),
                ..ExchangeMetadataOptions::default()
            }
        )
        .unwrap_err(),
        "Unknown exchange metadata: unknown"
    );
}
