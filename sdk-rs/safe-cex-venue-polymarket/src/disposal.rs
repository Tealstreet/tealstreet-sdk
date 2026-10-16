//! Venue disposal bodies; parent disposal is the shared exchange/socket boundary.
use crate::ws_timers::{clear_private_ping_interval, clear_public_ping_interval};
use safe_cex_exchange::runtime::ExchangeTask;
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashSet},
};

// ts: PolymarketExchange.dispose
pub fn dispose_exchange<E>(dispose_base: impl FnOnce() -> Result<(), E>) -> Result<(), E> {
    dispose_base()
}

// ts: PolymarketPrivateWebSocket.dispose
pub fn dispose_private_websocket<E>(
    ping_interval: &RefCell<Option<Box<dyn ExchangeTask>>>,
    dispose_base: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    clear_private_ping_interval(&mut ping_interval.borrow_mut());
    dispose_base()
}

// ts: PolymarketPublicWebSocket.dispose. Clearing does not inspect keys or values;
// collection element types remain caller-owned, as do the future socket fields.
pub fn dispose_public_websocket<K, V, E>(
    ping_interval: &RefCell<Option<Box<dyn ExchangeTask>>>,
    subscribed_assets: &RefCell<HashSet<K>>,
    orderbook_callbacks: &RefCell<BTreeMap<K, V>>,
    dispose_base: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    clear_public_ping_interval(&mut ping_interval.borrow_mut());
    subscribed_assets.borrow_mut().clear();
    orderbook_callbacks.borrow_mut().clear();
    dispose_base()
}
