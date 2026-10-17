//! Open-order fields retain raw JSON and the source numeric/date semantics.
use crate::{
    constants::{ORDER_SIDE, is_paradex_trigger_order_type},
    order_maps::{map_order_status, reverse_map_order_type},
};
use safe_cex_num::{
    TableHit, js_date_parse, js_string, js_table_lookup, js_time_clip, js_truthy,
    numeric_ingress::js_value_as_f64, parse_float,
};
use safe_cex_types::Order;
use serde_json::{Map, Value};

// ts: ParadexExchange#mapOrder
pub fn map_order(
    row: &Map<String, Value>,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Order, String> {
    let text = |field| {
        row.get(field)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Paradex order field {field} must be a string"))
    };
    let side = match js_table_lookup(ORDER_SIDE, text("side")?) {
        TableHit::Mapped(value) => *value,
        _ => return Err("Paradex order side is outside the shared OrderSide domain".into()),
    };
    let reduce_only = row
        .get("reduce_only")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            row.get("flags")
                .and_then(Value::as_array)
                .is_some_and(|flags| {
                    flags
                        .iter()
                        .any(|flag| flag.as_str() == Some("REDUCE_ONLY"))
                })
        });
    let raw_type = text("type")?;
    let kind = match reverse_map_order_type(raw_type, Some(reduce_only)) {
        TableHit::Mapped(value) => value,
        _ => return Err("Paradex order type is outside the shared OrderType domain".into()),
    };
    let status = match map_order_status(text("status")?) {
        TableHit::Mapped(value) => value,
        _ => return Err("Paradex order status is outside the shared OrderStatus domain".into()),
    };
    let price_field =
        if is_paradex_trigger_order_type(raw_type) && js_truthy(row.get("trigger_price")) {
            "trigger_price"
        } else {
            "price"
        };
    let price = if js_truthy(row.get(price_field)) {
        parse_float(&js_string(row.get(price_field)).map_err(|error| error.to_string())?)
    } else {
        0.0
    };
    let amount = parse_float(text("size")?);
    let remaining = parse_float(text("remaining_size")?);
    Ok(Order {
        client_order_id: (None).into(),
        id: text("id")?.to_owned(),
        parent_id: (None).into(),
        symbol: text("market")?.to_owned(),
        side: side.into(),
        r#type: (kind).into(),
        status: status.into(),
        price: price.into(),
        amount,
        filled: amount - remaining,
        remaining,
        reduce_only,
        time_in_force: (None).into(),
        // Date(number) applies TimeClip directly; only strings are date-parsed.
        last_update: match row.get("created_at").and_then(js_value_as_f64) {
            Some(value) => js_time_clip(value),
            None => js_date_parse(text("created_at")?, local_to_utc),
        },
        info: (Some(Value::Object(row.clone()))).into(),
        close: (None).into(),
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    })
}
