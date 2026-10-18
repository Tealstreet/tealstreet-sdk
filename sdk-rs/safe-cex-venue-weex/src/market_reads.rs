//! Market-data and execution reads over imported host operations.
use crate::types::ENDPOINTS;
use safe_cex_num::{math_max, math_min};
use std::{future::Future, pin::Pin};

type ReadFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
type MapCallback<'a, V, E> = Box<dyn FnMut(V, usize, &V) -> Result<V, E> + 'a>;
type FilterCallback<'a, V, E> = Box<dyn FnMut(&V, usize, &V) -> Result<bool, E> + 'a>;
type ReduceCallback<'a, V, E> = Box<dyn FnMut(V, V, usize, &V) -> Result<V, E> + 'a>;
type CompareCallback<'a, V, E> = Box<dyn FnMut(&V, &V) -> Result<f64, E> + 'a>;

pub struct WeexMarketReadRequest<V> {
    pub endpoint: &'static str,
    pub params: Option<V>,
    pub timeout: Option<f64>,
}

/// Collection operations supply JavaScript method, hole, species and coercion semantics.
pub struct WeexMarketReadRuntime<H, V, E> {
    pub request: for<'a> fn(&'a H, WeexMarketReadRequest<V>) -> Result<ReadFuture<'a, V, E>, E>,
    pub all: for<'a> fn(
        &'a H,
        ReadFuture<'a, V, E>,
        ReadFuture<'a, V, E>,
    ) -> Result<ReadFuture<'a, (V, V), E>, E>,
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub is_nullish: fn(&V) -> bool,
    pub equals_text: fn(&V, &str) -> bool,
    pub equals: fn(&V, &V) -> bool,
    pub number: fn(f64) -> V,
    pub text: fn(&str) -> V,
    pub undefined: fn() -> V,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub to_string: fn(&H, &V) -> Result<V, E>,
    pub template_string: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub to_integer_string: fn(&H, &V) -> Result<V, E>,
    pub to_number: fn(&H, &V) -> Result<f64, E>,
    pub less_than: fn(&H, &V, &V) -> Result<bool, E>,
    pub greater_than: fn(&H, &V, &V) -> Result<bool, E>,
    pub map: for<'a> fn(&'a H, &V, MapCallback<'a, V, E>) -> Result<V, E>,
    pub filter: for<'a> fn(&'a H, &V, FilterCallback<'a, V, E>) -> Result<V, E>,
    pub reduce: for<'a> fn(&'a H, &V, V, ReduceCallback<'a, V, E>) -> Result<V, E>,
    pub sort: for<'a> fn(&'a H, V, CompareCallback<'a, V, E>) -> Result<V, E>,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub new_map: fn(&H, V) -> Result<V, E>,
    pub map_getter: fn(&H, &V) -> Result<V, E>,
    pub map_lookup: fn(&H, V, V) -> Result<V, E>,
    pub push_method: fn(&H, &V) -> Result<V, E>,
    pub push: fn(&H, V, V) -> Result<(), E>,
    pub markets: fn(&H) -> Result<V, E>,
    pub tickers: fn(&H) -> Result<V, E>,
    pub markets_by_symbol: fn(&H) -> Result<V, E>,
    pub market_lookup: fn(&H, V, V) -> Result<V, E>,
    pub normalize_market: fn(&H, V) -> Result<V, E>,
    pub normalize_ticker: fn(&H, V, V, V) -> Result<V, E>,
    pub normalize_candle: fn(&H, V) -> Result<V, E>,
    pub normalize_order_book: fn(&H, V) -> Result<V, E>,
    pub normalize_public_trade: fn(&H, V, &V) -> Result<V, E>,
    pub normalize_execution: fn(&H, V, V) -> Result<V, E>,
    pub normalize_timestamp_ms: fn(&H, V) -> Result<f64, E>,
    pub interval: fn(&H, V) -> Result<V, E>,
    pub supported_interval: fn(&V) -> bool,
    pub copy_properties: fn(&H, V) -> Result<V, E>,
    pub define_property: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub extract_array: fn(&H, V) -> Result<V, E>,
    pub omit_undefined: fn(&H, V) -> Result<V, E>,
    pub public_only: fn(&H) -> Result<bool, E>,
    pub provider: fn(&H) -> Result<V, E>,
    pub public_websocket: fn(&H) -> Result<V, E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call: fn(&H, V, V, V, V) -> Result<V, E>,
    pub error_value: fn(&E) -> V,
    pub error_sink: fn(&H) -> Result<V, E>,
    pub emit_error: fn(&H, V, V) -> Result<(), E>,
    pub new_error: fn(&H, Vec<u16>) -> E,
}
fn ready<'a, V: 'a, E: 'a>(value: Result<V, E>) -> ReadFuture<'a, V, E> {
    Box::pin(std::future::ready(value))
}
fn endpoint(name: &str) -> &'static str {
    ENDPOINTS.iter().find(|(key, _)| *key == name).unwrap().1
}
fn get_request<V>(name: &str, params: Option<V>, timeout: Option<f64>) -> WeexMarketReadRequest<V> {
    WeexMarketReadRequest {
        endpoint: endpoint(name),
        params,
        timeout,
    }
}
fn fallback<H, V, E>(
    host: &H,
    rt: &WeexMarketReadRuntime<H, V, E>,
    error: &E,
    markets: bool,
) -> Result<V, E> {
    let sink = (rt.error_sink)(host)?;
    let error = (rt.error_value)(error);
    let response = (rt.optional_get)(host, &error, "response")?;
    let data = (rt.optional_get)(host, &response, "data")?;
    let message = (rt.optional_get)(host, &data, "msg")?;
    let message = if (rt.truthy)(&message) {
        message
    } else {
        (rt.optional_get)(host, &error, "message")?
    };
    (rt.emit_error)(host, sink, message)?;
    if markets {
        (rt.markets)(host)
    } else {
        (rt.tickers)(host)
    }
}
fn filtered_array<H, V, E>(host: &H, rt: &WeexMarketReadRuntime<H, V, E>, data: V) -> Result<V, E> {
    if (rt.is_array)(host, &data)? {
        Ok(data)
    } else {
        Ok((rt.array)(Vec::new()))
    }
}
fn sort_times<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    values: V,
    descending: bool,
) -> Result<V, E> {
    (rt.sort)(
        host,
        values,
        Box::new(move |a, b| {
            let (first, second) = if descending { (b, a) } else { (a, b) };
            let left = (rt.get)(host, first, "timestamp")?;
            let right = (rt.get)(host, second, "timestamp")?;
            let left = (rt.to_number)(host, &left)?;
            let right = (rt.to_number)(host, &right)?;
            Ok(left - right)
        }),
    )
}

