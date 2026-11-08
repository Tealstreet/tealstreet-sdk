//! Reported closed-PnL projection. Raw journal rows have a separate command contract.
use crate::history::ReadCursorHistory;
use crate::model::{Market, PositionSide};
use crate::{ClosedPositionFormat, Cx, VenueError};
use serde::Serialize;
use serde_json::Value;

pub struct ClosedPnlOutput {
    format: ClosedPositionFormat,
    account: String,
    inverse: bool,
    selected: Option<String>,
    markets: Vec<Market>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ClosedPnlEntry<'a> {
    provenance: &'static str,
    symbol: String,
    notional: f64,
    contracts: f64,
    entry_price: f64,
    exit_price: f64,
    unrealized_pnl: f64,
    side: PositionSide,
    fees: f64,
    time: f64,
    closed_time: f64,
    account: &'a str,
    leverage: f64,
}

fn text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Number(value) => {
            let n = value.as_f64().unwrap_or(0.);
            if n == 0. {
                "0".into()
            } else if (1e-6..1e21).contains(&n.abs()) {
                n.to_string()
            } else {
                format!("{n:e}")
            }
        }
        Value::Bool(value) => value.to_string(),
        Value::Array(values) => values.iter().map(text).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

fn float(row: &Value, key: &str) -> f64 {
    let value = text(&row[key]);
    let value = value.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    for end in std::iter::once(value.len()).chain(value.char_indices().rev().map(|(end, _)| end)) {
        if let Ok(number) = value[..end].parse::<f64>() {
            return if number.is_finite() { number } else { 0. };
        }
    }
    0.
}

fn timestamp(row: &Value, key: &str) -> f64 {
    let value = text(&row[key]);
    let value = value.trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let sign = usize::from(matches!(value.as_bytes().first(), Some(b'+' | b'-')));
    let end = sign + value[sign..].bytes().take_while(u8::is_ascii_digit).count();
    value[..end]
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.)
}

fn inverse_symbol(symbol: &str) -> bool {
    let (prefix, suffix) = symbol.rsplit_once("USD").unwrap_or(("", "?"));
    !prefix.is_empty()
        && prefix
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        && (suffix.is_empty()
            || (suffix.len() == 3
                && b"FGHJKMNQUVXZ".contains(&suffix.as_bytes()[0])
                && suffix.as_bytes()[1..].iter().all(u8::is_ascii_digit)))
}

impl ClosedPnlOutput {
    pub fn new(
        format: ClosedPositionFormat,
        account: String,
        inverse: bool,
        selected: Option<String>,
        markets: &[Market],
    ) -> Self {
        Self {
            format,
            account,
            inverse,
            selected,
            markets: markets.to_vec(),
        }
    }

