//! Native evidence for the exact EventOps receiver classifications.
use safe_cex_num::{ProtoMember, PrototypeRealm};
use safe_cex_venue_binance::private_event_value::{EventOps, EventValue};
use serde_json::json;

fn text(value: EventValue) -> Vec<u16> {
    let EventValue::String(units) = value else {
        panic!("expected native string, before serialization")
    };
    units.to_vec()
}

#[test]
fn event_from_json_retains_native_fields_bits_and_captured_prototype() {
    let realm = PrototypeRealm::current();
    for (name, size, flag) in [("first🚀", -0.0_f64, true), ("second", 13.5, false)] {
        let value = <EventValue as EventOps>::from_json(json!({
            "name": name, "size": size, "nested": {"enabled": flag}, "items": [null, 3]
        }));
        assert_eq!(
            text(value.read("name").unwrap()),
            name.encode_utf16().collect::<Vec<_>>()
        );
        let EventValue::Number(actual) = value.read("size").unwrap() else {
            panic!("JSON number must remain a native number")
        };
        assert_eq!(
            actual.to_bits(),
            size.to_bits(),
            "binary64 payload precedes serialization"
        );
        let nested = value.read("nested").unwrap();
        assert!(matches!(nested.read("enabled").unwrap(), EventValue::Bool(v) if v == flag));
        for object in [&value, &nested] {
            let EventValue::Object(storage) = object else {
                panic!("native object")
            };
            assert!(
                storage
                    .proto
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .strict_equals(&EventValue::Builtin(
                        realm.reference(ProtoMember::Prototype)
                    )),
                "each decoded object captures the existing native realm"
            );
        }
        let EventValue::Array(items) = value.read("items").unwrap() else {
            panic!("native array")
        };
        assert_eq!(items.borrow().len(), 2);
        assert!(matches!(items.borrow()[0], Some(EventValue::Null)));
        assert!(matches!(items.borrow()[1], Some(EventValue::Number(3.0))));
    }
    assert!(matches!(
        <EventValue as EventOps>::from_json(json!(null)),
        EventValue::Null
    ));
}

#[test]
fn event_text_value_wraps_literal_utf16_without_parsing() {
    for literal in ["not JSON {", "{\"id\":42}", "", "\0rocket🚀"] {
        assert_eq!(
            text(<EventValue as EventOps>::text_value(literal.into())),
            literal.encode_utf16().collect::<Vec<_>>()
        );
    }
}

#[test]
fn event_read_forwards_existing_properties_and_missingness() {
    let child = EventValue::object(vec![("value", EventValue::Number(-0.0))]);
    let value = EventValue::object(vec![
        ("not JSON {", child.clone()),
        ("ownUndefined", EventValue::Undefined),
    ]);
    assert!(
        value.read("not JSON {").unwrap().same(&child),
        "property result keeps its original reference"
    );
    assert!(matches!(
        value.read("absent").unwrap(),
        EventValue::Undefined
    ));
    assert!(value.has_own("ownUndefined"));
    assert!(matches!(
        value.read("ownUndefined").unwrap(),
        EventValue::Undefined
    ));
    assert!(!value.has_own("absent"));
    assert_eq!(EventValue::Null.read("x").unwrap_err().name, "TypeError");
    assert!(matches!(
        EventValue::array(vec![EventValue::Bool(true)])
            .read("length")
            .unwrap(),
        EventValue::Number(1.0)
    ));
}

#[test]
fn event_write_defines_the_existing_handle_without_decoding() {
    let object = EventValue::object(vec![]);
    let child = EventValue::object(vec![]);
    <EventValue as EventOps>::write(&object, "not JSON {", child.clone());
    assert!(object.has_own("not JSON {"));
    assert!(object.read("not JSON {").unwrap().same(&child));
    child.define("late", EventValue::Number(-0.0)).unwrap();
    let EventValue::Number(n) = object.read("not JSON {").unwrap().read("late").unwrap() else {
        panic!("native number")
    };
    assert_eq!(n.to_bits(), (-0.0_f64).to_bits());
    <EventValue as EventOps>::write(&object, "not JSON {", EventValue::Undefined);
    assert!(object.has_own("not JSON {"));
    assert!(matches!(
        object.read("not JSON {").unwrap(),
        EventValue::Undefined
    ));
}

#[test]
fn event_assign_sets_the_existing_handle_without_decoding() {
    let object = EventValue::object(vec![]);
    let child = EventValue::object(vec![("flag", EventValue::Bool(false))]);
    <EventValue as EventOps>::assign(&object, "not JSON {", child.clone());
    assert!(object.read("not JSON {").unwrap().same(&child));
    child.set("flag", EventValue::Bool(true)).unwrap();
    assert!(matches!(
        object.read("not JSON {").unwrap().read("flag").unwrap(),
        EventValue::Bool(true)
    ));
    <EventValue as EventOps>::assign(&object, "not JSON {", EventValue::Null);
    assert!(matches!(
        object.read("not JSON {").unwrap(),
        EventValue::Null
    ));
}

#[test]
fn event_optional_id_coerces_existing_values_with_expression_only_in_errors() {
    for value in [EventValue::Undefined, EventValue::Null] {
        assert!(matches!(
            value.optional_id("record.id").unwrap(),
            EventValue::Undefined
        ));
    }
    for (value, expected) in [
        (EventValue::Bool(true), "true"),
        (EventValue::Number(-0.0), "0"),
        (EventValue::str("literal {"), "literal {"),
    ] {
        assert_eq!(
            text(value.optional_id("unused expression").unwrap()),
            expected.encode_utf16().collect::<Vec<_>>()
        );
    }
    let invalid = EventValue::object(vec![("toString", EventValue::Bool(false))]);
    for expression in ["record.id", "another expression"] {
        let error = invalid.optional_id(expression).unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert_eq!(
            error.message,
            format!("{expression}?.toString is not a function")
        );
    }
}
