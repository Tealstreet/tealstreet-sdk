use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{Balance, Ticker};
use safe_cex_venue_kraken::spot_balance;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    process::Command,
    task::{Context, Poll, Waker},
};
fn complete<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(x) => x,
        Poll::Pending => panic!("ready read"),
    }
}
fn number(v: f64) -> Value {
    json!({"number":format!("{:016x}",if v.is_nan(){0x7ff8000000000000}else{v.to_bits()})})
}
#[path = "../../parity/evidence/retained_model_values.rs"]
pub mod retained_model_values;
fn capture(value: &(impl serde::Serialize + ?Sized)) -> Value {
    retained_model_values::observe(value, number)
}

fn fallback(n: f64) -> Balance {
    Balance {
        free: n + 1.0,
        used: n + 2.0,
        total: n + 3.0,
        upnl: n + 4.0,
        coins: None,
        info: Some(json!({"tag":"fallback"}).into()),
        currencies: Some(
            [(
                String::from("OLD"),
                Balance {
                    free: n + 5.0,
                    used: n + 6.0,
                    total: n + 7.0,
                    upnl: n + 8.0,
                    coins: None,
                    currencies: None,
                    info: Some(json!({"tag":"old"}).into()),
                }
                .into(),
            )]
            .into_iter()
            .collect(),
        ),
    }
}
fn observe(balance: &Balance) -> Value {
    let mut v = json!({"free":number(balance.free),"used":number(balance.used),"total":number(balance.total),"upnl":number(balance.upnl)});
    if let Some(info) = &balance.info {
        v["info"] = capture(info);
    }
    if let Some(currencies) = &balance.currencies {
        v["currencies"] = Value::Object(
            currencies
                .iter()
                .map(|(k, v)| (k.clone(), observe(v.as_balance().unwrap())))
                .collect(),
        );
    }
    v
}
fn ticker(symbol: &str, last: f64) -> Ticker {
    Ticker {
        id: symbol.into(),
        symbol: symbol.into(),
        bid: 0.0,
        ask: 0.0,
        last,
        mark: 0.0,
        index: Some(0.0),
        mid_price: None,
        timestamp: None,
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        info: None,
    }
}
#[test]
fn spot_balance_live_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-balance-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        source["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "SBALANCE node major"
    );
    assert!(
        !source["header"]["arch"].as_str().unwrap().is_empty(),
        "SBALANCE architecture"
    );
    let cases = source["spotBalances"].as_array().unwrap();
    assert_eq!(cases.len(), 29, "SBALANCE source cases");
    let counts = [
        Some(5),
        Some(5),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(2),
        Some(2),
        Some(2),
        Some(2),
        Some(2),
        Some(2),
        Some(2),
        Some(1),
        Some(0),
        Some(5),
        Some(5),
        Some(5),
        Some(1),
        None,
        Some(1),
        None,
        Some(1),
        None,
        Some(1),
        None,
        Some(5),
    ];
    let mut completed = 0;
    let mut currencies_compared = 0;
    for i in 0..29 {
        let case = &cases[i];
        let store = DefaultStore::new();
        let last = |n| {
            case["last"]
                .as_f64()
                .unwrap_or(if case["last"] == "NaN" { f64::NAN } else { n })
        };
        let mut tickers = vec![
            ticker("BTC/USDT", last(19.25)),
            ticker("BTC/USD", last(23.5)),
            ticker("ETH/USD", last(29.75)),
        ];
        if case["reverse"] == true {
            tickers.reverse();
        }
        Store::update(
            &store,
            StoreUpdate {
                balance: Some(fallback(101.25)),
                tickers: Some(tickers),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let calls = RefCell::new(vec![]);
        let emits = RefCell::new(vec![]);
        let mut emitter = ExchangeError::new("balance emit failure");
        emitter.metadata.store_subscriber_ids = vec![251, 257];
        let pointer = emitter.metadata.as_ref() as *const _;
        let released = Cell::new(false);
        let future = spot_balance::fetch_balance(
            &store,
            |endpoint| {
                calls
                    .borrow_mut()
                    .push(json!({"endpoint":endpoint,"body":{}}));
                if case["mode"] == "sync" {
                    return Err(ExchangeError::new("balance read failure"));
                }
                let mut response = Some(if case["mode"] == "async" {
                    Err(ExchangeError::new("balance read failure"))
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
            },
            |message| {
                emits.borrow_mut().push(json!(message));
                if case["replace"] == true {
                    Store::update(
                        &store,
                        StoreUpdate {
                            balance: Some(fallback(211.5)),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                }
                if case["emitFail"] == true {
                    Err(emitter)
                } else {
                    Ok(())
                }
            },
        );
        let mut future = std::pin::pin!(future);
        let first = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        if case["late"] == true {
            Store::update(
                &store,
                StoreUpdate {
                    tickers: Some(vec![
                        ticker("BTC/USDT", 41.25),
                        ticker("BTC/USD", 41.25),
                        ticker("ETH/USD", 41.25),
                    ]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        released.set(true);
        let result = match first {
            Poll::Ready(x) => x,
            Poll::Pending => complete(future.as_mut()),
        };
        assert_eq!(
            *calls.borrow(),
            *case["calls"].as_array().unwrap(),
            "SBALANCE native requests"
        );
        assert_eq!(
            *emits.borrow(),
            *case["emits"].as_array().unwrap(),
            "SBALANCE native emits"
        );
        assert_eq!(
            observe(&store.balance()),
            case["storeAfter"],
            "SBALANCE post-emission Store"
        );
        if let Some(count) = counts[i] {
            let mut balance = result.unwrap();
            let expected = &case["result"];
            assert_eq!(
                expected["currencies"].as_object().unwrap().len(),
                count,
                "SBALANCE source currency count"
            );
            let native = balance
                .currencies
                .as_ref()
                .expect("SBALANCE native currencies");
            assert_eq!(native.len(), count, "SBALANCE independent native count");
            assert_eq!(
                native.len(),
                expected["currencies"].as_object().unwrap().len(),
                "SBALANCE source/native count"
            );
            assert_eq!(
                observe(&balance),
                *expected,
                "SBALANCE direct native balance"
            );
            for index in 0..count {
                let (key, currency) = native
                    .get_index(index)
                    .expect("SBALANCE native currency index");
                let (source_key, source_currency) = expected["currencies"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .nth(index)
                    .unwrap();
                assert_eq!(key, source_key, "SBALANCE native currency order");
                assert_eq!(
                    observe(currency.as_balance().unwrap()),
                    *source_currency,
                    "SBALANCE native currency"
                );
                currencies_compared += 1;
            }
            assert!(balance.coins.is_none(), "SBALANCE native absent coins");
            if !emits.borrow().is_empty() {
                assert_eq!(case["sameFallback"], true, "SBALANCE source fallback alias");
                let before = store.balance().free;
                balance.free += 997.25;
                assert_eq!(
                    number(store.balance().free),
                    number(before),
                    "SBALANCE CHOSEN native snapshot independent"
                );
            }
        } else {
            let e = result.unwrap_err();
            assert_eq!(
                e.message,
                case["error"].as_str().unwrap(),
                "SBALANCE emitter message"
            );
            assert!(
                std::ptr::eq(e.metadata.as_ref(), pointer),
                "SBALANCE emitter allocation"
            );
            assert_eq!(
                e.metadata.store_subscriber_ids,
                vec![251, 257],
                "SBALANCE emitter subscribers"
            );
            assert_eq!(case["sameError"], true, "SBALANCE source emitter identity");
        }
        completed += 1;
    }
    let boundary = &source["spotBalanceBoundary"];
    assert_eq!(
        boundary["keys"].as_array().unwrap().len(),
        2,
        "SBALANCE source UTF16 unit acceptance"
    );
    assert_eq!(
        boundary["keys"],
        json!(["0", "1"]),
        "SBALANCE source UTF16 keys"
    );
    assert_eq!(
        boundary["units"],
        json!([[55357], [56832]]),
        "SBALANCE source UTF16 units"
    );
    let store = DefaultStore::new();
    Store::update(
        &store,
        StoreUpdate {
            balance: Some(fallback(101.25)),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let reports = RefCell::new(vec![]);
    let result = complete(spot_balance::fetch_balance(
        &store,
        |_| {
            Ok(std::future::ready(Ok(Some(
                serde_json::from_str(boundary["raw"].as_str().unwrap()).unwrap(),
            ))))
        },
        |message| {
            reports.borrow_mut().push(message.to_owned());
            Ok(())
        },
    ))
    .unwrap();
    assert_eq!(
        *reports.borrow(),
        vec!["Kraken Spot balance UTF-16 unit is outside UTF-8 profile"],
        "SBALANCE CHOSEN UTF16 refusal"
    );
    assert_eq!(
        observe(&result),
        observe(&fallback(101.25)),
        "SBALANCE boundary native fallback"
    );
    assert_eq!(completed, 29, "SBALANCE completed cases");
    assert_eq!(currencies_compared, 49, "SBALANCE completed currencies");
}
