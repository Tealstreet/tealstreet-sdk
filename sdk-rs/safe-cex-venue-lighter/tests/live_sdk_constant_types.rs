use safe_cex_venue_lighter::sdk_constant_types::*;
use serde::Deserialize;
use serde_json::{Value, json};
use std::process::Command;
mod support;
fn script() -> &'static str {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/sdk-constant-types-oracle.cjs"
    )
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_aliases_match_live_typescript() {
    let output = Command::new("node").arg(script()).output().unwrap();
    assert!(
        output.status.success(),
        "SDK literal schema: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "ChainIdValue",
            "ErrorCodeValue",
            "TransactionTypeValue",
            "KlineIntervalValue"
        ],
        "SDK alias inventory"
    );
    macro_rules! strings { ($typ:ty) => {{
        let expected = &oracle[stringify!($typ)];
        assert_eq!(json!(support::enum_inventory::<$typ>()), *expected, "{} accepted metadata", stringify!($typ));
        support::assert_string_only::<$typ>(expected, stringify!($typ));
        for value in expected.as_array().unwrap() {
            let parsed = <$typ>::deserialize(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), *value, "{} literal roundtrip", stringify!($typ));
            for bad in [json!({value.as_str().unwrap():null}), json!({value.as_str().unwrap():[]}), json!({value.as_str().unwrap():{}})] {
                assert!(<$typ>::deserialize(bad).is_err(), "{} public qualified decoder", stringify!($typ));
            }
        }
    }}; }
    strings!(ErrorCodeValue);
    strings!(TransactionTypeValue);
    strings!(KlineIntervalValue);
    let variants = [
        ChainIdValue::Mainnet,
        ChainIdValue::Sepolia,
        ChainIdValue::ArbitrumOne,
        ChainIdValue::ArbitrumSepolia,
        ChainIdValue::Optimism,
        ChainIdValue::OptimismSepolia,
        ChainIdValue::Polygon,
        ChainIdValue::PolygonMumbai,
        ChainIdValue::Base,
        ChainIdValue::BaseSepolia,
    ];
    let exhaustive = variants.map(|value| match value {
        ChainIdValue::Mainnet => 1,
        ChainIdValue::Sepolia => 11155111,
        ChainIdValue::ArbitrumOne => 42161,
        ChainIdValue::ArbitrumSepolia => 421614,
        ChainIdValue::Optimism => 10,
        ChainIdValue::OptimismSepolia => 11155420,
        ChainIdValue::Polygon => 137,
        ChainIdValue::PolygonMumbai => 80001,
        ChainIdValue::Base => 8453,
        ChainIdValue::BaseSepolia => 84532,
    });
    assert_eq!(
        json!(exhaustive),
        oracle["ChainIdValue"],
        "ChainIdValue exhaustive variants"
    );
    assert_eq!(
        json!(variants),
        oracle["ChainIdValue"],
        "ChainIdValue serialization"
    );
    let variants = [
        ErrorCodeValue::NetworkError,
        ErrorCodeValue::Timeout,
        ErrorCodeValue::InvalidParameter,
        ErrorCodeValue::InvalidSignature,
        ErrorCodeValue::InsufficientBalance,
        ErrorCodeValue::OrderNotFound,
        ErrorCodeValue::MarketNotFound,
        ErrorCodeValue::NonceTooLow,
        ErrorCodeValue::NonceTooHigh,
        ErrorCodeValue::TransactionExpired,
        ErrorCodeValue::RateLimitExceeded,
        ErrorCodeValue::AuthenticationFailed,
        ErrorCodeValue::CryptoNotInitialized,
        ErrorCodeValue::InternalError,
        ErrorCodeValue::Unknown,
    ];
    let exhaustive = variants.map(|value| match value {
        ErrorCodeValue::NetworkError => "NETWORK_ERROR",
        ErrorCodeValue::Timeout => "TIMEOUT",
        ErrorCodeValue::InvalidParameter => "INVALID_PARAMETER",
        ErrorCodeValue::InvalidSignature => "INVALID_SIGNATURE",
        ErrorCodeValue::InsufficientBalance => "INSUFFICIENT_BALANCE",
        ErrorCodeValue::OrderNotFound => "ORDER_NOT_FOUND",
        ErrorCodeValue::MarketNotFound => "MARKET_NOT_FOUND",
        ErrorCodeValue::NonceTooLow => "NONCE_TOO_LOW",
        ErrorCodeValue::NonceTooHigh => "NONCE_TOO_HIGH",
        ErrorCodeValue::TransactionExpired => "TX_EXPIRED",
        ErrorCodeValue::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
        ErrorCodeValue::AuthenticationFailed => "AUTH_FAILED",
        ErrorCodeValue::CryptoNotInitialized => "CRYPTO_NOT_INITIALIZED",
        ErrorCodeValue::InternalError => "INTERNAL_ERROR",
        ErrorCodeValue::Unknown => "UNKNOWN",
    });
    assert_eq!(
        json!(exhaustive),
        oracle["ErrorCodeValue"],
        "ErrorCodeValue exhaustive variants"
    );
    assert_eq!(
        json!(variants),
        oracle["ErrorCodeValue"],
        "ErrorCodeValue serialization"
    );
    let variants = [
        TransactionTypeValue::CreateOrder,
        TransactionTypeValue::CancelOrder,
        TransactionTypeValue::Transfer,
        TransactionTypeValue::Withdraw,
        TransactionTypeValue::UpdateApiKey,
    ];
    let exhaustive = variants.map(|value| match value {
        TransactionTypeValue::CreateOrder => "create_order",
        TransactionTypeValue::CancelOrder => "cancel_order",
        TransactionTypeValue::Transfer => "transfer",
        TransactionTypeValue::Withdraw => "withdraw",
        TransactionTypeValue::UpdateApiKey => "update_api_key",
    });
    assert_eq!(
        json!(exhaustive),
        oracle["TransactionTypeValue"],
        "TransactionTypeValue exhaustive variants"
    );
    assert_eq!(
        json!(variants),
        oracle["TransactionTypeValue"],
        "TransactionTypeValue serialization"
    );
    let variants = [
        KlineIntervalValue::OneMinute,
        KlineIntervalValue::FiveMinutes,
        KlineIntervalValue::FifteenMinutes,
        KlineIntervalValue::ThirtyMinutes,
        KlineIntervalValue::OneHour,
        KlineIntervalValue::FourHours,
        KlineIntervalValue::OneDay,
        KlineIntervalValue::OneWeek,
        KlineIntervalValue::OneMonth,
    ];
    let exhaustive = variants.map(|value| match value {
        KlineIntervalValue::OneMinute => "1m",
        KlineIntervalValue::FiveMinutes => "5m",
        KlineIntervalValue::FifteenMinutes => "15m",
        KlineIntervalValue::ThirtyMinutes => "30m",
        KlineIntervalValue::OneHour => "1h",
        KlineIntervalValue::FourHours => "4h",
        KlineIntervalValue::OneDay => "1d",
        KlineIntervalValue::OneWeek => "1w",
        KlineIntervalValue::OneMonth => "1M",
    });
    assert_eq!(
        json!(exhaustive),
        oracle["KlineIntervalValue"],
        "KlineIntervalValue exhaustive variants"
    );
    assert_eq!(
        json!(variants),
        oracle["KlineIntervalValue"],
        "KlineIntervalValue serialization"
    );
    for value in oracle["ChainIdValue"].as_array().unwrap() {
        let expected = value.as_f64().unwrap();
        let parsed = ChainIdValue::deserialize(value.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap().as_f64().unwrap(),
            expected,
            "ChainIdValue Value roundtrip"
        );
        let parsed = ChainIdValue::deserialize(serde::de::value::F64Deserializer::<
            serde::de::value::Error,
        >::new(expected))
        .unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap().as_f64().unwrap(),
            expected,
            "ChainIdValue f64 roundtrip"
        );
    }
    for bad in [
        json!(null),
        json!(false),
        json!(true),
        json!([]),
        json!({}),
        json!({"Mainnet":null}),
        json!("1"),
        json!("11155111"),
        json!(0),
        json!(-1),
        json!(1.5),
        json!(2),
        json!(11155112),
    ] {
        assert!(
            ChainIdValue::deserialize(bad.clone()).is_err(),
            "ChainIdValue rejects {bad}"
        );
    }
    for bad in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        1.0000000000000002,
    ] {
        assert!(
            ChainIdValue::deserialize(
                serde::de::value::F64Deserializer::<serde::de::value::Error>::new(bad)
            )
            .is_err(),
            "ChainIdValue rejects f64 {bad}"
        );
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_alias_source_controls_fail_by_name() {
    for (mutation, witness) in [
        ("widen", "ChainIdValue must remain a closed literal union"),
        ("literal", "ErrorCodeValue accepted metadata"),
        (
            "missing-reexport",
            "constant alias source program diagnostics must be empty: 1",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "sdk_aliases_match_live_typescript",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SDK_ALIAS_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(witness), "{log}");
    }
}
#[test]
#[ignore = "requires Node"]
fn sdk_aliases_require_selected_compiler() {
    let output = Command::new("node")
        .arg(script())
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/absent-alias-compiler"),
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
}
