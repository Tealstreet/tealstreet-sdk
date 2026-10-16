//! Spot discovery reads through the shared transport boundary.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_transport::{RestAdapter, RestRequest};
use serde_json::Value;

// ts: KucoinSpotExchange.fetchActiveOrderSymbols
pub async fn fetch_active_order_symbols(xhr: &dyn RestAdapter) -> Vec<Value> {
    // No shared state or latch is acquired before the await; dropping this
    // future drops its transport wait and a later caller can retry normally.
    let Ok(response) = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[7].1.into(),
            ..Default::default()
        })
        .await
    else {
        return Vec::new();
    };
    // Value-first decoding retains overflow lexemes. The source returns the
    // array as-is: do not silently filter nulls or coerce entries to strings.
    let Ok(data) = response.body.json_value() else {
        return Vec::new();
    };
    if data.get("code").and_then(Value::as_str) != Some("200000") {
        return Vec::new();
    }
    data.get("data")
        .and_then(|v| v.get("symbols"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

// ts: KucoinSpotExchange.fetchMarkets
pub async fn fetch_markets(
    base: &safe_cex_exchange::exchange::BaseExchange,
    xhr: &dyn RestAdapter,
) -> Result<Vec<safe_cex_types::Market>, ExchangeError> {
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[3].1.into(),
            ..Default::default()
        })
        .await;
    let mut inside_try = false;
    let decoded = match response {
        Ok(response) => response
            .body
            .json_value()
            .map_err(|e| format!("KuCoin Spot market JSON/UTF-8 representation refusal: {e}"))
            .and_then(|data| {
                inside_try =
                    !data.is_null() && data.get("code").and_then(Value::as_str) != Some("200000");
                decode_markets(&data)
            }),
        Err(error) => {
            // A valid JSON escape containing a lone surrogate cannot cross
            // this representation boundary. Report the refusal, never silently
            // replace the venue's rejection text with the transport message.
            let message = error
                .response
                .as_ref()
                .map(|r| r.body.json_value())
                .transpose()
                .map_err(|e| format!("KuCoin Spot market JSON/UTF-8 representation refusal: {e}"))
                .and_then(|data| market_error_message(data.as_ref().and_then(|d| d.get("msg"))));
            Err(match message {
                Ok(Some(message)) => message,
                Ok(None) => error.message,
                Err(message) => message,
            })
        }
    };
    match decoded {
        Ok(markets) => Ok(markets),
        Err(message) => {
            emit_read_error(base, message, inside_try, market_error_message)?;
            // Read after the await: a stream can have replaced the store while
            // this request was in flight. Never capture the fallback beforehand.
            Ok(base.store.markets().to_vec())
        }
    }
}
fn market_error_message(value: Option<&Value>) -> Result<Option<String>, String> {
    match value.filter(|v| safe_cex_num::js_truthy(Some(v))) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err("KuCoin Spot market error requires a string representation".into()),
    }
}
fn decode_markets(data: &Value) -> Result<Vec<safe_cex_types::Market>, String> {
    use safe_cex_types::{
        Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    };
    if data.is_null() {
        return Err("Cannot read properties of null (reading 'code')".into());
    }
    if data.get("code").and_then(Value::as_str) != Some("200000") {
        return Err(market_error_message(data.get("msg"))?
            .unwrap_or_else(|| "Failed to fetch KuCoin Spot markets".into()));
    }
    let rows = match data.get("data") {
        None => return Err("Cannot read properties of undefined (reading 'map')".into()),
        Some(Value::Null) => return Err("Cannot read properties of null (reading 'map')".into()),
        Some(value) => value.as_array().ok_or("data.data.map is not a function")?,
    };
    let mut output = Vec::with_capacity(rows.len());
    let mut output_refusal = None;
    for row in rows {
        if row.is_null() {
            return Err("Cannot read properties of null (reading 'symbol')".into());
        }
        // The source interpolates these values before its numeric fields. A
        // typed String refusal must not hide an interpolation/coercion error.
        let base_text =
            safe_cex_num::js_string(row.get("baseCurrency")).map_err(|e| e.message.to_owned())?;
        let quote_text =
            safe_cex_num::js_string(row.get("quoteCurrency")).map_err(|e| e.message.to_owned())?;
        let number = |field: &str, fallback: &str| -> Result<f64, String> {
            let text = match row.get(field).filter(|v| safe_cex_num::js_truthy(Some(v))) {
                None => fallback.to_owned(),
                value => safe_cex_num::js_string(value).map_err(|e| e.message.to_owned())?,
            };
            Ok(safe_cex_num::parse_float(&text))
        };
        let amount = number("baseIncrement", "1")?;
        let price = number("priceIncrement", "1")?;
        let min = number("baseMinSize", "0")?;
        let max = number("baseMaxSize", "9007199254740991")?;
        let string = |field: &str| {
            row.get(field)
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| {
                    format!("KuCoin Spot market {field} requires a string representation")
                })
        };
        let strings = (|| {
            Ok::<_, String>((
                string("symbol")?,
                string("baseCurrency")?,
                string("quoteCurrency")?,
            ))
        })();
        let (symbol, base, quote) = match strings {
            Ok(strings) => strings,
            Err(message) => {
                // Source maps every remaining row before returning the array;
                // a later null/coercion error wins over this output limitation.
                output_refusal.get_or_insert(message);
                continue;
            }
        };
        output.push(Market {
            id: symbol.clone(),
            symbol,
            pretty_symbol: Some(format!("{base_text}{quote_text}")),
            base,
            quote,
            active: row.get("enableTrading") == Some(&Value::Bool(true)),
            precision: MarketPrecision { amount, price },
            limits: MarketLimits {
                amount: MarketLimitsAmount { min, max },
                leverage: MarketLimitsLeverage { min: 1.0, max: 1.0 },
            },
            contract_size: Some(1.0),
            is_spot: Some(true),
            is_perpetual: None,
            is_inverse: Some(false),
            info: Some(row.clone()),
            is_expiring: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        });
    }
    match output_refusal {
        Some(message) => Err(message),
        None => Ok(output),
    }
}

