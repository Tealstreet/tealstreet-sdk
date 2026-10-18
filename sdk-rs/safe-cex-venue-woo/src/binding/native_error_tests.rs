use super::*;
use crate::binding_test_harness::Runtime;
use safe_cex_exchange::{js_runtime::{client::{JsHttpHost,JsRestAdapter},promise::JsFuture,value::{Js,Thrown,error_object}},runtime::{ExchangeError,ExchangeRuntime}};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{RestAdapter,RestRequest};
use std::{cell::RefCell,future::Future,rc::Rc,task::{Context,Poll,Waker}};

struct Host { runtime:Rc<Runtime>, original:Js, requests:Rc<RefCell<Vec<Js>>> }
impl JsHttpHost for Host {
    fn runtime(&self)->Rc<dyn ExchangeRuntime>{self.runtime.clone()}
    fn send(&self,config:Js)->JsFuture<'static>{self.requests.borrow_mut().push(config);let original=self.original.clone();Box::pin(async move{Err(Thrown::new(original))})}
}
fn drive<T>(runtime:&Runtime,future:impl Future<Output=T>)->T {
    let mut future=std::pin::pin!(future);
    for _ in 0..100 {runtime.pump();if let Poll::Ready(value)=future.as_mut().poll(&mut Context::from_waker(Waker::noop())){return value;}runtime.advance(100.0);}
    panic!("native WOO error must settle")
}
#[test]
fn constructed_woo_client_keeps_error_fields_through_typed_rest_adapter() {
    let runtime=Rc::new(Runtime::default());runtime.now.set(1_700_000_000_000.0);
    let original=error_object("TypeError","native WOO rejection");
    for (field,value) in [("code",Js::str("ERR_WOO_REJECTED")),("stack",Js::str("TypeError: native WOO rejection\n  at terminal-host")),("isAxiosError",Js::Bool(true)),("cause",Js::object(vec![("message",Js::str("native root cause"))]))] {original.define(field,value).unwrap();}
    original.define("response",Js::object(vec![("status",Js::Number(418.0)),("headers",Js::object(vec![])),("data",Js::object(vec![("message",Js::str("denied"))]))])).unwrap();
    let requests=Rc::new(RefCell::new(vec![]));
    let exchange=WooExchange::new(safe_cex_types::ExchangeName::Woo,crate::binding_test_harness::options(),Rc::new(DefaultStore::new()),WooDependencies{runtime:runtime.clone(),http:Rc::new(Host{runtime:runtime.clone(),original:original.clone(),requests:requests.clone()}),external_public_data:None}).unwrap();
    let adapter=JsRestAdapter::new(exchange.rt.exchange_runtime(),exchange.rt.this.get("xhr").unwrap());
    let failure=drive(&runtime,adapter.execute(RestRequest{method:"GET".into(),url:"/v3/accountinfo".into(),..Default::default()})).unwrap_err();
    let native=ExchangeError::from(failure);
    let error=Thrown::from(native);
    for field in ["name","message","code","stack","isAxiosError"] {assert!(error.value.get(field).unwrap().strict_equals(&original.get(field).unwrap()),"WOO-NATIVE-ERROR must preserve {field}");}
    assert_eq!(error.value.get("cause").unwrap().get("message").unwrap().to_display(),"native root cause");
    assert_eq!(error.value.get("response").unwrap().get("status").unwrap().to_number().unwrap(),418.0);
    assert_eq!(error.value.get("response").unwrap().get("data").unwrap().get("message").unwrap().to_display(),"denied");
    let Js::Object(object)=&error.value else {panic!("native Error instance")};assert_eq!(object.class,"Error");
    assert_eq!(requests.borrow().len(),1);
    assert!(!requests.borrow()[0].get("headers").unwrap().get("x-api-signature").unwrap().to_display().is_empty(),"production WOO signer executed before terminal rejection");
    use safe_cex_exchange::exchange::Exchange;
    exchange.dispose().unwrap();
}
