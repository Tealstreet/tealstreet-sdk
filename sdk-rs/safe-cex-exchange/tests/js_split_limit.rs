//! String/RegExp split limits through installed production native bindings.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc, sync::Arc};
type Trace = Rc<RefCell<Vec<String>>>;
fn limit(tag: &str, log: Trace) -> Js {
    match tag {
        "undefined" => Js::Undefined,
        "zero" => Js::Number(0.),
        "one" => Js::Number(1.),
        "two" => Js::Number(2.),
        "three" => Js::Number(3.),
        "negative_one" => Js::Number(-1.),
        "negative_two" => Js::Number(-2.),
        "fraction" => Js::Number(2.9),
        "negative_fraction" => Js::Number(-0.5),
        "negative_zero" => Js::Number(-0.),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "negative_infinity" => Js::Number(f64::NEG_INFINITY),
        "wrap_zero" => Js::Number(4_294_967_296.),
        "wrap_one" => Js::Number(4_294_967_297.),
        "negative_wrap" => Js::Number(-4_294_967_294.),
        "large_signed" => Js::Number(2_f64.powi(63)),
        "large_unsigned" => Js::Number(2_f64.powi(64)),
        "huge" => Js::Number(1e100),
        "string" => Js::str("2"),
        "object" | "throwing" => {
            let throwing = tag == "throwing";
            Js::object(vec![(
                "valueOf",
                Js::function("valueOf", move |_, _| {
                    log.borrow_mut().push("limit".into());
                    if throwing {
                        Err(Thrown::error("RangeError", "limit"))
                    } else {
                        Ok(Js::Number(2.))
                    }
                }),
            )])
        }
        "symbol" => Js::SymbolIterator,
        "bigint" => Js::BigInt(Rc::new(2.into())),
        _ => panic!("unknown source limit {tag}"),
    }
}
fn separator(tag: &str, log: Trace) -> Js {
    match tag {
        "comma" => Js::str(","),
        "empty" => Js::str(""),
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "emoji" => Js::str("😀"),
        "high" => Js::utf16(&[0xd800]),
        "low" => Js::utf16(&[0xdc00]),
        "object" | "throwing" => {
            let throwing = tag == "throwing";
            Js::object(vec![(
                "toString",
                Js::function("toString", move |_, _| {
                    log.borrow_mut().push("separator".into());
                    if throwing {
                        Err(Thrown::error("URIError", "separator"))
                    } else {
                        Ok(Js::str(","))
                    }
                }),
            )])
        }
        "regex_capture" | "regex_optional" | "regex_empty" => {
            let pattern = match tag {
                "regex_capture" => "(,)",
                "regex_optional" => "(,)|(x)",
                _ => "(?:)",
            };
            // The production lowering creates regex literals with this parser;
            // the global RegExp constructor is not installed by this host.
            let units: Vec<u16> = pattern.encode_utf16().collect();
            safe_cex_exchange::js_runtime::regexp::regexp(&units, &[]).unwrap()
        }
        _ => panic!("unknown source separator {tag}"),
    }
}
#[test]
fn split_limit_matches_node_uint32_coercion_utf16_and_regex_captures() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_split_limit_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 1440);
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
        for row in cases {
            let log: Trace = Rc::default();
            let text: Vec<u16> = row["source"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
                .collect();
            let result = Js::utf16(&text).call_method(
                "split",
                vec![
                    separator(row["separatorTag"].as_str().unwrap(), log.clone()),
                    limit(row["limitTag"].as_str().unwrap(), log.clone()),
                ],
            );
            let (result, error) = match result {
                Ok(value) => {
                    let rows: Vec<Value> = value
                        .array_items()
                        .unwrap()
                        .into_iter()
                        .map(|v| {
                            let v =
                                v.expect("split results are dense, including undefined captures");
                            if v.is_undefined() {
                                json!({"type":"undefined"})
                            } else {
                                json!({"type":"string","units":v.as_str().unwrap().as_ref()})
                            }
                        })
                        .collect();
                    (Some(rows), None)
                }
                Err(e) => (None, Some(e.value.get("name").unwrap().to_display())),
            };
            assert_eq!(
                json!({"result":result,"error":error,"log":*log.borrow()}),
                json!({"result":row["result"],"error":row["error"],"log":row["log"]}),
                "SPLIT {} {} {}",
                row["textTag"],
                row["separatorTag"],
                row["limitTag"]
            );
        }
    });
}
