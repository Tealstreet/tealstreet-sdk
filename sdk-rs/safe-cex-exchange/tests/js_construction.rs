use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use safe_cex_num::{ProtoMember, PrototypeRealm};
use serde_json::{Value, json};
use std::{
    cell::{Cell, OnceCell},
    process::Command,
    rc::Rc,
};

#[test]
fn class_call_refuses_before_running_constructor() {
    let calls = Rc::new(Cell::new(0));
    let effects = calls.clone();
    let class = Js::constructor("SourceClass", None, move |_, _| {
        effects.set(effects.get() + 1);
        Ok(Js::object(vec![]))
    });
    let result = class.call(Js::Undefined, vec![]);
    assert!(
        result.is_err(),
        "JS_CONSTRUCT class call refuses before constructor body"
    );
    assert_eq!(
        calls.get(),
        0,
        "JS_CONSTRUCT class call has no construction effects"
    );
    assert_eq!(
        result.unwrap_err().value.get("name").unwrap().to_display(),
        "TypeError"
    );
}

#[test]
fn construction_keeps_new_target_dynamic_super_bound_arguments_and_realm() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_construction_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let class = Js::constructor("SourceClass", None, |target, args| {
        let instance = target.allocate_instance()?;
        instance.define("value", arg(&args, 0))?;
        instance.define("target", target)?;
        Ok(instance)
    });
    let prototype = Js::object(vec![]);
    class.define("prototype", prototype.clone()).unwrap();
    let slot: Rc<OnceCell<Js>> = Rc::default();
    let active = slot.clone();
    let derived = Js::constructor("Derived", None, move |target, args| {
        let superclass = active.get().unwrap().get("__proto__")?;
        superclass.construct(args, Some(&target))
    });
    slot.set(derived.clone()).unwrap();
    derived.define("prototype", Js::object(vec![])).unwrap();
    derived.set("__proto__", class.clone()).unwrap();
    let token = Js::object(vec![]);
    let normal = class.construct(vec![token.clone()], None).unwrap();
    let explicit = class
        .construct(vec![token.clone()], Some(&derived))
        .unwrap();
    let prototype_of = |instance: &Js| {
        let Js::Object(data) = instance else {
            panic!("native constructed instance")
        };
        data.proto
            .borrow()
            .clone()
            .expect("constructed instance parent")
    };
    let inspect = |instance: &Js, target: &Js| {
        [
            instance.get("target").unwrap().same_reference(target),
            instance.get("value").unwrap().same_reference(&token),
            prototype_of(instance).same_reference(&target.get("prototype").unwrap()),
        ]
    };
    let bound = class
        .call_method("bind", vec![Js::object(vec![]), token.clone()])
        .unwrap();
    assert!(
        bound.is_constructor(),
        "JS_CONSTRUCT bound constructor capability"
    );
    let bound_normal = bound.construct(vec![], None).unwrap();
    let bound_explicit = bound.construct(vec![], Some(&derived)).unwrap();
    let other = Js::constructor("Other", None, |target, args| {
        let instance = target.allocate_instance()?;
        instance.define("value", arg(&args, 0))?;
        instance.define("target", target)?;
        instance.define("other", Js::Bool(true))?;
        Ok(instance)
    });
    derived.set("__proto__", other).unwrap();
    let dynamic = derived.construct(vec![token.clone()], None).unwrap();
    let realm = Rc::new(PrototypeRealm::default());
    let foreign = {
        let _binding = realm.bind();
        Js::constructor("Foreign", None, |target, _| target.allocate_instance())
    };
    foreign.define("prototype", Js::Number(1.0)).unwrap();
    let fallback = class
        .construct(vec![token.clone()], Some(&foreign))
        .unwrap();
    let fallback = fallback
        .get("__proto__")
        .unwrap()
        .same_reference(&Js::Builtin(realm.reference(ProtoMember::Prototype)));
    let invalid = class
        .construct(
            vec![],
            Some(&Js::function("arrow", |_, _| Ok(Js::Undefined))),
        )
        .unwrap_err()
        .value
        .get("name")
        .unwrap()
        .to_display();
    assert_eq!(
        json!({"refusal":"TypeError","before":0,"normal":inspect(&normal,&class),"explicit":inspect(&explicit,&derived),"bound":[bound_normal.get("target").unwrap().same_reference(&class),bound_normal.get("value").unwrap().same_reference(&token),bound_explicit.get("target").unwrap().same_reference(&derived),prototype_of(&bound_explicit).same_reference(&derived.get("prototype").unwrap())],"dynamic":[dynamic.get("other").unwrap().truthy(),dynamic.get("target").unwrap().same_reference(&derived),dynamic.get("value").unwrap().same_reference(&token)],"fallback":fallback,"invalid":invalid}),
        source,
        "JS_CONSTRUCT actual Node construction observations"
    );
    let failure = Js::object(vec![]);
    let thrown = failure.clone();
    let refusing = Js::constructor("Refusing", None, move |_, _| {
        Err(Thrown::new(thrown.clone()))
    });
    assert!(
        refusing
            .construct(vec![], None)
            .unwrap_err()
            .value
            .same_reference(&failure),
        "JS_CONSTRUCT original constructor throw"
    );
    let primitive = Js::constructor("PrimitiveNative", None, |_, _| Ok(Js::Number(1.0)));
    assert!(
        primitive.construct(vec![], None).is_err(),
        "JS_CONSTRUCT native construct operation requires an object result"
    );
}
