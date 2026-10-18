use super::*;
use safe_cex_exchange::{js_runtime::{client, promise::await_js, value::Js}, runtime::ExchangeRuntime};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse};
use serde_json::json;
use std::{cell::{Cell, RefCell}, collections::BTreeMap, future::Future, sync::Arc, task::{Context, Poll, Waker}};

#[allow(dead_code)]
mod support {
    use crate as safe_cex_venue_asterdex;
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/exchange_runtime.rs"));
}
#[path = "../tests/support/exchange_data.rs"]
mod exchange_data;

#[derive(Default)]
struct Gate { open: Cell<bool>, waker: RefCell<Option<Waker>> }
impl Gate {
    fn release(&self) {
        self.open.set(true);
        if let Some(waker) = self.waker.borrow_mut().take() { waker.wake(); }
    }
}
#[derive(Default)]
struct Wire { inner: support::Wire, gates: RefCell<BTreeMap<String, Rc<Gate>>> }
impl RestAdapter for Wire {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        let gate = self.gates.borrow().iter().find(|(path,_)| request.url.contains(path.as_str())).map(|(_,gate)|gate.clone());
        let response = self.inner.execute(request);
        Box::pin(async move {
            if let Some(gate) = gate {
                std::future::poll_fn(move |cx| {
                    if gate.open.get() { Poll::Ready(()) } else { *gate.waker.borrow_mut() = Some(cx.waker().clone()); Poll::Pending }
                }).await;
            }
            response.await
        })
    }
}
struct Fixture { venue: Rc<AsterDEXExchange>, runtime: Rc<support::Runtime>, store: Rc<DefaultStore>, wire: Arc<Wire> }
impl Fixture {
    fn new() -> Self {
        let runtime = support::runtime_fixture();
        #[expect(clippy::arc_with_non_send_sync)]
        let wire = Arc::new(Wire::default());
        exchange_data::script(&wire.inner);
        let store = Rc::new(DefaultStore::new());
        let rt = runtime.clone(); let http = wire.clone();
        let venue = AsterDEXExchange::new(ExchangeName::Asterdex, serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret","tuning":{"consoleLogging":false}})).unwrap(), store.clone(), AsterDEXDependencies {
            runtime: runtime.clone(), http: Rc::new(move |_|Ok(client::rest_http_host(rt.clone(),http.clone()))), external_public_data: None
        }).unwrap();
        Self { venue,runtime,store,wire }
    }
    fn hold(&self,path:&str)->Rc<Gate> {
        let gate=Rc::new(Gate::default()); self.wire.gates.borrow_mut().insert(path.into(),gate.clone());gate
    }
    fn tickers(&self) {
        let ticker = serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":0,"ask":0,"last":1,"mark":0,"index":0,"percentage":0,"fundingRate":0,"fundingTime":0,"openInterest":0,"volume":0,"quoteVolume":0})).unwrap();
        Store::update(self.store.as_ref(),StoreUpdate {tickers:Some(vec![ticker]),..Default::default()},false).unwrap();
    }
}
impl Drop for Fixture { fn drop(&mut self) { self.venue.dispose().unwrap(); } }

#[test]
fn clock_settles_and_clears_its_slot_after_disposal() {
    let f=Fixture::new();let gate=f.hold("/time");
    let clock=f.venue.rt.shared.extra.clock.get().unwrap();
    let mut pending=Box::pin(clock.sync_client_server_time(false,None));
    f.runtime.pump();
    assert!(clock.is_syncing());
    assert_eq!(f.wire.inner.requests.borrow().len(),1);
    f.venue.dispose().unwrap(); gate.release();f.runtime.pump();
    assert!(matches!(pending.as_mut().poll(&mut Context::from_waker(Waker::noop())),Poll::Ready(Ok(()))),"ASTER-CLOCK-SETTLEMENT in-flight source Promise must settle after dispose");
    assert!(!clock.is_syncing(),"finally must clear the shared slot");
    assert_eq!(clock.get_time_offset(),0.0,"stopped generation must not write clock state");
}

#[test]
fn open_interest_teardown_cancels_late_effects_without_another_cycle() {
    let f=Fixture::new();f.tickers();let gate=f.hold("/openInterest");
    f.venue.rt.this.call_method("startOpenInterestPolling",vec![]).unwrap();
    f.runtime.pump();
    assert!(f.wire.inner.requests.borrow().iter().any(|r|r.url.contains("/openInterest")),"detached poll must reach actual HTTP");f.venue.dispose().unwrap();f.tickers();gate.release();f.runtime.pump();
    assert_eq!(f.store.tickers()[0].open_interest,0.0,"accepted native teardown suppresses late Store effects");
    let count=f.wire.inner.requests.borrow().len();f.runtime.now.set(f.runtime.now.get()+30_000.0);f.runtime.pump();
    assert_eq!(f.wire.inner.requests.borrow().len(),count,"disposed poll must not schedule another cycle");
}

