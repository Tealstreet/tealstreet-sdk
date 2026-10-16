//! Actual fixed alias decoders and their native container identities.
use safe_cex_venue_grvt::{
    builder_workflow::JsResult, composition::Fields, public_lifecycle::AddedMarkets,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

#[test]
fn added_markets_bool_is_a_sequence_decoder() {
    let decoded: AddedMarkets<bool> = serde_json::from_str("[true,false]").unwrap();
    let rows = decoded.borrow();
    assert_eq!(rows.len(), 2);
    assert!(*rows[0].borrow());
    assert!(!*rows[1].borrow());
    assert!(!std::rc::Rc::ptr_eq(&rows[0], &rows[1]));
    drop(rows);
    for raw in ["null", "{}", "true", "[0]", "[null]"] {
        assert!(serde_json::from_str::<AddedMarkets<bool>>(raw).is_err());
    }
}

#[test]
fn js_result_bool_keeps_native_success_and_error_payloads() {
    let success: JsResult<bool, bool> = serde_json::from_str(r#"{"Ok":false}"#).unwrap();
    let failure: JsResult<bool, bool> = serde_json::from_str(r#"{"Err":true}"#).unwrap();
    assert_eq!(success, Ok(false));
    assert_eq!(failure, Err(true));
    for raw in [
        "null",
        "true",
        "[]",
        r#"{"Ok":0}"#,
        r#"{"Err":null}"#,
        r#"{"Other":true}"#,
    ] {
        assert!(serde_json::from_str::<JsResult<bool, bool>>(raw).is_err());
    }
}

#[test]
fn fields_text_keeps_declared_key_order_and_bool_values() {
    let fields: Fields<bool> = serde_json::from_str(r#"{"z":false,"a":true}"#).unwrap();
    let map = fields.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["z", "a"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [false, true]);
    drop(map);
    let other: Fields<bool> = serde_json::from_str(r#"{"second":true,"first":false}"#).unwrap();
    let map = other.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [true, false]);
}

#[test]
fn fields_borrowed_value_retains_decoded_escaped_keys() {
    // The fixture owns these strings for the program lifetime. The actual alias
    // retains &'static str keys; no key type or production lifetime is changed.
    static INPUT: OnceLock<Value> = OnceLock::new();
    let input: &'static Value =
        INPUT.get_or_init(|| serde_json::from_str(r#"{"\u007a":false,"a":true}"#).unwrap());
    let fields = <Fields<bool> as Deserialize>::deserialize(input).unwrap();
    let map = fields.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["z", "a"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [false, true]);
    let original_key = input.as_object().unwrap().get_key_value("z").unwrap().0;
    assert_eq!(map.get_index(0).unwrap().0.as_ptr(), original_key.as_ptr());
}

#[test]
fn fields_text_refuses_escaped_keys_and_owned_nonempty_values() {
    // Text decoding cannot borrow a key whose lexical escape requires a buffer.
    assert!(serde_json::from_str::<Fields<bool>>(r#"{"\u007a":false,"a":true}"#).is_err());
    // An owned Value cannot lend its member strings for the required lifetime.
    assert!(<Fields<bool> as Deserialize>::deserialize(serde_json::json!({"z":false})).is_err());
    assert!(serde_json::from_str::<Fields<bool>>("{}").is_ok());
    for raw in ["null", "[]", "true", r#"{"z":0}"#, r#"{"z":null}"#] {
        assert!(serde_json::from_str::<Fields<bool>>(raw).is_err());
    }
}
