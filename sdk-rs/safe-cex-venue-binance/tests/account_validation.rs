use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::number_from_string;
use safe_cex_types::Balance;
use safe_cex_venue_binance::account_validation::{
    AccountValidation, AccountValidationClient, FetchedAccountBalance, validate_binance_account,
    validate_binance_pm_account, validate_binance_spot_account,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn settle<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("ready validation callbacks must settle"),
    }
}
fn client_name(client: AccountValidationClient) -> &'static str {
    match client {
        AccountValidationClient::Primary => "primary",
        AccountValidationClient::Secondary => "secondary",
        AccountValidationClient::Spot => "spot",
    }
}
fn assert_number(actual: f64, text: &str, label: &str) {
    let expected = number_from_string(text);
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label}: native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label}: native Number bits"
        );
    }
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn futures_validation_matches_source() {
    run_cases("binance");
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn pm_validation_matches_source() {
    run_cases("binancepm");
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn spot_validation_matches_source() {
    run_cases("binancespot");
}

fn run_cases(variant: &str) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-validation.mjs"
        ))
        .args(["--variant", variant])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 106, "VALIDATION source cardinality {variant}");
    let mut completed = 0;
    for (index, c) in cases.iter().enumerate() {
        assert_eq!(c["variant"], variant, "VALIDATION source family identity");
        let spec = c["index"].as_u64().unwrap() as usize;
        let parse_error = variant == "binance" && matches!(spec, 8..=10);
        let primary_failure = c["primaryReject"] == true || parse_error;
        let secondary = variant != "binancespot" && !primary_failure && c["key"] == "backup";
        let fetched = variant != "binance"
            && !primary_failure
            && !(secondary && c["secondaryReject"] == true);
        let trace = RefCell::new(Vec::<Value>::new());
        let requests = RefCell::new(Vec::<(AccountValidationClient, &'static str)>::new());
        let emitted = RefCell::new(Vec::<Value>::new());
        let key_reads = Cell::new(0);
        let fetches = Cell::new(0);
        let payload =
            Rc::new(json!({"total":211,"free":157,"used":54,"upnl":-13,"tag":"opaque balance"}));
        assert_eq!(c["payload"], *payload, "VALIDATION fixed balance input");
        let null_payload = Rc::new(Value::Null);
        let mut sentinel = ExchangeError::new("emitter failed");
        sentinel.name = "RangeError".into();
        sentinel.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emitter-code"
        )));
        sentinel.response = Some(json!({"status":503}));
        let get = |client, url| {
            trace
                .borrow_mut()
                .push(json!(["get", client_name(client), url]));
            requests.borrow_mut().push((client, url));
            let reject = if client == AccountValidationClient::Secondary {
                c["secondaryReject"] == true
            } else {
                c["primaryReject"] == true
            };
            let mut error = ExchangeError::new(c["message"].as_str().unwrap());
            error.response = Some(json!({"data":{"msg":c["msg"]}}));
            std::future::ready(if reject {
                Err(error)
            } else {
                Ok(c.get("data").cloned())
            })
        };
        let key = || {
            key_reads.set(key_reads.get() + 1);
            trace.borrow_mut().push(json!(["key"]));
            c["key"] == "backup"
        };
        let fetch = || {
            fetches.set(fetches.get() + 1);
            trace.borrow_mut().push(json!([
                "fetch",
                if variant == "binancepm" {
                    "fetchBalance"
                } else {
                    "fetchPositionsAndBalance"
                }
            ]));
            std::future::ready(match c["fetchKind"].as_str().unwrap() {
                "value" => Ok(Some(FetchedAccountBalance {
                    balance: Some(payload.clone()),
                })),
                "null-balance" => Ok(Some(FetchedAccountBalance {
                    balance: Some(null_payload.clone()),
                })),
                "missing" => Ok(Some(FetchedAccountBalance { balance: None })),
                "undefined-result" | "null-result" => Ok(None),
                "reject" => Err(ExchangeError::new("balance failed")),
                kind => panic!("unknown source fetch case {kind}"),
            })
        };
        let emit = |message: Value| {
            emitted.borrow_mut().push(message.clone());
            trace.borrow_mut().push(json!(["emit", message]));
            if c["emitReject"] == true {
                Err(sentinel.clone())
            } else {
                Ok(())
            }
        };
        // Enum, numeric Balance fields and forwarded handles are observed natively.
        let native_error = if variant == "binance" {
            let result = settle(validate_binance_account(get, key, emit));
            match result {
                Ok(AccountValidation::Valid { balance }) => {
                    assert_eq!(
                        c["expected"]["result"]["kind"], "valid",
                        "VALIDATION native outcome {variant} case {index}"
                    );
                    let expected = match spec {
                        3 => Some(["113.75", "29.5", "84.25", "-7.25"]),
                        4 => Some(["-0", "0", "-0", "0"]),
                        7 => Some(["8", "Infinity", "-Infinity", "-Infinity"]),
                        11 => Some(["12.5", "0", "12.5", "0"]),
                        _ => None,
                    };
                    assert_eq!(
                        balance.is_some(),
                        expected.is_some(),
                        "VALIDATION native balance presence case {index}"
                    );
                    if let (Some(b), Some(values)) = (balance, expected) {
                        for ((name, actual), wanted) in [
                            ("total", b.total),
                            ("free", b.free),
                            ("used", b.used),
                            ("upnl", b.upnl),
                        ]
                        .into_iter()
                        .zip(values)
                        {
                            assert_number(
                                actual,
                                wanted,
                                &format!("VALIDATION native balance {name} case {index}"),
                            );
                            assert_number(
                                actual,
                                c["expected"]["result"]["balance"][name].as_str().unwrap(),
                                "VALIDATION source balance Number",
                            );
                        }
                        assert!(
                            b.info.is_none() && b.currencies.is_none() && b.coins.is_none(),
                            "VALIDATION native four-field balance"
                        );
                    }
                    assert_eq!(c["expected"]["result"]["hasBalance"], true);
                    None
                }
                Ok(AccountValidation::Invalid { error }) => {
                    assert_invalid(&error, c, index);
                    None
                }
                Err(error) => Some(error),
            }
        } else {
            let result = if variant == "binancepm" {
                settle(validate_binance_pm_account(get, key, fetch, emit))
            } else {
                settle(validate_binance_spot_account(get, fetch, emit))
            };
            match result {
                Ok(AccountValidation::Valid { balance }) => {
                    assert_eq!(
                        c["expected"]["result"]["kind"], "valid",
                        "VALIDATION native outcome {variant} case {index}"
                    );
                    match c["fetchKind"].as_str().unwrap() {
                        "value" => {
                            let b = balance.expect("VALIDATION native forwarded balance");
                            assert!(
                                Rc::ptr_eq(&b, &payload),
                                "VALIDATION native opaque balance identity"
                            );
                            assert_eq!(
                                *b, c["expected"]["result"]["balance"],
                                "VALIDATION decoded forwarded value"
                            );
                        }
                        "null-balance" => {
                            let b = balance.expect("VALIDATION native explicit null balance");
                            assert!(Rc::ptr_eq(&b, &null_payload));
                            assert_eq!(c["expected"]["result"]["balanceKind"], "null");
                        }
                        _ => {
                            assert!(balance.is_none(), "VALIDATION best-effort native undefined");
                            assert_eq!(c["expected"]["result"]["balanceKind"], "undefined");
                        }
                    }
                    assert_eq!(c["expected"]["result"]["hasBalance"], true);
                    None
                }
                Ok(AccountValidation::Invalid { error }) => {
                    assert_invalid(&error, c, index);
                    None
                }
                Err(error) => Some(error),
            }
        };
        let expected_error = &c["expected"]["error"];
        assert_eq!(
            native_error.is_some(),
            !expected_error.is_null(),
            "VALIDATION native thrown error presence {variant} case {index}"
        );
        if let Some(error) = native_error {
            assert_eq!(
                error.name,
                expected_error["name"].as_str().unwrap(),
                "VALIDATION native thrown name"
            );
            assert_eq!(
                error.message,
                expected_error["message"].as_str().unwrap(),
                "VALIDATION native thrown message"
            );
            assert_eq!(
                observed_error_code(&error.code)
                    .as_ref()
                    .unwrap_or(&Value::Null),
                &expected_error["code"]
            );
            assert_eq!(
                error.response.as_ref().unwrap_or(&Value::Null),
                &expected_error["response"]
            );
            assert_eq!(
                error == sentinel,
                expected_error["same"].as_bool().unwrap(),
                "VALIDATION native emitter identity"
            );
        }
        assert_eq!(
            requests.borrow().len(),
            1 + usize::from(secondary),
            "VALIDATION native GET count {variant} case {index}"
        );
        for (request, (client, url)) in requests.borrow().iter().enumerate() {
            assert_eq!(
                *client,
                if request == 1 {
                    AccountValidationClient::Secondary
                } else if variant == "binancespot" {
                    AccountValidationClient::Spot
                } else {
                    AccountValidationClient::Primary
                },
                "VALIDATION native client {variant} case {index}"
            );
            assert_eq!(
                *url,
                match variant {
                    "binance" => "/fapi/v3/account",
                    "binancepm" => "/papi/v1/account",
                    _ => "/api/v3/account",
                },
                "VALIDATION native endpoint {variant} case {index}"
            );
        }
        assert_eq!(
            key_reads.get(),
            usize::from(variant != "binancespot" && !primary_failure),
            "VALIDATION native option reads"
        );
        assert_eq!(
            fetches.get(),
            usize::from(fetched),
            "VALIDATION native best-effort fetch count {variant} case {index}"
        );
        assert_eq!(
            emitted.borrow().len(),
            usize::from(primary_failure),
            "VALIDATION native primary emission count {variant} case {index}"
        );
        assert_eq!(
            trace.borrow().as_slice(),
            c["expected"]["trace"].as_array().unwrap().as_slice(),
            "VALIDATION source effect order {variant} case {index}"
        );
        completed += 1;
    }
    assert_eq!(
        completed, 106,
        "VALIDATION completed comparison floor {variant}"
    );
}
fn assert_invalid(error: &Value, c: &Value, index: usize) {
    assert_eq!(
        c["expected"]["result"]["kind"], "invalid",
        "VALIDATION native invalid variant case {index}"
    );
    assert_eq!(
        error, &c["expected"]["result"]["error"],
        "VALIDATION decoded invalid value case {index}"
    );
    assert_eq!(c["expected"]["result"]["hasBalance"], false);
}

