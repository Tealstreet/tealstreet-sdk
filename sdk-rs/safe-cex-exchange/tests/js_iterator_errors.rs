#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        promise::await_js,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use serde_json::Value;
use std::{process::Command, rc::Rc, sync::Arc};

fn input(label: &str) -> Js {
    match label {
        "object" => Js::object(vec![]),
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "false" => Js::Bool(false),
        "true" => Js::Bool(true),
        "zero" => Js::Number(0.0),
        "negative-zero" => Js::Number(-0.0),
        "number" => Js::Number(5.0),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "negative-infinity" => Js::Number(f64::NEG_INFINITY),
        "bigint" => Js::BigInt(Rc::new(123.into())),
        "symbol" => Js::SymbolIterator,
        "function" => Js::function("input", |_, _| Ok(Js::Undefined)),
        "poisonous-coercion" => Js::object(vec![
            (
                "toString",
                Js::function("toString", |_, _| {
                    panic!("diagnostics must not coerce the input")
                }),
            ),
            (
                "valueOf",
                Js::function("valueOf", |_, _| {
                    panic!("diagnostics must not coerce the input")
                }),
            ),
        ]),
        label => panic!("unknown oracle input {label}"),
    }
}

fn diagnostic(error: Thrown) -> Value {
    serde_json::json!({
        "name":error.value.get("name").unwrap().to_display(),
        "message":error.value.get("message").unwrap().to_display(),
    })
}

#[test]
fn native_promise_iterator_diagnostics_match_node() {
    let oracle = Command::new("node")
        .args([
            "-e",
            r#"
(async () => {
 const poison = () => { throw new Error('diagnostics must not coerce the input'); };
 const inputs = {object:{}, undefined, null:null, false:false, true:true, zero:0,
  'negative-zero':-0, number:5, nan:NaN, infinity:Infinity, 'negative-infinity':-Infinity,
  bigint:123n, symbol:Symbol.iterator, function:function input(){},
  'poisonous-coercion':{toString:poison,valueOf:poison}};
 const rows=[];
 for(const method of ['all','allSettled']) for(const [label,input] of Object.entries(inputs)) {
  try { await Promise[method](input); throw new Error('expected rejection'); }
  catch(error) { rows.push({method,label,expected:{name:error.name,message:error.message}}); }
 }
 process.stdout.write(JSON.stringify(rows));
})().catch(error => {console.error(error);process.exitCode=1;});
"#,
        ])
        .output()
        .expect("Node oracle");
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&oracle.stdout).unwrap();
    assert_eq!(rows.len(), 30);
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor, async {
        let native = native_runtime();
        let http = client::rest_http_host(
            native.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            native,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        let promise = shared.globals.get("Promise").unwrap();
        for row in rows {
            let value = input(row["label"].as_str().unwrap());
            let result = promise
                .call_method(row["method"].as_str().unwrap(), vec![value.clone()])
                .expect("invalid iterable rejects the returned Promise");
            assert!(matches!(result, Js::Promise(_)));
            assert_eq!(
                diagnostic(await_js(&(shared.runtime.clone() as Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime>), result).await.unwrap_err()),
                row["expected"],
                "{row}"
            );
            assert_eq!(
                diagnostic(value.iterator().unwrap_err()),
                row["expected"],
                "GetIterator: {row}"
            );
        }
    });
}
