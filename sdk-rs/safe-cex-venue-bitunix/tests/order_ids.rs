use safe_cex_venue_bitunix::order_ids::{get_virtual_tpsl_leg, get_virtual_tpsl_parent_order_id};
use serde_json::Value;
use std::process::Command;

#[test]
fn actual_typescript_virtual_order_ids() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/order-ids-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("execute actual TS order ID helpers");
    assert!(
        output.status.success(),
        "actual TS helpers: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 25, "order ID corpus census");
    let mut refused = 0;
    let mut leg_types = 0;
    let mut leg_values = 0;
    let mut parents = 0;
    for row in rows {
        let units: Vec<u16> = serde_json::from_value(row["units"].clone()).unwrap();
        match String::from_utf16(&units) {
            Ok(id) => {
                leg_types += 1;
                assert_eq!(
                    if get_virtual_tpsl_leg(&id).is_some() {
                        "string"
                    } else {
                        "undefined"
                    },
                    row["legType"].as_str().unwrap(),
                    "virtual leg typeof {id:?}"
                );
                leg_values += 1;
                assert_eq!(
                    get_virtual_tpsl_leg(&id),
                    row["leg"].as_str(),
                    "virtual leg {id:?}"
                );
                let expected: Vec<u16> =
                    serde_json::from_value(row["parentUnits"].clone()).unwrap();
                parents += 1;
                assert_eq!(
                    get_virtual_tpsl_parent_order_id(&id)
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    expected,
                    "virtual parent {id:?}"
                );
            }
            Err(_) => {
                refused += 1;
                let expected: Vec<u16> =
                    serde_json::from_value(row["parentUnits"].clone()).unwrap();
                assert_eq!(
                    expected,
                    units[..units.len() - 4],
                    "source preserves lone surrogate prefix"
                );
                assert!(
                    row["leg"].as_str().is_some(),
                    "source accepts lone surrogate ID"
                );
            }
        }
    }
    assert_eq!(leg_types, 23, "ID native leg type comparison count");
    assert_eq!(leg_values, 23, "ID native leg value comparison count");
    assert_eq!(parents, 23, "ID native parent comparison count");
    assert_eq!(refused, 2, "order ID UTF-8 refusals");
}
