#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn frozen_typescript_terminal_oracle() {
        let cases: Vec<Value> =
            serde_json::from_str(include_str!("../tests/terminal_oracle.json")).unwrap();
        let mut store = DefaultStore::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        for case in cases {
            let now = case["now"].as_f64().unwrap();
            store.now = Arc::new(move || now);
            let args = &case["args"];
            let ids = |value: &Value| -> Vec<String> {
                value
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v["id"].as_str().unwrap().to_owned())
                    .collect()
            };
            match case["op"].as_str().unwrap() {
                "addOrder" => store
                    .add_order(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrder" => store
                    .add_or_update_order(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrders" => store
                    .add_or_update_orders(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "addOrUpdateOrdersFull" => store
                    .add_or_update_orders_full(serde_json::from_value(args[0].clone()).unwrap())
                    .unwrap(),
                "setOrdersForSymbol" => store
                    .set_orders_for_symbol(
                        args[0].as_str().unwrap(),
                        serde_json::from_value(args[1].clone()).unwrap(),
                    )
                    .unwrap(),
                "markOrdersTerminal" | "updateOrdersStatus" => {
                    let ids = ids(&args[0]);
                    let refs: Vec<_> = ids.iter().map(String::as_str).collect();
                    let status = serde_json::from_value(args[1].clone()).unwrap();
                    if case["op"] == "markOrdersTerminal" {
                        store.mark_orders_terminal(&refs, status).unwrap();
                    } else {
                        store.update_orders_status(&refs, status).unwrap();
                    }
                }
                "removeOrders" => {
                    let ids = ids(&args[0]);
                    store
                        .remove_orders(&ids.iter().map(String::as_str).collect::<Vec<_>>())
                        .unwrap();
                }
                "removeOrder" => store.remove_order(args[0]["id"].as_str().unwrap()).unwrap(),
                "update" => store
                    .update(
                        args[0]
                            .get("orders")
                            .map(|v| serde_json::from_value(v.clone()).unwrap()),
                        args[0]["updateIds"]["orders"].as_f64(),
                        args[1].as_bool().unwrap(),
                    )
                    .unwrap(),
                "reset" => store.reset().unwrap(),
                op => panic!("unknown {op}"),
            }
            let snapshot = store.serialize();
            let projection = json!({"orders":snapshot["orders"],"updateIds":snapshot["updateIds"],"requestStats":snapshot["requestStats"],"socketHealth":snapshot["socketHealth"]});
            crate::test_support::assert_json_eq(&projection, &case["expected"]);
            assert_eq!(
                calls.load(Ordering::Relaxed) as u64,
                case["notifications"].as_u64().unwrap(),
                "{case}"
            );
        }
    }
    #[test]
    fn terminal_cap_refresh_expiry_and_invalid_status() {
        let mut store = StoreState::new();
        store.now = Arc::new(|| 10.0);
        store
            .mark_orders_terminal(&["old"], OrderStatus::Closed)
            .unwrap();
        for i in 0..999 {
            store
                .mark_orders_terminal(&[&i.to_string()], OrderStatus::Canceled)
                .unwrap();
        }
        store
            .mark_orders_terminal(&["old"], OrderStatus::Closed)
            .unwrap();
        store
            .mark_orders_terminal(&["new"], OrderStatus::Closed)
            .unwrap();
        assert_eq!(store.terminal_order_ids.len(), 1000);
        assert!(!store.is_remembered_terminal("0", 11.0));
        assert!(store.is_remembered_terminal("old", 60009.0));
        assert!(!store.is_remembered_terminal("old", 60010.0));
        assert!(!store.is_remembered_terminal("new", f64::NAN));
        assert!(
            store
                .mark_orders_terminal(&["bad"], OrderStatus::Open)
                .is_err()
        );
        store.reset();
        assert!(store.terminal_order_ids.is_empty());
    }
    #[test]
    fn live_listener_identity_and_old_unsubscribe() {
        let mut store = StoreState::new();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let seen = calls.clone();
        let late: Listener = Arc::new(move |_, _| seen.lock().unwrap().push(2));
        let late_copy = late.clone();
        let seen = calls.clone();
        let first: Listener = Arc::new(move |store, _| {
            seen.lock().unwrap().push(1);
            let _remove = store.subscribe(late_copy.clone());
        });
        let remove = store.subscribe(first.clone());
        let _duplicate = store.subscribe(first.clone());
        store.reset();
        assert_eq!(*calls.lock().unwrap(), vec![1, 2]);
        remove();
        let _new_remove = store.subscribe(first);
        remove(); // The old closure removes the re-added callback identity too.
        calls.lock().unwrap().clear();
        store.reset();
        assert_eq!(*calls.lock().unwrap(), vec![2]);
    }
    #[test]
    fn request_increment_never_notifies_or_changes_socket_health() {
        let mut store = StoreState::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
        }));
        let socket = store.socket_health().clone();
        for _ in 0..1000 {
            store.record_request_outcome(RequestOutcome::Ok);
        }
        assert_eq!(store.request_stats().unwrap().ok, 1000.0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert_eq!(store.socket_health(), &socket);
        let snapshot = store.serialize();
        store.record_request_outcome(RequestOutcome::Auth);
        assert_eq!(snapshot["requestStats"]["auth"], 0.0);
        store.reset();
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(store.request_stats().unwrap(), &empty_request_counts());
        store.request_stats = None;
        store.record_request_outcome(RequestOutcome::Ok);
        assert!(store.serialize().get("requestStats").is_none());
    }
}
