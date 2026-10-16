use safe_cex_num::{js_number, js_number_string};
use safe_cex_types::{OrderBook, OrderBookOrders};
use safe_cex_venue_binance::orderbook::{BinanceOrderBookSyncState, apply_binance_depth_update};
use serde_json::{Value, json};
fn number(n: f64) -> Value {
    json!({"number":if n == 0.0 && n.is_sign_negative() { "-0".into() } else { js_number_string(n) }})
}
fn levels(rows: &[OrderBookOrders]) -> Value {
    rows.iter()
        .map(|r| json!({"price":number(r.price),"amount":number(r.amount),"total":number(r.total)}))
        .collect()
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn depth_application_preserves_sequence_errors_and_partial_mutation() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/apply-depth.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 3080, "SOURCE-CARDINALITY apply_depth total");
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let row = serde_json::from_str(case["row"].as_str().unwrap()).unwrap();
        let mut book: OrderBook = serde_json::from_value(case["initial"].clone()).unwrap();
        let mut state = BinanceOrderBookSyncState {
            snapshot_last_update_id: case["state"]
                .get("snapshotLastUpdateId")
                .map(|v| js_number(Some(v)).unwrap()),
            last_applied_update_id: case["state"]
                .get("lastAppliedUpdateId")
                .map(|v| js_number(Some(v)).unwrap()),
        };
        let result = match apply_binance_depth_update(
            &mut book,
            &row,
            &mut state,
            case["previous"].as_bool().unwrap(),
        ) {
            Ok(result) => {
                use safe_cex_venue_binance::protocol_types::BinanceOrderBookApplyResult;
                let native = match result {
                    BinanceOrderBookApplyResult::Applied => "applied",
                    BinanceOrderBookApplyResult::Ignored => "ignored",
                    BinanceOrderBookApplyResult::Gap => "gap",
                };
                assert_eq!(
                    native,
                    case["expected"]["result"]["value"].as_str().unwrap(),
                    "native depth result case {index}"
                );
                json!({"value":result})
            }
            Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
        };
        let mut output_state = serde_json::Map::new();
        if let Some(id) = state.snapshot_last_update_id {
            output_state.insert("snapshotLastUpdateId".into(), number(id));
        }
        if let Some(id) = state.last_applied_update_id {
            output_state.insert("lastAppliedUpdateId".into(), number(id));
        }
        let actual = json!({"result":result,"book":{"bids":levels(&book.bids),"asks":levels(&book.asks)},"state":output_state});
        assert_eq!(
            actual, case["expected"],
            "depth application case {index}: {case}"
        );
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 3080,
        "NATIVE-CARDINALITY apply_depth completed comparisons"
    );
    eprintln!("{} live depth application comparisons", cases.len());
}
