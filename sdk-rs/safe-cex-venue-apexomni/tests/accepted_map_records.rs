#![allow(non_snake_case)]
//! Every record has its own native-field witness and complete declaration-order array.
#[path = "support/accepted_map_record_observations.rs"]
mod observations;
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    route: usize,
) -> Result<T, serde_json::Error> {
    match route {
        0 => serde_json::from_str(raw),
        1 => serde_json::from_slice(raw.as_bytes()),
        2 => serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()),
        3 => <T as serde::Deserialize>::deserialize(&serde_json::from_str::<Value>(raw).unwrap()),
        4 => js_from_str(raw),
        5 => js_from_value(serde_json::from_str::<Value>(raw).unwrap()),
        6 => js_from_value_ref(&serde_json::from_str::<Value>(raw).unwrap()),
        _ => unreachable!(),
    }
}
fn check<T: DeserializeOwned + Serialize + JsNumberOutput>(
    name: &str,
    expected_cases: usize,
    expected_positive: usize,
    observe: impl Fn(&T) -> Value,
) {
    let fixtures: Value =
        serde_json::from_str(include_str!("support/accepted_map_record_fixtures.json")).unwrap();
    let cases = fixtures[name]["cases"].as_array().unwrap();
    assert_eq!(
        fixtures.as_object().unwrap().len(),
        26,
        "fixed reviewed owner census"
    );
    assert_eq!(
        cases.len(),
        expected_cases,
        "{name}: fixed independent case census"
    );
    assert_eq!(
        cases.iter().filter(|case| case["ok"] == true).count(),
        expected_positive,
        "{name}: positive native observations"
    );
    let mut executed = 0;
    for case in cases {
        let input = if let Some(raw) = case["raw"].as_str() {
            raw.to_owned()
        } else {
            let mut fields = fixtures[name]["sampleFieldsRaw"]
                .as_object()
                .unwrap()
                .clone();
            for key in case["omit"].as_array().unwrap() {
                fields.remove(key.as_str().unwrap());
            }
            for (key, raw) in case["setRaw"].as_object().unwrap() {
                fields.insert(key.clone(), raw.clone());
            }
            format!(
                "{{{}}}",
                fields
                    .iter()
                    .map(|(key, raw)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        raw.as_str().unwrap()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let raw = input.as_str();
        for route in 0..7 {
            executed += 1;
            let actual = decode::<T>(raw, route);
            if case["ok"] == true {
                let mut expected = fixtures[name]["expected"].clone();
                for (key, value) in case["expectedSet"].as_object().unwrap() {
                    expected[key] = value.clone();
                }
                let owner = actual.unwrap_or_else(|err| {
                    panic!(
                        "{name} native object {} route={route}: {err}",
                        case["label"]
                    )
                });
                assert_eq!(
                    observe(&owner),
                    expected,
                    "{name} native fields/presence/bits {} route={route}",
                    case["label"]
                );
            } else {
                assert!(
                    actual.is_err(),
                    "{name}: complete valid positional array or invalid record accepted: case={} route={route} input={raw}",
                    case["label"]
                );
            }
        }
    }
    assert_eq!(
        executed,
        expected_cases * 7,
        "{name}: fixed native decoder route census"
    );
}

#[test]
fn TickerObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::TickerObject>(
        "TickerObject",
        10,
        2,
        observations::observe_TickerObject,
    );
}

#[test]
fn DepthObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::DepthObject>(
        "DepthObject",
        15,
        7,
        observations::observe_DepthObject,
    );
}

#[test]
fn TradeObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::TradeObject>(
        "TradeObject",
        15,
        7,
        observations::observe_TradeObject,
    );
}

#[test]
fn KlineObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::KlineObject>(
        "KlineObject",
        15,
        7,
        observations::observe_KlineObject,
    );
}

#[test]
fn HistoryFundingObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::HistoryFundingObject>(
        "HistoryFundingObject",
        20,
        12,
        observations::observe_HistoryFundingObject,
    );
}

#[test]
fn CurrencyObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::CurrencyObject>(
        "CurrencyObject",
        10,
        2,
        observations::observe_CurrencyObject,
    );
}

#[test]
fn GlobalObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::GlobalObject>(
        "GlobalObject",
        35,
        27,
        observations::observe_GlobalObject,
    );
}

#[test]
fn PerpetualContractObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::PerpetualContractObject>(
        "PerpetualContractObject",
        26,
        15,
        observations::observe_PerpetualContractObject,
    );
}

#[test]
fn GetHistoryFundingOptions_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::GetHistoryFundingOptions>(
        "GetHistoryFundingOptions",
        35,
        23,
        observations::observe_GetHistoryFundingOptions,
    );
}

#[test]
fn KeyPair_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::KeyPair>(
        "KeyPair",
        12,
        3,
        observations::observe_KeyPair,
    );
}

#[test]
fn ApiKeyCredentials_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::ApiKeyCredentials>(
        "ApiKeyCredentials",
        10,
        2,
        observations::observe_ApiKeyCredentials,
    );
}

#[test]
fn PerpetualCurrencyObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::public_records::PerpetualCurrencyObject>(
        "PerpetualCurrencyObject",
        10,
        2,
        observations::observe_PerpetualCurrencyObject,
    );
}

#[test]
fn WalletObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::WalletObject>(
        "WalletObject",
        10,
        2,
        observations::observe_WalletObject,
    );
}

#[test]
fn OpenPositionObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::OpenPositionObject>(
        "OpenPositionObject",
        20,
        12,
        observations::observe_OpenPositionObject,
    );
}

#[test]
fn AccountsItem_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::AccountsItem>(
        "AccountsItem",
        15,
        7,
        observations::observe_AccountsItem,
    );
}

#[test]
fn AccountObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::AccountObject>(
        "AccountObject",
        17,
        8,
        observations::observe_AccountObject,
    );
}

#[test]
fn WorstPriceObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::WorstPriceObject>(
        "WorstPriceObject",
        10,
        2,
        observations::observe_WorstPriceObject,
    );
}

#[test]
fn FundingRateObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::FundingRateObject>(
        "FundingRateObject",
        15,
        7,
        observations::observe_FundingRateObject,
    );
}

#[test]
fn HistoricalPNLObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::HistoricalPNLObject>(
        "HistoricalPNLObject",
        19,
        9,
        observations::observe_HistoricalPNLObject,
    );
}

#[test]
fn AccountBalanceObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::private_records::AccountBalanceObject>(
        "AccountBalanceObject",
        10,
        2,
        observations::observe_AccountBalanceObject,
    );
}

#[test]
fn MultiChainObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::MultiChainObject>(
        "MultiChainObject",
        10,
        2,
        observations::observe_MultiChainObject,
    );
}

#[test]
fn SymbolObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::SymbolObject>(
        "SymbolObject",
        10,
        2,
        observations::observe_SymbolObject,
    );
}

#[test]
fn PerpetualObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::PerpetualObject>(
        "PerpetualObject",
        10,
        2,
        observations::observe_PerpetualObject,
    );
}

#[test]
fn SymbolInfoObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::SymbolInfoObject>(
        "SymbolInfoObject",
        79,
        56,
        observations::observe_SymbolInfoObject,
    );
}

#[test]
fn UserObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::UserObject>(
        "UserObject",
        26,
        11,
        observations::observe_UserObject,
    );
}

#[test]
fn OrderObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::config_records::OrderObject>(
        "OrderObject",
        30,
        17,
        observations::observe_OrderObject,
    );
}
