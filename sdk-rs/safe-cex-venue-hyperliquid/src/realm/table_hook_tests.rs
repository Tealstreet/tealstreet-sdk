use super::{Realm, facade_rest, test_runtime::TestRuntime};
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown},
    runtime::{ExchangeError, ExchangeErrorOrigin},
};
use std::{cell::Cell, fmt, rc::Rc};

#[test]
fn table_view_enumerates_native_keys_and_projects_only_its_reached_row() {
    use safe_cex_exchange::{js_runtime::value::from_json, native_store::DefaultStore};
    use safe_cex_types::{StoreReadError, StoreReadPhase};

    let store = Rc::new(DefaultStore::new());
    let native = store.facade().unwrap();
    let ticker = |symbol: &str| {
        from_json(&serde_json::json!({
            "id":symbol,"symbol":symbol,"bid":1,"ask":2,"last":3,"mark":4,"index":5,
            "percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0
        }))
    };
    let poison = ticker("POISON");
    native
        .call_method(
            "update",
            vec![Js::object(vec![(
                "tickers",
                Js::array(vec![ticker("SELECTED"), poison.clone()]),
            )])],
        )
        .unwrap();
    let cause = Js::object(vec![("code", Js::utf16(&[0xd800]))]);
    let thrown = cause.clone();
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    poison
        .define_accessor(
            "bid",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Err(Thrown::new(thrown.clone()))
            })),
            None,
        )
        .unwrap();
    let mut realm = Realm::new(Rc::new(TestRuntime::default()));
    let legacy = super::facade_store::store_object(&realm, store);
    let view = realm.get_str(&legacy, "tickersBySymbol").unwrap();
    assert_eq!(realm.own_keys(&view, true).unwrap().len(), 2);
    assert_eq!(reads.get(), 0, "key enumeration must not read DTO fields");
    let row = realm.get_str(&view, "SELECTED").unwrap();
    assert!(
        realm
            .get_str(&row, "bid")
            .unwrap()
            .strict_equal(&super::value::Value::Number(1.0))
    );
    assert_eq!(
        reads.get(),
        0,
        "selected projection must not touch an unrelated row"
    );
    let error = realm.get_str(&view, "POISON").unwrap_err();
    let recovered = realm.exchange_error(&error);
    let read = recovered
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<StoreReadError>()
        .unwrap();
    assert_eq!(read.phase(), StoreReadPhase::Projection);
    assert!(
        read.native_cause()
            .downcast_ref::<Thrown>()
            .unwrap()
            .value
            .same_reference(&cause)
    );
    assert_eq!(reads.get(), 1, "the reached getter runs exactly once");
}

#[derive(Debug)]
struct OriginalCause {
    value: Js,
    displays: Rc<Cell<usize>>,
}
impl fmt::Display for OriginalCause {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.displays.set(self.displays.get() + 1);
        f.write_str("opaque cause must not be formatted")
    }
}
impl std::error::Error for OriginalCause {}

