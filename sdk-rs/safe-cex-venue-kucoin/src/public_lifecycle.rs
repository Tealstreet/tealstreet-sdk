use crate::ws_helpers::KucoinPublicRole;
use std::{cell::Cell, collections::HashSet, rc::Rc};

pub fn on_open_futures<E>(
    is_disposed: &Cell<bool>,
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    reconnect_enabled: impl FnOnce() -> bool,
) -> Result<(), E> {
    if !is_disposed.get() {
        for action in [
            "subscribe",
            "ping",
            "emitter.off:markets:changed",
            "emitter.on:markets:changed",
        ] {
            invoke(action)?;
        }
        if reconnect_enabled() {
            invoke("emitWsReconnected")?;
        }
    }
    Ok(())
}

pub fn on_open_spot<E>(
    is_disposed: &Cell<bool>,
    mut invoke: impl FnMut(&str) -> Result<(), E>,
    reconnect_enabled: impl FnOnce() -> bool,
) -> Result<(), E> {
    if is_disposed.get() {
        return Ok(());
    }
    invoke("subscribe")?;
    invoke("ping")?;
    if reconnect_enabled() {
        invoke("emitWsReconnected")?;
    }
    Ok(())
}

pub fn ticker_for_api_symbol<T, E>(
    api_symbol: Option<&[u16]>,
    lookup: impl FnOnce(&[u16]) -> Result<Option<Rc<T>>, E>,
) -> Result<Option<Rc<T>>, E> {
    let Some(api_symbol) = api_symbol.filter(|symbol| !symbol.is_empty()) else {
        return Ok(None);
    };
    lookup(api_symbol)
}

/// Host reads preserve length coercion and live HasProperty/Get, including prototypes.
pub trait MarketAdditions<M, E> {
    fn empty(&self) -> Result<bool, E>;
    fn map_length(&self) -> Result<usize, E>;
    /// None means HasProperty=false; a present undefined value needs a native M.
    fn at(&self, index: usize) -> Result<Option<Rc<M>>, E>;
}

pub fn on_markets_changed<M, E>(
    added: &impl MarketAdditions<M, E>,
    has_ws: impl FnOnce() -> Result<bool, E>,
    is_disposed: &Cell<bool>,
    is_connected: impl FnOnce() -> Result<bool, E>,
    role: &Cell<KucoinPublicRole>,
    snapshot_shard_symbols: impl FnOnce() -> Result<Vec<Vec<u16>>, E>,
    mut symbol_for_market: impl FnMut(&M) -> Result<Vec<u16>, E>,
    send_channel_op: impl FnOnce(&str, &[&str], &[Vec<u16>]) -> Result<(), E>,
) -> Result<(), E> {
    let empty = added.empty()?;
    if empty || !has_ws()? || is_disposed.get() || !is_connected()? {
        return Ok(());
    }
    if role.get() != KucoinPublicRole::Snapshot {
        return Ok(());
    }
    let owned: HashSet<Vec<u16>> = snapshot_shard_symbols()?.into_iter().collect();
    let map_length = added.map_length()?;
    let mut mapped = vec![];
    for index in 0..map_length {
        let market = added.at(index)?;
        if let Some(market) = market {
            mapped.push(symbol_for_market(&market)?);
        }
    }
    let new_symbols = mapped
        .into_iter()
        .filter(|symbol| !symbol.is_empty() && owned.contains(symbol))
        .collect::<Vec<_>>();
    if !new_symbols.is_empty() {
        send_channel_op("subscribe", &["/contractMarket/snapshot"], &new_symbols)?;
    }
    Ok(())
}
