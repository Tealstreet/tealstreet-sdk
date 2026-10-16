use super::*;
use safe_cex_transport::{RestRequest, RestResponse};
use serde_json::json;
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn sequential_submission_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/submission-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 127);
    for case in cases {
        let calls = Rc::new(RefCell::new(vec![]));
        let sequence = Rc::new(RefCell::new(vec![]));
        let reports = RefCell::new(vec![]);
        let calls_post = calls.clone();
        let sequence_post = sequence.clone();
        let mask = case["mask"].as_u64().unwrap();
        let result = ready(submit_create_orders(
            case["payloads"].as_array().unwrap().clone(),
            move |payload| {
                let calls = calls_post.clone();
                let sequence = sequence_post.clone();
                async move {
                    let i = calls.borrow().len();
                    calls
                        .borrow_mut()
                        .push(json!({"url":"/v5/order/create","payload":payload}));
                    sequence.borrow_mut().push(format!("post:{i}"));
                    sequence.borrow_mut().push(format!("settled:{i}"));
                    if mask & (1 << i) != 0 {
                        let mut error = RestFailure::pre_transmit(
                            format!("transport:{i}"),
                            RestRequest::default(),
                        );
                        error.response = Some(Box::new(RestResponse {
                            status: 400,
                            reason: None,
                            headers: Default::default(),
                            body: safe_cex_transport::RestResponseBody::Encoded(
                                serde_json::to_vec(&json!({"retMsg":format!("venue:{i}")}))
                                    .unwrap(),
                            ),
                        }));
                        Err(error)
                    } else {
                        Ok(json!({"result":{"orderId":i.to_string()},"retCode":i}))
                    }
                }
            },
            |error| {
                let body: Value = error.response.as_ref().unwrap().body.json_value().unwrap();
                reports
                    .borrow_mut()
                    .push(json!({"body":body,"message":error.message}));
                sequence.borrow_mut().push(format!(
                    "reported:{}:{}",
                    body["retMsg"].as_str().unwrap(),
                    error.message
                ));
                Ok::<_, String>(())
            },
        ))
        .unwrap();
        let result: Vec<Value> = result
            .into_iter()
            .map(|(data, payload)| json!({"data":data,"payload":payload}))
            .collect();
        assert_eq!(json!(result), case["result"]);
        assert_eq!(json!(*calls.borrow()), case["calls"]);
        assert_eq!(json!(*sequence.borrow()), case["sequence"]);
        assert_eq!(json!(*reports.borrow()), case["reports"]);
    }
}

#[test]
fn dropping_pending_submission_never_starts_next_payload() {
    let calls = RefCell::new(vec![]);
    let mut future = Box::pin(submit_create_orders(
        vec![json!(1), json!(2)],
        |payload| {
            calls.borrow_mut().push(payload);
            std::future::pending::<Result<Value, RestFailure>>()
        },
        |_| Ok::<_, String>(()),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert_eq!(*calls.borrow(), [json!(1)]);
}

#[test]
fn reporting_failure_aborts_series_without_sending_next_payload() {
    let calls = RefCell::new(vec![]);
    let result = ready(submit_create_orders(
        vec![json!(1), json!(2)],
        |payload| {
            calls.borrow_mut().push(payload);
            std::future::ready(Err::<Value, _>(RestFailure::pre_transmit(
                "rejected".into(),
                RestRequest::default(),
            )))
        },
        |_| Err("reporting failed"),
    ));
    assert_eq!(result, Err("reporting failed"));
    assert_eq!(*calls.borrow(), [json!(1)]);
}
