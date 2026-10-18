use safe_cex_types::Market;
use safe_cex_venue_coinbase::{
    orders::map_coinbase_rest_order, utils::normalize_cfm_contract_size,
};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/orders.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[path = "support/compare.rs"]
mod comparison;
use comparison::compare;
#[test]
fn cfm_size_uses_javascript_number_not_parse_float() {
    for case in oracle()["sizes"].as_array().unwrap() {
        compare(
            &json!(normalize_cfm_contract_size(&case["value"])),
            &case["result"],
            &format!("normalizeCfmContractSize {}", case["value"]),
        );
    }
}
#[test]
fn rest_order_configuration_sizing_and_raw_info() {
    let mut corrected = 0;
    let mut special = std::collections::BTreeSet::new();
    for case in oracle()["cases"].as_array().unwrap() {
        let market: Market = serde_json::from_value(case["market"].clone()).unwrap();
        let result = map_coinbase_rest_order(&case["order"], &market, 12345.0);
        let status = case["order"]["status"].as_str().unwrap();
        if matches!(
            status,
            "constructor" | "__proto__" | "toString" | "valueOf" | "hasOwnProperty"
        ) {
            corrected += 1;
            assert!(result.is_none(), "typed status rejects inherited {status}");
            assert!(case["result"].is_object(), "TS defect still emits order");
            if status == "__proto__" {
                assert_eq!(case["result"]["status"], json!({}));
            } else {
                assert!(
                    case["result"].get("status").is_none(),
                    "JSON.stringify omits inherited function status"
                );
            }
        } else {
            // CHOSEN shared Order serializer: extra is its last declared field.
            // Preserve and compare every other key, including raw info insertion order.
            let mut expected = case["result"].clone();
            if let Some(object) = expected.as_object_mut() {
                let extra = object.shift_remove("extra").expect("TS extra field");
                object.insert("extra".into(), extra);
            }
            let mut actual = json!(&result);
            if let Some(order) = result {
                for (field, value) in [
                    ("price", order.price.to_number()),
                    ("amount", order.amount),
                    ("filled", order.filled),
                    ("remaining", order.remaining),
                    ("lastUpdate", order.last_update),
                ] {
                    project_number(&mut actual, field, value);
                }
                if let Some(value) = order.extra.and_then(|extra| extra.remaining_native) {
                    project_number(&mut actual["extra"], "remainingNative", value);
                }
            }
            for field in ["price", "amount", "filled", "remaining"] {
                if let Some(tag) = expected[field]["$number"].as_str() {
                    special.insert(tag.to_owned());
                }
            }
            compare(&actual, &expected, case["name"].as_str().unwrap());
        }
    }
    assert_eq!(corrected, 5);
    for tag in ["NaN", "Infinity", "-0"] {
        assert!(
            special.contains(tag),
            "missing numeric identity witness {tag}"
        );
    }
}

#[path = "support/special_numbers.rs"]
mod special_numbers;
use special_numbers::project_number;
