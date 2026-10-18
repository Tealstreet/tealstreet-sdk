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
    let deletion = (|| {
        target.delete(&utf16(if missing { "missing" } else { "held" }))?;
        marker.set("after", Js::Bool(true))
    })();
    let error = deletion
        .err()
        .map(|error| error.value.get("name").unwrap().to_display());
    json!({"error":error,"after":marker.get("after").unwrap().truthy(),"held":target.has_own("held"),"identity":target.get("held").unwrap().same_reference(&token)})
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
    assert_eq!(actual["error"], expected["error"]);
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
    assert_eq!(actual["error"], expected["error"]);
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

#[test]
fn strict_and_reflective_deletion_share_descriptors_without_reading_getters() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    use std::{cell::Cell, rc::Rc};
    let _: fn(&Js, &[u16]) -> Result<(), Thrown> = Js::delete;
    let _: fn(&Js, &[u16]) -> Result<bool, Thrown> = Js::reflect_delete;
    let output = Command::new("node").args(["-e",r#"'use strict';const rows=[];for(const fn of [false,true])for(const accessor of [false,true])for(const configurable of [false,true])for(const reflective of [false,true]){let reads=0;const target=fn?(()=>{}):{},token={},key=String.fromCharCode(0xd800),get=()=>{reads++;throw token};Object.defineProperty(target,key,accessor?{get,configurable}:{value:token,configurable});let result=null,error=null;try{if(reflective)result=Reflect.deleteProperty(target,key);else{delete target[key];result=true}}catch(e){error=e.name}const descriptor=Object.getOwnPropertyDescriptor(target,key);rows.push({fn,accessor,configurable,reflective,result,error,own:Object.hasOwn(target,key),reads,identity:!!descriptor&&(accessor?descriptor.get===get:descriptor.value===token)});}console.log(JSON.stringify(rows));"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 16);
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    let object = globals.get("Object").unwrap();
    for row in rows {
        let target = if row["fn"] == true {
            Js::function("target", |_, _| Ok(Js::Undefined))
        } else {
            Js::object(vec![])
        };
        let token = Js::object(vec![]);
        let count = Rc::new(Cell::new(0));
        let reads = count.clone();
        let thrown = token.clone();
        let getter = Js::function("get", move |_, _| {
            reads.set(reads.get() + 1);
            Err(Thrown::new(thrown.clone()))
        });
        let accessor = row["accessor"] == true;
        let descriptor = Js::object(vec![
            (
                if accessor { "get" } else { "value" },
                if accessor {
                    getter.clone()
                } else {
                    token.clone()
                },
            ),
            ("configurable", Js::Bool(row["configurable"] == true)),
        ]);
        let key = [0xd800];
        object
            .call_method(
                "defineProperty",
                vec![target.clone(), Js::utf16(&key), descriptor],
            )
            .unwrap();
        let completion = if row["reflective"] == true {
            target.reflect_delete(&key)
        } else {
            target.delete(&key).map(|()| true)
        };
        let (result, error) = match completion {
            Ok(value) => (json!(value), Value::Null),
            Err(error) => (
                Value::Null,
                json!(error.value.get("name").unwrap().to_display()),
            ),
        };
        let descriptor = object
            .call_method(
                "getOwnPropertyDescriptor",
                vec![target.clone(), Js::utf16(&key)],
            )
            .unwrap();
        let identity = !descriptor.is_undefined()
            && descriptor
                .get(if accessor { "get" } else { "value" })
                .unwrap()
                .same_reference(if accessor { &getter } else { &token });
        assert_eq!(
            json!({"result":result,"error":error,"own":!descriptor.is_undefined(),"reads":count.get(),"identity":identity}),
            json!({"result":row["result"],"error":row["error"],"own":row["own"],"reads":row["reads"],"identity":row["identity"]}),
            "DELETE_MODE actual descriptor profile {row}"
        );
    }
}

#[test]
fn deletion_preserves_inherited_aliases_array_holes_and_key_order() {
    let output=Command::new("node").args(["-e",r#"'use strict';let reads=0;const parent={get inherited(){reads++;throw Error('must not read')}},child=Object.create(parent),token={},array=[token,-0];const removed=Reflect.deleteProperty(array,'0');let arrayError=null;try{delete array.length}catch(e){arrayError=e.name}const map={keep:token,'2':2,'1':1,'tail':-0};delete map['2'];map['2']=2;console.log(JSON.stringify({inherited:Reflect.deleteProperty(child,'inherited'),reads,removed,hole:!(0 in array),length:array.length,same:array[1]===0&&Object.is(array[1],-0),arrayReflect:Reflect.deleteProperty(array,'length'),arrayError,keys:Object.keys(map),identity:map.keep===token}));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    let object = globals.get("Object").unwrap();
    let parent = Js::object(vec![]);
    let count = std::rc::Rc::new(std::cell::Cell::new(0));
    let reads = count.clone();
    parent
        .define_accessor(
            "inherited",
            Some(std::rc::Rc::new(move |_| {
                reads.set(reads.get() + 1);
                Err(safe_cex_exchange::js_runtime::value::Thrown::type_error(
                    "must not read",
                ))
            })),
            None,
        )
        .unwrap();
    let child = Js::object(vec![]);
    object
        .call_method("setPrototypeOf", vec![child.clone(), parent.clone()])
        .unwrap();
    let token = Js::object(vec![]);
    let array = Js::array(vec![token.clone(), Js::Number(-0.)]);
    let alias = array.clone();
    let removed = array.reflect_delete(&utf16("0")).unwrap();
    let array_error = array.delete(&utf16("length")).unwrap_err();
    let map = Js::object(vec![
        ("keep", token.clone()),
        ("2", Js::Number(2.)),
        ("1", Js::Number(1.)),
        ("tail", Js::Number(-0.)),
    ]);
    map.delete(&utf16("2")).unwrap();
    map.set("2", Js::Number(2.)).unwrap();
    assert_eq!(
        json!({"inherited":child.reflect_delete(&utf16("inherited")).unwrap(),"reads":count.get(),"removed":removed,"hole":!alias.has_own("0"),"length":alias.get("length").unwrap().to_number().unwrap() as u64,"same":alias.get("1").unwrap().to_number().unwrap().to_bits()==(-0.0f64).to_bits(),"arrayReflect":array.reflect_delete(&utf16("length")).unwrap(),"arrayError":array_error.value.get("name").unwrap().to_display(),"keys":map.own_keys().iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>(),"identity":map.get("keep").unwrap().same_reference(&token)}),
        expected
    );
    assert!(array.same_reference(&alias));
    assert!(parent.has_own("inherited"));
}

#[test]
fn strict_primitive_and_reflective_target_rules_match_node() {
    let output=Command::new("node").args(["-e",r#"'use strict';const rows=[];for(const target of [null,undefined,true,3,1n,Symbol.iterator,'𝌆'])for(const key of ['0','1','2','length','missing'])for(const reflective of [false,true]){let result=null,error=null;try{result=reflective?Reflect.deleteProperty(target,key):delete target[key]}catch(e){error=e.name}rows.push({result,error})}console.log(JSON.stringify(rows));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut actual = vec![];
    for target in [
        Js::Null,
        Js::Undefined,
        Js::Bool(true),
        Js::Number(3.),
        Js::BigInt(std::rc::Rc::new(1.into())),
        Js::SymbolIterator,
        Js::str("𝌆"),
    ] {
        for key in ["0", "1", "2", "length", "missing"] {
            for reflective in [false, true] {
                let result = if reflective {
                    target.reflect_delete(&utf16(key))
                } else {
                    target.delete(&utf16(key)).map(|()| true)
                };
                actual.push(match result{Ok(value)=>json!({"result":value,"error":Value::Null}),Err(error)=>json!({"result":Value::Null,"error":error.value.get("name").unwrap().to_display()})});
            }
        }
    }
    assert_eq!(actual.len(), 70);
    assert_eq!(
        actual, expected,
        "DELETE_MODE primitive boxing versus reflective object-only rule"
    );
}

#[test]
fn intrinsic_deletion_refuses_visibly_without_claiming_a_mutation() {
    let globals = Js::object(vec![]);
    globals::install_builtins::<()>(&globals, Weak::new());
    let prototype = globals.get("Object").unwrap().get("prototype").unwrap();
    let original = prototype.get("toString").unwrap();
    for reflective in [false, true] {
        let error = if reflective {
            prototype.reflect_delete(&utf16("toString")).unwrap_err()
        } else {
            prototype.delete(&utf16("toString")).unwrap_err()
        };
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        assert_eq!(
            error.message(),
            "Intrinsic property deletion is unsupported"
        );
        assert!(prototype.get("toString").unwrap().same_reference(&original));
    }
    assert!(prototype.reflect_delete(&utf16("missing")).unwrap());
    prototype.delete(&utf16("missing")).unwrap();
}
