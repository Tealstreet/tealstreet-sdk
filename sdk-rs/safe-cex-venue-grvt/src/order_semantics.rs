//! Execution semantics reconstructed from GRVT wire records.
use safe_cex_types::{OrderStatus, OrderTimeInForce};
use serde_json::{Map, Value};

/// ts: GrvtExchange#mapOrderStatus. The wire caller can supply missing/nonstring data.
pub fn map_order_status(status: Option<&Value>) -> OrderStatus {
    match status.and_then(Value::as_str) {
        Some("FILLED") => OrderStatus::Closed,
        Some("CANCELLED" | "REJECTED") => OrderStatus::Canceled,
        // PENDING means sent to the matching engine, hence Open, as does the default.
        _ => OrderStatus::Open,
    }
}

/// ts: GrvtExchange.getOrderCreationTimeMs, for JSON-data metadata.
pub fn get_order_creation_time_ms(metadata: Option<&Value>) -> Option<f64> {
    let raw = alias(metadata, "create_time", "ct")?;
    let text = match raw {
        Value::String(text) => text.clone(),
        Value::Number(_) => {
            safe_cex_num::js_number_string(safe_cex_num::numeric_ingress::js_value_as_f64(raw)?)
        }
        _ => return None,
    };
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    // For a nonnegative decimal integer, BigInt / 1_000_000 truncates six digits
    // before Number conversion. Parsing the full value first would round too early.
    let quotient = text.get(..text.len().checked_sub(6)?)?;
    let milliseconds = safe_cex_num::parse_float(quotient);
    (milliseconds.is_finite() && milliseconds > 0.0).then_some(milliseconds)
}

/// ts: getGrvtOrderTimeInForce
pub fn get_grvt_order_time_in_force(order: &Map<String, Value>) -> Option<OrderTimeInForce> {
    let post_only = order
        .get("post_only")
        .filter(|v| !v.is_null())
        .or_else(|| order.get("po"));
    let post_only = match post_only {
        None => false,
        Some(Value::Bool(value)) => *value,
        Some(_) => return None,
    };
    let time_in_force = order
        .get("time_in_force")
        .filter(|v| !v.is_null())
        .or_else(|| order.get("ti"));
    match time_in_force.and_then(Value::as_str) {
        Some("GOOD_TILL_TIME") => Some(if post_only {
            OrderTimeInForce::PostOnly
        } else {
            OrderTimeInForce::GoodTillCancel
        }),
        Some("IMMEDIATE_OR_CANCEL") if !post_only => Some(OrderTimeInForce::ImmediateOrCancel),
        Some("FILL_OR_KILL") if !post_only => Some(OrderTimeInForce::FillOrKill),
        _ => None,
    }
}

fn alias<'a>(value: Option<&'a Value>, full: &str, lite: &str) -> Option<&'a Value> {
    value.and_then(|value| {
        value
            .get(full)
            .filter(|v| !v.is_null())
            .or_else(|| value.get(lite))
    })
}

