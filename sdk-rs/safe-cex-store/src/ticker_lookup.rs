//! Observable property lookup for the source ticker-symbol dictionary.
//! The borrowed typed map remains an own-entry view. This operation also models
//! the ticker object installed by the source dictionary's __proto__ setter.
use safe_cex_num::{JsCoercionError, ProtoMember, TableHit, js_string, js_table_lookup, js_truthy};
use safe_cex_types::Ticker;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum TickerSymbolLookup {
    OwnTicker(String),
    PrototypeTicker(String),
    PrototypeField { truthy: bool, id: Option<Value> },
    StockInherited(ProtoMember),
    Missing,
}
impl TickerSymbolLookup {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::PrototypeField { truthy, .. } => *truthy,
            Self::Missing => false,
            _ => true,
        }
    }
    /// Read/coerce the captured value's id only after the caller's nonempty
    /// update check, as source Store.updateTicker does. None denotes no target.
    pub fn update_id(self) -> Result<Option<String>, JsCoercionError> {
        match self {
            Self::OwnTicker(id) | Self::PrototypeTicker(id) => Ok(Some(id)),
            Self::PrototypeField { truthy: true, id } => js_string(id.as_ref()).map(Some),
            Self::StockInherited(_) => js_string(None).map(Some),
            Self::PrototypeField { truthy: false, .. } | Self::Missing => Ok(None),
        }
    }
}
pub(crate) fn stock(key: &str) -> TickerSymbolLookup {
    match js_table_lookup::<&str, ()>(&[], key) {
        TableHit::Inherited(member) => TickerSymbolLookup::StockInherited(member),
        _ => TickerSymbolLookup::Missing,
    }
}
pub(crate) fn prototype_field(ticker: &Ticker, key: &str) -> TickerSymbolLookup {
    let numeric = numeric_field(ticker, key);
    if let Some(value) = numeric {
        return TickerSymbolLookup::PrototypeField {
            truthy: value != 0.0 && !value.is_nan(),
            id: None,
        };
    }
    let text = match key {
        "id" => Some(&ticker.id),
        "symbol" => Some(&ticker.symbol),
        _ => None,
    };
    if let Some(text) = text {
        return TickerSymbolLookup::PrototypeField {
            truthy: !text.is_empty(),
            id: None,
        };
    }
    if key == "info" {
        if let Some(value) = &ticker.info {
            return TickerSymbolLookup::PrototypeField {
                truthy: js_truthy(Some(value)),
                id: value.get("id").cloned(),
            };
        }
    }
    if key == "__proto__" {
        return TickerSymbolLookup::PrototypeTicker(ticker.id.clone());
    }
    stock(key)
}

fn numeric_field(ticker: &Ticker, key: &str) -> Option<f64> {
    match key {
        "bid" => Some(ticker.bid),
        "ask" => Some(ticker.ask),
        "last" => Some(ticker.last),
        "mark" => Some(ticker.mark),
        "index" => Some(ticker.index),
        "percentage" => Some(ticker.percentage),
        "openInterest" => Some(ticker.open_interest),
        "fundingRate" => Some(ticker.funding_rate),
        "fundingTime" => Some(ticker.funding_time),
        "volume" => Some(ticker.volume),
        "quoteVolume" => Some(ticker.quote_volume),
        "midPrice" => ticker.mid_price,
        "timestamp" => ticker.timestamp,
        _ => None,
    }
}

pub(crate) fn primitive_assignment_error(
    ticker: &Ticker,
    key: &str,
    field: &str,
) -> Option<String> {
    let text = match key {
        "id" => Some(&ticker.id),
        "symbol" => Some(&ticker.symbol),
        _ => None,
    };
    if let Some(text) = text {
        let readonly = field == "length"
            || field.parse::<usize>().ok().is_some_and(|index| {
                index.to_string() == field && index < text.encode_utf16().count()
            });
        return Some(if readonly {
            format!("Cannot assign to read only property '{field}' of string '{text}'")
        } else {
            format!("Cannot create property '{field}' on string '{text}'")
        });
    }
    numeric_field(ticker, key).map(|value| {
        format!(
            "Cannot create property '{field}' on number '{}'",
            safe_cex_num::js_number_string(value)
        )
    })
}
