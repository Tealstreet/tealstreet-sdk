#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::ScopeShared,
        value::{Js, Thrown, to_json},
    },
    native::native_runtime,
};
use serde::Deserialize;
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

#[derive(Deserialize)]
struct Case {
    label: String,
    key: String,
    expected: serde_json::Value,
}

fn input(label: &str) -> Js {
    match label {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "true" => Js::Bool(true),
        "zero" => Js::Number(0.0),
        "negative-zero" => Js::Number(-0.0),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "negative-infinity" => Js::Number(f64::NEG_INFINITY),
        "bigint" => Js::BigInt(Rc::new(123.into())),
        "symbol" => Js::SymbolIterator,
        "string" => Js::str("abc"),
        "empty" => Js::object(vec![]),
        "own" => Js::object(vec![("tradingMode", Js::Bool(false))]),
        "snake" => Js::object(vec![("trading_mode", Js::Number(17.0))]),
        "camel" => Js::object(vec![("tradingMode", Js::Number(23.0))]),
        "null-prototype" => {
            let obj = Js::object(vec![]);
            let Js::Object(data) = &obj else {
                unreachable!()
            };
            *data.proto.borrow_mut() = None;
            obj
        }
        "inherited" | "shadow" => {
            let obj = Js::object(if label == "shadow" {
                vec![("tradingMode", Js::Undefined)]
            } else {
                vec![]
            });
            let Js::Object(data) = &obj else {
                unreachable!()
            };
            *data.proto.borrow_mut() = Some(Js::object(vec![("tradingMode", Js::Number(31.0))]));
            obj
        }
        label => panic!("unknown source case {label}"),
    }
}

#[test]
fn native_get_key_matches_source_primitive_in_rejections() {
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).args(["-e", r#"
const fs = require('node:fs'), path = require('node:path'), Module = require('node:module');
const ts = require(process.env.SAFE_CEX_TYPESCRIPT_PATH || 'typescript');
const filename = path.resolve('packages/safe-cex/src/utils/get-key.ts');
const loaded = new Module(filename, module);
loaded.paths = Module._nodeModulePaths(path.dirname(filename));
loaded._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {compilerOptions:{module:ts.ModuleKind.CommonJS,esModuleInterop:true}}).outputText, filename);
const {v} = loaded.exports;
const values = {undefined, null:null, false:false, true:true, zero:0, 'negative-zero':-0,
 nan:NaN, infinity:Infinity, 'negative-infinity':-Infinity, bigint:123n, symbol:Symbol.iterator,
 string:'abc', empty:{}, 'null-prototype':Object.create(null), own:{tradingMode:false}, snake:{trading_mode:17}, camel:{tradingMode:23},
 inherited:Object.create({tradingMode:31}), shadow:Object.assign(Object.create({tradingMode:31}),{tradingMode:undefined})};
const rows = [];
for (const [label,obj] of Object.entries(values)) for (const key of ['0','length','tradingMode','trading_mode','missing']) {
 let expected;
 try { const result = v(obj,key); expected = result === undefined ? {undefined:true} : {value:result}; }
 catch (error) {expected = {error:error.name,message:error.message}}
 rows.push({label,key,expected});
}
const throttle = require('lodash/throttle');
for (const [label,obj] of Object.entries(values)) {
 const throttled = throttle(() => 7, 10, obj);
 rows.push({label,key:'$throttle',expected:{value:throttled()}});
 throttled.cancel();
}
for (const [label,obj] of [['undefined',undefined],['null',null]]) {
 const key = {toString() {throw new RangeError('key must not be coerced')}};
 const result = v(obj,key);
 rows.unshift({label,key:'$throwingKey',expected:result === undefined ? {undefined:true} : {value:result}});
}
for (const [label,obj] of Object.entries(values)) {
 if (obj == null || ['object','function'].includes(typeof obj)) continue;
 let keyCalls = 0;
 const key = {toString() {keyCalls++; throw new RangeError('key must not be coerced')}};
 let expected;
 try {v(obj,key); expected = {returned:true,keyCalls}}
 catch (error) {expected = {error:error.name,keyCalls}}
 rows.unshift({label,key:'$throwingPrimitiveKey',expected});
}
console.log(JSON.stringify(rows));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Case> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 126);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let native = native_runtime();
        let http = client::rest_http_host(native.clone(), Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let shared = Rc::new(ScopeShared::new(native, http,
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()), Js::object(vec![]), ()));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        let get = shared.globals.get("v").unwrap();
        for case in cases {
            let key_calls = Rc::new(Cell::new(0));
            let result = if case.key == "$throttle" {
                let throttled = shared.globals.get("throttle").unwrap().call(Js::Undefined,
                    vec![Js::function("value", |_, _| Ok(Js::Number(7.0))), Js::Number(10.0), input(&case.label)]).unwrap();
                let result = throttled.call(Js::Undefined, vec![]);
                throttled.call_method("cancel", vec![]).unwrap();
                result
            } else {
                let key = if matches!(case.key.as_str(), "$throwingKey" | "$throwingPrimitiveKey") {
                    let key_calls = key_calls.clone();
                    Js::object(vec![("toString", Js::function("toString", move |_, _| {
                        key_calls.set(key_calls.get() + 1);
                        Err(Thrown::error("RangeError", "key must not be coerced"))
                    }))])
                } else {
                    Js::str(&case.key)
                };
                get.call(Js::Undefined, vec![input(&case.label), key])
            };
            let actual = match result {
                Ok(value) if value.is_undefined() => serde_json::json!({"undefined":true}),
                Ok(value) => serde_json::json!({"value":to_json(&value).unwrap()}),
                Err(error) if case.key == "$throwingPrimitiveKey" => serde_json::json!({"error":error.value.get("name").unwrap().to_display(),"keyCalls":key_calls.get()}),
                Err(error) => serde_json::json!({"error":error.value.get("name").unwrap().to_display(),"message":error.message()}),
            };
            if let (Some(actual), Some(expected)) = (actual["value"].as_f64(), case.expected["value"].as_f64()) {
                assert_eq!(actual, expected, "{}: {}", case.label, case.key);
            } else {
                assert_eq!(actual, case.expected, "{}: {}", case.label, case.key);
            }
        }
    });
}
