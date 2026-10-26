use safe_cex_num::{
    js_number_string,
    numeric_ingress::{JsNumberOutput, js_from_str},
    order_policy::*,
};
use safe_cex_types::{Market, Order, OrderSide, OrderType, PlaceOrderOpts, Position, Ticker};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::borrow::{Borrow, BorrowMut};

fn number(v: &Value) -> f64 {
    f64::from_bits(
        u64::from_str_radix(v["$bits"].as_str().expect("native number tag"), 16).unwrap(),
    )
}
fn undefined(v: &Value) -> bool {
    v.get("$undefined") == Some(&Value::Bool(true))
}
fn raw(v: &Value) -> String {
    if v.get("$bits").is_some() {
        let n = number(v);
        if n.is_nan() {
            "0".into()
        } else if n == f64::INFINITY {
            "1e400".into()
        } else if n == f64::NEG_INFINITY {
            "-1e400".into()
        } else if n == 0.0 && n.is_sign_negative() {
            "-0".into()
        } else {
            js_number_string(n)
        }
    } else if let Some(values) = v.as_object() {
        format!(
            "{{{}}}",
            values
                .iter()
                .filter(|(_, v)| !undefined(v))
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), raw(v)))
                .collect::<Vec<_>>()
                .join(",")
        )
    } else if let Some(values) = v.as_array() {
        format!("[{}]", values.iter().map(raw).collect::<Vec<_>>().join(","))
    } else {
        serde_json::to_string(v).unwrap()
    }
}
fn decode<T: DeserializeOwned + serde::Serialize + JsNumberOutput>(v: &Value) -> T {
    js_from_str(&raw(v)).unwrap()
}
fn market(v: &Value) -> Market {
    let mut m: Market = decode(v);
    m.precision.amount = number(&v["precision"]["amount"]);
    m.precision.price = number(&v["precision"]["price"]);
    m.contract_size = v.get("contractSize").filter(|n| !undefined(n)).map(number);
    m
}
fn opts(v: &Value) -> PlaceOrderOpts {
    let mut o: PlaceOrderOpts = decode(v);
    o.amount = number(&v["amount"]);
    o.price = v.get("price").filter(|n| !undefined(n)).map(number);
    o.stop_loss = v.get("stopLoss").filter(|n| !undefined(n)).map(number);
    o.take_profit = v.get("takeProfit").filter(|n| !undefined(n)).map(number);
    o.trigger_price = v.get("triggerPrice").filter(|n| !undefined(n)).map(number);
    o
}
fn native(actual: f64, expected: &Value, name: &str) {
    let expected = number(expected);
    if expected.is_nan() {
        assert!(actual.is_nan(), "ORDER market native {name} NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "ORDER market native {name} bits"
        );
    }
}
fn optional(actual: Option<f64>, expected: Option<&Value>, name: &str) {
    if let Some(expected) = expected.filter(|n| !undefined(n)) {
        native(actual.expect("present native field"), expected, name);
    } else {
        assert!(actual.is_none(), "ORDER market native {name} omission");
    }
}
fn order_fields(actual: &PlaceOrderOpts, expected: &Value) {
    native(actual.amount, &expected["amount"], "amount");
    optional(actual.price, expected.get("price"), "price");
    optional(actual.stop_loss, expected.get("stopLoss"), "stopLoss");
    optional(actual.take_profit, expected.get("takeProfit"), "takeProfit");
    optional(
        actual.trigger_price,
        expected.get("triggerPrice"),
        "triggerPrice",
    );
    assert_eq!(
        actual.symbol,
        expected["symbol"].as_str().unwrap(),
        "ORDER market native symbol"
    );
    assert_eq!(
        actual.reduce_only,
        expected.get("reduceOnly").and_then(Value::as_bool),
        "ORDER market native reduceOnly"
    );
    assert_eq!(
        actual.client_order_id.as_deref(),
        expected.get("clientOrderId").and_then(Value::as_str),
        "ORDER market native clientOrderId"
    );
    assert_eq!(
        match actual.side {
            OrderSide::Buy => "buy",
            OrderSide::Sell => "sell",
        },
        expected["side"].as_str().unwrap(),
        "ORDER market native side"
    );
    assert_eq!(
        match actual.r#type {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::Conditional => "conditional",
            OrderType::StopLoss => "stop_market",
            OrderType::TakeProfit => "take_profit_market",
            OrderType::TrailingStopLoss => "trailing_stop_market",
        },
        expected["type"].as_str().unwrap(),
        "ORDER market native type"
    );
}
#[derive(Clone)]
struct Extended {
    order: PlaceOrderOpts,
    extension: Option<Value>,
}
impl Borrow<PlaceOrderOpts> for Extended {
    fn borrow(&self) -> &PlaceOrderOpts {
        &self.order
    }
}
impl BorrowMut<PlaceOrderOpts> for Extended {
    fn borrow_mut(&mut self) -> &mut PlaceOrderOpts {
        &mut self.order
    }
}

