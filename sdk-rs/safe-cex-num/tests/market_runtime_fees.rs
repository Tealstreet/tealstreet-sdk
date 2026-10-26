use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::{Market, MarketFees, MarketRuntime};

#[test]
fn fees_retain_distinct_numbers_through_canonical_market_numeric_ingress() {
    let raw = r#"{"id":"A","symbol":"A","base":"A","quote":"B","active":true,
        "precision":{"amount":0.1,"price":0.2},
        "limits":{"amount":{"min":0,"max":1},"leverage":{"min":1,"max":2}},
        "fees":{"maker":-0.0001,"taker":0.0003}}"#;
    let input: serde_json::Value = serde_json::from_str(raw).unwrap();
    for market in [
        js_from_str::<Market>(raw).unwrap(),
        js_from_value::<Market>(input.clone()).unwrap(),
        js_from_value_ref::<Market>(&input).unwrap(),
    ] {
        let fees = market.runtime.fees.unwrap();
        assert_eq!(fees.maker.to_bits(), (-0.0001_f64).to_bits());
        assert_eq!(fees.taker.to_bits(), 0.0003_f64.to_bits());
    }
    let fees: MarketFees = js_from_str(r#"{"maker":-0,"taker":1e400}"#).unwrap();
    assert_eq!(fees.maker.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(fees.taker, f64::INFINITY);
    let runtime: MarketRuntime = js_from_str(r#"{"fees":{"maker":-0,"taker":1e400}}"#).unwrap();
    assert_eq!(runtime.fees, Some(fees));
}
