//! Observe the actual bound placement consumer, not the standalone Result adapter.
use super::{HyperliquidDependencies, HyperliquidExchange, RuntimeClock};
use crate::binding_test_runtime as host;
use crate::{
    rate_limit::RateLimiter,
    realm::{PropKey, Realm, Value},
};
use safe_cex_exchange::native_store::DefaultStore;
use serde_json::{Value as Json, json};
use std::{
    cell::Cell,
    process::Command,
    rc::Rc,
    sync::{Arc, OnceLock},
};

fn source() -> &'static Json {
    static SOURCE: OnceLock<Json> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/order-error-audit.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

#[allow(clippy::arc_with_non_send_sync)]
fn fixture() -> (Rc<HyperliquidExchange>, Rc<host::Host>) {
    let runtime = Rc::new(host::Host::default());
    let exchange = HyperliquidExchange::new(
        serde_json::from_value(json!({"key":"","secret":"","applicationId":"0x1111111111111111111111111111111111111111"})).unwrap(),
        Rc::new(DefaultStore::new()),
        HyperliquidDependencies {
            runtime: runtime.clone(), http: host::Routes::new(vec![]),
            limiter: Rc::new(RateLimiter::new(Arc::new(RuntimeClock::new(runtime.clone())), 1200., 60_000.)),
            signers: Default::default(), external_public_data: None,
        },
    ).unwrap();
    (exchange, runtime)
}

fn profile(r: &mut Realm, error: &Value) -> Json {
    let class = r.import("HyperliquidUnreadableOrderResponseError").unwrap();
    let base = r.global_binding("Error").unwrap();
    let prototype = r.get_str(&class, "prototype").unwrap();
    let base_prototype = r.get_str(&base, "prototype").unwrap();
    let object = r.global_binding("Object").unwrap();
    let direct = r
        .call_method(&object, "getPrototypeOf", std::slice::from_ref(error))
        .unwrap();
    let parent = r
        .call_method(&object, "getPrototypeOf", std::slice::from_ref(&prototype))
        .unwrap();
    let class_parent = r
        .call_method(&object, "getPrototypeOf", std::slice::from_ref(&class))
        .unwrap();
    let descriptor = |key: &str| {
        let object = error.object().unwrap().borrow();
        object
            .props
            .iter()
            .find(|p| p.key.units() == Some(key.encode_utf16().collect::<Vec<_>>().as_slice()))
            .map(|p| json!([p.enumerable, p.writable, p.configurable]))
            .unwrap_or(Json::Null)
    };
    let name = r.get_str(error, "name").unwrap();
    let message = r.get_str(error, "message").unwrap();
    let constructor = r.get_str(error, "constructor").unwrap();
    let enumerable_keys: Vec<String> = r
        .own_keys(error, true)
        .unwrap()
        .iter()
        .map(|key| String::from_utf16(key.units().unwrap()).unwrap())
        .collect();
    json!({
        "name": r.to_string(&name).unwrap().as_ref(), "message": r.to_string(&message).unwrap().as_ref(),
        "display": r.to_string(error).unwrap().as_ref(),
        "isError": r.instance_of(error, &base).unwrap(), "isUnreadable": r.instance_of(error, &class).unwrap(),
        "directPrototype": direct.strict_equal(&prototype), "constructor": constructor.strict_equal(&class),
        "prototypeParent": parent.strict_equal(&base_prototype), "constructorParent": class_parent.strict_equal(&base),
        "nameDescriptor": descriptor("name"), "messageDescriptor": descriptor("message"), "enumerableKeys": enumerable_keys,
    })
}

#[test]
fn order_error_audit_bound_constructor_retains_error_inheritance_and_units() {
    let (exchange, _) = fixture();
    let mut r = exchange.binding.borrow().as_ref().unwrap().realm.clone();
    let class = r.import("HyperliquidUnreadableOrderResponseError").unwrap();
    assert_eq!(source()["constructors"].as_array().unwrap().len(), 4);
    for row in source()["constructors"].as_array().unwrap() {
        let units: Vec<u16> = serde_json::from_value(row["message"].clone()).unwrap();
        let error = r
            .construct(&class, &[Value::String(units.into())], None)
            .unwrap();
        assert_eq!(
            profile(&mut r, &error),
            row["observed"],
            "HL_ORDER_ERROR constructor inheritance/units"
        );
    }
}

#[test]
fn order_error_audit_real_response_consumer_reconciles_only_unreadable_errors() {
    let (exchange, _) = fixture();
    let binding = exchange.binding.borrow().as_ref().unwrap().clone();
    let mut r = binding.realm;
    let refreshes = Rc::new(Cell::new(0));
    let seen = refreshes.clone();
    let observer = r.native("observeOrderRefresh", 0, move |_, _, _| {
        seen.set(seen.get() + 1);
        Ok(Value::Undefined)
    });
    r.set_str(&binding.instance, "scheduleOrderRefresh", observer)
        .unwrap();
    assert_eq!(source()["cases"].as_array().unwrap().len(), 14);
    for row in source()["cases"].as_array().unwrap() {
        let before = refreshes.get();
        let input = if row["undefined"] == true {
            Value::Undefined
        } else {
            r.from_serde(&row["input"])
        };
        let result = r.call_method(
            &binding.instance,
            "validateResponseAndExtractOrderIds",
            &[input],
        );
        if let Some(expected) = row.get("error") {
            let error = result.expect_err("HL_ORDER_ERROR malformed response must reject");
            assert_eq!(
                profile(&mut r, &error),
                *expected,
                "HL_ORDER_ERROR actual bound parser/catch Error profile {row}"
            );
        } else {
            let ids = result.unwrap();
            assert_eq!(
                r.to_serde(&ids).unwrap(),
                row["ids"],
                "HL_ORDER_ERROR accepted response IDs"
            );
        }
        assert_eq!(
            refreshes.get() - before,
            row["refreshes"].as_u64().unwrap(),
            "HL_ORDER_ERROR reconcile unreadable once; venue refusal never {row}"
        );
    }
    assert_eq!(refreshes.get(), 9);
    assert!(
        r.has_own(&binding.instance, &PropKey::str("scheduleOrderRefresh"))
            .unwrap()
    );
}
