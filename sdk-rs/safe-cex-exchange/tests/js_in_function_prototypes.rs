#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{client, globals, library::in_operator, scope::ScopeShared, value::Js},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc, sync::Arc};

#[test]
fn shared_in_operator_traverses_function_prototypes_without_invoking_getters() {
    let output = Command::new("node")
        .args([
            "-e",
            r#"
const f=function target(){};const proto={inherited:undefined};let reads=0;
Object.defineProperty(proto,'hidden',{get(){reads++;throw Error('must not read')}});
Object.setPrototypeOf(f,proto);f.own=undefined;
const first=['inherited','hidden','own','missing','name'].map(k=>k in f);
const parent=function parent(){};parent.staticField=1;Object.setPrototypeOf(proto,parent);
const chain=['staticField','call','missing'].map(k=>k in f);
Object.setPrototypeOf(f,null);const detached=['inherited','staticField','own'].map(k=>k in f);
console.log(JSON.stringify({first,chain,detached,reads}));
"#,
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
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
    let object = shared.globals.get("Object").unwrap();
    let function = Js::function("target", |_, _| Ok(Js::Undefined));
    let proto = Js::object(vec![("inherited", Js::Undefined)]);
    let reads = Rc::new(Cell::new(0));
    let observed = reads.clone();
    object
        .call_method(
            "defineProperty",
            vec![
                proto.clone(),
                Js::str("hidden"),
                Js::object(vec![(
                    "get",
                    Js::function("get hidden", move |_, _| {
                        observed.set(observed.get() + 1);
                        panic!("IN_FUNCTION must not invoke getter")
                    }),
                )]),
            ],
        )
        .unwrap();
    object
        .call_method("setPrototypeOf", vec![function.clone(), proto.clone()])
        .unwrap();
    function.set("own", Js::Undefined).unwrap();
    let first: Vec<_> = ["inherited", "hidden", "own", "missing", "name"]
        .into_iter()
        .map(|k| in_operator(&function, k).unwrap())
        .collect();
    assert_eq!(
        json!(first),
        expected["first"],
        "IN_FUNCTION inherited own-undefined and nonenumerable properties remain present"
    );
    let parent = Js::function("parent", |_, _| Ok(Js::Undefined));
    parent.set("staticField", Js::Number(1.0)).unwrap();
    object
        .call_method("setPrototypeOf", vec![proto, parent])
        .unwrap();
    let chain: Vec<_> = ["staticField", "call", "missing"]
        .into_iter()
        .map(|k| in_operator(&function, k).unwrap())
        .collect();
    assert_eq!(
        json!(chain),
        expected["chain"],
        "IN_FUNCTION mixed ordinary/function/intrinsic prototype chain"
    );
    object
        .call_method("setPrototypeOf", vec![function.clone(), Js::Null])
        .unwrap();
    let detached: Vec<_> = ["inherited", "staticField", "own"]
        .into_iter()
        .map(|k| in_operator(&function, k).unwrap())
        .collect();
    assert_eq!(json!(detached), expected["detached"]);
    assert_eq!(json!(reads.get()), expected["reads"]);
    let array = Js::array(vec![Js::Undefined]);
    assert!(in_operator(&array, "length").unwrap());
    assert!(in_operator(&array, "0").unwrap());
    assert!(!in_operator(&array, "1").unwrap());
}
