use super::*;
use crate::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate, StoreUpdateIdsPatch};
use safe_cex_types::{
    OrderStatus, RequestOutcome, RequestOutcomeCounts, StoreData, StoreSubscriberError,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
#[path = "../../../safe-cex-store/src/test_support.rs"]
mod comparison;
fn snapshot(store: &DefaultStore) -> Value {
    crate::js_runtime::value::to_json(&store.graph.serialize().unwrap()).unwrap()
}
use std::sync::atomic::{AtomicUsize, Ordering};
#[test]
fn frozen_typescript_terminal_oracle() {
    let cases: Vec<Value> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-store/tests/terminal_oracle.json"
    )))
    .unwrap();
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
                    StoreUpdate {
                        orders: args[0]
                            .get("orders")
                            .map(|v| serde_json::from_value(v.clone()).unwrap()),
                        update_ids: Some(StoreUpdateIdsPatch {
                            orders: args[0]["updateIds"]["orders"].as_f64(),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    args[1].as_bool().unwrap(),
                )
                .unwrap(),
            "reset" => store.reset().unwrap(),
            op => panic!("unknown {op}"),
        }
        let snapshot = snapshot(&store);
        let projection = json!({"orders":snapshot["orders"],"updateIds":snapshot["updateIds"],"requestStats":snapshot["requestStats"],"socketHealth":snapshot["socketHealth"]});
        comparison::assert_json_eq(&projection, &case["expected"]);
        assert_eq!(
            calls.load(Ordering::Relaxed) as u64,
            case["notifications"].as_u64().unwrap(),
            "{case}"
        );
    }
}
#[test]
fn terminal_cap_refresh_expiry_and_invalid_status() {
    let mut store = DefaultStore::new();
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
    assert_eq!(
        store
            .graph
            .terminal_orders
            .get("size")
            .unwrap()
            .to_number()
            .unwrap() as usize,
        1000
    );
    assert!(!store.graph.remembered(Js::str("0"), 11.0).unwrap());
    assert!(store.graph.remembered(Js::str("old"), 60009.0).unwrap());
    assert!(!store.graph.remembered(Js::str("old"), 60010.0).unwrap());
    assert!(!store.graph.remembered(Js::str("new"), f64::NAN).unwrap());
    assert!(
        store
            .mark_orders_terminal(&["bad"], OrderStatus::Open)
            .is_err()
    );
    store.reset().unwrap();
    assert!(
        store
            .graph
            .terminal_orders
            .get("size")
            .unwrap()
            .to_number()
            .unwrap()
            == 0.0
    );
}
#[test]
fn live_listener_identity_and_old_unsubscribe() {
    let store = Rc::new(DefaultStore::new());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let seen = calls.clone();
    type Callback = Arc<dyn Fn(&StoreData, bool) -> Result<(), StoreSubscriberError>>;
    let late: Callback = Arc::new(move |_, _| {
        seen.lock().unwrap().push(2);
        Ok(())
    });
    let late_copy = late.clone();
    let seen = calls.clone();
    let weak = Rc::downgrade(&store);
    let first: Callback = Arc::new(move |_, _| {
        seen.lock().unwrap().push(1);
        let _remove = weak.upgrade().unwrap().subscribe(late_copy.clone());
        Ok(())
    });
    let remove = store.subscribe(first.clone());
    let _duplicate = store.subscribe(first.clone());
    store.reset().unwrap();
    assert_eq!(*calls.lock().unwrap(), vec![1, 2]);
    remove();
    let _new_remove = store.subscribe(first);
    remove();
    calls.lock().unwrap().clear();
    store.reset().unwrap();
    assert_eq!(*calls.lock().unwrap(), vec![2]);
}

#[test]
fn request_increment_never_notifies_or_changes_socket_health() {
    let store = DefaultStore::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }));
    let socket = store.socket_health().unwrap();
    for _ in 0..1000 {
        store.record_request_outcome(RequestOutcome::Ok).unwrap();
    }
    assert_eq!(store.request_stats().unwrap().unwrap().ok, 1000.0);
    assert_eq!(calls.load(Ordering::Relaxed), 0);
    assert_eq!(store.socket_health().unwrap(), socket);
    let saved_snapshot = snapshot(&store);
    store.record_request_outcome(RequestOutcome::Auth).unwrap();
    assert_eq!(saved_snapshot["requestStats"]["auth"], 0.0);
    store.reset().unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(
        store.request_stats().unwrap().unwrap(),
        RequestOutcomeCounts {
            ok: 0.0,
            rate_limit: 0.0,
            auth: 0.0,
            network: 0.0,
            other: 0.0
        }
    );
    store
        .graph
        .state
        .borrow()
        .delete(&utf16("requestStats"))
        .unwrap();
    store.record_request_outcome(RequestOutcome::Ok).unwrap();
    assert!(snapshot(&store).get("requestStats").is_none());
    // The native source snapshot owns undefined; the strict typed optional
    // record cannot represent that present property and refuses visibly.
    assert_eq!(
        store.serialize().unwrap_err().phase(),
        safe_cex_types::StoreReadPhase::Projection
    );
}
