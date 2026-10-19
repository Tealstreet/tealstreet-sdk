use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn bits(value: &Js) -> String {
    let n = value.as_f64().unwrap();
    if n.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", n.to_bits())
    }
}
fn keys(value: &Js) -> Vec<String> {
    value
        .own_keys()
        .iter()
        .filter(|key| {
            value
                .own_property_descriptor(key)
                .unwrap()
                .get("enumerable")
                .unwrap()
                .truthy()
        })
        .map(|key| String::from_utf16(key).unwrap())
        .collect()
}
fn descriptor(value: &Js, key: &str) -> Value {
    let d = value.own_property_descriptor(&utf16(key)).unwrap();
    json!([
        d.get("writable").unwrap().truthy(),
        d.get("enumerable").unwrap().truthy(),
        d.get("configurable").unwrap().truthy()
    ])
}
pub fn observe(profile: &Value, clone: impl Fn(&Js) -> Result<Js, Thrown>) -> Value {
    match profile.as_str().unwrap() {
        "primitives" => {
            let a = Js::array(vec![
                Js::Undefined,
                Js::Null,
                Js::Bool(true),
                Js::Number(-0.),
                Js::Number(0.),
                Js::Number(f64::NAN),
                Js::Number(f64::INFINITY),
                Js::Number(f64::NEG_INFINITY),
                Js::utf16(&[0xd800, 88, 0xdc00]),
                Js::BigInt(Rc::new("9007199254740993".parse().unwrap())),
            ]);
            let c = clone(&a).unwrap();
            json!({"ownUndefined":c.has_own("0")&&c.get("0").unwrap().is_undefined(),"nil":c.get("1").unwrap().strict_equals(&Js::Null),"bool":c.get("2").unwrap().truthy(),"numbers":(3..8).map(|i|bits(&c.get(&i.to_string()).unwrap())).collect::<Vec<_>>(),"units":c.get("8").unwrap().as_str().unwrap().as_ref(),"big":c.get("9").unwrap().to_display()})
        }
        "graph" => {
            let child = Js::object(vec![("n", Js::Number(17.))]);
            let root = Js::object(vec![("child", child.clone()), ("again", child.clone())]);
            root.set("self", root.clone()).unwrap();
            let map = Js::new_map();
            let Js::Map(entries) = &map else {
                unreachable!()
            };
            entries.borrow_mut().push((child.clone(), root.clone()));
            let set = Js::new_set();
            let Js::Set(entries) = &set else {
                unreachable!()
            };
            entries.borrow_mut().extend([child.clone(), root.clone()]);
            root.set("map", map.clone()).unwrap();
            root.set("set", set.clone()).unwrap();
            let c = clone(&root).unwrap();
            let cc = c.get("child").unwrap();
            let cm = c.get("map").unwrap();
            let Js::Map(cm) = &cm else { panic!("Map") };
            let cs = c.get("set").unwrap();
            let Js::Set(cs) = &cs else { panic!("Set") };
            let result = json!({"detached":!c.strict_equals(&root),"self":c.get("self").unwrap().strict_equals(&c),"alias":cc.strict_equals(&c.get("again").unwrap()),"mapKey":cm.borrow()[0].0.strict_equals(&cc),"mapValue":cm.borrow()[0].1.strict_equals(&c),"set":[cs.borrow()[0].strict_equals(&cc),cs.borrow()[1].strict_equals(&c)],"value":cc.get("n").unwrap().as_f64().unwrap() as i32});
            child.set("n", Js::Number(99.)).unwrap();
            assert_eq!(cc.get("n").unwrap().as_f64(), Some(17.));
            root.delete(&utf16("self")).unwrap();
            c.delete(&utf16("self")).unwrap();
            let Js::Map(original_map) = &map else {
                unreachable!()
            };
            original_map.borrow_mut().clear();
            let Js::Set(original_set) = &set else {
                unreachable!()
            };
            original_set.borrow_mut().clear();
            cm.borrow_mut().clear();
            cs.borrow_mut().clear();
            result
        }
        "sparse" => {
            let a = Js::array(vec![]);
            a.set("length", Js::Number(4.)).unwrap();
            a.set("1", Js::Undefined).unwrap();
            a.set("3", Js::Number(-0.)).unwrap();
            a.set("extra", Js::utf16(&[0xd800])).unwrap();
            let c = clone(&a).unwrap();
            json!({"length":c.get("length").unwrap().as_f64().unwrap() as usize,"keys":keys(&c),"hole":!c.has_own("0"),"ownUndefined":c.has_own("1")&&c.get("1").unwrap().is_undefined(),"bits":bits(&c.get("3").unwrap()),"units":c.get("extra").unwrap().as_str().unwrap().as_ref(),"descriptor":descriptor(&c,"1")})
        }
        "getters" => {
            let events = Rc::new(RefCell::new(Vec::new()));
            let seen = events.clone();
            let root = Js::object(vec![]);
            root.define_accessor(
                "a",
                Some(Rc::new(move |receiver| {
                    seen.borrow_mut().push("a");
                    receiver.delete(&utf16("b"))?;
                    let Js::Object(object) = receiver else {
                        unreachable!()
                    };
                    object.props.borrow_mut().insert(
                        utf16("c"),
                        Prop::DataDescriptor {
                            value: Js::Number(2.),
                            writable: true,
                            enumerable: false,
                            configurable: true,
                        },
                    );
                    Ok(Js::Number(-0.))
                })),
                None,
            )
            .unwrap();
            root.set("b", Js::Number(1.)).unwrap();
            root.set("c", Js::Number(2.)).unwrap();
            let c = clone(&root).unwrap();
            json!({"events":*events.borrow(),"keys":keys(&c),"bits":bits(&c.get("a").unwrap()),"c":c.get("c").unwrap().as_f64().unwrap() as i32,"descriptor":descriptor(&c,"a")})
        }
        "error" => {
            let child = Js::object(vec![("n", Js::Number(17.))]);
            let cause = Js::object(vec![("child", child.clone()), ("again", child)]);
            let e = error_object("TypeError", "");
            e.set("message", Js::utf16(&[0xd800, 88, 0xdc00])).unwrap();
            e.set("stack", Js::utf16(&[0xdc00, 116, 114, 97, 99, 101]))
                .unwrap();
            e.set("cause", cause.clone()).unwrap();
            e.set("extra", Js::function("extra", |_, _| Ok(Js::Undefined)))
                .unwrap();
            let c = clone(&e).unwrap();
            let cc = c.get("cause").unwrap();
            let mut result = json!({"name":c.get("name").unwrap().to_display(),"units":c.get("message").unwrap().as_str().unwrap().as_ref(),"stack":c.get("stack").unwrap().as_str().unwrap().as_ref(),"alias":cc.get("child").unwrap().strict_equals(&cc.get("again").unwrap()),"detached":!cc.strict_equals(&cause),"extra":c.has_own("extra"),"keys":keys(&c),"messageDescriptor":descriptor(&c,"message"),"stackFlags":[c.own_property_descriptor(&utf16("stack")).unwrap().get("enumerable").unwrap().truthy(),c.own_property_descriptor(&utf16("stack")).unwrap().get("configurable").unwrap().truthy()],"causeDescriptor":descriptor(&c,"cause")});
            c.set(
                "stack",
                Js::utf16(&[0xd800, 114, 101, 112, 108, 97, 99, 101, 109, 101, 110, 116]),
            )
            .unwrap();
            result["stackWrite"] = json!(c.get("stack").unwrap().as_str().unwrap().as_ref());
            result
        }
        "error-undefined" => {
            let e = error_object("Error", "");
            e.delete(&utf16("message")).unwrap();
            e.set("stack", Js::Number(17.)).unwrap();
            e.set("cause", Js::Undefined).unwrap();
            let c = clone(&e).unwrap();
            let mut result = json!({"message":c.has_own("message"),"stack":c.has_own("stack")&&c.get("stack").unwrap().is_undefined(),"cause":c.has_own("cause")&&c.get("cause").unwrap().is_undefined(),"keys":keys(&c),"stackFlags":[c.own_property_descriptor(&utf16("stack")).unwrap().get("enumerable").unwrap().truthy(),c.own_property_descriptor(&utf16("stack")).unwrap().get("configurable").unwrap().truthy()],"causeDescriptor":descriptor(&c,"cause")});
            c.set("stack", Js::Number(23.)).unwrap();
            result["stackWrite"] = json!(c.get("stack").unwrap().as_f64().unwrap() as i32);
            result
        }
        "refusal" => {
            let values = [
                Js::function("f", |_, _| Ok(Js::Undefined)),
                Js::SymbolIterator,
            ];
            json!(
                values
                    .iter()
                    .map(|v| clone(v)
                        .unwrap_err()
                        .value
                        .get("name")
                        .unwrap()
                        .to_display())
                    .collect::<Vec<_>>()
            )
        }
        "getter-throw" => {
            let failure = Js::object(vec![("original", Js::Bool(true))]);
            let token = failure.clone();
            let calls = Rc::new(RefCell::new(0));
            let seen = calls.clone();
            let value = Js::object(vec![]);
            value
                .define_accessor(
                    "value",
                    Some(Rc::new(move |_| {
                        *seen.borrow_mut() += 1;
                        Err(Thrown::new(token.clone()))
                    })),
                    None,
                )
                .unwrap();
            let error = clone(&value).unwrap_err();
            json!({"identity":error.value.strict_equals(&failure),"calls":*calls.borrow()})
        }
        _ => panic!("unreviewed structured clone profile"),
    }
}
