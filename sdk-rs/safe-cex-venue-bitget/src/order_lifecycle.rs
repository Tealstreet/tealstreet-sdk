//! Private-stream order caches. These are JS Maps, not prototype-bearing objects.
use indexmap::IndexMap;

// ts: BitgetPrivateWebsocket.pruneOrderLifecycleCaches
#[derive(Default)]
pub struct OrderLifecycleCaches {
    pub terminal_order_tombstones: IndexMap<String, f64>,
    pub recent_ws_open_orders: IndexMap<String, f64>,
}
impl OrderLifecycleCaches {
    // ts: BitgetPrivateWebsocket.pruneOrderLifecycleCaches
    pub fn prune_order_lifecycle_caches(&mut self, now: f64) {
        // Negate the source <= test; > would incorrectly discard NaN entries.
        self.terminal_order_tombstones.retain(|_, expires| {
            (*expires)
                .partial_cmp(&now)
                .is_none_or(|order| order.is_gt())
        });
        self.recent_ws_open_orders.retain(|_, expires| {
            (*expires)
                .partial_cmp(&now)
                .is_none_or(|order| order.is_gt())
        });
    }
    // ts: BitgetPrivateWebsocket.markOrderAsTerminal
    pub fn mark_order_as_terminal(&mut self, order_id: &str, clock: impl FnOnce() -> f64) {
        let now = clock();
        self.terminal_order_tombstones
            .insert(order_id.to_owned(), now + 10_000.0);
        self.recent_ws_open_orders.shift_remove(order_id);
        if self.terminal_order_tombstones.len() > 2000 {
            self.prune_order_lifecycle_caches(now);
        }
    }
    // ts: BitgetPrivateWebsocket.markOrderAsOpenFromWs
    pub fn mark_order_as_open_from_ws(&mut self, order_id: &str, clock: impl FnOnce() -> f64) {
        let now = clock();
        self.recent_ws_open_orders
            .insert(order_id.to_owned(), now + 5_000.0);
        // Source deliberately leaves the terminal tombstone in place.
        if self.recent_ws_open_orders.len() > 2000 {
            self.prune_order_lifecycle_caches(now);
        }
    }
}
