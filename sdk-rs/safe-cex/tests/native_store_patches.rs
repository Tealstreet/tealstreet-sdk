//! Production wiring: JSON is not the in-memory numeric patch representation.
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch, store::DefaultStore};
use safe_cex_types::{ModelField, PositionExtra, StoreData};
use serde_json::json;
use std::rc::Rc;
fn seed() -> StoreData {
    safe_cex_store::store_stub::STUB_STORE.clone()
}
fn same_number(actual: f64, expected: f64) {
    if expected.is_nan() {
        assert!(actual.is_nan());
    } else {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
}
#[test]
fn unrelated_position_patch_preserves_every_existing_nonfinite_field() {
    for price in [0.0, -0.0, 1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let real = Rc::new(DefaultStore::new());
        let host: Rc<dyn Store> = real.clone();
        let mut position = seed().positions.remove(0);
        position.id = "p".into();
        position.entry_price = price;
        position.extra = ModelField::Value(PositionExtra {
            contracts_native: Some(f64::NAN),
            unrealized_pnl_native: Some(f64::NEG_INFINITY),
        });
        host.add_or_update_position(position.clone())
            .expect("fixture Store mutation must succeed");
        same_number(host.positions()[0].entry_price, price);
        host.update_position("p", json!({"contracts":2}).as_object().unwrap())
            .unwrap();
        let rows = host.positions();
        same_number(rows[0].entry_price, price);
        assert_eq!(rows[0].contracts, 2.0);
        assert!(
            rows[0]
                .extra
                .as_ref()
                .unwrap()
                .contracts_native
                .unwrap()
                .is_nan()
        );
        drop(rows);
        position.contracts = 3.0;
        host.add_or_update_position(position.clone())
            .expect("fixture Store mutation must succeed");
        same_number(host.positions()[0].entry_price, price);
        host.add_or_update_positions(vec![position])
            .expect("fixture Store mutation must succeed");
        same_number(host.positions()[0].entry_price, price);
        let mut patch = ModelPatch::default();
        patch.set_number("entryPrice", price);
        patch.set_native(
            "extra",
            PositionExtra {
                contracts_native: Some(f64::INFINITY),
                unrealized_pnl_native: Some(f64::NAN),
            },
        );
        host.update_positions(&[(Some("p".into()), patch)]).unwrap();
        same_number(host.positions()[0].entry_price, price);
        assert!(
            host.positions()[0]
                .extra
                .as_ref()
                .unwrap()
                .unrealized_pnl_native
                .unwrap()
                .is_nan()
        );
    }
}
#[test]
fn ticker_and_bbo_native_updates_overwrite_old_prices_and_preserve_absence() {
    for value in [0.0, -0.0, 1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let real = Rc::new(DefaultStore::new());
        let host: Rc<dyn Store> = real.clone();
        let mut ticker = seed().tickers.remove(0);
        ticker.id = "t".into();
        ticker.symbol = "BTC".into();
        ticker.last = 100.0;
        ticker.mark = 99.0;
        host.update(
            StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation must succeed");
        let mut patch = ModelPatch::default();
        for field in ["last", "bid", "ask", "midPrice"] {
            patch.set_number(field, value);
        }
        host.update_ticker("t", &patch).unwrap();
        let ticks = host.tickers();
        for actual in [
            ticks[0].last,
            ticks[0].bid,
            ticks[0].ask,
            ticks[0].mid_price.unwrap(),
        ] {
            same_number(actual, value);
        }
        assert_eq!(ticks[0].mark, 99.0);
        drop(ticks);
        same_number(real.tickers_by_id()["t"].last, value);
        same_number(real.tickers_by_symbol()["BTC"].bid, value);
        let mut only_bid = ModelPatch::default();
        only_bid.set_number("bid", 7.0);
        host.update_ticker("t", &only_bid).unwrap();
        same_number(host.tickers()[0].last, value);
        let mut delta = ModelPatch::default();
        delta.set_number("ask", value);
        real.apply_ticker_deltas(&[(0.0, delta.clone())], true)
            .unwrap();
        same_number(host.tickers()[0].ask, value);
        let packed = DefaultStore::pack_ticker_deltas(&[(0.0, delta)]).unwrap();
        same_number(packed.packed[0].values[0], value);
        host.apply_packed_ticker_deltas(packed.packed, true)
            .expect("fixture Store mutation must succeed");
        same_number(host.tickers()[0].ask, value);
    }
}
#[test]
fn numeric_null_and_wrong_native_types_return_errors_instead_of_sanitizing() {
    let real = DefaultStore::new();
    let mut ticker = seed().tickers.remove(0);
    ticker.id = "t".into();
    real.update_full(
        StoreUpdate {
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .expect("fixture Store mutation must succeed");
    let error = real
        .update_ticker("t", json!({"last":null}).as_object().unwrap())
        .unwrap_err();
    let safe_cex_store::StoreMutationError::Patch(error) = error else {
        panic!("PATCH-IDENTITY numeric null must retain Patch variant");
    };
    assert_eq!(error.field(), "last");
    assert_eq!(error.message(), "expected number");
    let mut wrong = ModelPatch::default();
    wrong.set_native("last", "junk".to_owned());
    assert!(real.update_ticker("t", &wrong).is_err());
    assert!(serde_json::from_value::<ModelPatch>(json!([])).is_err());
}

#[test]
#[ignore = "executes actual TypeScript Store with the isolated compiler"]
fn native_patch_source_witness() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/native_patch_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        for field in ["entry", "nested", "last", "retained", "lastAlias"] {
            assert_eq!(row[field], row["input"], "source {field}");
        }
        assert_eq!(row["contracts"], 2);
        assert_eq!(row["bidAlias"], "7");
        assert_eq!(row["mark"], 99);
    }
    let control = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/native_patch_oracle.mjs"
        ))
        .arg("--negative-source-control")
        .output()
        .unwrap();
    assert!(
        !control.status.success(),
        "source mutation control unexpectedly passed"
    );
    assert!(String::from_utf8_lossy(&control.stderr).contains("source numeric patch must execute"));
}

