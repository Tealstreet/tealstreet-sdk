#![cfg(not(target_arch = "wasm32"))]

#[allow(dead_code)]
#[path = "support/accepted_store_harness.rs"]
mod fixture;

use safe_cex_store::Store;
use std::{cell::Cell, rc::Rc};

#[test]
fn cache_notification_precedes_fresh_position_lookup() {
    let store = fixture::store(vec![fixture::market("btc_perp", "BTC_USDT_PERP")]);
    let events = fixture::events();
    let remove = fixture::watch(
        &store,
        events.clone(),
        false,
        true,
        fixture::error("NotifyError", "notify failed"),
    );
    let accounts = Rc::new(Cell::new(0u32));
    let account_reads = accounts.clone();
    let future = safe_cex_venue_grvt::data_reads::set_leverage(
        store.as_ref(),
        "BTC_USDT_PERP",
        12.5,
        true,
        move || {
            let next = account_reads.get() + 1;
            account_reads.set(next);
            Some(format!("sub-{next}"))
        },
        || std::future::ready(Ok(serde_json::json!("endpoint-A"))),
        |payload, endpoint| {
            assert_eq!(payload["sub_account_id"], "sub-2", "second account read");
            assert_eq!(payload["instrument"], "btc_perp", "original native ID");
            assert_eq!(payload["leverage"], "12.5", "native request text");
            assert_eq!(endpoint, "endpoint-A", "authenticated endpoint");
            std::future::ready(Ok(serde_json::json!({"s": true})))
        },
        |_, _| Ok(()),
    );
    assert_eq!(accounts.get(), 2, "both account reads precede polling");
    assert!(
        fixture::observed(&events).is_empty(),
        "cache still unchanged"
    );
    fixture::ready(future).expect("configured setter succeeds");

    let positions = Store::positions(store.as_ref()).unwrap();
    assert_eq!(positions.len(), 1, "subscriber replaced the original array");
    assert_eq!(positions[0].id, "live", "fresh native position identity");
    assert_eq!(positions[0].leverage.to_bits(), 12.5f64.to_bits());
    let cache = Store::fetched_leverage(store.as_ref()).unwrap();
    let cached = &cache["BTC_USDT_PERP"];
    assert_eq!(cached.buy_leverage.to_bits(), 12.5f64.to_bits());
    assert_eq!(cached.sell_leverage.to_bits(), 12.5f64.to_bits());
    assert_eq!(cache["OLD"].buy_leverage.to_bits(), 7.0f64.to_bits());
    let observed = fixture::observed(&events);
    assert_eq!(
        observed.len(),
        3,
        "cache, reentrant replacement, live patch"
    );
    assert_eq!(
        observed
            .iter()
            .map(|row| row[2].as_bool().unwrap())
            .collect::<Vec<_>>(),
        [false, true, false],
        "native worker-origin flags"
    );
    remove();
}
