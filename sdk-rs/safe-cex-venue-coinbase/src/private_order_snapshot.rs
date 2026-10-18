//! Coinbase private order snapshot assembly with Store notification propagation.
use indexmap::IndexMap;
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_types::Order;

pub const COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE: usize = 50;

/// The Map key is captured at insertion; later changes to an order do not rekey it.
pub trait CoinbaseSnapshotOrder: Clone {
    fn snapshot_order_id(&self) -> String;
}
impl CoinbaseSnapshotOrder for Order {
    fn snapshot_order_id(&self) -> String {
        self.id.clone()
    }
}
impl<I> CoinbaseSnapshotOrder for crate::order_sync::SyncedOrderRoot<I> {
    fn snapshot_order_id(&self) -> String {
        self.borrow().fields.id.clone()
    }
}
#[derive(Debug)]
pub struct CoinbasePrivateOrderSnapshot<O = Order> {
    orders_by_id: Option<IndexMap<String, O>>,
    raw_order_count: usize,
}
impl<O> Default for CoinbasePrivateOrderSnapshot<O> {
    fn default() -> Self {
        Self {
            orders_by_id: None,
            raw_order_count: 0,
        }
    }
}
impl<O: CoinbaseSnapshotOrder> CoinbasePrivateOrderSnapshot<O> {
    pub fn merge_snapshot_orders(&mut self, orders: &[O]) {
        let current = self.orders_by_id.get_or_insert_with(IndexMap::new);
        for order in orders {
            current.insert(order.snapshot_order_id(), order.clone());
        }
    }
    /// Return a completed publication after resetting assembly. Callers notify only
    /// after releasing their state borrow, so callbacks may start a new snapshot.
    pub fn prepare_order_snapshot_batch(
        &mut self,
        raw_order_count: usize,
        orders: &[O],
    ) -> Option<Vec<O>> {
        self.merge_snapshot_orders(orders);
        self.raw_order_count += raw_order_count;
        if raw_order_count >= COINBASE_USER_ORDER_SNAPSHOT_BATCH_SIZE {
            return None;
        }
        let snapshot_orders = self
            .orders_by_id
            .take()
            .unwrap_or_default()
            .into_values()
            .collect::<Vec<_>>();
        let snapshot_raw_order_count = std::mem::take(&mut self.raw_order_count);
        if snapshot_raw_order_count == 0 || !snapshot_orders.is_empty() {
            Some(snapshot_orders)
        } else {
            None
        }
    }
    pub fn reset_order_snapshot_assembly(&mut self) {
        self.orders_by_id = None;
        self.raw_order_count = 0;
    }
    pub fn has_pending_snapshot(&self) -> bool {
        self.orders_by_id.is_some()
    }
    pub fn pending_snapshot_orders(&self) -> Option<Vec<O>> {
        self.orders_by_id
            .as_ref()
            .map(|orders| orders.values().cloned().collect())
    }
    pub fn pending_raw_order_count(&self) -> usize {
        self.raw_order_count
    }
}
impl CoinbasePrivateOrderSnapshot<Order> {
    // ts: CoinbasePrivateWebsocket.applyOrderSnapshotBatch
    pub fn apply_order_snapshot_batch(
        &mut self,
        store: &dyn Store,
        raw_order_count: usize,
        orders: &[Order],
    ) -> Result<(), StoreMutationError> {
        if let Some(snapshot_orders) = self.prepare_order_snapshot_batch(raw_order_count, orders) {
            let mut loaded = store.loaded().clone();
            loaded.orders = true;
            store.update(
                StoreUpdate {
                    orders: Some(snapshot_orders),
                    loaded: Some(loaded),
                    ..StoreUpdate::default()
                },
                false,
            )?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::CoinbasePrivateOrderSnapshot;
    use safe_cex_store::store::DefaultStore;
    use safe_cex_types::{
        Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, StoreSubscriberError,
    };
    use std::sync::Arc;

    fn source_cases() -> serde_json::Value {
        let output = std::process::Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/oracle.cjs"))
            .output()
            .expect("Node is required for the maintained Coinbase TS oracle");
        assert!(
            output.status.success(),
            "Coinbase TS oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("Coinbase oracle JSON")
    }

    fn order(id: &str, price: f64) -> Order {
        Order {
            client_order_id: (None).into(),
            id: id.into(),
            parent_id: (None).into(),
            status: safe_cex_types::ModelField::Value(OrderStatus::Open),
            symbol: "BTC-USD".into(),
            r#type: safe_cex_types::ModelField::Value(OrderType::Limit),
            side: safe_cex_types::ModelField::Value(OrderSide::Buy),
            price: price.into(),
            amount: 2.0,
            filled: 0.0,
            remaining: 2.0,
            reduce_only: false.into(),
            close: (None).into(),
            info: (None).into(),
            time_in_force: (Some(OrderTimeInForce::GoodTillCancel)).into(),
            last_update: 1.0,
            trigger_type: (None).into(),
            tp_trigger_type: (None).into(),
            sl_trigger_type: (None).into(),
            take_profit: (None).into(),
            stop_loss: (None).into(),
            extra: (None).into(),
        }
    }

    #[test]
    fn snapshot_uses_raw_batch_size_and_preserves_map_identity_order() {
        let store = DefaultStore::new();
        let mut snapshot = CoinbasePrivateOrderSnapshot::default();
        let first = [order("one", 10.0), order("two", 20.0)];
        snapshot
            .apply_order_snapshot_batch(&store, 50, &first)
            .unwrap();
        assert!(store.orders().is_empty());

        let replacement = [order("one", 11.0)];
        snapshot
            .apply_order_snapshot_batch(&store, 1, &replacement)
            .unwrap();
        let actual = store.orders();
        assert_eq!(actual.len(), 2, "snapshot batch count fixture");
        assert_eq!(actual[0].id, "one", "snapshot insertion order");
        assert_eq!(actual[0].price, 11.0, "snapshot replacement identity");
        assert_eq!(actual[1].id, "two", "snapshot second field identity");
        assert!(store.loaded().orders);
        assert_eq!(snapshot.pending_raw_order_count(), 0);
    }

    #[test]
    fn nonempty_quarantined_snapshot_retains_existing_orders() {
        let store = DefaultStore::new();
        store.add_or_update_order(order("old", 9.0)).unwrap();
        let mut snapshot = CoinbasePrivateOrderSnapshot::default();
        snapshot.apply_order_snapshot_batch(&store, 1, &[]).unwrap();
        assert_eq!(
            store.orders()[0].id,
            "old",
            "quarantined snapshot retention"
        );
        assert!(!store.loaded().orders, "quarantined snapshot loaded flag");
    }

    #[test]
    fn empty_snapshot_clears_orders_and_marks_loaded() {
        let store = DefaultStore::new();
        store.add_or_update_order(order("old", 9.0)).unwrap();
        let mut snapshot = CoinbasePrivateOrderSnapshot::default();
        snapshot.apply_order_snapshot_batch(&store, 0, &[]).unwrap();
        assert!(store.orders().is_empty(), "empty snapshot clears orders");
        assert!(store.loaded().orders, "empty snapshot loaded flag");
    }

    #[test]
    fn snapshot_notification_failure_propagates_after_commit_and_reset() {
        let store = DefaultStore::new();
        let _unsubscribe = store.subscribe(Arc::new(|_, _| {
            Err(StoreSubscriberError::new("subscriber failed"))
        }));
        let mut snapshot = CoinbasePrivateOrderSnapshot::default();
        snapshot.merge_snapshot_orders(&[order("one", 10.0)]);
        let error = snapshot
            .apply_order_snapshot_batch(&store, 1, &[])
            .unwrap_err();
        assert_eq!(error.to_string(), "subscriber failed");
        assert_eq!(store.orders()[0].id, "one", "snapshot commits before error");
        assert!(store.loaded().orders, "loaded flag commits before error");
        assert_eq!(
            snapshot.pending_raw_order_count(),
            0,
            "snapshot reset before notify"
        );
    }

    #[test]
    fn production_typescript_snapshot_cases_pin_boundary_and_error_order() {
        let output = source_cases();
        let cases = output["snapshotCases"].as_array().expect("snapshot cases");
        assert_eq!(cases.len(), 4);

        let complete = &cases[0];
        assert_eq!(
            complete["updates"], 1,
            "TS current-batch completion boundary"
        );
        assert_eq!(complete["orders"][0]["id"], "one");
        assert_eq!(complete["orders"][0]["price"], 11);
        assert_eq!(complete["orders"][1]["id"], "two");
        assert!(complete["loaded"].as_bool().unwrap());
        assert_eq!(complete["updates"], 1);

        let quarantined = &cases[1];
        assert_eq!(quarantined["orders"][0]["id"], "old");
        assert!(!quarantined["loaded"].as_bool().unwrap());
        assert_eq!(quarantined["updates"], 0);

        let empty = &cases[2];
        assert!(empty["orders"].as_array().unwrap().is_empty());
        assert!(empty["loaded"].as_bool().unwrap());
        assert_eq!(empty["updates"], 1);

        let failed = &cases[3];
        assert_eq!(failed["error"], "subscriber failed");
        assert_eq!(failed["orders"][0]["id"], "one");
        assert!(failed["loaded"].as_bool().unwrap());
        assert_eq!(failed["pendingRawOrderCount"], 0);
    }
}
