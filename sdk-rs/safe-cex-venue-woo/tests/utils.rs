use safe_cex_venue_woo::utils::*;
use serde_json::{Value, json};
#[test]
fn symbols_and_hedge_sides_match_live_typescript() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/utils.json")).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        1557,
        "WOO-CARDINALITY utils source total"
    );
    for case in cases.as_array().unwrap() {
        let input = case["input"].as_str().unwrap();
        let actual = match case["name"].as_str().unwrap() {
            "normalizeSymbol" => json!(normalize_symbol(input)),
            "reverseSymbol" => json!(reverse_symbol(input)),
            "mapWooHedgeTransactionSide" => map_woo_hedge_transaction_side(input).map_or(
                Value::Null,
                |(side, position_side)| json!({"side":side,"positionSide":position_side}),
            ),
            name => panic!("unknown source function {name}"),
        };
        assert_eq!(actual, case["output"], "{case}");
    }
}
