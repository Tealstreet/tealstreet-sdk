use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestExecution, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture, rate_limit::RateLimitClock,
    retry::ReadClassification, signature_rejection::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fake"),
    }
}
fn response(status: u16, data: &Value) -> RestResponse {
    RestResponse {
        status,
        reason: None,
        headers: BTreeMap::new(),
        body: RestResponseBody::Encoded(serde_json::to_vec(data).unwrap()),
    }
}
fn request(method: &str) -> RestRequest {
    RestRequest {
        method: method.into(),
        url: "/order".into(),
        ..Default::default()
    }
}
fn rejected(status: u16, data: &Value, request: RestRequest) -> RestFailure {
    let mut error = RestFailure::pre_transmit("Request failed".into(), request);
    error.response = Some(Box::new(response(status, data)));
    error
}
fn reset(request: RestRequest) -> RestFailure {
    RestFailure::from_transport(
        TransportError::Network(NetworkFailure::from_code(
            "ECONNRESET".into(),
            Some("ECONNRESET".into()),
        )),
        request,
    )
}
fn counting(calls: Rc<Cell<usize>>) -> IsSignatureRejection {
    Rc::new(move |data| {
        calls.set(calls.get() + 1);
        is_binance_style_signature_rejection(data)
    })
}

const REJECTION: &str = r#"{"code":-1022,"msg":"Signature for this request is not valid."}"#;

fn binance_corpus() -> Vec<(Value, bool)> {
    vec![
        (json!({"code":-1022}), true),
        (json!({"code":"-1022"}), true),
        (json!({"code":" -1022 "}), true),
        (
            json!({"code":-1021,"msg":"Signature for this request is not valid."}),
            true,
        ),
        (
            json!({"msg":"SIGNATURE FOR THIS REQUEST IS NOT VALID"}),
            true,
        ),
        (
            json!({"msg":["signature for this request is not valid"]}),
            true,
        ),
        (json!({"code":[-1022]}), true),
        (json!({"msg":"signature invalid"}), false),
        (json!({"code":-1021}), false),
        (json!({"code":null,"msg":null}), false),
        (json!({"code":true}), false),
        (json!({"code":{}}), false),
        (json!({"msg":123}), false),
        (json!("-1022"), false),
        (json!([]), false),
        (Value::Null, false),
    ]
}
// (errors: Some(data) is a response, None is none; resolved responses), then
// (retried, resent, sent with the stamp, predicate calls).
type Sequence = (Vec<Option<Value>>, Vec<Value>);
type Observed = (Vec<bool>, Vec<bool>, Vec<bool>, usize);
fn sequences() -> Vec<(Sequence, Observed)> {
    let rejection: Value = serde_json::from_str(REJECTION).unwrap();
    let fine = json!({"code":0});
    vec![
        (
            (
                vec![Some(rejection.clone()), Some(rejection.clone())],
                vec![],
            ),
            (vec![true, false], vec![], vec![], 1),
        ),
        (
            (vec![None, Some(rejection.clone())], vec![]),
            (vec![false, true], vec![], vec![], 1),
        ),
        (
            (
                vec![Some(fine.clone())],
                vec![rejection.clone(), rejection.clone()],
            ),
            (vec![false], vec![true, false], vec![true], 2),
        ),
        (
            (vec![], vec![fine.clone(), rejection.clone()]),
            (vec![], vec![false, true], vec![true], 2),
        ),
        (
            (vec![Some(rejection.clone())], vec![rejection.clone()]),
            (vec![true], vec![false], vec![], 1),
        ),
    ]
}
fn observe((errors, responses): &Sequence) -> Observed {
    let calls = Rc::new(Cell::new(0));
    let predicate = counting(calls.clone());
    let retry = SignatureRetry::default();
    let retried = errors
        .iter()
        .map(|data| {
            let error = match data {
                Some(data) => rejected(401, data, request("POST")),
                None => reset(request("POST")),
            };
            signature_rejected_response(&error, &retry, &predicate).unwrap()
        })
        .collect();
    let mut sent = vec![];
    let resent = responses
        .iter()
        .map(|data| {
            let execution = RestExecution {
                response: response(200, data),
                config: request("POST"),
            };
            let resend = resend_on_signature_rejection(&execution, &retry, &predicate).unwrap();
            if resend {
                sent.push(retry.has_retried());
            }
            resend
        })
        .collect();
    (retried, resent, sent, calls.get())
}

