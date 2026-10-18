use safe_cex_exchange::js_runtime::{
    globals,
    value::{Js, Thrown, to_json},
};
use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};

fn source() -> Value {
    let result = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_object_prototypes_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn scope() -> (Js, Js) {
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let object = globals.get("Object").unwrap();
    (globals, object)
}
fn kind(result: Result<Js, Thrown>) -> Value {
    match result {
        Ok(_) => Value::Null,
        Err(error) => json!(error.value.get("name").unwrap().to_display()),
    }
}
fn allocation(index: usize) -> Js {
    match index {
        0 => Js::object(vec![]),
        1 => Js::host_object(Rc::new(Cell::new(42))),
        _ => Js::object_with_class("NativeClass", vec![]),
    }
}
#[test]
fn ordinary_allocations_keep_their_realms_default_object_prototype() {
    let expected = source();
    let (_, object) = scope();
    let prototype = Js::Builtin(PrototypeRealm::current().reference(ProtoMember::Prototype));
    let actual = (0..3).map(|index| {
        let value=allocation(index);
        json!({"stock":value.get("__proto__").unwrap().same_reference(&prototype),"own":value.has_own("__proto__")})
    }).collect::<Vec<_>>();
    assert_eq!(
        json!(actual),
        expected["defaults"],
        "OBJECT_PROTOTYPE ordinary allocator links canonical realm identity"
    );
    assert!(
        object.get("prototype").unwrap().same_reference(&prototype),
        "OBJECT_PROTOTYPE facade exposes the same identity"
    );
    let first = Rc::new(PrototypeRealm::default());
    let second = Rc::new(PrototypeRealm::default());
    let make = |realm: &Rc<PrototypeRealm>, mark: &str| {
        let _binding = realm.bind();
        realm
            .reference(ProtoMember::Prototype)
            .set("realmMark", PrototypeScalar::String(mark.into()));
        Js::object(vec![])
    };
    let one = make(&first, "first");
    let two = make(&second, "second");
    assert_eq!(
        json!({"distinct":!one.get("__proto__").unwrap().same_reference(&two.get("__proto__").unwrap()),"first":one.get("realmMark").unwrap().to_display(),"second":two.get("realmMark").unwrap().to_display()}),
        expected["realms"],
        "OBJECT_PROTOTYPE allocation retains original realm after binding exits"
    );
}
#[test]
fn legacy_prototype_setter_preserves_receiver_null_shadow_and_resource() {
    let expected = source();
    let (globals, object) = scope();
    let mut actual = vec![];
    for index in 0..3 {
        let value = allocation(index);
        value.set("label", Js::str("visible")).unwrap();
        let prototype = Js::object(vec![]);
        prototype
            .define_accessor("inherited", Some(Rc::new(|this| this.get("label"))), None)
            .expect("test accessor target permits definition");
        value.set("__proto__", prototype.clone()).unwrap();
        let child = Js::object(vec![("label", Js::str("child"))]);
        child.set("__proto__", value.clone()).unwrap();
        let mut row = json!({"keys":to_json(&object.call_method("keys",vec![value.clone()]).unwrap()).unwrap(),"names":to_json(&object.call_method("getOwnPropertyNames",vec![value.clone()]).unwrap()).unwrap(),"inherited":to_json(&value.get("inherited").unwrap()).unwrap(),"child":to_json(&child.get("inherited").unwrap()).unwrap(),"json":globals.get("JSON").unwrap().call_method("stringify",vec![value.clone()]).unwrap().to_display(),"own":value.has_own("__proto__")});
        value.set("__proto__", Js::Number(17.)).unwrap();
        row["primitiveIgnored"] = json!(value.get("__proto__").unwrap().same_reference(&prototype));
        object.call_method("freeze", vec![value.clone()]).unwrap();
        row["sameFrozen"] = kind(value.set("__proto__", prototype).map(|_| value.clone()));
        row["changedFrozen"] = kind(
            value
                .set("__proto__", Js::object(vec![]))
                .map(|_| value.clone()),
        );
        if index == 1 {
            assert_eq!(value.resource::<Cell<i32>>().unwrap().get(), 42);
            assert!(
                child.resource::<Cell<i32>>().is_none(),
                "OBJECT_PROTOTYPE resource lookup stays own-only"
            );
        }
        actual.push(row);
    }
    assert_eq!(
        json!(actual),
        expected["legacy"],
        "OBJECT_PROTOTYPE legacy setter uses original receiver and preserves ordinary own fields"
    );
    let null_object = Js::object(vec![("label", Js::str("null"))]);
    null_object.set("__proto__", Js::Null).unwrap();
    let mut row = json!({"read":null_object.get("__proto__").unwrap().type_of(),"coercion":kind(null_object.to_js_string().map(Js::String)),"own":null_object.has_own("__proto__")});
    null_object.set("__proto__", Js::Number(17.)).unwrap();
    row["ownAfterSet"] = json!(null_object.has_own("__proto__"));
    row["valueAfterSet"] = json!(null_object.get("__proto__").unwrap().to_display());
    assert_eq!(
        row, expected["nullPrototype"],
        "OBJECT_PROTOTYPE null is distinct from allocator default"
    );
    let parent = Js::object(vec![]);
    let child = Js::object(vec![]);
    child.set("__proto__", parent.clone()).unwrap();
    assert_eq!(
        kind(parent.set("__proto__", child).map(|_| parent.clone())),
        expected["cycle"],
        "OBJECT_PROTOTYPE cycle refusal"
    );
    let shadow = Js::object(vec![("__proto__", Js::Number(4.))]);
    shadow.set("__proto__", Js::Number(9.)).unwrap();
    assert_eq!(
        json!({"value":shadow.get("__proto__").unwrap().to_display(),"stock":matches!(&shadow,Js::Object(o) if o.proto.borrow().as_ref().is_some_and(|p|p.same_reference(&Js::Builtin(PrototypeRealm::current().reference(ProtoMember::Prototype))))),"keys":to_json(&object.call_method("keys",vec![shadow]).unwrap()).unwrap()}),
        expected["ownShadow"],
        "OBJECT_PROTOTYPE own data shadows legacy setter"
    );
    let trace = Rc::new(RefCell::new(vec![]));
    let events = trace.clone();
    let prototype = Js::object(vec![]);
    prototype
        .define_accessor(
            "__proto__",
            None,
            Some(Rc::new(move |this, value| {
                events
                    .borrow_mut()
                    .push(json!([this.get("label")?.to_display(), value.to_display()]));
                Ok(())
            })),
        )
        .expect("test accessor target permits definition");
    let receiver = Js::object(vec![("label", Js::str("receiver"))]);
    receiver.set("__proto__", prototype).unwrap();
    receiver.set("__proto__", Js::Number(23.)).unwrap();
    assert_eq!(
        json!(*trace.borrow()),
        expected["setterTrace"],
        "OBJECT_PROTOTYPE inherited custom setter receives original receiver"
    );
}
#[test]
fn object_set_prototype_of_binding_preserves_order_identity_and_refusals() {
    let expected = source();
    let (_, object) = scope();
    let set =
        |value: Js, prototype: Js| object.call_method("setPrototypeOf", vec![value, prototype]);
    let calls = Rc::new(Cell::new(0));
    let counter = calls.clone();
    let target = Js::object(vec![]);
    target
        .define_accessor(
            "__proto__",
            None,
            Some(Rc::new(move |_, _| {
                counter.set(counter.get() + 1);
                Ok(())
            })),
        )
        .expect("test accessor target permits definition");
    let prototype = Js::object(vec![("marker", Js::str("static"))]);
    let result = set(target.clone(), prototype).expect("OBJECT_PROTOTYPE setPrototypeOf is bound");
    let function = Js::function("fn", |_, _| Ok(Js::Undefined));
    set(
        function.clone(),
        Js::object(vec![("marker", Js::str("fn"))]),
    )
    .unwrap();
    let descriptor = object
        .call_method(
            "getOwnPropertyDescriptor",
            vec![object.clone(), Js::str("prototype")],
        )
        .unwrap();
    assert_eq!(
        json!({"identity":result.same_reference(&target),"marker":target.get("marker").unwrap().to_display(),"bypassCalls":calls.get(),"functionIdentity":function.get("marker").unwrap().to_display(),"prototypeOwnEnumerable":descriptor.get("enumerable").unwrap().truthy()}),
        expected["static"],
        "OBJECT_PROTOTYPE static binding bypasses shadowing and supports functions"
    );
    let mut primitive = serde_json::Map::new();
    for value in [
        Js::Number(0.),
        Js::str("text"),
        Js::Bool(false),
        Js::BigInt(Rc::new(1.into())),
        Js::SymbolIterator,
    ] {
        primitive.insert(
            value.type_of().into(),
            json!(
                set(value.clone(), Js::object(vec![]))
                    .unwrap()
                    .strict_equals(&value)
            ),
        );
    }
    assert_eq!(
        Value::Object(primitive),
        expected["primitive"],
        "OBJECT_PROTOTYPE primitive target identity"
    );
    let mut validation = json!({"nullTarget":kind(set(Js::Null,Js::object(vec![]))),"undefinedTarget":kind(set(Js::Undefined,Js::object(vec![]))),"badProto":kind(set(Js::object(vec![]),Js::Number(17.))),"primitiveBadProto":kind(set(Js::Number(0.),Js::Number(17.)))});
    let frozen = Js::object(vec![]);
    let stock = frozen.get("__proto__").unwrap();
    object
        .call_method("preventExtensions", vec![frozen.clone()])
        .unwrap();
    validation["sameFrozen"] = kind(set(frozen.clone(), stock));
    validation["changedFrozen"] = kind(set(frozen, Js::object(vec![])));
    let a = Js::object(vec![]);
    let b = Js::object(vec![]);
    set(a.clone(), b.clone()).unwrap();
    validation["cycle"] = kind(set(b, a));
    let null_object = Js::object(vec![]);
    set(null_object.clone(), Js::Null).unwrap();
    validation["nullCoercion"] = kind(null_object.to_js_string().map(Js::String));
    set(null_object.clone(), object.get("prototype").unwrap()).unwrap();
    validation["reattached"] = json!(null_object.to_display());
    assert_eq!(
        validation, expected["validation"],
        "OBJECT_PROTOTYPE static validation and metadata semantics"
    );
}
