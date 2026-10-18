use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
use safe_cex_num::{ProtoMember, PrototypeRealm};
use serde_json::{Value, json};
use std::{
    cell::{Cell, OnceCell, RefCell},
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

#[test]
fn ordinary_has_instance_matches_node_prototypes_bound_functions_and_primitives() {
    let globals = Js::object(vec![]);
    safe_cex_exchange::js_runtime::globals::install_builtins::<()>(&globals, std::rc::Weak::new());
    let error = globals.get("Error").unwrap();
    let prototype = error.get("prototype").unwrap();
    let inherited = Js::object(vec![]);
    globals
        .get("Object")
        .unwrap()
        .call_method("setPrototypeOf", vec![inherited.clone(), prototype])
        .unwrap();
    let detached = error.construct(vec![Js::str("message")], None).unwrap();
    globals
        .get("Object")
        .unwrap()
        .call_method("setPrototypeOf", vec![detached.clone(), Js::Null])
        .unwrap();
    let bound = error.call_method("bind", vec![Js::Null]).unwrap();
    let twice = bound.call_method("bind", vec![Js::Null]).unwrap();
    bound.define("prototype", Js::Number(7.)).unwrap();
    inherited
        .define_accessor(
            "__proto__",
            Some(Rc::new(|_| {
                panic!("internal prototype walk must not read __proto__")
            })),
            None,
        )
        .unwrap();
    let ordinary_error = error.construct(vec![], None).unwrap();
    let invalid = Js::function("Invalid", |_, _| Ok(Js::Undefined));
    invalid.define("prototype", Js::Number(7.)).unwrap();
    let arrow = Js::function("Arrow", |_, _| Ok(Js::Undefined));
    arrow
        .define("prototype", error.get("prototype").unwrap())
        .unwrap();
    let bound_arrow = arrow.call_method("bind", vec![Js::Null]).unwrap();
    let mut actual = Vec::new();
    for (constructor, value) in [
        (error.clone(), ordinary_error),
        (error.clone(), inherited.clone()),
        (bound, inherited.clone()),
        (twice, inherited.clone()),
        (error.clone(), detached),
        (error.clone(), Js::Null),
        (error.clone(), Js::Undefined),
        (error.clone(), Js::Number(1.)),
        (error.clone(), Js::str("text")),
        (Js::object(vec![]), inherited.clone()),
        (invalid.clone(), Js::Number(1.)),
        (invalid, inherited.clone()),
        (bound_arrow, inherited),
    ] {
        actual.push(match constructor.ordinary_has_instance(&value) {
            Ok(value) => json!(value),
            Err(error) => json!({"throw":error.value.get("name").unwrap().to_display(),"message":error.value.get("message").unwrap().to_display()}),
        });
    }
    let script = r#"
const inherited=Object.create(Error.prototype), detached=new Error('message');
Object.setPrototypeOf(detached,null);
const bound=Error.bind(null), twice=bound.bind(null), invalid=function Invalid(){};
bound.prototype=7;Object.defineProperty(inherited,'__proto__',{get(){throw Error('shadowed');}});
invalid.prototype=7;const arrow=()=>{};arrow.prototype=Error.prototype;const boundArrow=arrow.bind(null);
const cases=[[Error,new Error()],[Error,inherited],[bound,inherited],[twice,inherited],
[Error,detached],[Error,null],[Error,undefined],[Error,1],[Error,'text'],[{},inherited],
[invalid,1],[invalid,inherited],[boundArrow,inherited]];
console.log(JSON.stringify(cases.map(([constructor,value])=>{
try{return Function.prototype[Symbol.hasInstance].call(constructor,value);}
catch(error){return {throw:error.name,message:error.message};}})));
"#;
    let output = Command::new("node").args(["-e", script]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json!(actual), expected, "ordinary_has_instance_node_cases");
    let reason = Js::object(vec![("opaque", Js::utf16(&[0xd800]))]);
    let thrown = reason.clone();
    let getter = Js::function("Getter", |_, _| Ok(Js::Undefined));
    getter
        .define_accessor(
            "prototype",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .unwrap();
    assert!(
        !getter.ordinary_has_instance(&Js::Number(1.)).unwrap(),
        "primitive LHS skips prototype getter"
    );
    assert!(
        getter
            .ordinary_has_instance(&Js::object(vec![]))
            .unwrap_err()
            .value
            .strict_equals(&reason)
    );
}
