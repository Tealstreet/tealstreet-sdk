//! Terminal memory through the actual JavaScript facade and native Store.
#[path = "support/store_correctness_source.rs"]
mod store_correctness_source;
use safe_cex_exchange::js_runtime::{
    store,
    value::{Js, from_typed},
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Order, OrderStatus, StoreSubscriberError};
use serde_json::json;
use std::{cell::Cell, rc::Rc};

fn order(id: &str) -> Order {
    serde_json::from_value(json!({"id":id,"symbol":"BTCUSD","status":"open","type":"limit",
        "side":"buy","price":20000,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":1})).unwrap()
}
fn terminal(
    facade: &Js,
    id: &str,
    status: OrderStatus,
) -> Result<Js, safe_cex_exchange::js_runtime::value::Thrown> {
    assert!(
        facade.get("markOrdersTerminal")?.is_callable(),
        "JS terminal bridge exposes callable markOrdersTerminal"
    );
    facade.call_method(
        "markOrdersTerminal",
        vec![
            Js::array(vec![Js::object(vec![("id", Js::str(id))])]),
            from_typed(&status).unwrap(),
        ],
    )
}

#[test]
fn terminal_memory_precedes_orders_and_filters_only_matching_stale_ids() {
    for status in [OrderStatus::Closed, OrderStatus::Canceled] {
        let native = Rc::new(DefaultStore::new());
        let facade = store::facade(native.clone()).unwrap();
        terminal(&facade, "future-🚀", status).unwrap();
        assert!(
            native.orders().unwrap().is_empty(),
            "marking an absent ID adds no order"
        );
        Store::update(
            native.as_ref(),
            StoreUpdate {
                orders: Some(vec![order("future-🚀"), order("live")]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            native
                .orders()
                .unwrap()
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["live"],
            "JS terminal bridge remembers an ID before any order exists"
        );
        terminal(&facade, "live", status).unwrap();
        assert_eq!(
            native.orders().unwrap()[0].status,
            status,
            "JS terminal bridge commits requested native status"
        );
        Store::update(
            native.as_ref(),
            StoreUpdate {
                orders: Some(vec![order("live"), order("other")]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert_eq!(
            native.orders().unwrap()[0].status,
            status,
            "JS terminal bridge refuses stale reopen"
        );
        assert_eq!(
            native.orders().unwrap()[1].status,
            OrderStatus::Open,
            "JS terminal memory leaves unrelated IDs unchanged"
        );
    }
}

#[test]
fn terminal_subscriber_error_preserves_committed_status_response_and_origin() {
    let native = Rc::new(DefaultStore::new());
    Store::update(
        native.as_ref(),
        StoreUpdate {
            orders: Some(vec![order("o")]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let facade = store::facade(native.clone()).unwrap();
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    let response = json!({"data":{"msg":"terminal observer failed"}});
    let cause = StoreSubscriberError::with_response("terminal observer failed", response.clone());
    let thrown = cause.clone();
    let _first = Store::subscribe(
        native.as_ref(),
        Box::new(move |state, _| {
            assert_eq!(
                state.orders[0].status,
                OrderStatus::Closed,
                "subscriber observes committed terminal status"
            );
            seen.set(seen.get() + 1);
            Err(thrown.clone())
        }),
    );
    let later = Rc::new(Cell::new(0));
    let skipped = later.clone();
    let _second = Store::subscribe(
        native.as_ref(),
        Box::new(move |_, _| {
            skipped.set(skipped.get() + 1);
            Ok(())
        }),
    );
    let error = terminal(&facade, "o", OrderStatus::Closed)
        .expect_err("JS terminal bridge must propagate subscriber failure")
        .to_exchange_error();
    assert_eq!(
        native.orders().unwrap()[0].status,
        OrderStatus::Closed,
        "subscriber failure retains terminal commit"
    );
    assert_eq!(calls.get(), 1);
    assert_eq!(
        later.get(),
        0,
        "first subscriber error aborts later subscribers"
    );
    assert_eq!(error.message, "terminal observer failed");
    assert_eq!(
        error.response_json_view(),
        Some(response),
        "JS terminal bridge retains subscriber response"
    );
    assert_eq!(
        error.store_subscriber_ids,
        [0],
        "JS terminal bridge retains subscriber identity"
    );
    assert_eq!(
        error.store_subscriber_cause,
        Some(cause),
        "JS terminal bridge retains subscriber cause"
    );
    assert!(
        error.origin.is_some(),
        "JS terminal bridge retains native mutation origin"
    );
}

#[test]
fn terminal_raw_ids_preserve_surrogates_without_replacement_aliasing() {
    let source = store_correctness_source::capture();
    let native = Rc::new(DefaultStore::new());
    let facade = store::facade(native.clone()).unwrap();
    facade
        .call_method(
            "markOrdersTerminal",
            vec![
                Js::array(vec![Js::object(vec![("id", Js::utf16(&[0xd800]))])]),
                from_typed(&OrderStatus::Closed).unwrap(),
            ],
        )
        .unwrap();
    let replacement = from_typed(&order("�")).unwrap();
    let surrogate = from_typed(&order("�")).unwrap();
    surrogate.set("id", Js::utf16(&[0xd800])).unwrap();
    facade
        .call_method(
            "update",
            vec![Js::object(vec![(
                "orders",
                Js::array(vec![replacement.clone(), surrogate]),
            )])],
        )
        .unwrap();
    let rows = facade
        .get("orders")
        .unwrap()
        .array_values()
        .unwrap()
        .unwrap();
    assert_eq!(rows.len(), 1, "only the original surrogate ID is filtered");
    assert!(
        rows[0].same_reference(&replacement),
        "raw terminal memory preserves replacement-character row identity"
    );
    let ids: Vec<Vec<u16>> = rows
        .iter()
        .map(|row| row.get("id").unwrap().as_str().unwrap().to_vec())
        .collect();
    assert_eq!(json!(ids), source["terminalIds"]);
    assert_eq!(native.orders().unwrap()[0].id, "�");
}
