//! Reported closed-PnL projection. Raw journal rows have a separate command contract.
use crate::history::ReadCursorHistory;
use crate::model::{Market, PositionSide};
use crate::{ClosedPositionFormat, Cx, VenueError};
use serde::Serialize;

#[path = "history_output_wire.rs"]
mod wire;
use wire::{Field, Row};

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
    notional: Option<f64>,
    contracts: Option<f64>,
    entry_price: Option<f64>,
    exit_price: Option<f64>,
    unrealized_pnl: Option<f64>,
    side: PositionSide,
    fees: Option<f64>,
    time: Option<f64>,
    closed_time: Option<f64>,
    account: &'a str,
    leverage: Option<f64>,
}

fn float(value: &Field) -> Option<f64> {
    let value = value
        .text
        .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    for end in std::iter::once(value.len()).chain(value.char_indices().rev().map(|(end, _)| end)) {
        if let Ok(number) = value[..end].parse::<f64>() {
            return number.is_finite().then_some(number);
        }
    }
    None
}

fn timestamp(value: &Field) -> Option<f64> {
    float(value)?;
    let value = value
        .text
        .trim_start_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    let sign = usize::from(matches!(value.as_bytes().first(), Some(b'+' | b'-')));
    let end = sign + value[sign..].bytes().take_while(u8::is_ascii_digit).count();
    value[..end].parse::<f64>().ok().filter(|v| v.is_finite())
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
            let raw: &serde_json::value::RawValue = serde_json::from_str(row)
                .map_err(|_| VenueError::new("invalid closed-position output"))?;
            if !raw.get().starts_with('{') {
                serde_json::from_str::<wire::ValidatedIgnored>(raw.get())
                    .map_err(|_| VenueError::new("invalid closed-position output"))?;
                continue;
            }
            let row: Row = serde_json::from_str(raw.get())
                .map_err(|_| VenueError::new("invalid closed-position output"))?;
            if let Some(entry) = self.entry(&row) {
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| match (a.closed_time, b.closed_time) {
            (Some(a), Some(b)) => b.total_cmp(&a),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        serde_json::to_string(&entries).map_err(|_| VenueError::new("invalid closed-PnL output"))
    }

    fn entry(&self, row: &Row) -> Option<ClosedPnlEntry<'_>> {
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
                &row.symbol,
                &row.created_time,
                &row.updated_time,
                &row.closed_size,
                &row.avg_entry_price,
                &row.avg_exit_price,
                &row.closed_pnl,
                &row.leverage,
            ),
            ClosedPositionFormat::Okx => (
                &row.inst_id,
                &row.c_time,
                &row.u_time,
                &row.close_total_pos,
                &row.open_avg_px,
                &row.close_avg_px,
                &row.realized_pnl,
                &row.lever,
            ),
            ClosedPositionFormat::Blofin => (
                &row.inst_id,
                &row.create_time,
                &row.update_time,
                &row.close_positions,
                &row.open_average_price,
                &row.close_average_price,
                &row.realized_pnl,
                &row.leverage,
            ),
            ClosedPositionFormat::Bitunix => (
                &row.symbol,
                &row.ctime,
                &row.mtime,
                &row.qty,
                &row.entry_price,
                &row.close_price,
                &row.realized_pnl_bitunix,
                &row.leverage,
            ),
            ClosedPositionFormat::Unsupported => return None,
        };
        let symbol = if self.format == ClosedPositionFormat::Bybit && symbol_key.zero_or_false {
            String::new()
        } else {
            symbol_key.text.clone()
        };
        let time = timestamp(time_key);
        if symbol.is_empty() {
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
        let price = float(exit_key);
        let native = float(size_key);
        let contracts = native
            .and_then(|native| {
                if self.format == ClosedPositionFormat::Bybit
                    && (self.inverse || inverse_symbol(&symbol))
                {
                    let price = price.filter(|price| *price > 0.)?;
                    let value = (native / price) * 1e8;
                    Some(
                        (if value.fract() == 0. {
                            value
                        } else {
                            (value + 0.5).floor()
                        }) / 1e8,
                    )
                } else if self.format == ClosedPositionFormat::Okx
                    && let Some(market) = market
                {
                    let size = market.contract_size.filter(|s| *s != 0.).unwrap_or(1.);
                    if market.is_inverse == Some(true) {
                        Some((native * size) / price.filter(|price| *price > 0.)?)
                    } else {
                        let digits = crate::num::decimals(native) + crate::num::decimals(size);
                        let factor = 10f64.powi(digits as i32);
                        let value = native * size * factor;
                        Some(
                            (if value.fract() == 0. {
                                value
                            } else {
                                (value + 0.5).floor()
                            }) / factor,
                        )
                    }
                } else {
                    Some(native)
                }
            })
            .filter(|value| value.is_finite());
        let notional = if self.format == ClosedPositionFormat::Bybit {
            if self.inverse || inverse_symbol(&symbol) {
                native
            } else {
                float(&row.cum_exit_value)
            }
        } else {
            contracts
                .zip(price)
                .map(|(contracts, price)| contracts * price)
                .filter(|value| value.is_finite())
        };
        let side = match self.format {
            ClosedPositionFormat::Bybit if !row.side.sell_literal => PositionSide::Short,
            ClosedPositionFormat::Okx if row.direction.text.to_lowercase() == "short" => {
                PositionSide::Short
            }
            ClosedPositionFormat::Blofin if row.position_side.text.to_lowercase() == "short" => {
                PositionSide::Short
            }
            ClosedPositionFormat::Bitunix if row.side.text.to_uppercase() == "SELL" => {
                PositionSide::Short
            }
            _ => PositionSide::Long,
        };
        let fees = match self.format {
            ClosedPositionFormat::Bybit => float(&row.open_fee)
                .zip(float(&row.close_fee))
                .map(|(open, close)| open + close)
                .filter(|value| value.is_finite()),
            ClosedPositionFormat::Okx | ClosedPositionFormat::Blofin => {
                float(&row.fee).map(f64::abs)
            }
            _ => float(&row.fee),
        };
        let closed = timestamp(close_time_key);
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
            entry_price: float(entry_key),
            exit_price: price,
            unrealized_pnl: float(pnl_key),
            side,
            fees,
            time,
            closed_time: closed,
            account: &self.account,
            leverage: float(leverage_key),
        })
    }
}

pub fn finish<P>(
    cx: &mut Cx<'_, P>,
    history: ReadCursorHistory<String>,
    result: Result<(), VenueError>,
    mut output: Option<ClosedPnlOutput>,
) {
    if let Some(output) = output.as_mut() {
        output.markets = cx.store.markets().to_vec();
    }
    let _ = history.finish_with(cx, result, |rows| match output {
        Some(output) => output.encode(&rows),
        None => Ok(format!("[{}]", rows.join(","))),
    });
}
