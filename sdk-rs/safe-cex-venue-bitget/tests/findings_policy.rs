use safe_cex_venue_bitget::{
    income::map_bitget_bills,
    orders::{map_inverse_order, map_order},
};
use serde_json::{Value, json};
const KEYS: [&str; 12] = [
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "hasOwnProperty",
    "__lookupGetter__",
    "__lookupSetter__",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toString",
    "valueOf",
    "__proto__",
    "toLocaleString",
];
fn source(script: &str) -> Value {
    let output = std::process::Command::new("node")
        .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
        .arg("--finding-policy")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual-source policy {script}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn inherited_order_fields_and_native_info_input_have_separate_prices() {
    for (inverse, script) in [
        (false, "order-oracle.cjs"),
        (true, "inverse-order-capture.mjs"),
    ] {
        let reference = source(script);
        assert_eq!(
            reference["prototype"].as_array().unwrap().len(),
            12,
            "independent inherited order floor"
        );
        assert_eq!(
            reference["utf16"].as_array().unwrap().len(),
            2,
            "independent native symbol floor"
        );
        assert_eq!(
            reference["nativeInfoNaN"], true,
            "source retains native NaN BEFORE projection"
        );
        for (i, key) in KEYS.into_iter().enumerate() {
            let input = json!({"orderId":"x","status":key,"orderType":"limit","side":"buy","symbol":"BTCUSD","size":"1","price":"1"});
            let result = if inverse {
                map_inverse_order(
                    Some(&input.clone().into()),
                    "coin-futures",
                    |_| None,
                    || 1.0,
                )
            } else {
                map_order(Some(&input.clone().into()), "usdt-futures", || 1.0)
            };
            let Err(error) = result else {
                panic!("BITGET-NATIVE-CLOSED-OUTPUT must refuse whole result");
            };
            assert_eq!(
                error.name, "RepresentationError",
                "native inherited order refusal {inverse}/{key}"
            );
            assert_eq!(reference["prototype"][i]["key"], key);
            assert_eq!(
                reference["prototype"][i]["kind"],
                if key == "__proto__" {
                    "object"
                } else {
                    "function"
                }
            );
        }
        let input = json!({"orderId":"x","status":"new","orderType":"limit","side":"buy","symbol":"BTCUSD","size":"1","price":"1","nested":{"amount":f64::NAN}});
        assert!(
            input["nested"]["amount"].is_null(),
            "BITGET-NATIVE-NAN-INPUT Value has already excluded NaN BEFORE mapper"
        );
        let records = if inverse {
            map_inverse_order(
                Some(&input.clone().into()),
                "coin-futures",
                |_| None,
                || 1.0,
            )
        } else {
            map_order(Some(&input.clone().into()), "usdt-futures", || 1.0)
        }
        .unwrap();
        assert_eq!(records.len(), 1, "ordinary record positive control");
        assert!(
            records[0]
                .info
                .as_ref()
                .unwrap()
                .get("nested")
                .unwrap()
                .get("amount")
                .unwrap()
                .is_null(),
            "this is an input-domain cost, not a mapper numeric repair"
        );
        for row in reference["utf16"].as_array().unwrap() {
            let units = row["units"].as_array().unwrap();
            assert!(
                units
                    .iter()
                    .any(|v| matches!(v.as_u64(), Some(0xd800 | 0xdc00))),
                "actual returned materialized source unit"
            );
            assert!(
                serde_json::from_str::<String>(row["escaped"].as_str().unwrap()).is_err(),
                "required String cannot retain the native unit; no mapper result is produced"
            );
        }
    }
}
#[test]
fn inherited_income_category_omission_keeps_the_open_transaction_id() {
    let reference = source("history-capture.cjs");
    assert_eq!(
        reference["prototype"].as_array().unwrap().len(),
        12,
        "independent inherited bill floor"
    );
    for (i, key) in KEYS.into_iter().enumerate() {
        let input = json!({"cTime":"1700000000123","businessType":key,"billId":"bill-1","coin":"USDT","symbol":"BTCUSDT","amount":"12.5","fee":"-0.2"});
        let records = map_bitget_bills(&[input]).unwrap();
        assert_eq!(records.len(), 1, "native inherited bill is retained");
        assert_eq!(
            records[0].category, None,
            "BITGET-NATIVE-OPTIONAL-CATEGORY {key}"
        );
        assert_eq!(
            records[0].income.to_bits(),
            12.3f64.to_bits(),
            "native admitted number unaffected"
        );
        assert_eq!(
            records[0].tran_id,
            reference["prototype"][i]["tranId"].as_str().unwrap(),
            "BITGET-NATIVE-OPEN-STRING {key}"
        );
        assert_eq!(
            reference["prototype"][i]["kind"],
            if key == "__proto__" {
                "object"
            } else {
                "function"
            }
        );
    }
    for row in reference["utf16"].as_array().unwrap() {
        assert!(
            matches!(row["unit"].as_u64(), Some(0xd800 | 0xdc00)),
            "returned native ledger string retains unit"
        );
        assert!(
            serde_json::from_str::<Value>(row["escaped"].as_str().unwrap()).is_err(),
            "whole bill ingress refuses the required String unit"
        );
    }
}
#[test]
fn bill_page_native_string_result_price_is_not_optional_category_policy() {
    let reference = source("bill-page-capture.cjs");
    assert_eq!(
        reference["utf16"].as_array().unwrap().len(),
        2,
        "independent page string floor"
    );
    for row in reference["utf16"].as_array().unwrap() {
        assert_eq!(row["requests"], 1, "source issues the actual page callback");
        assert!(matches!(row["unit"].as_u64(), Some(0xd800 | 0xdc00)));
        assert!(
            serde_json::from_str::<Value>(row["escaped"].as_str().unwrap()).is_err(),
            "whole parsed response/bill refuses source returned string"
        );
    }
}
