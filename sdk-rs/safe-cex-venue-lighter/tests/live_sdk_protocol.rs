use safe_cex_venue_lighter::sdk_protocol::*;
use serde_json::{Value, json};
use std::process::Command;
fn number(value: f64) -> Value {
    json!({"$number": format!("{:016x}",value.to_bits())})
}
// Preserve declaration order and duplicate Rust keys; an object would erase both.
fn numbers(entries: &[(&str, f64)]) -> Value {
    json!({"$table": entries.iter().map(|(key, value)| json!([key, number(*value)])).collect::<Vec<_>>()})
}
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-protocol-oracle.cjs"
    )
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_protocol_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = [
        ("TxType", "TX_TYPE", numbers(&TX_TYPE)),
        (
            "MaxAccountIndex",
            "MAX_ACCOUNT_INDEX",
            number(MAX_ACCOUNT_INDEX),
        ),
        (
            "MaxApiKeyIndex",
            "MAX_API_KEY_INDEX",
            number(MAX_API_KEY_INDEX),
        ),
        ("MaxNonce", "MAX_NONCE", number(MAX_NONCE)),
        ("MaxExpiry", "MAX_EXPIRY", number(MAX_EXPIRY)),
        (
            "MinOrderSize",
            "MIN_ORDER_SIZE",
            json!({"$bigint":MIN_ORDER_SIZE.to_string()}),
        ),
        (
            "MaxOrderSize",
            "MAX_ORDER_SIZE",
            json!({"$bigint":MAX_ORDER_SIZE.to_string()}),
        ),
        ("PricePrecision", "PRICE_PRECISION", number(PRICE_PRECISION)),
        (
            "QuantityPrecision",
            "QUANTITY_PRECISION",
            number(QUANTITY_PRECISION),
        ),
        (
            "DefaultGasLimit",
            "DEFAULT_GAS_LIMIT",
            number(DEFAULT_GAS_LIMIT),
        ),
        (
            "DefaultGasPriceMultiplier",
            "DEFAULT_GAS_PRICE_MULTIPLIER",
            number(DEFAULT_GAS_PRICE_MULTIPLIER),
        ),
        ("ApiVersion", "API_VERSION", json!(API_VERSION)),
    ];
    assert_eq!(
        oracle
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        entries.iter().map(|e| e.0).collect::<Vec<_>>(),
        "SDK exported declaration inventory"
    );
    let public_names: Vec<_> = include_str!("../src/sdk_protocol.rs")
        .lines()
        .filter_map(|line| {
            line.strip_prefix("pub const ")
                .or_else(|| line.strip_prefix("pub static "))
        })
        .map(|line| line.split(':').next().unwrap())
        .collect();
    assert_eq!(
        public_names,
        entries.iter().map(|e| e.1).collect::<Vec<_>>(),
        "Rust SDK exported constant inventory"
    );
    for (name, _, actual) in entries {
        assert_eq!(actual, oracle[name], "SDK protocol {name}");
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_protocol_source_mutations_fail_by_name() {
    for (mutation, name) in [
        ("tx", "TxType"),
        ("gas", "DefaultGasPriceMultiplier"),
        ("size", "MaxOrderSize"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_protocol_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_PROTOCOL_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(
            log.contains(&format!("SDK protocol {name}"))
                && log.contains("assertion `left == right` failed"),
            "{log}"
        );
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_protocol_require_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/missing-sdk-compiler"),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
