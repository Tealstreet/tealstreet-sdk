//! Account REST reads with imported transport, normalization and JavaScript primitives.
use crate::{symbol_settings::WeexDefaultSymbolConfig, types::ENDPOINTS};
use safe_cex_types::{MarginType, PositionMode};
use std::{future::Future, pin::Pin};

type ReadFuture<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
type SettledFuture<'a, V, E> = Pin<Box<dyn Future<Output = (Result<V, E>, Result<V, E>)> + 'a>>;

pub struct WeexAccountReadRequest<V> {
    pub endpoint: Vec<u16>,
    pub params: Option<Vec<(&'static str, V)>>,
}

/// Factories eagerly start host-owned work; aggregate primitives retain peer requests.
pub struct WeexAccountReadRuntime<H, V, E> {
    pub request: for<'a> fn(&'a H, WeexAccountReadRequest<V>) -> Result<ReadFuture<'a, V, E>, E>,
    pub config: for<'a> fn(&'a H, V, bool) -> Result<ReadFuture<'a, V, E>, E>,
    pub balance: for<'a> fn(&'a H) -> Result<ReadFuture<'a, V, E>, E>,
    pub positions: for<'a> fn(&'a H) -> Result<ReadFuture<'a, V, E>, E>,
    pub all: for<'a> fn(
        &'a H,
        ReadFuture<'a, V, E>,
        ReadFuture<'a, V, E>,
    ) -> Result<ReadFuture<'a, (V, V), E>, E>,
    pub settled: for<'a> fn(
        &'a H,
        ReadFuture<'a, V, E>,
        ReadFuture<'a, V, E>,
    ) -> Result<SettledFuture<'a, V, E>, E>,
    pub public_only: fn(&H) -> Result<bool, E>,
    pub markets_by_symbol: fn(&H) -> Result<V, E>,
    pub market: fn(&H, &V, &V) -> Result<V, E>,
    pub get: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_get: fn(&H, &V, &str) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub equals_text: fn(&V, &str) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub to_string: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub number: fn(f64) -> V,
    pub undefined: fn() -> V,
    pub null: fn() -> V,
    pub to_number: fn(&H, &V, f64) -> Result<f64, E>,
    pub array: fn(Vec<V>) -> V,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub length: fn(&H, &V) -> Result<usize, E>,
    pub present: fn(&H, &V, usize) -> Result<bool, E>,
    pub at: fn(&H, &V, usize) -> Result<V, E>,
    pub iterator: fn(&H, &V) -> Result<V, E>,
    pub next: fn(&H, &V) -> Result<Option<V>, E>,
    pub close_on_error: fn(&H, &V, &E),
    pub extract_array: fn(&H, &V, &[&str]) -> Result<V, E>,
    pub normalize_balance: fn(&H, V) -> Result<V, E>,
    pub normalize_order: fn(&H, V, V) -> Result<V, E>,
    pub normalize_algo_order: fn(&H, V, V) -> Result<V, E>,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub uniq_by_id: fn(&H, V) -> Result<V, E>,
    pub store_updater: fn(&H) -> Result<V, E>,
    pub fetched_leverage: fn(&H) -> Result<V, E>,
    pub copy_properties: fn(&H, &V) -> Result<V, E>,
    pub set_property: fn(&H, &V, &V, V) -> Result<(), E>,
    pub store_update: fn(&H, V, V) -> Result<(), E>,
    pub cache: fn(&H) -> Result<V, E>,
    pub config_setter: fn(&H, &V) -> Result<V, E>,
    pub set_config: fn(&H, V, V, WeexDefaultSymbolConfig) -> Result<(), E>,
    pub push_method: fn(&H, &V) -> Result<V, E>,
    pub push_spread: fn(&H, V, &V) -> Result<(), E>,
    pub error_message: fn(&H, &E) -> Result<Vec<u16>, E>,
    pub warning: fn(&H, Vec<u16>) -> Result<(), E>,
}
fn ready<'a, V: 'a, E: 'a>(value: Result<V, E>) -> ReadFuture<'a, V, E> {
    Box::pin(std::future::ready(value))
}
fn endpoint(name: &str) -> Vec<u16> {
    ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
        .encode_utf16()
        .collect()
}
fn spread<H, V, E>(
    host: &H,
    rt: &WeexAccountReadRuntime<H, V, E>,
    value: &V,
    out: &mut Vec<V>,
) -> Result<(), E> {
    let iterator = (rt.iterator)(host, value)?;
    while let Some(item) = (rt.next)(host, &iterator)? {
        out.push(item);
    }
    Ok(())
}
fn warn<H, V, E>(
    host: &H,
    rt: &WeexAccountReadRuntime<H, V, E>,
    mut prefix: Vec<u16>,
    error: &E,
) -> Result<(), E> {
    prefix.extend((rt.error_message)(host, error)?);
    (rt.warning)(host, prefix)
}

