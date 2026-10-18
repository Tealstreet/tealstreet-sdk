use safe_cex_exchange::js_runtime::value::{Js, Prop, Thrown, arg, utf16};
use safe_cex_num::PrototypeRealm;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};

#[test]
fn functions_keep_live_prototypes_receivers_and_bound_inheritance() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_function_prototypes_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let base = Js::function("Base", |_, _| Ok(Js::Undefined));
    let derived = Js::function("Derived", |_, _| Ok(Js::Undefined));
    let sentinel = Js::object(vec![]);
    base.set("staticValue", sentinel.clone()).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let reads = trace.clone();
    let expected_receiver = derived.clone();
    base.define_accessor(
        "receiver",
        Some(Rc::new(move |this| {
            reads
                .borrow_mut()
                .push(this.same_reference(&expected_receiver));
            Ok(this.clone())
        })),
        None,
    );
    let writes = trace.clone();
    let expected_receiver = derived.clone();
    base.define_accessor(
        "write",
        None,
        Some(Rc::new(move |this, value| {
            writes
                .borrow_mut()
                .push(this.same_reference(&expected_receiver));
            this.set("written", value)
        })),
    );
    let Js::Function(data) = &base else {
        unreachable!()
    };
    data.props.borrow_mut().insert(
        utf16("readonly"),
        Prop::DataDescriptor {
            value: sentinel.clone(),
            writable: false,
            enumerable: false,
            configurable: false,
        },
    );
    derived.set("__proto__", base.clone()).unwrap();
    let inherited = derived
        .get("staticValue")
        .unwrap()
        .same_reference(&sentinel);
    assert!(
        inherited,
        "FUNCTION_PROTOTYPE inherited static member retains original referent"
    );
    assert!(
        !derived.has_own("__proto__"),
        "FUNCTION_PROTOTYPE legacy setter changes internal parent"
    );
    let receiver = derived.get("receiver").unwrap().same_reference(&derived);
    derived.set("write", Js::Number(-0.0)).unwrap();
    let written =
        derived.get("written").unwrap().as_f64().unwrap().to_bits() == (-0.0_f64).to_bits();
    assert!(!derived.has_own("write"));
    let readonly = derived
        .set("readonly", Js::Number(1.0))
        .unwrap_err()
        .value
        .get("name")
        .unwrap()
        .to_display();
    assert!(!derived.has_own("readonly"));
    let cycle = base
        .set("__proto__", derived.clone())
        .unwrap_err()
        .value
        .get("name")
        .unwrap()
        .to_display();
    derived.set("__proto__", Js::Number(1.0)).unwrap();
    let primitive_ignored = derived.get("__proto__").unwrap().same_reference(&base);
    derived.define("__proto__", sentinel.clone()).unwrap();
    let own_shadow = derived.get("__proto__").unwrap().same_reference(&sentinel)
        && derived
            .get("staticValue")
            .unwrap()
            .same_reference(&sentinel);
    derived.delete(&utf16("__proto__"));
    assert!(derived.get("__proto__").unwrap().same_reference(&base));
    let make = |realm: &Rc<PrototypeRealm>, name| {
        let _binding = realm.bind();
        Js::function(name, |_, _| Ok(Js::Undefined))
    };
    let first_realm = Rc::new(PrototypeRealm::default());
    let second_realm = Rc::new(PrototypeRealm::default());
    let a = make(&first_realm, "A");
    let b = make(&second_realm, "B");
    let _other_binding = second_realm.bind();
    let realms = [
        a.get("__proto__")
            .unwrap()
            .same_reference(&a.get("__proto__").unwrap()),
        !a.get("__proto__")
            .unwrap()
            .same_reference(&b.get("__proto__").unwrap()),
        !a.get("call")
            .unwrap()
            .same_reference(&b.get("call").unwrap()),
    ];
    assert!(
        realms.iter().all(|value| *value),
        "FUNCTION_PROTOTYPE allocation realm is retained"
    );
    let call = derived.get("call").unwrap();
    let bind = derived.get("bind").unwrap();
    let function_prototype = base.get("__proto__").unwrap();
    let methods = [
        call.same_reference(&base.get("call").unwrap()),
        call.same_reference(&derived.get("call").unwrap()),
        bind.same_reference(&base.get("bind").unwrap()),
        bind.same_reference(&derived.get("bind").unwrap()),
        call.get("__proto__")
            .unwrap()
            .same_reference(&function_prototype),
    ];
    let target = Js::function("target", |this, args| {
        Ok(Js::array(vec![this, arg(&args, 0)]))
    });
    target.set("__proto__", base.clone()).unwrap();
    let bound = target
        .call_method("bind", vec![sentinel.clone(), Js::Number(-0.0)])
        .unwrap();
    let result = bound.call(Js::Undefined, vec![]).unwrap();
    let binding = json!([
        bound.get("__proto__").unwrap().same_reference(&base),
        bound.get("staticValue").unwrap().same_reference(&sentinel),
        result.get("0").unwrap().same_reference(&sentinel),
        result.get("1").unwrap().as_f64().unwrap().to_bits() == (-0.0_f64).to_bits(),
        bound.get("name").unwrap().to_display(),
    ]);
    let unknown_key = derived.get_key(&[0xd800]).unwrap().is_undefined();
    let empty = function_prototype
        .call_method("call", vec![Js::Null])
        .unwrap()
        .is_undefined();
    let detached = Js::function("Detached", |_, _| Ok(Js::Undefined));
    detached.set("__proto__", Js::Null).unwrap();
    let coercion = detached
        .to_js_string()
        .unwrap_err()
        .value
        .get("name")
        .unwrap()
        .to_display();
    let detached = json!([
        detached.get("call").unwrap().is_undefined(),
        detached.get("__proto__").unwrap().is_undefined(),
        coercion
    ]);
    assert_eq!(
        json!({"inherited":inherited,"receiver":receiver,"written":written,"readonly":readonly,"cycle":cycle,"primitiveIgnored":primitive_ignored,"ownShadow":own_shadow,"trace":*trace.borrow(),"realms":realms,"methods":methods,"binding":binding,"unknownKey":unknown_key,"empty":empty,"detached":detached}),
        expected,
        "FUNCTION_PROTOTYPE actual Node language observations"
    );
}

