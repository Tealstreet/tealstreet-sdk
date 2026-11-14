use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::Position;
use serde_json::{Value, json};

fn row(notional: Value) -> Value {
    json!({
            "id":"p", "symbol":"PF_XBTUSD", "side":"long", "entryPrice":100.0,
            "notional":notional, "leverage":5.0, "unrealizedPnl":0.0,
            "contracts":2.0, "liquidationPrice":null
    })
}

fn check_unknown(position: Position) {
    assert_eq!(
        serde_json::to_value(&position).unwrap()["notional"],
        Value::Null
    );
    assert_eq!(position.contracts, 2.0);
}

#[test]
fn nullable_notional_raw_ingress_preserves_unknown() {
    check_unknown(js_from_str::<Position>(&row(Value::Null).to_string()).unwrap());
}

#[test]
fn nullable_notional_owned_ingress_preserves_unknown() {
    check_unknown(js_from_value::<Position>(row(Value::Null)).unwrap());
}

#[test]
fn nullable_notional_borrowed_ingress_preserves_unknown() {
    check_unknown(js_from_value_ref::<Position>(&row(Value::Null)).unwrap());
}

#[test]
fn known_notional_controls_remain_numeric_through_all_three_ingress_paths() {
    for notional in [json!(0.0), json!(250.0)] {
        let row = row(notional.clone());
        let raw = row.to_string();
        for position in [
            js_from_str::<Position>(&raw).unwrap(),
            js_from_value::<Position>(row.clone()).unwrap(),
            js_from_value_ref::<Position>(&row).unwrap(),
        ] {
            assert_eq!(
                serde_json::to_value(&position).unwrap()["notional"],
                notional
            );
            assert_eq!(position.contracts, 2.0);
        }
    }
}

#[test]
fn native_position_leverage_cannot_turn_unknown_valuation_into_zero() {
    use safe_cex_num::sizing::calculate_position_account_leverage;
    for unknown in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for balance in [0.0, 1000.0] {
            assert!(calculate_position_account_leverage(unknown, balance).is_nan());
        }
    }
    assert_eq!(calculate_position_account_leverage(0.0, 1000.0), 0.0);
    assert_eq!(calculate_position_account_leverage(250.0, 1000.0), 0.25);
    assert_eq!(calculate_position_account_leverage(250.0, 0.0), 0.0);
}

#[test]
fn native_total_leverage_cannot_turn_incomplete_exposure_into_zero() {
    use safe_cex_num::sizing::calculate_total_account_leverage;
    for unknown in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for balance in [0.0, 1000.0] {
            assert!(calculate_total_account_leverage(&[unknown, 250.0], balance).is_nan());
        }
    }
    assert_eq!(
        calculate_total_account_leverage(&[0.0, 250.0], 1000.0),
        0.25
    );
    assert_eq!(calculate_total_account_leverage(&[], 1000.0), 0.0);
}

#[test]
fn native_total_notional_keeps_unavailable_inputs_and_overflow_unknown() {
    use safe_cex_num::sizing::calculate_total_notional;
    for unknown in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(calculate_total_notional(&[unknown, 250.0]).is_nan());
    }
    assert!(calculate_total_notional(&[f64::MAX, f64::MAX]).is_nan());
    assert_eq!(calculate_total_notional(&[0.0, -250.0]), 250.0);
    assert_eq!(calculate_total_notional(&[]), 0.0);
}
