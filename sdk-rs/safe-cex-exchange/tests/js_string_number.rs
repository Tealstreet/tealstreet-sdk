//! Full-string Number parsing through installed native-host globals.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::{Scope, ScopeShared},
        value::Js,
    },
    native::native_runtime,
};
use std::{future::Future, rc::Rc, sync::Arc};
fn run<F: Future<Output = ()>>(body: impl FnOnce(Scope<()>) -> F) {
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
        safe_cex_exchange::js_runtime::library::install_library(
            &shared.globals,
            Rc::downgrade(&shared),
        );
        body(Scope {
            shared,
            this: Js::Undefined,
        })
        .await;
    });
}

#[test]
fn native_number_rejects_radix_signs_and_preserves_large_radix_rounding() {
    let output=std::process::Command::new("node").args(["-e",r#"
const texts=[];
for(const prefix of ['0x','0X','0b','0B','0o','0O']){
 for(const rest of ['+1','-1','1','+0','++1',' 1','1 ','1e2','1_0','','0001'])texts.push(prefix+rest);
 for(const sign of ['+','-'])texts.push(sign+prefix+'1');
}
texts.push('0x100000000000000000000000000000000','0x'+'f'.repeat(300),'0b1'+'0'.repeat(128),'0o1'+'0'.repeat(50),'0x20000000000001','0x20000000000003','  0xff  ','\ufeff0b10\u2028','+12','-0','1.25e2','Infinity','-Infinity','infinity','0.1tail');
console.log(JSON.stringify(texts.map(text=>{const n=Number(text),b=Buffer.alloc(8);b.writeDoubleBE(n);return {text,nan:Number.isNaN(n),bits:b.toString('hex')}})));
"#]).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 93, "complete Node corpus");
    run(|scope| async move {
        let number = scope.global("Number").unwrap();
        for case in cases {
            let text = case["text"].as_str().unwrap();
            let actual = number
                .call(Js::Undefined, vec![Js::str(text)])
                .unwrap()
                .as_f64()
                .unwrap();
            if case["nan"].as_bool().unwrap() {
                assert!(
                    actual.is_nan(),
                    "Number({text:?}) must be NaN, got {actual}"
                );
            } else {
                assert_eq!(
                    actual.to_bits(),
                    u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap(),
                    "Number({text:?}) bits"
                );
            }
        }
    });
}

#[test]
fn global_is_nan_coerces_once_and_preserves_original_throws() {
    let output = std::process::Command::new("node").args(["-e", r#"
const values=[undefined,null,false,true,0,-0,NaN,Infinity,'','  ','1','bad',[],[1],[1,2],{},1n,Symbol.iterator];
console.log(JSON.stringify(values.map(value=>{try{return isNaN(value)}catch(e){return e.name}})));
"#]).output().unwrap();
    assert!(output.status.success());
    let expected: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    run(|scope| async move {
        let check = scope
            .global("isNaN")
            .expect("HL_SHARED_GLOBAL isNaN is installed");
        let values = vec![
            Js::Undefined,
            Js::Null,
            Js::Bool(false),
            Js::Bool(true),
            Js::Number(0.),
            Js::Number(-0.),
            Js::Number(f64::NAN),
            Js::Number(f64::INFINITY),
            Js::str(""),
            Js::str("  "),
            Js::str("1"),
            Js::str("bad"),
            Js::array(vec![]),
            Js::array(vec![Js::Number(1.)]),
            Js::array(vec![Js::Number(1.), Js::Number(2.)]),
            Js::object(vec![]),
            Js::BigInt(Rc::new(1.into())),
            Js::SymbolIterator,
        ];
        for (value, expected) in values.into_iter().zip(expected) {
            let actual = match check.call(Js::Undefined, vec![value]) {
                Ok(Js::Bool(value)) => serde_json::json!(value),
                Err(error) => serde_json::json!(error.value.get("name").unwrap().to_display()),
                result => panic!("unexpected isNaN result: {result:?}"),
            };
            assert_eq!(actual, expected);
        }
        let calls = Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        let cause = Js::object(vec![]);
        let failure = cause.clone();
        let input = Js::object(vec![(
            "valueOf",
            Js::function("valueOf", move |_, _| {
                observed.set(observed.get() + 1);
                Err(safe_cex_exchange::js_runtime::value::Thrown::new(
                    failure.clone(),
                ))
            }),
        )]);
        assert!(
            check
                .call(Js::Undefined, vec![input])
                .unwrap_err()
                .value
                .same_reference(&cause)
        );
        assert_eq!(calls.get(), 1);
    });
}
