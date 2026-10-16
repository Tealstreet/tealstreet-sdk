use safe_cex_venue_coinbase::types;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;

fn records() -> &'static Value {
    static RECORDS: OnceLock<Value> = OnceLock::new();
    RECORDS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/unions.cjs"))
            .output()
            .expect("Node is required for the Coinbase TypeScript record oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("record oracle JSON")
    })
}

fn ts_numeric_paths() -> &'static Value {
    static PATHS: OnceLock<Value> = OnceLock::new();
    PATHS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/numeric-ingress.cjs"
            ))
            .output()
            .expect("Node is required for the Coinbase numeric ingress oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value =
            serde_json::from_slice(&output.stdout).expect("numeric ingress oracle JSON");
        capture_metadata::assert_capture_metadata(&value, 1);
        value
    })
}

macro_rules! check_negative_zero {
    ($ty:path, $type_name:literal, $field:ident) => {{
        let mut value = records()["legacyRecords"][$type_name].clone();
        value[stringify!($field)] = json!(-0.0);
        let expected = value[stringify!($field)]
            .as_f64()
            .expect("JSON number")
            .to_bits();
        assert_eq!(expected, (-0.0_f64).to_bits());
        let decoded: $ty = serde_json::from_value(value.clone()).expect("typed record decoder");
        assert_eq!(
            decoded.$field.to_bits(),
            expected,
            "{}.{} erased the negative-zero sign",
            $type_name,
            stringify!($field)
        );

        // Exercise the raw JSON token path separately from owned Value decoding.
        value[stringify!($field)] = json!(1.25);
        let raw = value.to_string();
        let marker = format!("\"{}\":1.25", stringify!($field));
        assert!(
            raw.contains(&marker),
            "fixture lacks {}.{}",
            $type_name,
            stringify!($field)
        );
        let raw = raw.replace(&marker, &format!("\"{}\":-0", stringify!($field)));
        let raw_decoded: $ty = serde_json::from_str(&raw).expect("raw typed record decoder");
        assert_eq!(
            raw_decoded.$field.to_bits(),
            expected,
            "raw {}.{} decode erased the negative-zero sign",
            $type_name,
            stringify!($field)
        );
        let parsed_value: Value = serde_json::from_str(&raw).expect("parsed wire Value");
        let parsed_value_decoded: $ty =
            serde_json::from_value(parsed_value).expect("typed decoder from parsed Value");
        assert_eq!(
            parsed_value_decoded.$field.to_bits(),
            (-0.0_f64).to_bits(),
            "parsed-Value {}.{} erased the native negative-zero sign",
            $type_name,
            stringify!($field)
        );

        let ts_case = ts_numeric_paths()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["type"] == $type_name && case["name"] == stringify!($field))
            .unwrap_or_else(|| {
                panic!(
                    "missing live TypeScript path {}.{}",
                    $type_name,
                    stringify!($field)
                )
            });
        assert_eq!(ts_case["negativeZero"], true);
        assert_eq!(ts_case["bits"], "8000000000000000");
    }};
}

macro_rules! check_raw_overflow {
    ($ty:path, $type_name:literal, $field:ident) => {{
        let mut value = records()["legacyRecords"][$type_name].clone();
        value[stringify!($field)] = json!(1.25);
        let raw = value.to_string();
        let marker = format!("\"{}\":1.25", stringify!($field));
        assert!(
            raw.contains(&marker),
            "fixture lacks {}.{}",
            $type_name,
            stringify!($field)
        );
        let raw = raw.replace(&marker, &format!("\"{}\":1e400", stringify!($field)));
        let raw_decode = serde_json::from_str::<$ty>(&raw);
        let owned_value = serde_json::from_str::<Value>(&raw).expect("owned wire Value");
        let owned_decode = serde_json::from_value::<$ty>(owned_value);
        let ts_case = ts_numeric_paths()["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["type"] == $type_name && case["name"] == stringify!($field))
            .unwrap_or_else(|| {
                panic!(
                    "missing live TypeScript path {}.{}",
                    $type_name,
                    stringify!($field)
                )
            });
        assert_eq!(ts_case["overflowAccepted"], true);
        assert_eq!(ts_case["overflowInfinite"], true);
        for (path, rust) in [("raw", raw_decode), ("owned Value", owned_decode)] {
            assert_eq!(
                rust.is_ok(),
                ts_case["overflowAccepted"].as_bool().unwrap(),
                "{}.{} {path} overflow decode: {:?}",
                $type_name,
                stringify!($field),
                rust.as_ref().err()
            );
            if let Ok(decoded) = rust {
                assert_eq!(
                    decoded.$field,
                    f64::INFINITY,
                    "{}.{} {path} overflow value",
                    $type_name,
                    stringify!($field)
                );
            }
        }
    }};
}

#[test]
fn public_numeric_record_decoders_distinguish_negative_zero_ingress_paths() {
    assert_eq!(ts_numeric_paths()["source"], "JSON.parse");
    assert_eq!(ts_numeric_paths()["fields"].as_array().unwrap().len(), 13);

    check_negative_zero!(types::MarketResponse, "MarketResponse", num_products);

    check_negative_zero!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        price_band_percent
    );
    check_negative_zero!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        market_order_percent
    );
    check_negative_zero!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        position_limit_adq_pct
    );
    check_negative_zero!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        base_imf
    );

    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        total_balance_fiat
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        total_balance_crypto
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_trade_fiat
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        allocation
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_trade_crypto
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        unrealized_pnl
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_transfer_fiat
    );
    check_negative_zero!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_transfer_crypto
    );
}

#[test]
fn public_numeric_record_decoders_match_raw_overflow_ingress() {
    check_raw_overflow!(types::MarketResponse, "MarketResponse", num_products);
    check_raw_overflow!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        price_band_percent
    );
    check_raw_overflow!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        market_order_percent
    );
    check_raw_overflow!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        position_limit_adq_pct
    );
    check_raw_overflow!(
        types::IntxInstrumentsResponseItem,
        "IntxInstrumentsResponseItem",
        base_imf
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        total_balance_fiat
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        total_balance_crypto
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_trade_fiat
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        allocation
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_trade_crypto
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        unrealized_pnl
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_transfer_fiat
    );
    check_raw_overflow!(
        types::PortfolioBreakdownResponseBreakdownSpotPositionsItem,
        "PortfolioBreakdownResponseBreakdownSpotPositionsItem",
        available_to_transfer_crypto
    );
}
