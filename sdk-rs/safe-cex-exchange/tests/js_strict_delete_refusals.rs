use safe_cex_exchange::js_runtime::{
    globals,
    value::{Js, utf16},
};
use serde_json::{Value, json};
use std::{process::Command, rc::Weak};
fn source(mode: &str, missing: bool) -> Value {
    let output=Command::new("node").args(["-e",r#"'use strict';const token={},target={held:token},marker={after:false};const mode=process.argv[1],key=process.argv[2]==='true'?'missing':'held';if(mode==='freeze')Object.freeze(target);if(mode==='seal')Object.seal(target);let error=null;try{delete target[key];marker.after=true}catch(e){error=e.name}console.log(JSON.stringify({after:marker.after,error,held:Object.hasOwn(target,'held'),identity:target.held===token}));"#,mode,&missing.to_string()]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn native(mode: &str, missing: bool) -> Value {
    let g = Js::object(vec![]);
    globals::install_builtins::<()>(&g, Weak::new());
    let object = g.get("Object").unwrap();
    let token = Js::object(vec![]);
    let target = Js::object(vec![("held", token.clone())]);
    let marker = Js::object(vec![("after", Js::Bool(false))]);
    if mode == "freeze" {
        object.call_method("freeze", vec![target.clone()]).unwrap();
    }
    if mode == "seal" {
        object
            .call_method(
                "defineProperty",
                vec![
                    target.clone(),
                    Js::str("held"),
                    Js::object(vec![("configurable", Js::Bool(false))]),
                ],
            )
            .unwrap();
        object
            .call_method("preventExtensions", vec![target.clone()])
            .unwrap();
        assert!(
            !object
                .call_method("isExtensible", vec![target.clone()])
                .unwrap()
                .truthy()
        );
        let d = object
            .call_method(
                "getOwnPropertyDescriptor",
                vec![target.clone(), Js::str("held")],
            )
            .unwrap();
        assert!(!d.get("configurable").unwrap().truthy());
        assert!(d.get("writable").unwrap().truthy());
    }
    target.delete(&utf16(if missing { "missing" } else { "held" }));
    marker.set("after", Js::Bool(true)).unwrap();
    json!({"after":marker.get("after").unwrap().truthy(),"held":target.has_own("held"),"identity":target.get("held").unwrap().same_reference(&token)})
}
#[test]
fn strict_delete_of_frozen_property_stops_before_the_following_write() {
    let expected = source("freeze", false);
    assert_eq!(expected["error"], "TypeError");
    let actual = native("freeze", false);
    assert_eq!(
        actual["after"], expected["after"],
        "DELETE_STRICT frozen property must throw before subsequent source statement"
    );
    assert_eq!(actual["held"], expected["held"]);
    assert_eq!(actual["identity"], expected["identity"]);
}
#[test]
fn strict_delete_of_sealed_property_stops_before_the_following_write() {
    let expected = source("seal", false);
    assert_eq!(expected["error"], "TypeError");
    let actual = native("seal", false);
    assert_eq!(
        actual["after"], expected["after"],
        "DELETE_STRICT sealed property must throw before subsequent source statement"
    );
    assert_eq!(actual["held"], expected["held"]);
    assert_eq!(actual["identity"], expected["identity"]);
}
#[test]
fn configurable_and_missing_delete_controls_complete_and_keep_expected_referents() {
    for (mode, missing) in [("ordinary", false), ("freeze", true), ("seal", true)] {
        let expected = source(mode, missing);
        assert_eq!(expected["error"], Value::Null);
        let actual = native(mode, missing);
        for key in ["after", "held", "identity"] {
            assert_eq!(
                actual[key], expected[key],
                "DELETE_STRICT admitted source control {mode}/{missing}/{key}"
            );
        }
    }
}
