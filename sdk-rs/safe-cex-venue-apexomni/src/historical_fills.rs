//! REST pagination, history fallback and bounded execution-cache fallback.
use crate::{
    exchange::{ApexOmniExchange, ApexRequest, failure},
    executions,
    response::{field_string, float, get, text, truthy},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as Q, js_number_string, math_max, math_min};
use safe_cex_types::retained_json::RetainedJsonInput as J;
use safe_cex_types::{Execution, ExecutionsOptions, Market, OrderSide};
fn first<'a>(row: &'a J, keys: &[&str]) -> &'a J {
    keys.iter()
        .map(|k| get(row, k))
        .find(|v| truthy(v))
        .unwrap_or(&J::Undefined)
}
fn defined(row: &J, keys: &[&str], extra: Option<J>) -> Result<J, ExchangeError> {
    for value in keys.iter().map(|k| get(row, k)).chain(extra.iter()) {
        if !matches!(value, J::Undefined | J::Null) && !text(value)?.is_empty() {
            return Ok(if truthy(value) {
                value.clone()
            } else {
                J::Text("0".encode_utf16().collect())
            });
        }
    }
    Ok(J::Text("0".encode_utf16().collect()))
}
fn extract(payload: &J) -> Vec<J> {
    let body = [get(payload, "body"), get(payload, "data"), payload]
        .into_iter()
        .find(|v| !matches!(v, J::Undefined | J::Null))
        .unwrap_or(payload);
    let root = [get(body, "data"), get(body, "result"), body]
        .into_iter()
        .find(|v| !matches!(v, J::Undefined | J::Null))
        .unwrap_or(body);
    [
        get(root, "fills"),
        get(root, "orders"),
        get(root, "list"),
        get(root, "rows"),
        get(root, "results"),
        root,
    ]
    .into_iter()
    .find_map(|v| {
        if let J::Array(rows) = v {
            Some(rows.clone())
        } else {
            None
        }
    })
    .unwrap_or_default()
}
fn filled_history(payload: &J) -> Result<Vec<J>, ExchangeError> {
    let mut rows = Vec::new();
    for row in extract(payload) {
        if matches!(row, J::Null | J::Undefined) {
            let mut error = failure(format!(
                "Cannot read properties of {} (reading 'cumSuccessFillSize')",
                if matches!(row, J::Null) {
                    "null"
                } else {
                    "undefined"
                }
            ));
            error.name = "TypeError".into();
            return Err(error);
        }
        let raw = get(&row, "cumSuccessFillSize");
        let amount = if truthy(raw) { float(raw)? } else { 0. };
        if amount > 0. {
            rows.push(row);
        }
    }
    Ok(rows)
}
fn milliseconds(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite()).map(|v| {
        if v > 1e12 {
            v.floor()
        } else {
            (v * 1000.).floor()
        }
    })
}
fn message(error: &ExchangeError) -> Result<String, ExchangeError> {
    if let Some(value) = error
        .response
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("msg"))
        && safe_cex_num::js_truthy(Some(value))
    {
        return text(&J::from(value.clone()));
    }
    Ok(if error.message.is_empty() {
        error.js_error_string()
    } else {
        error.message.clone()
    })
}
impl ApexOmniExchange {
    fn parse_history_rows(
        &self,
        rows: &[J],
        market: &Market,
        order_id: Option<&str>,
        fallback: bool,
    ) -> Result<Vec<(Execution, J)>, ExchangeError> {
        let expected = [
            self.normalize_apex_symbol_key(&J::Text((market.id.clone()).encode_utf16().collect()))?,
            self.normalize_apex_symbol_key(&J::Text(
                (market.symbol.clone()).encode_utf16().collect(),
            ))?,
        ];
        let mut result = Vec::new();
        for row in rows {
            if matches!(row, J::Undefined | J::Null) {
                let mut error = failure(format!(
                    "Cannot read properties of {} (reading 'symbol')",
                    if matches!(row, J::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                ));
                error.name = "TypeError".into();
                return Err(error);
            }
            let raw = executions::row_symbol(row);
            if !matches!(raw, J::Undefined | J::Null) && !text(raw)?.is_empty() {
                let symbol = text(raw)?;
                let resolved = self
                    .base
                    .store
                    .markets_by_id()
                    .get(&symbol)
                    .cloned()
                    .or_else(|| self.base.store.markets_by_symbol().get(&symbol).cloned());
                let symbol = resolved.as_ref().map_or(symbol, |m| {
                    if m.id.is_empty() {
                        m.symbol.clone()
                    } else {
                        m.id.clone()
                    }
                });
                if !expected.contains(
                    &self.normalize_apex_symbol_key(&J::Text((symbol).encode_utf16().collect()))?,
                ) {
                    continue;
                }
            }
            let Some(side) = executions::execution_side(first(row, &["side", "S"]))? else {
                continue;
            };
            let value = if truthy(first(row, &["cumSuccessFillValue", "cumMatchFillValue"])) {
                float(first(row, &["cumSuccessFillValue", "cumMatchFillValue"]))?
            } else {
                0.
            };
            let size = if truthy(first(row, &["cumSuccessFillSize", "cumMatchFillSize"])) {
                float(first(row, &["cumSuccessFillSize", "cumMatchFillSize"]))?
            } else {
                0.
            };
            let derived = (value.is_finite() && size.is_finite() && size > 0.)
                .then(|| J::Float(value / size));
            let price = float(&defined(
                row,
                &[
                    "fillPrice",
                    "fillPx",
                    "execPrice",
                    "latestMatchFillPrice",
                    "avgPrice",
                    "price",
                    "p",
                ],
                derived,
            )?)?;
            let quantity = if fallback {
                let value = get(row, "cumSuccessFillSize");
                if !matches!(value, J::Undefined | J::Null) && !text(value)?.is_empty() {
                    if truthy(value) {
                        value.clone()
                    } else {
                        J::Text("0".encode_utf16().collect())
                    }
                } else {
                    defined(
                        row,
                        &[
                            "fillSize",
                            "execQty",
                            "matchFillSize",
                            "lastFillSize",
                            "qty",
                            "q",
                            "amount",
                            "size",
                            "v",
                            "cumSuccessFillSize",
                            "cumMatchFillSize",
                        ],
                        None,
                    )?
                }
            } else {
                defined(
                    row,
                    &[
                        "fillSize",
                        "execQty",
                        "matchFillSize",
                        "lastFillSize",
                        "qty",
                        "q",
                        "amount",
                        "size",
                        "v",
                        "cumSuccessFillSize",
                        "cumMatchFillSize",
                    ],
                    None,
                )?
            };
            let qty = float(&quantity)?;
            if !price.is_finite() || !qty.is_finite() || price <= 0. || qty <= 0. {
                continue;
            }
            let Some(timestamp) =
                self.normalize_execution_timestamp_to_seconds(executions::row_time(row))?
            else {
                continue;
            };
            if timestamp == 0. || timestamp.is_nan() {
                continue;
            }
            let id = first(
                row,
                &["orderId", "orderID", "id", "i", "clientOrderId", "clientId"],
            );
            let id = if matches!(id, J::Undefined) {
                String::new()
            } else {
                text(id)?
            };
            if order_id.is_some_and(|filter| !filter.is_empty() && !id.is_empty() && id != filter) {
                continue;
            }
            let fee = float(&defined(
                row,
                &[
                    "fee",
                    "fillFee",
                    "execFee",
                    "cumSuccessFillFee",
                    "cumMatchFillFee",
                ],
                None,
            )?)?;
            let asset = first(
                row,
                &[
                    "token",
                    "feeToken",
                    "feeAsset",
                    "feeCurrency",
                    "settleAssetId",
                ],
            );
            let asset = if matches!(asset, J::Undefined) {
                J::Text((market.quote.clone()).encode_utf16().collect())
            } else {
                asset.clone()
            };
            result.push((
                Execution {
                    timestamp,
                    symbol: market.symbol.clone(),
                    order_id: (!id.is_empty()).then_some(id),
                    price,
                    qty,
                    side,
                    commission: fee.is_finite().then(|| fee.abs()),
                    commission_asset: None,
                    id: None,
                    position_side: None,
                    realized_pnl: None,
                    maker: None,
                    venue_reported_revenue: None,
                    venue_reported_revenue_asset: None,
                },
                asset,
            ));
        }
        Ok(result)
    }
    async fn history_request(
        &self,
        path: &'static str,
        symbol: &str,
        from: Option<f64>,
        to: Option<f64>,
        limit: f64,
        timeout: f64,
        page: Option<usize>,
        history: bool,
    ) -> Result<J, ExchangeError> {
        let mut params = vec![
            ("limit".into(), Q::Number(limit)),
            ("symbol".into(), Q::String(symbol.into())),
        ];
        if let Some(page) = page {
            params.insert(0, ("page".into(), Q::Number(page as f64)));
        }
        if history {
            params.push(("orderType".into(), Q::String("HISTORY".into())));
        }
        if let Some(from) = from {
            params.push(("beginTimeInclusive".into(), Q::Number(from)));
        }
        if let Some(to) = to {
            params.push(("endTimeExclusive".into(), Q::Number(to)));
        }
        self.host
            .request(ApexRequest {
                method: "get",
                path,
                params: Q::Object(params),
                data: Q::Undefined,
                content_type: None,
                timeout_ms: Some(timeout),
                unlimited: false,
            })
            .await
    }
    // ts: ApexOmniExchange.fetchHistoricalFills. The closed shared extra record omits the undocumented timeout setting.
    pub async fn fetch_historical_fills(
        &self,
        opts: &ExecutionsOptions,
        configured_timeout: &Q,
    ) -> Result<Vec<Execution>, ExchangeError> {
        if self.base.options().public_data_only == Some(true) || !self.base.can_sign() {
            return Ok(Vec::new());
        }
        let Some(symbol) = opts.symbol.as_ref().filter(|s| !s.is_empty()) else {
            return Ok(Vec::new());
        };
        let Some(market) = self.base.store.markets_by_symbol().get(symbol).cloned() else {
            return Ok(Vec::new());
        };
        let limit = math_min(math_max(opts.limit.unwrap_or(100.), 1.), 100.);
        let configured = crate::data::number(configured_timeout).map_err(failure)?;
        let timeout = if configured.is_finite() && configured > 0. {
            configured
        } else {
            20000.
        };
        let from = milliseconds(opts.from);
        let to = milliseconds(opts.to);
        let mut symbols = Vec::new();
        for s in [&market.id, &market.symbol] {
            if !s.is_empty() && !symbols.contains(s) {
                symbols.push(s.clone());
            }
        }
        let ws = || {
            self.get_ws_execution_fallback(
                &market.symbol,
                opts.order_id.as_deref(),
                from,
                to,
                limit,
            )
        };
        let result = async {
            let max_pages = if from.is_some_and(|v| v != 0.) || to.is_some_and(|v| v != 0.) {
                3
            } else {
                1
            };
            let mut primary = Vec::new();
            let mut last_error = None;
            for symbol in &symbols {
                let mut rows = Vec::new();
                let mut page = 0;
                let fetched = async {
                    while page < max_pages && (rows.len() as f64) < limit {
                        let payload = self
                            .history_request(
                                "/v3/fills",
                                symbol,
                                from,
                                to,
                                limit,
                                timeout,
                                (page > 0).then_some(page),
                                false,
                            )
                            .await?;
                        let page_rows = extract(&payload);
                        if page_rows.is_empty() {
                            break;
                        }
                        let count = page_rows.len();
                        rows.extend(page_rows);
                        if (count as f64) < limit {
                            break;
                        }
                        page += 1;
                    }
                    Ok::<_, ExchangeError>(())
                }
                .await;
                match fetched {
                    Ok(()) => {
                        if !rows.is_empty() {
                            primary = rows;
                            break;
                        }
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            if primary.is_empty()
                && let Some(error) = last_error
            {
                self.base.log(&format!(
                    "Execution history fills request failed, falling back to history-orders: {}",
                    message(&error)?,
                ), "warning")?;
            }
            let mut fallback = Vec::new();
            if primary.is_empty() {
                let mut last_error = None;
                for symbol in &symbols {
                    let request = self
                        .history_request(
                            "/v3/history-orders",
                            symbol,
                            from,
                            to,
                            limit,
                            timeout,
                            None,
                            true,
                        )
                        .await;
                    match request.and_then(|payload| filled_history(&payload)) {
                        Ok(rows) => {
                            if !rows.is_empty() {
                                fallback = rows;
                                break;
                            }
                        }
                        Err(error) => last_error = Some(error),
                    }
                }
                if fallback.is_empty()
                    && let Some(error) = last_error
                {
                    return Err(error);
                }
            }
            let mut executions =
                self.parse_history_rows(&primary, &market, opts.order_id.as_deref(), false)?;
            executions.extend(self.parse_history_rows(
                &fallback,
                &market,
                opts.order_id.as_deref(),
                true,
            )?);
            if executions.is_empty() {
                executions = ws()?
                    .into_iter()
                    .map(|execution| {
                        let asset = execution
                            .commission_asset
                            .as_ref()
                            .map_or(J::Undefined, |asset| {
                                J::Text((asset.clone()).encode_utf16().collect())
                            });
                        (execution, asset)
                    })
                    .collect();
            }
            let mut keys = Vec::new();
            let mut deduped = Vec::new();
            for (execution, asset) in executions {
                if from.is_some_and(|from| from != 0. && execution.timestamp * 1000. < from)
                    || to.is_some_and(|to| to != 0. && execution.timestamp * 1000. > to)
                {
                    continue;
                }
                let side = if execution.side == OrderSide::Buy {
                    "buy"
                } else {
                    "sell"
                };
                let key = format!(
                    "{}:{}:{}:{}:{}",
                    execution
                        .order_id
                        .as_deref()
                        .filter(|v| !v.is_empty())
                        .unwrap_or("no-order"),
                    js_number_string(execution.timestamp),
                    js_number_string(execution.price),
                    js_number_string(execution.qty),
                    side,
                );
                if let Some(index) = keys.iter().position(|k| k == &key) {
                    deduped[index] = (execution, asset);
                } else {
                    keys.push(key);
                    deduped.push((execution, asset));
                }
            }
            deduped.sort_by(|a, b| {
                b.0.timestamp
                    .partial_cmp(&a.0.timestamp)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let count = if limit.is_nan() {
                0
            } else {
                limit.trunc() as usize
            };
            deduped.truncate(count);
            // Admit shared String fields only after the complete source selection.
            deduped
                .into_iter()
                .map(|(mut execution, asset)| {
                    execution.commission_asset = if matches!(asset, J::Undefined) {
                        None
                    } else {
                        Some(field_string(&asset)?)
                    };
                    Ok(execution)
                })
                .collect::<Result<Vec<_>, ExchangeError>>()
        }
        .await;
        match result {
            Ok(executions) => Ok(executions),
            Err(error) => {
                let message = message(&error)?;
                let cached = ws()?;
                if !cached.is_empty() {
                    self.base.log(&format!("Execution history fetch failed ({message}), serving {} cached ws fills",cached.len()),"warning")?;
                    return Ok(cached);
                }
                self.base.log(
                    &format!("Execution history fetch failed: {message}"),
                    "warning",
                )?;
                Ok(Vec::new())
            }
        }
    }
}
