//! Installed builtin surface on NativeRuntime with the real native HTTP host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::{Scope, ScopeShared},
        value::{Js, from_json, to_json},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{rc::Rc, sync::Arc};

fn run_native(body: impl FnOnce(Scope<()>)) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
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
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        let scope = Scope {
            shared,
            this: Js::Undefined,
        };
        body(scope);
    });
}

#[test]
fn native_host_executes_source_array_dayjs_property_and_console_calls() {
    run_native(|scope| {
        let array = Js::array(vec![Js::Number(1.), Js::Number(2.), Js::Number(3.)]);
        assert_eq!(array.call_method("pop", vec![]).unwrap().as_f64(), Some(3.));
        assert!(
            array
                .call_method("reverse", vec![])
                .unwrap()
                .same_reference(&array)
        );
        assert_eq!(to_json(&array).unwrap(), json!([2., 1.]));
        let day = scope
            .global("dayjs")
            .unwrap()
            .call(Js::Undefined, vec![Js::str("2024-03-10 08:15:00")])
            .unwrap();
        let adjusted = day
            .call_method("utcOffset", vec![Js::Number(480.), Js::Bool(true)])
            .unwrap();
        assert_eq!(
            adjusted.call_method("valueOf", vec![]).unwrap().as_f64(),
            Some(1_710_029_700_000.)
        );
        let midnight = adjusted
            .call_method("startOf", vec![Js::str("date")])
            .unwrap();
        assert_eq!(
            midnight.call_method("valueOf", vec![]).unwrap().as_f64(),
            Some(1_710_000_000_000.)
        );
        let object = Js::object(vec![("visible", Js::Number(1.))]);
        scope
            .global("Object.defineProperty")
            .unwrap()
            .call(
                Js::Undefined,
                vec![
                    object.clone(),
                    Js::str("hidden"),
                    Js::object(vec![("value", Js::Number(2.))]),
                ],
            )
            .unwrap();
        let keys = scope
            .global("Object.getOwnPropertyNames")
            .unwrap()
            .call(Js::Undefined, vec![object])
            .unwrap();
        assert_eq!(to_json(&keys).unwrap(), json!(["visible", "hidden"]));
        scope
            .global("console.log")
            .unwrap()
            .call(Js::Undefined, vec![Js::str("native builtin regression")])
            .unwrap();
    });
}

