use safe_cex_num::js_number_string;
use safe_cex_types::{OrderBook, OrderBookOrders};
use safe_cex_venue_binance::orderbook::{
    BinanceOrderBookSyncState, load_binance_order_book_snapshot,
    reset_binance_order_book_sync_state,
};
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn snapshot_and_reset_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/orderbook.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 3132, "SOURCE-CARDINALITY orderbook total");
    fn number(n: f64) -> Value {
        json!({"value":js_number_string(n),"negativeZero":n==0.0 && n.is_sign_negative()})
    }
    fn levels(rows: &[OrderBookOrders]) -> Value {
        rows.iter().map(|r|json!({"price":number(r.price),"amount":number(r.amount),"total":number(r.total)})).collect()
    }
    let mut completed_comparisons = 0;
    for (index, row) in rows.iter().enumerate() {
        let mut book = OrderBook {
            bids: vec![OrderBookOrders {
                price: 9.0,
                amount: 9.0,
                total: 9.0,
            }],
            asks: vec![OrderBookOrders {
                price: 8.0,
                amount: 8.0,
                total: 8.0,
            }],
        };
        let mut state = BinanceOrderBookSyncState {
            snapshot_last_update_id: Some(1.0),
            last_applied_update_id: Some(99.0),
        };
        let bids: Vec<Vec<String>> =
            serde_json::from_value(row["snapshot"]["bids"].clone()).unwrap();
        let asks: Vec<Vec<String>> =
            serde_json::from_value(row["snapshot"]["asks"].clone()).unwrap();
        let mut dirty = state.clone();
        reset_binance_order_book_sync_state(&mut dirty);
        assert_eq!(dirty, BinanceOrderBookSyncState::default());
        assert_eq!(row["resetDirty"], json!({}));
        load_binance_order_book_snapshot(
            &mut book,
            &bids,
            &asks,
            row["snapshot"]["lastUpdateId"].as_f64().unwrap(),
            &mut state,
        );
        assert_eq!(
            json!({"bids":levels(&book.bids),"asks":levels(&book.asks)}),
            row["expected"]["book"],
            "snapshot {index}"
        );
        assert_eq!(
            state.snapshot_last_update_id,
            Some(
                row["expected"]["state"]["snapshotLastUpdateId"]
                    .as_f64()
                    .unwrap()
            )
        );
        assert_eq!(state.last_applied_update_id, None);
        reset_binance_order_book_sync_state(&mut state);
        assert_eq!(state, BinanceOrderBookSyncState::default());
        assert_eq!(row["reset"], json!({}));
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 3132,
        "NATIVE-CARDINALITY orderbook completed comparisons"
    );
    eprintln!("{} live snapshot/reset cases", rows.len());
}

#[test]
fn snapshot_ids_retain_nonfinite_and_signed_zero_and_reset_independent_state() {
    // Captured on darwin-arm64. NaN identity is semantic; its CPU payload is not.
    // Preserve every bit for non-NaN values, especially signed zero.
    fn semantic_bits(value: f64) -> u64 {
        if value.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            value.to_bits()
        }
    }
    assert_eq!(
        semantic_bits(f64::from_bits(0xfff8_0000_0000_0000)),
        0x7ff8_0000_0000_0000,
        "NaN sign canonical capture"
    );
    assert_eq!(
        semantic_bits(f64::from_bits(0x7ff8_0000_0000_0042)),
        0x7ff8_0000_0000_0000,
        "NaN payload canonical capture"
    );
    assert_eq!(
        semantic_bits(-0.0),
        0x8000_0000_0000_0000,
        "signed-zero capture retains minus zero"
    );
    assert_eq!(
        semantic_bits(1.5),
        0x3ff8_0000_0000_0000,
        "finite capture retains payload"
    );
    for id in [
        f64::NAN,
        f64::from_bits(0xfff8_0000_0000_0042),
        f64::INFINITY,
        f64::NEG_INFINITY,
        -0.0,
        0.0,
        1.5,
    ] {
        let mut book = OrderBook {
            bids: vec![],
            asks: vec![],
        };
        let mut state = BinanceOrderBookSyncState {
            snapshot_last_update_id: Some(id),
            last_applied_update_id: Some(id),
        };
        reset_binance_order_book_sync_state(&mut state);
        assert_eq!(state, BinanceOrderBookSyncState::default());
        state.last_applied_update_id = Some(id);
        reset_binance_order_book_sync_state(&mut state);
        assert_eq!(state, BinanceOrderBookSyncState::default());
        load_binance_order_book_snapshot(&mut book, &[], &[], id, &mut state);
        assert_eq!(
            semantic_bits(state.snapshot_last_update_id.unwrap()),
            semantic_bits(id)
        );
        assert_eq!(state.last_applied_update_id, None);
    }
}
