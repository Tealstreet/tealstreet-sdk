//! Symbol-config lookup with deferred fetch and source defaults.
use safe_cex_types::{MarginType, PositionMode};
use std::{future::Future, pin::Pin};

#[derive(Debug, PartialEq)]
pub struct WeexDefaultSymbolConfig {
    pub margin_type: MarginType,
    pub position_mode: PositionMode,
    pub cross_leverage: f64,
    pub isolated_long_leverage: f64,
    pub isolated_short_leverage: f64,
}

pub struct WeexSymbolSettingsRuntime<H, V, M, E> {
    pub cached: fn(&H, &[u16]) -> Result<V, E>,
    pub truthy: fn(&V) -> bool,
    pub fetch: for<'a> fn(&'a H, &'a [u16]) -> Pin<Box<dyn Future<Output = Result<M, E>> + 'a>>,
    pub fetched: fn(&H, &M, &[u16]) -> Result<V, E>,
    pub account_position_mode: fn(&H) -> Result<PositionMode, E>,
    pub default_config: fn(&H, WeexDefaultSymbolConfig) -> Result<V, E>,
}

pub fn get_symbol_config_from_cache<H, V, M, E>(
    host: &H,
    runtime: &WeexSymbolSettingsRuntime<H, V, M, E>,
    symbol: &[u16],
) -> Result<V, E> {
    (runtime.cached)(host, symbol)
}

pub fn get_or_fetch_symbol_config<'a, H, V: 'a, M: 'a, E: 'a>(
    host: &'a H,
    runtime: &'a WeexSymbolSettingsRuntime<H, V, M, E>,
    symbol: &'a [u16],
    refetch: bool,
) -> Pin<Box<dyn Future<Output = Result<V, E>> + 'a>> {
    let cached = match (runtime.cached)(host, symbol) {
        Ok(value) => value,
        Err(error) => return Box::pin(std::future::ready(Err(error))),
    };
    if (runtime.truthy)(&cached) && !refetch {
        return Box::pin(std::future::ready(Ok(cached)));
    }
    let fetch = (runtime.fetch)(host, symbol);
    Box::pin(async move {
        let configs = fetch.await?;
        let config = (runtime.fetched)(host, &configs, symbol)?;
        if (runtime.truthy)(&config) {
            return Ok(config);
        }
        let position_mode = (runtime.account_position_mode)(host)?;
        (runtime.default_config)(
            host,
            WeexDefaultSymbolConfig {
                margin_type: MarginType::Cross,
                position_mode,
                cross_leverage: 20.0,
                isolated_long_leverage: 20.0,
                isolated_short_leverage: 20.0,
            },
        )
    })
}