fn dayjs_oracle() -> Value {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_builtins_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn observed(value: Js) -> Value {
    match value {
        Js::Number(n) if n.is_nan() => json!("NaN"),
        Js::Number(n) if n == f64::INFINITY => json!("Infinity"),
        Js::Number(n) if n == f64::NEG_INFINITY => json!("-Infinity"),
        Js::Number(n) if n == 0.0 && n.is_sign_negative() => json!("-0"),
        other => to_json(&other).unwrap(),
    }
}
fn same_numbers(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_numbers(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|b| same_numbers(v, b)))
        }
        _ => left == right,
    }
}
#[test]
fn native_dayjs_offset_coercions_match_installed_plugin() {
    let expected = dayjs_oracle();
    let cases = expected["coercions"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    run_native(|scope| {
        let dayjs = scope.global("dayjs").unwrap();
        for row in cases {
            let input = if row["input"]["type"] == "number" {
                Js::Number(row["input"]["value"].as_str().unwrap().parse().unwrap())
            } else {
                from_json(&row["input"])
            };
            let before = dayjs
                .call(Js::Undefined, vec![Js::Number(1710083045678.0)])
                .unwrap();
            let after = before
                .call_method("utcOffset", vec![input.clone(), from_json(&row["keep"])])
                .unwrap();
            for (key, member) in [
                ("valueOf", "valueOf"),
                ("offset", "utcOffset"),
                ("unix", "unix"),
            ] {
                let actual = observed(after.call_method(member, vec![]).unwrap());
                assert!(same_numbers(&actual, &row[key]), "{row}: {key}: {actual}");
            }
            assert_eq!(
                before.same_reference(&after),
                row["sameInstance"].as_bool().unwrap(),
                "{row}"
            );
            assert_eq!(
                after
                    .call_method("utcOffset", vec![])
                    .unwrap()
                    .strict_equals(&input),
                row["sameInputOffset"].as_bool().unwrap(),
                "{row}"
            );
        }
    });
}
#[test]
fn native_dayjs_start_of_end_boundaries_match_installed_plugin() {
    let expected = dayjs_oracle();
    let cases = expected["ends"].as_array().unwrap();
    assert_eq!(cases.len(), 576);
    run_native(|scope| {
        let dayjs = scope.global("dayjs").unwrap();
        for row in cases {
            let start = if row["start"] == "NaN" {
                Js::Number(f64::NAN)
            } else {
                from_json(&row["start"])
            };
            let mut day = dayjs.call(Js::Undefined, vec![start]).unwrap();
            if !row["offset"].is_null() {
                day = day
                    .call_method("utcOffset", vec![from_json(&row["offset"])])
                    .unwrap();
            }
            let unit = if row["unit"].is_null() {
                Js::Undefined
            } else {
                from_json(&row["unit"])
            };
            day = day
                .call_method("startOf", vec![unit, from_json(&row["flag"])])
                .unwrap();
            for (key, member) in [
                ("valueOf", "valueOf"),
                ("utcOffset", "utcOffset"),
                ("unix", "unix"),
            ] {
                let actual = observed(day.call_method(member, vec![]).unwrap());
                assert!(same_numbers(&actual, &row[key]), "{row}: {key}: {actual}");
            }
        }
    });
}

#[test]
fn native_operator_objects_reuse_ordinary_primitive_conversion() {
    use safe_cex_exchange::js_runtime::{ops, value::Thrown};
    use std::cell::RefCell;
    let output = std::process::Command::new("node").args(["-e", r#"
const modes=['default','skip-value','skip-both','null-proto','object-results','negative-zero','value-throws','getter-throws'];
const rows=[];
for(const mode of modes){
 const trace=[], token={unit:'\ud800'}, obj=mode==='null-proto'?Object.create(null):{};
 if(!['default','null-proto'].includes(mode))for(const name of ['valueOf','toString'])Object.defineProperty(obj,name,{get(){
   trace.push('get:'+name);
   if(mode==='getter-throws'&&name==='valueOf')throw token;
   if(mode==='skip-both'||(mode==='skip-value'&&name==='valueOf'))return null;
   return function(){trace.push('call:'+name);if(this!==obj)throw Error('wrong receiver');if(mode==='value-throws')throw token;if(mode==='object-results')return {};return name==='valueOf'?-0:'9';};
 }});
 let observed;
 try{const value=+obj;observed={kind:'number',value:Number.isNaN(value)?'NaN':Object.is(value,-0)?'-0':String(value)};}
 catch(e){observed={kind:'throw',same:e===token,name:e===token?null:e.name,message:e===token?null:e.message};}
 rows.push({mode,trace,observed});
}
process.stdout.write(JSON.stringify(rows));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.as_array().unwrap().len(), 8);
    run_native(|scope| {
        let mut rows = vec![];
        for mode in [
            "default",
            "skip-value",
            "skip-both",
            "null-proto",
            "object-results",
            "negative-zero",
            "value-throws",
            "getter-throws",
        ] {
            let trace = Rc::new(RefCell::new(Vec::<String>::new()));
            let token = Js::object(vec![("unit", Js::utf16(&[0xd800]))]);
            let obj = Js::object(vec![]);
            if mode == "null-proto" {
                scope
                    .global("Object")
                    .unwrap()
                    .call_method("setPrototypeOf", vec![obj.clone(), Js::Null])
                    .unwrap();
            }
            if !matches!(mode, "default" | "null-proto") {
                for name in ["valueOf", "toString"] {
                    let trace = trace.clone();
                    let token = token.clone();
                    obj.define_accessor(
                        name,
                        Some(Rc::new(move |owner| {
                            trace.borrow_mut().push(format!("get:{name}"));
                            if mode == "getter-throws" && name == "valueOf" {
                                return Err(Thrown::new(token.clone()));
                            }
                            if mode == "skip-both" || mode == "skip-value" && name == "valueOf" {
                                return Ok(Js::Null);
                            }
                            let owner = owner.clone();
                            let trace = trace.clone();
                            let token = token.clone();
                            Ok(Js::function(name, move |this, _| {
                                assert!(this.same_reference(&owner));
                                trace.borrow_mut().push(format!("call:{name}"));
                                if mode == "value-throws" {
                                    return Err(Thrown::new(token.clone()));
                                }
                                Ok(if mode == "object-results" {
                                    Js::object(vec![])
                                } else if name == "valueOf" {
                                    Js::Number(-0.)
                                } else {
                                    Js::str("9")
                                })
                            }))
                        })),
                        None,
                    )
                    .expect("test accessor target permits definition");
                }
            }
            let observed = match ops::to_numeric(&obj) {
                Ok(Js::Number(value)) => {
                    json!({"kind":"number", "value":if value.is_nan() {"NaN".to_owned()} else if value.to_bits() == (-0.0_f64).to_bits() {"-0".to_owned()} else {safe_cex_num::js_number_string(value)}})
                }
                Ok(value) => panic!("expected Number, got {value:?}"),
                Err(error) => {
                    let same = error.value.same_reference(&token);
                    if same {
                        assert_eq!(
                            &*token.get("unit").unwrap().to_js_string().unwrap(),
                            &[0xd800]
                        );
                    }
                    json!({"kind":"throw", "same":same, "name":if same {Value::Null} else {json!(error.value.get("name").unwrap().to_display())}, "message":if same {Value::Null} else {json!(error.value.get("message").unwrap().to_display())}})
                }
            };
            rows.push(json!({"mode":mode,"trace":*trace.borrow(),"observed":observed}));
        }
        assert_eq!(json!(rows), source);
    });
}

#[test]
fn native_array_join_preserves_utf16_elements_separators_and_coercions() {
    let oracle = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_array_join_utf16_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    run_native(|_scope| {
        let high = Js::utf16(&[0xd800]);
        let low = Js::utf16(&[0xdc00]);
        let element = Js::object(vec![(
            "toString",
            Js::function("native element", |_, _| Ok(Js::utf16(&[0xd800]))),
        )]);
        let separator = Js::object(vec![(
            "toString",
            Js::function("native separator", |_, _| Ok(Js::utf16(&[0xdc00]))),
        )]);
        let holes = Js::array(vec![]);
        holes.set("length", Js::Number(3.0)).unwrap();
        let native_array = Js::array(vec![
            Js::Undefined,
            Js::Null,
            high.clone(),
            Js::Number(-0.0),
            Js::Number(f64::INFINITY),
        ]);
        let cases = [
            ("elements", native_array, Js::str("|")),
            (
                "separator",
                Js::array(vec![Js::str("a"), Js::str("b")]),
                low,
            ),
            (
                "default",
                Js::array(vec![high.clone(), Js::str("x")]),
                Js::Undefined,
            ),
            (
                "nested",
                Js::array(vec![
                    Js::array(vec![high.clone(), Js::Null]),
                    Js::str("tail"),
                ]),
                Js::str(";"),
            ),
            (
                "null separator",
                Js::array(vec![Js::str("a"), Js::str("b")]),
                Js::Null,
            ),
            ("empty", Js::array(vec![]), high.clone()),
            ("holes", holes, high),
            ("coercion", Js::array(vec![element]), separator),
        ];
        assert_eq!(expected.as_array().unwrap().len(), cases.len());
        for ((name, array, separator), expected) in
            cases.into_iter().zip(expected.as_array().unwrap())
        {
            assert_eq!(expected["name"], name);
            let actual = array.call_method("join", vec![separator]).unwrap();
            let expected: Vec<u16> = serde_json::from_value(expected["units"].clone()).unwrap();
            assert_eq!(
                actual.as_str().unwrap().as_ref(),
                expected.as_slice(),
                "native Array.join UTF16: {name}"
            );
        }
    });
}
