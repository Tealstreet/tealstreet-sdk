use safe_cex_venue_lighter::sdk_constants::*;
use serde_json::{Value, json};
use std::process::Command;
fn number(value: f64) -> Value {
    json!({"$number": format!("{:016x}",value.to_bits())})
}
// Preserve declaration order and duplicate Rust keys; an object would erase both.
fn numbers(entries: &[(&str, f64)]) -> Value {
    json!({"$table": entries.iter().map(|(key, value)| json!([key, number(*value)])).collect::<Vec<_>>()})
}
fn strings(entries: &[(&str, &str)]) -> Value {
    json!({"$table": entries.iter().map(|(key, value)| json!([key, value])).collect::<Vec<_>>()})
}
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-constants-oracle.cjs"
    )
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_constants_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entries = [
        ("ChainId", "CHAIN_ID", numbers(&CHAIN_ID)),
        ("ApiEndpoints", "API_ENDPOINTS", strings(&API_ENDPOINTS)),
        ("WsEndpoints", "WS_ENDPOINTS", strings(&WS_ENDPOINTS)),
        ("Defaults", "DEFAULTS", numbers(&DEFAULTS)),
        ("RateLimits", "RATE_LIMITS", numbers(&RATE_LIMITS)),
        ("HttpStatus", "HTTP_STATUS", numbers(&HTTP_STATUS)),
        ("ErrorCode", "ERROR_CODE", strings(&ERROR_CODE)),
        (
            "TransactionType",
            "TRANSACTION_TYPE",
            strings(&TRANSACTION_TYPE),
        ),
        ("TokenDecimals", "TOKEN_DECIMALS", numbers(&TOKEN_DECIMALS)),
        ("FeePrecision", "FEE_PRECISION", number(FEE_PRECISION)),
        (
            "MaxSlippageBps",
            "MAX_SLIPPAGE_BPS",
            number(MAX_SLIPPAGE_BPS),
        ),
        (
            "MinSlippageBps",
            "MIN_SLIPPAGE_BPS",
            number(MIN_SLIPPAGE_BPS),
        ),
        (
            "DefaultSlippageBps",
            "DEFAULT_SLIPPAGE_BPS",
            number(DEFAULT_SLIPPAGE_BPS),
        ),
        ("BasisPoints", "BASIS_POINTS", number(BASIS_POINTS)),
        (
            "ProtocolVersion",
            "PROTOCOL_VERSION",
            json!(PROTOCOL_VERSION),
        ),
        ("SdkVersion", "SDK_VERSION", json!(SDK_VERSION)),
        ("UserAgent", "USER_AGENT", json!(USER_AGENT)),
        (
            "WsProtocolVersion",
            "WS_PROTOCOL_VERSION",
            json!(WS_PROTOCOL_VERSION),
        ),
        (
            "MaxWsMessageSize",
            "MAX_WS_MESSAGE_SIZE",
            number(MAX_WS_MESSAGE_SIZE),
        ),
        (
            "OrderBookDepth",
            "ORDER_BOOK_DEPTH",
            numbers(&ORDER_BOOK_DEPTH),
        ),
        ("KlineInterval", "KLINE_INTERVAL", strings(&KLINE_INTERVAL)),
        ("ZeroAddress", "ZERO_ADDRESS", json!(ZERO_ADDRESS)),
        (
            "MaxUint256",
            "MAX_UINT256",
            json!({"$bigint":MAX_UINT256.to_string()}),
        ),
        (
            "MaxUint128",
            "MAX_UINT128",
            json!({"$bigint":MAX_UINT128.to_string()}),
        ),
        (
            "MaxUint64",
            "MAX_UINT64",
            json!({"$bigint":MAX_UINT64.to_string()}),
        ),
        ("MaxUint32", "MAX_UINT32", number(MAX_UINT32)),
        ("MaxUint16", "MAX_UINT16", number(MAX_UINT16)),
        ("MaxUint8", "MAX_UINT8", number(MAX_UINT8)),
        ("EnvVars", "ENV_VARS", strings(&ENV_VARS)),
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
    let public_names: Vec<_> = include_str!("../src/sdk_constants.rs")
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
        assert_eq!(actual, oracle[name], "SDK constant {name}");
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_constant_source_mutations_fail_by_name() {
    for (mutation, name) in [
        ("chain", "ChainId"),
        ("version", "SdkVersion"),
        ("bigint", "MaxUint64"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_constants_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_CONSTANT_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(
            log.contains(&format!("SDK constant {name}"))
                && log.contains("assertion `left == right` failed"),
            "{log}"
        );
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_constants_require_selected_compiler() {
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
