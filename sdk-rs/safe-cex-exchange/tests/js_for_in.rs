use safe_cex_exchange::js_runtime::{
    globals,
    value::{Js, to_json},
};
use serde_json::Value;
use std::{process::Command, rc::Weak};
fn object() -> Js {
    let g = Js::object(vec![]);
    globals::install_builtins::<()>(&g, Weak::new());
    g.get("Object").unwrap()
}
#[test]
fn for_in_requires_canonical_hidden_function_name_and_length_descriptors() {
    let output=Command::new("node").args(["-e",r#"const f=()=>{};console.log(JSON.stringify(['length','name'].map(k=>{const d=Object.getOwnPropertyDescriptor(f,k);return {key:k,enumerable:d.enumerable,configurable:d.configurable,writable:d.writable}})));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let object = object();
    let function = Js::function("f", |_, _| Ok(Js::Undefined));
    let names = function.own_property_names();
    assert!(
        names.is_ok(),
        "FOR_IN_METADATA native function must expose canonical nonenumerable names"
    );
    let actual:Vec<Value>=["length","name"].into_iter().map(|key|{
        assert!(names.as_ref().unwrap().iter().any(|name|**name==key.encode_utf16().collect::<Vec<_>>()));
        let d=object.call_method("getOwnPropertyDescriptor",vec![function.clone(),Js::str(key)]).unwrap();
        serde_json::json!({"key":key,"enumerable":to_json(&d.get("enumerable").unwrap()).unwrap(),"configurable":to_json(&d.get("configurable").unwrap()).unwrap(),"writable":to_json(&d.get("writable").unwrap()).unwrap()})
    }).collect();
    assert_eq!(
        serde_json::json!(actual),
        expected,
        "FOR_IN_METADATA source function shadowing descriptors"
    );
}
