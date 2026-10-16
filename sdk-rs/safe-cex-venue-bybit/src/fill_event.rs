//! Execution amounts are cumulative fills, never the order's protection-band price/size.
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, PartialEq)]
pub struct BybitExecutionAmounts {
    pub price: f64,
    pub amount: f64,
}

pub fn finite_number(value: Option<&Value>) -> Result<Option<f64>, &'static str> {
    if value.is_none_or(|value| value.is_null() || value.as_str() == Some("")) {
        return Ok(None);
    }
    let parsed = match value {
        Some(Value::Number(value)) => value.as_f64().unwrap_or(f64::NAN),
        value => crate::exchange_state::parse_float(value)?,
    };
    Ok(parsed.is_finite().then_some(parsed))
}

pub fn resolve_bybit_execution(
    order: Option<&Map<String, Value>>,
) -> Result<BybitExecutionAmounts, &'static str> {
    let get = |key: &str| {
        use safe_cex_num::{
            TableHit,
            get_key::{GetKeyObject, GetKeyValue, v},
        };
        match v(order.map(GetKeyObject::Json), key) {
            TableHit::Mapped(GetKeyValue::Json(value)) => Ok(Some(value)),
            TableHit::Mapped(GetKeyValue::Undefined) | TableHit::Missing => Ok(None),
            TableHit::Inherited(_) => Err("Bybit execution field cannot carry an inherited member"),
        }
    };
    // Preserve lazy fallback: a valid execution field must not coerce the raw order field.
    let price = match finite_number(get("avgPrice")?)? {
        Some(value) => value,
        None => finite_number(get("price")?)?.unwrap_or(0.),
    };
    let amount = match finite_number(get("cumExecQty")?)? {
        Some(value) => value,
        None => finite_number(get("qty")?)?.unwrap_or(0.),
    };
    Ok(BybitExecutionAmounts { price, amount })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
    fn execution_fields_match_live_source() {
        let run = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/fill-event-fixtures.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let cases: Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(
            cases.as_array().unwrap().len(),
            1406,
            "fill_event.rs-CARDINALITY source total"
        );
        for (index, case) in cases.as_array().unwrap().iter().enumerate() {
            let result = resolve_bybit_execution(case["order"].as_object());
            match result {
                Ok(value) => {
                    assert!(case.get("error").is_none(), "case {index}: {case}");
                    assert_eq!(
                        value.price,
                        case["result"]["price"].as_f64().unwrap(),
                        "price {index}: {case}"
                    );
                    assert_eq!(
                        value.amount,
                        case["result"]["amount"].as_f64().unwrap(),
                        "amount {index}: {case}"
                    );
                    assert_eq!(
                        value.price == 0. && value.price.is_sign_negative(),
                        case["negativePrice"] == true
                    );
                    assert_eq!(
                        value.amount == 0. && value.amount.is_sign_negative(),
                        case["negativeAmount"] == true
                    );
                }
                Err(error) => assert_eq!(
                    error,
                    case["error"].as_str().unwrap_or("unexpected failure"),
                    "case {index}: {case}"
                ),
            }
        }
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitExecutionAmounts")]
struct BybitExecutionAmountsWire {
    price: f64,
    amount: f64,
}
impl<'de> Deserialize<'de> for BybitExecutionAmounts {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitExecutionAmountsWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitExecutionAmounts {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitExecutionAmountsWire::serialize(self, serializer)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitExecutionAmounts {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { price, amount });
    }
}
