//! Private WebSocket position/scalar helper algorithms. Full WS composition is
//! separate; JS primitives and virtual object/collection calls are host bindings.
use crate::{cross_cache::CrossCacheHost, spot_json::JsString};
pub trait PrivatePositionHost: CrossCacheHost {
    fn number(&mut self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn number_value(&self, value: f64) -> Self::Value;
    fn lowercase(&mut self, value: &JsString) -> Result<JsString, Self::Error>;
    fn index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn add(&mut self, left: &Self::Value, right: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Fresh property read from this body's lexical source class binding.
    fn max_tracked_fill_ids(&mut self) -> Result<Self::Value, Self::Error>;
    /// ECMAScript relational comparison, including left-before-right coercion.
    fn greater_than(
        &mut self,
        value: &Self::Value,
        limit: &Self::Value,
    ) -> Result<bool, Self::Error>;
}
fn text(s: &str) -> JsString {
    JsString(s.encode_utf16().collect())
}
fn contains(s: &JsString, part: &str) -> bool {
    let part = part.encode_utf16().collect::<Vec<_>>();
    s.0.windows(part.len()).any(|w| w == part)
}
fn method<H: PrivatePositionHost>(
    h: &mut H,
    receiver: &H::Value,
    name: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let function = h.get(receiver, name)?;
    h.call(&function, receiver, args)
}
fn optional<H: PrivatePositionHost>(
    h: &mut H,
    value: &H::Value,
    property: &str,
) -> Result<H::Value, H::Error> {
    if h.is_nullish(value) {
        Ok(h.undefined())
    } else {
        h.get(value, property)
    }
}
pub fn get_fill_position_side<H: PrivatePositionHost>(h: &H, fill_dir: &JsString) -> H::Value {
    if contains(fill_dir, "long") {
        h.literal(&text("long"))
    } else if contains(fill_dir, "short") {
        h.literal(&text("short"))
    } else {
        h.undefined()
    }
}
pub fn safe_parse_number<H: PrivatePositionHost>(
    h: &mut H,
    value: &H::Value,
) -> Result<f64, H::Error> {
    let parsed = h.number(value)?;
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
pub fn track_processed_realized_pnl_fill<H: PrivatePositionHost>(
    h: &mut H,
    socket: &H::Value,
    fill_id: &H::Value,
) -> Result<(), H::Error> {
    let ids = h.get(socket, "processedRealizedPnlFillIds")?;
    method(h, &ids, "add", &[fill_id.clone()])?;
    let queue = h.get(socket, "processedRealizedPnlFillQueue")?;
    method(h, &queue, "push", &[fill_id.clone()])?;
    let queue = h.get(socket, "processedRealizedPnlFillQueue")?;
    let length = h.get(&queue, "length")?;
    let limit = h.max_tracked_fill_ids()?;
    if h.greater_than(&length, &limit)? {
        let queue = h.get(socket, "processedRealizedPnlFillQueue")?;
        let oldest = method(h, &queue, "shift", &[])?;
        if h.truthy(&oldest) {
            let ids = h.get(socket, "processedRealizedPnlFillIds")?;
            method(h, &ids, "delete", &[oldest])?;
        }
    }
    Ok(())
}
pub fn get_realized_pnl_key(symbol: &JsString, side: &JsString) -> JsString {
    JsString(
        symbol
            .0
            .iter()
            .copied()
            .chain([b':' as u16])
            .chain(side.0.iter().copied())
            .collect(),
    )
}
pub fn resolve_symbol_from_coin<H: PrivatePositionHost>(
    h: &mut H,
    socket: &H::Value,
    coin: &JsString,
    markets_by_id: &H::Value,
) -> Result<H::Value, H::Error> {
    let coin_value = h.literal(coin);
    let mut market = h.index(markets_by_id, &coin_value)?;
    if !h.truthy(&market) && contains(coin, ":") {
        let parent = h.get(socket, "parent")?;
        let map = h.get(&parent, "marketsLowerCaseMap")?;
        // Source method resolution precedes coin.toLowerCase argument evaluation.
        let function = h.get(&map, "get")?;
        let lower = h.lowercase(coin)?;
        let lower = h.literal(&lower);
        market = h.call(&function, &map, &[lower])?;
    }
    let symbol = optional(h, &market, "symbol")?;
    Ok(if h.truthy(&symbol) {
        symbol
    } else {
        coin_value
    })
}
pub fn set_realized_pnl_for_position<H: PrivatePositionHost>(
    h: &mut H,
    socket: &H::Value,
    coin: &H::Value,
    side: &H::Value,
    value: &H::Value,
    markets_by_id: &H::Value,
) -> Result<(), H::Error> {
    let symbol = method(
        h,
        socket,
        "resolveSymbolFromCoin",
        &[coin.clone(), markets_by_id.clone()],
    )?;
    let map = h.get(socket, "realizedPnlByPositionKey")?;
    let function = h.get(&map, "set")?;
    let key = method(h, socket, "getRealizedPnlKey", &[symbol, side.clone()])?;
    h.call(&function, &map, &[key, value.clone()])?;
    Ok(())
}
pub fn increment_realized_pnl_for_position<H: PrivatePositionHost>(
    h: &mut H,
    socket: &H::Value,
    coin: &H::Value,
    side: &H::Value,
    delta: &H::Value,
    markets_by_id: &H::Value,
) -> Result<(), H::Error> {
    let symbol = method(
        h,
        socket,
        "resolveSymbolFromCoin",
        &[coin.clone(), markets_by_id.clone()],
    )?;
    let key = method(h, socket, "getRealizedPnlKey", &[symbol, side.clone()])?;
    // set receiver/method are resolved before the inner fresh get receiver.
    let map = h.get(socket, "realizedPnlByPositionKey")?;
    let function = h.get(&map, "set")?;
    let get_map = h.get(socket, "realizedPnlByPositionKey")?;
    let old = method(h, &get_map, "get", &[key.clone()])?;
    let old = if h.truthy(&old) {
        old
    } else {
        h.number_value(0.0)
    };
    let value = h.add(&old, delta)?;
    h.call(&function, &map, &[key, value])?;
    Ok(())
}
pub fn get_realized_pnl_for_position<H: PrivatePositionHost>(
    h: &mut H,
    socket: &H::Value,
    symbol: &H::Value,
    side: &H::Value,
) -> Result<H::Value, H::Error> {
    let map = h.get(socket, "realizedPnlByPositionKey")?;
    let function = h.get(&map, "get")?;
    let key = method(
        h,
        socket,
        "getRealizedPnlKey",
        &[symbol.clone(), side.clone()],
    )?;
    h.call(&function, &map, &[key])
}
pub fn resolve_position_mode<H: PrivatePositionHost>(
    h: &mut H,
    asset_position: &H::Value,
) -> Result<H::Value, H::Error> {
    let kind = optional(h, asset_position, "type")?;
    let kind = if h.truthy(&kind) {
        kind
    } else {
        h.literal(&text(""))
    };
    let raw = h.to_string(&kind)?;
    let lower = h.lowercase(&raw)?;
    let mode = if lower == text("twoway") || lower == text("hedged") {
        "hedged"
    } else {
        "oneway"
    };
    Ok(h.literal(&text(mode)))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalanceSummaryField {
    MarginSummary,
    CrossMarginSummary,
}
impl BalanceSummaryField {
    pub fn source_name(self) -> &'static str {
        match self {
            Self::MarginSummary => "marginSummary",
            Self::CrossMarginSummary => "crossMarginSummary",
        }
    }
}
#[derive(Clone, Debug)]
pub struct CoreBalanceSummary {
    pub account_value: f64,
    pub total_margin_used: f64,
    pub source_field: BalanceSummaryField,
}
pub fn extract_core_balance_summary<H: PrivatePositionHost>(
    h: &mut H,
    payload: &H::Value,
) -> Result<Option<CoreBalanceSummary>, H::Error> {
    let margin = optional(h, payload, "marginSummary")?;
    let field = if h.truthy(&margin) {
        BalanceSummaryField::MarginSummary
    } else {
        let cross = optional(h, payload, "crossMarginSummary")?;
        if !h.truthy(&cross) {
            return Ok(None);
        }
        BalanceSummaryField::CrossMarginSummary
    };
    let key = h.literal(&text(field.source_name()));
    let first = if h.is_nullish(payload) {
        h.undefined()
    } else {
        h.index(payload, &key)?
    };
    let account = optional(h, &first, "accountValue")?;
    let account = h.number(&account)?;
    let second = if h.is_nullish(payload) {
        h.undefined()
    } else {
        h.index(payload, &key)?
    };
    let used = optional(h, &second, "totalMarginUsed")?;
    let used = h.number(&used)?;
    if !account.is_finite() || !used.is_finite() {
        return Ok(None);
    }
    Ok(Some(CoreBalanceSummary {
        account_value: account,
        total_margin_used: used,
        source_field: field,
    }))
}
pub fn get_message_timestamp<H: PrivatePositionHost>(
    h: &mut H,
    data: &H::Value,
) -> Result<Option<f64>, H::Error> {
    let mut raw = optional(h, data, "serverTime")?;
    if h.is_nullish(&raw) {
        raw = optional(h, data, "time")?;
        if h.is_nullish(&raw) {
            let nested = optional(h, data, "clearinghouseState")?;
            raw = optional(h, &nested, "time")?;
        }
    }
    let timestamp = h.number(&raw)?;
    Ok((timestamp.is_finite() && timestamp > 0.0).then_some(timestamp))
}
