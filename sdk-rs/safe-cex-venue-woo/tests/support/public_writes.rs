#[path = "number_bits.rs"]
mod capture_bits;
use super::{
    handle_bbo_stream_events, handle_mark_prices_stream_events, handle_tickers_stream_events,
};
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[test]
fn public_writer_matches_numeric_bits_and_notification_failure() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/public-writes.json")).unwrap();
    assert_eq!(cases.len(), 87, "WOO-WRITER source case count");
    assert_eq!(
        cases
            .iter()
            .map(|case| case["tickers"].as_array().unwrap().len())
            .sum::<usize>(),
        174,
        "WOO-WRITER source ticker total"
    );
    let mut parity = 0;
    let mut notifications = 0;
    let mut refusals = 0;
    for case in cases {
        assert_eq!(
            case["tickers"].as_array().unwrap().len(),
            2,
            "WOO-WRITER source tickers per case"
        );
        let input = &case["input"];
        let store = DefaultStore::new();
        let tickers = ["BTCUSDT", "ETHUSDT"].map(|symbol| serde_json::from_value(json!({"id":symbol,"symbol":symbol,"bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":20})).unwrap());
        Store::update(
            &store,
            StoreUpdate {
                tickers: Some(tickers.into()),
                ..Default::default()
            },
            false,
        )
        .expect("test ticker initialization must succeed");
        let observed = Arc::new(Mutex::new(Vec::new()));
        let mut subscriptions = Vec::new();
        for name in ["first", "second"] {
            let calls = observed.clone();
            let throws = input["throws"] == true;
            subscriptions.push(store.subscribe(Arc::new(move |_, _| {
                calls.lock().unwrap().push(name);
                if name == "first" && throws {
                    let mut cause =
                        safe_cex_exchange::runtime::ExchangeError::new("subscriber failure");
                    cause.code = Some(safe_cex_exchange::js_runtime::value::Js::Number(731.0));
                    return Err(cause.into());
                }
                Ok(())
            })));
        }
        let rows = input
            .get("raw")
            .map(|raw| {
                safe_cex_transport::how::json_parse(raw.as_str().unwrap())
                    .into_value()
                    .unwrap()
            })
            .unwrap_or_else(|| input["rows"].clone());
        let retained_rows =
            safe_cex_types::retained_json::RetainedJsonInput::from_value(rows.clone()).unwrap();
        let result = match input["method"].as_str().unwrap() {
            "handleTickersStreamEvents" => handle_tickers_stream_events(&store, &retained_rows),
            "handleBBOStreamEvents" => handle_bbo_stream_events(&store, &retained_rows),
            "handleMarkPricesStreamEvents" => {
                handle_mark_prices_stream_events(&store, &retained_rows)
            }
            _ => unreachable!(),
        };
        assert_eq!(
            store.tickers().unwrap().len(),
            2,
            "WOO-WRITER native retained tickers"
        );
        assert_eq!(
            store.tickers().unwrap().len(),
            case["tickers"].as_array().unwrap().len(),
            "WOO-WRITER expected/native ticker count"
        );
        if case["error"]["message"] == "subscriber failure" {
            let error = result.expect_err("subscriber failure must propagate");
            assert_eq!(error.message, "subscriber failure");
            assert_eq!(
                error.store_subscriber_ids.len(),
                1,
                "writer retains one Store-local subscriber identity"
            );
            assert_eq!(
                error
                    .code
                    .as_ref()
                    .and_then(safe_cex_exchange::js_runtime::value::Js::as_f64),
                Some(731.0),
                "subscriber cause identity survives writer"
            );
            assert_eq!(
                json!(*observed.lock().unwrap()),
                case["notifications"],
                "subscriber throw aborts ordered fanout"
            );
            let native = store.tickers().unwrap();
            for index in 0..2 {
                let actual = &native[index];
                let expected = &case["tickers"][index];
                for (field, value) in [
                    ("last", actual.last),
                    ("bid", actual.bid),
                    ("ask", actual.ask),
                    ("mark", actual.mark),
                    ("volume", actual.volume),
                    ("quoteVolume", actual.quote_volume),
                ] {
                    assert_eq!(
                        capture_bits::number_bits(value),
                        expected[field]["$number"],
                        "committed first patch and remaining batch abort {field}"
                    );
                }
            }
            notifications += 1;
        } else if result.is_err() && case["error"].is_null() {
            // CHOSEN candidate refusal: dynamic copied undefined/null/string/
            // bool/object values cannot inhabit this typed Ticker field.
            assert!(rows.as_array().unwrap().iter().any(|row| {
                ["close", "amount", "volume", "bid", "ask", "price"]
                    .iter()
                    .any(|field| !row[*field].is_number())
            }));
            refusals += 1;
        } else if !case["error"].is_null() {
            let error = result.expect_err("malformed symbol must abort the batch");
            assert_eq!(
                json!({"name":error.name,"message":error.message}),
                case["error"],
                "later source error wins over typed refusal"
            );
            refusals += 1;
        } else {
            result.unwrap();
            assert_eq!(
                json!(*observed.lock().unwrap()),
                case["notifications"],
                "writer notification order {input}"
            );
            let native = store.tickers().unwrap();
            for index in 0..2 {
                let actual = &native[index];
                let expected = &case["tickers"][index];
                assert_eq!(actual.id, expected["id"]);
                for (name, value) in [
                    ("bid", actual.bid),
                    ("ask", actual.ask),
                    ("last", actual.last),
                    ("mark", actual.mark),
                    ("index", actual.index),
                    ("percentage", actual.percentage),
                    ("openInterest", actual.open_interest),
                    ("fundingRate", actual.funding_rate),
                    ("fundingTime", actual.funding_time),
                    ("volume", actual.volume),
                    ("quoteVolume", actual.quote_volume),
                ] {
                    assert_eq!(
                        capture_bits::number_bits(value),
                        expected[name]["$number"],
                        "writer native bits {name} {input}"
                    );
                }
            }
            parity += 1;
        }
        for unsubscribe in subscriptions {
            unsubscribe();
        }
    }
    assert!(parity > 0 && notifications > 0 && refusals > 0);
}
