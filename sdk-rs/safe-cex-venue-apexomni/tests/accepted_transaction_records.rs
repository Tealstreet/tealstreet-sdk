#![allow(non_snake_case)]
//! Every record has its own native-field witness and complete declaration-order array.
#[path = "support/accepted_transaction_record_observations.rs"]
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
    let fixtures: Value = serde_json::from_str(include_str!(
        "support/accepted_transaction_record_fixtures.json"
    ))
    .unwrap();
    let cases = fixtures[name]["cases"].as_array().unwrap();
    assert_eq!(
        fixtures.as_object().unwrap().len(),
        22,
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
fn TxSignature_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxSignature>(
        "TxSignature",
        10,
        2,
        observations::observe_txsignature,
    );
}

#[test]
fn SignContractPayload_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignContractPayload>(
        "SignContractPayload",
        55,
        40,
        observations::observe_signcontractpayload,
    );
}

#[test]
fn TxContractData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxContractData>(
        "TxContractData",
        74,
        43,
        observations::observe_txcontractdata,
    );
}

#[test]
fn SignedContractData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignedContractData>(
        "SignedContractData",
        10,
        2,
        observations::observe_signedcontractdata,
    );
}

#[test]
fn SignOrderPayload_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignOrderPayload>(
        "SignOrderPayload",
        60,
        45,
        observations::observe_signorderpayload,
    );
}

#[test]
fn TxOrderData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxOrderData>(
        "TxOrderData",
        79,
        48,
        observations::observe_txorderdata,
    );
}

#[test]
fn SignedOrderData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignedOrderData>(
        "SignedOrderData",
        10,
        2,
        observations::observe_signedorderdata,
    );
}

#[test]
fn SignTransferPayload_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignTransferPayload>(
        "SignTransferPayload",
        42,
        33,
        observations::observe_signtransferpayload,
    );
}

#[test]
fn TxTransferData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxTransferData>(
        "TxTransferData",
        44,
        33,
        observations::observe_txtransferdata,
    );
}

#[test]
fn SignedTransferData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignedTransferData>(
        "SignedTransferData",
        10,
        2,
        observations::observe_signedtransferdata,
    );
}

#[test]
fn SignWithdrawPayload_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignWithdrawPayload>(
        "SignWithdrawPayload",
        60,
        49,
        observations::observe_signwithdrawpayload,
    );
}

#[test]
fn TxWithdrawData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxWithdrawData>(
        "TxWithdrawData",
        66,
        46,
        observations::observe_txwithdrawdata,
    );
}

#[test]
fn SignedWithdrawData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignedWithdrawData>(
        "SignedWithdrawData",
        10,
        2,
        observations::observe_signedwithdrawdata,
    );
}

#[test]
fn ChangePubKeyOnchain_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::ChangePubKeyOnchain>(
        "ChangePubKeyOnchain",
        14,
        3,
        observations::observe_changepubkeyonchain,
    );
}

#[test]
fn ChangePubKeyECDSA_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::ChangePubKeyECDSA>(
        "ChangePubKeyECDSA",
        14,
        3,
        observations::observe_changepubkeyecdsa,
    );
}

#[test]
fn ChangePubKeyCREATE2_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::ChangePubKeyCREATE2>(
        "ChangePubKeyCREATE2",
        14,
        3,
        observations::observe_changepubkeycreate2,
    );
}

#[test]
fn SignChangePubKeyPayload_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignChangePubKeyPayload>(
        "SignChangePubKeyPayload",
        50,
        36,
        observations::observe_signchangepubkeypayload,
    );
}

#[test]
fn TxChangePubKeyData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::TxChangePubKeyData>(
        "TxChangePubKeyData",
        49,
        37,
        observations::observe_txchangepubkeydata,
    );
}

#[test]
fn SignedChangePubKeyData_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::transaction_records::SignedChangePubKeyData>(
        "SignedChangePubKeyData",
        12,
        3,
        observations::observe_signedchangepubkeydata,
    );
}

#[test]
fn TxEthSignature_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::signer_records::TxEthSignature>(
        "TxEthSignature",
        15,
        4,
        observations::observe_txethsignature,
    );
}

#[test]
fn CreateOrderOptionsObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::order_records::CreateOrderOptionsObject>(
        "CreateOrderOptionsObject",
        38,
        16,
        observations::observe_createorderoptionsobject,
    );
}

#[test]
fn OrderResponseObject_rejects_complete_positional_array() {
    check::<safe_cex_venue_apexomni::order_records::OrderResponseObject>(
        "OrderResponseObject",
        99,
        73,
        observations::observe_orderresponseobject,
    );
}
