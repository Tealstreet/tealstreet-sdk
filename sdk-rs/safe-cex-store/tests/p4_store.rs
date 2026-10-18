use safe_cex_store::{StoreUpdate, store::DefaultStore};
use serde_json::{Map, Value, json};
use std::sync::{Arc, Mutex};
fn equal(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            assert_eq!(a.as_f64().unwrap().to_bits(), b.as_f64().unwrap().to_bits())
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                equal(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<std::collections::BTreeSet<_>>(),
                b.keys().collect()
            );
            for (key, value) in a {
                equal(value, &b[key]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
fn map(value: &Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}
#[test]
#[ignore = "runs current TypeScript full-store source"]
fn full_store_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_store_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 49);
    equal(
        &serde_json::to_value(&*safe_cex_store::store_stub::STUB_STORE).unwrap(),
        &rows[0]["stubStore"],
    );
    let store = DefaultStore::new();
    let notifications = Arc::new(Mutex::new(0_u64));
    let count = notifications.clone();
    let _remove = store.subscribe(Arc::new(move |_, _| {
        *count.lock().unwrap() += 1;
        Ok(())
    }));
    for row in rows {
        let args = &row["args"];
        let mut result = Value::Null;
        match row["op"].as_str().unwrap() {
            "update" => store
                .update_full(
                    serde_json::from_value::<StoreUpdate>(args[0].clone()).unwrap(),
                    args.get(1).and_then(Value::as_bool).unwrap_or(false),
                )
                .unwrap(),
            "updateTicker" => store
                .update_ticker(args[0]["id"].as_str().unwrap(), &map(&args[1]))
                .unwrap(),
            "updateMarket" => store
                .update_market(args[0]["id"].as_str().unwrap(), &map(&args[1]))
                .unwrap(),
            "applyTickerDeltas" => store
                .apply_ticker_deltas(
                    &args[0]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| (v[0].as_f64().unwrap(), map(&v[1])))
                        .collect::<Vec<_>>(),
                    args.get(1).and_then(Value::as_bool).unwrap_or(false),
                )
                .unwrap(),
            "packTickerDeltas" => {
                result = serde_json::to_value(
                    DefaultStore::pack_ticker_deltas(
                        &args[0]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|v| (v[0].as_f64().unwrap(), map(&v[1])))
                            .collect::<Vec<_>>(),
                    )
                    .unwrap(),
                )
                .unwrap()
            }
            "applyPackedTickerDeltas" => store
                .apply_packed_ticker_deltas(
                    &serde_json::from_value::<Vec<_>>(args[0].clone()).unwrap(),
                    args.get(1).and_then(Value::as_bool).unwrap_or(false),
                )
                .unwrap(),
            "consumeTickerDeltas" => {
                result = serde_json::to_value(store.consume_ticker_deltas()).unwrap()
            }
            "updatePosition" => store
                .update_position(args[0]["id"].as_str().unwrap(), &map(&args[1]))
                .unwrap(),
            "addOrUpdatePosition" => store
                .add_or_update_position(serde_json::from_value(args[0].clone()).unwrap())
                .unwrap(),
            "addOrUpdatePositions" => store
                .add_or_update_positions(serde_json::from_value(args[0].clone()).unwrap())
                .unwrap(),
            "updatePositions" => store
                .update_positions(
                    &args[0]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| {
                            (
                                Some(v[0]["id"].as_str().unwrap().to_owned()),
                                map(&v[1]).into(),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .unwrap(),
            "addOrUpdatePositionsFull" => store
                .add_or_update_positions_full(serde_json::from_value(args[0].clone()).unwrap())
                .unwrap(),
            "setSetting" => store
                .set_setting(args[0].as_str().unwrap(), args[1].as_bool().unwrap())
                .unwrap(),
            "updateStarkConfig" => store.update_stark_config(Some(args[0].clone())).unwrap(),
            "updateZkLinkConfig" => store
                .update_zk_link_config(Some(args[0].clone().into()))
                .unwrap(),
            "updateUserData" => store.update_user_data(Some(args[0].clone())).unwrap(),
            "reset" => store.reset().unwrap(),
            op => panic!("{op}"),
        }
        let actual = json!({"snapshot":store.serialize(),"marketsVersion":store.get_markets_version(),"tickersVersion":store.get_tickers_version(),"tickersById":&*store.tickers_by_id(),"tickersBySymbol":&*store.tickers_by_symbol(),"marketsById":&*store.markets_by_id(),"marketsBySymbol":&*store.markets_by_symbol(),"userData":store.user_data().as_deref(),"zkLinkConfig":store.zk_link_config().as_deref(),"notifications":*notifications.lock().unwrap(),"result":result});
        let compare = std::panic::catch_unwind(|| equal(&actual, &row["expected"]));
        assert!(
            compare.is_ok(),
            "frame {}\nactual {actual}\nexpected {}",
            row["op"],
            row["expected"]
        );
    }
}

#[test]
fn shared_store_callbacks_can_read_and_reenter_after_mutation() {
    use safe_cex_store::Store;
    use std::cell::Cell;
    use std::rc::Rc;
    let concrete = Rc::new(DefaultStore::new());
    let store: Rc<dyn Store> = concrete.clone();
    let observed = Rc::new(Cell::new(0));
    let weak = Rc::downgrade(&store);
    let counter = observed.clone();
    let cleanup = store.subscribe(Box::new(move |state, from_worker| {
        assert!(!from_worker);
        let store = weak.upgrade().unwrap();
        assert_eq!(store.options().is_hedged, state.options.is_hedged);
        counter.set(counter.get() + 1);
        if state.options.is_hedged {
            store.set_setting("isHedged", false).unwrap();
        }
        Ok(())
    }));
    store.set_setting("isHedged", true).unwrap();
    assert_eq!(observed.get(), 2);
    assert!(!store.options().is_hedged);
    cleanup();
    store.set_setting("isHedged", true).unwrap();
    assert_eq!(observed.get(), 2);
}

#[test]
fn packed_masks_with_no_known_bits_do_not_dereference_an_invalid_index() {
    use safe_cex_store::PackedTickerDelta;
    let store = DefaultStore::new();
    for mask in [f64::NAN, 2048.0] {
        store
            .apply_packed_ticker_deltas(
                &[PackedTickerDelta {
                    index: f64::NAN,
                    mask,
                    values: vec![],
                }],
                false,
            )
            .unwrap();
    }
}

#[test]
fn populated_stub_is_lossless_through_the_shared_store_model() {
    let original: Value = serde_json::from_str(include_str!("../src/store_stub.json")).unwrap();
    equal(
        &serde_json::to_value(&*safe_cex_store::store_stub::STUB_STORE).unwrap(),
        &original,
    );
}
