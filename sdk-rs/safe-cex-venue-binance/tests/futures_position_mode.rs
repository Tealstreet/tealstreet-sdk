use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::Position;
use safe_cex_venue_binance::futures_position_mode::change_binance_position_mode;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    task::{Context, Poll, Waker},
};

fn failure(data: &Value, phase: &str) -> ExchangeError {
    let mut error = ExchangeError::new(format!("{phase} failed"));
    error = error.with_retained_response((json!({"data":data})).into());
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
        phase
    )));
    error
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn futures_position_mode_preserves_confirmation_error_precedence() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/futures-position-mode.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 2304);
    for (index, case) in cases.iter().enumerate() {
        let store = DefaultStore::new();
        let positions = case["positions"].as_array().unwrap().iter().enumerate().map(|(id, value)| {
            let mut position: Position = serde_json::from_value(json!({"id":id.to_string(),"symbol":"BTCUSDT","side":"long","entryPrice":1,"notional":2,"leverage":3,"unrealizedPnl":4,"contracts":0,"liquidationPrice":6})).unwrap();
            position.contracts = value.as_f64().unwrap_or_else(|| safe_cex_num::number_from_string(value.as_str().unwrap()));
            position
        }).collect();
        let () = Store::update(
            &store,
            StoreUpdate {
                positions: Some(positions),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation succeeds");
        let trace = RefCell::new(vec![]);
        let applied = Cell::new(0);
        let mut emitter_error = ExchangeError::new("emitter failed");
        emitter_error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emitter-code"
        )));
        let outcome = {
            let mut future = std::pin::pin!(change_binance_position_mode(
                &store,
                case["hedged"].as_bool().unwrap(),
                case["symbol"].as_str(),
                |url, body| {
                    trace
                        .borrow_mut()
                        .push(json!(["post", url, body["dualSidePosition"]]));
                    std::future::ready(if case["requestFailure"].is_null() {
                        Ok(())
                    } else {
                        Err(failure(&case["requestFailure"], "request"))
                    })
                },
                |hedged, symbol| {
                    trace.borrow_mut().push(json!(["confirm", hedged, symbol]));
                    let next = applied.get();
                    applied.set(next + 1);
                    match case["confirmations"].as_array().unwrap().get(next) {
                        Some(data) => Err(failure(data, "confirmation")),
                        None => Ok(()),
                    }
                },
                |message| {
                    trace.borrow_mut().push(json!(["emit", message]));
                    if case["emitterThrows"] == true {
                        Err(emitter_error.clone())
                    } else {
                        Ok(())
                    }
                }
            ));
            match future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                Poll::Ready(result) => result,
                Poll::Pending => panic!("ready POST must settle"),
            }
        };
        let error = outcome.err().map(|error| json!({"name":error.name,"message":error.message,"code":observed_error_code(&error.code),"same":error==emitter_error}));
        assert_eq!(
            json!({"trace":*trace.borrow(),"error":error}),
            case["expected"],
            "Futures position mode callback effects case {index}"
        );
    }
}

#[test]
fn futures_position_mode_waits_for_one_post_before_confirmation() {
    let store = DefaultStore::new();
    let sends = Cell::new(0);
    let confirmed = Cell::new(0);
    let complete = Cell::new(false);
    let mut future = Box::pin(change_binance_position_mode(
        &store,
        true,
        Some(""),
        |_, _| {
            sends.set(sends.get() + 1);
            std::future::poll_fn(|_| {
                if complete.get() {
                    Poll::Ready(Ok(()))
                } else {
                    Poll::Pending
                }
            })
        },
        |hedged, symbol| {
            assert!(hedged);
            assert_eq!(symbol, Some(""));
            confirmed.set(confirmed.get() + 1);
            Ok(())
        },
        |_| panic!("successful POST must not emit"),
    ));
    for _ in 0..2 {
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(confirmed.get(), 0, "no confirmation before POST settlement");
    }
    complete.set(true);
    assert!(matches!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(()))
    ));
    assert_eq!(sends.get(), 1, "one POST through repeated polls");
    assert_eq!(confirmed.get(), 1, "one confirmed mode after success");
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
