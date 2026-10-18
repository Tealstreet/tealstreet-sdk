use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use std::sync::{Arc, Mutex};
struct WeightHttp {
    header: String,
    sent: Mutex<Vec<RestRequest>>,
}
impl RestAdapter for WeightHttp {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.sent.lock().unwrap().push(request);
        let header = self.header.clone();
        Box::pin(async move {
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: [("x-mbx-used-weight-1m".into(), header)].into(),
                body: RestResponseBody::Encoded(b"[]".to_vec()),
            })
        })
    }
}
#[test]
fn native_response_weight_pauses_actual_owner_at_threshold() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        for (header, expected) in [
            ("1919", 0.0_f64),
            ("1920", 80000.0),
            ("2500", 80000.0),
            ("invalid", 0.0),
        ] {
            let runtime = Rc::new(Runtime::default());
            runtime.now.set(20000.0);
            let http = Arc::new(WeightHttp {
                header: header.into(),
                sent: Mutex::new(vec![]),
            });
            let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                http.clone(),
                Rc::new(|_, _| panic!("no external provider")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let owner = ExchangeOwner::new(
                venue,
                match venue {
                    Venue::Futures => safe_cex_types::ExchangeName::Binance,
                    Venue::PM => safe_cex_types::ExchangeName::Binancepm,
                    Venue::Spot => safe_cex_types::ExchangeName::Binancespot,
                },
                serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"}))
                    .unwrap(),
                Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
                dependencies,
            )
            .unwrap();
            assert_eq!(owner.paused_until.get().to_bits(), 0.0_f64.to_bits());
            run(
                &runtime,
                owner.invoke(
                    safe_cex_venue_binance::exchange_classes::Method::Orders,
                    vec![],
                ),
            )
            .unwrap();
            assert!(!http.sent.lock().unwrap().is_empty());
            assert!(
                http.sent
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|r| r.url.contains("signature=")),
                "actual signed native requests"
            );
            assert_eq!(
                owner.paused_until.get().to_bits(),
                if venue == Venue::Futures {
                    expected
                } else {
                    0.0
                }
                .to_bits(),
                "{venue:?} {header}: actual interceptor -> header -> pause state"
            );
            owner.dispose().unwrap();
        }
    }
}
