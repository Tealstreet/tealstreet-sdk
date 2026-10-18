use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{js_number_string, number_from_string};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Market, Position};
use safe_cex_venue_binance::leverage::{
    BinanceLeverageRequest, set_binance_leverage, set_binance_pm_leverage,
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
        Poll::Pending => panic!("ready leverage POST must settle"),
    }
}
fn number_text(n: f64) -> String {
    if n == 0.0 && n.is_sign_negative() {
        "-0".into()
    } else {
        js_number_string(n)
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
fn fixture(market: bool, positions: bool, min: f64) -> DefaultStore {
    let store = DefaultStore::new();
    let mut m:Market=serde_json::from_value(json!({"id":"market-id","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":10}}})).unwrap();
    m.limits.leverage.min = min;
    let rows = if positions {
        [("BTCUSDT:long","BTCUSDT","long",5.0),("BTCUSDT:short","BTCUSDT","short",11.0),("ETHUSDT:long","ETHUSDT","long",13.0)].into_iter().map(|(id,symbol,side,leverage)| {
        serde_json::from_value::<Position>(json!({"id":id,"symbol":symbol,"side":side,"entryPrice":1,"notional":2,"leverage":leverage,"unrealizedPnl":4,"contracts":1,"liquidationPrice":6})).unwrap()
    }).collect()
    } else {
        vec![]
    };
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(if market { vec![m] } else { vec![] }),
            positions: Some(rows),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn leverage_matches_live_source_and_native_store() {
    let out = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/leverage.mjs"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 138, "LEVERAGE source cardinality");
    let mut completed = 0;
    for (index, c) in cases.iter().enumerate() {
        assert_eq!(
            c["variant"],
            if index < 69 { "binance" } else { "binancepm" },
            "LEVERAGE source family identity case {index}"
        );
        let spec = c["spec"].as_u64().unwrap() as usize;
        let input = [
            "20",
            "0",
            "7",
            "5",
            "NaN",
            "Infinity",
            "-Infinity",
            "-0",
            "7",
        ][spec];
        let clamped = ["10", "1", "7", "5", "NaN", "10", "1", "-0", "7"][spec];
        assert_eq!(
            c["input"], input,
            "LEVERAGE fixed native input case {index}"
        );
        assert_eq!(c["min"], if spec == 7 { "-0" } else { "1" });
        let market = c["market"].as_bool().unwrap();
        let positions = c["positions"].as_bool().unwrap();
        let should_post = market && positions && spec != 3;
        let reject = c["reject"].as_bool().unwrap();
        let store = fixture(
            market,
            positions,
            number_from_string(c["min"].as_str().unwrap()),
        );
        let trace = Rc::new(RefCell::new(vec![]));
        let notifications = Rc::new(RefCell::new(Vec::<Vec<f64>>::new()));
        let listener_reject = c["listenerReject"] == true;
        let t = trace.clone();
        let n = notifications.clone();
        let unsubscribe = Store::subscribe(
            &store,
            Box::new(move |data, _| {
                let values: Vec<_> = data.positions.iter().map(|p| p.leverage).collect();
                n.borrow_mut().push(values.clone());
                t.borrow_mut().push(json!([
                    "notify",
                    values.into_iter().map(number_text).collect::<Vec<_>>()
                ]));
                if listener_reject {
                    let mut error = ExchangeError::new("listener failed");
                    error.response = Some(json!({"data":{"msg":"subscriber detail"}}));
                    Err(error.into())
                } else {
                    Ok(())
                }
            }),
        );
        let posts = RefCell::new(Vec::<(String, BinanceLeverageRequest)>::new());
        let mut emitter_error = ExchangeError::new("emitter failed");
        emitter_error.name = "RangeError".into();
        emitter_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emitter-code"
        )));
        let client = if c["variant"] == "binancepm" {
            "papiXhr"
        } else {
            "xhr"
        };
        let post = |url: &'static str, body: BinanceLeverageRequest| {
            trace.borrow_mut().push(json!([
                "post",
                client,
                url,
                body.symbol,
                number_text(body.leverage)
            ]));
            posts.borrow_mut().push((url.into(), body));
            let mut error = ExchangeError::new("request failed");
            error.response = Some(json!({"data":{"msg":c["msg"]}}));
            std::future::ready(if reject { Err(error) } else { Ok(()) })
        };
        let emit = |message: Value| {
            trace.borrow_mut().push(json!(["emit", message]));
            if c["emitReject"] == true {
                Err(emitter_error.clone())
            } else {
                Ok(())
            }
        };
        let outcome = if client == "papiXhr" {
            settle(set_binance_pm_leverage(
                &store,
                "BTCUSDT",
                number_from_string(input),
                post,
                emit,
            ))
        } else {
            settle(set_binance_leverage(
                &store,
                "BTCUSDT",
                number_from_string(input),
                post,
                emit,
            ))
        };
        // Native typed request and Store values are observed before the trace/wire view.
        assert_eq!(
            posts.borrow().len(),
            usize::from(should_post),
            "LEVERAGE native POST count case {index}"
        );
        for (url, body) in posts.borrow().iter() {
            assert_eq!(
                url,
                if client == "papiXhr" {
                    "/papi/v1/um/leverage"
                } else {
                    "/fapi/v1/leverage"
                },
                "LEVERAGE native endpoint case {index}"
            );
            assert_eq!(
                body.symbol, "BTCUSDT",
                "LEVERAGE native request symbol case {index}"
            );
            assert_number(
                body.leverage,
                clamped,
                &format!("LEVERAGE native clamp {client} case {index}"),
            );
        }
        let did_update = should_post && !reject;
        assert_eq!(
            notifications.borrow().len(),
            usize::from(did_update),
            "LEVERAGE native notification count case {index}"
        );
        let native = store.positions();
        assert_eq!(
            native.len(),
            if positions { 3 } else { 0 },
            "LEVERAGE native position count case {index}"
        );
        if positions {
            for (row, (id, initial)) in [
                ("BTCUSDT:long", "5"),
                ("BTCUSDT:short", "11"),
                ("ETHUSDT:long", "13"),
            ]
            .into_iter()
            .enumerate()
            {
                assert_eq!(
                    native[row].id, id,
                    "LEVERAGE native position ID case {index}"
                );
                let expected = if did_update && row < 2 {
                    clamped
                } else {
                    initial
                };
                assert_number(
                    native[row].leverage,
                    expected,
                    &format!("LEVERAGE native position {client} case {index} row {row}"),
                );
                if did_update {
                    assert_number(
                        notifications.borrow()[0][row],
                        expected,
                        "LEVERAGE native notification value",
                    );
                }
            }
        }
        let error = outcome.err();
        let expected_error = &c["expected"]["error"];
        assert_eq!(
            error.is_some(),
            !expected_error.is_null(),
            "LEVERAGE native error presence case {index}"
        );
        if let Some(error) = &error {
            assert_eq!(
                error.name,
                expected_error["name"].as_str().unwrap(),
                "LEVERAGE native error name"
            );
            assert_eq!(
                error.message,
                expected_error["message"].as_str().unwrap(),
                "LEVERAGE native error message"
            );
            assert_eq!(
                observed_error_code(&error.code)
                    .as_ref()
                    .unwrap_or(&Value::Null),
                &expected_error["code"],
                "LEVERAGE native error code"
            );
            assert_eq!(
                *error == emitter_error,
                expected_error["same"].as_bool().unwrap(),
                "LEVERAGE native emitter error identity"
            );
        }
        assert_eq!(
            trace.borrow().as_slice(),
            c["expected"]["trace"].as_array().unwrap().as_slice(),
            "LEVERAGE source effect order case {index}"
        );
        let projected: Vec<_> = native
            .iter()
            .map(|p| json!({"id":p.id,"leverage":number_text(p.leverage)}))
            .collect();
        assert_eq!(
            json!(projected),
            c["expected"]["positions"],
            "LEVERAGE source position view case {index}"
        );
        unsubscribe();
        completed += 1;
    }
    assert_eq!(completed, 138, "LEVERAGE completed comparison cardinality");
}

