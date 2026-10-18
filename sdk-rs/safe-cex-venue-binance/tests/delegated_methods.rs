use safe_cex_exchange::{journal::HistoricalPositionOptions, runtime::ExchangeError};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, Timeframe};
use safe_cex_venue_binance::delegated_methods::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Failure = Rc<ExchangeError>;
type Cleanup = Rc<dyn Fn() -> Result<(), Failure>>;
type Callback = Rc<dyn Fn(Rc<RefCell<Value>>) -> Result<(), Failure>>;
fn run<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("unexpected pending fixture"),
    }
}
fn oracle() -> Vec<Value> {
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/delegated-methods.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(rows.len(), 27, "independent source floor 8+3+4+4+4+4");
    rows
}
fn listener(variant: &str, method: &str, count: usize) {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|r| r["variant"] == variant && r["method"] == method)
    {
        let candle = method == "listenOHLCV";
        let mode = case["mode"].as_str().unwrap();
        let external = case["external"] == true;
        let opts: CandleOptionsHandle = Rc::new(RefCell::new(
            serde_json::from_value::<OHLCVOptions>(
                json!({"symbol":"BTCUSDT","interval":"1m","from":100,"to":200,"limit":7}),
            )
            .unwrap(),
        ));
        let payload = Rc::new(RefCell::new(if candle {
            json!({"timestamp":1000,"open":11,"high":22,"low":3,"close":12.3,"volume":3.6})
        } else {
            json!({"bids":[{"price":12.3,"amount":3.6,"total":3.6}],"asks":[{"price":12.4,"amount":7,"total":7}]})
        }));
        let errors: [Failure; 3] = ["listen", "callback", "cleanup"]
            .map(|role| Rc::new(ExchangeError::new(format!("{role} failure"))));
        let calls = Cell::new(0);
        let callbacks = Rc::new(Cell::new(0));
        let cleanups = Rc::new(Cell::new(0));
        let seen = RefCell::new(None);
        let same_options = Cell::new(false);
        let same_callback = Cell::new(false);
        let n = callbacks.clone();
        let p = payload.clone();
        let e = errors[1].clone();
        let reject_callback = mode == "callback-reject";
        let callback: Callback = Rc::new(move |arg| {
            n.set(n.get() + 1);
            assert!(Rc::ptr_eq(&arg, &p), "DELEGATE native payload identity");
            if candle {
                arg.borrow_mut()["close"] = json!(20);
            } else {
                arg.borrow_mut()["bids"][0]["price"] = json!(18);
            }
            if reject_callback {
                Err(e.clone())
            } else {
                Ok(())
            }
        });
        let n = cleanups.clone();
        let e = errors[2].clone();
        let reject_cleanup = mode == "cleanup-reject";
        let cleanup: Cleanup = Rc::new(move || {
            n.set(n.get() + 1);
            if reject_cleanup {
                Err(e.clone())
            } else {
                Ok(())
            }
        });
        let listen = |role: &'static str, symbol: &str, cb: Callback| -> Result<Cleanup, Failure> {
            calls.set(calls.get() + 1);
            *seen.borrow_mut() = Some((role, symbol.to_string()));
            same_callback.set(Rc::ptr_eq(&cb, &callback));
            if mode == "listen-reject" {
                return Err(errors[0].clone());
            }
            cb(payload.clone())?;
            Ok(cleanup.clone())
        };
        let result = if candle {
            let listen_candle = |arg: CandleOptionsHandle, cb: Callback| {
                same_options.set(Rc::ptr_eq(&arg, &opts));
                let symbol = arg.borrow().symbol.clone();
                if mode != "listen-reject" {
                    arg.borrow_mut().limit = Some(9.0);
                }
                listen("public", &symbol, cb)
            };
            if variant == "binancepm" {
                listen_binance_pm_ohlcv(opts.clone(), callback.clone(), listen_candle)
            } else {
                listen_binance_spot_ohlcv(opts.clone(), callback.clone(), listen_candle)
            }
        } else if variant == "binance" {
            listen_binance_order_book(
                "BTCUSDT",
                callback.clone(),
                external,
                |symbol, cb| listen("external", symbol, cb),
                |symbol, cb| listen("public", symbol, cb),
            )
        } else if variant == "binancepm" {
            listen_binance_pm_order_book("BTCUSDT", callback.clone(), |symbol, cb| {
                listen("public", symbol, cb)
            })
        } else {
            listen_binance_spot_order_book("BTCUSDT", callback.clone(), |symbol, cb| {
                listen("public", symbol, cb)
            })
        };
        let mut origin = match &result {
            Err(error) => Some(if Rc::ptr_eq(error, &errors[0]) {
                "listen"
            } else {
                assert!(
                    Rc::ptr_eq(error, &errors[1]),
                    "native callback error identity"
                );
                "callback"
            }),
            Ok(returned) => {
                assert!(
                    Rc::ptr_eq(returned, &cleanup),
                    "native returned cleanup identity"
                );
                None
            }
        };
        if let Ok(returned) = result {
            for _ in 0..2 {
                if let Err(error) = returned() {
                    assert!(
                        Rc::ptr_eq(&error, &errors[2]),
                        "native cleanup error identity"
                    );
                    origin = Some("cleanup");
                    break;
                }
            }
        }
        let expected_origin = match mode {
            "listen-reject" => Some("listen"),
            "callback-reject" => Some("callback"),
            "cleanup-reject" => Some("cleanup"),
            _ => None,
        };
        assert_eq!(
            origin, expected_origin,
            "DELEGATE {variant} {method} native original error role"
        );
        assert_eq!(calls.get(), 1, "native authored delegate call count");
        assert_eq!(
            callbacks.get(),
            usize::from(mode != "listen-reject"),
            "native authored callback count"
        );
        assert_eq!(
            cleanups.get(),
            match mode {
                "ok" => 2,
                "cleanup-reject" => 1,
                _ => 0,
            },
            "native repeatable cleanup count"
        );
        assert_eq!(
            seen.borrow().as_ref().unwrap().0,
            if external { "external" } else { "public" },
            "DELEGATE {variant} {method} native provider route"
        );
        assert_eq!(
            seen.borrow().as_ref().unwrap().1,
            "BTCUSDT",
            "DELEGATE {variant} {method} native symbol"
        );
        assert!(same_callback.get(), "native original callback identity");
        if candle {
            assert!(
                same_options.get(),
                "DELEGATE {variant} {method} native options identity"
            );
            let o = opts.borrow();
            assert_eq!(o.symbol, "BTCUSDT");
            assert_eq!(
                o.interval,
                safe_cex_types::AnyTimeframe::Variant1(Timeframe::M1)
            );
            assert_eq!(o.from, Some(100.0));
            assert_eq!(o.to, Some(200.0));
            assert_eq!(
                o.limit,
                Some(if mode == "listen-reject" { 7.0 } else { 9.0 }),
                "native option mutation visible to caller"
            );
            let decoded: Candle = serde_json::from_value(payload.borrow().clone()).unwrap();
            assert_eq!(decoded.timestamp, 1000.0);
            assert_eq!(decoded.open, 11.0);
            assert_eq!(decoded.high, 22.0);
            assert_eq!(decoded.low, 3.0);
            assert_eq!(
                decoded.close,
                if mode == "listen-reject" { 12.3 } else { 20.0 }
            );
            assert_eq!(decoded.volume, 3.6);
            assert_eq!(decoded.last_side, None);
        } else {
            let decoded: OrderBook = serde_json::from_value(payload.borrow().clone()).unwrap();
            assert_eq!(decoded.bids.len(), 1);
            assert_eq!(decoded.asks.len(), 1);
            assert_eq!(
                decoded.bids[0].price,
                if mode == "listen-reject" { 12.3 } else { 18.0 }
            );
            assert_eq!(decoded.bids[0].amount, 3.6);
            assert_eq!(decoded.bids[0].total, 3.6);
            assert_eq!(decoded.asks[0].price, 12.4);
            assert_eq!(decoded.asks[0].amount, 7.0);
            assert_eq!(decoded.asks[0].total, 7.0);
        }
        // All native assertions precede source comparisons; no re-encoding.
        assert_eq!(case["calls"], 1);
        assert_eq!(case["callbacks"], callbacks.get());
        assert_eq!(case["cleanups"], cleanups.get());
        assert_eq!(case["error"].as_str(), origin);
        assert_eq!(case["received"]["role"], seen.borrow().as_ref().unwrap().0);
        assert_eq!(case["received"]["symbol"], "BTCUSDT");
        assert_eq!(case["received"]["sameCallback"], true);
        if candle {
            assert_eq!(case["received"]["sameOptions"], true);
            assert_eq!(
                case["opts"]["limit"].as_f64().unwrap(),
                opts.borrow().limit.unwrap()
            );
        }
        assert_eq!(case["payload"], *payload.borrow());
        assert_eq!(
            case["samePayload"],
            if mode == "listen-reject" {
                Value::Null
            } else {
                json!(true)
            }
        );
        assert_eq!(
            case["sameCleanup"],
            if mode == "listen-reject" || mode == "callback-reject" {
                Value::Null
            } else {
                json!(true)
            }
        );
        completed += 1;
    }
    assert_eq!(completed, count, "fixed authored completed delegate cases");
}
#[test]
#[ignore = "requires source oracle"]
fn pm_candles() {
    listener("binancepm", "listenOHLCV", 4);
}
#[test]
#[ignore = "requires source oracle"]
fn spot_candles() {
    listener("binancespot", "listenOHLCV", 4);
}
#[test]
#[ignore = "requires source oracle"]
fn futures_book() {
    listener("binance", "listenOrderBook", 8);
}
#[test]
#[ignore = "requires source oracle"]
fn pm_book() {
    listener("binancepm", "listenOrderBook", 4);
}
#[test]
#[ignore = "requires source oracle"]
fn spot_book() {
    listener("binancespot", "listenOrderBook", 4);
}
#[test]
#[ignore = "requires source oracle"]
fn historical_positions() {
    let mut completed = 0;
    for case in oracle()
        .into_iter()
        .filter(|r| r["method"] == "fetchHistoricalPositions")
    {
        let mode = case["mode"].as_str().unwrap();
        let calls = Cell::new(0);
        let scoped = Cell::new(false);
        let opts = Rc::new(HistoricalPositionOptions {
            account: "account-A".into(),
            from: Some(100.0),
            to: Some(200.0),
        });
        let records = Rc::new(vec![
            json!({"symbol":"BTCUSDT","size":3.6}),
            json!({"symbol":"ETHUSDT","size":7}),
        ]);
        let error = Rc::new(ExchangeError::new("fold failure"));
        let result = run(fetch_binance_historical_positions(
            opts.clone(),
            |arg, flag| {
                calls.set(calls.get() + 1);
                scoped.set(flag);
                assert!(Rc::ptr_eq(&arg, &opts), "native fold options identity");
                assert_eq!(arg.account, "account-A");
                assert_eq!(arg.from, Some(100.0));
                assert_eq!(arg.to, Some(200.0));
                if mode == "sync-reject" {
                    Err(error.clone())
                } else {
                    Ok(std::future::ready(if mode == "reject" {
                        Err(error.clone())
                    } else {
                        Ok(records.clone())
                    }))
                }
            },
        ));
        assert_eq!(calls.get(), 1, "native fold authored call count");
        assert!(scoped.get(), "DELEGATE history native symbolScoped");
        match result {
            Ok(value) => {
                assert_eq!(mode, "ok");
                assert!(
                    Rc::ptr_eq(&value, &records),
                    "native result records identity"
                );
                assert_eq!(value.len(), 2);
                assert_eq!(value[0]["symbol"], "BTCUSDT");
                assert_eq!(value[0]["size"].as_f64().unwrap(), 3.6);
                assert_eq!(value[1]["symbol"], "ETHUSDT");
                assert_eq!(value[1]["size"].as_f64().unwrap(), 7.0);
            }
            Err(value) => assert!(
                Rc::ptr_eq(&value, &error),
                "native original fold error identity"
            ),
        }
        assert_eq!(case["calls"], 1);
        assert_eq!(case["received"]["sameOptions"], true);
        assert_eq!(case["received"]["symbolScoped"], true);
        assert_eq!(case["received"]["account"], "account-A");
        assert_eq!(case["received"]["from"], 100);
        assert_eq!(case["received"]["to"], 200);
        assert_eq!(
            case["error"].as_str(),
            if mode == "ok" { None } else { Some("fold") }
        );
        assert_eq!(case["sameResult"], mode == "ok");
        if mode == "ok" {
            assert_eq!(case["value"].as_array().unwrap().len(), 2);
            assert_eq!(case["value"].as_array().unwrap(), records.as_ref());
        }
        completed += 1;
    }
    assert_eq!(completed, 3, "fixed authored fold scenarios");
}
#[test]
fn historical_positions_waits_without_retry() {
    let calls = Cell::new(0);
    let ready = Cell::new(false);
    let result = Rc::new(vec![json!({"size":3.6})]);
    let mut future = std::pin::pin!(fetch_binance_historical_positions((), |_, scoped| {
        calls.set(calls.get() + 1);
        assert!(scoped);
        Ok::<_, Failure>(std::future::poll_fn(|_| {
            if ready.get() {
                Poll::Ready(Ok(result.clone()))
            } else {
                Poll::Pending
            }
        }))
    }));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    assert_eq!(calls.get(), 1);
    ready.set(true);
    let Poll::Ready(Ok(value)) = future.as_mut().poll(&mut cx) else {
        panic!("expected settled result")
    };
    assert!(Rc::ptr_eq(&value, &result));
    assert_eq!(value.len(), 1);
    assert_eq!(value[0]["size"].as_f64().unwrap(), 3.6);
}