pub fn fetch_markets<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
) -> ReadFuture<'a, V, E> {
    let request = match (rt.request)(host, get_request("EXCHANGE_INFO", None, None)) {
        Ok(v) => v,
        Err(e) => return ready(fallback(host, rt, &e, true)),
    };
    Box::pin(async move {
        let result = (async {
            let response = request.await?;
            let data = (rt.get)(host, &response, "data")?;
            let symbols = (rt.get)(host, &data, "symbols")?;
            let symbols = if (rt.truthy)(&symbols) {
                symbols
            } else {
                (rt.array)(Vec::new())
            };
            let filtered = (rt.filter)(
                host,
                &symbols,
                Box::new(move |row, _, _| {
                    let quote = (rt.get)(host, row, "quoteAsset")?;
                    if !(rt.equals_text)(&quote, "USDT") {
                        return Ok(false);
                    }
                    Ok((rt.equals_text)(
                        &(rt.get)(host, row, "marginAsset")?,
                        "USDT",
                    ))
                }),
            )?;
            let mapped = (rt.map)(
                host,
                &filtered,
                Box::new(move |row, _, _| (rt.normalize_market)(host, row)),
            )?;
            (rt.filter)(
                host,
                &mapped,
                Box::new(move |row, _, _| Ok(!(rt.is_undefined)(row))),
            )
        })
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => fallback(host, rt, &e, true),
        }
    })
}

pub fn fetch_tickers<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
) -> ReadFuture<'a, V, E> {
    let aggregate = (|| {
        let ticker = (rt.request)(host, get_request("TICKER_24HR", None, None))?;
        let book = (rt.request)(host, get_request("BOOK_TICKER", None, None))?;
        (rt.all)(host, ticker, book)
    })();
    let aggregate = match aggregate {
        Ok(v) => v,
        Err(e) => return ready(fallback(host, rt, &e, false)),
    };
    Box::pin(async move {
        let result = (async {
            let (ticker_response, book_response) = aggregate.await?;
            let map_rows = |rows: &V| {
                (rt.map)(
                    host,
                    rows,
                    Box::new(move |row, _, _| {
                        let symbol = (rt.get)(host, &row, "symbol")?;
                        let key = (rt.to_string)(host, &symbol)?;
                        Ok((rt.array)(vec![key, row]))
                    }),
                )
            };
            let tickers = (rt.get)(host, &ticker_response, "data")?;
            let tickers = (rt.new_map)(host, map_rows(&tickers)?)?;
            let books = (rt.get)(host, &book_response, "data")?;
            let books = (rt.new_map)(host, map_rows(&books)?)?;
            let markets = (rt.markets)(host)?;
            (rt.reduce)(
                host,
                &markets,
                (rt.array)(Vec::new()),
                Box::new(move |acc, market, _, _| {
                    let get = (rt.map_getter)(host, &tickers)?;
                    let ticker = (rt.map_lookup)(host, get, (rt.get)(host, &market, "symbol")?)?;
                    let get = (rt.map_getter)(host, &books)?;
                    let book = (rt.map_lookup)(host, get, (rt.get)(host, &market, "symbol")?)?;
                    if !(rt.truthy)(&ticker) && !(rt.truthy)(&book) {
                        return Ok(acc);
                    }
                    let push = (rt.push_method)(host, &acc)?;
                    let next = (rt.normalize_ticker)(host, ticker, market, book)?;
                    (rt.push)(host, push, next)?;
                    Ok(acc)
                }),
            )
        })
        .await;
        match result {
            Ok(v) => Ok(v),
            Err(e) => fallback(host, rt, &e, false),
        }
    })
}