#[test]
fn market_order_policy_matches_native_source_values() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_policy_market.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ORDER market source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    // 12*8*2*2*5*2 + (49+14)*2 + 6 + 6*7*7*2+24 + 6*2*2*2*11*5 + 8 + 3.
    assert_eq!(rows.len(), 7235, "ORDER market source cardinality");
    let mut compared = [0; 12];
    for row in &rows {
        let input = &row["input"];
        let expected = &row["expected"];
        let op = row["op"].as_str().unwrap();
        let mut logs = Vec::new();
        match op {
            "roundOrderPrice"
            | "roundOrderSize"
            | "roundOrderSizeToNearest"
            | "roundMarketCloseSize"
            | "roundMarketOrderSize" => {
                let m = market(&input["market"]);
                let n = number(&input["raw"]);
                let (actual, index) = match op {
                    "roundOrderPrice" => (round_order_price(n, &m), 0),
                    "roundOrderSize" => {
                        (round_order_size(n, &m, input["exchangeName"].as_str()), 1)
                    }
                    "roundOrderSizeToNearest" => (round_order_size_to_nearest(n, &m), 2),
                    "roundMarketCloseSize" => (round_market_close_size(n, &m), 3),
                    _ => (
                        round_market_order_size(n, &m, input["reduceOnly"].as_bool().unwrap()),
                        4,
                    ),
                };
                native(actual, expected, op);
                compared[index] += 1;
            }
            "venueSupportsInfinityStops" => {
                match venue_supports_infinity_stops(input["name"].as_str().unwrap()) {
                    Ok(value) => {
                        assert!(
                            row.get("error").is_none(),
                            "ORDER metadata expected refusal"
                        );
                        assert!(
                            expected.is_boolean() || undefined(expected),
                            "ORDER metadata required native value"
                        );
                        assert_eq!(
                            value,
                            expected.as_bool(),
                            "ORDER metadata native infinityStops"
                        );
                    }
                    Err(error) => {
                        assert_eq!(
                            error.name,
                            row["error"]["name"].as_str().unwrap(),
                            "ORDER metadata native error name"
                        );
                        assert_eq!(
                            error.message,
                            row["error"]["message"].as_str().unwrap(),
                            "ORDER metadata native error message"
                        );
                    }
                }
                compared[5] += 1;
            }
            "venueAutoDownsizesReduceOrders" => {
                assert_eq!(
                    venue_auto_downsizes_reduce_orders(input["name"].as_str().unwrap()),
                    expected.as_bool().unwrap(),
                    "ORDER metadata native autoDownsize"
                );
                compared[6] += 1;
            }
            "isStopLikeOrderType" => {
                let kind: OrderType = serde_json::from_value(input["type"].clone()).unwrap();
                assert_eq!(
                    is_stop_like_order_type(kind),
                    expected.as_bool().unwrap(),
                    "ORDER native stop type"
                );
                compared[7] += 1;
            }
            "prepareOrder" => {
                let m = market(&input["context"]["market"]);
                let source = &input["opts"];
                let input = Extended {
                    order: opts(source),
                    extension: source.get("extension").cloned(),
                };
                match prepare_order(
                    &input,
                    OrderPolicyContext {
                        market: &m,
                        exchange_name: row["input"]["context"]["exchangeName"].as_str(),
                    },
                ) {
                    Ok(out) => {
                        assert!(row.get("error").is_none(), "ORDER prepare expected refusal");
                        order_fields(&out.order, expected);
                        assert_eq!(
                            out.extension.as_ref(),
                            expected.get("extension"),
                            "ORDER native extension preserved"
                        );
                    }
                    Err(error) => {
                        assert!(
                            row.get("error").is_some(),
                            "ORDER prepare unexpected refusal"
                        );
                        assert_eq!(
                            error.name,
                            row["error"]["name"].as_str().expect("source error name"),
                            "ORDER prepare native error name"
                        );
                        assert_eq!(
                            error.message,
                            row["error"]["message"]
                                .as_str()
                                .expect("source error message"),
                            "ORDER prepare native error message"
                        );
                    }
                }
                compared[8] += 1;
            }
            "downSizeReduceOrder" | "clampReduceOnlyOrder" => {
                let context = if op == "downSizeReduceOrder" {
                    input
                } else {
                    &input["context"]
                };
                let m = market(&context["market"]);
                let o = opts(&input["order"]);
                let positions: Vec<Position> = decode(&context["positions"]);
                let orders: Vec<Order> = decode(if op == "downSizeReduceOrder" {
                    &context["orders"]
                } else {
                    &context["restingOrders"]
                });
                let mut log = |message: &str| logs.push(message.to_owned());
                let (out, index) = if op == "downSizeReduceOrder" {
                    (
                        down_size_reduce_order(
                            &m,
                            o,
                            &positions,
                            &orders,
                            number(&input["price"]),
                            &mut log,
                        ),
                        9,
                    )
                } else {
                    let ticker: Option<Ticker> =
                        context.get("ticker").filter(|v| !undefined(v)).map(decode);
                    (
                        clamp_reduce_only_order(
                            o,
                            ReduceOnlyClampContext {
                                policy: OrderPolicyContext {
                                    market: &m,
                                    exchange_name: context["exchangeName"].as_str(),
                                },
                                positions: &positions,
                                resting_orders: &orders,
                                ticker: ticker.as_ref(),
                            },
                            &mut log,
                        ),
                        10,
                    )
                };
                order_fields(&out, expected);
                compared[index] += 1;
            }
            "ScriptValidationError" => {
                let error = ScriptValidationError::new(input["message"].as_str().unwrap().into());
                assert_eq!(
                    error.name,
                    expected["name"].as_str().unwrap(),
                    "ORDER native validation error name"
                );
                assert_eq!(
                    error.message,
                    expected["message"].as_str().unwrap(),
                    "ORDER native validation error message"
                );
                compared[11] += 1;
            }
            _ => panic!("ORDER market unknown source operation {op}"),
        }
        assert_eq!(
            logs,
            row["logs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "ORDER native logging"
        );
    }
    assert_eq!(
        compared,
        [768, 768, 768, 768, 768, 63, 63, 6, 612, 528, 2120, 3],
        "ORDER market compared cardinality"
    );
}

