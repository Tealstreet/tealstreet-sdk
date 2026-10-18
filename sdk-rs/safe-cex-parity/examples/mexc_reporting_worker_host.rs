//! Real MEXC reader -> BaseExchange -> installed worker bridge -> browser post.
#[cfg(target_arch = "wasm32")]
mod browser {
    use safe_cex_exchange::native_store::DefaultStore;
    use safe_cex_exchange::{
        exchange::{BaseExchange, Exchange, ExchangeSettings},
        runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
        websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
    };
    use safe_cex_transport::{
        RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
        TransportProvider, socket::TunnelHost,
    };
    use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput};
    use safe_cex_venue_mexc::{exchange::MexcExchange, sockets::MexcSocketDependencies};
    use safe_cex_webworker::{
        data_post::WorkerDataPost, exchange_worker_start::install_exchange_event_bridge,
    };
    use serde_json::{Value, json};
    use std::{
        cell::Cell,
        future::Future,
        pin::Pin,
        rc::Rc,
        sync::Arc,
        task::{Context, Poll, Waker},
    };
    use wasm_bindgen::prelude::*;

    struct WorkerRuntime;
    impl ExchangeRuntime for WorkerRuntime {
        fn monotonic_now_ms(&self) -> f64 {
            100.
        }
        fn now_ms(&self) -> f64 {
            100.
        }
        fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
            panic!("unexpected sleep")
        }
        fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
            panic!("unexpected spawn")
        }
        fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
        fn unique_id(&self) -> String {
            "reporting-host".into()
        }
        fn is_worker(&self) -> bool {
            true
        }
        fn random(&self) -> f64 {
            0.5
        }
        fn report_exception(
            &self,
            _: &ExchangeError,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
            panic!("worker must post")
        }
        fn report_raw_exception(
            &self,
            _: &safe_cex_exchange::js_runtime::value::Js,
            _: &safe_cex_types::retained_json::RetainedJsonInput,
        ) {
            panic!("worker must post")
        }
        fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
            panic!("unexpected RPC")
        }
    }
    impl WsRuntime for WorkerRuntime {
        fn transport(&self) -> Arc<dyn TransportProvider> {
            Arc::new(safe_cex_transport::DirectTransport)
        }
        fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
            panic!("unexpected tunnel")
        }
        fn dial(
            &self,
            _: String,
            _: Vec<String>,
            _: WebSocketConnectionOptions,
            _: bool,
        ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
            panic!("unexpected socket")
        }
        fn headers_capable(&self) -> bool {
            true
        }
        fn enable_reconnecting(&self) -> bool {
            false
        }
        fn min_reconnection_delay_ms(&self) -> f64 {
            1000.
        }
        fn library_min_reconnection_delay_ms(&self) -> f64 {
            3000.
        }
        fn diagnostics_enabled(&self) -> bool {
            false
        }
        fn lifecycle_logs_enabled(&self) -> bool {
            false
        }
    }
    struct Failure {
        numeric: bool,
        calls: Cell<usize>,
    }
    impl RestAdapter for Failure {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            assert_eq!(request.method, "get");
            assert!(matches!(
                request.url.as_str(),
                "/api/v1/private/position/open_positions" | "/api/v1/private/account/assets"
            ));
            self.calls.set(self.calls.get() + 1);
            let mut failure = RestFailure::pre_transmit("transport".into(), request);
            failure.response = Some(Box::new(RestResponse {
                status: 401,
                reason: Some("Unauthorized".into()),
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    RetainedJsonInput::from_value(
                        json!({"message": if self.numeric { json!(7) } else { json!("detail") }}),
                    )
                    .unwrap(),
                ),
            }));
            Box::pin(async move { Err(failure) })
        }
    }
    fn ready<T>(mut future: ExchangeFuture<'_, T>) -> Result<T, ExchangeError> {
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => result,
            Poll::Pending => panic!("scripted HTTP reply must complete"),
        }
    }
    #[wasm_bindgen]
    pub fn post_mexc_reporting_data(
        post: js_sys::Function,
        numeric: bool,
        method: &str,
    ) -> Result<(), JsValue> {
        let runtime = Rc::new(WorkerRuntime);
        let failure = Rc::new(Failure {
            numeric,
            calls: Cell::new(0),
        });
        let base = BaseExchange::new(
            ExchangeName::Mexc,
            serde_json::from_value(json!({"key":"key","secret":"secret","publicDataOnly":false}))
                .unwrap(),
            Rc::new(DefaultStore::new()),
            runtime.clone(),
            ExchangeSettings::default(),
        );
        let venue = MexcExchange::new(
            base,
            failure.clone(),
            Rc::new(|v| v),
            MexcSocketDependencies {
                runtime,
                current_time_ms: Rc::new(|| 100.),
                remote_account_name: None,
                emit_fill: Rc::new(|_| Ok(())),
            },
            Rc::new(|_, _| panic!("unexpected external data")),
        );
        install_exchange_event_bridge(
            venue.base(),
            WorkerDataPost::for_browser(Rc::new(move |payload| {
                post.call1(&JsValue::UNDEFINED, payload)
                    .map(|_| ())
                    .map_err(|value| ExchangeError::new(format!("post failed: {value:?}")))
            })),
        );
        match method {
            "positions" => {
                assert!(ready(venue.fetch_positions(None)).unwrap().is_empty());
            }
            "validate" => {
                assert!(ready(venue.validate_account()).unwrap().balance.is_none());
            }
            "account" => {
                assert!(ready(venue.get_account()).is_err());
            }
            _ => panic!("unknown reader"),
        }
        assert_eq!(failure.calls.get(), 1, "real MEXC reader HTTP call floor");
        Ok(())
    }
}
