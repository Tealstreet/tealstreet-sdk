use crate::ws_helpers::KucoinPublicRole;
use std::cell::Cell;

pub fn subscribe_public<E>(
    role: KucoinPublicRole,
    snapshot_shard_symbols: impl FnOnce() -> Result<Vec<Vec<u16>>, E>,
    send_channel_op: impl FnOnce(&str, &[&str], &[Vec<u16>]) -> Result<(), E>,
) -> Result<(), E> {
    if role == KucoinPublicRole::Snapshot {
        let symbols = snapshot_shard_symbols()?;
        send_channel_op("subscribe", &["/contractMarket/snapshot"], &symbols)?;
    }
    Ok(())
}

pub fn subscribe_active_ticker<E>(
    api_symbol: &[u16],
    is_disposed: &Cell<bool>,
    send_channel_op: impl FnOnce(&str, &[&str], &[Vec<u16>]) -> Result<(), E>,
) -> Result<(), E> {
    if !api_symbol.is_empty() && !is_disposed.get() {
        send_channel_op(
            "subscribe",
            &["/contractMarket/tickerV2", "/contract/instrument"],
            &[api_symbol.to_vec()],
        )?;
    }
    Ok(())
}

pub fn unsubscribe_active_ticker<E>(
    api_symbol: &[u16],
    is_disposed: &Cell<bool>,
    send_channel_op: impl FnOnce(&str, &[&str], &[Vec<u16>]) -> Result<(), E>,
) -> Result<(), E> {
    if !api_symbol.is_empty() && !is_disposed.get() {
        send_channel_op(
            "unsubscribe",
            &["/contractMarket/tickerV2", "/contract/instrument"],
            &[api_symbol.to_vec()],
        )?;
    }
    Ok(())
}
