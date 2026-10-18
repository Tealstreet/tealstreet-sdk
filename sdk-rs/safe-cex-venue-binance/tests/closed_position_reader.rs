use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::*;
use safe_cex_venue_binance::closed_position_reader::derive_binance_closed_positions;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    task::{Context, Poll, Waker},
};

fn number(value: f64) -> Value {
    json!({"bits":format!("{:016x}", value.to_bits())})
}
fn normalize(entry: ClosedPnlEntry) -> Value {
    assert_eq!(
        entry.provenance,
        DataProvenance::Derived,
        "native closed PnL provenance"
    );
    macro_rules! numeric {
        ($field:ident, $ty:ident, $variant:ident) => {
            match entry.$field {
                $ty::$variant(value) => number(value),
                _ => panic!("expected numeric {}", stringify!($field)),
            }
        };
    }
    json!({
        "provenance":entry.provenance,"symbol":entry.symbol,"account":entry.account,
        "side":entry.side,"time":number(entry.time),"closedTime":number(entry.closed_time),
        "notional":numeric!(notional,ClosedPnlEntryNotional,Variant1),
        "contracts":numeric!(contracts,ClosedPnlEntryContracts,Variant1),
        "entryPrice":numeric!(entry_price,ClosedPnlEntryEntryPrice,Variant1),
        "exitPrice":numeric!(exit_price,ClosedPnlEntryExitPrice,Variant1),
        "unrealizedPnl":numeric!(unrealized_pnl,ClosedPnlEntryUnrealizedPnl,Variant1),
        "fees":numeric!(fees,ClosedPnlEntryFees,Variant1),
        "leverage":numeric!(leverage,ClosedPnlEntryLeverage,Variant0)
    })
}

fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture pending"),
    }
}
fn norm(value: &Value) -> Value {
    match value {
        Value::Number(n) => number(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(norm).collect()),
        Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), norm(v))).collect()),
        _ => value.clone(),
    }
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn closed_position_reader_matches_source() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/closed-position-reader.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 658);
    for (index, case) in cases.iter().enumerate() {
        let mut opts: HistoricalTradeOptions =
            serde_json::from_value(case["opts"].clone()).unwrap();
        if let Some(special) = case.get("special") {
            let value = special["value"].as_str().unwrap().parse::<f64>().unwrap();
            if special["bound"] == "from" {
                opts.from = Some(value);
            } else {
                opts.to = Some(value);
            }
        }
        let trace = RefCell::new(vec![]);
        let calls = Cell::new(0);
        let clock = Cell::new(0);
        let mut emit_failure = ExchangeError::new("fixture emitter");
        emit_failure.name = "SyntaxError".into();
        emit_failure.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "emit-code"
        )));
        let result = run(derive_binance_closed_positions(
            &opts,
            case["publicOnly"] == true,
            || {
                clock.set(clock.get() + 1);
                3000000000.0
            },
            |endpoint, params| {
                let keys: Vec<_> = params.as_object().unwrap().keys().cloned().collect();
                trace
                    .borrow_mut()
                    .push(json!({"get":endpoint,"params":norm(&params),"keys":keys}));
                let call = calls.get();
                calls.set(call + 1);
                std::future::ready(if case["reject"].as_i64() == Some(call) {
                    let mut error = ExchangeError::new("fixture request");
                    error.name = "RangeError".into();
                    error =
                        error.with_retained_response((json!({"data":{"msg":case["msg"]}})).into());
                    Err(error)
                } else {
                    Ok(case.get("page").cloned())
                })
            },
            |message| {
                trace.borrow_mut().push(json!({"emit":message}));
                if case["emitThrows"] == true {
                    Err(emit_failure.clone())
                } else {
                    Ok(())
                }
            },
        ));
        let (result, error) = match result {
            Ok(entries) => (
                Value::Array(entries.into_iter().map(normalize).collect()),
                Value::Null,
            ),
            Err(error) => (
                Value::Null,
                json!({"name":error.name,"message":error.message,"code":observed_error_code(&error.code),"same":error==emit_failure}),
            ),
        };
        assert_eq!(
            json!({"trace":trace.into_inner(),"result":result,"error":error,"clockCalls":clock.get()}),
            case["expected"],
            "closed reader source case {index}"
        );
    }
}
#[test]
fn pending_get_is_sequential_and_drop_stops_next_page() {
    let opts: HistoricalTradeOptions =
        serde_json::from_value(json!({"symbol":"BTCUSDT","from":0,"to":1209600002})).unwrap();
    let calls = Cell::new(0);
    let settled = Cell::new(false);
    let dropped = Cell::new(0);
    struct OnDrop<'a>(&'a Cell<usize>);
    impl Drop for OnDrop<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let mut future = Box::pin(derive_binance_closed_positions(
        &opts,
        false,
        || panic!("explicit to must not sample clock"),
        |_, _| {
            calls.set(calls.get() + 1);
            let guard = OnDrop(&dropped);
            let settled = &settled;
            async move {
                let _guard = guard;
                std::future::poll_fn(|_| {
                    if settled.get() {
                        Poll::Ready(Ok(Some(json!([]))))
                    } else {
                        Poll::Pending
                    }
                })
                .await
            }
        },
        |_| panic!("no error"),
    ));
    let mut context = Context::from_waker(Waker::noop());
    assert_eq!(calls.get(), 0);
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(calls.get(), 1);
    assert!(future.as_mut().poll(&mut context).is_pending());
    assert_eq!(calls.get(), 1);
    drop(future);
    assert_eq!(dropped.get(), 1);
    assert_eq!(calls.get(), 1);
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn closed_reader_oracle_controls_fail_loudly() {
    let script = format!(
        "{}/scripts/closed-position-reader.mjs",
        env!("CARGO_MANIFEST_DIR")
    );
    let output = std::process::Command::new("node")
        .arg(&script)
        .env("BINANCE_READER_CONTROL", "skip-get")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("default thirty-day request count"));
    let output = std::process::Command::new("node")
        .arg(script)
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            "/__binance_missing_compiler__/typescript",
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
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
