#[path = "support/binding_runtime.rs"]
mod support;
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown},
    },
    runtime::ExchangeRuntime,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_venue_bitrue::{BitrueDependencies, BitrueExchange};
use std::{
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Http(Rc<support::Runtime>);
impl JsHttpHost for Http {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _config: Js) -> JsFuture<'static> {
        Box::pin(async { Err(Thrown::error("Error", "unexpected HTTP request")) })
    }
}
fn complete<T>(rt: &support::Runtime, future: impl Future<Output = T>) -> T {
    let mut f = pin!(future);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..128 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
        rt.pump();
        rt.advance(1000.);
    }
    panic!("Bitrue binding future did not complete")
}
#[test]
fn constructs_production_bitrue_and_inherits_source_account_rejection() {
    let runtime = Rc::new(support::Runtime::default());
    let venue = BitrueExchange::new(
        ExchangeName::Bitrue,
        serde_json::from_value::<ExchangeOptions>(
            serde_json::json!({"key":"","secret":"","applicationId":"fixture-app"}),
        )
        .unwrap(),
        Rc::new(DefaultStore::new()),
        BitrueDependencies {
            runtime: runtime.clone(),
            http: Rc::new(Http(runtime.clone())),
            external_public_data: None,
        },
    )
    .unwrap();
    assert_eq!(
        complete(&runtime, venue.get_account()).unwrap_err().message,
        "Not implemented"
    );
    venue.dispose().unwrap();
}
