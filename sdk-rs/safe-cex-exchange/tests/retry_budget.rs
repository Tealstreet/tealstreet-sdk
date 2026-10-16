use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, TransportError,
    TransportFuture,
    rate_limit::RateLimitClock,
    retry::{self, ClassifiedRequest, ReadClassification},
};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Clock {
    now: Cell<u64>,
    waits: RefCell<Vec<u64>>,
}
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            self.waits.borrow_mut().push(ms);
            self.now.set(self.now.get() + ms);
            Ok(())
        })
    }
}
struct Attempts<'a> {
    clock: &'a Clock,
    elapsed: u64,
    timeouts: RefCell<Vec<Option<i64>>>,
}
impl RestAdapter for Attempts<'_> {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.timeouts.borrow_mut().push(request.timeout_ms);
            self.clock.now.set(self.clock.now.get() + self.elapsed);
            let mut error = RestFailure::from_transport(
                TransportError::Host(format!("attempt-{}", self.timeouts.borrow().len())),
                request,
            );
            error.code = Some("CUSTOM".into());
            Err(error)
        })
    }
}
fn run<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("deterministic clock must complete"),
    }
}
#[test]
#[ignore = "live installed Axios in mandatory isolated core gate"]
fn live_retry_predicate_and_derived_timeout_budget() {
    let output = std::process::Command::new("node")
        .arg("scripts/retry-budget-oracle.cjs")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        oracle["predicates"].as_array().unwrap().len(),
        640,
        "RETRY-CARDINALITY source predicates"
    );
    for case in oracle["predicates"].as_array().unwrap() {
        let mut error = RestFailure::from_transport(
            TransportError::Host("fixture".into()),
            RestRequest {
                method: case["method"].as_str().unwrap().into(),
                ..Default::default()
            },
        );
        error.code = case["code"].as_str().map(str::to_owned);
        error.response = case["status"].as_u64().map(|status| {
            Box::new(RestResponse {
                status: status as u16,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
            })
        });
        assert_eq!(
            retry::is_network_or_idempotent_request_error(&error),
            case["expected"].as_bool().unwrap(),
            "general error evidence must match installed network-or-idempotent predicate: {case}"
        );
    }
    assert_eq!(
        oracle["budgets"].as_array().unwrap().len(),
        6,
        "RETRY-CARDINALITY source budgets"
    );
    for (index, case) in oracle["budgets"].as_array().unwrap().iter().enumerate() {
        let clock = Clock {
            now: Cell::new(case["start"].as_u64().unwrap()),
            ..Default::default()
        };
        let adapter = Attempts {
            clock: &clock,
            elapsed: case["elapsed"].as_u64().unwrap(),
            timeouts: RefCell::default(),
        };
        let request = RestRequest {
            method: "get".into(),
            timeout_ms: case["timeout"].as_i64(),
            ..Default::default()
        };
        let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(request) else {
            panic!("fixture read")
        };
        let delay = case["delay"].as_u64().unwrap();
        let policy = retry::only_reads(read, 3, |_, result| result.is_err())
            .with_delay(move |n| n as u64 * delay);
        let error = run(policy.execute(&adapter, &clock)).unwrap_err();
        let attempts = [3, 1, 3, 1, 4, 4][index];
        let schedules = [2, 0, 2, 0, 3, 3][index];
        assert_eq!(
            adapter.timeouts.borrow().len(),
            attempts,
            "budget native independent adapter-call floor {index}"
        );
        assert_eq!(
            clock.waits.borrow().len(),
            schedules,
            "budget native independent scheduler-call floor {index}"
        );
        assert_eq!(
            error.code.as_deref(),
            Some("CUSTOM"),
            "budget native original terminal code {index}"
        );
        let expected_timeouts: Vec<Option<i64>> = case["timeouts"]
            .as_array()
            .unwrap()
            .iter()
            .map(serde_json::Value::as_i64)
            .collect();
        assert_eq!(
            expected_timeouts.len(),
            attempts,
            "budget Source independent adapter-call floor {index}"
        );
        assert_eq!(
            *adapter.timeouts.borrow(),
            expected_timeouts,
            "retry native timeout values consume elapsed plus delay: {case}"
        );
        let waits: Vec<u64> = case["waits"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_u64().unwrap())
            .collect();
        assert_eq!(
            waits.len(),
            schedules,
            "budget Source independent scheduler-call floor {index}"
        );
        assert_eq!(
            *clock.waits.borrow(),
            waits,
            "budget refusal must not sleep: {case}"
        );
        assert_eq!(
            error.message,
            case["message"].as_str().unwrap(),
            "budget refusal must preserve terminal error"
        );
    }
}

#[test]
fn derived_budget_preserves_terminal_error_and_venue_status_or_is_explicit() {
    let clock = Clock {
        now: Cell::new(1000),
        ..Default::default()
    };
    let adapter = Attempts {
        clock: &clock,
        elapsed: 8000,
        timeouts: RefCell::default(),
    };
    let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(RestRequest {
        method: "get".into(),
        timeout_ms: Some(20000),
        ..Default::default()
    }) else {
        unreachable!()
    };
    let policy = retry::only_reads(read, 3, |_, result| result.is_err());
    let error = run(policy.execute(&adapter, &clock)).unwrap_err();
    assert_eq!(
        *adapter.timeouts.borrow(),
        [Some(20000), Some(12000), Some(4000)],
        "remaining timeout is derived, never reset"
    );
    assert_eq!(error.message, "attempt-3");
    assert_eq!(error.code.as_deref(), Some("CUSTOM"));
    assert_eq!(
        *clock.waits.borrow(),
        [0, 0],
        "budget native zero-delay scheduling before terminal exhaustion"
    );
    for status in [500, 503, 504] {
        let mut error = RestFailure::pre_transmit(
            "fixture".into(),
            RestRequest {
                method: "post".into(),
                url: "/listenKey".into(),
                ..Default::default()
            },
        );
        error.code = Some("ECONNABORTED".into());
        error.response = Some(Box::new(RestResponse {
            status,
            reason: None,
            headers: Default::default(),
            body: safe_cex_transport::RestResponseBody::Encoded(vec![]),
        }));
        assert!(!retry::is_network_or_idempotent_request_error(&error));
        let venue_choice = retry::is_network_or_idempotent_request_error(&error)
            || error
                .response
                .as_ref()
                .is_some_and(|response| matches!(response.status, 503 | 504));
        assert_eq!(
            venue_choice,
            matches!(status, 503 | 504),
            "venue 503/504 configuration stays separate from library predicate"
        );
    }
}

#[test]
fn retry_execution_requires_an_explicit_host_clock() {
    trybuild::TestCases::new().compile_fail("tests/ui/retry_missing_clock.rs");
}