#[test]
fn signature_rejection_decisions_match_actual_typescript_source() {
    let mut native = vec![];
    for (data, expected) in binance_corpus() {
        let actual = is_binance_style_signature_rejection(&data.clone().into()).unwrap();
        assert_eq!(actual, expected, "native binance-style rejection {data}");
        native.push(actual);
    }
    for (sequence, expected) in sequences() {
        assert_eq!(observe(&sequence), expected, "native sequence {sequence:?}");
    }
    let inputs = json!({
        "binance": binance_corpus().into_iter().map(|(data, _)| data).collect::<Vec<_>>(),
        "sequences": sequences().into_iter().map(|((errors, responses), _)| json!({
            "errors": errors.into_iter().map(|data| match data {
                Some(data) => json!({"data": data}),
                None => json!({}),
            }).collect::<Vec<_>>(),
            "responses": responses,
        })).collect::<Vec<_>>(),
    });
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signature_rejection_oracle.cjs"
        ))
        .arg(inputs.to_string())
        .output()
        .expect("Node required; the signature rejection oracle never skips");
    assert!(
        output.status.success(),
        "signature rejection oracle failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["key"], SIGNATURE_RETRY_CONFIG_KEY);
    let values = |items: &Value| -> Vec<bool> {
        items
            .as_array()
            .unwrap()
            .iter()
            .map(|item| item["value"].as_bool().expect("source returned, not threw"))
            .collect()
    };
    assert_eq!(
        values(&source["binance"]),
        native,
        "source binance-style rejection"
    );
    let source_sequences = source["sequences"].as_array().unwrap();
    assert_eq!(source_sequences.len(), sequences().len());
    for (case, (_, expected)) in source_sequences.iter().zip(sequences()) {
        let sent = case["sent"]
            .as_array()
            .unwrap()
            .iter()
            .map(|flag| flag.as_bool().unwrap())
            .collect();
        let observed = (
            values(&case["retried"]),
            values(&case["resent"]),
            sent,
            case["calls"].as_u64().unwrap() as usize,
        );
        assert_eq!(observed, expected, "source sequence");
    }
}

struct Venue {
    accepted: RefCell<Vec<RestRequest>>,
    replies: RefCell<VecDeque<Result<RestResponse, RestFailure>>>,
}
impl Venue {
    fn new(replies: Vec<Result<RestResponse, RestFailure>>) -> Self {
        Self {
            accepted: RefCell::default(),
            replies: RefCell::new(replies.into()),
        }
    }
}
impl RestAdapter for Venue {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.accepted.borrow_mut().push(request.clone());
            let reply = self
                .replies
                .borrow_mut()
                .pop_front()
                .expect("unscripted extra attempt");
            reply.map_err(|mut error| {
                error.request = Box::new(request);
                error
            })
        })
    }
}
#[derive(Default)]
struct Clock(RefCell<Vec<u64>>);
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        0
    }
    fn sleep_ms(&self, milliseconds: u64) -> TransportFuture<'_, ()> {
        self.0.borrow_mut().push(milliseconds);
        Box::pin(async { Ok(()) })
    }
}
fn engine(
    method: &str,
    venue: &Venue,
    predicate: &IsSignatureRejection,
) -> Result<(), RestFailure> {
    let clock = Clock::default();
    run(retry_network_reads_and_signature_rejection(
        ReadClassification::Methods.classify(request(method)),
        3,
        venue,
        &clock,
        &SignatureRetry::default(),
        predicate,
    ))
    .map(|_| ())
}

#[test]
fn engine_resends_one_signature_rejection_and_network_reads_only() {
    let rejection: Value = serde_json::from_str(REJECTION).unwrap();
    let predicate: IsSignatureRejection = Rc::new(is_binance_style_signature_rejection);
    let fine = || Ok(response(200, &json!({"code":0})));
    let refused = || Err(rejected(401, &rejection, RestRequest::default()));
    let lost = || Err(reset(RestRequest::default()));
    for (method, replies, ok, attempts) in [
        ("POST", vec![refused(), fine()], true, 2),
        ("POST", vec![refused(), refused()], false, 2),
        ("POST", vec![lost()], false, 1),
        ("GET", vec![lost(), lost(), fine()], true, 3),
        ("GET", vec![refused(), refused()], false, 2),
        ("GET", vec![refused(), lost(), fine()], true, 3),
    ] {
        let venue = Venue::new(replies);
        assert_eq!(
            engine(method, &venue, &predicate).is_ok(),
            ok,
            "native engine result {method}"
        );
        assert_eq!(
            venue.accepted.borrow().len(),
            attempts,
            "native engine attempts {method}"
        );
    }
    let throwing: IsSignatureRejection = Rc::new(|_| {
        Err(safe_cex_num::JsCoercionError {
            name: "TypeError",
            message: "predicate threw",
        })
    });
    for method in ["GET", "POST"] {
        let venue = Venue::new(vec![refused()]);
        let error = engine(method, &venue, &throwing).unwrap_err();
        assert_eq!(
            error.message, "predicate threw",
            "native predicate failure {method}"
        );
        assert_eq!(venue.accepted.borrow().len(), 1);
    }
}
