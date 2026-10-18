//! A retained intrinsic referent is shared with the existing native realm.
use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_js_json, to_typed};
use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar, PrototypeValue};
use serde_json::json;
use std::{collections::BTreeMap, process::Command, rc::Rc};

#[test]
fn retained_intrinsic_identity_fields_and_arity_match_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/prototype-value-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let members = [
        ProtoMember::Constructor,
        ProtoMember::DefineGetter,
        ProtoMember::HasOwnProperty,
        ProtoMember::ToString,
        ProtoMember::ValueOf,
        ProtoMember::Prototype,
    ];
    assert_eq!(source.as_array().unwrap().len(), members.len());
    for (member, expected) in members.into_iter().zip(source.as_array().unwrap()) {
        let realm = Rc::<PrototypeRealm>::default();
        let reference = realm.reference(member);
        let first = Js::Builtin(reference.clone());
        let second = Js::from_prototype(PrototypeValue::Reference(realm.reference(member)));
        let before = json!([
            first.type_of(),
            first.to_display(),
            first.get("length").unwrap().to_display(),
            first.strict_equals(&second)
        ]);
        assert_eq!(
            before, expected["before"],
            "INTRINSIC native kind/text/arity/identity {member:?}"
        );
        assert!(!first.strict_equals(&Js::Builtin(
            Rc::<PrototypeRealm>::default().reference(member)
        )));
        reference.set("late", PrototypeScalar::Number(7.0));
        assert_eq!(
            second.get("late").unwrap().as_f64(),
            expected["late"].as_f64(),
            "INTRINSIC reads live native fields"
        );
        let keys = second
            .own_keys()
            .iter()
            .map(|key| String::from_utf16(key).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(keys),
            expected["keys"],
            "INTRINSIC enumerates live native fields"
        );
        for value in [-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            first.set("written", Js::Number(value)).unwrap();
            let PrototypeValue::Number(native) = reference.get("written") else {
                panic!("INTRINSIC writes reach native realm as a number")
            };
            assert_eq!(
                native.to_bits(),
                value.to_bits(),
                "INTRINSIC writes reach native realm without projection"
            );
            assert_eq!(
                second.get("written").unwrap().as_f64().unwrap().to_bits(),
                value.to_bits()
            );
        }
        if member == ProtoMember::Prototype {
            let record: BTreeMap<String, f64> = to_typed(&first).unwrap();
            assert!(record["written"].is_nan());
        } else {
            assert!(matches!(
                to_js_json(&first).unwrap(),
                safe_cex_num::JsJsonValue::Undefined
            ));
        }
    }
}

#[test]
fn captured_intrinsic_prototype_keeps_original_receiver_and_own_shadow() {
    let realm = Rc::<PrototypeRealm>::default();
    let prototype = Js::Builtin(realm.reference(ProtoMember::Prototype));
    let object = Js::object(vec![]);
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(prototype.clone());
    assert!(
        object.get("__proto__").unwrap().strict_equals(&prototype),
        "INTRINSIC legacy getter keeps receiver"
    );
    realm
        .reference(ProtoMember::Prototype)
        .set("later", PrototypeScalar::Number(9.0));
    assert_eq!(object.get("later").unwrap().as_f64(), Some(9.0));
    object.set("__proto__", Js::Number(1.0)).unwrap();
    assert!(object.get("__proto__").unwrap().strict_equals(&prototype));
    object.define("__proto__", Js::Undefined).unwrap();
    assert!(object.has_own("__proto__"));
    assert!(object.get("__proto__").unwrap().is_undefined());
    object.set("__proto__", Js::Null).unwrap();
    assert!(matches!(object.get("__proto__").unwrap(), Js::Null));
    assert!(
        data.proto
            .borrow()
            .as_ref()
            .unwrap()
            .strict_equals(&prototype)
    );
    object
        .delete(&"__proto__".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    object.set("__proto__", Js::Null).unwrap();
    assert!(data.proto.borrow().is_none());
    assert!(object.get("__proto__").unwrap().is_undefined());
}

#[test]
fn native_thrown_to_shared_failure_retains_error_identity() {
    let native = Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
        "native opaque failure",
    ));
    let thrown = Thrown {
        value: Js::Undefined,
        native: Some(native.clone()),
    };
    let failure: Rc<safe_cex_exchange::runtime::ExchangeError> = thrown.into();
    assert!(Rc::ptr_eq(&failure, &native));
}
