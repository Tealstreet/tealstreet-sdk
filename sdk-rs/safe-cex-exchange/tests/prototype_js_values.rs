//! A retained intrinsic referent is shared with the existing native realm.
#[path = "support/runtime.rs"]
mod runtime;
use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_js_json, to_typed};
use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar, PrototypeValue};
use serde_json::json;
use std::{collections::BTreeMap, process::Command, rc::Rc};

#[test]
fn retained_intrinsic_identity_fields_and_arity_match_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/prototype-value-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let members = [
        ProtoMember::Constructor,
        ProtoMember::DefineGetter,
        ProtoMember::HasOwnProperty,
        ProtoMember::ToString,
        ProtoMember::ValueOf,
        ProtoMember::Prototype,
    ];
    assert_eq!(source.as_array().unwrap().len(), members.len());
    for (member, expected) in members.into_iter().zip(source.as_array().unwrap()) {
        let realm = Rc::<PrototypeRealm>::default();
        let reference = realm.reference(member);
        let first = Js::Builtin(reference.clone());
        let second = Js::from_prototype(PrototypeValue::Reference(realm.reference(member)));
        let before = json!([
            first.type_of(),
            first.to_display(),
            first.get("length").unwrap().to_display(),
            first.strict_equals(&second)
        ]);
        assert_eq!(
            before, expected["before"],
            "INTRINSIC native kind/text/arity/identity {member:?}"
        );
        assert!(!first.strict_equals(&Js::Builtin(
            Rc::<PrototypeRealm>::default().reference(member)
        )));
        reference.set("late", PrototypeScalar::Number(7.0));
        assert_eq!(
            second.get("late").unwrap().as_f64(),
            expected["late"].as_f64(),
            "INTRINSIC reads live native fields"
        );
        let keys = second
            .own_keys()
            .iter()
            .map(|key| String::from_utf16(key).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            json!(keys),
            expected["keys"],
            "INTRINSIC enumerates live native fields"
        );
        for value in [-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            first.set("written", Js::Number(value)).unwrap();
            let PrototypeValue::Number(native) = reference.get("written") else {
                panic!("INTRINSIC writes reach native realm as a number")
            };
            assert_eq!(
                native.to_bits(),
                value.to_bits(),
                "INTRINSIC writes reach native realm without projection"
            );
            assert_eq!(
                second.get("written").unwrap().as_f64().unwrap().to_bits(),
                value.to_bits()
            );
        }
        if member == ProtoMember::Prototype {
            let record: BTreeMap<String, f64> = to_typed(&first).unwrap();
            assert!(record["written"].is_nan());
        } else {
            assert!(matches!(
                to_js_json(&first).unwrap(),
                safe_cex_num::JsJsonValue::Undefined
            ));
        }
    }
}

#[test]
fn captured_intrinsic_prototype_keeps_original_receiver_and_own_shadow() {
    let realm = Rc::<PrototypeRealm>::default();
    let prototype = Js::Builtin(realm.reference(ProtoMember::Prototype));
    let object = Js::object(vec![]);
    let Js::Object(data) = &object else {
        unreachable!()
    };
    *data.proto.borrow_mut() = Some(prototype.clone());
    assert!(
        object.get("__proto__").unwrap().strict_equals(&prototype),
        "INTRINSIC legacy getter keeps receiver"
    );
    realm
        .reference(ProtoMember::Prototype)
        .set("later", PrototypeScalar::Number(9.0));
    assert_eq!(object.get("later").unwrap().as_f64(), Some(9.0));
    object.set("__proto__", Js::Number(1.0)).unwrap();
    assert!(object.get("__proto__").unwrap().strict_equals(&prototype));
    object.define("__proto__", Js::Undefined).unwrap();
    assert!(object.has_own("__proto__"));
    assert!(object.get("__proto__").unwrap().is_undefined());
    object.set("__proto__", Js::Null).unwrap();
    assert!(matches!(object.get("__proto__").unwrap(), Js::Null));
    assert!(
        data.proto
            .borrow()
            .as_ref()
            .unwrap()
            .strict_equals(&prototype)
    );
    object
        .delete(&"__proto__".encode_utf16().collect::<Vec<_>>())
        .unwrap();
    object.set("__proto__", Js::Null).unwrap();
    assert!(data.proto.borrow().is_none());
    assert!(object.get("__proto__").unwrap().is_undefined());
}