// ts: KucoinSpotExchange.fetchTickers
pub async fn fetch_tickers(
    base: &safe_cex_exchange::exchange::BaseExchange,
    xhr: &dyn RestAdapter,
) -> Result<Vec<safe_cex_types::Ticker>, ExchangeError> {
    let response = xhr
        .execute(RestRequest {
            method: "GET".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[4].1.into(),
            ..Default::default()
        })
        .await;
    let decode = |body: &safe_cex_transport::RestResponseBody| {
        body.json_value()
            .map_err(|error| format!("KuCoin Spot ticker response representation refusal: {error}"))
    };
    let mut inside_try = false;
    let result = match response {
        Ok(response) => decode(&response.body).and_then(|data| {
            inside_try =
                !data.is_null() && data.get("code").and_then(Value::as_str) != Some("200000");
            decode_tickers(base, &data)
        }),
        Err(error) => {
            let message = error
                .response
                .as_ref()
                .map(|r| decode(&r.body))
                .transpose()
                .and_then(|body| ticker_error_message(body.as_ref().and_then(|v| v.get("msg"))));
            Err(match message {
                Ok(Some(message)) => message,
                Ok(None) => error.message,
                Err(message) => message,
            })
        }
    };
    match result {
        Ok(tickers) => Ok(tickers),
        Err(message) => {
            emit_read_error(base, message, inside_try, ticker_error_message)?;
            Ok(base.store.tickers().to_vec())
        }
    }
}
fn ticker_error_message(value: Option<&Value>) -> Result<Option<String>, String> {
    match value.filter(|v| safe_cex_num::js_truthy(Some(v))) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err("KuCoin Spot ticker error requires a string representation".into()),
    }
}
fn decode_tickers(
    base: &safe_cex_exchange::exchange::BaseExchange,
    data: &Value,
) -> Result<Vec<safe_cex_types::Ticker>, String> {
    use safe_cex_num::{TableHit, js_string, js_table_lookup, js_truthy, parse_float};
    if data.is_null() {
        return Err("Cannot read properties of null (reading 'code')".into());
    }
    if data.get("code").and_then(Value::as_str) != Some("200000") {
        return Err(ticker_error_message(data.get("msg"))?
            .unwrap_or_else(|| "Failed to fetch KuCoin Spot tickers".into()));
    }
    let rows = data.get("data").and_then(|v| v.get("ticker"));
    let Some(rows) = rows.filter(|v| js_truthy(Some(v))) else {
        return Ok(Vec::new());
    };
    let rows = rows
        .as_array()
        .ok_or("KuCoin Spot ticker rows require an array representation")?;
    let mut output = Vec::with_capacity(rows.len());
    let mut output_refused = false;
    for row in rows {
        if row.is_null() {
            return Err("Cannot read properties of null (reading 'symbol')".into());
        }
        let symbol = js_string(row.get("symbol")).map_err(|e| e.message.to_owned())?;
        let markets = base.store.markets_by_symbol();
        let own = markets.get(&symbol).map(|market| (symbol.as_str(), market));
        let hit = js_table_lookup(own.as_slice(), &symbol);
        // Inherited markets pass the source guard; they must not silently become
        // missing markets. Their undefined id/symbol cannot cross typed Ticker.
        if !hit.is_truthy_by(|_| true) {
            continue;
        }
        let number = |key: &str| -> Result<f64, String> {
            let value = row.get(key).filter(|v| js_truthy(Some(v)));
            let text = match value {
                None => "0".into(),
                Some(value) => js_string(Some(value)).map_err(|e| e.message.to_owned())?,
            };
            Ok(parse_float(&text))
        };
        let last = number("last")?;
        let bid = number("buy")?;
        let ask = number("sell")?;
        let percentage = number("changeRate")? * 100.0;
        let volume = number("vol")?;
        let quote_volume = number("volValue")?;
        // Source evaluates every numeric property before publishing the ticker.
        // An inherited output refusal must not mask an earlier coercion error.
        let market = match hit {
            TableHit::Mapped(market) => *market,
            TableHit::Inherited(_) => {
                // Source keeps mapping subsequent rows before it publishes the
                // batch. Preserve later-row errors ahead of this output limit.
                output_refused = true;
                continue;
            }
            TableHit::Missing => continue,
        };
        output.push(safe_cex_types::Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid,
            ask,
            last,
            mark: last,
            index: last,
            mid_price: None,
            timestamp: None,
            percentage,
            funding_rate: 0.0,
            funding_time: 0.0,
            open_interest: 0.0,
            volume,
            quote_volume,
            info: Some(row.clone()),
        });
    }
    if output_refused {
        Err("KuCoin Spot inherited market has undefined typed ticker id/symbol".into())
    } else {
        Ok(output)
    }
}

fn emit_read_error(
    base: &safe_cex_exchange::exchange::BaseExchange,
    message: String,
    inside_try: bool,
    error_message: fn(Option<&Value>) -> Result<Option<String>, String>,
) -> Result<(), ExchangeError> {
    use safe_cex_exchange::error_reporting::ExchangeErrorInput;
    match base.emit_and_log_error(ExchangeErrorInput::Message(message), None, None) {
        Err(error) if inside_try => {
            let caught = error_message(
                error
                    .response
                    .as_ref()
                    .and_then(|r| r.get("data"))
                    .and_then(|d| d.get("msg")),
            )
            .unwrap_or_else(Some)
            .or(Some(error.message));
            base.emit_and_log_error(
                caught.map_or(ExchangeErrorInput::Missing, ExchangeErrorInput::Message),
                None,
                None,
            )
        }
        result => result,
    }
}
