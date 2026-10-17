//! Native observations supporting the four private-construction Clone reviews.
use safe_cex_store::{
    Store, StoreMutationError, StoreUpdate,
    risk::{calculate_orders_risk, format_pnl},
    store::DefaultStore,
    store_stub::STUB_STORE,
    virtual_clock::VirtualClock,
};
use safe_cex_types::{OrderStatus, StoreSubscriberError};
use serde_json::{Map, json};
use std::{cell::Cell, error::Error, rc::Rc, sync::Arc};

#[test]
fn risk_error_clone_keeps_both_public_failure_shapes_and_owns_its_key() {
    let range = format_pnl(12.5, Some(101.0), None).unwrap_err();
    let copy = range.clone();
    assert_eq!(copy, range);
    assert_eq!(copy.name(), "RangeError");
    assert!(copy.key().is_none());
    assert!(copy.member().is_none());
    drop(range);
    assert_eq!(
        copy.message(),
        "minimumFractionDigits value is out of range."
    );

    let mut order = STUB_STORE.orders[0].clone();
    order.status = OrderStatus::Open.into();
    order.symbol = "constructor".into();
    let inherited = calculate_orders_risk(&[], &[], &[], &[order], None).unwrap_err();
    let copy = inherited.clone();
    assert_eq!(copy, inherited);
    assert_eq!(copy.name(), "TypeError");
    assert_eq!(copy.member(), Some(safe_cex_num::ProtoMember::Constructor));
    assert_eq!(copy.key(), Some("constructor"));
    assert_ne!(
        copy.key().unwrap().as_ptr(),
        inherited.key().unwrap().as_ptr()
    );
    let display = inherited.to_string();
    drop(inherited);
    assert_eq!(copy.to_string(), display);
    assert_eq!(copy.key(), Some("constructor"));
}

#[test]
fn patch_error_clone_is_owned_failure_data_after_a_real_store_refusal() {
    let store = DefaultStore::new();
    store
        .update_full(
            StoreUpdate {
                orders: Some(STUB_STORE.orders.clone()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let before = store.orders().to_vec();
    let fields = Map::from_iter([("amount".into(), json!("invalid-native-number"))]);
    let StoreMutationError::Patch(original) =
        store.update_order(&before[0].id, &fields).unwrap_err()
    else {
        panic!("actual typed field refusal must retain StorePatchError");
    };
    let copy = original.clone();
    assert_eq!(copy, original);
    assert_eq!(copy.field(), "amount");
    assert_ne!(copy.field().as_ptr(), original.field().as_ptr());
    assert_ne!(copy.message().as_ptr(), original.message().as_ptr());
    assert_eq!(copy.is_type_error(), original.is_type_error());
    let message = original.to_string();
    drop(original);
    assert_eq!(copy.to_string(), message);
    assert_eq!(
        *store.orders(),
        before,
        "cloning failure data cannot commit the refused patch"
    );
}

#[test]
fn notification_error_clone_preserves_subscriber_and_shared_native_cause() {
    let store = DefaultStore::new();
    store.set_setting("isHedged", false).unwrap();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let cause: Rc<dyn Error> = Rc::new(std::io::Error::other("native subscriber cause"));
    let retained = cause.clone();
    let contract: &dyn Store = &store;
    let _cleanup = contract.subscribe(Box::new(move |_, _| {
        count.set(count.get() + 1);
        Err(StoreSubscriberError::with_response(
            "native callback failure",
            json!({"status":409,"data":{"original":true}}),
        )
        .with_cause(retained.clone()))
    }));
    let StoreMutationError::Notification(original) =
        store.set_setting("isHedged", true).unwrap_err()
    else {
        panic!("actual subscriber refusal must retain StoreNotificationError");
    };
    let copy = original.clone();
    assert_eq!(copy, original);
    assert_eq!(copy.subscriber_id(), original.subscriber_id());
    assert_eq!(copy.cause().response(), original.cause().response());
    assert!(!std::ptr::eq(
        copy.cause().response().unwrap(),
        original.cause().response().unwrap()
    ));
    assert!(std::ptr::eq(copy.cause().source().unwrap(), cause.as_ref()));
    assert!(std::ptr::eq(
        copy.cause().source().unwrap(),
        original.cause().source().unwrap()
    ));
    drop(original);
    assert_eq!(copy.message(), "native callback failure");
    assert_eq!(
        copy.cause().source().unwrap().to_string(),
        "native subscriber cause"
    );
    assert_eq!(calls.get(), 1, "cloning the error never repeats fanout");
    assert!(
        store.options().is_hedged,
        "source mutation committed before the original failure"
    );
}

fn clock() -> VirtualClock {
    let mut clock = VirtualClock::default();
    clock.runtime = "worker".into();
    clock.now = Arc::new(|| 10_000.0);
    clock
}
#[test]
fn virtual_clock_clone_shares_one_clock_state_and_copies_runtime_configuration() {
    let original = clock();
    let mut copy = original.clone();
    let independent = clock();
    assert!(Arc::ptr_eq(&original.now, &copy.now));
    assert_ne!(original.runtime.as_ptr(), copy.runtime.as_ptr());
    copy.sync_from_server_time(15_000.0);
    assert_eq!(original.get_current_time_ms(), 15_000.0);
    assert_eq!(copy.get_current_time_ms(), 15_000.0);
    assert_eq!(independent.get_current_time_ms(), 10_000.0);
    original.sync_from_server_time(30_000.0);
    assert_eq!(copy.get_current_time_ms(), 30_000.0);
    copy.runtime = "native".into();
    assert_eq!(copy.runtime, "native");
    assert_eq!(original.runtime, "worker");
    assert_eq!(original.get_current_time_ms(), 30_000.0);
    drop(original);
    copy.runtime = "worker".into();
    assert_eq!(copy.get_current_time_ms(), 30_000.0);
}
