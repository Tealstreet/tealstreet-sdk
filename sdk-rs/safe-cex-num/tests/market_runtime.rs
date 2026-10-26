use safe_cex_num::numeric_ingress::{js_capture_output, js_from_str};
use safe_cex_types::{Market, retained_json::RetainedJsonInput};

#[test]
fn market_runtime_nested_numbers_retain_native_bits() {
    let market: Market = js_from_str(r#"{"id":"A","symbol":"A","base":"A","quote":"B","active":true,"type":"swap","spot":false,"swap":true,"future":false,"contract":true,"linear":true,"inverse":false,"precision":{"amount":0.1,"price":0.2,"base":-0,"quote":1e400},"limits":{"amount":{"min":0,"max":1},"price":{"min":0,"max":-0},"cost":{"min":-0,"max":1e400},"leverage":{"min":1,"max":2}}}"#).unwrap();
    assert_eq!(market.runtime.kind.as_deref(), Some("swap"));
    assert_eq!(market.runtime.spot, Some(false));
    assert_eq!(market.runtime.swap, Some(true));
    assert_eq!(
        market.precision.base.unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(market.precision.quote, Some(f64::INFINITY));
    assert_eq!(
        market.limits.price.as_ref().unwrap().max.unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    let cost = market.limits.cost.as_ref().unwrap();
    assert_eq!(cost.min.to_bits(), (-0.0f64).to_bits());
    assert_eq!(cost.max, f64::INFINITY);
    let RetainedJsonInput::Object(fields) = js_capture_output(&market).unwrap() else {
        panic!("market object")
    };
    assert!(
        fields
            .iter()
            .any(|(key, value)| String::from_utf16(key).unwrap() == "swap"
                && *value == RetainedJsonInput::Bool(true))
    );
    assert!(
        !fields
            .iter()
            .any(|(key, _)| String::from_utf16(key).unwrap() == "runtime")
    );
}
