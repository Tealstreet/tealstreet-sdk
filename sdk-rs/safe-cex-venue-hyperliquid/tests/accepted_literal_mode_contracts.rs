use safe_cex_num::numeric_ingress::JsNumberOutput;
use safe_cex_venue_hyperliquid::types::{
    HyperliquidUserAbstractionMode, WsUserNonFundingLedgerUpdateType,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;
fn routes<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
) -> [Result<T, serde_json::Error>; 6] {
    let input: Value = serde_json::from_str(raw).unwrap();
    [
        serde_json::from_str(raw),
        T::deserialize(input.clone()),
        T::deserialize(&input),
        safe_cex_num::numeric_ingress::js_from_str(raw),
        safe_cex_num::numeric_ingress::js_from_value(input.clone()),
        safe_cex_num::numeric_ingress::js_from_value_ref(&input),
    ]
}

fn literal<T: DeserializeOwned + Serialize + JsNumberOutput + Debug + PartialEq>(
    tag: &str,
    expected: T,
) {
    let encoded = serde_json::to_string(tag).unwrap();
    for raw in [encoded.clone(), format!(" \n{encoded}\t ")] {
        for result in routes::<T>(&raw) {
            let decoded = result.unwrap();
            assert_eq!(decoded, expected, "native enum variant for {tag}");
            assert_eq!(
                serde_json::to_value(decoded).unwrap(),
                Value::String(tag.into())
            );
        }
    }
    let tagged = serde_json::to_string(&json!({tag: null})).unwrap();
    for result in routes::<T>(&tagged) {
        assert!(result.is_err(), "tagged object widened literal {tag}");
    }
}

fn forbidden<T: DeserializeOwned + Serialize + JsNumberOutput>() {
    for raw in ["null", "true", "0", "[]", "{}", "\"\"", "\"__INVALID__\""] {
        for result in routes::<T>(raw) {
            assert!(result.is_err(), "forbidden literal shape {raw}");
        }
    }
}

#[test]
fn user_abstraction_mode_variants_and_string_only_ingress() {
    literal(
        "unifiedAccount",
        HyperliquidUserAbstractionMode::UnifiedAccount,
    );
    literal(
        "portfolioMargin",
        HyperliquidUserAbstractionMode::PortfolioMargin,
    );
    literal("disabled", HyperliquidUserAbstractionMode::Disabled);
    literal("default", HyperliquidUserAbstractionMode::Default);
    literal(
        "dexAbstraction",
        HyperliquidUserAbstractionMode::DexAbstraction,
    );
    literal("standard", HyperliquidUserAbstractionMode::Standard);
    forbidden::<HyperliquidUserAbstractionMode>();
}

#[test]
fn websocket_ledger_variants_and_string_only_ingress() {
    literal("deposit", WsUserNonFundingLedgerUpdateType::Deposit);
    literal("withdraw", WsUserNonFundingLedgerUpdateType::Withdraw);
    literal("transfer", WsUserNonFundingLedgerUpdateType::Transfer);
    literal("liquidation", WsUserNonFundingLedgerUpdateType::Liquidation);
    forbidden::<WsUserNonFundingLedgerUpdateType>();
}