#[test]
fn legacy_native_error_keeps_original_origin_without_reading_the_cause() {
    let mut realm = Realm::new(Rc::new(TestRuntime::default()));
    let value = Js::object(vec![]);
    value.define_key(&[0xd800], Js::Number(-0.0)).unwrap();
    let reads = Rc::new(Cell::new(0));
    let getter_reads = reads.clone();
    value
        .define_accessor(
            "message",
            Some(Rc::new(move |_| {
                getter_reads.set(getter_reads.get() + 1);
                Err(Thrown::new(Js::utf16(&[0xdc00])))
            })),
            None,
        )
        .unwrap();
    let weak = match &value {
        Js::Object(data) => Rc::downgrade(data),
        _ => unreachable!(),
    };
    let displays = Rc::new(Cell::new(0));
    let origin = ExchangeErrorOrigin::new(OriginalCause {
        value: value.clone(),
        displays: displays.clone(),
    });
    let mut failure = ExchangeError::new("Store property access failed");
    failure.origin = Some(origin.clone());
    let thrown = facade_rest::thrown_from(&mut realm, failure);
    let projection_reads = Rc::new(Cell::new(0));
    for key in ["message", "code", "response"] {
        let count = projection_reads.clone();
        let poison = realm.native("poison diagnostic", 0, move |_, _, _| {
            count.set(count.get() + 1);
            Err(super::value::Value::units(&[0xd800]))
        });
        realm.define_getter(&thrown, super::value::PropKey::str(key), poison);
    }
    let recovered = realm.exchange_error(&thrown);
    assert_eq!(
        recovered.origin.as_ref(),
        Some(&origin),
        "HL_STORE_ORIGIN preserve the original opaque failure across the actual Realm boundary"
    );
    let cause = recovered
        .origin
        .as_ref()
        .unwrap()
        .error()
        .downcast_ref::<OriginalCause>()
        .unwrap();
    assert!(cause.value.same_reference(&value));
    assert_eq!(
        cause
            .value
            .get_key(&[0xd800])
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(displays.get(), 0);
    assert_eq!(projection_reads.get(), 0);
    drop(recovered);
    drop(origin);
    drop(value);
    assert!(weak.upgrade().is_some());
    drop(thrown);
    assert!(
        weak.upgrade().is_none(),
        "opaque origin must release with its Realm wrapper"
    );
}

struct FailingView(super::value::Value);
impl super::value::Exotic for FailingView {
    fn lookup(
        &self,
        _: &mut Realm,
        key: &super::value::PropKey,
    ) -> Result<Option<super::value::Value>, super::value::Thrown> {
        if key.same(&super::value::PropKey::str("row")) {
            Err(self.0.clone())
        } else {
            Ok(None)
        }
    }
    fn keys(&self, _: &mut Realm) -> Result<Vec<super::value::PropKey>, super::value::Thrown> {
        Err(self.0.clone())
    }
}

#[test]
fn legacy_table_hook_failures_stop_each_consumer_and_keep_original_throw() {
    use super::value::{Kind, PropKey, Value};
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/realm-table-hooks-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 13);
    let mut actual = vec![];
    for row in &expected {
        let mut realm = Realm::new(Rc::new(TestRuntime::default()));
        let cause = realm.object_from(vec![]);
        realm
            .define_str(&cause, "\u{1d306}", Value::Number(-0.0))
            .unwrap();
        let reads = Rc::new(Cell::new(0));
        let getter_reads = reads.clone();
        let poison = realm.native("do not inspect", 0, move |_, _, _| {
            getter_reads.set(getter_reads.get() + 1);
            Err(Value::units(&[0xd800]))
        });
        realm.define_getter(&cause, PropKey::str("message"), poison);
        let target = realm.alloc(
            Kind::Exotic(Rc::new(FailingView(cause.clone()))),
            Some(&realm.intrinsics().object_proto),
        );
        let label = row["label"].as_str().unwrap();
        let result = (|| -> Result<(), Value> {
            match label {
                "get" => {
                    realm.get_str(&target, "row")?;
                }
                "in" => {
                    realm.has_property(&target, &PropKey::str("row"))?;
                }
                "has-own" => {
                    realm.has_own(&target, &PropKey::str("row"))?;
                }
                "names" => {
                    realm.own_keys(&target, false)?;
                }
                "entries" => {
                    realm.own_entries(&target)?;
                }
                "for-in" => {
                    realm.for_in_keys(&target)?;
                }
                "spread" => {
                    realm.spread_object(&target)?;
                }
                "stringify" => {
                    realm.stringify(&target)?;
                }
                "Object.keys" | "Object.values" | "Object.entries" => {
                    let object = realm.global_binding("Object")?;
                    realm.call_method(
                        &object,
                        label.strip_prefix("Object.").unwrap(),
                        &[target.clone()],
                    )?;
                }
                "Object.hasOwn" => {
                    let object = realm.global_binding("Object")?;
                    realm.call_method(&object, "hasOwn", &[target.clone(), Value::str("row")])?;
                }
                "hasOwnProperty" => {
                    let method = realm.get_str(
                        &Value::Object(realm.intrinsics().object_proto.clone()),
                        "hasOwnProperty",
                    )?;
                    realm.call(&method, &target, &[Value::str("row")])?;
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        actual.push(serde_json::json!({"label":label,"same":result.as_ref().err().is_some_and(|error|error.strict_equal(&cause)),"after":result.is_ok(),"reads":reads.get()}));
    }
    assert_eq!(
        actual, expected,
        "HL_TABLE_HOOK failures must escape without coercion or continuation"
    );
}

struct ReentrantView(
    std::cell::RefCell<std::rc::Weak<std::cell::RefCell<super::value::ObjectData>>>,
);
impl super::value::Exotic for ReentrantView {
    fn lookup(
        &self,
        realm: &mut Realm,
        _: &super::value::PropKey,
    ) -> Result<Option<super::value::Value>, super::value::Thrown> {
        let target = super::value::Value::Object(self.0.borrow().upgrade().unwrap());
        realm.define_str(&target, "side", super::value::Value::Number(1.0))?;
        Ok(Some(super::value::Value::Number(42.0)))
    }
    fn keys(&self, realm: &mut Realm) -> Result<Vec<super::value::PropKey>, super::value::Thrown> {
        let target = super::value::Value::Object(self.0.borrow().upgrade().unwrap());
        realm.define_str(&target, "side", super::value::Value::Number(2.0))?;
        Ok(vec![super::value::PropKey::str("row")])
    }
}

#[test]
fn legacy_table_hooks_can_reenter_without_retaining_property_borrows() {
    use super::value::{Kind, PropKey};
    let mut realm = Realm::new(Rc::new(TestRuntime::default()));
    let view = Rc::new(ReentrantView(std::cell::RefCell::new(std::rc::Weak::new())));
    let target = realm.alloc(
        Kind::Exotic(view.clone()),
        Some(&realm.intrinsics().object_proto),
    );
    *view.0.borrow_mut() = Rc::downgrade(target.object().unwrap());
    assert_eq!(
        realm.get_str(&target, "row").unwrap().as_number(),
        Some(42.0)
    );
    assert_eq!(
        realm.get_str(&target, "side").unwrap().as_number(),
        Some(1.0)
    );
    let keys = realm.own_keys(&target, true).unwrap();
    assert_eq!(keys.len(), 2);
    assert!(keys[0].same(&PropKey::str("row")));
    assert!(keys[1].same(&PropKey::str("side")));
    assert!(realm.has_property(&target, &PropKey::str("row")).unwrap());
    assert!(realm.has_own(&target, &PropKey::str("row")).unwrap());
    assert_eq!(
        realm.get_str(&target, "side").unwrap().as_number(),
        Some(2.0)
    );
}