pub fn fetch_ohlcv<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    opts: V,
) -> ReadFuture<'a, V, E> {
    let prepared = (|| {
        let interval = (rt.interval)(host, (rt.get)(host, &opts, "interval")?)?;
        if !(rt.supported_interval)(&interval) {
            let mut message: Vec<u16> = "WEEX does not support ".encode_utf16().collect();
            let value = (rt.template_string)(host, &(rt.get)(host, &opts, "interval")?)?;
            message.extend(value);
            message.extend(" candles".encode_utf16());
            return Err((rt.new_error)(host, message));
        }
        let from = (rt.get)(host, &opts, "from")?;
        let historical =
            !(rt.is_undefined)(&from) || !(rt.is_undefined)(&(rt.get)(host, &opts, "to")?);
        let limit = (rt.get)(host, &opts, "limit")?;
        let limit = if (rt.truthy)(&limit) {
            limit
        } else {
            (rt.number)(if historical { 100.0 } else { 500.0 })
        };
        let limit = math_min(
            (rt.to_number)(host, &limit)?,
            if historical { 100.0 } else { 1000.0 },
        );
        let params = (rt.object)(vec![
            ("symbol", (rt.get)(host, &opts, "symbol")?),
            ("interval", interval),
            ("startTime", (rt.get)(host, &opts, "from")?),
            ("endTime", (rt.get)(host, &opts, "to")?),
            ("limit", (rt.number)(limit)),
            (
                "priceType",
                if historical {
                    (rt.text)("LAST")
                } else {
                    (rt.undefined)()
                },
            ),
        ]);
        (rt.request)(
            host,
            get_request(
                if historical {
                    "HISTORY_KLINES"
                } else {
                    "KLINES"
                },
                Some(params),
                Some(300_000.0),
            ),
        )
    })();
    let request = match prepared {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let response = request.await?;
        let data = filtered_array(host, rt, (rt.get)(host, &response, "data")?)?;
        let mapped = (rt.map)(
            host,
            &data,
            Box::new(move |row, _, _| (rt.normalize_candle)(host, row)),
        )?;
        let filtered = (rt.filter)(host, &mapped, Box::new(move |v, _, _| Ok((rt.truthy)(v))))?;
        sort_times(host, rt, filtered, false)
    })
}

pub fn listen_ohlcv<H, V, E>(
    host: &H,
    rt: &WeexMarketReadRuntime<H, V, E>,
    opts: V,
    callback: V,
) -> Result<V, E> {
    let first = (rt.provider)(host)?;
    let (receiver, name) = if (rt.truthy)(&first) {
        ((rt.provider)(host)?, "subscribeCandle")
    } else {
        ((rt.public_websocket)(host)?, "listenOHLCV")
    };
    let method = (rt.method)(host, &receiver, name)?;
    (rt.call)(host, method, receiver, opts, callback)
}

pub fn listen_order_book<H, V, E>(
    host: &H,
    rt: &WeexMarketReadRuntime<H, V, E>,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let first = (rt.provider)(host)?;
    let (receiver, name) = if (rt.truthy)(&first) {
        ((rt.provider)(host)?, "subscribeOrderbook")
    } else {
        ((rt.public_websocket)(host)?, "listenOrderBook")
    };
    let method = (rt.method)(host, &receiver, name)?;
    (rt.call)(host, method, receiver, symbol, callback)
}

pub fn fetch_order_book<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    symbol: V,
    limit: V,
) -> ReadFuture<'a, V, E> {
    let limit = if (rt.is_undefined)(&limit) {
        (rt.number)(200.0)
    } else {
        limit
    };
    let params = (rt.object)(vec![("symbol", symbol), ("limit", limit)]);
    let request = match (rt.request)(host, get_request("DEPTH", Some(params), None)) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let response = request.await?;
        let data = (rt.get)(host, &response, "data")?;
        let normalized = (rt.normalize_order_book)(host, data.clone())?;
        let result = (rt.copy_properties)(host, normalized)?;
        let id = (rt.optional_get)(host, &data, "lastUpdateId")?;
        let id = (rt.to_integer_string)(host, &id)?;
        (rt.define_property)(host, &result, "lastUpdateId", id)?;
        Ok(result)
    })
}