fn leverage_failure<H, V, E>(
    host: &H,
    rt: &WeexAccountReadRuntime<H, V, E>,
    symbol: &V,
    error: &E,
) -> Result<V, E> {
    let mut prefix: Vec<u16> = "Error fetching WEEX leverage for ".encode_utf16().collect();
    prefix.extend((rt.to_string)(host, symbol)?);
    prefix.extend(": ".encode_utf16());
    warn(host, rt, prefix, error)?;
    Ok((rt.null)())
}
fn config_failure<H, V, E>(
    host: &H,
    rt: &WeexAccountReadRuntime<H, V, E>,
    error: &E,
) -> Result<V, E> {
    warn(
        host,
        rt,
        "Failed to fetch WEEX symbol config: "
            .encode_utf16()
            .collect(),
        error,
    )?;
    (rt.cache)(host)
}

pub fn fetch_symbol_max_leverage<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbol: V,
) -> ReadFuture<'a, V, E> {
    let market = match (rt.markets_by_symbol)(host)
        .and_then(|markets| (rt.market)(host, &markets, &symbol))
    {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    if (rt.truthy)(&market) {
        return ready((|| {
            let limits = (rt.get)(host, &market, "limits")?;
            let leverage = (rt.get)(host, &limits, "leverage")?;
            (rt.get)(host, &leverage, "max")
        })());
    }
    let request = match (rt.request)(
        host,
        WeexAccountReadRequest {
            endpoint: endpoint("EXCHANGE_INFO"),
            params: Some(vec![("symbol", symbol)]),
        },
    ) {
        Ok(request) => request,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        let data = request.await?;
        let symbols = (rt.get)(host, &data, "symbols")?;
        let raw = (rt.optional_get)(host, &symbols, "0")?;
        let max = (rt.optional_get)(host, &raw, "maxLeverage")?;
        Ok((rt.number)((rt.to_number)(host, &max, 1.0)?))
    })
}

pub fn fetch_leverage_for_symbol<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbol: V,
) -> ReadFuture<'a, V, E> {
    let request = match (rt.config)(host, symbol.clone(), true) {
        Ok(request) => request,
        Err(error) => return ready(leverage_failure(host, rt, &symbol, &error)),
    };
    Box::pin(async move {
        let result = (async {
            let config = request.await?;
            let buy_margin = (rt.get)(host, &config, "marginType")?;
            let buy = (rt.get)(
                host,
                &config,
                if (rt.equals_text)(&buy_margin, "isolated") {
                    "isolatedLongLeverage"
                } else {
                    "crossLeverage"
                },
            )?;
            let sell_margin = (rt.get)(host, &config, "marginType")?;
            let sell = (rt.get)(
                host,
                &config,
                if (rt.equals_text)(&sell_margin, "isolated") {
                    "isolatedShortLeverage"
                } else {
                    "crossLeverage"
                },
            )?;
            let value = (rt.object)(vec![("buyLeverage", buy), ("sellLeverage", sell)]);
            let updater = (rt.store_updater)(host)?;
            let existing = (rt.fetched_leverage)(host)?;
            let fetched = (rt.copy_properties)(host, &existing)?;
            (rt.set_property)(host, &fetched, &symbol, value.clone())?;
            (rt.store_update)(
                host,
                updater,
                (rt.object)(vec![("fetchedLeverage", fetched)]),
            )?;
            Ok(value)
        })
        .await;
        match result {
            Ok(value) => Ok(value),
            Err(error) => leverage_failure(host, rt, &symbol, &error),
        }
    })
}

