use safe_cex_store::{Store, StoreUpdate};
use serde_json::json;
#[test]
fn snapshot_health_flags_and_leverage_keep_native_identity() {
    use safe_cex_exchange::native_store::DefaultStore;
    let flags = [
        "publicHealthy",
        "privateHealthy",
        "publicInitialized",
        "privateInitialized",
    ];
    for selected in flags {
        let real = DefaultStore::new();
        let mut health = serde_json::Map::new();
        for flag in flags {
            health.insert(flag.into(), json!(flag == selected));
        }
        let update: StoreUpdate=serde_json::from_value(json!({"socketHealth":health,"fetchedLeverage":{"identity":{"buyLeverage":11,"sellLeverage":23}}})).unwrap();
        Store::update(&real, update, false).unwrap();
        let actual = Store::socket_health(&real)
            .unwrap()
            .unwrap()
            .into_value()
            .unwrap();
        for flag in flags {
            assert_eq!(
                actual[flag],
                json!(flag == selected),
                "STORE-NATIVE one-hot socket flag {flag}"
            );
        }
        let leverage = real.fetched_leverage().unwrap();
        assert_eq!(
            (
                leverage["identity"].buy_leverage,
                leverage["identity"].sell_leverage
            ),
            (11.0, 23.0),
            "STORE-NATIVE leverage side identity"
        );
    }
}

#[test]
fn socket_health_retains_undefined_through_typed_notifications_and_payloads() {
    use safe_cex_exchange::{native_store::DefaultStore, store_payload::sanitize_for_post_message};
    use safe_cex_types::retained_json::RetainedJsonInput as Native;
    use std::{cell::RefCell, rc::Rc};
    let store = DefaultStore::new();
    let delivered = Rc::new(RefCell::new(Vec::new()));
    let capture = delivered.clone();
    let _subscription = Store::subscribe(
        &store,
        Box::new(move |data, _| {
            capture
                .borrow_mut()
                .push(data.socket_health.clone().unwrap());
            Ok(())
        }),
    );
    let field = |name: &str, value| (name.encode_utf16().collect(), value);
    Store::update_socket_health(
        &store,
        &Native::Object(vec![
            field("kept", Native::Bool(true)),
            field("privateCloseCode", Native::Float(1006.0)),
        ]),
    )
    .unwrap();
    Store::update_socket_health(
        &store,
        &Native::Object(vec![
            field("privateCloseCode", Native::Undefined),
            field("privateLastMessage", Native::Undefined),
            field("publicCloseCode", Native::Undefined),
        ]),
    )
    .unwrap();
    let snapshot = Store::serialize(&store).unwrap();
    let health = snapshot.socket_health.as_ref().unwrap();
    assert_eq!(health.get("kept"), Some(&Native::Bool(true)));
    for key in ["privateCloseCode", "privateLastMessage", "publicCloseCode"] {
        assert_eq!(
            health.get(key),
            Some(&Native::Undefined),
            "snapshot owns {key}"
        );
    }
    assert_eq!(delivered.borrow().last(), Some(health));
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_typed_preserving_undefined::<Native>(
            &sanitize_for_post_message(
                &safe_cex_exchange::js_runtime::value::from_typed(&snapshot).unwrap(),
                false,
                false
            )
            .unwrap()
            .get("socketHealth")
            .unwrap()
        )
        .unwrap(),
        *health
    );
    assert_eq!(
        serde_json::to_value(health).unwrap(),
        json!({
            "publicHealthy":false,"privateHealthy":false,
            "publicInitialized":false,"privateInitialized":false,"kept":true
        })
    );
    let names: Vec<_> = health
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, _)| String::from_utf16(name).unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "publicHealthy",
            "privateHealthy",
            "publicInitialized",
            "privateInitialized",
            "kept",
            "privateCloseCode",
            "privateLastMessage",
            "publicCloseCode"
        ]
    );
}
