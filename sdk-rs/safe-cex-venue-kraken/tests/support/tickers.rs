use super::*;
use safe_cex_types::Ticker;
use safe_cex_venue_kraken::tickers;
const FIELDS: [&str; 11] = [
    "bid",
    "ask",
    "last",
    "mark",
    "index",
    "percentage",
    "openInterest",
    "fundingRate",
    "fundingTime",
    "volume",
    "quoteVolume",
];
fn fields(t: &Ticker) -> [f64; 11] {
    [
        t.bid,
        t.ask,
        t.last,
        t.mark,
        t.index,
        t.percentage,
        t.open_interest,
        t.funding_rate,
        t.funding_time,
        t.volume,
        t.quote_volume,
    ]
}
fn snapshot(t: &Ticker) -> Value {
    assert!(t.timestamp.is_none(), "TICKER native absent timestamp");
    let mut value = json!({"id":t.id,"symbol":t.symbol});
    if let Some(info) = t.info.as_ref() {
        value["info"] = capture(info);
    }
    for (index, field) in FIELDS.into_iter().enumerate() {
        value[field] = number(fields(t)[index]);
    }
    value
}
fn fallback(tag: &str, n: f64) -> Ticker {
    Ticker {
        id: format!("id-{tag}"),
        symbol: format!("symbol-{tag}"),
        bid: n + 1.0,
        ask: n + 2.0,
        last: n + 3.0,
        mark: n + 4.0,
        index: n + 5.0,
        percentage: n + 6.0,
        open_interest: n + 7.0,
        funding_rate: n + 8.0,
        funding_time: n + 9.0,
        volume: n + 10.0,
        quote_volume: n + 11.0,
        mid_price: None,
        timestamp: None,
        info: Some(json!({"tag":tag})),
    }
}
fn old() -> Vec<Ticker> {
    vec![fallback("old-A", 101.25), fallback("old-B", 211.5)]
}
fn later() -> Vec<Ticker> {
    vec![fallback("new-A", 307.75), fallback("new-B", 419.125)]
}
#[test]
fn futures_tickers_live_oracle() {
    let source = oracle();
    let cases = source["futuresTickers"].as_array().unwrap();
    assert_eq!(cases.len(), 49, "TICKER source case total");
    let census: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
    let counts = census["reads"]["outputs"]["futuresTickers"]
        .as_array()
        .unwrap();
    assert_eq!(counts.len(), 49, "TICKER independent case count");
    let mut comparisons = 0;
    let mut field_comparisons = 0;
    let mut native_records = 0;
    for index in 0..49 {
        let case = &cases[index];
        let store = DefaultStore::new();
        let markets: Vec<Market> = serde_json::from_value(case["markets"].clone()).unwrap();
        Store::update(
            &store,
            StoreUpdate {
                markets: Some(markets.clone()),
                tickers: Some(old()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let calls = RefCell::new(vec![]);
        let traces = RefCell::new(vec![]);
        let mut read_error = ExchangeError::new("ticker read failure");
        read_error.metadata.code = Some(safe_cex_exchange::js_runtime::value::Js::str(
            "READ-TICKERS",
        ));
        read_error.metadata.store_subscriber_ids = vec![151, 157];
        let read_pointer = read_error.metadata.as_ref() as *const _;
        let mut read_error = Some(read_error);
        let mut trace_error = ExchangeError::new("trace failure");
        trace_error.metadata.store_subscriber_ids = vec![163, 167];
        let trace_pointer = trace_error.metadata.as_ref() as *const _;
        let released = Cell::new(false);
        let future = tickers::fetch_tickers(
            &store,
            |endpoint| {
                calls.borrow_mut().push(json!(endpoint));
                if case["mode"] == "sync" {
                    return Err(read_error.take().unwrap());
                }
                let mut received = Some(if case["mode"] == "async" {
                    Err(read_error.take().unwrap())
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str(raw).unwrap()))
                });
                let released = &released;
                Ok(poll_fn(move |_| {
                    if released.get() {
                        Poll::Ready(received.take().unwrap())
                    } else {
                        Poll::Pending
                    }
                }))
            },
            |error| {
                let same = std::ptr::eq(error.metadata.as_ref(), read_pointer);
                traces.borrow_mut().push(json!({"name":error.name,"message":error.message,"sameError":same,"code":error.metadata.code.as_ref().map(safe_cex_exchange::js_runtime::value::to_json).transpose().expect("finite fixture error code")}));
                if case["mode"] == "sync" || case["mode"] == "async" {
                    assert_eq!(
                        error.metadata.store_subscriber_ids,
                        vec![151, 157],
                        "TICKER traced subscriber identity"
                    );
                }
                if case["replaceFallback"] == true {
                    Store::update(
                        &store,
                        StoreUpdate {
                            tickers: Some(later()),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                }
                if case["traceFail"] == true {
                    Err(trace_error)
                } else {
                    Ok(())
                }
            },
        );
        let mut future = std::pin::pin!(future);
        let first = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()));
        assert_eq!(calls.borrow().len(), 1, "TICKER native request prefix");
        assert_eq!(case["prefixCalls"], 1, "TICKER source request prefix");
        let result = match first {
            Poll::Ready(result) => {
                assert_eq!(
                    case["mode"], "sync",
                    "TICKER early completion only sync failure"
                );
                result
            }
            Poll::Pending => {
                if case["lateMarkets"] == true {
                    let mut markets = markets;
                    for market in &mut markets {
                        market.id = format!("late-{}", market.id);
                    }
                    Store::update(
                        &store,
                        StoreUpdate {
                            markets: Some(markets),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                }
                released.set(true);
                complete(future.as_mut())
            }
        };
        assert_eq!(
            *calls.borrow(),
            case["calls"].as_array().unwrap().clone(),
            "TICKER native request endpoint"
        );
        assert_eq!(
            *traces.borrow(),
            case["traces"].as_array().unwrap().clone(),
            "TICKER native trace arguments"
        );
        assert_eq!(
            store.tickers().iter().map(snapshot).collect::<Vec<_>>(),
            case["storeAfter"].as_array().unwrap().clone(),
            "TICKER post-trace Store state"
        );
        if let Some(count) = counts[index].as_u64() {
            let tickers = result.expect("TICKER expected success");
            native_records += tickers.len();
            assert_eq!(
                tickers.len(),
                count as usize,
                "TICKER independent native length"
            );
            cardinality::native_len(
                tickers.len(),
                &case["result"],
                "TICKER source/native length",
            );
            if !traces.borrow().is_empty() {
                assert_eq!(case["sameFallback"], true, "TICKER source fallback alias");
                assert_ne!(
                    tickers.as_ptr(),
                    store.tickers().as_ptr(),
                    "TICKER CHOSEN owned fallback snapshot"
                );
            }
            for row in 0..count as usize {
                let ticker = tickers.get(row).expect("TICKER native index coverage");
                let expected = &case["result"][row];
                assert_eq!(
                    expected.as_object().unwrap().len(),
                    if traces.borrow().is_empty() { 14 } else { 13 },
                    "TICKER source field inventory count"
                );
                assert_eq!(
                    ticker.id,
                    expected["id"].as_str().unwrap(),
                    "TICKER native id"
                );
                assert_eq!(
                    ticker.symbol,
                    expected["symbol"].as_str().unwrap(),
                    "TICKER native symbol"
                );
                assert!(ticker.mid_price.is_none(), "TICKER native midPrice absent");
                assert_eq!(
                    ticker.info.as_ref().map(capture),
                    expected.get("info").cloned(),
                    "TICKER native info"
                );
                for (field_index, field) in FIELDS.into_iter().enumerate() {
                    assert!(
                        expected.get(field).is_some(),
                        "TICKER source field inventory"
                    );
                    assert_eq!(
                        number(fields(ticker)[field_index]),
                        expected[field],
                        "TICKER native {field} case {index} row {row}"
                    );
                    field_comparisons += 1;
                }
            }
        } else {
            let error = result.expect_err("TICKER expected trace rejection");
            assert_eq!(
                error.message,
                case["error"].as_str().unwrap(),
                "TICKER trace failure message"
            );
            assert!(
                std::ptr::eq(error.metadata.as_ref(), trace_pointer),
                "TICKER trace failure allocation"
            );
            assert_eq!(
                error.metadata.store_subscriber_ids,
                vec![163, 167],
                "TICKER trace failure subscriber identity"
            );
            assert_eq!(
                case["sameError"], true,
                "TICKER source trace error identity"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 49, "TICKER completed comparisons");
    assert_eq!(native_records, 63, "TICKER native total records");
    assert_eq!(field_comparisons, 693, "TICKER completed numeric fields");
}
#[test]
fn futures_tickers_chosen_market_boundary() {
    let case = &oracle()["tickerBoundary"];
    assert_eq!(
        case["result"].as_array().unwrap().len(),
        1,
        "TICKER source inherited pseudo-market"
    );
    assert_eq!(
        case["result"][0]["id"],
        json!({"undefined":true}),
        "TICKER source untyped inherited id"
    );
    assert_eq!(
        case["result"][0]["symbol"],
        json!({"undefined":true}),
        "TICKER source untyped inherited symbol"
    );
    let store = DefaultStore::new();
    let result = complete(tickers::fetch_tickers(
        &store,
        |_| {
            Ok(ready(Ok(Some(
                serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
            ))))
        },
        |_| panic!("unexpected boundary trace"),
    ))
    .unwrap();
    assert!(
        result.is_empty(),
        "TICKER CHOSEN typed Store omits pseudo-market"
    );
}
