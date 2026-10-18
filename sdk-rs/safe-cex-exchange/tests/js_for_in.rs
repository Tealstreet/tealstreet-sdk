use safe_cex_exchange::js_runtime::{
    globals,
    value::{Js, to_json},
};
use serde_json::Value;
use std::{
    process::Command,
    rc::{Rc, Weak},
};
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

#[test]
fn for_in_record_preserves_native_order_hidden_shadows_and_does_not_get_values() {
    use safe_cex_exchange::js_runtime::for_in::ForInRecord;
    use std::{cell::Cell, rc::Rc};
    let out=Command::new("node").args(["-e",r#"const proto={inherited:1,shadow:2};const o=Object.create(proto);o['2']=2;o['1']=1;o['01']=1;o.own=undefined;let reads=0;Object.defineProperty(o,'shadow',{get(){reads++;throw Error('must not read')}});Object.defineProperty(o,'getter',{enumerable:true,get(){reads++;throw Error('must not read')}});const keys=[];for(const k in o)keys.push(k);console.log(JSON.stringify({keys,reads}));"#]).output().unwrap();
    assert!(out.status.success());
    let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
    let object = object();
    let proto = Js::object(vec![
        ("inherited", Js::Number(1.)),
        ("shadow", Js::Number(2.)),
    ]);
    let o = Js::object(vec![
        ("2", Js::Number(2.)),
        ("1", Js::Number(1.)),
        ("01", Js::Number(1.)),
        ("own", Js::Undefined),
    ]);
    object
        .call_method("setPrototypeOf", vec![o.clone(), proto])
        .unwrap();
    let reads = Rc::new(Cell::new(0));
    for (key, enumerable) in [("shadow", false), ("getter", true)] {
        let observed = reads.clone();
        object
            .call_method(
                "defineProperty",
                vec![
                    o.clone(),
                    Js::str(key),
                    Js::object(vec![
                        ("enumerable", Js::Bool(enumerable)),
                        (
                            "get",
                            Js::function("get", move |_, _| {
                                observed.set(observed.get() + 1);
                                panic!("FOR_IN must not invoke a getter")
                            }),
                        ),
                    ]),
                ],
            )
            .unwrap();
    }
    let mut record = ForInRecord::get(&o);
    let mut keys = vec![];
    while let Some(key) = record.next_key().unwrap() {
        keys.push(String::from_utf16(&key).unwrap());
    }
    assert!(record.next_key().unwrap().is_none());
    assert_eq!(
        serde_json::json!({"keys":keys,"reads":reads.get()}),
        expected,
        "FOR_IN source order and nonenumerable shadows"
    );
}

#[test]
fn for_in_record_observes_deletion_before_visitation_and_keeps_undefined_presence() {
    use safe_cex_exchange::js_runtime::{for_in::ForInRecord, value::utf16};
    let out=Command::new("node").args(["-e",r#"const p={inherited:undefined};const o=Object.assign(Object.create(p),{first:1,later:2,last:undefined});const keys=[];for(const k in o){keys.push(k);if(k==='first')delete o.later;}console.log(JSON.stringify(keys));"#]).output().unwrap();
    let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
    let object = object();
    let o = Js::object(vec![
        ("first", Js::Number(1.)),
        ("later", Js::Number(2.)),
        ("last", Js::Undefined),
    ]);
    object
        .call_method(
            "setPrototypeOf",
            vec![o.clone(), Js::object(vec![("inherited", Js::Undefined)])],
        )
        .unwrap();
    let mut record = ForInRecord::get(&o);
    let mut keys = vec![];
    while let Some(key) = record.next_key().unwrap() {
        if key.as_ref() == utf16("first").as_ref() {
            o.delete(&utf16("later"));
        }
        keys.push(String::from_utf16(&key).unwrap());
    }
    assert_eq!(
        serde_json::json!(keys),
        expected,
        "FOR_IN unvisited configurable deletion"
    );
}

#[test]
fn for_in_record_traverses_function_and_intrinsic_prototypes_without_leaking_hidden_names() {
    use safe_cex_exchange::js_runtime::for_in::ForInRecord;
    let out=Command::new("node").args(["-e",r#"Object.prototype.apply=1;const p={name:1,length:1,visible:1};const f=()=>{};Object.setPrototypeOf(p,Function.prototype);Object.setPrototypeOf(f,p);f.own=undefined;const keys=[];for(const k in f)keys.push(k);console.log(JSON.stringify(keys));"#]).output().unwrap();
    let expected: Value = serde_json::from_slice(&out.stdout).unwrap();
    let realm = Rc::new(safe_cex_num::PrototypeRealm::default());
    let _binding = realm.bind();
    let object = object();
    let f = Js::function("f", |_, _| Ok(Js::Undefined));
    let proto = Js::object(vec![
        ("name", Js::Number(1.)),
        ("length", Js::Number(1.)),
        ("visible", Js::Number(1.)),
    ]);
    let function_proto = f.super_constructor().unwrap();
    object
        .call_method("setPrototypeOf", vec![proto.clone(), function_proto])
        .unwrap();
    object
        .call_method("setPrototypeOf", vec![f.clone(), proto])
        .unwrap();
    f.set("own", Js::Undefined).unwrap();
    let object_proto = object.get("prototype").unwrap();
    object_proto.set("apply", Js::Number(1.)).unwrap();
    let mut record = ForInRecord::get(&f);
    let mut keys = vec![];
    while let Some(key) = record.next_key().unwrap() {
        keys.push(String::from_utf16(&key).unwrap());
    }
    assert_eq!(
        serde_json::json!(keys),
        expected,
        "FOR_IN function name/length and intrinsic apply shadow inherited enumerables"
    );
}

#[test]
fn for_in_record_uses_utf16_indices_holes_and_nullish_empty_iteration() {
    use safe_cex_exchange::js_runtime::for_in::ForInRecord;
    for (input, expected) in [
        (Js::Null, vec![]),
        (Js::Undefined, vec![]),
        (Js::Number(2.), vec![]),
        (Js::utf16(&[0xd800, 0x61]), vec!["0", "1"]),
        (
            Js::array(vec![Js::Undefined, Js::Number(-0.)]),
            vec!["0", "1"],
        ),
    ] {
        let mut record = ForInRecord::get(&input);
        let mut keys = vec![];
        while let Some(key) = record.next_key().unwrap() {
            keys.push(String::from_utf16(&key).unwrap());
        }
        assert_eq!(keys, expected, "FOR_IN native value profile");
    }
}

#[test]
fn function_metadata_uses_ordinary_definition_deletion_and_readonly_assignment() {
    use safe_cex_exchange::js_runtime::{for_in::ForInRecord, value::utf16};
    let output=Command::new("node").args(["-e",r#"'use strict';const p={name:1,length:2};const f=()=>{};Object.setPrototypeOf(f,p);let refused=false;try{f.name='replacement'}catch(e){refused=e.name==='TypeError'};delete f.name;delete f.length;const keys=[];for(const k in f)keys.push(k);console.log(JSON.stringify({refused,keys,name:f.name,length:f.length}));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let object = object();
    let f = Js::function("f", |_, _| Ok(Js::Undefined));
    let p = Js::object(vec![("name", Js::Number(1.)), ("length", Js::Number(2.))]);
    object
        .call_method("setPrototypeOf", vec![f.clone(), p])
        .unwrap();
    let refused = f.set("name", Js::str("replacement")).is_err();
    f.delete(&utf16("name"));
    f.delete(&utf16("length"));
    let mut record = ForInRecord::get(&f);
    let mut keys = vec![];
    while let Some(key) = record.next_key().unwrap() {
        keys.push(String::from_utf16(&key).unwrap());
    }
    let name = f.get("name").unwrap();
    let length = f.get("length").unwrap();
    assert_eq!(name.to_number().unwrap().to_bits(), 1f64.to_bits());
    assert_eq!(length.to_number().unwrap().to_bits(), 2f64.to_bits());
    let actual = Js::object(vec![
        ("refused", Js::Bool(refused)),
        (
            "keys",
            Js::array(keys.iter().map(|key| Js::str(key)).collect()),
        ),
        ("name", name),
        ("length", length),
    ]);
    let rendered = globals::stringify_json(&actual).unwrap().unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&String::from_utf16(&rendered).unwrap()).unwrap(),
        expected,
        "FOR_IN_METADATA native bits and JS JSON deletion expose original prototype"
    );
}

#[test]
fn canonical_intrinsic_own_enumerability_matches_node_without_reading_values() {
    use safe_cex_num::{ProtoMember, PrototypeValue};
    let output=Command::new("node").args(["-e",r#"console.log(JSON.stringify([Object.prototype,Object,Function,Function.prototype,Object.prototype.hasOwnProperty,Function.prototype.call].map(value=>Object.getOwnPropertyNames(value).sort().map(key=>[key,Object.getOwnPropertyDescriptor(value,key).enumerable]))));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let realm = Rc::new(safe_cex_num::PrototypeRealm::default());
    let constructor = realm.reference(ProtoMember::Constructor);
    let function_proto = constructor.prototype().unwrap();
    let PrototypeValue::Reference(function) = constructor.get("constructor") else {
        panic!("Function constructor")
    };
    let PrototypeValue::Reference(call) = function_proto.get("call") else {
        panic!("Function call")
    };
    let native = [
        realm.reference(ProtoMember::Prototype),
        constructor,
        function,
        function_proto,
        realm.reference(ProtoMember::HasOwnProperty),
        call,
    ];
    let actual: Vec<_> = native
        .into_iter()
        .map(|value| {
            let mut properties = value.own_properties();
            properties.sort_by(|a, b| a.0.cmp(&b.0));
            properties
        })
        .collect();
    assert_eq!(
        serde_json::json!(actual),
        expected,
        "FOR_IN_METADATA actual stock descriptor names and enumerability"
    );
}

#[test]
fn for_in_record_retains_surrogate_keys_and_omits_actual_array_holes() {
    use safe_cex_exchange::js_runtime::{for_in::ForInRecord, value::utf16};
    let output=Command::new("node").args(["-e",r#"const o={'\ud800':undefined};const a=[undefined,-0];delete a[0];console.log(JSON.stringify([o,a].map(value=>{const keys=[];for(const key in value)keys.push(Array.from({length:key.length},(_,i)=>key.charCodeAt(i)));return keys})));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let o = Js::object(vec![]);
    o.define_key(&[0xd800], Js::Undefined).unwrap();
    let a = Js::array(vec![Js::Undefined, Js::Number(-0.)]);
    a.delete(&utf16("0"));
    let mut actual = vec![];
    for value in [o, a] {
        let mut record = ForInRecord::get(&value);
        let mut keys = vec![];
        while let Some(key) = record.next_key().unwrap() {
            keys.push(key.to_vec());
        }
        actual.push(keys);
    }
    assert_eq!(
        serde_json::json!(actual),
        expected,
        "FOR_IN UTF16 key identity and actual absent index"
    );
}

#[test]
fn bound_metadata_observes_own_length_before_name_and_preserves_utf16() {
    use safe_cex_exchange::js_runtime::{for_in::ForInRecord, value::utf16};
    use std::cell::RefCell;
    let output=Command::new("node").args(["-e",r#"const log=[];const f=()=>{};Object.defineProperty(f,'length',{get(){log.push('length');return 7.9}});Object.defineProperty(f,'name',{get(){log.push('name');return '\ud800'}});const bound=f.bind(null,1,2);console.log(JSON.stringify({log,name:Array.from({length:bound.name.length},(_,i)=>bound.name.charCodeAt(i)),length:bound.length,keys:Object.keys(bound)}));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let object = object();
    let f = Js::function("f", |_, _| Ok(Js::Undefined));
    let log = Rc::new(RefCell::new(Vec::<String>::new()));
    for name in ["length", "name"] {
        let observed = log.clone();
        object
            .call_method(
                "defineProperty",
                vec![
                    f.clone(),
                    Js::str(name),
                    Js::object(vec![
                        ("configurable", Js::Bool(true)),
                        (
                            "get",
                            Js::function("get", move |_, _| {
                                observed.borrow_mut().push(name.into());
                                Ok(if name == "length" {
                                    Js::Number(7.9)
                                } else {
                                    Js::utf16(&[0xd800])
                                })
                            }),
                        ),
                    ]),
                ],
            )
            .unwrap();
    }
    let bound = f
        .call_method("bind", vec![Js::Null, Js::Number(1.), Js::Number(2.)])
        .unwrap();
    let mut record = ForInRecord::get(&bound);
    assert!(record.next_key().unwrap().is_none());
    assert_eq!(
        bound.get("length").unwrap().to_number().unwrap().to_bits(),
        5f64.to_bits()
    );
    let actual = serde_json::json!({"log":*log.borrow(),"name":bound.get("name").unwrap().to_js_string().unwrap().as_ref(),"length":bound.get("length").unwrap().to_number().unwrap() as u64,"keys":bound.own_keys().iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>()});
    assert_eq!(
        actual, expected,
        "FOR_IN_METADATA bound descriptors follow source getter order and native bits"
    );
    bound.delete(&utf16("name"));
    assert!(!bound.has_own("name"));
}

#[test]
fn bound_length_uses_number_only_integer_conversion_and_positive_zero() {
    let output=Command::new("node").args(["-e",r#"const out=[0,-0,NaN,Infinity,-Infinity,2.9,'4',1n].map(length=>{const f=()=>{};Object.defineProperty(f,'length',{value:length});const v=f.bind(null).length;const bytes=new DataView(new ArrayBuffer(8));bytes.setFloat64(0,v);return bytes.getBigUint64(0).toString(16).padStart(16,'0')});console.log(JSON.stringify(out));"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    let object = object();
    let mut actual = vec![];
    for value in [
        Js::Number(0.),
        Js::Number(-0.),
        Js::Number(f64::NAN),
        Js::Number(f64::INFINITY),
        Js::Number(f64::NEG_INFINITY),
        Js::Number(2.9),
        Js::str("4"),
        Js::BigInt(Rc::new(1.into())),
    ] {
        let f = Js::function("f", |_, _| Ok(Js::Undefined));
        object
            .call_method(
                "defineProperty",
                vec![
                    f.clone(),
                    Js::str("length"),
                    Js::object(vec![("value", value)]),
                ],
            )
            .unwrap();
        let bound = f.call_method("bind", vec![Js::Null]).unwrap();
        actual.push(format!(
            "{:016x}",
            bound.get("length").unwrap().to_number().unwrap().to_bits()
        ));
    }
    assert_eq!(
        serde_json::json!(actual),
        expected,
        "FOR_IN_METADATA exact bound length bits and nonnumber refusal to coerce"
    );
}