pub fn fetch_balance<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
) -> ReadFuture<'a, V, E> {
    let request = match (rt.request)(
        host,
        WeexAccountReadRequest {
            endpoint: endpoint("BALANCE"),
            params: None,
        },
    ) {
        Ok(request) => request,
        Err(error) => return ready(Err(error)),
    };
    Box::pin(async move {
        let data = request.await?;
        let rows = (rt.extract_array)(host, &data, &[])?;
        (rt.normalize_balance)(host, rows)
    })
}

pub fn fetch_balance_and_positions<'a, H, V: 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
) -> ReadFuture<'a, V, E> {
    let balance = match (rt.balance)(host) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    let positions = match (rt.positions)(host) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    let aggregate = match (rt.all)(host, balance, positions) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let (balance, positions) = aggregate.await?;
        Ok((rt.object)(vec![
            ("balance", balance),
            ("positions", positions),
        ]))
    })
}

pub fn fetch_symbol_configs<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbol: V,
) -> ReadFuture<'a, V, E> {
    match (rt.public_only)(host) {
        Ok(true) => return ready((rt.cache)(host)),
        Err(e) => return ready(Err(e)),
        Ok(false) => {}
    }
    let params = if (rt.truthy)(&symbol) {
        Some(vec![("symbol", symbol)])
    } else {
        None
    };
    let request = match (rt.request)(
        host,
        WeexAccountReadRequest {
            endpoint: endpoint("SYMBOL_CONFIG"),
            params,
        },
    ) {
        Ok(request) => request,
        Err(error) => return ready(config_failure(host, rt, &error)),
    };
    Box::pin(async move {
        let result = (async {
            let data = request.await?;
            let rows = (rt.extract_array)(host, &data, &[])?;
            let iterator = (rt.iterator)(host, &rows)?;
            while let Some(row) = (rt.next)(host, &iterator)? {
                let entry = (|| {
                    let margin = (rt.get)(host, &row, "marginType")?;
                    let mode = (rt.get)(host, &row, "separatedType")?;
                    let cross = (rt.get)(host, &row, "crossLeverage")?;
                    let cross_leverage = (rt.to_number)(host, &cross, 20.0)?;
                    let long = (rt.get)(host, &row, "isolatedLongLeverage")?;
                    let isolated_long_leverage = (rt.to_number)(host, &long, 20.0)?;
                    let short = (rt.get)(host, &row, "isolatedShortLeverage")?;
                    let isolated_short_leverage = (rt.to_number)(host, &short, 20.0)?;
                    let config = WeexDefaultSymbolConfig {
                        margin_type: if (rt.equals_text)(&margin, "ISOLATED") {
                            MarginType::Isolated
                        } else {
                            MarginType::Cross
                        },
                        position_mode: if (rt.equals_text)(&mode, "SEPARATED") {
                            PositionMode::Hedged
                        } else {
                            PositionMode::OneWay
                        },
                        cross_leverage,
                        isolated_long_leverage,
                        isolated_short_leverage,
                    };
                    {
                        let cache = (rt.cache)(host)?;
                        let setter = (rt.config_setter)(host, &cache)?;
                        let symbol = (rt.get)(host, &row, "symbol")?;
                        (rt.set_config)(host, setter, symbol, config)
                    }
                })();
                if let Err(error) = entry {
                    (rt.close_on_error)(host, &iterator, &error);
                    return Err(error);
                }
            }
            Ok(())
        })
        .await;
        if let Err(error) = result {
            warn(
                host,
                rt,
                "Failed to fetch WEEX symbol config: "
                    .encode_utf16()
                    .collect(),
                &error,
            )?;
        }
        (rt.cache)(host)
    })
}

