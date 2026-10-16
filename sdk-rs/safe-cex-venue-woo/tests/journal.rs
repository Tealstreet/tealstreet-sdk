#[path = "support/execution_field_identity.rs"]
mod native_identity;
use safe_cex_types::{Execution, HistoricalOrder};
use safe_cex_venue_woo::journal::{optional_num, woo_orders_to_executions};
use serde_json::Value;
#[test]
fn journal_matches_live_order_axis_projection() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/journal.json")).unwrap();
    assert_eq!(cases.len(), 29, "WOO-CARDINALITY journal source total");
    for case in cases {
        let orders: Vec<HistoricalOrder> = serde_json::from_value(case["orders"].clone()).unwrap();
        let expected: Vec<Execution> = serde_json::from_value(case["expected"].clone()).unwrap();
        let actual = woo_orders_to_executions(&orders);
        let source = case["expected"].as_array().unwrap();
        assert_eq!(actual.len(), source.len());
        for (value, input) in actual.iter().zip(source) {
            native_identity::check(value, input);
        }
        assert_eq!(actual, expected, "{}", case["name"]);
    }
}
#[test]
fn optional_finite_numbers_keep_zero_and_discard_nonfinite_values() {
    for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(optional_num(Some(number)), None);
    }
    assert_eq!(optional_num(None), None);
    assert_eq!(optional_num(Some(0.0)), Some(0.0));
    assert!(optional_num(Some(-0.0)).unwrap().is_sign_negative());
}
