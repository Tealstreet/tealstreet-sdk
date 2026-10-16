//! Exact source literal sets, native variants and valid tagged-object refusals.
use safe_cex_venue_binance::{
    binance_types::BinanceIncomeType as Income,
    protocol_types::{ApiType, BinanceOrderBookApplyResult as Apply},
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

fn decode<T: DeserializeOwned>(value: &Value, route: usize) -> Result<T, serde_json::Error> {
    match route {
        0 => serde_json::from_value(value.clone()),
        1 => T::deserialize(value),
        2 => <T as serde::Deserialize>::deserialize(value.clone()),
        3 => serde_json::from_str(&serde_json::to_string(value).unwrap()),
        _ => unreachable!(),
    }
}

fn string_enum<T: DeserializeOwned + Serialize + PartialEq + Debug>(cases: &[(&str, T)]) {
    for (tag, expected) in cases {
        let mut tagged = serde_json::Map::new();
        tagged.insert((*tag).into(), Value::Null);
        let tagged = Value::Object(tagged);
        for route in 0..4 {
            let decoded: T = decode(&json!(tag), route).unwrap();
            assert_eq!(
                &decoded, expected,
                "native variant before serialization: {tag} route={route}"
            );
            assert_eq!(
                serde_json::to_value(&decoded).unwrap(),
                json!(tag),
                "existing wire spelling: {tag} route={route}"
            );
            assert!(
                decode::<T>(&tagged, route).is_err(),
                "valid unit tag accepted as object: {tag} route={route}"
            );
            let wrong_case = if tag.to_lowercase() == *tag {
                tag.to_uppercase()
            } else {
                tag.to_lowercase()
            };
            for invalid in [
                json!([tag]),
                Value::Null,
                json!(false),
                json!(17),
                json!(""),
                json!("UNKNOWN"),
                json!(wrong_case),
                json!(format!(" {tag}")),
                json!(format!("{tag} ")),
                json!(format!("\u{feff}{tag}")),
            ] {
                assert!(
                    decode::<T>(&invalid, route).is_err(),
                    "non-admitted string/shape accepted: {invalid} route={route}"
                );
            }
        }
    }
    // The real Serde derive inventories accepted rename/alias spellings here.
    let error = decode::<T>(&json!("__not_a_tag__"), 2)
        .unwrap_err()
        .to_string();
    let tags: Vec<_> = cases.iter().map(|(tag, _)| format!("`{tag}`")).collect();
    let expected = if tags.len() == 2 {
        format!(
            "unknown variant `__not_a_tag__`, expected {} or {}",
            tags[0], tags[1]
        )
    } else {
        format!(
            "unknown variant `__not_a_tag__`, expected one of {}",
            tags.join(", ")
        )
    };
    assert_eq!(
        error, expected,
        "complete actual Serde rename/alias inventory"
    );
}

#[test]
fn income_type_rejects_each_valid_tagged_object() {
    // packages/safe-cex/src/exchanges/binance/binance.types.ts::BinanceIncomeType
    string_enum(&[
        ("TRANSFER", Income::TRANSFER),
        ("WELCOME_BONUS", Income::WelcomeBonus),
        ("REALIZED_PNL", Income::RealizedPnl),
        ("FUNDING_FEE", Income::FundingFee),
        ("COMMISSION", Income::COMMISSION),
        ("INSURANCE_CLEAR", Income::InsuranceClear),
        ("REFERRAL_KICKBACK", Income::ReferralKickback),
        ("COMMISSION_REBATE", Income::CommissionRebate),
        ("API_REBATE", Income::ApiRebate),
        ("CONTEST_REWARD", Income::ContestReward),
        ("CROSS_COLLATERAL_TRANSFER", Income::CrossCollateralTransfer),
        ("OPTIONS_PREMIUM_FEE", Income::OptionsPremiumFee),
        ("OPTIONS_SETTLE_PROFIT", Income::OptionsSettleProfit),
        ("INTERNAL_TRANSFER", Income::InternalTransfer),
        ("AUTO_EXCHANGE", Income::AutoExchange),
        ("DELIVERED_SETTELMENT", Income::DeliveredSettelment),
        ("COIN_SWAP_DEPOSIT", Income::CoinSwapDeposit),
        ("COIN_SWAP_WITHDRAW", Income::CoinSwapWithdraw),
        (
            "POSITION_LIMIT_INCREASE_FEE",
            Income::PositionLimitIncreaseFee,
        ),
    ]);
}

#[test]
fn api_type_rejects_each_valid_tagged_object() {
    // packages/safe-cex/src/exchanges/binance/binancepm.api.ts::ApiType
    string_enum(&[("papi", ApiType::Papi), ("fapi", ApiType::Fapi)]);
}

#[test]
fn orderbook_apply_rejects_each_valid_tagged_object() {
    // packages/safe-cex/src/exchanges/binance/binance.orderbook.ts::BinanceOrderBookApplyResult
    string_enum(&[
        ("applied", Apply::Applied),
        ("ignored", Apply::Ignored),
        ("gap", Apply::Gap),
    ]);
}