#[test]
fn declared_policy_boundaries_have_actual_source_witnesses() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_policy_boundaries.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ORDER boundary source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let witness: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        witness["sparse"]["symbol"], "BTC",
        "ORDER source sparse symbol"
    );
    assert_eq!(
        witness["sparse"]["type"], "limit",
        "ORDER source sparse type"
    );
    assert_eq!(
        witness["sparse"]["amount"].as_f64().unwrap().to_bits(),
        1.0_f64.to_bits(),
        "ORDER source sparse amount"
    );
    assert_eq!(
        witness["sparse"]["hasSide"], false,
        "ORDER source sparse missing side accepted"
    );
    assert!(
        serde_json::from_str::<PlaceOrderOpts>(r#"{"symbol":"BTC","type":"limit","amount":1}"#)
            .is_err(),
        "ORDER chosen typed sparse refusal"
    );
    assert_eq!(
        witness["metadata"]["before"], true,
        "ORDER source stock metadata"
    );
    assert_eq!(
        witness["metadata"]["after"], false,
        "ORDER source mutable metadata"
    );
    assert_eq!(
        venue_supports_infinity_stops("bybit").unwrap(),
        Some(true),
        "ORDER chosen frozen metadata"
    );
}

#[test]
fn preparation_preserves_caller_fields_on_success_and_later_refusal() {
    let source = serde_json::json!({"symbol":"BTC","type":"limit","side":"buy","amount":{"$bits":"3fd6666666666666"},
        "price":{"$bits":"405edd2f1a9fbe77"},"stopLoss":{"$bits":"0000000000000000"}});
    let original = opts(&source);
    let m: Market = serde_json::from_value(serde_json::json!({"id":"m","symbol":"BTC","base":"BTC","quote":"USDT","active":true,
        "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    let error = prepare_order(
        &original,
        OrderPolicyContext {
            market: &m,
            exchange_name: Some("bybit"),
        },
    )
    .unwrap_err();
    assert_eq!(
        error.message, "invalid stopLoss for 'BTC': 0",
        "ORDER later price refusal"
    );
    native(
        original.amount,
        &source["amount"],
        "original amount after refusal",
    );
    native(
        original.price.unwrap(),
        &source["price"],
        "original price after refusal",
    );
    let mut valid = original.clone();
    valid.stop_loss = None;
    let output = prepare_order(
        &valid,
        OrderPolicyContext {
            market: &m,
            exchange_name: Some("bybit"),
        },
    )
    .unwrap();
    assert_eq!(
        output.amount.to_bits(),
        0.3_f64.to_bits(),
        "ORDER copied rounded amount"
    );
    native(
        valid.amount,
        &source["amount"],
        "original amount after success",
    );
    native(
        valid.price.unwrap(),
        &source["price"],
        "original price after success",
    );
}
