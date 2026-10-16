use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::JsCoercionError;
use safe_cex_venue_bitget::account_read::get_account;
use serde_json::{Value, json};
use std::{
    error::Error,
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};

type Failure = Box<dyn Error>;
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ACCOUNT fixture pending"),
    }
}
const NAMES: [&str; 13] = [
    "missing",
    "null",
    "false",
    "zero",
    "negativeZero",
    "empty",
    "alpha",
    "beta",
    "number",
    "array",
    "object",
    "poison",
    "overflow",
];
fn value(index: usize) -> Option<Value> {
    match index {
        0 => None,
        1 => Some(Value::Null),
        2 => Some(json!(false)),
        3 => Some(json!(0)),
        4 => Some(serde_json::from_str("-0").unwrap()),
        5 => Some(json!("")),
        6 => Some(json!("A-parent")),
        7 => Some(json!("B-child")),
        8 => Some(json!(37)),
        9 => Some(json!([null, 7])),
        10 => Some(json!({})),
        11 => Some(json!({"toString":null})),
        12 => Some(serde_json::from_str("1e400").unwrap()),
        _ => unreachable!(),
    }
}
fn tag(index: usize) -> Value {
    match index {
        0 => json!({"undefined":true}),
        3 => json!({"number":"0"}),
        4 => json!({"number":"-0"}),
        8 => json!({"number":"37"}),
        9 => json!([null,{"number":"7"}]),
        12 => json!({"number":"Infinity"}),
        _ => value(index).unwrap(),
    }
}
fn truthy(index: usize) -> bool {
    index >= 6
}
fn string(index: usize) -> Result<&'static str, &'static str> {
    Ok(match index {
        0 => "undefined",
        1 => "null",
        2 => "false",
        3 | 4 => "0",
        5 => "",
        6 => "A-parent",
        7 => "B-child",
        8 => "37",
        9 => ",7",
        10 => "[object Object]",
        11 => return Err("Cannot convert object to primitive value"),
        12 => "Infinity",
        _ => unreachable!(),
    })
}
fn expected(
    p: usize,
    u: usize,
    i: usize,
) -> Result<(String, Option<String>, Option<String>), &'static str> {
    let user = string(if truthy(p) { p } else { u })?.to_owned();
    let sub = if truthy(p) {
        Some(string(u)?.to_owned())
    } else {
        None
    };
    let affiliate = if truthy(i) {
        Some(string(i)?.to_owned())
    } else {
        None
    };
    Ok((user, sub, affiliate))
}
fn error_shape(error: &Failure) -> Value {
    if let Some(error) = error.downcast_ref::<JsCoercionError>() {
        json!({"name":error.name,"message":error.message,"same":false})
    } else if let Some(error) = error.downcast_ref::<ExchangeError>() {
        json!({"name":error.name,"message":error.message,"same":true})
    } else {
        panic!("ACCOUNT unexpected error {error}");
    }
}
#[test]
#[ignore = "requires Node and TypeScript; venue oracle includes ignored tests"]
fn account_matches_source_and_native_fields() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-capture.cjs"
        ))
        .output()
        .expect("ACCOUNT Node required");
    assert!(
        output.status.success(),
        "ACCOUNT source capture {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).expect("ACCOUNT complete capture");
    let cases = capture
        .get("cases")
        .and_then(Value::as_array)
        .expect("ACCOUNT cases required");
    // Cardinality is enforced by the fixed native walk and consumed-count check
    // below. Empty/subset sources must fail at the missing required input.
    let mut native_results = Vec::new();
    let mut index = 0;
    let mut outcomes = [0usize; 2];
    for p in 0..13 {
        for u in 0..13 {
            for i in 0..13 {
                let case = cases.get(index).expect("ACCOUNT required native input");
                assert_eq!(
                    case.get("input"),
                    Some(
                        &json!({"kind":"fields","parent":NAMES[p],"user":NAMES[u],"inviter":NAMES[i],"values":[tag(p),tag(u),tag(i)]})
                    ),
                    "ACCOUNT input content {index}"
                );
                let mut data = serde_json::Map::new();
                for (key, n) in [("parentId", p), ("userId", u), ("inviterId", i)] {
                    if let Some(v) = value(n) {
                        data.insert(key.into(), v);
                    }
                }
                let mut calls = Vec::new();
                let result = run(get_account::<Failure, _>(|url| {
                    calls.push(url);
                    std::future::ready(Ok(Some(json!({"data":data}))))
                }));
                let native = match (result, expected(p, u, i)) {
                    (Ok(record), Ok((user, sub, affiliate))) => {
                        assert_eq!(record.user_id, user, "ACCOUNT native user identity {index}");
                        assert_eq!(
                            record.sub_id,
                            Some(sub),
                            "ACCOUNT native sub identity {index}"
                        );
                        assert_eq!(
                            record.affiliate_id,
                            Some(affiliate),
                            "ACCOUNT native affiliate identity {index}"
                        );
                        outcomes[0] += 1;
                        Ok(record)
                    }
                    (Err(error), Err(message)) => {
                        let coercion = error
                            .downcast_ref::<JsCoercionError>()
                            .expect("ACCOUNT coercion error type");
                        assert_eq!(
                            coercion.name, "TypeError",
                            "ACCOUNT native error name {index}"
                        );
                        assert_eq!(
                            coercion.message, message,
                            "ACCOUNT native error message {index}"
                        );
                        outcomes[1] += 1;
                        Err(error)
                    }
                    (actual, expected) => {
                        panic!("ACCOUNT native disposition {index}: {actual:?} vs {expected:?}")
                    }
                };
                assert_eq!(
                    calls,
                    ["/api/v2/spot/account/info"],
                    "ACCOUNT native request {index}"
                );
                native_results.push(native);
                index += 1;
            }
        }
    }
    assert_eq!(outcomes, [1728, 469], "ACCOUNT native branches");
    for placement in ["outer", "inner"] {
        for shape in [
            "missing", "null", "false", "number", "string", "array", "object",
        ] {
            let case = cases.get(index).expect("ACCOUNT required shape input");
            assert_eq!(
                case.get("input"),
                Some(&json!({"kind":"shape","placement":placement,"shape":shape})),
                "ACCOUNT shape content {index}"
            );
            let value = match shape {
                "missing" => None,
                "null" => Some(Value::Null),
                "false" => Some(json!(false)),
                "number" => Some(json!(7)),
                "string" => Some(json!("scalar")),
                "array" => Some(json!([])),
                "object" => Some(json!({})),
                _ => unreachable!(),
            };
            let body = if placement == "outer" {
                value
            } else {
                Some(value.map_or_else(|| json!({}), |v| json!({"data":v})))
            };
            let result = run(get_account::<Failure, _>(|url| {
                assert_eq!(url, "/api/v2/spot/account/info");
                std::future::ready(Ok(body))
            }));
            let native = if placement == "inner" && !["missing", "null"].contains(&shape) {
                let record = result.expect("ACCOUNT primitive data succeeds");
                assert_eq!(record.user_id, "undefined", "ACCOUNT primitive user");
                assert_eq!(
                    record.sub_id,
                    Some(None),
                    "ACCOUNT primitive sub present null"
                );
                assert_eq!(
                    record.affiliate_id,
                    Some(None),
                    "ACCOUNT primitive affiliate present null"
                );
                Ok(record)
            } else {
                let error = result.expect_err("ACCOUNT missing shape fails");
                let coercion = error
                    .downcast_ref::<JsCoercionError>()
                    .expect("ACCOUNT native shape error type");
                let field = if placement == "outer" && ["missing", "null"].contains(&shape) {
                    "data"
                } else {
                    "parentId"
                };
                let value = if shape == "null" { "null" } else { "undefined" };
                assert_eq!(
                    coercion.message,
                    format!("Cannot read properties of {value} (reading '{field}')"),
                    "ACCOUNT native shape error {index}"
                );
                assert_eq!(
                    coercion.name, "TypeError",
                    "ACCOUNT native shape error name {index}"
                );
                Err(error)
            };
            native_results.push(native);
            index += 1;
        }
    }
    for failure in ["sync", "reject"] {
        let case = cases.get(index).expect("ACCOUNT required error input");
        assert_eq!(
            case.get("input"),
            Some(&json!({"kind":"failure","failure":failure})),
            "ACCOUNT error content"
        );
        let mut error = ExchangeError::new("original read");
        error.name = "ReadFailure".into();
        let error: Failure = Box::new(error);
        let pointer = std::ptr::from_ref(error.as_ref()) as *const ();
        let returned = run(get_account::<Failure, _>(|url| {
            assert_eq!(url, "/api/v2/spot/account/info");
            std::future::ready(Err(error))
        }))
        .unwrap_err();
        assert_eq!(
            returned.downcast_ref::<ExchangeError>().unwrap().name,
            "ReadFailure",
            "ACCOUNT native read metadata"
        );
        assert_eq!(
            std::ptr::from_ref(returned.as_ref()) as *const (),
            pointer,
            "ACCOUNT original read identity"
        );
        assert_eq!(
            returned.downcast_ref::<ExchangeError>().unwrap().message,
            "original read",
            "ACCOUNT native read message"
        );
        native_results.push(Err(returned));
        index += 1;
    }
    assert_eq!(index, 2213, "ACCOUNT native comparison count");
    assert_eq!(cases.len(), index, "ACCOUNT source consumed count");
    // Every native field, error and request assertion above completes before
    // serializing any native result or comparing source output bytes/JSON.
    for (index, result) in native_results.into_iter().enumerate() {
        let wire = match result {
            Ok(record) => json!({"value":serde_json::to_value(record).unwrap()}),
            Err(error) => json!({"error":error_shape(&error)}),
        };
        assert_eq!(
            cases[index].get("calls"),
            Some(&json!(["/api/v2/spot/account/info"])),
            "ACCOUNT source request {index}"
        );
        assert_eq!(
            cases[index].get("output"),
            Some(&wire),
            "ACCOUNT source output {index}"
        );
    }
    let boundaries = capture
        .get("boundaries")
        .and_then(Value::as_array)
        .expect("ACCOUNT boundaries required");
    assert_eq!(boundaries.len(), 3, "ACCOUNT boundary total");
    for (index, field) in ["parentId", "userId", "inviterId"].iter().enumerate() {
        assert_eq!(
            boundaries[index].get("field"),
            Some(&json!(field)),
            "ACCOUNT boundary identity"
        );
        let text = boundaries[index]
            .get("output")
            .and_then(Value::as_str)
            .unwrap();
        assert!(
            text.contains("\\ud800"),
            "ACCOUNT source surrogate retained"
        );
        assert!(
            serde_json::from_str::<Value>(text).is_err(),
            "ACCOUNT UTF8 representation refusal"
        );
    }
}

#[test]
fn account_read_awaits_and_cancels() {
    use std::{cell::Cell, rc::Rc};
    struct Pending(Rc<Cell<usize>>);
    impl Future for Pending {
        type Output = Result<Option<Value>, Failure>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for Pending {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Rc::new(Cell::new(0));
    let calls = Cell::new(0);
    let pending = Pending(drops.clone());
    let mut future = Box::pin(get_account::<Failure, _>(|url| {
        assert_eq!(url, "/api/v2/spot/account/info", "ACCOUNT pending endpoint");
        calls.set(calls.get() + 1);
        pending
    }));
    assert_eq!(calls.get(), 0, "ACCOUNT lazy read before poll");
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending(),
        "ACCOUNT read remains pending"
    );
    assert_eq!(calls.get(), 1, "ACCOUNT one pending read");
    drop(future);
    assert_eq!(drops.get(), 1, "ACCOUNT caller cancellation owns read");
}
