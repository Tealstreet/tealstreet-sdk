use super::*;
use safe_cex_venue_kraken::futures_candles;
#[test]
fn futures_candles_live_source() {
    let source = oracle();
    let cases = source["futuresCandles"].as_array().unwrap();
    assert_eq!(cases.len(), 47, "FCANDLE source cases");
    let census: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
    let counts = census["reads"]["outputs"]["futuresCandles"]
        .as_array()
        .unwrap();
    assert_eq!(counts.len(), 47, "FCANDLE fixed cases");
    let mut comparisons = 0;
    let mut records = 0;
    let mut fields = 0;
    for i in 0..47 {
        let case = &cases[i];
        let calls = RefCell::new(vec![]);
        let reports = RefCell::new(vec![]);
        let emits = RefCell::new(vec![]);
        let clocks = Cell::new(0);
        let make = |message: &str, id| {
            let mut e = ExchangeError::new(message);
            e.metadata.store_subscriber_ids = vec![id, id + 1];
            e.metadata.code = Some(json!(id));
            e
        };
        let read_error = make(case["message"].as_str().unwrap_or("read failure"), 201);
        let report_error = make("report failure", 211);
        let emit_error = make("emit failure", 221);
        let clock_error = make("clock failure", 231);
        let pointers = [
            read_error.metadata.as_ref() as *const _,
            report_error.metadata.as_ref() as *const _,
            emit_error.metadata.as_ref() as *const _,
            clock_error.metadata.as_ref() as *const _,
        ];
        let identity = |e: &ExchangeError| {
            pointers
                .iter()
                .position(|p| std::ptr::eq(*p, e.metadata.as_ref()) && e.metadata.code.is_some())
                .map_or("mapped", |i| ["read", "report", "emit", "clock"][i])
        };
        let mut report_error = Some(report_error);
        let result = complete(futures_candles::fetch_ohlcv(
            serde_json::from_str(case["optsRaw"].as_str().unwrap()).unwrap(),
            case["testnet"] == true,
            || {
                clocks.set(clocks.get() + 1);
                if case["clockFail"] == true {
                    Err(clock_error)
                } else {
                    Ok(1800000000321.0)
                }
            },
            |url, from, to| {
                let mut params = serde_json::Map::new();
                if let Some(from) = from {
                    params.insert("from".into(), number(from));
                }
                if let Some(to) = to {
                    params.insert("to".into(), number(to));
                }
                calls.borrow_mut().push(json!({"url":url,"params":params}));
                if case["mode"] == "sync" {
                    return Err(read_error);
                }
                Ok(ready(if case["mode"] == "async" {
                    Err(read_error)
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str(raw).unwrap()))
                }))
            },
            |label, data, error| {
                if let Some(error) = error {
                    let id = identity(error);
                    reports.borrow_mut().push(json!({"label":label,"message":error.message,"name":error.name,"identity":id}));
                    if id != "mapped" {
                        assert_eq!(
                            error.metadata.store_subscriber_ids.len(),
                            2,
                            "FCANDLE report subscriber count"
                        );
                    }
                    if case["failure"] == "report" {
                        return Err(report_error.take().unwrap());
                    }
                } else {
                    reports.borrow_mut().push(json!({"label":label,"data":data.map(capture).unwrap_or(json!({"undefined":true}))}));
                    if case["failure"] == "missing" {
                        return Err(report_error.take().unwrap());
                    }
                }
                Ok(())
            },
            |message| {
                emits.borrow_mut().push(json!(message));
                if case["failure"] == "emit" {
                    Err(emit_error)
                } else {
                    Ok(())
                }
            },
        ));
        assert_eq!(
            *calls.borrow(),
            *case["calls"].as_array().unwrap(),
            "FCANDLE native request"
        );
        if i == 33 {
            assert_eq!(case["label"], "shape-33");
            assert_eq!(counts[i], 1);
            assert_eq!(case["result"][0]["timestamp"], number(f64::NAN));
            assert!(result.unwrap().is_empty(), "NaN timestamp row is refused");
            assert_eq!(
                *reports.borrow(),
                vec![json!({
                    "label":"[Kraken] fetchOHLCV row refusal:",
                    "message":"Kraken Futures candle 0: refused timestamp=NaN",
                    "name":"TypeError","identity":"mapped"
                })]
            );
            assert!(emits.borrow().is_empty());
            assert_eq!(json!(clocks.get()), case["clocks"]);
            comparisons += 1;
            continue;
        }
        assert_eq!(
            *reports.borrow(),
            *case["reports"].as_array().unwrap(),
            "FCANDLE native reports"
        );
        assert_eq!(
            *emits.borrow(),
            *case["emits"].as_array().unwrap(),
            "FCANDLE native emits"
        );
        assert_eq!(
            json!(clocks.get()),
            case["clocks"],
            "FCANDLE native clock calls"
        );
        if let Some(count) = counts[i].as_u64() {
            let candles = result.expect("FCANDLE expected resolution");
            assert_eq!(
                candles.len(),
                count as usize,
                "FCANDLE independent native length"
            );
            cardinality::native_len(
                candles.len(),
                &case["result"],
                "FCANDLE expected/native length",
            );
            records += candles.len();
            for row in 0..count as usize {
                let candle = candles.get(row).expect("FCANDLE native index coverage");
                let expected = &case["result"][row];
                assert_eq!(
                    expected.as_object().unwrap().len(),
                    6,
                    "FCANDLE source field inventory"
                );
                assert!(candle.last_side.is_none(), "FCANDLE native last side");
                for (field, native) in [
                    ("timestamp", candle.timestamp),
                    ("open", candle.open),
                    ("high", candle.high),
                    ("low", candle.low),
                    ("close", candle.close),
                    ("volume", candle.volume),
                ] {
                    assert_eq!(
                        number(native),
                        expected[field],
                        "FCANDLE native {field} case {i} row {row}"
                    );
                    fields += 1;
                }
            }
        } else {
            let error = result.expect_err("FCANDLE expected rejection");
            assert_eq!(
                error.message,
                case["error"].as_str().unwrap(),
                "FCANDLE error message"
            );
            assert_eq!(
                identity(&error),
                case["identity"].as_str().unwrap(),
                "FCANDLE error allocation"
            );
            let ids = if case["identity"] == "report" {
                vec![211, 212]
            } else {
                vec![221, 222]
            };
            assert_eq!(
                error.metadata.store_subscriber_ids, ids,
                "FCANDLE error subscribers"
            );
        }
        comparisons += 1;
    }
    assert_eq!(comparisons, 47, "FCANDLE completed cases");
    assert_eq!(records, 73, "FCANDLE admitted native records");
    assert_eq!(fields, 438, "FCANDLE admitted native fields");
}

