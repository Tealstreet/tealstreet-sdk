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
        Rc::new(safe_cex_store::store::DefaultStore::new()),
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
    target.define_accessor(
        if replace { "prior" } else { "added" },
        Some(Rc::new(|_| Ok(Js::str("replacement")))),
        None,
    );
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