#[test]
fn native_thrown_to_shared_failure_retains_error_identity() {
    let native = Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
        "native opaque failure",
    ));
    let thrown = Thrown {
        value: Js::Undefined,
        native: Some(native.clone()),
    };
    let failure: Rc<safe_cex_exchange::runtime::ExchangeError> = thrown.into();
    assert!(Rc::ptr_eq(&failure, &native));
}

#[test]
fn worker_structured_clone_matches_node_graphs_and_refusals() {
    use safe_cex_exchange::js_runtime::{
        structured_clone::structured_clone,
        value::{Prop, error_object, utf16},
    };
    use std::cell::RefCell;
    let output = Command::new("node").args(["-e", r#"
      const assert=require('node:assert/strict');
      const bits=n=>{const b=new DataView(new ArrayBuffer(8));b.setFloat64(0,n);return b.getBigUint64(0).toString(16).padStart(16,'0')};
      const nanClones=[0x7ff8000000000042n,0xfff8000000000042n].map(raw=>{const b=new DataView(new ArrayBuffer(8));b.setBigUint64(0,raw);return bits(structuredClone(b.getFloat64(0)))});
      const child={n:17}, a=[];a.length=3;a[1]=child;a.extra=undefined;
      const root={child,again:child,a,nums:[NaN,Infinity,-Infinity,-0],text:'\ud800X\udc00',big:9007199254740993n,flag:true,nil:null,own:undefined};root.self=root;
      root.map=new Map([[child,root]]);root.set=new Set([child,root]);
      const copy=structuredClone(root);assert.equal(copy.flag,true);assert.equal(copy.nil,null);assert.ok(Object.hasOwn(copy,'own'));assert.equal(copy.own,undefined);assert.notEqual(copy,root);assert.equal(copy.self,copy);assert.equal(copy.child,copy.again);assert.equal(copy.a[1],copy.child);assert.equal(copy.map.get(copy.child),copy);assert.ok(copy.set.has(copy));assert.equal(copy.a.length,3);assert.ok(!Object.hasOwn(copy.a,0));assert.ok(!Object.hasOwn(copy.a,2));assert.ok(Object.hasOwn(copy.a,'extra'));
      const events=[];const obj={get a(){events.push('a');delete this.b;return child},b:2};const flat=structuredClone(obj);assert.ok(!Object.hasOwn(flat,'b'));
      const error=new TypeError('x');Object.defineProperty(error,'stack',{get(){events.push('stack');return 'trace'}});error.cause=root;error.extra=()=>{};
      const e=structuredClone(error);assert.ok(e instanceof TypeError);assert.equal(e.cause.self,e.cause);assert.ok(!Object.hasOwn(e,'extra'));
      const flipped=structuredClone({get a(){Object.defineProperty(this,'b',{enumerable:false});return 1},b:2});assert.deepEqual(Object.keys(flipped),['a','b']);
      const emptyStack=new Error('x');emptyStack.stack=17;const noStack=structuredClone(emptyStack);assert.ok(Object.hasOwn(noStack,'stack'));assert.equal(noStack.stack,undefined);
      const failure={marker:NaN};assert.throws(()=>structuredClone({get value(){throw failure}}),x=>x===failure);
      const refused=[()=>{},Symbol.iterator,Promise.resolve()].map(v=>{try{structuredClone(v);throw Error('accepted')}catch(e){return e.name}});
      console.log(JSON.stringify({nanClones,nums:copy.nums.map(bits),text:Array.from({length:copy.text.length},(_,i)=>copy.text.charCodeAt(i)),big:String(copy.big),enumFlip:Object.keys(flipped),emptyStack:[Object.hasOwn(noStack,'stack'),noStack.stack===undefined],keys:Object.keys(copy.a),events,error:[e.name,e.message,e.stack,Object.getOwnPropertyDescriptor(e,'message').enumerable],refused}));
    "#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let nan_clones: Vec<_> = [0x7ff8000000000042_u64, 0xfff8000000000042]
        .into_iter()
        .map(|bits| {
            format!(
                "{:016x}",
                structured_clone(&Js::Number(f64::from_bits(bits)))
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_bits()
            )
        })
        .collect();
    let child = Js::object(vec![("n", Js::Number(17.))]);
    let array = Js::array(vec![]);
    array.set("length", Js::Number(3.)).unwrap();
    array.set("1", child.clone()).unwrap();
    array.set("extra", Js::Undefined).unwrap();
    let nums = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.];
    let root = Js::object(vec![
        ("child", child.clone()),
        ("again", child.clone()),
        ("a", array),
        ("nums", Js::array(nums.map(Js::Number).to_vec())),
        ("text", Js::utf16(&[0xd800, 88, 0xdc00])),
        (
            "big",
            Js::BigInt(Rc::new("9007199254740993".parse().unwrap())),
        ),
    ]);
    root.set("flag", Js::Bool(true)).unwrap();
    root.set("nil", Js::Null).unwrap();
    root.set("own", Js::Undefined).unwrap();
    root.set("self", root.clone()).unwrap();
    let map = Js::new_map();
    let Js::Map(entries) = &map else {
        unreachable!()
    };
    entries.borrow_mut().push((child.clone(), root.clone()));
    root.set("map", map).unwrap();
    let set = Js::new_set();
    let Js::Set(entries) = &set else {
        unreachable!()
    };
    entries.borrow_mut().push(child.clone());
    entries.borrow_mut().push(root.clone());
    root.set("set", set).unwrap();
    let copy = structured_clone(&root).unwrap();
    assert!(copy.get("flag").unwrap().strict_equals(&Js::Bool(true)));
    assert!(copy.get("nil").unwrap().strict_equals(&Js::Null));
    assert!(copy.has_own("own"));
    assert!(copy.get("own").unwrap().is_undefined());
    assert!(!copy.strict_equals(&root));
    assert!(copy.get("self").unwrap().strict_equals(&copy));
    let copied_child = copy.get("child").unwrap();
    assert!(copied_child.strict_equals(&copy.get("again").unwrap()));
    let a = copy.get("a").unwrap();
    assert!(a.get("1").unwrap().strict_equals(&copied_child));
    assert!(!a.has_own("0"));
    assert!(!a.has_own("2"));
    assert_eq!(a.get("length").unwrap().as_f64(), Some(3.));
    let Js::Map(map) = copy.get("map").unwrap() else {
        panic!("Map kind")
    };
    assert!(map.borrow()[0].0.strict_equals(&copied_child));
    assert!(map.borrow()[0].1.strict_equals(&copy));
    let Js::Set(set) = copy.get("set").unwrap() else {
        panic!("Set kind")
    };
    assert!(set.borrow()[1].strict_equals(&copy));
    let events = Rc::new(RefCell::new(vec![]));
    let seen = events.clone();
    let returned = child.clone();
    let object = Js::object(vec![("b", Js::Number(2.))]);
    object
        .define_accessor(
            "a",
            Some(Rc::new(move |receiver| {
                seen.borrow_mut().push("a");
                receiver.delete(&utf16("b"))?;
                Ok(returned.clone())
            })),
            None,
        )
        .unwrap();
    // Put the getter first, as in the source object literal.
    let Js::Object(inner) = &object else {
        unreachable!()
    };
    inner.props.borrow_mut().move_index(1, 0);
    let flat = structured_clone(&object).unwrap();
    assert!(!flat.has_own("b"));
    let error = error_object("TypeError", "x");
    let seen = events.clone();
    error
        .define_accessor(
            "stack",
            Some(Rc::new(move |_| {
                seen.borrow_mut().push("stack");
                Ok(Js::str("trace"))
            })),
            None,
        )
        .unwrap();
    error.set("cause", root.clone()).unwrap();
    error
        .set(
            "extra",
            Js::function("unclonable", |_, _| Ok(Js::Undefined)),
        )
        .unwrap();
    let e = structured_clone(&error).unwrap();
    assert!(!e.has_own("extra"));
    assert!(
        e.get("cause")
            .unwrap()
            .get("self")
            .unwrap()
            .strict_equals(&e.get("cause").unwrap())
    );
    let Js::Object(error_obj) = &e else {
        unreachable!()
    };
    let enumerable = matches!(
        error_obj.props.borrow().get(&utf16("message")),
        Some(Prop::DataDescriptor {
            enumerable: true,
            ..
        })
    );
    let flipped = Js::object(vec![("b", Js::Number(2.))]);
    flipped
        .define_accessor(
            "a",
            Some(Rc::new(|receiver| {
                let Js::Object(object) = receiver else {
                    unreachable!()
                };
                object.props.borrow_mut().insert(
                    utf16("b"),
                    Prop::DataDescriptor {
                        value: Js::Number(2.),
                        writable: true,
                        enumerable: false,
                        configurable: true,
                    },
                );
                Ok(Js::Number(1.))
            })),
            None,
        )
        .unwrap();
    let Js::Object(object) = &flipped else {
        unreachable!()
    };
    object.props.borrow_mut().move_index(1, 0);
    let flipped = structured_clone(&flipped).unwrap();
    let empty_stack = error_object("Error", "x");
    empty_stack.set("stack", Js::Number(17.)).unwrap();
    let no_stack = structured_clone(&empty_stack).unwrap();
    let failure = Js::object(vec![("marker", Js::Number(f64::NAN))]);
    let thrown = failure.clone();
    let getter = Js::object(vec![]);
    getter
        .define_accessor(
            "value",
            Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
            None,
        )
        .unwrap();
    assert!(
        structured_clone(&getter)
            .unwrap_err()
            .value
            .strict_equals(&failure)
    );
    let refused = [
        Js::function("unclonable", |_, _| Ok(Js::Undefined)),
        Js::SymbolIterator,
        Js::Promise(safe_cex_exchange::js_runtime::promise::Promise::pending(
            &(Rc::new(runtime::Runtime::default())
                as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>),
        )),
    ];
    let refused: Vec<_> = refused
        .iter()
        .map(|v| {
            structured_clone(v)
                .unwrap_err()
                .value
                .get("name")
                .unwrap()
                .to_display()
        })
        .collect();
    let numbers = copy.get("nums").unwrap();
    let observed: Vec<_> = (0..4)
        .map(|i| {
            format!(
                "{:016x}",
                numbers
                    .get(&i.to_string())
                    .unwrap()
                    .as_f64()
                    .unwrap()
                    .to_bits()
            )
        })
        .collect();
    assert_eq!(
        json!({"nanClones":nan_clones,"nums":observed,"text":copy.get("text").unwrap().as_str().unwrap().as_ref(),"big":copy.get("big").unwrap().to_display(),"emptyStack":[no_stack.has_own("stack"),no_stack.get("stack").unwrap().is_undefined()],"enumFlip":flipped.own_keys().iter().map(|key|String::from_utf16(key).unwrap()).collect::<Vec<_>>(),"keys":a.own_keys().iter().filter(|k|**k!=utf16("length")).map(|k|String::from_utf16(k).unwrap()).collect::<Vec<_>>(),"events":*events.borrow(),"error":[e.get("name").unwrap().to_display(),e.get("message").unwrap().to_display(),e.get("stack").unwrap().to_display(),enumerable],"refused":refused}),
        source
    );
    for graph in [&root, &copy, &e.get("cause").unwrap()] {
        graph.delete(&utf16("self")).unwrap();
        graph.delete(&utf16("map")).unwrap();
        graph.delete(&utf16("set")).unwrap();
    }
}

mod clone_observation {
    use safe_cex_exchange::js_runtime::value::{Js, Prop, Thrown, error_object, utf16};
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/structured-clone-observers.rs"
    ));
}

#[test]
fn structured_clone_owner_profiles_match_node() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/structured-clone-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 8);
    for row in rows {
        let actual = clone_observation::observe(
            &row["input"],
            safe_cex_exchange::js_runtime::structured_clone::structured_clone,
        );
        assert_eq!(actual, row["output"], "STRUCTURED_CLONE {}", row["input"]);
    }
}
