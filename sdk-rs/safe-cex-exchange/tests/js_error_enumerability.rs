//! Error instances through actual installed globals and the native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals,
        scope::ScopeShared,
        value::{Js, Prop, Thrown, to_json, utf16},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{process::Command, rc::Rc, sync::Arc};
fn args(tag: &str) -> Vec<Js> {
    let value = match tag {
        "omitted" => return vec![],
        "undefined" => Js::Undefined,
        "null" => Js::Null,
        "empty" => Js::str(""),
        "text" => Js::str("failure"),
        "number" => Js::Number(17.),
        "negative_zero" => Js::Number(-0.),
        "nan" => Js::Number(f64::NAN),
        "infinity" => Js::Number(f64::INFINITY),
        "surrogate" => Js::utf16(&[0xd800]),
        "object" => Js::object(vec![(
            "toString",
            Js::function("toString", |_, _| Ok(Js::str("coerced"))),
        )]),
        "throwing_object" => Js::object(vec![(
            "toString",
            Js::function("toString", |_, _| {
                Err(Thrown::error("RangeError", "conversion"))
            }),
        )]),
        "symbol" => Js::SymbolIterator,
        _ => panic!("unknown source input {tag}"),
    };
    vec![value]
}
fn units(value: Js) -> Value {
    json!(value.to_js_string().unwrap().as_ref())
}
// Observe native stored descriptors independently of JS descriptor reflection.
// Creation/keys/assign/JSON still use the actual installed globals.
fn descriptor(error: &Js, key: &str) -> Value {
    let Js::Object(object) = error else {
        panic!("Error must be an object")
    };
    let property = object.props.borrow().get(utf16(key).as_ref()).cloned();
    let (value, writable, enumerable, configurable) = match property {
        None => return Value::Null,
        Some(Prop::Data(value)) => (value, true, true, true),
        Some(Prop::DataDescriptor {
            value,
            writable,
            enumerable,
            configurable,
        }) => (value, writable, enumerable, configurable),
        Some(Prop::Accessor { .. }) => panic!("Error instance descriptor must be data"),
    };
    json!({"value":units(value),"writable":writable,"enumerable":enumerable,"configurable":configurable})
}
#[test]
fn native_error_instance_descriptors_and_serialization_match_node() {
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_error_enumerability_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 156);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt,async{
  let runtime=native_runtime();let http=client::rest_http_host(runtime.clone(),Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()));
  let shared=Rc::new(ScopeShared::new(runtime,http,Rc::new(safe_cex_store::store::DefaultStore::new()),Js::object(vec![]),()));
  globals::install_builtins(&shared.globals,Rc::downgrade(&shared));
  let object=shared.globals.get("Object").unwrap().clone();let json=shared.globals.get("JSON").unwrap().clone();
  for row in cases{
   let ctor=shared.globals.get(row["ctor"].as_str().unwrap()).unwrap().clone();
   let result=match ctor.call(Js::Undefined,args(row["tag"].as_str().unwrap())){
    Err(error)=>json!({"thrown":error.value.get("name").unwrap().to_display()}),
    Ok(error)=>{
     match row["op"].as_str().unwrap(){"message"=>error.set("message",Js::str("changed")).unwrap(),"name"=>error.set("name",Js::str("UserError")).unwrap(),"delete_message"=>error.delete(&utf16("message")),"pristine"=>{},_=>unreachable!()};
     let copy=object.call_method("assign",vec![Js::object(vec![]),error.clone()]).unwrap();
     json!({"name":units(error.get("name").unwrap()),"message":units(error.get("message").unwrap()),"keys":to_json(&object.call_method("keys",vec![error.clone()]).unwrap()).unwrap(),"json":json.call_method("stringify",vec![error.clone()]).unwrap().to_display(),"copy":json.call_method("stringify",vec![copy]).unwrap().to_display(),"spread":json.call_method("stringify",vec![error.spread_object().unwrap()]).unwrap().to_display(),"nameDescriptor":descriptor(&error,"name"),"messageDescriptor":descriptor(&error,"message"),"render":units(error)})
    }
   };
   assert_eq!(result,row["result"],"ERROR {} {} {}",row["ctor"],row["tag"],row["op"]);
  }
  // Custom native Axios diagnostics retain their distinct enumerable name/message.
  let axios=safe_cex_exchange::js_runtime::value::error_object("AxiosError","failure");
  assert_eq!(json!({"nameDescriptor":descriptor(&axios,"name"),"messageDescriptor":descriptor(&axios,"message")}),source["axios"]);
 });
}