#[test]
fn pending_primary_skips_option_fetch_and_emission() {
    for variant in ["binance", "binancepm", "binancespot"] {
        let calls = Cell::new(0);
        let reads = Cell::new(0);
        let get = |_: AccountValidationClient, _: &'static str| {
            calls.set(calls.get() + 1);
            std::future::pending::<Result<Option<Value>, ExchangeError>>()
        };
        let key = || {
            reads.set(reads.get() + 1);
            true
        };
        let fetch=|| -> std::future::Ready<Result<Option<FetchedAccountBalance<Balance>>,ExchangeError>> {panic!("pending validation cannot fetch balance")};
        let emit =
            |_: Value| -> Result<(), ExchangeError> { panic!("pending validation cannot emit") };
        let mut future: std::pin::Pin<
            Box<dyn Future<Output = Result<AccountValidation<Balance>, ExchangeError>> + '_>,
        > = match variant {
            "binance" => Box::pin(validate_binance_account(get, key, emit)),
            "binancepm" => Box::pin(validate_binance_pm_account(get, key, fetch, emit)),
            _ => Box::pin(validate_binance_spot_account(get, fetch, emit)),
        };
        for _ in 0..2 {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(calls.get(), 1, "VALIDATION one pending primary GET");
            assert_eq!(
                reads.get(),
                0,
                "VALIDATION option read follows primary await"
            );
        }
        drop(future);
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
