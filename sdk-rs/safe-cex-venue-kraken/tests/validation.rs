use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_types::{Balance, ValidateAccountResult};
use safe_cex_venue_kraken::validation::{self, ValidationRead};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn, ready},
    process::Command,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/validation-oracle.cjs"
            ))
            .output()
            .expect("validation actual-source oracle");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(!source["header"]["arch"].as_str().unwrap().is_empty());
        assert!(!source["header"]["v8"].as_str().unwrap().is_empty());
        assert_eq!(
            source["header"]["hashes"].as_object().unwrap().len(),
            2,
            "VALIDATION actual source hashes"
        );
        source
    })
}

fn number(value: f64) -> Value {
    json!({"number":format!("{:016x}", value.to_bits())})
}

fn balance(free: f64) -> Balance {
    Balance {
        free,
        used: 17.5,
        total: 29.75,
        upnl: -31.125,
        currencies: Some(
            [(
                "USD".into(),
                Balance {
                    free: 37.25,
                    used: 41.5,
                    total: 43.75,
                    upnl: -47.125,
                    info: Some(json!({"tag":"currency"}).into()),
                    currencies: None,
                    coins: None,
                }
                .into(),
            )]
            .into_iter()
            .collect(),
        ),
        info: Some(json!({"tag":"balance"}).into()),
        coins: None,
    }
}

fn assert_balance(value: &Balance, expected: &Value) {
    for (name, native) in [
        ("free", value.free),
        ("used", value.used),
        ("total", value.total),
        ("upnl", value.upnl),
    ] {
        assert_eq!(
            number(native),
            expected[name],
            "VALIDATION native balance {name}"
        );
    }
    assert_eq!(
        value.info.as_ref().unwrap(),
        &expected["info"].clone().into(),
        "VALIDATION native balance info"
    );
    assert!(value.coins.is_none(), "VALIDATION absent coins");
    let currencies = value.currencies.as_ref().unwrap();
    assert_eq!(currencies.len(), 1, "VALIDATION native currency count");
    let (key, currency) = currencies.get_index(0).expect("VALIDATION currency index");
    assert_eq!(key, "USD", "VALIDATION native currency key");
    let currency = currency.as_balance().unwrap();
    for (name, native) in [
        ("free", currency.free),
        ("used", currency.used),
        ("total", currency.total),
        ("upnl", currency.upnl),
    ] {
        assert_eq!(
            number(native),
            expected["currencies"]["USD"][name],
            "VALIDATION native currency {name}"
        );
    }
    assert_eq!(
        currency.info.as_ref().unwrap(),
        &expected["currencies"]["USD"]["info"].clone().into()
    );
    assert!(currency.coins.is_none() && currency.currencies.is_none());
}

