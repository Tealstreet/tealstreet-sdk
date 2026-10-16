use safe_cex_num::{JsCoercionError, numeric_ingress::js_from_str};
use safe_cex_types::Order;
use safe_cex_venue_bitunix::order_links::{
    get_attached_virtual_tpsl_leg, get_tpsl_raw_order_id, is_position_tpsl_order,
    is_standalone_tpsl_order,
};
use serde_json::{Value, json};
use std::process::Command;
fn string_result(value: Result<String, JsCoercionError>) -> Value {
    match value {
        Ok(v) => json!({"type":"string","units":v.encode_utf16().collect::<Vec<_>>()}),
        Err(_) => json!({"error":"TypeError"}),
    }
}
#[test]
fn actual_typescript_order_links() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/order-links-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ORDER LINK actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 250, "ORDER LINK source total");
    let mut compared = [0; 4];
    let mut refused = 0;
    let mut parent_count = 0;
    let mut zero_count = 0;
    for (index, row) in rows.iter().enumerate() {
        let expected = if index < 196 {
            [0, 1, 1, 2, 2, 1, 1][index % 7]
        } else {
            1
        };
        assert_eq!(
            row["orders"].as_array().unwrap().len(),
            expected,
            "ORDER LINK source parent count {index}"
        );
        let raw = row["raw"].as_str().unwrap();
        let order: Order = match js_from_str(raw) {
            Ok(v) => v,
            Err(_) => {
                assert!(
                    raw.contains(r#""id":"\ud800""#),
                    "ORDER LINK only UTF8 refusal"
                );
                assert_eq!(
                    row["rawId"],
                    json!({"type":"string","units":[55296]}),
                    "ORDER LINK source surrogate ID"
                );
                refused += 1;
                continue;
            }
        };
        let parents_raw = format!(
            "[{}]",
            row["orders"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(",")
        );
        let parents: Vec<Order> = js_from_str(&parents_raw).unwrap();
        assert_eq!(
            parents.len(),
            expected,
            "ORDER LINK independent native parent length {index}"
        );
        parent_count += parents.len();
        if index >= 196 && (index - 196) % 3 == 1 {
            assert_eq!(
                order.amount.to_bits(),
                (-0.0f64).to_bits(),
                "ORDER LINK native signed zero amount"
            );
            zero_count += 1;
        }
        assert_eq!(
            string_result(get_tpsl_raw_order_id(&order)),
            row["rawId"],
            "ORDER LINK native raw ID {index}"
        );
        compared[0] += 1;
        let attached = get_attached_virtual_tpsl_leg(&order, &parents).map_or_else(
            || json!({"type":"undefined"}),
            |v| json!({"type":"string","units":v.encode_utf16().collect::<Vec<_>>()}),
        );
        assert_eq!(
            attached, row["attached"],
            "ORDER LINK native attached {index}"
        );
        compared[1] += 1;
        assert_eq!(
            json!({"type":"boolean","value":is_standalone_tpsl_order(&order,&parents)}),
            row["standalone"],
            "ORDER LINK native standalone {index}"
        );
        compared[2] += 1;
        let position = match is_position_tpsl_order(&order) {
            Ok(v) => json!({"type":"boolean","value":v}),
            Err(_) => json!({"error":"TypeError"}),
        };
        assert_eq!(
            position, row["position"],
            "ORDER LINK native position {index}"
        );
        compared[3] += 1;
    }
    assert_eq!(
        compared, [247; 4],
        "ORDER LINK independent native comparisons"
    );
    assert_eq!(parent_count, 275, "ORDER LINK native parent total");
    assert_eq!(zero_count, 17, "ORDER LINK signed zero comparisons");
    assert_eq!(refused, 3, "ORDER LINK UTF8 refusal count");
}
