use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::Store;
use safe_cex_store::{
    StoreUpdate,
    model_patch::{ModelPatch, PatchSource},
};
use serde_json::{Map, Value, json};

#[test]
fn native_info_patch_retains_reserved_key_object() {
    let mut cases = 0;
    for native in [false, true] {
        for operation in ["order", "position", "positions", "market"] {
            if operation == "positions" && !native {
                continue;
            }
            let store = DefaultStore::new();
            let seed = safe_cex_store::store_stub::STUB_STORE.clone();
            store
                .update_full(
                    StoreUpdate {
                        orders: Some(seed.orders.clone()),
                        positions: Some(seed.positions.clone()),
                        markets: Some(seed.markets.clone()),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            let info = Value::Object(Map::from_iter([(
                "$serde_json::private::Number".into(),
                Value::String("17".into()),
            )]));
            let fields = Map::from_iter([("info".into(), info.clone())]);
            let model = ModelPatch::from(fields.clone());
            let patch: &dyn PatchSource = if native { &model } else { &fields };
            let actual = match operation {
                "order" => {
                    store
                        .update_order(&seed.orders[0].id, patch)
                        .expect("STORE-INFO native order patch accepted");
                    assert_eq!(
                        store.orders()[0].price.to_number().to_bits(),
                        seed.orders[0].price.to_number().to_bits(),
                        "STORE-INFO order sibling"
                    );
                    store.orders()[0].info.clone().into_value()
                }
                "position" => {
                    store
                        .update_position(&seed.positions[0].id, patch)
                        .expect("STORE-INFO native position patch accepted");
                    assert_eq!(
                        store.positions()[0].entry_price.to_bits(),
                        seed.positions[0].entry_price.to_bits(),
                        "STORE-INFO position sibling"
                    );
                    store.positions()[0].info.clone().into_value()
                }
                "positions" => {
                    store
                        .update_positions(&[(Some(seed.positions[0].id.clone()), model)])
                        .expect("STORE-INFO native batch patch accepted");
                    assert_eq!(
                        store.positions()[0].entry_price.to_bits(),
                        seed.positions[0].entry_price.to_bits(),
                        "STORE-INFO batch sibling"
                    );
                    store.positions()[0].info.clone().into_value()
                }
                "market" => {
                    store
                        .update_market(&seed.markets[0].id, patch)
                        .expect("STORE-INFO native market patch accepted");
                    assert_eq!(
                        store.markets()[0].symbol,
                        seed.markets[0].symbol,
                        "STORE-INFO market sibling"
                    );
                    store.markets()[0].info.clone()
                }
                _ => unreachable!(),
            }
            .expect("STORE-INFO native info present");
            assert!(
                actual.is_object(),
                "STORE-INFO native object retained: {operation}/{native}"
            );
            assert_eq!(
                actual.as_object().unwrap().len(),
                1,
                "STORE-INFO exact key population"
            );
            assert_eq!(
                actual.get("$serde_json::private::Number").unwrap(),
                &safe_cex_types::retained_json::RetainedJsonInput::from(json!("17")),
                "STORE-INFO literal key value"
            );
            cases += 1;
        }
    }
    assert_eq!(
        cases, 7,
        "STORE-INFO independent operation/carrier case floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-store/tests/patch_info_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "STORE-INFO actual source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
