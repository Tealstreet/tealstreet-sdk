use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{MarginType, Position};
use safe_cex_venue_orderly::margin::get_current_margin_type;
use serde_json::Value;
use std::{collections::HashMap, process::Command};
fn margin(value: &Value) -> MarginType {
    match value.as_str().unwrap() {
        "cross" => MarginType::Cross,
        "isolated" => MarginType::Isolated,
        value => panic!("unknown source margin {value}"),
    }
}
#[test]
fn actual_typescript_current_margin_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/current-margin-oracle.cjs"
        ))
        .output()
        .expect("one Node current-margin corpus");
    assert!(
        output.status.success(),
        "actual source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        114,
        "ORDERLY-CARDINALITY current_margin/rows source total"
    );
    eprintln!("current-margin runtime: {}", corpus["runtime"]);
    let mut compared_rows = 0;
    for row in corpus["rows"].as_array().unwrap() {
        let label = row["label"].as_str().unwrap();
        let mut positions: Vec<Position> =
            js_from_str(row["positionsJson"].as_str().unwrap()).unwrap();
        let source_positions: Value =
            serde_json::from_str(row["positionsJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            row["bits"].as_array().unwrap().len(),
            source_positions.as_array().unwrap().len(),
            "ORDERLY-CARDINALITY current-margin/source positions/{label}"
        );
        assert_eq!(
            positions.len(),
            source_positions.as_array().unwrap().len(),
            "ORDERLY-CARDINALITY current-margin/native positions/{label}"
        );
        for index in row["nanIndices"].as_array().unwrap() {
            positions[index.as_u64().unwrap() as usize].contracts = f64::NAN;
        }
        for (index, position) in positions.iter().enumerate() {
            assert_eq!(
                format!(
                    "{:016x}",
                    if position.contracts.is_nan() {
                        0x7ff8000000000000
                    } else {
                        position.contracts.to_bits()
                    }
                ),
                row["bits"][index].as_str().unwrap(),
                "current-margin/contracts-bits/{label}/{index}"
            );
        }
        let cache: HashMap<String, MarginType> = row["cache"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| (entry[0].as_str().unwrap().to_owned(), margin(&entry[1])))
            .collect();
        let store = DefaultStore::new();
        Store::update(
            &store,
            StoreUpdate {
                positions: Some(positions),
                ..StoreUpdate::default()
            },
            false,
        )
        .expect("populate fixture Store");
        assert_eq!(
            get_current_margin_type(row["symbol"].as_str().unwrap(), &cache, &store),
            margin(&row["expected"]),
            "current-margin/native/{label}"
        );
        compared_rows += 1;
    }
    assert_eq!(
        compared_rows, 114,
        "ORDERLY-COMPARISONS current_margin/rows"
    );
    assert_eq!(
        corpus["boundary"]["unit"], 0xd800,
        "source cache surrogate key"
    );
    assert_eq!(
        corpus["boundary"]["expected"], "isolated",
        "source cache surrogate result"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "chosen current-margin UTF-8 refusal"
    );
}
