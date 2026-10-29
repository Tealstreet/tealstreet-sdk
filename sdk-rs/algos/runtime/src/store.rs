use std::collections::HashMap;

use safe_cex_engine::model::{Market, Order, Position, Ticker, UpdateIds};

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
}

impl AlgoStore {
    pub fn order(&self, id: &str) -> Option<&Order> {
        self.orders.iter().find(|order| order.id == id)
    }
}
