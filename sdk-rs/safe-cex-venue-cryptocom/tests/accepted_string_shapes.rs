//! Independent source literal sets, native variant identities and shape witnesses.
use safe_cex_num::numeric_ingress::{self, JsNumberOutput};
use safe_cex_venue_cryptocom::wire::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

fn paths<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
) -> [Result<T, serde_json::Error>; 6] {
    let input: Value = serde_json::from_str(raw).unwrap();
    [
        serde_json::from_str(raw),
        T::deserialize(input.clone()),
        T::deserialize(&input),
        numeric_ingress::js_from_str(raw),
        numeric_ingress::js_from_value(input.clone()),
        numeric_ingress::js_from_value_ref(&input),
    ]
}
fn check<T: DeserializeOwned + JsNumberOutput + Serialize + Debug + Copy>(
    cases: &[(&str, &str)],
    native: impl Fn(T) -> &'static str,
) {
    for &(tag, expected_variant) in cases {
        let raw = serde_json::to_string(tag).unwrap();
        for result in paths::<T>(&raw) {
            let decoded = result.unwrap();
            assert_eq!(
                native(decoded),
                expected_variant,
                "direct native variant for {tag}"
            );
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                json!(tag),
                "exact source serialization for {tag}"
            );
        }
        let map = json!({tag:null}).to_string();
        for (path, result) in paths::<T>(&map).into_iter().enumerate() {
            assert!(
                result.is_err(),
                "tagged-shape admission for {tag}, path {path}"
            );
        }
        for invalid in [
            format!(" {tag}"),
            format!("{tag} "),
            format!("\u{feff}{tag}"),
            tag.to_lowercase(),
        ] {
            if !cases.iter().any(|(allowed, _)| *allowed == invalid) {
                for result in paths::<T>(&serde_json::to_string(&invalid).unwrap()) {
                    assert!(result.is_err(), "unexpected literal {invalid}");
                }
            }
        }
    }
    for raw in ["null", "true", "0", "[]", "{}", "\"__INVALID__\""] {
        for result in paths::<T>(raw) {
            assert!(result.is_err(), "nonliteral shape {raw}");
        }
    }
}

#[test]
fn string_only_tradeside() {
    check::<CryptocomTradeSide>(&[("BUY", "Buy"), ("SELL", "Sell")], |value| match value {
        CryptocomTradeSide::Buy => "Buy",
        CryptocomTradeSide::Sell => "Sell",
    });
}

#[test]
fn string_only_journaltype() {
    check::<CryptocomJournalType>(&[("TRADING", "Trading")], |value| match value {
        CryptocomJournalType::Trading => "Trading",
    });
}

#[test]
fn string_only_liquidityindicator() {
    check::<CryptocomLiquidityIndicator>(&[("MAKER", "Maker"), ("TAKER", "Taker")], |value| {
        match value {
            CryptocomLiquidityIndicator::Maker => "Maker",
            CryptocomLiquidityIndicator::Taker => "Taker",
        }
    });
}

#[test]
fn string_only_ordertype() {
    check::<CryptocomOrderType>(
        &[
            ("LIMIT", "Limit"),
            ("MARKET", "Market"),
            ("STOP_LOSS", "StopLoss"),
            ("STOP_LIMIT", "StopLimit"),
            ("TAKE_PROFIT", "TakeProfit"),
            ("TAKE_PROFIT_LIMIT", "TakeProfitLimit"),
        ],
        |value| match value {
            CryptocomOrderType::Limit => "Limit",
            CryptocomOrderType::Market => "Market",
            CryptocomOrderType::StopLoss => "StopLoss",
            CryptocomOrderType::StopLimit => "StopLimit",
            CryptocomOrderType::TakeProfit => "TakeProfit",
            CryptocomOrderType::TakeProfitLimit => "TakeProfitLimit",
        },
    );
}

#[test]
fn string_only_timeinforce() {
    check::<CryptocomTimeInForce>(
        &[
            ("GOOD_TILL_CANCEL", "GoodTillCancel"),
            ("FILL_OR_KILL", "FillOrKill"),
            ("IMMEDIATE_OR_CANCEL", "ImmediateOrCancel"),
            ("POST_ONLY", "PostOnly"),
        ],
        |value| match value {
            CryptocomTimeInForce::GoodTillCancel => "GoodTillCancel",
            CryptocomTimeInForce::FillOrKill => "FillOrKill",
            CryptocomTimeInForce::ImmediateOrCancel => "ImmediateOrCancel",
            CryptocomTimeInForce::PostOnly => "PostOnly",
        },
    );
}

#[test]
fn string_only_refpricetype() {
    check::<CryptocomRefPriceType>(
        &[
            ("MARK_PRICE", "Mark"),
            ("INDEX_PRICE", "Index"),
            ("LAST_PRICE", "Last"),
        ],
        |value| match value {
            CryptocomRefPriceType::Mark => "Mark",
            CryptocomRefPriceType::Index => "Index",
            CryptocomRefPriceType::Last => "Last",
        },
    );
}

#[test]
fn string_only_orderstatus() {
    check::<CryptocomOrderStatus>(
        &[
            ("ACTIVE", "Active"),
            ("FILLED", "Filled"),
            ("CANCELED", "Canceled"),
            ("EXPIRED", "Expired"),
            ("PENDING", "Pending"),
            ("REJECTED", "Rejected"),
        ],
        |value| match value {
            CryptocomOrderStatus::Active => "Active",
            CryptocomOrderStatus::Filled => "Filled",
            CryptocomOrderStatus::Canceled => "Canceled",
            CryptocomOrderStatus::Expired => "Expired",
            CryptocomOrderStatus::Pending => "Pending",
            CryptocomOrderStatus::Rejected => "Rejected",
        },
    );
}
