#[path = "support/exchange_metadata_native.rs"]
mod identity;
use safe_cex_types::{ExchangeName, exchange_metadata::*};

#[test]
fn full_metadata_native_fields_unified_mobile_and_domains() {
    let table = exchange_metadata();
    assert_eq!(table.len(), 49); // independently counted source object entries, stopped at record 49.
    let inverse = get_exchange_metadata("binanceinverse", false).unwrap();
    assert_eq!(inverse.exchange_name, ExchangeName::Binanceinverse);
    assert_eq!(inverse.pretty_name, "Binance Inverse");
    assert_eq!(inverse.default_quote, "USD");
    assert_eq!(inverse.latency_domain.as_deref(), Some("dapi.binance.com"));
    assert!(!inverse.enabled);
    assert!(inverse.enabled_in_dev);
    assert!(!inverse.supported_on_mobile);
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
