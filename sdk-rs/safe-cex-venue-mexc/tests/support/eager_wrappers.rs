use super::*;
use safe_cex_store::Store;
use std::cell::Cell;

const LEVERAGE: &str = "/api/v1/private/position/leverage";

fn load_market(
    venue: &safe_cex_venue_mexc::exchange::MexcExchange,
    rt: &support::Runtime,
    wire: &support::Wire,
    store: &safe_cex_store::store::DefaultStore,
) {
    script_venue(wire);
    let markets = drive(rt, venue.fetch_markets()).unwrap();
    Store::update(
        store,
        StoreUpdate {
            markets: Some(markets),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    calls(wire);
}

#[test]
fn eager_metadata_notifies_before_returning_its_rejected_waiter() {
    let (venue, rt, store, wire) = support::fixture_with(
        json!({"key":"","secret":""}),
        Rc::new(|_, _| panic!("no external provider")),
    );
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _subscription = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            observed.set(observed.get() + 1);
            Err(safe_cex_types::StoreSubscriberError::new(
                "eager subscriber failed",
            ))
        }),
    );
    let waiter = (&*venue as &dyn Exchange).update_symbol_metadata("BTCUSDT".into());
    assert_eq!(
        notifications.get(),
        1,
        "metadata source prefix notifies before SDK waiter polling"
    );
    assert_eq!(
        store.fetched_leverage()["BTCUSDT"].buy_leverage,
        20.0,
        "commit precedes throw"
    );
    let error = drive(&rt, waiter).unwrap_err();
    assert_eq!(error.message, "eager subscriber failed");
    assert_eq!(notifications.get(), 1);
    assert!(calls(&wire).is_empty());
}

#[test]
fn eager_metadata_held_request_finishes_after_waiter_drop() {
    let (venue, rt, store, wire) = support::fixture();
    load_market(&venue, &rt, &wire, &store);
    script(
        &wire,
        LEVERAGE,
        json!([
            {"positionType":1,"leverage":31,"maxLeverageView":80,"openType":2},
            {"positionType":2,"leverage":12,"maxLeverage":60,"openType":1}
        ]),
    );
    wire.hold_responses.set(true);
    let waiter = (&*venue as &dyn Exchange).update_symbol_metadata("BTCUSDT".into());
    assert!(
        sent(&calls(&wire), "GET", LEVERAGE),
        "native request starts without polling metadata waiter"
    );
    rt.pump();
    assert!(
        !store.fetched_leverage().contains_key("BTCUSDT"),
        "held response fences continuation"
    );
    drop(waiter);
    wire.release_responses();
    rt.pump();
    assert_eq!(store.markets()[0].limits.leverage.max, 60.0);
    let fetched = store.fetched_leverage()["BTCUSDT"].clone();
    assert_eq!(
        (fetched.buy_leverage, fetched.sell_leverage),
        (31.0, 12.0),
        "dropped waiter retains native continuation"
    );
    assert!(calls(&wire).is_empty(), "no replay");
    script(&wire, LEVERAGE, json!([]));
    let refreshed = drive(&rt, venue.fetch_leverage_for_symbol("BTCUSDT".into()))
        .unwrap()
        .unwrap();
    assert_eq!(
        (refreshed.buy_leverage, refreshed.sell_leverage),
        (31.0, 12.0),
        "continuation updates the shared settings, not a snapshot"
    );
    assert_eq!(calls(&wire).len(), 1, "only the explicit follow-up read");
}

