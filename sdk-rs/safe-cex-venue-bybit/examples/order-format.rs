use safe_cex_types::{Market, PlaceOrderOpts, Ticker};
use safe_cex_venue_bybit::order_format::*;
use serde_json::{Value, json};
use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Vec<Value> = serde_json::from_str(&input).unwrap();
    let outputs:Vec<_>=cases.into_iter().map(|case| {
  let opts:PlaceOrderOpts=serde_json::from_value(case["opts"].clone()).unwrap();
  assert_eq!(serde_json::to_value(opts.sl_trigger_type).unwrap(), case["opts"]["slTriggerType"], "native formatter sl_trigger_type identity");
  assert_eq!(serde_json::to_value(opts.tp_trigger_type).unwrap(), case["opts"]["tpTriggerType"], "native formatter tp_trigger_type identity");
  assert_eq!(serde_json::to_value(opts.trigger_type).unwrap(), case["opts"]["triggerType"], "native formatter trigger_type identity");
  let market:Option<Market>=serde_json::from_value(case["market"].clone()).unwrap();
  let ticker:Option<Ticker>=serde_json::from_value(case["ticker"].clone()).unwrap();
  let request=format_order_for_batch(&opts,market.as_ref(),ticker.as_ref(),case["positionIdx"].as_f64().unwrap());
  if let Some(expected)=case.get("nativeExpected") {
    let label=case["label"].as_str().unwrap();
    let market=market.as_ref().unwrap();
    assert_eq!(market.precision.price.to_bits(),case["market"]["precision"]["price"].as_f64().unwrap().to_bits(),"BYBIT-PRECISION native price step {label}");
    assert_eq!(market.precision.amount.to_bits(),case["market"]["precision"]["amount"].as_f64().unwrap().to_bits(),"BYBIT-PRECISION native amount step {label}");
    let native=request.as_ref().expect("precision supplement must format");
    for field in ["price","triggerPrice","stopLoss","takeProfit","qty"] {
      assert_eq!(native.get(field),expected.get(field),"BYBIT-PRECISION {field} {label}");
    }
  }
  json!({"request":request.as_ref().ok(),"errors":request.as_ref().err().into_iter().collect::<Vec<_>>(),"triggerBy":get_bybit_trigger_by(&opts),"directionPrice":ticker.as_ref().map(|t|get_bybit_trigger_direction_price(&opts,t))})
 }).collect();
    println!("{}", serde_json::to_string(&outputs).unwrap());
}
