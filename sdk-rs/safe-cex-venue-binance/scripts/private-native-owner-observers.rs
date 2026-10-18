// Terminal runtime/socket fixture reused without replacing any venue operation.
#[path = "../tests/support/inverse_runtime.rs"]
pub mod terminal;
#[cfg(not(test))]
use crate as venue;
use safe_cex_exchange::{js_runtime::value::Js, native::NativeHttpEvents};
use safe_cex_store::store::DefaultStore;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse};
#[cfg(test)]
use safe_cex_venue_binance as venue;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use venue::{
    exchange_classes::ExchangeOwner, private_classes::PrivateState, private_session::Venue,
};
struct NoHttp;
impl RestAdapter for NoHttp {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("private-owner observations do not send HTTP")
    }
}
pub struct Harness {
    pub owner: Rc<ExchangeOwner>,
    pub runtime: Rc<terminal::Runtime>,
    pub errors: Rc<RefCell<Vec<Vec<Js>>>>,
    pub logs: Rc<RefCell<Vec<Vec<Js>>>>,
}
impl Harness {
    pub fn state(&self) -> Rc<PrivateState> {
        std::ops::Deref::deref(self.owner.private.as_ref()).clone()
    }
}
pub fn actual() -> Harness {
    let runtime = Rc::new(terminal::Runtime::default());
    runtime.now.set(terminal::START_MS);
    let deps = venue::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(NoHttp),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        NativeHttpEvents::default(),
    );
    let owner = ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"native-key","secret":"native-secret"})).unwrap(),
        Rc::new(DefaultStore::new()),
        deps,
    )
    .unwrap();
    let errors = Rc::new(RefCell::new(vec![]));
    let seen = errors.clone();
    owner.base.on_args(
        "error",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    let logs = Rc::new(RefCell::new(vec![]));
    let seen = logs.clone();
    owner.base.on_args(
        "log",
        Rc::new(move |_, args| {
            seen.borrow_mut().push(args.to_vec());
            Ok(())
        }),
    );
    Harness {
        owner,
        runtime,
        errors,
        logs,
    }
}
pub fn event(input: &Value) -> Js {
    safe_cex_exchange::js_runtime::value::from_json(&json!({"e":"ORDER_TRADE_UPDATE", "o":{
        "i":input["id"],"c":"native-client","X":"NEW","x":"NEW","ot":"LIMIT","o":"LIMIT",
        "s":"BTCUSDT","S":"BUY","p":"91.2","ap":"0","sp":"0","q":input["amount"],"z":"0","R":false
    }}))
}
pub fn rows(h: &Harness) -> Value {
    let rows = h.owner.base.store.orders();
    json!(
        rows.iter()
            .map(
                |r| json!({"id":r.id,"amountBits":format!("{:016x}",r.amount.to_bits()),
        "priceBits":format!("{:016x}",r.price.to_number().to_bits())})
            )
            .collect::<Vec<_>>()
    )
}
