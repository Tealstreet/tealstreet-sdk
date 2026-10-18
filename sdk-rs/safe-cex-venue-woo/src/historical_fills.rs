//! Historical fill callers, composed over the shared exchange and REST boundaries.
use crate::{
    history_fetch::fetch_transaction_history_rows,
    legacy_fills::WooLegacyFillsQuery,
    response,
    transaction::{ParsedWooTransaction, parse_transaction_history_row},
};
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::Exchange, runtime::ExchangeError,
};
use safe_cex_transport::{RestAdapter, RestFailure};
use safe_cex_types::{Execution, ExecutionsOptions, HistoricalTrade, HistoricalTradeOptions};
use serde_json::Value;

fn bounds(value: Option<f64>, milliseconds: bool) -> Result<Option<f64>, ExchangeError> {
    // Source ignores nonfinite bounds; do not project NaN through JSON.
    let value = value.filter(|v| v.is_finite()).map(Value::from);
    let result = if milliseconds {
        response::normalize_timestamp_milliseconds(value.as_ref())
    } else {
        response::normalize_timestamp_seconds(value.as_ref())
    };
    result.map_err(|error| ExchangeError::new(error.message))
}
fn rest_error(failure: RestFailure) -> ExchangeError {
    failure.into()
}

fn report(venue: &dyn Exchange, error: ExchangeError) -> Result<(), ExchangeError> {
    let response = error.retained_response_input();
    let response_message = response
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("message"));
    let input = match response_message.filter(|v| safe_cex_num::js_retained_truthy(v)) {
        Some(safe_cex_types::retained_json::RetainedJsonInput::Text(message)) => {
            ExchangeErrorInput::Message(String::from_utf16_lossy(message))
        }
        Some(_) => {
            return Err(ExchangeError::new(
                "history error reporter cannot retain a non-string response message",
            ));
        }
        None => ExchangeErrorInput::Message(error.message),
    };
    venue.base().emit_and_log_error(input, None, None)
}
fn parse_rows(rows: Vec<Value>) -> Result<Vec<ParsedWooTransaction>, ExchangeError> {
    let mut parsed = Vec::new();
    for row in rows {
        let Some(row) = row.as_object() else {
            safe_cex_num::get_key::v_unknown(Some(&row), "symbol")
                .map_err(|error| ExchangeError::new(error.message))?;
            continue;
        };
        if let Some(row) = parse_transaction_history_row(row).map_err(ExchangeError::new)? {
            parsed.push(row);
        }
    }
    Ok(parsed)
}
fn select(
    rows: &mut Vec<ParsedWooTransaction>,
    symbol: Option<&str>,
    order: Option<&str>,
    from: Option<f64>,
    to: Option<f64>,
    limit: f64,
    milliseconds: bool,
) {
    rows.retain(|row| {
        let time = if milliseconds {
            row.timestamp_milliseconds
        } else {
            row.timestamp_seconds
        };
        symbol.is_none_or(|s| row.symbol == s)
            && order.is_none_or(|s| row.order_id.as_deref() == Some(s))
            && from.is_none_or(|v| time >= v)
            && to.is_none_or(|v| time <= v)
    });
    rows.sort_by(|a, b| {
        let (a, b) = if milliseconds {
            (a.timestamp_milliseconds, b.timestamp_milliseconds)
        } else {
            (a.timestamp_seconds, b.timestamp_seconds)
        };
        b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal)
    });
    let count = if limit.is_nan() {
        0
    } else {
        limit.trunc() as usize
    };
    rows.truncate(count);
}
fn asset(row: &ParsedWooTransaction) -> Result<Option<String>, ExchangeError> {
    match &row.commission_asset {
        None => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(ExchangeError::new(
            "history commissionAsset cannot cross the typed String output",
        )),
    }
}
fn executions(rows: Vec<ParsedWooTransaction>) -> Result<Vec<Execution>, ExchangeError> {
    rows.into_iter()
        .map(|row| {
            Ok(Execution {
                commission_asset: asset(&row)?,
                id: row.fill_id,
                timestamp: row.timestamp_seconds,
                symbol: row.symbol,
                order_id: row.order_id,
                price: row.price,
                qty: row.qty,
                side: row.side,
                commission: row.commission,
                position_side: Some(row.position_side),
                realized_pnl: row.realized_pnl,
                maker: row.maker,
                venue_reported_revenue: None,
                venue_reported_revenue_asset: None,
            })
        })
        .collect()
}
fn trades(rows: Vec<ParsedWooTransaction>) -> Result<Vec<HistoricalTrade>, ExchangeError> {
    rows.into_iter()
        .map(|row| {
            Ok(HistoricalTrade {
                commission_asset: asset(&row)?,
                id: row.fill_id,
                order_id: row.order_id.filter(|id| !id.is_empty()).unwrap_or_else(|| {
                    format!(
                        "{}-{}",
                        row.symbol,
                        safe_cex_num::js_number_string(row.timestamp_milliseconds)
                    )
                }),
                symbol: row.symbol,
                price: row.price,
                qty: row.qty,
                side: row.side,
                commission: row.commission,
                position_side: row.position_side,
                realized_pnl: row.realized_pnl,
                maker: row.maker,
                time: row.timestamp_milliseconds,
                quote_qty: None,
                buyer: None,
            })
        })
        .collect()
}

