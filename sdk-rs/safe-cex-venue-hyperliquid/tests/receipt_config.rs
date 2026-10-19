use safe_cex_transport::{
    RestAdapter, RestBody, RestExecution, RestFuture, RestRequest, RestResponse, RestResponseBody,
    TransportFuture, rate_limit::RateLimitClock,
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_hyperliquid::{api::create_exchange_api, rate_limit::RateLimiter};
use serde_json::{Value, json};
use std::{
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex},
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("receipt fixture unexpectedly pending"),
    }
}
struct Clock;
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        1000
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
}
struct Adapter {
    url: &'static str,
    body: Value,
    response: Value,
    requests: Mutex<Vec<RestRequest>>,
    effective: Mutex<Option<RestRequest>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
        panic!("receipt forwarding must not discard effective config")
    }
    fn execute_with_receipt(&self, mut request: RestRequest) -> RestFuture<'_, RestExecution> {
        Box::pin(async move {
            self.requests.lock().unwrap().push(request.clone());
            request.url = self.url.into();
            request.body = Some(RestBody::Untransformed(
                safe_cex_types::retained_json::RetainedJsonInput::from_value(self.body.clone())
                    .unwrap(),
            ));
            *self.effective.lock().unwrap() = Some(request.clone());
            Ok(RestExecution {
                response: RestResponse {
                    status: 200,
                    reason: Some("OK".into()),
                    headers: Default::default(),
                    body: RestResponseBody::Untransformed(
                        safe_cex_types::retained_json::RetainedJsonInput::from_value(
                            self.response.clone(),
                        )
                        .unwrap(),
                    ),
                },
                config: request,
            })
        })
    }
}
#[test]
fn effective_receipt_url_and_action_classification() {
    let terminal = "Order was never placed, already canceled, or filled. asset=0";
    let inventory = [
        (
            "info-to-exchange-null",
            "/info",
            "/exchange",
            "order",
            "order",
            true,
            true,
            false,
        ),
        (
            "exchange-to-info-null",
            "/exchange",
            "/info",
            "order",
            "order",
            true,
            false,
            false,
        ),
        (
            "order-to-cancel-stays-quiet",
            "/exchange",
            "/exchange",
            "order",
            "cancel",
            false,
            true,
            false,
        ),
        (
            "cancel-to-order-emits-effective-data",
            "/exchange",
            "/exchange",
            "cancel",
            "order",
            false,
            true,
            true,
        ),
    ];
    assert_eq!(inventory.len(), 4, "hyperliquid_receipt_declared_inventory");
    let mut visited = Vec::new();
    for (
        id,
        original_url,
        effective_url,
        original_action,
        effective_action,
        null,
        rejected,
        telemetry_expected,
    ) in inventory
    {
        let effective_body =
            json!({"action":{"type":effective_action},"receiptMarker":"effective"});
        let adapter = Arc::new(Adapter {
            url: effective_url,
            body: effective_body.clone(),
            response: if null {
                Value::Null
            } else {
                json!({"status":"err","response":terminal})
            },
            requests: Mutex::new(Vec::new()),
            effective: Mutex::new(None),
        });
        let basic_events = Arc::new(Mutex::new(Vec::<String>::new()));
        let basic_output = basic_events.clone();
        let telemetry_inputs = Arc::new(Mutex::new(Vec::<Value>::new()));
        let telemetry_input_output = telemetry_inputs.clone();
        let telemetry_events = Arc::new(Mutex::new(Vec::<(String, Option<Value>)>::new()));
        let telemetry_output = telemetry_events.clone();
        let options: ExchangeOptions =
            serde_json::from_value(json!({"key":"", "secret":"", "publicDataOnly":true})).unwrap();
        let client = create_exchange_api(
            options,
            adapter.clone(),
            Rc::new(RateLimiter::new(Arc::new(Clock), 1200.0, 60000.0)),
            Arc::new(move |message| basic_output.lock().unwrap().push(message.into())),
        )
        .with_order_error_telemetry(
            Arc::new(move |body, message| {
                telemetry_input_output.lock().unwrap().push(body.clone());
                Some(json!({"observed":body,"message":message}))
            }),
            Arc::new(move |message, context| {
                telemetry_output
                    .lock()
                    .unwrap()
                    .push((message.into(), context.cloned()))
            }),
        );
        let original_body = json!({"action":{"type":original_action},"receiptMarker":"original"});
        let request = client.build_request(original_url, &original_body).unwrap();
        let result = ready(client.execute(request, false, &|_| Ok(())));
        assert_eq!(
            result.is_err(),
            rejected,
            "hyperliquid_receipt_url_classification {id}"
        );
        assert_eq!(
            adapter.requests.lock().unwrap().len(),
            1,
            "hyperliquid_receipt_attempts {id}"
        );
        let input = adapter.requests.lock().unwrap();
        assert_eq!(input[0].url, original_url);
        assert_eq!(
            input[0].body.as_ref().unwrap().untransformed().unwrap(),
            &safe_cex_types::retained_json::RetainedJsonInput::from_value(original_body.clone())
                .unwrap()
        );
        drop(input);
        if let Err(error) = result {
            assert_eq!(
                *error.request,
                adapter.effective.lock().unwrap().clone().unwrap(),
                "hyperliquid_receipt_failure_config {id}"
            );
            assert!(
                error.response.is_some(),
                "hyperliquid_receipt_failure_response {id}"
            );
            if null {
                assert_eq!(error.message, format!("{} (HTTP 200)", safe_cex_venue_hyperliquid::order_response::EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE));
            } else {
                assert_eq!(
                    error.message, terminal,
                    "hyperliquid_receipt_action_refusal {id}"
                );
            }
        }
        assert_eq!(
            basic_events.lock().unwrap().len(),
            usize::from(id == "info-to-exchange-null"),
            "hyperliquid_receipt_basic_emission {id}"
        );
        assert_eq!(
            telemetry_inputs.lock().unwrap().len(),
            usize::from(telemetry_expected),
            "hyperliquid_receipt_action_classification {id}"
        );
        assert_eq!(
            telemetry_events.lock().unwrap().len(),
            usize::from(telemetry_expected),
            "hyperliquid_receipt_telemetry_emission {id}"
        );
        if telemetry_expected {
            assert_eq!(
                telemetry_inputs.lock().unwrap()[0],
                effective_body,
                "hyperliquid_receipt_effective_telemetry_data {id}"
            );
            let events = telemetry_events.lock().unwrap();
            assert_eq!(events[0].0, terminal);
            assert_eq!(events[0].1.as_ref().unwrap()["observed"], effective_body);
        }
        visited.push(id);
    }
    assert_eq!(
        visited,
        [
            "info-to-exchange-null",
            "exchange-to-info-null",
            "order-to-cancel-stays-quiet",
            "cancel-to-order-emits-effective-data"
        ],
        "hyperliquid_receipt_executed_inventory"
    );
}