#[test]
fn position_repair_waits_before_dispose_and_cleans_flight_on_teardown() {
    let f=Fixture::new();let gate=f.hold("/positionRisk");
    let repairs=&f.venue.rt.shared.extra.private.get().unwrap().repairs;
    repairs.run_position_repair();f.runtime.pump();assert!(repairs.state().position_in_flight);
    f.venue.dispose().unwrap();f.runtime.pump();
    assert!(!repairs.state().position_in_flight,"accepted native teardown clears busy and suppresses late Store effects");
    gate.release();f.runtime.pump();assert!(!repairs.state().position_in_flight);
    assert!(f.store.positions().is_empty());assert!(!f.store.loaded().positions);
}

#[test]
fn private_timers_use_fixed_delays_and_are_cleared_by_source_dispose() {
    let f=Fixture::new();let stream=f.venue.rt.shared.extra.private.get().unwrap();
    support::drive(&f.runtime,stream.listen_key.fetch_listen_key()).unwrap();
    stream.repairs.schedule_balance_reconcile(0.0).unwrap();stream.repairs.schedule_order_reconcile().unwrap();
    let state=stream.listen_key.state();assert!(state.refresh_timer&&state.watchdog_timer);
    assert!(stream.repairs.state().balance_timer&&stream.repairs.state().order_timer);
    f.venue.dispose().unwrap();
    let state=stream.listen_key.state();assert!(!state.refresh_timer&&!state.watchdog_timer);
    let state=stream.repairs.state();assert!(!state.balance_timer&&!state.order_timer);
    let count=f.wire.inner.requests.borrow().len();f.runtime.now.set(f.runtime.now.get()+4_300_000.0);f.runtime.pump();
    assert_eq!(f.wire.inner.requests.borrow().len(),count,"disposed source clears each timer; arbitrary delays are not native call inputs");
    assert!(f.store.orders().is_empty()&&f.store.positions().is_empty());
}

#[test]
fn sync_position_mode_preserves_native_axios_name_and_cause() {
    use safe_cex_exchange::js_runtime::{client::JsHttpHost,promise::JsFuture,value::{Thrown,error_object}};
    struct Host { runtime:Rc<support::Runtime>, original:Js }
    impl JsHttpHost for Host {
        fn runtime(&self)->Rc<dyn ExchangeRuntime>{self.runtime.clone()}
        fn send(&self,config:Js)->JsFuture<'static>{
            let original=self.original.clone();original.define("config",config).unwrap();Box::pin(async move{Err(Thrown::new(original))})
        }
    }
    let runtime=support::runtime_fixture();let original=error_object("AxiosError","source rejection");original.define("isAxiosError",Js::Bool(true)).unwrap();
    let host=Rc::new(Host{runtime:runtime.clone(),original:original.clone()});
    let venue=AsterDEXExchange::new(ExchangeName::Asterdex,serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),Rc::new(DefaultStore::new()),AsterDEXDependencies{runtime:runtime.clone(),http:Rc::new(move |_|Ok(host.clone())),external_public_data:None}).unwrap();
    let pending=venue.rt.this.call_method("syncPositionMode",vec![]).unwrap();
    let error=support::drive(&runtime,await_js(&venue.rt.exchange_runtime(),pending)).unwrap_err();
    assert_eq!(error.value.get("name").unwrap().to_display(),"AxiosError","source rethrows the original AxiosError class");
    let thrown=error;
    assert!(thrown.value.strict_equals(&original),"source rethrows the original AxiosError identity");
    venue.dispose().unwrap();
}

#[test]
fn signature_diagnostics_keep_error_class_without_secret_material() {
    let f=Fixture::new();let key="0xDEAD_private_fixture_secret";
    let error=f.venue.rt.shared.globals.call_method("generateAsterDEXCodeMessageSignature",vec![Js::str("x=1"),Js::str(key),Js::Bool(false)]).unwrap_err();
    assert_eq!(error.value.get("name").unwrap().to_display(),"TypeError");
    let message=error.value.get("message").unwrap().to_display();
    assert!(!message.contains(key)&&!message.contains("private_fixture_secret"));
}
