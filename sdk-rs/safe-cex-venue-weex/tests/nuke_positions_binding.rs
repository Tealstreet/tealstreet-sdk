//! Construct the production class/Factory; only native HTTP and runtime I/O are scripted.
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, Position};
use serde_json::json;
#[allow(dead_code)]
mod io {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_runtime.rs"
    ));
}
fn nuke_replacement(empty: bool) {
    let endpoint = |name| {
        safe_cex_venue_weex::types::ENDPOINTS
            .iter()
            .find(|(key, _)| *key == name)
            .unwrap()
            .1
    };
    let h = io::harness(io::options(), |http| {
        http.ok("POST", endpoint("CLOSE_POSITIONS"), json!({}));
        http.ok("GET", endpoint("POSITIONS"), if empty { json!([]) } else {
            json!([{"symbol":"BTCUSDT","positionSide":"LONG","size":"2","openValue":"200","leverage":"20","marginType":"CROSS","updatedTime":1700000000000.0}])
        });
    });
    let market: Market = serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":50},"leverage":{"min":1,"max":80}}})).unwrap();
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let expected: Vec<Position> = h.drive(h.venue().fetch_positions(None)).unwrap();
    assert!(
        !expected.is_empty(),
        "source fills absent symbols with fake zero positions"
    );
    let mut stale = expected[0].clone();
    stale.id = "retired-hedged-position".into();
    stale.contracts = 7.0;
    let mut old = expected.clone();
    old[0].contracts = 9.0;
    old.insert(0, stale);
    Store::update(
        h.store.as_ref(),
        StoreUpdate {
            positions: Some(old),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    h.drive(h.venue().nuke_positions(0)).unwrap();
    assert_eq!(h.http.calls(endpoint("CLOSE_POSITIONS")).len(), 1);
    assert_eq!(h.http.calls(endpoint("POSITIONS")).len(), 2);
    assert!(
        !h.store
            .positions()
            .unwrap()
            .iter()
            .any(|p| p.id == "retired-hedged-position"),
        "source Store.update replaces the snapshot"
    );
    assert_eq!(
        &*h.store.positions().unwrap(),
        expected.as_slice(),
        "exact refetched snapshot, including fake zero rows"
    );
}
#[test]
fn nuke_positions_replaces_snapshot_through_constructed_binding() {
    nuke_replacement(false);
}
#[test]
fn nuke_positions_replaces_snapshot_after_empty_rest_response() {
    nuke_replacement(true);
}
