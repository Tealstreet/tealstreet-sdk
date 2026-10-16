use crate::private_event_value::EventOps;
use safe_cex_num::{JsCoercionError, ProtoMember, js_string, parse_float};
use serde_json::Value;

// ts: toOptionalNumber (binance.order-history.ts)
pub fn to_optional_number(value: Option<&Value>) -> Result<Option<f64>, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok((parsed.is_finite() && parsed != 0.0).then_some(parsed))
}

// ts: toNumber (binance.order-history.ts)
pub fn to_number(value: Option<&Value>) -> Result<f64, JsCoercionError> {
    let text = js_string(
        value
            .filter(|v| !v.is_null())
            .or(Some(&Value::String(String::new()))),
    )?;
    let parsed = parse_float(&text);
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}

/// Canonical source mapping retains native fields and the originating realm.
pub fn map_binance_historical_order(
    row: &crate::private_event_value::EventValue,
    realm: &std::rc::Rc<safe_cex_num::PrototypeRealm>,
) -> Result<Option<crate::private_event_value::EventValue>, crate::request_weight::Failure> {
    crate::order_history_native::map_binance_historical_order_native(row, realm)
}

/// Explicit opt-in projection into the shared closed HistoricalOrder model.
/// See parity/MANIFEST-SPEC.md#closed-typed-output-boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinanceHistoricalOrderError {
    Native(crate::request_weight::Failure),
    ClosedOutput {
        field: &'static str,
        member: ProtoMember,
    },
    Projection(String),
}
impl std::fmt::Display for BinanceHistoricalOrderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native(error) => error.fmt(f),
            Self::ClosedOutput { field, member } => write!(
                f,
                "HistoricalOrder.{field} cannot carry inherited {member:?}"
            ),
            Self::Projection(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for BinanceHistoricalOrderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Native(error) => Some(error.as_ref()),
            _ => None,
        }
    }
}

pub fn map_binance_historical_order_typed(
    row: &serde_json::Map<String, Value>,
) -> Result<Option<safe_cex_types::HistoricalOrder>, BinanceHistoricalOrderError> {
    use crate::private_event_value::EventValue;
    let realm = safe_cex_num::PrototypeRealm::current();
    let input = crate::private_session::native(Value::Object(row.clone()));
    let Some(result) = map_binance_historical_order(&input, &realm)
        .map_err(BinanceHistoricalOrderError::Native)?
    else {
        return Ok(None);
    };
    // Run the complete source body before applying the requested output model.
    for field in ["type", "status", "positionSide"] {
        if let EventValue::Builtin(reference) = result
            .read(field)
            .map_err(BinanceHistoricalOrderError::Native)?
            && let safe_cex_num::BuiltinIdentity::Object(member) = reference.member()
        {
            return Err(BinanceHistoricalOrderError::ClosedOutput { field, member });
        }
    }
    // The typed optional fields deserialize a present value as T. Native own
    // undefined denotes an absent optional value at this explicit projection.
    let projected = EventValue::object_in_realm(
        realm,
        result
            .spread()
            .into_iter()
            .filter(|(_, value)| !matches!(value, EventValue::Undefined))
            .collect(),
    );
    crate::class_decode::decode(projected)
        .map(Some)
        .map_err(|error| BinanceHistoricalOrderError::Projection(error.to_string()))
}
