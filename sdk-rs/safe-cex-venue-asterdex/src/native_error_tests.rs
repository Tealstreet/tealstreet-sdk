use super::*;
use safe_cex_exchange::{js_runtime::{client::JsHttpHost, promise::{JsFuture,await_js}, value::{Thrown,error_object}}, runtime::ExchangeRuntime};
use safe_cex_store::store::DefaultStore;
use serde_json::json;

#[allow(dead_code)]
mod support {
    use crate as safe_cex_venue_asterdex;
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/exchange_runtime.rs"));
}
struct Host { runtime:Rc<support::Runtime>, original:Js }
impl JsHttpHost for Host {
    fn runtime(&self)->Rc<dyn ExchangeRuntime>{self.runtime.clone()}
    fn send(&self,config:Js)->JsFuture<'static>{
        let original=self.original.clone();original.define("config",config).unwrap();Box::pin(async move{Err(Thrown::new(original))})
    }
}
#[test]
fn sync_position_mode_retains_native_axios_error_observations() {
    let runtime=support::runtime_fixture();
    let original=error_object("AxiosError","native Aster rejection");
    original.define("isAxiosError",Js::Bool(true)).unwrap();
    original.define("code",Js::str("ERR_ASTER_REJECTED")).unwrap();
    original.define("stack",Js::str("AxiosError: native Aster rejection\n  at terminal-host")).unwrap();
    let cause=error_object("TypeError","root cause");
    cause.define("number",Js::Number(f64::INFINITY)).unwrap();cause.define("missing",Js::Undefined).unwrap();
    original.define("cause",cause).unwrap();
    let host=Rc::new(Host{runtime:runtime.clone(),original:original.clone()});
    let venue=AsterDEXExchange::new(ExchangeName::Asterdex,serde_json::from_value(json!({"key":"fixture","secret":"fixture","tuning":{"consoleLogging":false}})).unwrap(),Rc::new(DefaultStore::new()),AsterDEXDependencies{runtime:runtime.clone(),http:Rc::new(move |_|Ok(host.clone())),external_public_data:None}).unwrap();
    let pending=venue.rt.this.call_method("syncPositionMode",vec![]).unwrap();
    let error=support::drive(&runtime,await_js(&venue.rt.exchange_runtime(),pending)).unwrap_err();
    for field in ["name","message","code","stack","isAxiosError"] {
        assert!(error.value.get(field).unwrap().strict_equals(&original.get(field).unwrap()),"ASTER-NATIVE-ERROR must preserve {field}");
    }
    let Js::Object(object)=&error.value else {panic!("native Error instance")};
    assert_eq!(object.class,"Error");
    let cause=error.value.get("cause").unwrap();
    assert_eq!(cause.get("name").unwrap().to_display(),"TypeError");
    assert_eq!(cause.get("message").unwrap().to_display(),"root cause");
    assert_eq!(cause.get("number").unwrap().to_number().unwrap(),f64::INFINITY);
    assert!(cause.has_own("missing")&&cause.get("missing").unwrap().is_undefined());
    assert!(error.value.get("response").unwrap().is_undefined(), "source network rejection has no response");
    assert!(!error.value.strict_equals(&original),"accepted owned-error boundary snapshots identity");
    venue.dispose().unwrap();
}

#[test]
fn native_error_contract_matches_both_maintained_typescript_clients() {
    let result = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args(["--import", "tsx", concat!(env!("CARGO_MANIFEST_DIR"), "/../safe-cex-exchange/tests/rest-rejection-source.cjs")])
        .output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    let text = String::from_utf8(result.stdout).unwrap();
    let result: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(result, json!({"asterdex":true,"woo":true}));
}
