//! Shared production qs binding, compared to the installed qs implementation.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::ScopeShared,
        value::{Js, arg, from_json},
    },
    native::native_runtime,
};
use std::{rc::Rc, sync::Arc};

#[test]
#[ignore = "requires installed qs oracle"]
fn native_qs_options_match_the_installed_package() {
    let node = std::process::Command::new("node").arg("-e").arg(r#"
const path=require('node:path');
const {createRequire}=require('node:module');
const load=process.env.SAFE_CEX_TYPESCRIPT_PATH
  ? createRequire(path.join(process.env.SAFE_CEX_TYPESCRIPT_PATH,'package.json')) : require;
const qs=load('qs');
const inputs=[
 {Zeta:'(x y)',alpha:['雪',null,''],_b:{z:[1,2],a:'x+y'},empty:[]},
 {ids:Array.from({length:12},(_,i)=>i),nested:{Zeta:1,alpha:2,_b:3}},
 [{z:'a b'},null,'literal%20space']
];
const cases=[];
for(const params of inputs) for(const arrayFormat of ['indices','repeat','comma','brackets'])
 for(const format of ['RFC3986','RFC1738']) for(const encode of [true,false]) for(const sorted of [false,true]) {
 const options={arrayFormat,format,encode};
 const expected=qs.stringify(params,{...options,...(sorted?{sort:(a,b)=>a.localeCompare(b)}:{})});
 cases.push({params,options,sorted,expected});
}
console.log(JSON.stringify(cases));
"#).output().unwrap();
    assert!(
        node.status.success(),
        "{}",
        String::from_utf8_lossy(&node.stderr)
    );
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&node.stdout).unwrap();
    assert_eq!(cases.len(), 96, "independent qs option corpus");
    let tokio = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&tokio, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        let qs = shared.globals.get("qs").unwrap();
        for case in cases {
            let options = from_json(&case["options"]);
            if case["sorted"] == true {
                let collator = safe_cex_num::JsRootCollator::try_new().unwrap();
                options
                    .define(
                        "sort",
                        Js::function("localeCompare", move |_, args| {
                            let left = arg(&args, 0).to_display();
                            let right = arg(&args, 1).to_display();
                            Ok(Js::Number(match collator.compare(&left, &right) {
                                std::cmp::Ordering::Less => -1.0,
                                std::cmp::Ordering::Equal => 0.0,
                                std::cmp::Ordering::Greater => 1.0,
                            }))
                        }),
                    )
                    .unwrap();
            }
            let result = qs
                .call_method("stringify", vec![from_json(&case["params"]), options])
                .unwrap();
            assert_eq!(
                result.to_display(),
                case["expected"].as_str().unwrap(),
                "qs options {case}"
            );
        }
    });
}