pub fn fetch_public_trades<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    symbol: V,
    limit: V,
) -> ReadFuture<'a, V, E> {
    let prepared = (|| {
        let limit = if (rt.is_undefined)(&limit) {
            (rt.number)(100.0)
        } else {
            limit
        };
        let limit = math_min(math_max((rt.to_number)(host, &limit)?, 1.0), 1000.0);
        let params = (rt.object)(vec![
            ("symbol", symbol.clone()),
            ("limit", (rt.number)(limit)),
        ]);
        (rt.request)(host, get_request("PUBLIC_TRADES", Some(params), None))
    })();
    let request = match prepared {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let response = request.await?;
        let data = filtered_array(host, rt, (rt.get)(host, &response, "data")?)?;
        let mapped = (rt.map)(
            host,
            &data,
            Box::new(move |row, _, _| (rt.normalize_public_trade)(host, row, &symbol)),
        )?;
        (rt.filter)(host, &mapped, Box::new(move |v, _, _| Ok((rt.truthy)(v))))
    })
}

pub fn fetch_user_trade_rows<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    opts: V,
) -> ReadFuture<'a, V, E> {
    let prepared = (|| {
        let limit = (rt.get)(host, &opts, "limit")?;
        let limit = if (rt.truthy)(&limit) {
            limit
        } else {
            (rt.number)(100.0)
        };
        let limit = math_min(math_max((rt.to_number)(host, &limit)?, 1.0), 100.0);
        let params = (rt.object)(vec![
            ("symbol", (rt.get)(host, &opts, "symbol")?),
            ("orderId", (rt.get)(host, &opts, "orderId")?),
            ("startTime", (rt.get)(host, &opts, "from")?),
            ("endTime", (rt.get)(host, &opts, "to")?),
            ("limit", (rt.number)(limit)),
        ]);
        let params = (rt.omit_undefined)(host, params)?;
        (rt.request)(host, get_request("USER_TRADES", Some(params), None))
    })();
    let request = match prepared {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let response = request.await?;
        let data = (rt.get)(host, &response, "data")?;
        let rows = (rt.extract_array)(host, data)?;
        (rt.filter)(
            host,
            &rows,
            Box::new(move |row, _, _| {
                let order_id = (rt.get)(host, &opts, "orderId")?;
                if (rt.truthy)(&order_id) {
                    let actual = (rt.to_integer_string)(host, &(rt.get)(host, row, "orderId")?)?;
                    let expected =
                        (rt.to_integer_string)(host, &(rt.get)(host, &opts, "orderId")?)?;
                    if !(rt.equals)(&actual, &expected) {
                        return Ok(false);
                    }
                }
                let time = (rt.get)(host, row, "time")?;
                let time = if (rt.is_nullish)(&time) {
                    (rt.get)(host, row, "createdTime")?
                } else {
                    time
                };
                let timestamp = (rt.number)((rt.normalize_timestamp_ms)(host, time)?);
                let from = (rt.get)(host, &opts, "from")?;
                if (rt.truthy)(&from)
                    && (rt.less_than)(host, &timestamp, &(rt.get)(host, &opts, "from")?)?
                {
                    return Ok(false);
                }
                let to = (rt.get)(host, &opts, "to")?;
                if (rt.truthy)(&to)
                    && (rt.greater_than)(host, &timestamp, &(rt.get)(host, &opts, "to")?)?
                {
                    return Ok(false);
                }
                Ok(true)
            }),
        )
    })
}

pub fn fetch_historical_fills<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexMarketReadRuntime<H, V, E>,
    opts: V,
) -> ReadFuture<'a, V, E> {
    match (rt.public_only)(host) {
        Ok(true) => return ready(Ok((rt.array)(Vec::new()))),
        Err(e) => return ready(Err(e)),
        Ok(false) => {}
    }
    let request = fetch_user_trade_rows(host, rt, opts);
    Box::pin(async move {
        let rows = request.await?;
        let mapped = (rt.map)(
            host,
            &rows,
            Box::new(move |row, _, _| {
                let symbol = (rt.get)(host, &row, "symbol")?;
                let market = if (rt.truthy)(&symbol) {
                    let markets = (rt.markets_by_symbol)(host)?;
                    let symbol = (rt.get)(host, &row, "symbol")?;
                    (rt.market_lookup)(host, markets, symbol)?
                } else {
                    (rt.undefined)()
                };
                (rt.normalize_execution)(host, row, market)
            }),
        )?;
        let filtered = (rt.filter)(host, &mapped, Box::new(move |v, _, _| Ok((rt.truthy)(v))))?;
        sort_times(host, rt, filtered, true)
    })
}
