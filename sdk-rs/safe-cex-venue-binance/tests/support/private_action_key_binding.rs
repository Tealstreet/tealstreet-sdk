//! Actual native listen-key member and its erased future; no replacement parent.
use super::*;
use safe_cex_transport::{RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_binance::listen_key::{ListenKeyData, ListenKeyResponse};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

struct Held {
    release: Arc<AtomicBool>,
    requests: Arc<Mutex<Vec<RestRequest>>>,
    rejected: bool,
}
impl RestAdapter for Held {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.lock().unwrap().push(request.clone());
        let release = self.release.clone();
        let body = if self.rejected {
            Input::from_value(json!({"code":-123,"msg":"native listen-key refusal"})).unwrap()
        } else {
            Input::Object(vec![(
                "listenKey".encode_utf16().collect(),
                Input::Object(vec![
                    ("negativeZero".encode_utf16().collect(), Input::Float(-0.0)),
                    ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
                    ("ownUndefined".encode_utf16().collect(), Input::Undefined),
                ]),
            )])
        };
        let mut response = Some(RestResponse {
            status: if self.rejected { 400 } else { 200 },
            reason: None,
            headers: Default::default(),
            body: RestResponseBody::Untransformed(body),
        });
        Box::pin(std::future::poll_fn(move |_| {
            if !release.load(Ordering::SeqCst) {
                return Poll::Pending;
            }
            let response = response.take().expect("one real HTTP completion");
            Poll::Ready(if response.status == 200 {
                Ok(response)
            } else {
                Err(safe_cex_transport::RestFailure {
                    code: Some("ERR_BAD_REQUEST".into()),
                    message: "Request failed with status code 400".into(),
                    transport_error: None,
                    response: Some(Box::new(response)),
                    request: Box::new(request.clone()),
                    context: None,
                })
            })
        }))
    }
}
#[test]
fn private_pin_key_retains_native_value_and_request_rejection_after_suspension() {
    for venue in [Venue::Futures, Venue::PM] {
        for rejected in [false, true] {
            let runtime = Rc::new(Runtime::default());
            let release = Arc::new(AtomicBool::new(false));
            let requests = Arc::new(Mutex::new(vec![]));
            let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                Arc::new(Held {
                    release: release.clone(),
                    requests: requests.clone(),
                    rejected,
                }),
                Rc::new(|_, _| panic!("unexpected external provider")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let name = if venue == Venue::PM {
                safe_cex_types::ExchangeName::Binancepm
            } else {
                safe_cex_types::ExchangeName::Binance
            };
            let owner = ExchangeOwner::new(
                venue,
                name,
                serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"}))
                    .unwrap(),
                Rc::new(safe_cex_store::store::DefaultStore::new()),
                dependencies,
            )
            .unwrap();
            let path = if venue == Venue::PM {
                "/papi/v1/listenKey"
            } else {
                "/fapi/v1/listenKey"
            };
            let mut future: safe_cex_venue_binance::private_classes::PinKey =
                PrivateParent::post_key(owner.state.as_ref(), path).unwrap();
            for _ in 0..3 {
                runtime.pump();
                assert!(
                    future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending(),
                    "actual PinKey stays pending before native HTTP release"
                );
            }
            {
                let requests = requests.lock().unwrap();
                assert_eq!(requests.len(), 1, "one actual listen-key request");
                assert_eq!(requests[0].method, "post");
                assert!(requests[0].url.starts_with(path));
            }
            release.store(true, Ordering::SeqCst);
            let settled = run(&runtime, future);
            if rejected {
                let error = match settled {
                    Err(error) => error,
                    Ok(_) => panic!("PinKey must propagate real HTTP refusal"),
                };
                assert_eq!(
                    error.message, "Request failed with status code 400",
                    "source rejected interceptor rethrows the original HTTP error"
                );
                assert_eq!(
                    error.response.as_ref().expect("original HTTP response")["data"]["msg"],
                    "native listen-key refusal"
                );
                assert!(!error.name.is_empty());
            } else {
                let ListenKeyResponse::Data(ListenKeyData::Present(slot)) = settled.unwrap() else {
                    panic!("actual PinKey exposes the source data.listenKey result")
                };
                let key = slot.borrow().clone().expect("retained native listenKey");
                assert_eq!(
                    key.read("negativeZero")
                        .unwrap()
                        .number()
                        .unwrap()
                        .to_bits(),
                    (-0.0f64).to_bits()
                );
                assert!(key.read("nan").unwrap().number().unwrap().is_nan());
                assert!(key.has_own("ownUndefined"));
                assert!(matches!(
                    key.read("ownUndefined").unwrap(),
                    EventValue::Undefined
                ));
            }
            for _ in 0..3 {
                runtime.pump();
            }
            assert_eq!(
                requests.lock().unwrap().len(),
                1,
                "settling PinKey cannot resend"
            );
            owner.dispose().unwrap();
        }
    }
}
