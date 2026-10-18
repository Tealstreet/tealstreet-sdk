use crate::runtime::ExchangeRuntime;
use safe_cex_types::{Order, OrderType, PlaceOrderOpts, PreparedOrderOpts};
use std::rc::Rc;

type ShapePredicate = Rc<dyn Fn(&PreparedOrderOpts) -> bool>;
pub struct ClientOrderIdRule {
    pub label: String,
    pub requirement: String,
    pub is_valid: Rc<dyn Fn(&str) -> bool>,
    pub is_honorable_shape: Option<ShapePredicate>,
    pub mint: Option<Rc<dyn Fn() -> String>>,
}
#[derive(Default)]
pub struct ClientOrderIdRuleOverrides {
    pub is_honorable_shape: Option<ShapePredicate>,
    pub mint: Option<Rc<dyn Fn() -> String>>,
}
/// Callers supply the original RegExp display and a predicate with its JS semantics.
pub fn pattern_client_order_id_rule(
    label: String,
    pattern_display: String,
    test: Rc<dyn Fn(&str) -> bool>,
    overrides: ClientOrderIdRuleOverrides,
) -> ClientOrderIdRule {
    ClientOrderIdRule {
        label,
        requirement: format!("match {pattern_display}"),
        is_valid: test,
        is_honorable_shape: overrides.is_honorable_shape,
        mint: overrides.mint,
    }
}
pub struct PlacedOrderRef {
    pub order_ids: Vec<String>,
    pub client_order_id: String,
}
/// A cancel+replace `updateOrder` result: the replacement's venue order IDs plus
/// the client order ID it went out with. Derefs to the IDs, as TS keeps it a
/// `string[]`, so `result[0]` callers are unaffected.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplacementOrderIds {
    pub order_ids: Vec<String>,
    pub client_order_id: Option<String>,
}
impl std::ops::Deref for ReplacementOrderIds {
    type Target = Vec<String>;
    fn deref(&self) -> &Vec<String> {
        &self.order_ids
    }
}
pub fn with_replacement_client_order_id(
    order_ids: Vec<String>,
    client_order_id: String,
) -> ReplacementOrderIds {
    ReplacementOrderIds {
        order_ids,
        client_order_id: Some(client_order_id),
    }
}
/// TS truthiness: an empty client order ID reads as absent.
pub fn replacement_client_order_id_of(order_ids: &ReplacementOrderIds) -> Option<&str> {
    order_ids
        .client_order_id
        .as_deref()
        .filter(|id| !id.is_empty())
}
pub enum ClientOrderLookup {
    Found(Box<Order>),
    NotFound { definitive: bool },
}
pub struct ClientOrderIdPlaceOrderOpts {
    options: PlaceOrderOpts,
}
impl ClientOrderIdPlaceOrderOpts {
    pub fn new(mut options: PlaceOrderOpts, client_order_id: String) -> Self {
        options.client_order_id = Some(client_order_id);
        Self { options }
    }
    pub fn options(&self) -> &PlaceOrderOpts {
        &self.options
    }
    pub fn into_options(self) -> PlaceOrderOpts {
        self.options
    }
}
pub fn default_mint_candidates(runtime: &dyn ExchangeRuntime) -> Vec<String> {
    vec![
        format!("ts{}", runtime.unique_id().replace(['-', '_'], "")),
        (100_000_000.0 + (runtime.random() * 900_000_000.0).floor()).to_string(),
    ]
}
pub fn client_order_id_shape_message(label: &str, options: impl Into<PreparedOrderOpts>) -> String {
    let options = options.into();
    let kind = serde_json::to_value(options.r#type).unwrap();
    format!(
        "{label} clientOrderId cannot be honored for this order shape (type={}{}{}{}{}); caller client order IDs are supported only on plain Limit/Market orders",
        kind.as_str().unwrap_or("undefined"),
        if options.stop_loss.is_some() {
            " +stopLoss"
        } else {
            ""
        },
        if options.take_profit.is_some() {
            " +takeProfit"
        } else {
            ""
        },
        if options.trigger_price.is_some() {
            " +triggerPrice"
        } else {
            ""
        },
        if options.close == Some(true) {
            " +close"
        } else {
            ""
        }
    )
}
pub fn client_order_id_split_message(label: &str) -> String {
    format!("{label} clientOrderId is supported only when one order maps to one native order")
}
pub fn same_client_order_id(value: Option<&serde_json::Value>, id: &str) -> bool {
    value.is_some_and(|value| !value.is_null() && js_string(value) == id)
}
pub fn is_regular_client_order_shape(order: impl Into<PreparedOrderOpts>) -> bool {
    let order = order.into();
    matches!(
        order.r#type,
        safe_cex_types::ModelField::Value(OrderType::Limit | OrderType::Market)
    ) && order.trigger_price.is_none()
        && order.stop_loss.is_none()
        && order.take_profit.is_none()
}

fn js_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => {
            let number = number
                .as_f64()
                .unwrap_or_else(|| number.to_string().parse::<f64>().unwrap());
            safe_cex_num::js_number_string(number)
        }
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".into(),
        serde_json::Value::Object(_) => "[object Object]".into(),
        serde_json::Value::Array(values) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    String::new()
                } else {
                    js_string(value)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}

pub fn order_carries_client_order_id(
    order: &Order,
    id: &str,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    if id.is_empty() || id == "0" || id == "undefined" {
        return Ok(false);
    }
    if order.id == id
        || order.parent_id.as_deref() == Some(id)
        || order.client_order_id.as_deref() == Some(id)
    {
        return Ok(true);
    }
    let Some(info) = order.info.value() else {
        return Ok(false);
    };
    for key in [
        "clOrdId",
        "clientOrderId",
        "orderLinkId",
        "clientOid",
        "clOrdID",
        "c",
        "client_id",
        "client_order_id",
        "clientId",
    ] {
        if let Some(value) = info.get(key) {
            if !matches!(
                value,
                safe_cex_types::retained_json::RetainedJsonInput::Null
                    | safe_cex_types::retained_json::RetainedJsonInput::Undefined
            ) && safe_cex_types::js_coercion::js_retained_string_utf16(value)?
                .iter()
                .copied()
                .eq(id.encode_utf16())
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
