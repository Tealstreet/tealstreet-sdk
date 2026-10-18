//! Numeric-hint primitive conversion on the production native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Thrown},
    },
    native::native_runtime,
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[test]
fn native_number_conversion_matches_node_numeric_hint_order() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(runtime.clone(), Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
        let shared = Rc::new(ScopeShared::new(runtime, http,
            Rc::new(safe_cex_store::store::DefaultStore::new()), Js::object(vec![]), ()));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));

        let output = std::process::Command::new("node").args(["-e", r#"
const rows=[];
for(let mode=0;mode<8;mode++) {
 const trace=[];
 const obj={valueOf(){trace.push('valueOf');return [7,{},null,undefined,-0,1n,{},false][mode]},
 toString(){trace.push('toString');return mode===6?{}:'9'}};
 if(mode===2)obj.valueOf=null;
 try{const n=+obj;rows.push({trace,result:Number.isNaN(n)?'NaN':Object.is(n,-0)?'-0':String(n)})}
 catch(e){rows.push({trace,error:e.name})}
}
console.log(JSON.stringify(rows));
"#]).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let expected:Vec<serde_json::Value>=serde_json::from_slice(&output.stdout).unwrap();
        for (mode, expected) in expected.iter().enumerate() {
            let trace = Rc::new(RefCell::new(Vec::<String>::new()));
            let value_trace = trace.clone();
            let string_trace = trace.clone();
            let obj = Js::object(vec![]);
            let value_of = Js::function("valueOf", move |this, _| {
                assert!(this.has_own("toString"), "original receiver");
                value_trace.borrow_mut().push("valueOf".into());
                Ok(match mode {
                    0 => Js::Number(7.),
                    1 | 6 => Js::object(vec![]),
                    3 => Js::Undefined,
                    4 => Js::Number(-0.),
                    5 => Js::BigInt(Rc::new(1.into())),
                    7 => Js::Bool(false),
                    _ => unreachable!(),
                })
            });
            obj.define("valueOf", if mode==2 {Js::Null} else {value_of}).unwrap();
            obj.define("toString", Js::function("toString", move |_, _| {
                string_trace.borrow_mut().push("toString".into());
                Ok(if mode==6 {Js::object(vec![])} else {Js::str("9")})
            })).unwrap();
            let result = obj.to_number();
            let actual = match result {
                Ok(n) => serde_json::json!({"trace":*trace.borrow(),"result":if n.is_nan(){"NaN".to_owned()}else if n==0. && n.is_sign_negative(){"-0".to_owned()}else{safe_cex_num::js_number_string(n)}}),
                Err(e) => serde_json::json!({"trace":*trace.borrow(),"error":e.value.get("name").unwrap().to_display()}),
            };
            assert_eq!(&actual, expected, "numeric hint case {mode}");
        }
        let number = shared.globals.get("Number").unwrap().clone();
        let marker = Js::object(vec![("id",Js::str("getter failure"))]);
        let thrown = marker.clone();
        let obj = Js::object(vec![]);
        obj.define_accessor("valueOf",Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),None);
        obj.define("toString",Js::function("must not run",|_,_|panic!("numeric getter error must propagate"))).unwrap();
        let error=number.call(Js::Undefined,vec![obj]).unwrap_err();
        assert!(error.value.strict_equals(&marker));

        let obj=Js::object(vec![]);
        obj.define_accessor("valueOf",Some(Rc::new(move |this| {
            this.define("toString",Js::function("late string",|_,_|Ok(Js::str("12"))))?;
            Ok(Js::function("object result",|_,_|Ok(Js::object(vec![]))))
        })),None);
        assert_eq!(number.call(Js::Undefined,vec![obj]).unwrap().as_f64(),Some(12.));
        let inherited = Js::object(vec![("marker", Js::Number(17.))]);
        let proto = Js::object(vec![("valueOf", Js::function("inherited", |this, _| this.get("marker")))]);
        if let Js::Object(data) = &inherited { *data.proto.borrow_mut() = Some(proto); }
        assert_eq!(number.call(Js::Undefined, vec![inherited]).unwrap().as_f64(), Some(17.));
        let null_proto = Js::object(vec![]);
        if let Js::Object(data) = &null_proto { *data.proto.borrow_mut() = Some(Js::Null); }
        let error = null_proto.to_number().unwrap_err();
        assert_eq!(error.value.get("name").unwrap().to_display(), "TypeError");
        let function=Js::function("source function",|_,_|unreachable!());
        function.define("valueOf",Js::function("numeric value",|_,_|Ok(Js::Number(13.)))).unwrap();
        assert_eq!(number.call(Js::Undefined,vec![function]).unwrap().as_f64(),Some(13.));
    });
}
