use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestExecution, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_blofin::api::{ApiRuntime, create_api};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    for _ in 0..100 {
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
    }
    panic!("receipt fixture unexpectedly pending")
}
#[derive(Default)]
struct Runtime {
    successes: Cell<usize>,
    failures: Cell<usize>,
}
impl RateLimitClock for Runtime {
    fn now_ms(&self) -> u64 {
        1000
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
impl ApiRuntime for Runtime {
    fn timestamp(&self) -> f64 {
        panic!("receipt host supplies signing")
    }
    fn nonce(&self) -> String {
        panic!("receipt host supplies signing")
    }
    fn remote_signer_present(&self) -> bool {
        true
    }
    fn record_auth_failure(&self) {
        self.failures.set(self.failures.get() + 1);
    }
    fn record_auth_success(&self) {
        self.successes.set(self.successes.get() + 1);
    }
}
struct Adapter {
    signed: bool,
    fail_first: bool,
    code: &'static str,
    requests: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("receipt forwarding must not discard effective config")
    }
    fn execute_with_receipt(&self, mut request: RestRequest) -> RestFuture<'_, RestExecution> {
        Box::pin(async move {
            self.requests.borrow_mut().push(request.clone());
            if self.fail_first && self.requests.borrow().len() == 1 {
                return Err(safe_cex_transport::RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "scripted first network failure".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                ));
            }
            if self.signed {
                request
                    .headers
                    .insert("ACCESS-SIGN".into(), "returned-signature".into());
            } else {
                request.headers.shift_remove("ACCESS-SIGN");
            }
            Ok(RestExecution {
                response: RestResponse {
                    status: 200,
                    reason: Some("OK".into()),
                    headers: Default::default(),
                    body: RestResponseBody::Encoded(
                        serde_json::to_vec(&json!({"code":self.code})).unwrap(),
                    ),
                },
                config: request,
            })
        })
    }
}
fn cases(fail_first: bool) {
    let inventory = [
        ("added-signature-success", true, "0"),
        ("removed-signature-success", false, "0"),
        ("added-signature-auth-failure", true, "152401"),
        ("removed-signature-auth-failure", false, "152401"),
    ];
    assert_eq!(inventory.len(), 4, "blofin_receipt_declared_inventory");
    let mut visited = Vec::new();
    for (id, signed, code) in inventory {
        let runtime = Rc::new(Runtime::default());
        let adapter = Rc::new(Adapter {
            signed,
            fail_first,
            code,
            requests: RefCell::new(Vec::new()),
        });
        let options: ExchangeOptions = serde_json::from_value(json!({
            "key":"key", "secret":"secret", "passphrase":"passphrase",
        }))
        .unwrap();
        let api = create_api(&options, runtime.clone(), |_, _, _| Ok(adapter.clone())).unwrap();
        let mut request = RestRequest {
            method: "get".into(),
            url: "/private".into(),
            ..Default::default()
        };
        if !signed {
            request
                .headers
                .insert("ACCESS-SIGN".into(), "pre-dispatch-signature".into());
        }
        let execution = ready(api.execute_with_receipt(request.clone())).unwrap();
        let attempts = if fail_first { 2 } else { 1 };
        assert_eq!(
            adapter.requests.borrow().len(),
            attempts,
            "blofin_receipt_attempts {id}"
        );
        assert_eq!(
            adapter.requests.borrow()[0].headers,
            request.headers,
            "blofin_receipt_original_input {id}"
        );
        assert_eq!(
            execution.config.headers.contains_key("ACCESS-SIGN"),
            signed,
            "blofin_receipt_returned_config {id}"
        );
        assert_eq!(execution.response.body.json_value().unwrap()["code"], code);
        let success_count = if signed && code == "0" { attempts } else { 0 };
        let failure_count = if signed && code == "152401" {
            attempts
        } else {
            0
        };
        assert_eq!(
            runtime.successes.get(),
            success_count,
            "blofin_receipt_auth_effective_config {id}"
        );
        assert_eq!(
            runtime.failures.get(),
            failure_count,
            "blofin_receipt_auth_effective_config {id}"
        );
        visited.push(id);
    }
    assert_eq!(
        visited,
        [
            "added-signature-success",
            "removed-signature-success",
            "added-signature-auth-failure",
            "removed-signature-auth-failure"
        ],
        "blofin_receipt_executed_inventory"
    );
}
#[test]
fn effective_receipt_auth_headers() {
    cases(false);
}
#[test]
fn effective_receipt_auth_headers_on_retry_unwind() {
    cases(true);
}
