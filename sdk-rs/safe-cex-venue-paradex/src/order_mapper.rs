//! Open-order fields retain native data and the source numeric/date semantics.
use crate::{
    constants::{ORDER_SIDE, is_paradex_trigger_order_type},
    order_maps::{map_order_status, reverse_map_order_type},
};
use safe_cex_num::{
    TableHit, js_date_parse, js_retained_parse_float, js_retained_string, js_table_lookup,
    js_time_clip,
};
use safe_cex_types::{ModelField, Order, retained_json::RetainedJsonInput as Input};

#[derive(Debug)]
pub(crate) struct OrderProjectionRefusal {
    pub field: &'static str,
    pub value: Option<Input>,
    pub message: String,
}

// ts: ParadexExchange#mapOrder
pub fn map_order(row: &Input, local_to_utc: impl FnOnce(f64) -> f64) -> Result<Order, String> {
    map_order_row(row, local_to_utc)?.map_err(|refusal| refusal.message)
}

// Source evaluation errors remain distinct from the batch's strict identity
// refusals. The standalone utility retains the original refusal error.
pub(crate) fn map_order_row(
    row: &Input,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Result<Order, OrderProjectionRefusal>, String> {
    let text = |field| {
        row.get(field)
            .and_then(Input::as_text)
            .map(String::from_utf16)
            .transpose()
            .map_err(|_| format!("Paradex order field {field} is outside UTF-8"))?
            .ok_or_else(|| format!("Paradex order field {field} must be a string"))
    };
    let side_key = js_retained_string(row.get("side")).map_err(|error| error.to_string())?;
    let side = match js_table_lookup(ORDER_SIDE, &side_key) {
        TableHit::Mapped(value) => ModelField::Value(*value),
        TableHit::Missing => ModelField::Undefined,
        TableHit::Inherited(_) => {
            return Err("Paradex order side is outside the shared OrderSide domain".into());
        }
    };
    let reduce_only = row
        .get("reduce_only")
        .and_then(Input::as_bool)
        .unwrap_or_else(|| {
            row.get("flags")
                .and_then(Input::as_array)
                .is_some_and(|flags| flags.iter().any(|flag| flag.text_equals("REDUCE_ONLY")))
        });
    let raw_type = text("type")?;
    let kind = match reverse_map_order_type(&raw_type, Some(reduce_only)) {
        TableHit::Mapped(value) => value,
        _ => return Err("Paradex order type is outside the shared OrderType domain".into()),
    };
    let status = match map_order_status(&text("status")?) {
        TableHit::Mapped(value) => value,
        _ => return Err("Paradex order status is outside the shared OrderStatus domain".into()),
    };
    let price_field = if is_paradex_trigger_order_type(&raw_type)
        && row.get("trigger_price").is_some_and(Input::is_truthy)
    {
        "trigger_price"
    } else {
        "price"
    };
    let price = if row.get(price_field).is_some_and(Input::is_truthy) {
        js_retained_parse_float(row.get(price_field)).map_err(|error| error.to_string())?
    } else {
        0.0
    };
    let amount = js_retained_parse_float(row.get("size")).map_err(|error| error.to_string())?;
    let remaining =
        js_retained_parse_float(row.get("remaining_size")).map_err(|error| error.to_string())?;
    let last_update = match row.get("created_at") {
        None | Some(Input::Undefined) => f64::NAN,
        Some(Input::Null) => 0.0,
        Some(Input::Bool(value)) => {
            if *value {
                1.0
            } else {
                0.0
            }
        }
        Some(value) => match value.as_f64() {
            Some(number) => js_time_clip(number),
            None => js_date_parse(
                &js_retained_string(Some(value)).map_err(|error| error.to_string())?,
                local_to_utc,
            ),
        },
    };
    let identity = |field: &'static str| {
        row.get(field)
            .and_then(Input::as_text)
            .and_then(|units| String::from_utf16(units).ok())
            .ok_or_else(|| OrderProjectionRefusal {
                field,
                value: row.get(field).cloned(),
                message: format!("Paradex order field {field} must be a string"),
            })
    };
    let identities = (|| Ok((identity("id")?, identity("market")?)))();
    let (id, symbol) = match identities {
        Ok(fields) => fields,
        Err(refusal) => return Ok(Err(refusal)),
    };
    Ok(Ok(Order {
        client_order_id: (None).into(),
        id,
        parent_id: (None).into(),
        symbol,
        side,
        r#type: (kind).into(),
        status: status.into(),
        price: price.into(),
        amount,
        filled: amount - remaining,
        remaining,
        reduce_only: reduce_only.into(),
        time_in_force: None.into(),
        // Date distinguishes missing, null and booleans before primitive/string parsing.
        last_update,
        info: Some(row.clone()).into(),
        close: (None).into(),
        trigger_type: (None).into(),
        tp_trigger_type: (None).into(),
        sl_trigger_type: (None).into(),
        take_profit: (None).into(),
        stop_loss: (None).into(),
        extra: (None).into(),
    }))
}