fn compare(family: &str) {
    let source = oracle();
    let cases = source[family].as_array().unwrap();
    assert_eq!(cases.len(), 76, "VALIDATION {family} source cases");
    let mut completed = 0;
    let mut native_balances = 0;
    let mut balance_calls = 0;
    let mut thrown = 0;
    for index in 0..76 {
        let case = &cases[index];
        let trace = RefCell::new(Vec::new());
        let requests = RefCell::new(Vec::new());
        let released = Cell::new(false);
        let late = Cell::new(false);
        let mut original = ExchangeError::new(case["message"].as_str().unwrap_or("read failure"));
        original.metadata.response = case.get("response").cloned();
        let read = |request: ValidationRead| {
            assert_eq!(
                request.method,
                if family == "futures" { "GET" } else { "POST" },
                "VALIDATION native request method"
            );
            assert_eq!(
                request.endpoint,
                if family == "futures" {
                    "/v3/accounts"
                } else {
                    "/0/private/Balance"
                },
                "VALIDATION native endpoint"
            );
            if family == "spot" {
                assert_eq!(
                    request.body,
                    Some(json!({})),
                    "VALIDATION native empty POST body"
                );
            } else {
                assert!(request.body.is_none(), "VALIDATION native absent GET body");
            }
            requests
                .borrow_mut()
                .push(json!({"method":request.method,"endpoint":request.endpoint,
                "body":request.body.unwrap_or(json!({"undefined":true}))}));
            trace.borrow_mut().push("read".to_owned());
            if case["mode"] == "sync" {
                return Err(original);
            }
            let mut response: Option<Result<Option<JsNumberInput>, ExchangeError>> =
                Some(if case["mode"] == "async" {
                    Err(original)
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str(raw).unwrap()))
                });
            let released = &released;
            Ok(poll_fn(move |_| {
                if released.get() {
                    Poll::Ready(response.take().unwrap())
                } else {
                    Poll::Pending
                }
            }))
        };
        let fetch_balance = || {
            trace.borrow_mut().push(
                if late.get() {
                    "late-balance"
                } else {
                    "balance"
                }
                .to_owned(),
            );
            if case["balanceMode"] == "sync" {
                return Err(ExchangeError::new("balance failure"));
            }
            Ok(ready(if case["balanceMode"] == "async" {
                Err(ExchangeError::new("balance failure"))
            } else {
                Ok(balance(if late.get() { 53.625 } else { 11.25 }))
            }))
        };
        let future: std::pin::Pin<
            Box<dyn Future<Output = Result<ValidateAccountResult, ExchangeError>> + '_>,
        > = if family == "futures" {
            Box::pin(validation::validate_futures_account(read, fetch_balance))
        } else {
            Box::pin(validation::validate_spot_account(read, fetch_balance))
        };
        let mut future = future;
        let mut context = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut context);
        assert_eq!(
            *trace.borrow(),
            vec!["read"],
            "VALIDATION native prefix trace"
        );
        assert_eq!(
            case["prefixTrace"],
            json!(["read"]),
            "VALIDATION source prefix trace"
        );
        late.set(case["balanceMode"] == "late");
        released.set(true);
        let result = match first {
            Poll::Ready(value) => value,
            Poll::Pending => match future.as_mut().poll(&mut context) {
                Poll::Ready(value) => value,
                Poll::Pending => panic!("VALIDATION released future remained pending"),
            },
        };
        assert_eq!(
            requests.borrow().len(),
            1,
            "VALIDATION independent request count"
        );
        assert_eq!(
            *requests.borrow(),
            *case["calls"].as_array().unwrap(),
            "VALIDATION native requests"
        );
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "VALIDATION native continuation trace"
        );
        balance_calls += trace
            .borrow()
            .iter()
            .filter(|s| s.ends_with("balance"))
            .count();
        if let Some(expected) = case.get("thrown") {
            let error = result.expect_err("VALIDATION expected native rejection");
            assert_eq!(
                error.name,
                expected["name"].as_str().unwrap(),
                "VALIDATION native rejection name"
            );
            assert_eq!(
                error.message,
                expected["message"].as_str().unwrap(),
                "VALIDATION native rejection message"
            );
            thrown += 1;
        } else {
            let result = result.expect("VALIDATION expected native resolution");
            let expected = &case["result"];
            assert_eq!(
                result.error.as_value().as_str(),
                expected["error"].as_str(),
                "VALIDATION {family} native error case {index}"
            );
            if let Some(value) = &result.balance {
                assert_balance(value, &expected["balance"]);
                native_balances += 1;
            } else {
                assert!(
                    expected.get("balance").is_none()
                        || expected["balance"] == json!({"undefined":true}),
                    "VALIDATION native absent balance"
                );
            }
        }
        completed += 1;
    }
    assert_eq!(completed, 76, "VALIDATION completed {family} cases");
    assert_eq!(
        native_balances,
        if family == "futures" { 4 } else { 29 },
        "VALIDATION independent {family} balances"
    );
    assert_eq!(
        balance_calls,
        if family == "futures" { 6 } else { 31 },
        "VALIDATION independent {family} balance calls"
    );
    assert_eq!(
        thrown,
        if family == "futures" { 3 } else { 0 },
        "VALIDATION independent {family} rejections"
    );
}

#[test]
fn futures_validation_live_source() {
    compare("futures");
}

#[test]
fn spot_validation_live_source() {
    compare("spot");
}

#[test]
fn futures_utf16_representation_boundary() {
    let source = oracle();
    let cases = source["futuresUtf16Boundaries"].as_array().unwrap();
    assert_eq!(cases.len(), 2, "VALIDATION paired UTF16 boundary cases");
    for index in 0..2 {
        let case = &cases[index];
        let prefix = if index == 0 {
            "Kraken API Error: "
        } else {
            "Validation failed: "
        };
        let mut units: Vec<u16> = prefix.encode_utf16().collect();
        units.push(55357);
        assert_eq!(
            case["units"],
            json!(units),
            "VALIDATION source lone surrogate acceptance"
        );
        assert_eq!(
            case["hasBalance"], false,
            "VALIDATION boundary source absent balance"
        );
        let read = |_: ValidationRead| {
            Ok(ready(if index == 0 {
                Ok(Some(
                    serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
                ))
            } else {
                let mut error = ExchangeError::new("original");
                error.metadata.response = Some(json!({"data":{"error":"😀"}}));
                Err(error)
            }))
        };
        let balance_calls = Cell::new(0);
        let future = validation::validate_futures_account(read, || {
            balance_calls.set(balance_calls.get() + 1);
            Ok(ready(Ok(balance(11.25))))
        });
        let result = match std::pin::pin!(future)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("VALIDATION immediate boundary future"),
        };
        assert_eq!(
            balance_calls.get(),
            0,
            "VALIDATION forbidden boundary balance call"
        );
        if index == 0 {
            let value = result.expect("VALIDATION envelope representation diagnostic");
            assert_eq!(
                value.error.as_value().as_str(),
                Some("Validation failed: Validation error index is outside shared UTF-8 profile"),
                "VALIDATION native envelope representation refusal"
            );
            assert!(
                value.balance.is_none(),
                "VALIDATION native boundary absent balance"
            );
        } else {
            let error = result.expect_err("VALIDATION catch representation refusal");
            assert_eq!(
                error.name, "TypeError",
                "VALIDATION native representation error name"
            );
            assert_eq!(
                error.message, "Validation error index is outside shared UTF-8 profile",
                "VALIDATION native representation cause"
            );
        }
    }
}
