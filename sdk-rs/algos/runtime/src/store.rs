use std::collections::HashMap;

use safe_cex_engine::model::{Balance, Market, Order, Position, Ticker, UpdateIds};

/// The account state an algo reads, decoded from engine frames by the port.
/// It is a copy: an algo never mutates the engine's store.
#[derive(Clone, Debug, Default)]
pub struct AlgoStore {
    /// The venue name, for the few venue-specific rules ported from TS.
    pub venue: String,
    pub orders: Vec<Order>,
    pub positions: Vec<Position>,
    pub tickers: HashMap<String, Ticker>,
    pub markets: HashMap<String, Market>,
    pub update_ids: UpdateIds,
    /// `None` until the account's first balance frame.
    pub balance: Option<Balance>,
    pub private_socket: PrivateSocketHealth,
}

/// The private socket's health as the TS `store.socketHealth` reports it.
/// All false until the port learns otherwise; absent telemetry is not failure.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PrivateSocketHealth {
    /// The socket delivered at least one message.
    pub initialized: bool,
    pub healthy: bool,
    /// Reconnects stopped after repeated failures.
    pub reconnect_paused: bool,
}

impl AlgoStore {
    pub fn order(&self, id: &str) -> Option<&Order> {
        self.orders.iter().find(|order| order.id == id)
    }
}