#[test]
fn pending_leverage_post_cannot_commit() {
    for pm in [false, true] {
        let store = fixture(true, true, 1.0);
        let calls = Cell::new(0);
        let post = |_: &'static str, _: BinanceLeverageRequest| {
            calls.set(calls.get() + 1);
            std::future::pending::<Result<(), ExchangeError>>()
        };
        let emit = |_: Value| -> Result<(), ExchangeError> { panic!("pending POST cannot emit") };
        let mut future: std::pin::Pin<Box<dyn Future<Output = Result<(), ExchangeError>> + '_>> =
            if pm {
                Box::pin(set_binance_pm_leverage(&store, "BTCUSDT", 7.0, post, emit))
            } else {
                Box::pin(set_binance_leverage(&store, "BTCUSDT", 7.0, post, emit))
            };
        for _ in 0..2 {
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert_eq!(calls.get(), 1, "LEVERAGE one pending POST");
            assert_eq!(
                store.positions()[0].leverage,
                5.0,
                "LEVERAGE no mutation before POST success"
            );
            assert_eq!(store.positions()[1].leverage, 11.0);
        }
        drop(future);
        assert_eq!(store.positions()[0].leverage, 5.0);
    }
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn inherited_market_guard_matches_live_source() {
    let out = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/leverage.mjs"))
        .arg("--prototype")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 4, "LEVERAGE prototype source cardinality");
    for (index, c) in cases.iter().enumerate() {
        let pm = index >= 2;
        let present = index % 2 == 1;
        assert_eq!(c["variant"], if pm { "binancepm" } else { "binance" });
        assert_eq!(c["present"], present);
        let store = fixture(false, false, 1.0);
        if present {
            let row:Position=serde_json::from_value(json!({"id":"constructor:long","symbol":"constructor","side":"long","entryPrice":1,"notional":2,"leverage":5,"unrealizedPnl":4,"contracts":1,"liquidationPrice":6})).unwrap();
            Store::update(
                &store,
                StoreUpdate {
                    positions: Some(vec![row]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let post = |_: &'static str,
                    _: BinanceLeverageRequest|
         -> std::future::Ready<Result<(), ExchangeError>> {
            panic!("prototype guard must precede POST")
        };
        let emit = |_: Value| -> Result<(), ExchangeError> {
            panic!("prototype guard errors must bypass emission")
        };
        let error = if pm {
            settle(set_binance_pm_leverage(
                &store,
                "constructor",
                7.0,
                post,
                emit,
            ))
        } else {
            settle(set_binance_leverage(&store, "constructor", 7.0, post, emit))
        }
        .unwrap_err();
        assert_eq!(
            error.name,
            if present { "TypeError" } else { "Error" },
            "LEVERAGE inherited market native name case {index}"
        );
        assert_eq!(
            error.message,
            if present {
                "Cannot read properties of undefined (reading 'leverage')"
            } else {
                "Position constructor not found"
            },
            "LEVERAGE inherited market native message case {index}"
        );
        assert_eq!(error.name, c["error"]["name"].as_str().unwrap());
        assert_eq!(error.message, c["error"]["message"].as_str().unwrap());
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
