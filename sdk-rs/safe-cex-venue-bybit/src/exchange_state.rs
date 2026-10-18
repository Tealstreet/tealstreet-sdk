//! Pure snapshot selection from BybitExchange; used before publishing REST positions.
use safe_cex_types::Position;
use serde_json::Value;
use std::collections::HashMap;

fn string_value(value: &Value) -> Result<String, &'static str> {
    safe_cex_num::js_string(Some(value)).map_err(|error| error.message)
}
pub(crate) fn number(value: Option<&Value>) -> Result<f64, &'static str> {
    safe_cex_num::js_number(value).map_err(|error| error.message)
}
// ts: BybitExchange.getPositionUpdatedTime
pub fn get_position_updated_time(position: &Position) -> Result<f64, &'static str> {
    let time = position
        .info
        .as_ref()
        .and_then(|info| info.get("updatedTime"));
    let value = safe_cex_num::js_retained_number(time.filter(|value| !value.is_nullish()).or(
        Some(&safe_cex_types::retained_json::RetainedJsonInput::Null),
    ))
    .map_err(|error| error.message)?;
    Ok(if value.is_finite() { value } else { 0. })
}
// ts: BybitExchange.getPositionReliabilityScore
pub fn get_position_reliability_score(position: &Position) -> Result<u32, &'static str> {
    let info = position.info.as_ref();
    let seq = safe_cex_num::js_retained_number(info.and_then(|info| info.get("seq")))
        .map_err(|error| error.message)?;
    let status = info
        .and_then(|info| info.get("positionStatus"))
        .and_then(|value| value.as_text())
        .unwrap_or(&[]);
    let average = info.and_then(|info| info.get("avgPrice"));
    let mut score = 0;
    if position.contracts.is_finite() && position.contracts != 0. {
        score += 1000;
    }
    if get_position_updated_time(position)? > 0. {
        score += 100;
    }
    if seq.is_finite() && seq >= 0. {
        score += 50;
    }
    if !safe_cex_num::js_trim(&String::from_utf16_lossy(status)).is_empty() {
        score += 20;
    }
    if let Some(value) = average.filter(|value| !value.is_nullish())
        && !safe_cex_num::js_trim(&String::from_utf16_lossy(
            &safe_cex_types::js_coercion::js_retained_string_utf16(value)
                .map_err(|error| error.message)?,
        ))
        .is_empty()
    {
        score += 5;
    }
    Ok(score)
}
// ts: BybitExchange.dedupePositionsById
pub fn dedupe_positions_by_id(positions: Vec<Position>) -> Result<Vec<Position>, &'static str> {
    if positions.len() < 2 {
        return Ok(positions);
    }
    let mut indices = HashMap::<String, usize>::new();
    let mut selected: Vec<(Position, u32, f64)> = Vec::new();
    for position in positions {
        let score = get_position_reliability_score(&position)?;
        let time = get_position_updated_time(&position)?;
        if let Some(&index) = indices.get(&position.id) {
            let (_, previous_score, previous_time) = &selected[index];
            if score > *previous_score || (score == *previous_score && time > *previous_time) {
                selected[index] = (position, score, time);
            }
        } else {
            indices.insert(position.id.clone(), selected.len());
            selected.push((position, score, time));
        }
    }
    Ok(selected
        .into_iter()
        .map(|(position, _, _)| position)
        .collect())
}

pub(crate) fn parse_float(value: Option<&Value>) -> Result<f64, &'static str> {
    let text = safe_cex_num::js_string(value).map_err(|error| error.message)?;
    Ok(safe_cex_num::parse_float(&text))
}
#[derive(Clone, Copy, PartialEq)]
enum TickerFamily {
    Primary,
    InverseClass,
    Spot,
}
// ts: BybitExchange.parseTickers — an inverse market's volume24h is in USD contracts and its
// turnover24h in the coin, so volume reads turnover24h and quoteVolume reads volume24h.
pub fn parse_tickers(
    raw: Option<&[Value]>,
    markets: &[safe_cex_types::Market],
    cached: &[safe_cex_types::Ticker],
) -> Result<Vec<safe_cex_types::Ticker>, &'static str> {
    parse_tickers_family(raw, markets, cached, TickerFamily::Primary)
}

// ts: BybitInverseExchange.parseTickers — volume24h and volume24h * lastPrice on every market.
pub fn parse_inverse_class_tickers(
    raw: Option<&[Value]>,
    markets: &[safe_cex_types::Market],
    cached: &[safe_cex_types::Ticker],
) -> Result<Vec<safe_cex_types::Ticker>, &'static str> {
    parse_tickers_family(raw, markets, cached, TickerFamily::InverseClass)
}

// ts: BybitSpotExchange.parseTickers
pub fn parse_spot_tickers(
    raw: Option<&[Value]>,
    markets: &[safe_cex_types::Market],
    cached: &[safe_cex_types::Ticker],
) -> Result<Vec<safe_cex_types::Ticker>, &'static str> {
    parse_tickers_family(raw, markets, cached, TickerFamily::Spot)
}

fn parse_tickers_family(
    raw: Option<&[Value]>,
    markets: &[safe_cex_types::Market],
    cached: &[safe_cex_types::Ticker],
    family: TickerFamily,
) -> Result<Vec<safe_cex_types::Ticker>, &'static str> {
    let spot = family == TickerFamily::Spot;
    let Some(raw) = raw else {
        return Ok(cached.to_vec());
    };
    let markets: HashMap<_, _> = markets.iter().map(|m| (m.symbol.as_str(), m)).collect();
    if raw.iter().any(Value::is_null) {
        return Err("Bybit ticker row is null");
    }
    let mut tickers = Vec::new();
    for row in raw {
        let symbol = row
            .get("symbol")
            .map(string_value)
            .transpose()?
            .unwrap_or_else(|| "undefined".into());
        let Some(market) = markets.get(symbol.as_str()) else {
            continue;
        };
        let value = |key| parse_float(row.get(key));
        let inverse = family == TickerFamily::Primary && market.is_inverse == Some(true);
        let last = value("lastPrice")?;
        let volume = value(if inverse { "turnover24h" } else { "volume24h" })?;
        tickers.push(safe_cex_types::Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid: value("bid1Price")?,
            ask: value("ask1Price")?,
            last,
            mark: value("markPrice")?,
            index: value("indexPrice")?,
            percentage: value("price24hPcnt")? * 100.,
            open_interest: value("openInterest")?,
            funding_rate: if spot || market.is_expiring == Some(true) {
                0.
            } else {
                value("fundingRate")?
            },
            funding_time: if spot || market.is_expiring == Some(true) {
                0.
            } else {
                value("nextFundingTime")?
            },
            volume,
            quote_volume: if inverse {
                value("volume24h")?
            } else {
                volume * last
            },
            mid_price: None,
            timestamp: None,
            info: None,
        });
    }
    Ok(tickers)
}
