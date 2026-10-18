#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::{Promise, await_js},
        scope::ScopeShared,
        value::{Js, Thrown, to_json},
    },
    native::native_runtime,
    runtime::ExchangeRuntime,
};
use serde::Deserialize;
use std::{
    process::Command,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::Duration,
};

#[derive(Clone, Deserialize)]
struct Case {
    method: String,
    source: Vec<u16>,
    needle: Vec<u16>,
    position: String,
    expected: serde_json::Value,
}

fn cases() -> &'static [Case] {
    static CASES: OnceLock<Vec<Case>> = OnceLock::new();
    CASES.get_or_init(|| {
        let output = Command::new("node").args(["-e", r#"
const rows=[],units=s=>Array.from({length:s.length},(_,i)=>s.charCodeAt(i));
const positions=['omit','undefined','null','false','true','NaN','Infinity','-Infinity','-3','-0.5','-0','0','0.9','1','1.9','2','3','4','5','8','999','"2.9"'];
const value=p=>p==='undefined'?undefined:p==='NaN'?NaN:p==='Infinity'?Infinity:p==='-Infinity'?-Infinity:JSON.parse(p);
for(const method of ['indexOf','lastIndexOf','includes','startsWith','endsWith'])
for(const source of ['', 'ababa', 'a😀ba😀', '\ud800x\udfff', 'undefined'])
for(const needle of ['a','b','😀','\ud83d','\ude00','\ud800','\udfff','x','','undefined'])
for(const position of positions){
const args=position==='omit'?[needle]:[needle,value(position)];
rows.push({method,source:units(source),needle:units(needle),position,expected:source[method](...args)});
}
console.log(JSON.stringify(rows));
"#]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn position(tag: &str) -> Js {
    match tag {
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "true" => Js::Bool(true),
        "NaN" => Js::Number(f64::NAN),
        "Infinity" => Js::Number(f64::INFINITY),
        "-Infinity" => Js::Number(f64::NEG_INFINITY),
        tag if tag.starts_with('"') => Js::str(&serde_json::from_str::<String>(tag).unwrap()),
        tag => Js::Number(tag.parse().unwrap()),
    }
}

fn check(method: &str) {
    let rows: Vec<Case> = cases()
        .iter()
        .filter(|case| case.method == method)
        .cloned()
        .collect();
    let expected: Vec<_> = rows.iter().map(|case| case.expected.clone()).collect();
    assert_eq!(expected.len(), 1100, "complete Node position corpus");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let http = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native.clone(),
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let runtime: Rc<dyn ExchangeRuntime> = native.clone();
        let actual =
            Promise::spawn(
                &runtime,
                Box::pin(async move {
                    native.sleep(0).await.map_err(Thrown::from)?;
                    let constructor = shared.globals.get("String")?;
                    let mut actual = vec![];
                    for case in rows {
                        let source =
                            constructor.call(Js::Undefined, vec![Js::utf16(&case.source)])?;
                        let mut args = vec![Js::utf16(&case.needle)];
                        if case.position != "omit" {
                            args.push(position(&case.position));
                        }
                        actual.push(source.call_method(&case.method, args).unwrap_or_else(
                            |error| Js::str(&format!("THROWN: {}", error.value.to_display())),
                        ));
                    }
                    Ok(Js::array(actual))
                }),
            );
        let actual = tokio::time::timeout(Duration::from_secs(5), await_js(&runtime, Js::Promise(actual)))
            .await
            .unwrap()
            .unwrap();
        let actual = to_json(&actual).unwrap();
        let actual = actual.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        let mismatches: Vec<_> = actual
            .iter()
            .zip(&expected)
            .enumerate()
            .filter(
                |(_, (actual, expected))| match (actual.as_f64(), expected.as_f64()) {
                    (Some(actual), Some(expected)) => actual != expected,
                    _ => actual != expected,
                },
            )
            .collect();
        assert!(
            mismatches.is_empty(),
            "{method}: {} of {} Node cases differ; first cases: {:?}",
            mismatches.len(),
            expected.len(),
            mismatches.iter().take(8).collect::<Vec<_>>()
        );
    });
}

#[test]
fn native_index_of_matches_node_positions_and_utf16() {
    check("indexOf");
}
#[test]
fn native_last_index_of_matches_node_positions_and_utf16() {
    check("lastIndexOf");
}
#[test]
fn native_includes_matches_node_positions_and_utf16() {
    check("includes");
}
#[test]
fn native_starts_with_matches_node_positions_and_utf16() {
    check("startsWith");
}
#[test]
fn native_ends_with_matches_node_positions_and_utf16() {
    check("endsWith");
}

#[test]
fn native_search_coercion_order_and_thrown_identity_match_node() {
    let output = Command::new("node").args(["-e", r#"
const rows=[];for(const method of ['indexOf','lastIndexOf','includes','startsWith','endsWith']){
const trace=[],marker={},needle={toString(){trace.push('search');return 'a'}},position={valueOf(){trace.push('position');throw marker}};
try{'aba'[method](needle,position);trace.push('returned')}catch(error){trace.push(error===marker?'identity':'wrong')}
rows.push(trace);
}console.log(JSON.stringify(rows));
"#]).output().unwrap();
    assert!(output.status.success());
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let native = native_runtime();
        let runtime: Rc<dyn ExchangeRuntime> = native.clone();
        let actual = Promise::spawn(
            &runtime,
            Box::pin(async move {
                native.sleep(0).await.map_err(Thrown::from)?;
                let mut rows = vec![];
                for method in [
                    "indexOf",
                    "lastIndexOf",
                    "includes",
                    "startsWith",
                    "endsWith",
                ] {
                    let trace = Rc::new(std::cell::RefCell::new(vec![]));
                    let marker = Js::object(vec![]);
                    let search_trace = trace.clone();
                    let needle = Js::object(vec![(
                        "toString",
                        Js::function("search", move |_, _| {
                            search_trace.borrow_mut().push(Js::str("search"));
                            Ok(Js::str("a"))
                        }),
                    )]);
                    let position_trace = trace.clone();
                    let thrown = marker.clone();
                    let position = Js::object(vec![(
                        "valueOf",
                        Js::function("position", move |_, _| {
                            position_trace.borrow_mut().push(Js::str("position"));
                            Err(Thrown::new(thrown.clone()))
                        }),
                    )]);
                    let outcome = match Js::str("aba").call_method(method, vec![needle, position]) {
                        Err(error) if error.value.strict_equals(&marker) => "identity",
                        Err(_) => "wrong",
                        Ok(_) => "returned",
                    };
                    trace.borrow_mut().push(Js::str(outcome));
                    rows.push(Js::array(trace.borrow().clone()));
                }
                Ok(Js::array(rows))
            }),
        );
        let actual = tokio::time::timeout(Duration::from_secs(5), await_js(&runtime, Js::Promise(actual)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(to_json(&actual).unwrap(), expected);
    });
}
