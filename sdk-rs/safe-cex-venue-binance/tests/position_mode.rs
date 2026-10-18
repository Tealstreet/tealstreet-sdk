use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::position_mode::*;
use serde_json::{Value, json};
use std::{
    cell::Cell,
    future::Future,
    task::{Context, Poll, Waker},
};

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture did not settle"),
    }
}
fn fixture_error() -> ExchangeError {
    let mut error = ExchangeError::new("fixture request");
    error.name = "RangeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    error =
        error.with_retained_response((json!({"status":503,"data":{"msg":"venue message"}})).into());
    error
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn position_mode_readers_match_live_source() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/position-mode.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("Node position-mode oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 84);
    for case in cases {
        let pm = case["variant"] == "binancepm";
        let mut trace = vec![];
        let error = fixture_error();
        let get = |endpoint: &'static str| {
            trace.push(json!({"client":if pm {"papiXhr"} else {"xhr"},"args":[endpoint]}));
            let result = if case["reject"] == true {
                Err(error.clone())
            } else {
                Ok(case.get("data").cloned())
            };
            std::future::ready(result)
        };
        let result = if pm {
            run(fetch_binance_pm_position_mode(get))
        } else {
            run(fetch_binance_position_mode(get))
        };
        let (result, error_out) = match result {
            Ok(value) => (json!(value), Value::Null),
            Err(actual) => (
                Value::Null,
                json!({"name":actual.name,"message":actual.message,"code":observed_error_code(&actual.code),"same":actual==error,"response":actual.response_json_view()}),
            ),
        };
        assert_eq!(
            json!({"trace":trace,"result":result,"error":error_out}),
            json!({"trace":case["trace"],"result":case["result"],"error":case["error"]}),
            "position-mode {} case{}",
            case["variant"],
            case["index"]
        );
    }
}

#[test]
fn readers_await_once_and_preserve_rejection_metadata() {
    for pm in [false, true] {
        let calls = Cell::new(0);
        let ready = Cell::new(false);
        let error = fixture_error();
        let get = |_: &'static str| {
            calls.set(calls.get() + 1);
            std::future::poll_fn(|_| {
                if ready.get() {
                    Poll::Ready(Err(error.clone()))
                } else {
                    Poll::Pending
                }
            })
        };
        let mut future: std::pin::Pin<Box<dyn Future<Output = Result<bool, ExchangeError>> + '_>> =
            if pm {
                Box::pin(fetch_binance_pm_position_mode(get))
            } else {
                Box::pin(fetch_binance_position_mode(get))
            };
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(calls.get(), 0);
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(calls.get(), 1);
        assert!(future.as_mut().poll(&mut context).is_pending());
        assert_eq!(calls.get(), 1);
        ready.set(true);
        assert_eq!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(error.clone()))
        );
        assert_eq!(calls.get(), 1);
    }
}

fn observed_error_code(
    code: &Option<safe_cex_exchange::js_runtime::value::Js>,
) -> Option<serde_json::Value> {
    code.as_ref().map(|value| {
        let finite =
            safe_cex_exchange::js_runtime::value::to_json(value).expect("finite JSON code fixture");
        let text = safe_cex_num::js_json_stringify(&finite.into())
            .expect("source JSON code fixture")
            .expect("defined code fixture");
        serde_json::from_str(&text).expect("JSON code observation")
    })
}
