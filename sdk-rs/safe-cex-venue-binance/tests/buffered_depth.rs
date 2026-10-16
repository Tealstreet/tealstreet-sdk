use safe_cex_num::number_from_string;
use safe_cex_venue_binance::orderbook::{BinanceDepthUpdate, get_buffered_binance_depth_updates};
use serde_json::{Value, json};

#[test]
fn depth_update_wire_record_rejects_non_maps() {
    for value in [
        json!([]),
        json!([1, 2]),
        json!(null),
        json!("x"),
        json!(1),
        json!(false),
    ] {
        assert!(<BinanceDepthUpdate as serde::Deserialize>::deserialize(value).is_err());
    }
    let value = json!({"u":null,"extra":{"any":[true,"x"]}});
    assert_eq!(
        serde_json::to_value(
            <BinanceDepthUpdate as serde::Deserialize>::deserialize(value.clone()).unwrap()
        )
        .unwrap(),
        value
    );
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn buffered_depth_matches_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/buffered-depth.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 2565, "SOURCE-CARDINALITY buffered_depth total");
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        let updates: Vec<BinanceDepthUpdate> =
            serde_json::from_str(c["input"].as_str().unwrap()).unwrap();
        let result = get_buffered_binance_depth_updates(
            &updates,
            number_from_string(c["snapshot"].as_str().unwrap()),
        );
        let actual = match result {
            Ok(rows) => {
                json!({"indices":rows.iter().map(|row|updates.iter().position(|input|std::ptr::eq(*row,input)).expect("retains source record identity")).collect::<Vec<_>>()})
            }
            Err(e) => json!({"error":e.name}),
        };
        assert_eq!(actual, c["expected"], "case {index}: {c}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 2565,
        "NATIVE-CARDINALITY buffered_depth completed comparisons"
    );
    eprintln!("{} live buffer comparisons", cases.len());
}
