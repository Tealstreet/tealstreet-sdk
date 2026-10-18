use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use safe_cex_num::{ProtoMember, PrototypeRealm};
use serde_json::{Value, json};
use std::{
    cell::{Cell, OnceCell, RefCell},
    process::Command,
    rc::Rc,
};

#[test]
fn instance_of_uses_canonical_prototypes_bound_targets_and_original_getters() {
    let constructor = Js::constructor("Base", None, |target, _| target.allocate_instance());
    let prototype = Js::object(vec![]);
    constructor.define("prototype", prototype.clone()).unwrap();
    let instance = constructor.construct(vec![], None).unwrap();
    let bound = constructor.call_method("bind", vec![Js::Undefined]).unwrap();
    let other = Js::object(vec![]);
    let mut actual = vec![instance.instance_of(&constructor).unwrap(), instance.instance_of(&bound).unwrap(), other.instance_of(&constructor).unwrap(), Js::Number(1.0).instance_of(&constructor).unwrap()];
    let reads = Rc::new(Cell::new(0));
    let observed = reads.clone();
    let cause = Js::object(vec![]);
    let thrown = cause.clone();
    let inspected = Js::function("inspect", |_, _| Ok(Js::Undefined));
    inspected.define_accessor("prototype", Some(Rc::new(move |_| { observed.set(observed.get() + 1); Err(Thrown::new(thrown.clone())) })), None).unwrap();
    actual.push(Js::Null.instance_of(&inspected).unwrap());
    assert_eq!(reads.get(), 0, "primitive lhs does not inspect a function prototype");
    assert!(instance.instance_of(&inspected).unwrap_err().value.same_reference(&cause));
    assert_eq!(reads.get(), 1);
    for constructor in [Js::Null, Js::Number(1.0), Js::object(vec![])] {
        assert_eq!(instance.instance_of(&constructor).unwrap_err().value.get("name").unwrap().to_display(), "TypeError");
    }
    let source = Command::new("node").args(["-e", "function Base(){};const instance=new Base(),bound=Base.bind(undefined);const trace=[];const inspected=new Proxy(function(){},{get(t,k,r){if(k==='prototype'){trace.push(k);throw cause;}return Reflect.get(t,k,r);}}),cause={};const values=[instance instanceof Base,instance instanceof bound,{} instanceof Base,1 instanceof Base,null instanceof inspected];if(trace.length)throw Error('primitive read');try{instance instanceof inspected;}catch(error){if(error!==cause)throw error;}if(trace.length!==1)throw Error('getter count');console.log(JSON.stringify(values));"]).output().unwrap();
    assert!(source.status.success(), "{}", String::from_utf8_lossy(&source.stderr));
    assert_eq!(json!(actual), serde_json::from_slice::<Value>(&source.stdout).unwrap());
}

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
fn callable_error_constructor_keeps_message_units_and_construct_order() {
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let error = globals.get("Error").unwrap();
    assert!(error.is_callable() && error.is_constructor());
    assert!(
        error.own_keys().is_empty(),
        "JS_CONSTRUCT Error prototype remains nonenumerable"
    );
    for result in [
        error.call(Js::Undefined, vec![Js::utf16(&[0xd800])]),
        error.construct(vec![Js::utf16(&[0xd800])], None),
    ] {
        let value = result.unwrap();
        assert_eq!(
            value.get("message").unwrap().as_str().unwrap().as_ref(),
            &[0xd800]
        );
        assert_eq!(value.get("name").unwrap().to_display(), "Error");
        assert!(value.own_keys().is_empty());
    }
    let trace = Rc::new(RefCell::new(Vec::new()));
    let target = Js::constructor("Target", None, |target, _| target.allocate_instance());
    let events = trace.clone();
    let prototype = error.get("prototype").unwrap();
    target
        .define_accessor(
            "prototype",
            Some(Rc::new(move |_| {
                events.borrow_mut().push("prototype");
                Ok(prototype.clone())
            })),
            None,
        )
        .expect("test accessor target permits definition");
    let events = trace.clone();
    let message = Js::object(vec![(
        "toString",
        Js::function("toString", move |_, _| {
            events.borrow_mut().push("message");
            Ok(Js::str("text"))
        }),
    )]);
    assert_eq!(
        error
            .construct(vec![message], Some(&target))
            .unwrap()
            .get("message")
            .unwrap()
            .to_display(),
        "text"
    );
    let output = Command::new("node").arg("-e").arg("const trace=[]; const target=new Proxy(function Target(){},{get(t,k,r){if(k==='prototype') {trace.push('prototype'); return Error.prototype;} return Reflect.get(t,k,r);}});Reflect.construct(Error,[{toString(){trace.push('message');return 'text';}}],target);console.log(JSON.stringify(trace));").output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        json!(*trace.borrow()),
        serde_json::from_slice::<Value>(&output.stdout).unwrap(),
        "JS_CONSTRUCT Error reads prototype before message coercion"
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
        let superclass = active.get().unwrap().super_constructor()?;
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
    assert!(
        explicit.get("target").unwrap().same_reference(&derived),
        "JS_CONSTRUCT original explicit new.target"
    );
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
    derived.define("__proto__", token.clone()).unwrap();
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