pub fn fetch_open_order_rows<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    target: Vec<u16>,
    symbols: V,
) -> ReadFuture<'a, V, E> {
    let iterator = (|| {
        let requested = if (rt.truthy)(&symbols) && (rt.length)(host, &symbols)? > 0 {
            symbols
        } else {
            (rt.array)(vec![(rt.undefined)()])
        };
        (rt.iterator)(host, &requested)
    })();
    let iterator = match iterator {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    let first = match (rt.next)(host, &iterator) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    let make_request = move |symbol: V| {
        let mut params = Vec::new();
        // omitUndefined removes only undefined, not null or an empty string.
        if !(rt.is_undefined)(&symbol) {
            params.push(("symbol", symbol));
        }
        params.push(("limit", (rt.number)(100.0)));
        params.push((
            "page",
            (rt.number)(if target == endpoint("OPEN_ALGO_ORDERS") {
                1.0
            } else {
                0.0
            }),
        ));
        (rt.request)(
            host,
            WeexAccountReadRequest {
                endpoint: target.clone(),
                params: Some(params),
            },
        )
    };
    let first = match first.map(&make_request).transpose() {
        Ok(v) => v,
        Err(error) => {
            (rt.close_on_error)(host, &iterator, &error);
            return ready(Err(error));
        }
    };
    Box::pin(async move {
        let out = (rt.array)(Vec::new());
        let mut current = first;
        while let Some(request) = current {
            let body = (async {
                let data = request.await?;
                let push = (rt.push_method)(host, &out)?;
                let rows = (rt.extract_array)(host, &data, &["orders", "items"])?;
                (rt.push_spread)(host, push, &rows)
            })
            .await;
            if let Err(error) = body {
                (rt.close_on_error)(host, &iterator, &error);
                return Err(error);
            }
            current = match (rt.next)(host, &iterator)?.map(&make_request).transpose() {
                Ok(v) => v,
                Err(error) => {
                    (rt.close_on_error)(host, &iterator, &error);
                    return Err(error);
                }
            };
        }
        Ok(out)
    })
}

pub fn fetch_normal_orders<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbols: V,
) -> ReadFuture<'a, V, E> {
    let request = fetch_open_order_rows(host, rt, endpoint("OPEN_ORDERS"), symbols);
    Box::pin(async move {
        let rows = request.await?;
        let length = (rt.length)(host, &rows)?;
        let mut normalized = Vec::new();
        for index in 0..length {
            if (rt.present)(host, &rows, index)? {
                let row = (rt.at)(host, &rows, index)?;
                let markets = (rt.markets_by_symbol)(host)?;
                let symbol = (rt.get)(host, &row, "symbol")?;
                let market = (rt.market)(host, &markets, &symbol)?;
                normalized.push((rt.normalize_order)(host, row, market)?);
            }
        }
        Ok((rt.array)(
            normalized.into_iter().filter(rt.truthy).collect(),
        ))
    })
}

pub fn fetch_algo_orders<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbols: V,
) -> ReadFuture<'a, V, E> {
    let request = fetch_open_order_rows(host, rt, endpoint("OPEN_ALGO_ORDERS"), symbols);
    Box::pin(async move {
        let rows = request.await?;
        let length = (rt.length)(host, &rows)?;
        let mut out = Vec::new();
        for index in 0..length {
            if !(rt.present)(host, &rows, index)? {
                continue;
            }
            let row = (rt.at)(host, &rows, index)?;
            let markets = (rt.markets_by_symbol)(host)?;
            let symbol = (rt.get)(host, &row, "symbol")?;
            let market = (rt.market)(host, &markets, &symbol)?;
            let normalized = (rt.normalize_algo_order)(host, row, market)?;
            if (rt.is_array)(host, &normalized)? {
                let length = (rt.length)(host, &normalized)?;
                for index in 0..length {
                    if (rt.present)(host, &normalized, index)? {
                        out.push((rt.at)(host, &normalized, index)?);
                    }
                }
            } else {
                out.push(normalized);
            }
        }
        Ok((rt.array)(out))
    })
}

pub fn fetch_orders<'a, H, V: Clone + 'a, E: 'a>(
    host: &'a H,
    rt: &'a WeexAccountReadRuntime<H, V, E>,
    symbols: V,
) -> ReadFuture<'a, V, E> {
    match (rt.public_only)(host) {
        Ok(true) => return ready(Ok((rt.array)(Vec::new()))),
        Err(e) => return ready(Err(e)),
        Ok(false) => {}
    }
    let normal = fetch_normal_orders(host, rt, symbols.clone());
    let algo = fetch_algo_orders(host, rt, symbols);
    let aggregate = match (rt.settled)(host, normal, algo) {
        Ok(v) => v,
        Err(e) => return ready(Err(e)),
    };
    Box::pin(async move {
        let (normal, algo) = aggregate.await;
        let mut out = Vec::new();
        for orders in [normal, algo].into_iter().flatten() {
            spread(host, rt, &orders, &mut out)?;
        }
        (rt.uniq_by_id)(host, (rt.array)(out))
    })
}
