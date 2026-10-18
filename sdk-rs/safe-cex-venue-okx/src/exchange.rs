//! OKX exchange policy; orchestration and private state live in the shared core.
use safe_cex_types::OrderStopTriggerType;
use serde::{Deserialize, Serialize};

pub fn is_algo_order(order_type: safe_cex_types::OrderType) -> bool {
    use safe_cex_types::OrderType;
    match order_type {
        OrderType::Market | OrderType::Limit => false,
        OrderType::Conditional
        | OrderType::StopLoss
        | OrderType::TakeProfit
        | OrderType::TrailingStopLoss => true,
    }
}

pub fn get_algo_order_placement_priority(order_type: safe_cex_types::OrderType) -> u8 {
    use safe_cex_types::OrderType;
    match order_type {
        OrderType::StopLoss => 0,
        OrderType::TrailingStopLoss => 1,
        OrderType::TakeProfit => 2,
        OrderType::Market | OrderType::Limit | OrderType::Conditional => 3,
    }
}

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

/// Named binding for findTickerMarkForCurrency's anonymous optional-field input.
#[derive(Debug, Clone, Serialize)]
pub struct TickerMarkInput {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub mark: Option<f64>,
}

#[derive(Deserialize)]
#[serde(remote = "TickerMarkInput")]
struct TickerMarkWire {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    id: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    mark: Option<f64>,
}

impl<'de> Deserialize<'de> for TickerMarkInput {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                TickerMarkWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TickerMarkInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, symbol, mark });
    }
}

pub fn find_ticker_mark_for_currency(tickers: &[TickerMarkInput], currency: &str) -> f64 {
    let candidates = [
        format!("{currency}-USDT-SWAP"),
        format!("{currency}-USD-SWAP"),
        format!("{currency}USDT"),
        format!("{currency}USD"),
    ];
    tickers
        .iter()
        .find(|ticker| {
            ticker.id.as_ref().is_some_and(|id| candidates.contains(id))
                || ticker
                    .symbol
                    .as_ref()
                    .is_some_and(|symbol| candidates.contains(symbol))
        })
        .and_then(|ticker| ticker.mark)
        .unwrap_or(1.0)
}

pub fn get_okx_trigger_px_type(trigger_type: Option<OrderStopTriggerType>) -> &'static str {
    match trigger_type {
        Some(OrderStopTriggerType::IndexPrice) => "index",
        Some(OrderStopTriggerType::MarkPrice) => "mark",
        _ => "last",
    }
}

pub fn parse_okx_trigger_px_type(trigger_type: Option<&str>) -> OrderStopTriggerType {
    match trigger_type {
        Some("index") => OrderStopTriggerType::IndexPrice,
        Some("mark") => OrderStopTriggerType::MarkPrice,
        _ => OrderStopTriggerType::LastPrice,
    }
}

pub fn get_instrument_types() -> [crate::types::OkxInstrumentType; 1] {
    [crate::types::OkxInstrumentType::Swap]
}

pub fn is_supported_market_instrument(market: &serde_json::Map<String, serde_json::Value>) -> bool {
    market.get("instType").and_then(serde_json::Value::as_str) == Some("SWAP")
        && matches!(
            market.get("ctType").and_then(serde_json::Value::as_str),
            Some("linear" | "inverse")
        )
}

pub fn get_market_instrument_type(
    market: &serde_json::Map<String, serde_json::Value>,
) -> Result<crate::types::OkxInstrumentType, safe_cex_num::JsCoercionError> {
    let selected = market
        .get("info")
        .and_then(|info| info.get("instType"))
        .filter(|value| crate::income::truthy(value))
        .or_else(|| {
            market
                .get("instType")
                .filter(|value| crate::income::truthy(value))
        });
    let name = match selected {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => "SWAP".into(),
    };
    Ok(if name == "FUTURES" {
        crate::types::OkxInstrumentType::Futures
    } else {
        crate::types::OkxInstrumentType::Swap
    })
}

// Measured JS /-SWAP$/ without multiline matches the absolute end only,
// including when the input has a trailing line terminator.
fn strip_swap_suffix(value: &str) -> String {
    value.strip_suffix("-SWAP").unwrap_or(value).to_owned()
}

pub fn get_index_ticker_inst_id(
    market: &safe_cex_types::Market,
) -> Result<String, safe_cex_num::JsCoercionError> {
    let underlying = safe_cex_num::js_retained_string(Some(
        market
            .info
            .as_ref()
            .and_then(|value| value.get("uly"))
            .filter(|value| value.is_truthy())
            .unwrap_or(&safe_cex_types::retained_json::RetainedJsonInput::Text(
                Vec::new(),
            )),
    ))?;
    Ok(if underlying.is_empty() {
        strip_swap_suffix(&market.id)
    } else {
        underlying
    })
}

pub fn get_markets_for_index_ticker_inst_id<'a>(
    markets: &'a [safe_cex_types::Market],
    instrument: &str,
) -> Result<Vec<&'a safe_cex_types::Market>, safe_cex_num::JsCoercionError> {
    let mut matching = Vec::new();
    for market in markets {
        if get_index_ticker_inst_id(market)? == instrument {
            matching.push(market);
        }
    }
    Ok(matching)
}

pub fn format_market_symbol(
    market: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, safe_cex_num::JsCoercionError> {
    let id =
        crate::income::truthy_string(market.get("instId").unwrap_or(&serde_json::Value::Null))?;
    let xperp = market.get("instType").and_then(serde_json::Value::as_str) == Some("FUTURES")
        && market.get("ruleType").and_then(serde_json::Value::as_str) == Some("xperp");
    Ok(if xperp { id } else { strip_swap_suffix(&id) }.replace('-', ""))
}

/// Preserve undefined on the cold branch before coercing either display argument.
pub fn format_market_pretty_symbol(
    market: Option<&serde_json::Value>,
    base: Option<&serde_json::Value>,
    quote: Option<&serde_json::Value>,
) -> Result<Option<String>, safe_cex_num::JsCoercionError> {
    let market = match market {
        None => {
            return Err(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of undefined (reading 'instType')",
            });
        }
        Some(serde_json::Value::Null) => {
            return Err(safe_cex_num::JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'instType')",
            });
        }
        Some(market) => market,
    };
    if market.get("instType").and_then(serde_json::Value::as_str) != Some("FUTURES")
        || market.get("ruleType").and_then(serde_json::Value::as_str) != Some("xperp")
    {
        return Ok(None);
    }
    let base = safe_cex_num::js_string(base)?;
    let quote = safe_cex_num::js_string(quote)?;
    Ok(Some(format!("{base}/{quote} XPERP")))
}
