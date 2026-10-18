//! Terminal memory through the actual JavaScript facade and native Store.
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
        let facade = store::facade(native.clone());
        terminal(&facade, "future-🚀", status).unwrap();
        assert!(
            native.orders().is_empty(),
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
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["live"],
            "JS terminal bridge remembers an ID before any order exists"
        );
        terminal(&facade, "live", status).unwrap();
        assert_eq!(
            native.orders()[0].status,
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
            native.orders()[0].status,
            status,
            "JS terminal bridge refuses stale reopen"
        );
        assert_eq!(
            native.orders()[1].status,
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
    let facade = store::facade(native.clone());
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
        native.orders()[0].status,
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
        error.response,
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
fn terminal_typed_ids_refuse_lone_surrogates_before_mutation() {
    let native = Rc::new(DefaultStore::new());
    let facade = store::facade(native.clone());
    let error = facade
        .call_method(
            "markOrdersTerminal",
            vec![
                Js::array(vec![Js::object(vec![("id", Js::utf16(&[0xd800]))])]),
                from_typed(&OrderStatus::Closed).unwrap(),
            ],
        )
        .expect_err("terminal typed ID must refuse rather than replace a surrogate");
    assert_eq!(error.to_exchange_error().name, "TypeError");
    assert_eq!(
        error.message(),
        "invalid utf-16: lone surrogate found",
        "the native String decoder must refuse rather than replace a surrogate"
    );
    Store::update(
        native.as_ref(),
        StoreUpdate {
            orders: Some(vec![order("�")]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    assert_eq!(
        native.orders().len(),
        1,
        "refused ID must not record a replacement-character terminal key"
    );
}