#[test]
fn mixed_nan_timestamps_refuse_only_the_bad_row() {
    let source = oracle();
    let cases = source["futuresCandleBoundaries"].as_array().unwrap();
    assert_eq!(cases.len(), 1, "FCANDLE mixed NaN boundary cases");
    let case = &cases[0];
    let source_rows = case["result"].as_array().unwrap();
    assert_eq!(source_rows.len(), 4, "FCANDLE mixed NaN source acceptance");
    assert_eq!(
        source_rows
            .iter()
            .filter(|row| row["timestamp"] == number(f64::NAN))
            .count(),
        1
    );
    let reports = RefCell::new(Vec::new());
    let emits = RefCell::new(Vec::new());
    let result = complete(futures_candles::fetch_ohlcv(
        serde_json::from_str(case["optsRaw"].as_str().unwrap()).unwrap(),
        false,
        || Ok(1800000000321.0),
        |_, _, _| {
            Ok(ready(Ok(Some(
                serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
            ))))
        },
        |_, _, error| {
            reports.borrow_mut().push(error.unwrap().name.clone());
            Ok(())
        },
        |message| {
            emits.borrow_mut().push(message.to_owned());
            Ok(())
        },
    ))
    .unwrap();
    assert_eq!(result.len(), 3, "good rows survive timestamp refusal");
    for (candle, timestamp) in result.iter().zip([1.0_f64, 2.0, 3.0]) {
        assert_eq!(candle.timestamp.to_bits(), timestamp.to_bits());
    }
    assert_eq!(
        *reports.borrow(),
        vec!["TypeError"],
        "FCANDLE per-row refusal diagnostic"
    );
    assert_eq!(
        *emits.borrow(),
        Vec::<String>::new(),
        "row refusal does not invoke the batch catch emission"
    );
}
