#[path = "support/input_identity.rs"]
mod input_identity;
#[path = "support/retained_input.rs"]
mod retained_input;
use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::IncomeCategory;
use safe_cex_venue_weex::{income::normalize_income, utils::WeexUtilityInput};
use serde_json::Value;
use std::{path::Path, process::Command};
#[test]
fn income_records_match_actual_typescript() {
    let output = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/income-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(corpus["header"]["arch"].is_string());
    assert!(corpus["header"]["nodeMajor"].is_number());
    assert!(corpus["header"]["node"].is_string());
    assert_eq!(corpus["header"]["nanBits"], "7ff8000000000000");
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 34, "income::source_cases");
    let names = [
        "distinct",
        "aliases",
        "empty",
        "null",
        "primitive",
        "zero",
        "numeric",
        "false_id",
        "id_fallback",
        "empty_id",
        "coercion",
        "symbol_error",
        "income_error",
        "asset_error",
        "time_error",
        "id_error",
        "trade_error",
    ];
    let categories = [
        "REALIZED_PNL",
        "DELIVERED_SETTELMENT",
        "FUNDING_FEE",
        "COMMISSION",
        "OPTIONS_PREMIUM_FEE",
        "POSITION_LIMIT_INCREASE_FEE",
        "COMMISSION_REBATE",
        "REFERRAL_KICKBACK",
        "API_REBATE",
        "TRANSFER",
        "INTERNAL_TRANSFER",
        "CROSS_COLLATERAL_TRANSFER",
        "COIN_SWAP_DEPOSIT",
        "COIN_SWAP_WITHDRAW",
        "AUTO_EXCHANGE",
        "INSURANCE_CLEAR",
        "UNKNOWN",
    ];
    let (mut compared, mut success, mut fields, mut errors) = (0, 0, 0, 0);
    for (index, case) in cases.iter().enumerate() {
        let name = if index < 17 {
            names[index].to_owned()
        } else {
            format!("category_{}", categories[index - 17])
        };
        assert_eq!(case["id"], name, "income::source_identity");
        for route in 0..4 {
            let raw = case["raw"].as_str().unwrap();
            let input: WeexUtilityInput = match route {
                0 => serde_json::from_str(raw).unwrap(),
                1 => js_from_str(raw).unwrap(),
                2 => js_from_value(serde_json::from_str(raw).unwrap()).unwrap(),
                _ => js_from_value_ref(&serde_json::from_str(raw).unwrap()).unwrap(),
            };
            let label = format!("income::{name}::route_{route}");
            match normalize_income(&input, |v| v) {
                Err(e) => {
                    assert_eq!(
                        e.name,
                        case["error"]["name"].as_str().unwrap(),
                        "{label}::error_name"
                    );
                    assert_eq!(
                        e.message,
                        case["error"]["message"].as_str().unwrap(),
                        "{label}::error_message"
                    );
                    errors += 1;
                }
                Ok(value) => {
                    let expected = &case["result"];
                    let has_trade_id = [0, 1, 6, 8, 10].contains(&index);
                    assert_eq!(
                        value.trade_id.is_some(),
                        has_trade_id,
                        "{label}::native_trade_presence"
                    );
                    assert_eq!(
                        expected.get("tradeId").is_some(),
                        has_trade_id,
                        "{label}::source_trade_presence"
                    );
                    assert_eq!(
                        value.symbol,
                        expected["symbol"].as_str().unwrap(),
                        "{label}::symbol"
                    );
                    assert_eq!(
                        value.income_type,
                        expected["incomeType"].as_str().unwrap(),
                        "{label}::incomeType"
                    );
                    assert_eq!(
                        value.asset,
                        expected["asset"].as_str().unwrap(),
                        "{label}::asset"
                    );
                    assert_eq!(
                        value.tran_id,
                        expected["tranId"].as_str().unwrap(),
                        "{label}::tranId"
                    );
                    assert_eq!(
                        value.trade_id.as_deref(),
                        expected.get("tradeId").map(|v| v.as_str().unwrap()),
                        "{label}::tradeId"
                    );
                    let category = match expected["category"].as_str().unwrap() {
                        "realized_pnl" => IncomeCategory::RealizedPnl,
                        "funding" => IncomeCategory::Funding,
                        "commission" => IncomeCategory::Commission,
                        "rebate" => IncomeCategory::Rebate,
                        "transfer" => IncomeCategory::Transfer,
                        "liquidation" => IncomeCategory::Liquidation,
                        "other" => IncomeCategory::Other,
                        other => panic!("unexpected category {other}"),
                    };
                    assert_eq!(value.category, Some(category), "{label}::category");
                    for (field, n) in [("income", value.income), ("timestamp", value.timestamp)] {
                        assert_eq!(
                            format!("{:016x}", n.to_bits()),
                            expected[field]["bits"].as_str().unwrap(),
                            "{label}::{field}"
                        );
                        fields += 1;
                    }
                    assert_eq!(
                        expected.as_object().unwrap().len(),
                        if has_trade_id { 8 } else { 7 },
                        "{label}::source_fields"
                    );
                    success += 1;
                }
            }
            compared += 1;
        }
    }
    assert_eq!(compared, 136, "income::compared");
    assert_eq!(success, 108, "income::successes");
    assert_eq!(errors, 28, "income::errors");
    assert_eq!(fields, 216, "income::numeric_fields");
    input_identity::check(&corpus, 0xd86dee57ede8cc92, "income");
    let refusals = corpus["refusals"].as_array().unwrap();
    assert_eq!(refusals.len(), 2, "income::refusal_count");
    let expected = [
        r#"{"symbol":"\ud800"}"#.to_owned(),
        format!("{{\"extra\":{}0{}}}", "[".repeat(150), "]".repeat(150)),
    ];
    for (index, case) in refusals.iter().enumerate() {
        assert_eq!(case["raw"], expected[index], "income::refusal_identity");
        assert_eq!(case["accepted"], true, "income::source_refusal_acceptance");
        if index == 0 {
            let native = retained_input::admit_utf16(&expected[index]);
            assert!(
                normalize_income(&native, |value| value).is_err(),
                "UTF-8 model symbol refusal remains visible"
            );
        } else {
            assert!(
                js_from_str::<WeexUtilityInput>(&expected[index]).is_err(),
                "income::depth_refusal"
            );
        }
    }
    assert_eq!(
        corpus["callable"]["bits"], "401d000000000000",
        "income::callable"
    );
}