async fn internal_rows(
    venue: &dyn Exchange,
    adapter: &dyn RestAdapter,
    opts: &HistoricalTradeOptions,
) -> Result<Vec<ParsedWooTransaction>, ExchangeError> {
    if venue.is_public_data_only() || opts.symbol.is_empty() {
        return Ok(Vec::new());
    }
    let Some(symbol) =
        response::normalize_requested_symbol(Some(&opts.symbol)).filter(|s| !s.is_empty())
    else {
        return Ok(Vec::new());
    };
    let limit = safe_cex_num::math_min(
        safe_cex_num::math_max(opts.limit.unwrap_or(500.0), 1.0),
        500.0,
    );
    let from = bounds(opts.from, true)?;
    let to = bounds(opts.to, true)?;
    let query = WooLegacyFillsQuery {
        symbol: Some(symbol.clone()),
        from_milliseconds: from,
        to_milliseconds: to,
        limit,
        page: None,
    };
    let rows = match fetch_transaction_history_rows(adapter, &query)
        .await
        .map_err(rest_error)
        .and_then(parse_rows)
    {
        Ok(rows) => rows,
        Err(error) => {
            report(venue, error)?;
            return Ok(Vec::new());
        }
    };
    let mut rows = rows;
    select(
        &mut rows,
        Some(&symbol),
        opts.order_id.as_deref().filter(|s| !s.is_empty()),
        from,
        to,
        limit,
        true,
    );
    Ok(rows)
}

// ts: WOOXExchange#fetchHistoricalTradesInternal
pub async fn fetch_historical_trades_internal(
    venue: &dyn Exchange,
    adapter: &dyn RestAdapter,
    opts: &HistoricalTradeOptions,
) -> Result<Vec<HistoricalTrade>, ExchangeError> {
    trades(internal_rows(venue, adapter, opts).await?)
}

// ts: WOOXExchange#fetchHistoricalFills
pub async fn fetch_historical_fills(
    venue: &dyn Exchange,
    adapter: &dyn RestAdapter,
    opts: &ExecutionsOptions,
) -> Result<Vec<Execution>, ExchangeError> {
    if venue.is_public_data_only() {
        return Ok(Vec::new());
    }
    let symbol =
        response::normalize_requested_symbol(opts.symbol.as_deref()).filter(|s| !s.is_empty());
    let order = opts.order_id.as_deref().filter(|s| !s.is_empty());
    let from = bounds(opts.from, false)?;
    let to = bounds(opts.to, false)?;
    let limit = safe_cex_num::math_min(
        safe_cex_num::math_max(opts.limit.unwrap_or(500.0), 1.0),
        500.0,
    );
    let query = WooLegacyFillsQuery {
        symbol: symbol.clone(),
        from_milliseconds: bounds(opts.from, true)?,
        to_milliseconds: bounds(opts.to, true)?,
        limit,
        page: None,
    };
    let mut rows = match fetch_transaction_history_rows(adapter, &query)
        .await
        .map_err(rest_error)
        .and_then(parse_rows)
    {
        Ok(rows) => rows,
        Err(error) => {
            report(venue, error)?;
            return Ok(Vec::new());
        }
    };
    select(&mut rows, symbol.as_deref(), order, from, to, limit, false);
    if rows.is_empty()
        && let Some(symbol) = symbol
    {
        let options = HistoricalTradeOptions {
            symbol,
            order_id: order.map(str::to_owned),
            from: opts.from,
            to: opts.to,
            limit: Some(limit),
            from_id: None,
            account_name: None,
        };
        let history = match internal_rows(venue, adapter, &options).await {
            Ok(rows) => rows,
            Err(error) => {
                report(venue, error)?;
                return Ok(Vec::new());
            }
        };
        let history = trades(history)?;
        let mut result: Vec<_> = history
            .into_iter()
            .map(|trade| Execution {
                id: trade.id,
                timestamp: (trade.time / 1000.0).floor(),
                symbol: trade.symbol,
                order_id: Some(trade.order_id),
                price: trade.price,
                qty: trade.qty,
                side: trade.side,
                commission: trade.commission,
                commission_asset: trade.commission_asset,
                position_side: Some(trade.position_side),
                realized_pnl: trade.realized_pnl,
                maker: trade.maker,
                venue_reported_revenue: None,
                venue_reported_revenue_asset: None,
            })
            .collect();
        result.sort_by(|a, b| {
            b.timestamp
                .partial_cmp(&a.timestamp)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.truncate(if limit.is_nan() {
            0
        } else {
            limit.trunc() as usize
        });
        return Ok(result);
    }
    executions(rows)
}