#[test]
fn eager_unit_cancel_commits_after_held_response_and_waiter_drop() {
    let (venue, rt, store, wire) = support::fixture();
    load_market(&venue, &rt, &wire, &store);
    let orders = drive(&rt, venue.fetch_regular_orders(None)).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(orders),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    calls(&wire);
    wire.hold_responses.set(true);
    let waiter = (&*venue as &dyn Exchange).cancel_all_orders();
    rt.pump();
    assert!(
        sent(&calls(&wire), "POST", CANCEL_ALL),
        "assembly unit starts before waiter polling"
    );
    assert_eq!(store.orders()[0].status, safe_cex_types::OrderStatus::Open);
    drop(waiter);
    wire.release_responses();
    rt.pump();
    assert_eq!(
        store.orders()[0].status,
        safe_cex_types::OrderStatus::Canceled
    );
    assert!(calls(&wire).is_empty());
}

#[test]
fn eager_typed_update_continues_cancel_and_replace_after_waiter_drop() {
    let (venue, rt, store, wire) = support::fixture();
    load_market(&venue, &rt, &wire, &store);
    let order = drive(&rt, venue.fetch_regular_orders(None))
        .unwrap()
        .remove(0);
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(vec![order.clone()]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    calls(&wire);
    wire.hold_responses.set(true);
    let waiter = (&*venue as &dyn Exchange).update_order(UpdateOrderOpts {
        order,
        update: serde_json::from_value(json!({"price":59100})).unwrap(),
    });
    rt.pump();
    assert!(
        sent(&calls(&wire), "POST", CANCEL),
        "assembly typed starts cancellation before waiter polling"
    );
    drop(waiter);
    wire.release_responses();
    for _ in 0..8 {
        rt.pump();
        rt.now.set(rt.now.get() + 1000.0);
    }
    let log = calls(&wire);
    assert!(
        sent(&log, "POST", SUBMIT),
        "typed continuation still places replacement"
    );
    assert_eq!(bodies(&log, "POST", SUBMIT)[0]["price"], json!("59100"));
    assert!(!sent(&log, "POST", CANCEL), "cancel not replayed");
}

#[test]
fn eager_wrappers_match_maintained_source_prefix_and_detached_work() {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/eager-source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let observed: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(observed["members"].as_array().unwrap().len(), 4);
    assert_eq!(observed["publicNotifications"], 1);
    for name in ["detachedPrivate", "detachedUnit", "detachedTyped"] {
        assert_eq!(observed[name], true, "{name}");
    }
}

#[test]
fn eager_metadata_ready_http_still_fences_throwing_store_continuation() {
    let (venue, rt, store, wire) = support::fixture();
    load_market(&venue, &rt, &wire, &store);
    script(
        &wire,
        LEVERAGE,
        json!([
            {"positionType":1,"leverage":31,"maxLeverageView":80,"openType":2},
            {"positionType":2,"leverage":12,"maxLeverage":60,"openType":1}
        ]),
    );
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let _first = Store::subscribe(
        store.as_ref(),
        Box::new(move |data, _| {
            seen.set(seen.get() + 1);
            if seen.get() == 1 {
                assert_eq!(data.markets[0].limits.leverage.max, 60.0);
                assert!(!data.fetched_leverage.contains_key("BTCUSDT"));
            } else {
                assert_eq!(data.fetched_leverage["BTCUSDT"].buy_leverage, 20.0);
            }
            Err(safe_cex_types::StoreSubscriberError::new(format!(
                "subscriber {}",
                seen.get()
            )))
        }),
    );
    let later = Rc::new(Cell::new(0));
    let seen = later.clone();
    let _later = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    let waiter = (&*venue as &dyn Exchange).update_symbol_metadata("BTCUSDT".into());
    assert_eq!(
        count.get(),
        0,
        "ready HTTP does not collapse source await checkpoint"
    );
    assert!(sent(&calls(&wire), "GET", LEVERAGE));
    let error = drive(&rt, waiter).unwrap_err();
    assert_eq!(
        error.message, "subscriber 2",
        "fallback preserves final subscriber failure"
    );
    assert_eq!(count.get(), 2);
    assert_eq!(later.get(), 0, "throw aborts each notification fanout");
    assert_eq!(store.fetched_leverage()["BTCUSDT"].sell_leverage, 20.0);
}
