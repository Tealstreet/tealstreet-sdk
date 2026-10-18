// Bounded observations of actual exchange assembly bridges, before output serialization.
use crate::request_weight::Failure;
use safe_cex_exchange::js_runtime::value::{Js, from_json};
use safe_cex_types::Market;
use serde_json::{Value, json};

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}
// Read the JSON helper's returned carrier before serializing its observation.
// Object order, UTF16 string units and numeric bits remain explicit.
pub fn json_observation(value: &Value) -> Value {
    match value {
        Value::Null => json!({"kind":"null"}),
        Value::Bool(value) => json!({"bool":value}),
        Value::Number(value) => json!({"bits":bits(value.as_f64().unwrap())}),
        Value::String(value) => json!({"units":value.encode_utf16().collect::<Vec<_>>()}),
        Value::Array(rows) => {
            json!({"array":rows.iter().map(json_observation).collect::<Vec<_>>()})
        }
        Value::Object(rows) => {
            json!({"object":rows.iter().map(|(key,value)| json!([key, json_observation(value)])).collect::<Vec<_>>()})
        }
    }
}
pub fn native_input(input: &Value) -> Js {
    match input["profile"].as_str() {
        Some("undefined") => Js::Undefined,
        Some("nonfinite") => Js::object(vec![
            ("negativeZero", Js::Number(-0.0)),
            ("nan", Js::Number(f64::NAN)),
            ("positiveInfinity", Js::Number(f64::INFINITY)),
            ("missing", Js::Undefined),
            (
                "array",
                Js::array(vec![Js::Undefined, Js::Number(f64::NAN)]),
            ),
        ]),
        _ => from_json(&input["value"]),
    }
}
pub fn json_bridge(input: &Value, function: impl FnOnce(&Js) -> Result<Value, Failure>) -> Value {
    let source = native_input(input);
    match function(&source) {
        Ok(value) => json!({"ok":json_observation(&value)}),
        Err(error) => json!({"name":error.name,"message":error.message}),
    }
}
pub fn diagnostic(input: &Value, function: impl FnOnce(String) -> Failure) -> Value {
    let source = input.as_str().unwrap().to_owned();
    let value = function(source.clone());
    assert!(value.code.is_none());
    assert!(value.original_error.is_none());
    assert!(value.response.is_none());
    json!({"name":value.name,"units":value.message.encode_utf16().collect::<Vec<_>>()})
}
pub fn markets(input: &Value, function: impl FnOnce(&Js) -> Result<Vec<Market>, Failure>) -> Value {
    let source = from_json(&input["value"]);
    if input["profile"] == "native-numbers" {
        let row = source.get("0").unwrap();
        row.get("precision")
            .unwrap()
            .set("price", Js::Number(-0.0))
            .unwrap();
        row.get("precision")
            .unwrap()
            .set("amount", Js::Number(f64::NAN))
            .unwrap();
        row.get("limits")
            .unwrap()
            .get("leverage")
            .unwrap()
            .set("max", Js::Number(f64::INFINITY))
            .unwrap();
    }
    match function(&source) {
        Err(error) => json!({"error":error.name,"messagePresent":!error.message.is_empty()}),
        Ok(rows) => json!({"rows":rows.iter().map(|row| {
            assert!(row.option.is_none(), "selected corpus has no option terms");
            assert!(row.info.is_none(), "selected corpus has no opaque info");
            json!({"id":row.id,"symbol":row.symbol,"prettySymbol":row.pretty_symbol,
                "base":row.base,"quote":row.quote,"active":row.active,
                "precision":[bits(row.precision.amount),bits(row.precision.price)],
                "amount":[bits(row.limits.amount.min),bits(safe_cex_num::numeric_value_to_number(&row.limits.amount.max))],
                "leverage":[bits(row.limits.leverage.min),bits(row.limits.leverage.max)],
                "minimumPrice":row.limits.price.as_ref().map(|value| bits(value.min)),
                "contractSize":row.contract_size.map(bits),
                "flags":[row.is_expiring,row.is_inverse,row.is_spot,row.is_perpetual,row.is_read_only],
                "fees":[row.taker_fee.map(bits),row.maker_fee.map(bits)]})
        }).collect::<Vec<_>>()}),
    }
}
