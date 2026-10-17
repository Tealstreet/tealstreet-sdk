use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_binance::account_fetch::*;
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
        Poll::Ready(result) => result,
        Poll::Pending => panic!("fixture did not settle"),
    }
}
fn sentinel() -> ExchangeError {
    let mut error = ExchangeError::new("fixture request");
    error.name = "RangeError".into();
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        "fixture-code"
    )));
    error.response = Some(json!({"status":503,"data":{"msg":"venue detail"}}));
    error
}
fn cases() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-fetch.mjs"
        ))
        .output()
        .expect("Node account fetch oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    // 24 authored account fixtures x 2 rejection modes x 2 readers,
    // plus 12 authored spot fixtures x 2 rejection modes.
    assert_eq!(cases.len(), 120, "fixed source fixture count");
    cases
}
fn expected_alias(index: usize) -> Option<Value> {
    match index {
        0 => Some(json!("first-alias")),
        1 => Some(json!(113.75)),
        2 => Some(Value::Null),
        3 => Some(json!(false)),
        4 => Some(json!({"id":"nested","count":7})),
        5 => Some(json!([1, 2, 3])),
        6 | 9..=12 | 23 => None,
        _ => panic!("not an authored success case {index}"),
    }
}
fn expected_positions(index: usize) -> Option<Value> {
    match index {
        0 => Some(json!([{"symbol":"BTCUSDT","contracts":3.6},{"symbol":"ETHUSDT","contracts":7}])),
        1 => Some(json!([])),
        2 => Some(Value::Null),
        3 => Some(json!(false)),
        4 => Some(json!(113.75)),
        5 => Some(json!({"id":"opaque","count":7})),
        6 | 9..=11 => None,
        _ => panic!("not an authored success case {index}"),
    }
}
fn assert_error(actual: &ExchangeError, expected: &Value, fixture: &ExchangeError, label: &str) {
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "{label} native error name"
    );
    assert_eq!(
        actual.message,
        expected["message"].as_str().unwrap(),
        "{label} native error message"
    );
    assert_eq!(
        observed_error_code(&actual.code).as_ref(),
        expected.get("code").filter(|v| !v.is_null()),
        "{label} native error code"
    );
    assert_eq!(
        actual.response.as_ref(),
        expected.get("response").filter(|v| !v.is_null()),
        "{label} native response"
    );
    assert_eq!(
        actual == fixture,
        expected["same"].as_bool().unwrap(),
        "{label} native rejection preservation"
    );
}
fn decoded_positions(data: Option<Value>) -> FetchedPositions<Value> {
    match data {
        None => FetchedPositions::Undefined,
        Some(Value::Null) => FetchedPositions::Null,
        Some(Value::Object(mut record)) => FetchedPositions::Present {
            positions: record.remove("positions"),
        },
        Some(_) => FetchedPositions::Present { positions: None },
    }
}
fn compare_variant(variant: &str, count: usize) {
    let mut completed = 0;
    for case in cases()
        .into_iter()
        .filter(|case| case["variant"] == variant)
    {
        let index = case["index"].as_u64().unwrap() as usize;
        let reject = case["reject"].as_bool().unwrap();
        let label = format!("ACCOUNT FETCH {variant} case {index} reject {reject}");
        let fixture = sentinel();
        let data = if reject {
            Err(fixture.clone())
        } else {
            Ok(case.get("data").cloned())
        };
        let mut calls = 0;
        let mut endpoints = vec![];
        let result = if variant == "binancespot" {
            run(fetch_binance_spot_positions(|| {
                calls += 1;
                std::future::ready(data.map(decoded_positions))
            }))
        } else {
            let get = |path: &'static str| {
                calls += 1;
                endpoints.push(path);
                std::future::ready(data)
            };
            let result = if variant == "binancepm" {
                run(get_binance_pm_account(get))
            } else {
                run(get_binance_account(get))
            };
            result.map(|account| account.user_id)
        };
        // Native assertions precede the source trace checks. No re-encoding.
        match result {
            Ok(actual) => {
                assert!(
                    case["error"].is_null(),
                    "{label} native expected error but returned value"
                );
                assert!(!reject, "{label} native swallowed request rejection");
                let expected = if variant == "binancespot" {
                    expected_positions(index)
                } else {
                    expected_alias(index)
                };
                assert_eq!(actual, expected, "{label} native decoded value");
                let source = if variant == "binancespot" {
                    case.get("result")
                } else {
                    case["result"].get("userId")
                };
                assert_eq!(
                    actual.as_ref(),
                    source,
                    "{label} native source value, null vs undefined"
                );
                assert_eq!(case["sameValue"], true, "{label} source value identity");
            }
            Err(actual) => {
                assert!(
                    !case["error"].is_null(),
                    "{label} native unexpected error: {actual:?}"
                );
                if reject {
                    assert_eq!(actual, fixture, "{label} native original rejection");
                }
                assert_error(&actual, &case["error"], &fixture, &label);
            }
        }
        assert_eq!(calls, 1, "{label} native call count");
        if variant == "binancespot" {
            assert_eq!(endpoints.len(), 0, "{label} native no GET");
            assert_eq!(
                case["trace"][0]["fetch"], "fetchPositionsAndBalance",
                "{label} source fetch"
            );
        } else {
            let endpoint = if variant == "binancepm" {
                "/papi/v1/balance"
            } else {
                "/fapi/v3/balance"
            };
            assert_eq!(endpoints, [endpoint], "{label} native endpoint");
            assert_eq!(
                case["trace"][0]["args"][0], endpoint,
                "{label} source endpoint"
            );
            assert_eq!(
                case["trace"][0]["client"],
                if variant == "binancepm" {
                    "papiXhr"
                } else {
                    "xhr"
                },
                "{label} configured source client"
            );
        }
        assert_eq!(
            case["trace"].as_array().unwrap().len(),
            1,
            "{label} fixed source call count"
        );
        completed += 1;
    }
    assert_eq!(completed, count, "fixed variant fixture count {variant}");
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn futures_account_matches_source() {
    compare_variant("binance", 48);
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn pm_account_matches_source() {
    compare_variant("binancepm", 48);
}
#[test]
#[ignore = "requires installed TypeScript and actual source oracle"]
fn spot_positions_matches_source() {
    compare_variant("binancespot", 24);
}

#[test]
fn readers_await_once_and_preserve_rejection() {
    for variant in ["binance", "binancepm", "binancespot"] {
        let calls = Cell::new(0);
        let settled = Cell::new(false);
        let fixture = sentinel();
        let fetch = || {
            calls.set(calls.get() + 1);
            std::future::poll_fn(|_| {
                if settled.get() {
                    Poll::Ready(Err(fixture.clone()))
                } else {
                    Poll::Pending
                }
            })
        };
        let mut future: std::pin::Pin<
            Box<dyn Future<Output = Result<Option<Value>, ExchangeError>> + '_>,
        > = match variant {
            "binance" => {
                Box::pin(async { get_binance_account(|_| fetch()).await.map(|a| a.user_id) })
            }
            "binancepm" => {
                Box::pin(async { get_binance_pm_account(|_| fetch()).await.map(|a| a.user_id) })
            }
            _ => Box::pin(fetch_binance_spot_positions(|| async {
                fetch().await.map(decoded_positions)
            })),
        };
        let mut context = Context::from_waker(Waker::noop());
        assert_eq!(calls.get(), 0, "{variant} lazy reader");
        for _ in 0..2 {
            assert!(future.as_mut().poll(&mut context).is_pending());
            assert_eq!(calls.get(), 1, "{variant} native one pending request");
        }
        settled.set(true);
        assert_eq!(
            future.as_mut().poll(&mut context),
            Poll::Ready(Err(fixture.clone())),
            "{variant} native rejected value"
        );
        assert_eq!(calls.get(), 1, "{variant} native no retry");
    }
}

#[test]
fn returned_arrays_move_without_cloning() {
    for variant in ["binance", "binancepm", "binancespot"] {
        let values = vec![json!({"id":"first"}), json!({"id":"second"})];
        let buffer = values.as_ptr();
        let key = if variant == "binancespot" {
            "positions"
        } else {
            "accountAlias"
        };
        let record = Value::Object([(key.into(), Value::Array(values))].into_iter().collect());
        let data = if variant == "binancespot" {
            record
        } else {
            Value::Array(vec![record])
        };
        let returned = match variant {
            "binance" => {
                run(get_binance_account(|_| std::future::ready(Ok(Some(data)))))
                    .unwrap()
                    .user_id
            }
            "binancepm" => {
                run(get_binance_pm_account(|_| {
                    std::future::ready(Ok(Some(data)))
                }))
                .unwrap()
                .user_id
            }
            _ => run(fetch_binance_spot_positions(|| {
                std::future::ready(Ok(decoded_positions(Some(data))))
            }))
            .unwrap(),
        }
        .unwrap();
        assert_eq!(
            returned,
            json!([{"id":"first"},{"id":"second"}]),
            "{variant} native moved decoded contents"
        );
        assert_eq!(
            returned.as_array().unwrap().as_ptr(),
            buffer,
            "{variant} native allocation preserved"
        );
    }
}

#[test]
fn native_spot_positions_preserve_nonfinite_fields_and_identity() {
    use safe_cex_types::{Position, PositionSide};
    use std::rc::Rc;
    let input = Rc::new(vec![Position {
        id: "native-spot".into(),
        symbol: "BTCUSDT".into(),
        side: safe_cex_types::ModelField::Value(PositionSide::Long),
        entry_price: 113.75,
        notional: f64::INFINITY,
        leverage: 1.0,
        margin_type: (None).into(),
        position_mode: (None).into(),
        unrealized_pnl: f64::from_bits(0x7ff8_0000_0000_0031),
        realized_pnl: (Some(f64::NEG_INFINITY)).into(),
        contracts: 3.6,
        liquidation_price: -0.0,
        margin_size: (None).into(),
        info: (None).into(),
        extra: (None).into(),
        last_update: (None).into(),
        option: (None).into(),
    }]);
    let forwarded = input.clone();
    let returned = run(fetch_binance_spot_positions(|| {
        std::future::ready(Ok(FetchedPositions::Present {
            positions: Some(forwarded),
        }))
    }))
    .unwrap()
    .unwrap();
    assert_eq!(returned.len(), 1, "native spot fixed position count");
    let position = &returned[0];
    assert_eq!(position.id, "native-spot", "native spot position ID");
    assert_eq!(position.symbol, "BTCUSDT", "native spot position symbol");
    assert_eq!(
        position.contracts.to_bits(),
        3.6f64.to_bits(),
        "native spot quantity"
    );
    assert_eq!(
        position.entry_price.to_bits(),
        113.75f64.to_bits(),
        "native spot entry price"
    );
    assert_eq!(
        position.notional,
        f64::INFINITY,
        "native spot infinite notional"
    );
    assert_eq!(
        position.unrealized_pnl.to_bits(),
        0x7ff8_0000_0000_0031,
        "native spot NaN payload"
    );
    assert_eq!(
        position.realized_pnl,
        Some(f64::NEG_INFINITY),
        "native spot negative infinity"
    );
    assert_eq!(
        position.liquidation_price.to_bits(),
        (-0.0f64).to_bits(),
        "native spot signed zero"
    );
    assert!(
        Rc::ptr_eq(&returned, &input),
        "native spot positions identity"
    );
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