/// ts: getGrvtTriggerMetadata
pub fn get_grvt_trigger_metadata(
    order: &safe_cex_types::retained_json::RetainedJsonInput,
) -> Result<Option<crate::records::GrvtTriggerMetadata>, safe_cex_num::JsCoercionError> {
    use crate::types::GrvtTriggerKind;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    fn alias<'a>(value: Option<&'a Input>, full: &str, lite: &str) -> Option<&'a Input> {
        value.and_then(|value| {
            value
                .get(full)
                .filter(|value| !value.is_nullish())
                .or_else(|| value.get(lite))
        })
    }
    use safe_cex_types::OrderStopTriggerType;
    let metadata = order
        .get("metadata")
        .filter(|v| !v.is_nullish())
        .or_else(|| order.get("m"));
    let trigger = alias(metadata, "trigger", "t");
    if !trigger.is_some_and(Input::is_truthy) {
        return Ok(None);
    }
    let kind = match alias(trigger, "trigger_type", "tt").and_then(Input::as_text) {
        Some(value) if value.iter().copied().eq("STOP_LOSS".encode_utf16()) => {
            GrvtTriggerKind::StopLoss
        }
        Some(value) if value.iter().copied().eq("TAKE_PROFIT".encode_utf16()) => {
            GrvtTriggerKind::TakeProfit
        }
        _ => return Ok(None),
    };
    let tpsl = alias(trigger, "tpsl", "t");
    let price = safe_cex_num::js_retained_number(alias(tpsl, "trigger_price", "tp"))?;
    let trigger_type = match alias(tpsl, "trigger_by", "tb").and_then(Input::as_text) {
        Some(value) if value.iter().copied().eq("LAST".encode_utf16()) => {
            Some(OrderStopTriggerType::LastPrice)
        }
        Some(value) if value.iter().copied().eq("MARK".encode_utf16()) => {
            Some(OrderStopTriggerType::MarkPrice)
        }
        Some(value) if value.iter().copied().eq("INDEX".encode_utf16()) => {
            Some(OrderStopTriggerType::IndexPrice)
        }
        _ => None,
    };
    Ok(Some(crate::records::GrvtTriggerMetadata {
        r#type: kind,
        trigger_price: price.is_finite().then_some(price),
        trigger_type,
        close: alias(tpsl, "close_position", "cp").and_then(Input::as_bool),
        is_split_position: alias(tpsl, "is_split_position", "is").and_then(Input::as_bool),
    }))
}

/// Preserve the instrument prefix and normalize only a terminal ASCII PERP suffix.
pub fn symbol_to_instrument_id(symbol: &str) -> String {
    if let Some((prefix, suffix)) = symbol.rsplit_once('_')
        && suffix.eq_ignore_ascii_case("PERP")
    {
        return format!("{prefix}_Perp");
    }
    symbol.to_owned()
}

/// Closed GRVT time-in-force values used by the order-signing payload.
pub fn map_time_in_force_to_enum(time_in_force: crate::types::GrvtTimeInForce) -> f64 {
    use crate::types::GrvtTimeInForce;
    match time_in_force {
        GrvtTimeInForce::GoodTillTime => 1.0,
        GrvtTimeInForce::AllOrNone => 2.0,
        GrvtTimeInForce::ImmediateOrCancel => 3.0,
        GrvtTimeInForce::FillOrKill => 4.0,
    }
}

/// Resolve the source's ordinary-object interval table, including inherited hits.
pub fn convert_interval(
    interval: &str,
) -> safe_cex_num::TableHit<crate::types::GrvtCandlestickInterval> {
    use crate::types::GrvtCandlestickInterval;
    use safe_cex_num::TableHit;
    let mapping = [
        ("1m", GrvtCandlestickInterval::Ci1M),
        ("3m", GrvtCandlestickInterval::Ci3M),
        ("5m", GrvtCandlestickInterval::Ci5M),
        ("15m", GrvtCandlestickInterval::Ci15M),
        ("30m", GrvtCandlestickInterval::Ci30M),
        ("1h", GrvtCandlestickInterval::Ci1H),
        ("2h", GrvtCandlestickInterval::Ci2H),
        ("4h", GrvtCandlestickInterval::Ci4H),
        ("6h", GrvtCandlestickInterval::Ci6H),
        ("12h", GrvtCandlestickInterval::Ci12H),
        ("1d", GrvtCandlestickInterval::Ci1D),
        ("1w", GrvtCandlestickInterval::Ci1W),
    ];
    match safe_cex_num::js_table_lookup(&mapping, interval) {
        TableHit::Mapped(value) => TableHit::Mapped(*value),
        TableHit::Inherited(member) => TableHit::Inherited(member),
        TableHit::Missing => TableHit::Mapped(GrvtCandlestickInterval::Ci1M),
    }
}
