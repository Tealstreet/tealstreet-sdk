#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc, sync::Arc};

fn object_builtin() -> (Js, Rc<ScopeShared<()>>) {
    let runtime = native_runtime();
    let http = client::rest_http_host(
        runtime.clone(),
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
    );
    let shared = Rc::new(ScopeShared::new(
        runtime,
        http,
        Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
        Js::object(vec![]),
        (),
    ));
    globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
    (shared.globals.get("Object").unwrap(), shared)
}
fn source(function: bool, integrity: &str, replace: bool) -> Value {
    let script = r#"'use strict';
const target = process.argv[1]==='true' ? function target(){} : {};
target.prior='original';
Object[process.argv[2]](target);
const key=process.argv[3]==='true'?'prior':'added';
let error;
try { Object.defineProperty(target,key,{get(){return 'replacement'},enumerable:true,configurable:true}); }
catch(e) { error=e.name; }
console.log(JSON.stringify({error,prior:target.prior,added:Object.hasOwn(target,'added'),keys:Object.keys(target)}));"#;
    let output = Command::new("node")
        .args([
            "-e",
            script,
            &function.to_string(),
            integrity,
            &replace.to_string(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["error"], "TypeError",
        "ACCESSOR_INSTALL independent source refuses mutation"
    );
    value
}
fn native_state(function: bool, integrity: &str, replace: bool) -> Value {
    let (object, _shared) = object_builtin();
    let target = if function {
        Js::function("target", |_, _| Ok(Js::Undefined))
    } else {
        Js::object(vec![])
    };
    target.set("prior", Js::str("original")).unwrap();
    object.call_method(integrity, vec![target.clone()]).unwrap();
    let error = target
        .define_accessor(
            if replace { "prior" } else { "added" },
            Some(Rc::new(|_| Ok(Js::str("replacement")))),
            None,
        )
        .expect_err("integrity must refuse native accessor installation");
    assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    json!({"prior":target.get("prior").unwrap().to_display(),"added":target.has_own("added"),
        "keys":target.own_keys().iter().map(|k|String::from_utf16(k).unwrap()).collect::<Vec<_>>()})
}

#[test]
fn frozen_object_rejects_native_accessor_addition_without_mutation() {
    let source = source(false, "freeze", false);
    let actual = native_state(false, "freeze", false);
    assert_eq!(
        actual["added"], source["added"],
        "ACCESSOR_INSTALL frozen object cannot gain native accessor"
    );
    assert_eq!(actual["keys"], source["keys"]);
}
#[test]
fn frozen_function_rejects_native_accessor_replacement_without_mutation() {
    let source = source(true, "freeze", true);
    let actual = native_state(true, "freeze", true);
    assert_eq!(
        actual["prior"], source["prior"],
        "ACCESSOR_INSTALL nonconfigurable function data stays intact"
    );
}
#[test]
fn nonextensible_object_rejects_native_accessor_addition_without_mutation() {
    let source = source(false, "preventExtensions", false);
    let actual = native_state(false, "preventExtensions", false);
    assert_eq!(
        actual["keys"], source["keys"],
        "ACCESSOR_INSTALL nonextensible object retains exact keys"
    );
}

#[test]
fn configurable_accessor_replacement_clears_omitted_slots_like_node() {
    let output = Command::new("node")
        .args(["-e", r#"const o={}; const define=(get,set)=>Object.defineProperty(o,'x',{get,set,enumerable:true,configurable:true}); define(()=>7,()=>{}); define(undefined,()=>{}); const a=Object.getOwnPropertyDescriptor(o,'x'); define(()=>9,undefined); const b=Object.getOwnPropertyDescriptor(o,'x'); define(undefined,undefined); const c=Object.getOwnPropertyDescriptor(o,'x'); console.log(JSON.stringify([a.get===undefined,typeof a.set,b.set===undefined,b.get(),c.get===undefined,c.set===undefined,c.enumerable,c.configurable,Object.keys(o)]));"#])
        .output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let (object, _shared) = object_builtin();
    let target = Js::object(vec![]);
    target
        .define_accessor(
            "x",
            Some(Rc::new(|_| Ok(Js::Number(7.0)))),
            Some(Rc::new(|_, _| Ok(()))),
        )
        .unwrap();
    target
        .define_accessor("x", None, Some(Rc::new(|_, _| Ok(()))))
        .unwrap();
    let descriptor = || {
        object
            .call_method(
                "getOwnPropertyDescriptor",
                vec![target.clone(), Js::str("x")],
            )
            .unwrap()
    };
    let a = descriptor();
    target
        .define_accessor("x", Some(Rc::new(|_| Ok(Js::Number(9.0)))), None)
        .unwrap();
    let b = descriptor();
    target.define_accessor("x", None, None).unwrap();
    let c = descriptor();
    assert_eq!(
        json!([
            a.get("get").unwrap().is_undefined(),
            if a.get("set").unwrap().is_callable() {
                "function"
            } else {
                "undefined"
            },
            b.get("set").unwrap().is_undefined(),
            b.get("get")
                .unwrap()
                .call(target.clone(), vec![])
                .unwrap()
                .to_number()
                .unwrap() as i64,
            c.get("get").unwrap().is_undefined(),
            c.get("set").unwrap().is_undefined(),
            c.get("enumerable").unwrap().truthy(),
            c.get("configurable").unwrap().truthy(),
            target
                .own_keys()
                .iter()
                .map(|k| String::from_utf16(k).unwrap())
                .collect::<Vec<_>>()
        ]),
        expected
    );
}

#[test]
fn inherited_accessors_keep_receiver_native_bits_and_original_throw() {
    let (object, _shared) = object_builtin();
    let parent = Js::function("parent", |_, _| Ok(Js::Undefined));
    parent
        .define_accessor(
            "x",
            Some(Rc::new(|this| this.get("value"))),
            Some(Rc::new(|this, value| this.set("value", value))),
        )
        .unwrap();
    let bits = 0x7ff8_0000_0000_0042;
    let child = Js::object(vec![("value", Js::Number(f64::from_bits(bits)))]);
    object
        .call_method("setPrototypeOf", vec![child.clone(), parent.clone()])
        .unwrap();
    assert_eq!(child.get("x").unwrap().to_number().unwrap().to_bits(), bits);
    child.set("x", Js::Number(-0.0)).unwrap();
    assert_eq!(
        child.get("value").unwrap().to_number().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(parent.get("value").unwrap().is_undefined());
    let original = Js::object(vec![]);
    let get_error = original.clone();
    let set_error = original.clone();
    parent
        .define_accessor(
            "failure",
            Some(Rc::new(move |_| {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    get_error.clone(),
                ))
            })),
            Some(Rc::new(move |_, _| {
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    set_error.clone(),
                ))
            })),
        )
        .unwrap();
    assert!(
        child
            .get("failure")
            .unwrap_err()
            .value
            .same_reference(&original)
    );
    assert!(
        child
            .set("failure", Js::Null)
            .unwrap_err()
            .value
            .same_reference(&original)
    );
    let source = Command::new("node").args(["-e",r#"const p=function(){}, c={value:NaN}, token={}; Object.defineProperty(p,'x',{get(){return this.value},set(v){this.value=v},enumerable:true,configurable:true}); Object.setPrototypeOf(c,p); if(!Number.isNaN(c.x))throw Error('receiver'); c.x=-0; if(!Object.is(c.value,-0)||p.value!==undefined)throw Error('write'); Object.defineProperty(p,'failure',{get(){throw token},set(){throw token},enumerable:true,configurable:true}); let n=0; try{c.failure}catch(e){if(e===token)n++} try{c.failure=null}catch(e){if(e===token)n++} if(n!==2)throw Error('identity');"#]).output().unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
}

#[test]
fn non_object_accessor_targets_return_explicit_type_error() {
    for target in [
        Js::Null,
        Js::Undefined,
        Js::Bool(true),
        Js::Number(2.0),
        Js::str("x"),
    ] {
        let error = target.define_accessor("x", None, None).unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
    }
    let output = Command::new("node").args(["-e", "for(const target of [null,undefined,true,2,'x']){let error;try{Object.defineProperty(target,'x',{get:undefined,set:undefined,enumerable:true,configurable:true})}catch(e){error=e}if(error?.name!=='TypeError')throw Error('refusal')} "]).output().unwrap();
    assert!(output.status.success());
}