#[test]
fn order_market_and_balance_do_not_roundtrip_existing_nonfinite_values() {
    for value in [0.0, -0.0, 1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let real = DefaultStore::new();
        let mut order = seed().orders.remove(0);
        order.id = "o".into();
        order.price = value.into();
        real.add_or_update_order(order)
            .expect("fixture Store mutation must succeed");
        real.update_order("o", json!({"amount":2}).as_object().unwrap())
            .unwrap();
        same_number(real.orders()[0].price.to_number(), value);
        let mut market = seed().markets.remove(0);
        market.id = "m".into();
        market.precision.price = value;
        let mut balance = seed().balance;
        balance.total = value;
        real.update_full(
            StoreUpdate {
                markets: Some(vec![market]),
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation must succeed");
        real.update_market("m", json!({"quote":"USDC"}).as_object().unwrap())
            .unwrap();
        same_number(real.markets()[0].precision.price, value);
        same_number(real.balance().total, value);
        let mut patch = ModelPatch::default();
        patch.set_native(
            "precision",
            safe_cex_types::MarketPrecision {
                amount: value,
                price: value,
            },
        );
        real.update_market("m", &patch).unwrap();
        same_number(real.markets()[0].precision.amount, value);
        same_number(real.markets_by_id()["m"].precision.price, value);
    }
}

#[test]
fn empty_and_info_only_deltas_preserve_duplicate_id_alias_owner() {
    let real = DefaultStore::new();
    let mut first = seed().tickers.remove(0);
    first.id = "duplicate".into();
    first.symbol = "A".into();
    let mut second = first.clone();
    second.symbol = "B".into();
    real.update_full(
        StoreUpdate {
            tickers: Some(vec![first, second]),
            ..Default::default()
        },
        false,
    )
    .expect("fixture Store mutation must succeed");
    for patch in [json!({}), json!({"info":{"discard":true}})] {
        real.apply_ticker_deltas(&[(0.0, patch.as_object().unwrap().clone())], true)
            .unwrap();
        assert_eq!(real.tickers_by_id()["duplicate"].symbol, "B");
    }
}

#[test]
fn invalid_later_fields_commit_no_state_aliases_counters_or_notifications() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    let real = DefaultStore::new();
    let mut data = seed();
    data.tickers[0].id = "t".into();
    data.tickers[0].symbol = "BTC".into();
    data.tickers[0].last = 100.0;
    data.orders[0].id = "o".into();
    data.orders[0].price = 100.0.into();
    data.markets[0].id = "m".into();
    data.markets[0].quote = "USDT".into();
    let mut position = data.positions[0].clone();
    data.positions[0].id = "p1".into();
    data.positions[0].symbol = "A".into();
    position.id = "p2".into();
    position.symbol = "B".into();
    data.positions.push(position);
    real.update_full(
        StoreUpdate {
            tickers: Some(data.tickers),
            markets: Some(data.markets),
            orders: Some(data.orders),
            positions: Some(data.positions),
            ..Default::default()
        },
        false,
    )
    .expect("fixture Store mutation must succeed");
    let notifications = Arc::new(AtomicUsize::new(0));
    let counter = notifications.clone();
    let _subscription = real.subscribe(Arc::new(move |_, _| {
        counter.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }));
    let before = real.update_ids();
    let positions = real.positions().to_vec();
    let markets_version = real.get_markets_version();
    assert!(
        real.update_ticker("t", json!({"last":7,"bid":null}).as_object().unwrap())
            .is_err()
    );
    assert_eq!(real.tickers()[0].last, 100.0);
    assert_eq!(real.tickers_by_id()["t"].last, 100.0);
    assert!(
        real.update_order("o", json!({"price":7,"amount":null}).as_object().unwrap())
            .is_err()
    );
    assert_eq!(real.orders()[0].price, 100.0);
    assert!(
        real.update_market(
            "m",
            json!({"quote":"USDC","precision":null})
                .as_object()
                .unwrap()
        )
        .is_err()
    );
    assert_eq!(real.markets()[0].quote, "USDT");
    assert_eq!(real.markets_by_id()["m"].quote, "USDT");
    assert_eq!(real.get_markets_version(), markets_version);
    assert!(
        real.update_positions(&[
            (
                Some("p1".into()),
                json!({"contracts":7}).as_object().unwrap().clone().into()
            ),
            (
                Some("p2".into()),
                json!({"entryPrice":null})
                    .as_object()
                    .unwrap()
                    .clone()
                    .into()
            )
        ])
        .is_err()
    );
    assert_eq!(*real.positions(), positions);
    assert!(
        real.apply_ticker_deltas(
            &[
                (0.0, json!({"last":7}).as_object().unwrap().clone()),
                (0.0, json!({"bid":null}).as_object().unwrap().clone())
            ],
            true
        )
        .is_err()
    );
    assert_eq!(real.tickers()[0].last, 100.0);
    assert_eq!(real.tickers_by_id()["t"].last, 100.0);
    assert_eq!(real.update_ids(), before);
    assert_eq!(notifications.load(Ordering::Relaxed), 0);
}

#[test]
fn malformed_indices_on_empty_deltas_are_noops() {
    let real = DefaultStore::new();
    real.update_full(
        StoreUpdate {
            tickers: Some(seed().tickers),
            ..Default::default()
        },
        false,
    )
    .expect("fixture Store mutation must succeed");
    for index in [0.5, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for patch in [json!({}), json!({"info":{}})] {
            real.apply_ticker_deltas(&[(index, patch.as_object().unwrap().clone())], true)
                .unwrap();
        }
    }
}

#[test]
fn malformed_nonempty_delta_returns_error_without_mutation() {
    let real = DefaultStore::new();
    real.update_full(
        StoreUpdate {
            tickers: Some(seed().tickers),
            ..Default::default()
        },
        false,
    )
    .expect("fixture Store mutation must succeed");
    for index in [0.5, f64::NAN, -0.5] {
        let error = real
            .apply_ticker_deltas(
                &[(index, json!({"last":7}).as_object().unwrap().clone())],
                true,
            )
            .unwrap_err();
        let safe_cex_store::StoreMutationError::Patch(error) = error else {
            panic!("PATCH-IDENTITY invalid index must retain Patch variant");
        };
        assert_eq!(error.field(), "$index");
    }
}