#[test]
fn inherited_function_accessors_keep_original_throws_and_data_shadows() {
    let base = Js::function("Base", |_, _| Ok(Js::Undefined));
    let child = Js::function("Child", |_, _| Ok(Js::Undefined));
    let failure = Js::object(vec![]);
    let thrown = failure.clone();
    base.define_accessor(
        "failure",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    );
    child.set("__proto__", base.clone()).unwrap();
    assert!(
        child
            .get("failure")
            .unwrap_err()
            .value
            .same_reference(&failure),
        "FUNCTION_PROTOTYPE original inherited getter failure"
    );
    child.define("failure", Js::Undefined).unwrap();
    assert!(child.get("failure").unwrap().is_undefined());
    child.delete(&utf16("failure"));
    assert!(
        child
            .get("failure")
            .unwrap_err()
            .value
            .same_reference(&failure)
    );
    let object = Js::object(vec![]);
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(child.clone());
    assert!(
        object
            .get("failure")
            .unwrap_err()
            .value
            .same_reference(&failure)
    );
    child.define_accessor("onlyGet", Some(Rc::new(|_| Ok(Js::Number(2.0)))), None);
    assert!(object.set("onlyGet", Js::Number(1.0)).is_err());
    assert!(!object.has_own("onlyGet"));
    assert!(
        object.set("__proto__", object.clone()).is_err(),
        "FUNCTION_PROTOTYPE mixed object/function cycle refused"
    );
}
