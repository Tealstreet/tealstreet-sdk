use safe_cex_num::JsCoercionError;
use safe_cex_types::IncomeRecord;
use safe_cex_venue_bitget::history_reads::fetch_income_history;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    error::Error,
    fmt,
    future::Future,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture unexpectedly pending"),
    }
}
#[derive(Debug)]
struct PageFailure(Option<Value>);
impl fmt::Display for PageFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("page failure")
    }
}
impl Error for PageFailure {}
#[derive(Debug)]
struct ListenerFailure {
    marker: String,
    metadata: Vec<u8>,
}
impl fmt::Display for ListenerFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("listener failure")
    }
}
impl Error for ListenerFailure {}
fn decode(value: &Value) -> Option<Value> {
    if value.get("$undefined").is_some() {
        return None;
    }
    Some(match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter_map(|(k, v)| decode(v).map(|v| (k.clone(), v)))
                .collect(),
        ),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|v| decode(v).unwrap_or(Value::Null))
                .collect(),
        ),
        value => value.clone(),
    })
}
fn check(oracle: &Value) {
    for case in oracle["cases"].as_array().unwrap() {
        let calls = RefCell::new(Vec::new());
        let opts = json!({"symbol":"BTC"});
        let rows = Vec::<IncomeRecord>::with_capacity(1);
        let pointer = rows.as_ptr();
        let emit_failure: Box<dyn Error> = Box::new(ListenerFailure {
            marker: "original subscriber".into(),
            metadata: vec![7, 19, 42],
        });
        let emit_pointer = std::ptr::from_ref(emit_failure.as_ref()) as *const ();
        let result = run(fetch_income_history::<_, Box<dyn Error>, _>(
            &opts,
            |arg| {
                calls
                    .borrow_mut()
                    .push(json!({"page":true,"sameOpts":std::ptr::eq(arg, &opts)}));
                std::future::ready(if case["mode"] == "resolve" {
                    Ok(rows)
                } else {
                    Err(Box::new(PageFailure(decode(&case["thrown"]))) as Box<dyn Error>)
                })
            },
            |error| error.downcast_ref::<PageFailure>().unwrap().0.clone(),
            |message| {
                calls.borrow_mut().push(json!({"emit":message}));
                if case["emitThrows"] == true {
                    Err(emit_failure)
                } else {
                    Ok(())
                }
            },
        ));
        let output = match result {
            Ok(rows) => json!({"value":rows,"sameRows": rows.as_ptr() == pointer}),
            Err(error) => {
                let name = if let Some(listener) = error.downcast_ref::<ListenerFailure>() {
                    assert_eq!(
                        listener.marker, "original subscriber",
                        "reporter marker retained"
                    );
                    assert_eq!(listener.metadata, [7, 19, 42], "reporter metadata retained");
                    "ListenerError"
                } else if error.is::<JsCoercionError>() {
                    "TypeError"
                } else {
                    panic!("unexpected error {error}")
                };
                let message = error
                    .downcast_ref::<JsCoercionError>()
                    .map_or_else(|| error.to_string(), |e| e.message.to_owned());
                json!({"error":{"name":name,"message":message,"sameEmit":std::ptr::from_ref(error.as_ref()) as *const () == emit_pointer}})
            }
        };
        assert_eq!(
            output, case["output"],
            "fetchIncomeHistory outcome {}",
            case["name"]
        );
        assert_eq!(
            *calls.borrow(),
            case["calls"].as_array().unwrap().clone(),
            "fetchIncomeHistory effects {}",
            case["name"]
        );
    }
    for case in oracle["successCases"].as_array().unwrap() {
        let numeric = [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e308];
        let index = case["index"].as_u64().unwrap() as usize;
        let rows = vec![
            IncomeRecord {
                symbol: "Z".into(),
                income_type: "funding".into(),
                income: numeric[index],
                asset: "USDT".into(),
                timestamp: numeric[(index + 1) % numeric.len()],
                tran_id: "z".into(),
                trade_id: None,
                category: None,
            },
            IncomeRecord {
                symbol: "A".into(),
                income_type: "fee".into(),
                income: 7.0,
                asset: "BGB".into(),
                timestamp: 3.0,
                tran_id: "a".into(),
                trade_id: None,
                category: None,
            },
        ];
        let pointer = rows.as_ptr();
        let calls = RefCell::new(Vec::new());
        let result = run(fetch_income_history::<_, Box<dyn Error>, _>(
            (),
            |()| {
                calls.borrow_mut().push("page");
                std::future::ready(Ok(rows))
            },
            |_| panic!("success has no error"),
            |_| panic!("success must not report"),
        ))
        .unwrap();
        assert_eq!(
            result.as_ptr(),
            pointer,
            "successful populated page retains allocation"
        );
        assert_eq!(
            *calls.borrow(),
            ["page"],
            "successful populated page call count"
        );
        assert_eq!(
            case["sameRows"], true,
            "source returns original populated page"
        );
        assert_eq!(case["calls"], json!(["page"]), "source success call count");
        let tagged = |n: f64| json!({"$number":safe_cex_num::js_number_string(n),"bits":format!("{:016x}",n.to_bits())});
        let actual: Vec<Value> = result
            .iter()
            .map(|row| {
                let mut value = serde_json::to_value(row).unwrap();
                value["income"] = tagged(row.income);
                value["timestamp"] = tagged(row.timestamp);
                value
            })
            .collect();
        assert_eq!(
            json!(actual),
            case["rows"],
            "fetchIncomeHistory populated native fields {}",
            case["name"]
        );
    }
    for witness in oracle["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<Value>(witness["json"].as_str().unwrap()).is_err(),
            "income history shared UTF8 refusal"
        );
        assert!(
            witness["emitted"].as_str().unwrap().contains("\\ud"),
            "source diagnostic retains surrogate"
        );
    }
}
#[test]
fn frozen_income_history() {
    check(&serde_json::from_str(include_str!("income-history-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_income_history() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/income-history-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    check(&serde_json::from_slice(&output.stdout).unwrap());
}
#[test]
fn dropping_income_history_drops_read_without_reporting() {
    struct Pending<'a>(&'a std::cell::Cell<bool>);
    impl Future for Pending<'_> {
        type Output = Result<Vec<IncomeRecord>, Box<dyn Error>>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for Pending<'_> {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let dropped = std::cell::Cell::new(false);
    let mut future = Box::pin(fetch_income_history(
        (),
        |()| Pending(&dropped),
        |_| panic!("pending read has no error"),
        |_| panic!("pending read must not report"),
    ));
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    drop(future);
    assert!(dropped.get(), "income history owns pending page read");
}