    pub fn encode(&self, rows: &[String]) -> Result<String, VenueError> {
        let mut entries = Vec::new();
        for row in rows {
            let row: Value = serde_json::from_str(row)
                .map_err(|_| VenueError::new("invalid closed-position output"))?;
            if let Some(entry) = self.entry(&row) {
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| b.closed_time.total_cmp(&a.closed_time));
        serde_json::to_string(&entries).map_err(|_| VenueError::new("invalid closed-PnL output"))
    }

    fn entry(&self, row: &Value) -> Option<ClosedPnlEntry<'_>> {
        let (
            symbol_key,
            time_key,
            close_time_key,
            size_key,
            entry_key,
            exit_key,
            pnl_key,
            leverage_key,
        ) = match self.format {
            ClosedPositionFormat::Bybit => (
                "symbol",
                "createdTime",
                "updatedTime",
                "closedSize",
                "avgEntryPrice",
                "avgExitPrice",
                "closedPnl",
                "leverage",
            ),
            ClosedPositionFormat::Okx => (
                "instId",
                "cTime",
                "uTime",
                "closeTotalPos",
                "openAvgPx",
                "closeAvgPx",
                "realizedPnl",
                "lever",
            ),
            ClosedPositionFormat::Blofin => (
                "instId",
                "createTime",
                "updateTime",
                "closePositions",
                "openAveragePrice",
                "closeAveragePrice",
                "realizedPnl",
                "leverage",
            ),
            ClosedPositionFormat::Bitunix => (
                "symbol",
                "ctime",
                "mtime",
                "qty",
                "entryPrice",
                "closePrice",
                "realizedPNL",
                "leverage",
            ),
            ClosedPositionFormat::Unsupported => return None,
        };
        let symbol = text(&row[symbol_key]);
        let time = timestamp(row, time_key);
        if symbol.is_empty()
            || time == 0.
            || (self.format == ClosedPositionFormat::Bybit && time <= 0.)
        {
            return None;
        }
        let market = if self.format == ClosedPositionFormat::Okx {
            self.selected
                .as_ref()
                .and_then(|s| self.markets.iter().find(|m| &m.symbol == s))
                .or_else(|| self.markets.iter().find(|m| m.id == symbol))
        } else {
            self.markets.iter().find(|m| m.id == symbol)
        };
        let price = float(row, exit_key);
        let native = float(row, size_key);
        let mut contracts = native;
        if self.format == ClosedPositionFormat::Bybit
            && (self.inverse || inverse_symbol(&symbol))
            && price > 0.
        {
            let value = (native / price) * 1e8;
            contracts = if value.fract() == 0. {
                value
            } else {
                (value + 0.5).floor()
            } / 1e8;
        } else if self.format == ClosedPositionFormat::Okx
            && price > 0.
            && let Some(market) = market
        {
            let size = market.contract_size.filter(|s| *s != 0.).unwrap_or(1.);
            contracts = if market.is_inverse == Some(true) {
                (native * size) / price
            } else {
                let digits = crate::num::decimals(native) + crate::num::decimals(size);
                let factor = 10f64.powi(digits as i32);
                let value = native * size * factor;
                (if value.fract() == 0. {
                    value
                } else {
                    (value + 0.5).floor()
                }) / factor
            };
        }
        let notional = if self.format == ClosedPositionFormat::Bybit {
            if self.inverse || inverse_symbol(&symbol) {
                native
            } else {
                float(row, "cumExitValue")
            }
        } else {
            contracts * price
        };
        let side = match self.format {
            ClosedPositionFormat::Bybit if row["side"] != "Sell" => PositionSide::Short,
            ClosedPositionFormat::Okx if text(&row["direction"]).to_lowercase() == "short" => {
                PositionSide::Short
            }
            ClosedPositionFormat::Blofin
                if text(&row["positionSide"]).to_lowercase() == "short" =>
            {
                PositionSide::Short
            }
            ClosedPositionFormat::Bitunix if text(&row["side"]).to_uppercase() == "SELL" => {
                PositionSide::Short
            }
            _ => PositionSide::Long,
        };
        let fees = match self.format {
            ClosedPositionFormat::Bybit => float(row, "openFee") + float(row, "closeFee"),
            ClosedPositionFormat::Okx | ClosedPositionFormat::Blofin => float(row, "fee").abs(),
            _ => float(row, "fee"),
        };
        let closed = timestamp(row, close_time_key);
        Some(ClosedPnlEntry {
            provenance: "venue",
            symbol: if matches!(
                self.format,
                ClosedPositionFormat::Okx | ClosedPositionFormat::Blofin
            ) {
                market.map_or(symbol, |m| m.symbol.clone())
            } else {
                symbol
            },
            notional,
            contracts,
            entry_price: float(row, entry_key),
            exit_price: price,
            unrealized_pnl: float(row, pnl_key),
            side,
            fees,
            time,
            closed_time: if closed == 0.
                || (self.format == ClosedPositionFormat::Bybit && closed <= 0.)
            {
                time
            } else {
                closed
            },
            account: &self.account,
            leverage: float(row, leverage_key),
        })
    }
}

pub fn finish<P>(
    cx: &mut Cx<'_, P>,
    history: ReadCursorHistory<String>,
    result: Result<(), VenueError>,
    output: Option<ClosedPnlOutput>,
) {
    let _ = history.finish_with(cx, result, |rows| match output {
        Some(output) => output.encode(&rows),
        None => Ok(format!("[{}]", rows.join(","))),
    });
}
