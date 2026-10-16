use safe_cex_types::{OrderType, PlaceOrderOpts};
use safe_cex_venue_binance::{order_logic, pm_order_logic};
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn order_classification_and_hedge_side_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/order-logic.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 217, "SOURCE-CARDINALITY order_logic total");
    let mut completed_comparisons = 0;
    for (index, row) in rows.iter().enumerate() {
        let input = row.get("input");
        let actual = match row["kind"].as_str().unwrap() {
            "position" => {
                let opts: PlaceOrderOpts = serde_json::from_value(input.unwrap().clone()).unwrap();
                let hedged = row["isHedged"].as_bool().unwrap();
                json!(if row["variant"] == "binance" {
                    order_logic::get_order_position_side(hedged, &opts)
                } else {
                    pm_order_logic::get_order_position_side(hedged, &opts)
                })
            }
            "algo" => {
                let order_type: OrderType = serde_json::from_value(input.unwrap().clone()).unwrap();
                json!(order_logic::is_algo_order_type(order_type))
            }
            "error" => json!(order_logic::is_binance_position_mode_already_set_error(
                input
            )),
            "balance" => json!(order_logic::detect_balance_issue(input)),
            _ => panic!("unknown oracle row"),
        };
        assert_eq!(actual, row["expected"], "case {index}: {row}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 217,
        "NATIVE-CARDINALITY order_logic completed comparisons"
    );
    eprintln!("{} live order logic comparisons", rows.len());
}
