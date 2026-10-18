use super::*;
use safe_cex_num::TableHit;
use safe_cex_types::Candle;
use safe_cex_venue_kraken::spot_reads;

fn pair(value: JsJsonValue) -> Value {
    match value {
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => number(v),
        JsJsonValue::String(v) => json!(v),
        JsJsonValue::Array(values) => Value::Array(values.into_iter().map(pair).collect()),
        JsJsonValue::Object(values) => {
            Value::Object(values.into_iter().map(|(k, v)| (k, pair(v))).collect())
        }
        _ => panic!("OHLCV unexpected pair domain"),
    }
}
fn store(case: &Value) -> DefaultStore {
    let store = DefaultStore::new();
    let market: Market=serde_json::from_value(json!({"id":"BTC/USD","symbol":"BTC/USD","base":"BTC","quote":"USD","active":true,
        "precision":{"amount":0.125,"price":0.25},"limits":{"amount":{"min":1.5,"max":17.25},"leverage":{"min":2.75,"max":31.5}},
        "info":{"pairName":case.get("pair").cloned().unwrap_or(json!("XXBTZUSD"))}})).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    store
}
fn fields(c: &Candle) -> [f64; 6] {
    [c.timestamp, c.open, c.high, c.low, c.close, c.volume]
}
const FIELDS: [&str; 6] = ["timestamp", "open", "high", "low", "close", "volume"];

#[test]
fn spot_ohlcv_live_oracle() {
    let oracle = oracle();
    let cases = oracle["ohlcv"].as_array().unwrap();
    assert_eq!(cases.len(), 55, "OHLCV source total cases");
    let census: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
    let counts = census["reads"]["outputs"]["ohlcv"].as_array().unwrap();
    assert_eq!(counts.len(), 55, "OHLCV independent case inventory");
    let mut compared_cases = 0;
    let mut compared_fields = 0;
    let mut native_records = 0;
    for index in 0..55 {
        let case = &cases[index];
        let store = store(case);
        let calls = RefCell::new(vec![]);
        let reports = RefCell::new(vec![]);
        let mut console_error = ExchangeError::new("console failure");
        console_error.metadata.store_subscriber_ids = vec![101, 103];
        let error_pointer = console_error.metadata.as_ref() as *const _;
        let result = complete(spot_reads::fetch_ohlcv(
            serde_json::from_str(case["optsRaw"].as_str().unwrap()).unwrap(),
            &store,
            |endpoint, value, interval, since| {
                let interval = match interval {
                    TableHit::Mapped(v) => number(v),
                    TableHit::Inherited(member) => json!({"inherited":member.js_string()}),
                    TableHit::Missing => panic!("OHLCV missing interval not defaulted"),
                };
                calls.borrow_mut().push(json!({"endpoint":endpoint,"pair":pair(retained_model_values::prepared(&value)),"interval":interval,"since":since.map_or(json!({"undefined":true}),number)}));
                let failure =
                    || ExchangeError::new(case["message"].as_str().unwrap_or("read failure"));
                if case["mode"] == "sync" {
                    return Err(failure());
                }
                Ok(ready(if case["mode"] == "async" {
                    Err(failure())
                } else {
                    Ok(case["raw"]
                        .as_str()
                        .map(|raw| serde_json::from_str(raw).unwrap()))
                }))
            },
            |label, message| {
                reports.borrow_mut().push(json!([label, message]));
                if case["consoleFail"] == true {
                    Err(console_error)
                } else {
                    Ok(())
                }
            },
        ));
        assert_eq!(calls.borrow().len(), 1, "OHLCV native request count");
        assert_eq!(
            *calls.borrow(),
            case["calls"].as_array().unwrap().clone(),
            "OHLCV native query fields"
        );
        assert_eq!(
            *reports.borrow(),
            case["reports"].as_array().unwrap().clone(),
            "OHLCV native console trace"
        );
        if let Some(expected_count) = counts[index].as_u64() {
            let candles = result.expect("OHLCV expected success");
            native_records += candles.len();
            assert_eq!(
                candles.len(),
                expected_count as usize,
                "OHLCV independent native length"
            );
            cardinality::native_len(candles.len(), &case["result"], "OHLCV source/native length");
            for row in 0..expected_count as usize {
                let candle = candles.get(row).expect("OHLCV native index coverage");
                let expected = &case["result"][row];
                assert_eq!(
                    expected.as_object().unwrap().len(),
                    6,
                    "OHLCV source field count"
                );
                assert!(candle.last_side.is_none(), "OHLCV native lastSide absent");
                for (field_index, field) in FIELDS.into_iter().enumerate() {
                    let value = fields(candle)[field_index];
                    assert!(
                        expected.get(field).is_some(),
                        "OHLCV source field inventory"
                    );
                    assert_eq!(
                        number(value),
                        expected[field],
                        "OHLCV native {field} case {index} row {row}"
                    );
                    compared_fields += 1;
                }
            }
        } else {
            let error = result.expect_err("OHLCV expected console rejection");
            assert_eq!(
                error.message,
                case["error"].as_str().unwrap(),
                "OHLCV console rejection message"
            );
            assert!(
                std::ptr::eq(error.metadata.as_ref(), error_pointer),
                "OHLCV console error allocation"
            );
            assert_eq!(
                error.metadata.store_subscriber_ids,
                vec![101, 103],
                "OHLCV console subscriber identity"
            );
            assert_eq!(case["sameError"], true, "OHLCV source console identity");
        }
        compared_cases += 1;
    }
    assert_eq!(compared_cases, 55, "OHLCV completed cases");
    assert_eq!(native_records, 93, "OHLCV native total records");
    assert_eq!(compared_fields, 558, "OHLCV completed native fields");
}

#[test]
fn spot_ohlcv_chosen_timestamp_boundary() {
    let source = &oracle()["ohlcvBoundaries"];
    assert_eq!(
        source.as_array().unwrap().len(),
        2,
        "OHLCV boundary source total"
    );
    for index in 0..2 {
        let case = &source[index];
        assert_eq!(
            case["result"].as_array().unwrap().len(),
            1,
            "OHLCV source retains untyped timestamp"
        );
        assert_eq!(
            case["result"][0]["timestamp"],
            if index == 0 {
                json!("17.25")
            } else if index == 1 {
                Value::Null
            } else {
                number(f64::INFINITY)
            },
            "OHLCV source timestamp value"
        );
        let store = DefaultStore::new();
        let reports = RefCell::new(vec![]);
        let result = complete(spot_reads::fetch_ohlcv(
            serde_json::from_str(case["optsRaw"].as_str().unwrap()).unwrap(),
            &store,
            |_, _, _, _| {
                Ok(ready(Ok(Some(
                    serde_json::from_str(case["raw"].as_str().unwrap()).unwrap(),
                ))))
            },
            |label, message| {
                reports
                    .borrow_mut()
                    .push((label.to_owned(), message.to_owned()));
                Ok(())
            },
        ))
        .unwrap();
        assert!(result.is_empty(), "OHLCV CHOSEN timestamp refusal");
        assert_eq!(
            *reports.borrow(),
            vec![(
                "[KrakenSpot] fetchOHLCV error:".into(),
                "Kraken Spot candle timestamp must be a number".into()
            )],
            "OHLCV CHOSEN refusal trace"
        );
    }
}
