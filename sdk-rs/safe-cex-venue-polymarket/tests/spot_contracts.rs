use safe_cex_types::{ExchangeName, PositionMode};
use safe_cex_venue_polymarket::spot_contracts::*;
use serde_json::Value;
use std::{
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn run<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("spot fixture unexpectedly yielded"),
    }
}
fn number_bits(value: &Value) -> u64 {
    u64::from_str_radix(value.as_str().unwrap(), 16).unwrap()
}
#[derive(Debug)]
struct LogFailure {
    name: String,
    message: String,
    marker: u64,
}
fn warning(
    method: usize,
    symbol: &str,
    leverage: f64,
    flag: bool,
    log: &mut impl FnMut(&str, &str) -> Result<(), Rc<LogFailure>>,
) -> Result<(), Rc<LogFailure>> {
    match method {
        3 => run(set_leverage(log, symbol, leverage)),
        4 => run(change_position_mode(log, flag)),
        5 => run(change_margin_type(log, flag)),
        _ => panic!("not a warning method"),
    }
}
fn calls(actual: &[(String, String)], expected: &Value, method: &str, count: usize) {
    assert_eq!(actual.len(), count, "spot_log_native_count:{method}");
    assert_eq!(
        expected.as_array().unwrap().len(),
        count,
        "spot_log_source_count:{method}"
    );
    for index in 0..count {
        let actual = actual.get(index).expect("spot_log_native_present");
        assert_eq!(
            actual.0,
            expected[index][0].as_str().unwrap(),
            "spot_log_native_message:{method}"
        );
        assert_eq!(
            actual.1,
            expected[index][1].as_str().unwrap(),
            "spot_log_native_severity:{method}"
        );
    }
}
#[test]
fn spot_contracts_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot_contracts_oracle.cjs"
        ))
        .output()
        .unwrap();
    eprintln!("polymarket spot oracle: one Node invocation for the entire corpus");
    assert!(
        output.status.success(),
        "spot_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!("capture header: {}", corpus["capture"]);
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 30, "spot_source_case_count");
    let names = [
        "name",
        "getPositionMode",
        "fetchSymbolMaxLeverage",
        "setLeverage",
        "changePositionMode",
        "changeMarginType",
    ];
    let (mut compared, mut logged) = (0, 0);
    // Independent six-by-five floor: a source subset cannot shrink the native walk.
    for (method, source_name) in names.iter().enumerate() {
        for iteration in 0..5 {
            let case = cases
                .get(method * 5 + iteration)
                .expect("spot_source_case_present");
            assert_eq!(case["name"], *source_name, "spot_source_method_order");
            assert_eq!(case["iteration"], iteration, "spot_source_iteration");
            let symbol = case["input"]["symbol"].as_str().unwrap();
            let leverage = f64::from_bits(number_bits(&case["input"]["leverageBits"]));
            let flag = case["input"]["flag"].as_bool().unwrap();
            let mut records = vec![];
            let mut log = |message: &str, severity: &str| {
                records.push((message.to_owned(), severity.to_owned()));
                Ok::<_, Rc<LogFailure>>(())
            };
            match method {
                0 => {
                    assert_eq!(name(), ExchangeName::Polymarket, "spot_name_native");
                    assert_eq!(case["value"], "polymarket", "spot_name_source");
                    assert_eq!(case["type"], "string", "spot_name_source_type");
                }
                1 => {
                    assert_eq!(
                        run(get_position_mode()),
                        PositionMode::OneWay,
                        "spot_mode_native"
                    );
                    assert_eq!(case["value"], "oneway", "spot_mode_source");
                    assert_eq!(case["type"], "string", "spot_mode_source_type");
                }
                2 => {
                    assert_eq!(
                        run(fetch_symbol_max_leverage(symbol)).to_bits(),
                        number_bits(&case["bits"]),
                        "spot_leverage_native"
                    );
                    assert_eq!(case["type"], "number", "spot_leverage_source_type");
                }
                3..=5 => {
                    warning(method, symbol, leverage, flag, &mut log)
                        .expect("spot_warning_native_success");
                    assert_eq!(case["type"], "undefined", "spot_warning_source_return");
                }
                _ => unreachable!(),
            }
            let count = usize::from(method >= 3);
            assert_eq!(
                case["immediateCalls"], count,
                "spot_source_eager_calls:{source_name}"
            );
            calls(&records, &case["calls"], source_name, count);
            compared += 1;
            logged += count;
        }
    }
    assert_eq!(compared, 30, "spot_performed_native_comparisons");
    assert_eq!(logged, 15, "spot_performed_log_comparisons");
    let failures = corpus["failures"].as_array().unwrap();
    assert_eq!(failures.len(), 3, "spot_source_failure_count");
    for index in 0..3 {
        let case = failures.get(index).expect("spot_source_failure_present");
        let source_name = names[index + 3];
        assert_eq!(case["name"], source_name, "spot_source_failure_method");
        assert_eq!(case["preserved"], true, "spot_source_failure_identity");
        assert_eq!(case["immediateCalls"], 1, "spot_source_failure_eager_calls");
        let error = Rc::new(LogFailure {
            name: "LogFailure".into(),
            message: "log listener threw".into(),
            marker: 7,
        });
        let mut records = vec![];
        let mut log = |message: &str, severity: &str| {
            records.push((message.to_owned(), severity.to_owned()));
            Err(error.clone())
        };
        let actual = warning(index + 3, "token-1", 12.5, true, &mut log)
            .expect_err("spot_warning_native_rejection");
        assert!(
            Rc::ptr_eq(&actual, &error),
            "spot_error_native_identity:{source_name}"
        );
        assert_eq!(actual.name, case["error"]["name"], "spot_error_native_name");
        assert_eq!(
            actual.message, case["error"]["message"],
            "spot_error_native_message"
        );
        assert_eq!(
            actual.marker,
            case["error"]["context"]["marker"].as_u64().unwrap(),
            "spot_error_native_context"
        );
        calls(&records, &case["calls"], source_name, 1);
    }
}

#[test]
fn warning_calls_precede_future_poll() {
    use std::{cell::RefCell, pin::Pin};
    let names = ["setLeverage", "changePositionMode", "changeMarginType"];
    let mut checked = 0;
    for (index, method) in names.iter().enumerate() {
        let records = Rc::new(RefCell::new(vec![]));
        let logged = records.clone();
        let mut log = move |message: &str, severity: &str| {
            logged
                .borrow_mut()
                .push((message.to_owned(), severity.to_owned()));
            Ok::<_, ()>(())
        };
        let future: Pin<Box<dyn Future<Output = Result<(), ()>> + '_>> = match index {
            0 => Box::pin(set_leverage(&mut log, "token-1", 12.5)),
            1 => Box::pin(change_position_mode(&mut log, true)),
            2 => Box::pin(change_margin_type(&mut log, false)),
            _ => unreachable!(),
        };
        assert_eq!(
            records.borrow().len(),
            1,
            "spot_warning_native_eager:{method}"
        );
        run(future).expect("spot_eager_future_success");
        assert_eq!(
            records.borrow().len(),
            1,
            "spot_warning_native_once:{method}"
        );
        checked += 1;
    }
    assert_eq!(checked, 3, "spot_eager_performed_comparisons");
}
