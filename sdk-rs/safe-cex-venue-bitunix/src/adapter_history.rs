//! History pagers and journal axes bound to the ported page walks and mappers.
use crate::{
    adapter::{BitunixVenue, coercion, reason, to_value, units, utf8},
    history_reads::{HistoryPageRequest, HistoryRowSelectors},
};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue, js_string};
use safe_cex_types::{
    ClosedPnlEntry, HistoricalOrder, HistoricalTradeOptions, IncomeHistoryOptions, IncomeRecord,
    OrderHistoryOptions,
};
use serde_json::Value;
use std::{future::Future, pin::Pin, rc::Rc};

type Rows = Pin<Box<dyn Future<Output = Result<Vec<Value>, ExchangeError>>>>;
/// String(row[field] ?? …): nullish fields fall through to the next selector.
fn field(row: &Value, name: &'static str) -> Result<Option<Vec<u16>>, ExchangeError> {
    match row.get(name) {
        None | Some(Value::Null) => Ok(None),
        value => Ok(Some(units(&js_string(value).map_err(coercion)?))),
    }
}
fn params(request: &HistoryPageRequest<String>) -> JsJsonValue {
    let mut fields = vec![];
    if let Some(symbol) = &request.symbol {
        fields.push(("symbol".to_owned(), JsJsonValue::String(symbol.clone())));
    }
    for (key, value) in [
        ("startTime", request.start_time),
        ("endTime", request.end_time),
    ] {
        if let Some(value) = value {
            fields.push((key.into(), JsJsonValue::Number(value)));
        }
    }
    fields.push(("limit".into(), JsJsonValue::Number(request.limit)));
    if let Some(skip) = request.skip {
        fields.push(("skip".into(), JsJsonValue::Number(skip)));
    }
    JsJsonValue::Object(vec![("params".into(), JsJsonValue::Object(fields))])
}
fn get(value: &Value, key: &str) -> Value {
    value.get(key).cloned().unwrap_or(Value::Null)
}
impl BitunixVenue {
    /// fetchBitunixHistoryPages(endpoint, start, end, timeOf, idOf, symbol)
    fn history_pages(
        self: &Rc<Self>,
        endpoint: &'static str,
        start: Option<f64>,
        end: Option<f64>,
        selectors: HistoryRowSelectors<Value, ExchangeError>,
        symbol: Option<String>,
    ) -> Rows {
        let me = self.clone();
        Box::pin(async move {
            let xhr = me.xhr();
            let mut read = |row: &Value, name: &'static str| field(row, name);
            let HistoryRowSelectors {
                time_fields,
                id_field,
                time_of,
                id_of,
            } = selectors;
            let mut id_read = |row: &Value, name: &'static str| field(row, name);
            crate::history_reads::fetch_bitunix_history_pages(
                &units(endpoint),
                start,
                end,
                symbol,
                |path, request| {
                    let f = utf8(path).map(|path| xhr.get(&path, params(&request)));
                    async move { f?.await }
                },
                (
                    |body: Value| Ok(get(&body, "data")),
                    |d: &Value, k| Ok(get(d, k)),
                    Value::is_array,
                ),
                (
                    |d: Value| Ok(d.as_array().cloned().unwrap_or_default()),
                    |l: &Vec<Value>, i| Ok(l.get(i).cloned()),
                    |l: &Vec<Value>| Ok(l.len() as f64),
                ),
                |row| time_of(row, time_fields, &mut read),
                |row| id_of(row, id_field, &mut id_read),
                |text, severity| me.log(&utf8(&text)?, severity),
                |e| me.emit_error(&reason(&e)),
            )
            .await
        })
    }
    pub(crate) fn fetch_income_history(
        self: &Rc<Self>,
        o: Option<IncomeHistoryOptions>,
    ) -> ExchangeFuture<'static, Vec<IncomeRecord>> {
        let me = self.clone();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            let (start, end, symbol) =
                o.map_or((None, None, None), |o| (o.start_time, o.end_time, o.symbol));
            crate::history_reads::fetch_income_history(
                || Ok(me.public_only()),
                || Ok((start, end, symbol)),
                |endpoint, from, to, selectors, symbol| {
                    me.history_pages(endpoint, from, to, selectors, symbol)
                },
                |row| {
                    crate::history_mappers::map_bitunix_trade_to_income_records(&row)
                        .map_err(coercion)
                },
                |r: &IncomeRecord| Ok(r.timestamp),
            )
            .await
        })
    }
    pub(crate) fn fetch_income_history_range(
        self: &Rc<Self>,
        start: f64,
        end: f64,
        kind: Option<String>,
    ) -> ExchangeFuture<'static, Vec<IncomeRecord>> {
        let me = self.clone();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            crate::exchange_lifecycle::fetch_income_history_range(start, end, kind, |range| {
                me.fetch_income_history(Some(IncomeHistoryOptions {
                    start_time: Some(range.start_time),
                    end_time: Some(range.end_time),
                    income_type: range.income_type,
                    symbol: None,
                    limit: None,
                }))
            })
            .await
        })
    }
    fn fetch_closed_position_rows(
        self: &Rc<Self>,
        from: Option<f64>,
        to: Option<f64>,
        symbol: Option<String>,
    ) -> Rows {
        let me = self.clone();
        Box::pin(async move {
            crate::history_reads::fetch_closed_position_rows(
                || Ok(me.public_only()),
                || Ok((from, to, symbol)),
                |endpoint, from, to, selectors, symbol| {
                    me.history_pages(endpoint, from, to, selectors, symbol)
                },
            )
            .await
        })
    }
    pub(crate) fn fetch_closed_pnl(
        self: &Rc<Self>,
        o: HistoricalTradeOptions,
    ) -> ExchangeFuture<'static, Vec<ClosedPnlEntry>> {
        let me = self.clone();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            let account = o.account_name.clone().unwrap_or_default();
            crate::history_reads::fetch_closed_pnl(
                || Ok(me.public_only()),
                || Ok((o.from, o.to, Some(o.symbol.clone()))),
                |endpoint, from, to, selectors, symbol| {
                    me.history_pages(endpoint, from, to, selectors, symbol)
                },
                || Ok(account.clone()),
                |row, account| {
                    crate::history_mappers::map_bitunix_closed_position(&row, &account)
                        .map_err(coercion)
                },
                |e: &ClosedPnlEntry| Ok(e.closed_time),
            )
            .await
        })
    }
    pub(crate) fn fetch_historical_positions(
        self: &Rc<Self>,
        o: safe_cex_exchange::journal::HistoricalPositionOptions,
    ) -> ExchangeFuture<'static, Vec<Value>> {
        let me = self.clone();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            let quotes = me.clone();
            let records = crate::history_reads::fetch_historical_positions(
                || Ok((o.from, o.to)),
                |from, to| me.fetch_closed_position_rows(from, to, None),
                || Ok(o.account.clone()),
                |symbol: String| {
                    let markets = quotes.base.store.markets_by_symbol();
                    Ok(markets.get(&symbol).map(|m| units(&m.quote)))
                },
                |row: Value, account: String, resolve: &mut dyn FnMut(String) -> Result<Vec<u16>, ExchangeError>| {
                    let mut failure = None;
                    let mut settle = |symbol: &str| match resolve(symbol.to_owned()).and_then(|q| utf8(&q)) {
                        Ok(quote) => quote,
                        Err(e) => {
                            failure = Some(e);
                            String::new()
                        }
                    };
                    let record = crate::position_adapter::map_bitunix_position_history(
                        &row,
                        &mut crate::position_adapter::BitunixPositionAdapterDeps {
                            account: &account,
                            resolve_settle_currency: &mut settle,
                            realized_is_net_of_funding: false,
                        },
                    )
                    .map_err(ExchangeError::new)?;
                    match failure {
                        Some(e) => Err(e),
                        None => Ok(record),
                    }
                },
            )
            .await?;
            records.iter().map(to_value).collect()
        })
    }
    pub(crate) fn fetch_historical_orders(
        self: &Rc<Self>,
        o: Option<OrderHistoryOptions>,
    ) -> ExchangeFuture<'static, Vec<HistoricalOrder>> {
        let me = self.clone();
        safe_cex_exchange::eager::start_eager(&self.base.runtime, async move {
            let (from, to, limit, symbol) = o.map_or((None, None, None, None), |o| {
                (o.from, o.to, o.limit, o.symbol)
            });
            let xhr = me.xhr();
            crate::history_reads::fetch_historical_orders(
                || Ok(me.public_only()),
                |key| {
                    Ok(match key {
                        "from" => from,
                        "to" => to,
                        _ => limit,
                    })
                },
                || Ok(symbol.clone()),
                |endpoint, request| xhr.get(endpoint, params(&request)),
                (
                    |body: Value| {
                        Ok(get(&get(&body, "data"), "orderList")
                            .as_array()
                            .cloned()
                            .unwrap_or_default())
                    },
                    |l: &Vec<Value>, i| Ok(l.get(i).cloned()),
                    |l: &Vec<Value>| Ok(l.len() as f64),
                ),
                |row: &Value, name| field(row, name),
                |row| crate::history_mappers::map_bitunix_historical_order(&row).map_err(coercion),
                |order: &HistoricalOrder| Ok(order.created_time),
                |message, severity| me.log(message, severity),
                |e| me.emit_error(&reason(&e)),
            )
            .await
        })
    }
}
