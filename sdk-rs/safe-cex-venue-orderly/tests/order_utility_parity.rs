use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{MarginType, OrderType, PlaceOrderOpts, Position};
use safe_cex_venue_orderly::{margin::resolve_margin_type_for_order, utils::is_algo_order};
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
fn actual_typescript_order_utility_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-utility-oracle.cjs"
        ))
        .output()
        .expect("one Node order-utility corpus");
    assert!(
        output.status.success(),
        "actual source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["rows"].as_array().unwrap().len(),
        512,
        "ORDERLY-CARDINALITY order_utility/rows source total"
    );
    assert_eq!(
        corpus["algo"].as_array().unwrap().len(),
        6,
        "ORDERLY-CARDINALITY order_utility/algo source total"
    );
    eprintln!("order-utility runtime: {}", corpus["runtime"]);
    let mut compared_rows = 0;
    for row in corpus["rows"].as_array().unwrap() {
        let label = row["label"].as_str().unwrap();
        let positions: Vec<Position> = js_from_str(row["positionsJson"].as_str().unwrap()).unwrap();
        let opts: PlaceOrderOpts = js_from_str(row["optsJson"].as_str().unwrap()).unwrap();
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
        let result = resolve_margin_type_for_order(&opts, &cache, &store);
        if row["expected"].get("margin").is_some() {
            assert_eq!(
                result,
                Ok(margin(&row["expected"]["margin"])),
                "resolve-margin/native/{label}"
            );
        } else {
            assert_eq!(
                row["expected"]["name"], "Error",
                "resolve-margin/source-error-kind/{label}"
            );
            assert_eq!(
                result,
                Err(row["expected"]["message"].as_str().unwrap().to_owned()),
                "resolve-margin/error/{label}"
            );
        }
        compared_rows += 1;
    }
    assert_eq!(compared_rows, 512, "ORDERLY-COMPARISONS order_utility/rows");
    let mut compared_algo = 0;
    for row in corpus["algo"].as_array().unwrap() {
        let order_type: OrderType =
            js_from_str(&serde_json::to_string(&row["input"]).unwrap()).unwrap();
        assert_eq!(
            is_algo_order(order_type),
            row["expected"].as_bool().unwrap(),
            "algo/native/{}",
            row["input"].as_str().unwrap()
        );
        compared_algo += 1;
    }
    assert_eq!(compared_algo, 6, "ORDERLY-COMPARISONS order_utility/algo");
    assert_eq!(
        corpus["boundary"]["unit"], 0xd800,
        "source resolve-margin surrogate key"
    );
    assert_eq!(
        corpus["boundary"]["margin"], "isolated",
        "source resolve-margin surrogate result"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "chosen resolve-margin UTF-8 refusal"
    );
}
