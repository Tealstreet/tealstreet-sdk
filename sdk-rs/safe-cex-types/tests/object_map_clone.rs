use safe_cex_types::object_map::JsObjectMap;
use std::{cell::RefCell, rc::Rc};

#[test]
fn decoded_object_map_clone_keeps_native_values_and_separate_owned_storage() {
    let original: JsObjectMap<Vec<String>> =
        serde_json::from_str(r#"{"label":["name-a"],"10":["ten-only"],"2":["two-a","two-b"]}"#)
            .unwrap();
    // Authored fixture has three properties. Native assertions precede cloning;
    // there is no serialization/roundtrip surrogate for the stored values.
    assert_eq!(original.len(), 3, "native decoded property count");
    assert_eq!(
        original.keys().map(String::as_str).collect::<Vec<_>>(),
        ["2", "10", "label"],
        "native decoded key order"
    );
    assert_eq!(original["2"], ["two-a", "two-b"], "native decoded two");
    assert_eq!(original["10"], ["ten-only"], "native decoded ten");
    assert_eq!(original["label"], ["name-a"], "native decoded label");

    let mut cloned = original.clone();
    assert_eq!(
        cloned.keys().map(String::as_str).collect::<Vec<_>>(),
        ["2", "10", "label"],
        "native Clone key snapshot"
    );
    assert_eq!(cloned["2"], ["two-a", "two-b"], "native cloned two");
    assert_eq!(cloned["10"], ["ten-only"], "native cloned ten");
    assert_eq!(cloned["label"], ["name-a"], "native cloned label");
    cloned.get_mut("2").unwrap()[0] = "changed".into();
    assert_eq!(cloned.remove("10").unwrap(), ["ten-only"]);
    cloned.insert("1".into(), vec!["new-only".into()]);
    assert_eq!(cloned.len(), 3, "native changed clone property count");
    assert_eq!(
        cloned.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2", "label"],
        "native changed clone key order"
    );
    assert_eq!(cloned["2"], ["changed", "two-b"]);
    assert_eq!(original.len(), 3, "native original property count retained");
    assert_eq!(original["2"], ["two-a", "two-b"], "owned payload isolation");
    assert_eq!(original["10"], ["ten-only"], "removed clone key retained");
    assert!(original.get("1").is_none(), "new clone key is isolated");
}

#[test]
fn object_map_clone_delegates_shared_payload_identity_to_immediate_t() {
    // This inner value is not Clone. The immediate T is Rc<RefCell<_>>, which
    // is Clone and deliberately retains identity rather than copying the value.
    struct NonClonePayload {
        value: i32,
    }
    let payload = Rc::new(RefCell::new(NonClonePayload { value: 17 }));
    let mut original = JsObjectMap::new();
    original.insert("shared".into(), Rc::clone(&payload));
    let mut cloned = original.clone();
    assert!(Rc::ptr_eq(&original["shared"], &cloned["shared"]));
    cloned["shared"].borrow_mut().value = 37;
    assert_eq!(
        original["shared"].borrow().value,
        37,
        "payload alias retained"
    );
    cloned.insert(
        "extra".into(),
        Rc::new(RefCell::new(NonClonePayload { value: 88 })),
    );
    assert_eq!(original.len(), 1, "shared payload does not share container");
    assert!(original.get("extra").is_none());
    assert_eq!(cloned.len(), 2, "clone has its independently added key");
    assert_eq!(cloned["extra"].borrow().value, 88);
}
