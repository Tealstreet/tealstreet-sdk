use safe_cex_types::{OrderType, PlaceOrderOpts};
use safe_cex_venue_hyperliquid::cloid::*;
use serde_json::{Value, json};

fn corpus() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/cloid-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "cloid source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    corpus["cases"].as_array().unwrap().clone()
}

fn bits(value: f64) -> String {
    hex(value.to_bits().to_be_bytes())
}
fn hex(bytes: [u8; 8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn enum_text<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap()
}

#[test]
fn actual_source_cloid_helpers_match_native_values() {
    let cases = corpus();
    assert_eq!(cases.len(), 34, "CLOID fixed source total");
    let mut totals = [0usize; 5];
    for row in &cases {
        match row["operation"].as_str().unwrap() {
            "pattern" => {
                totals[0] += 1;
                let value = row["value"].as_str().unwrap();
                assert_eq!(
                    is_hyperliquid_cloid(value),
                    row["result"].as_bool().unwrap(),
                    "CLOID pattern {value:?}"
                );
            }
            "read" => {
                totals[1] += 1;
                let input: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
                assert_eq!(
                    read_hyperliquid_cloid(Some(&input))
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    row["result"],
                    "CLOID read {input}"
                );
            }
            "rule" => {
                totals[2] += 1;
                let rule = hyperliquid_client_order_id_rule();
                assert_eq!(rule.label, row["label"].as_str().unwrap());
                assert_eq!(rule.requirement, row["requirement"].as_str().unwrap());
                let ids = [
                    "0x0123456789abcdef0123456789abcdef",
                    "0x0123456789ABCDEF0123456789ABCDEF",
                ];
                let valid: Vec<Value> = ids
                    .iter()
                    .map(|id| Value::from((rule.is_valid)(id)))
                    .collect();
                assert_eq!(Value::from(valid), row["valid"]);
                let shape = rule
                    .is_honorable_shape
                    .as_ref()
                    .expect("CLOID honorable-shape override");
                let expected = row["honorable"].as_object().unwrap();
                assert_eq!(expected.len(), 6, "CLOID every OrderType value");
                for (name, honorable) in expected {
                    let r#type: OrderType =
                        serde_json::from_value(Value::from(name.as_str())).unwrap();
                    let mut opts: PlaceOrderOpts = serde_json::from_value(json!({
                        "symbol":"BTC","type":name,"side":"buy","amount":1
                    }))
                    .unwrap();
                    opts.r#type = r#type;
                    assert_eq!(
                        shape(&safe_cex_types::PreparedOrderOpts::from(&opts)),
                        honorable.as_bool().unwrap(),
                        "CLOID honorable {name}"
                    );
                }
                assert_eq!(row["mints"], "function");
                let minted = (rule.mint.as_ref().expect("CLOID mint override"))();
                assert!(is_hyperliquid_cloid(&minted), "CLOID minted shape {minted}");
            }
            "mint" => {
                totals[3] += 1;
                let bytes: Vec<u8> = row["bytes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|byte| byte.as_u64().unwrap() as u8)
                    .collect();
                let bytes: [u8; 16] = bytes.try_into().unwrap();
                assert_eq!(
                    format_hyperliquid_cloid(bytes),
                    row["result"].as_str().unwrap()
                );
            }
            "record" => {
                totals[4] += 1;
                let input: Value = serde_json::from_str(row["raw"].as_str().unwrap()).unwrap();
                let native =
                    placement_record_from_wire(&input, "BTC-PERP".into(), 1_700_000_000_000.0);
                if let Some(error) = row.get("error") {
                    assert_eq!(
                        native.unwrap_err(),
                        error.as_str().unwrap(),
                        "CLOID record error {input}"
                    );
                    continue;
                }
                let native = native.unwrap();
                let expected = &row["record"];
                assert_eq!(
                    enum_text(&native.r#type),
                    expected["type"],
                    "CLOID record type {input}"
                );
                assert_eq!(
                    bits(native.price),
                    expected["price"].as_str().unwrap(),
                    "CLOID price bits {input}"
                );
                assert_eq!(enum_text(&native.side), expected["side"]);
                assert_eq!(
                    bits(native.amount),
                    expected["amount"].as_str().unwrap(),
                    "CLOID amount bits {input}"
                );
                assert_eq!(Value::from(native.reduce_only), expected["reduceOnly"]);
                assert_eq!(
                    native
                        .time_in_force
                        .as_ref()
                        .map(enum_text)
                        .unwrap_or(Value::Null),
                    expected["timeInForce"],
                    "CLOID record tif {input}"
                );
                assert_eq!(native.symbol, expected["symbol"].as_str().unwrap());
                assert_eq!(native.placed_at, expected["placedAt"].as_f64().unwrap());
            }
            other => panic!("unknown operation {other}"),
        }
    }
    assert_eq!(totals, [11, 8, 1, 4, 10], "CLOID per-operation totals");
}

#[test]
fn minted_cloids_are_fresh_lowercase_hex() {
    let first = mint_hyperliquid_cloid().unwrap();
    let second = mint_hyperliquid_cloid().unwrap();
    assert!(is_hyperliquid_cloid(&first) && is_hyperliquid_cloid(&second));
    assert_ne!(first, second, "CLOID two mints from the CSPRNG differ");
}
