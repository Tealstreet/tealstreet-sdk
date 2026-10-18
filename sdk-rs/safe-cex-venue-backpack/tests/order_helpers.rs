use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderStopTriggerType, OrderType};
use safe_cex_venue_backpack::{order_utils::*, types::BackpackOrderRequestTriggerBy};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
};

fn oracle() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_helpers_oracle.cjs"
        ))
        .output()
        .expect("actual TypeScript order helper corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn bits(value: f64) -> String {
    format!(
        "{:016x}",
        if value.is_nan() {
            0x7ff8000000000000
        } else {
            value.to_bits()
        }
    )
}
fn optional_number(value: &Value) -> Option<f64> {
    match value[0].as_str().unwrap() {
        "undefined" | "null" => None,
        "number" => Some(number(&value[1])),
        other => panic!("unsupported source numeric observation {other}"),
    }
}
fn trigger(value: &Value) -> Option<OrderStopTriggerType> {
    value.as_str().map(|value| match value {
        "mark_price" => OrderStopTriggerType::MarkPrice,
        "last_price" => OrderStopTriggerType::LastPrice,
        "index_price" => OrderStopTriggerType::IndexPrice,
        _ => panic!("unexpected declared source trigger {value}"),
    })
}
fn entropy_error() -> ExchangeError {
    let mut error = ExchangeError::new("entropy failed");
    error.name = "EntropyError".into();
    error.store_subscriber_ids = vec![3, 8];
    error
}

#[test]
fn actual_typescript_next_batch_client_id() {
    let corpus = oracle();
    eprintln!("order helper runtime: {}", corpus["runtime"]);
    let cases = corpus["rng"].as_array().unwrap();
    assert_eq!(cases.len(), 16, "NEXT-ID independent source cases");
    let input_values = [
        0.0,
        1.0,
        2147483648.0,
        4294967295.0,
        5.0,
        0.0,
        -0.0,
        f64::from_bits(1),
        0.5,
        1.0 - f64::EPSILON / 2.0,
        1.0,
        -f64::EPSILON,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(
            case["input"],
            bits(input_values[i]),
            "NEXT-ID independent input case {i}"
        );
        let is_secure = case["kind"] == "secure";
        assert_eq!(is_secure, i < 5, "NEXT-ID independent route {i}");
        let calls = RefCell::new(Vec::new());
        let input = number(&case["input"]);
        let secure = |array: &mut [u32; 1]| {
            assert_eq!(*array, [0], "NEXT-ID fresh native buffer {i}");
            calls.borrow_mut().push(json!(["secure", 1, [array[0]]]));
            array[0] = input as u32;
            Ok(*array)
        };
        let actual = next_batch_client_id(if is_secure { Some(secure) } else { None }, || {
            calls.borrow_mut().push(json!(["fallback"]));
            Ok(input)
        })
        .unwrap();
        assert_eq!(bits(actual), case["result"], "NEXT-ID native bits case {i}");
        assert_eq!(
            calls.borrow().len(),
            1,
            "NEXT-ID independent native calls {i}"
        );
        assert_eq!(
            case["calls"].as_array().unwrap().len(),
            1,
            "NEXT-ID independent source calls {i}"
        );
        assert_eq!(
            calls.borrow().as_slice(),
            case["calls"].as_array().unwrap(),
            "NEXT-ID native entropy route {i}"
        );
        completed[usize::from(!is_secure)] += 1;
    }
    assert_eq!(completed, [5, 11], "NEXT-ID completed native comparisons");
    let failures = corpus["rngFailures"].as_array().unwrap();
    assert_eq!(failures.len(), 2, "NEXT-ID independent failure cases");
    for (i, case) in failures.iter().enumerate() {
        let calls = RefCell::new(Vec::new());
        let failure = entropy_error();
        let secure = |_: &mut [u32; 1]| {
            calls.borrow_mut().push(json!("secure"));
            Err(failure.clone())
        };
        let actual = next_batch_client_id(if i == 0 { Some(secure) } else { None }, || {
            calls.borrow_mut().push(json!("fallback"));
            Err(failure.clone())
        })
        .unwrap_err();
        assert_eq!(actual, failure, "NEXT-ID native failure metadata {i}");
        assert_eq!(actual.name, case["name"]);
        assert_eq!(actual.message, case["message"]);
        assert_eq!(case["sameFailure"], true);
        assert_eq!(
            calls.borrow().len(),
            1,
            "NEXT-ID independent failed calls {i}"
        );
        assert_eq!(calls.borrow().as_slice(), case["calls"].as_array().unwrap());
    }
}

#[test]
fn actual_typescript_unique_batch_client_ids() {
    let corpus = oracle();
    let cases = corpus["ids"].as_array().unwrap();
    // Hand-pinned expectations from the explicit sequences, never corpus-derived counts.
    let labels = [
        "zero",
        "negative zero",
        "negative",
        "nan",
        "infinity",
        "fractional",
        "ordered",
        "invalid candidates",
        "duplicates exhausted",
        "attempt 64 accepted",
        "attempt 65 excluded",
        "full 64",
        "65 unavailable",
        "generator throws",
    ];
    let expected_calls = [0, 0, 0, 0, 64, 3, 4, 10, 64, 64, 64, 64, 64, 2];
    let expected_counts = [
        0.0,
        -0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        2.5,
        3.0,
        2.0,
        2.0,
        1.0,
        1.0,
        64.0,
        65.0,
        2.0,
    ];
    let expected_candidates: [Vec<f64>; 14] = [
        vec![1.0],
        vec![1.0],
        vec![1.0],
        vec![1.0],
        vec![1.0],
        vec![1.0, 2.0, 3.0],
        vec![9.0, 2.0, 9.0, 7.0],
        vec![
            0.0,
            -0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.5,
            4294967296.0,
            4294967295.0,
            1.0,
        ],
        vec![1.0],
        (0..64).map(|i| if i == 63 { 5.0 } else { 0.0 }).collect(),
        (0..65).map(|i| if i == 64 { 5.0 } else { 0.0 }).collect(),
        (1..=64).map(f64::from).collect(),
        (1..=65).map(f64::from).collect(),
        vec![7.0, 8.0],
    ];
    assert_eq!(cases.len(), 14, "UNIQUE-ID independent source cases");
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(
            case["label"], labels[i],
            "UNIQUE-ID independent case identity {i}"
        );
        assert_eq!(
            case["count"],
            bits(expected_counts[i]),
            "UNIQUE-ID independent count bits {i}"
        );
        assert_eq!(
            case["throwAt"].as_u64(),
            if i == 13 { Some(1) } else { None },
            "UNIQUE-ID independent throw position {i}"
        );
        assert_eq!(
            case["values"].as_array().unwrap().len(),
            expected_candidates[i].len(),
            "UNIQUE-ID independent candidate count {i}"
        );
        for (j, expected) in expected_candidates[i].iter().enumerate() {
            assert_eq!(
                case["values"][j],
                bits(*expected),
                "UNIQUE-ID independent candidate bits {i}:{j}"
            );
        }
        let values: Vec<_> = case["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(number)
            .collect();
        let calls = Cell::new(0);
        let mut failure = ExchangeError::new("generator failed");
        failure.name = "EntropyError".into();
        failure.store_subscriber_ids = vec![17];
        let actual = create_batch_client_ids(number(&case["count"]), || {
            let index = calls.get();
            calls.set(index + 1);
            if case["throwAt"].as_u64() == Some(index as u64) {
                return Err(failure.clone());
            }
            Ok(values[index % values.len()])
        });
        assert_eq!(
            calls.get(),
            expected_calls[i],
            "UNIQUE-ID independent native attempts {i}"
        );
        assert_eq!(
            case["calls"], expected_calls[i],
            "UNIQUE-ID independent source attempts {i}"
        );
        if [0, 1, 6, 7, 9, 11].contains(&i) {
            let actual = actual.expect("UNIQUE-ID source success");
            let expected: Vec<f64> = match i {
                0 | 1 => vec![],
                6 => vec![9.0, 2.0, 7.0],
                7 => vec![f64::from(u32::MAX), 1.0],
                9 => vec![5.0],
                11 => (1..=64).map(f64::from).collect(),
                _ => unreachable!(),
            };
            assert_eq!(
                actual.len(),
                expected.len(),
                "UNIQUE-ID independent native output length {i}"
            );
            assert_eq!(actual, expected, "UNIQUE-ID independent native order {i}");
            assert_eq!(case["outcome"]["ok"], true);
            assert_eq!(
                case["outcome"]["result"].as_array().unwrap().len(),
                expected.len(),
                "UNIQUE-ID independent source output length {i}"
            );
            for (j, id) in actual.iter().enumerate() {
                assert_eq!(
                    bits(*id),
                    case["outcome"]["result"][j],
                    "UNIQUE-ID native id bits {i}:{j}"
                );
            }
            completed[0] += 1;
        } else {
            let actual = actual.expect_err("UNIQUE-ID source rejection");
            assert_eq!(case["outcome"]["ok"], false);
            assert_eq!(
                actual.name, case["outcome"]["name"],
                "UNIQUE-ID native error name {i}"
            );
            assert_eq!(
                actual.message, case["outcome"]["message"],
                "UNIQUE-ID native error message {i}"
            );
            assert_eq!(case["outcome"]["sameFailure"], i == 13);
            if i == 13 {
                assert_eq!(
                    actual, failure,
                    "UNIQUE-ID native original failure metadata"
                );
            }
            completed[1] += 1;
        }
    }
    assert_eq!(completed, [6, 8], "UNIQUE-ID completed native comparisons");
}

#[test]
fn actual_typescript_trigger_source_selection() {
    let corpus = oracle();
    let cases = corpus["triggers"].as_array().unwrap();
    assert_eq!(cases.len(), 128, "TRIGGER-BY independent source cases");
    let options = [
        None,
        Some(OrderStopTriggerType::MarkPrice),
        Some(OrderStopTriggerType::LastPrice),
        Some(OrderStopTriggerType::IndexPrice),
    ];
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        assert_eq!(
            case["kind"],
            if i < 64 { "tp" } else { "sl" },
            "TRIGGER-BY independent kind {i}"
        );
        assert_eq!(
            trigger(&case["tp"]),
            options[(i / 16) % 4],
            "TRIGGER-BY independent TP default {i}"
        );
        assert_eq!(
            trigger(&case["sl"]),
            options[(i / 4) % 4],
            "TRIGGER-BY independent SL default {i}"
        );
        assert_eq!(
            trigger(&case["explicit"]),
            options[i % 4],
            "TRIGGER-BY independent explicit type {i}"
        );
        let kind = match case["kind"].as_str().unwrap() {
            "tp" => TriggerKind::TakeProfit,
            "sl" => TriggerKind::StopLoss,
            _ => panic!("source kind"),
        };
        let calls = Cell::new(0);
        let actual = get_trigger_by(kind, trigger(&case["explicit"]), |name| {
            assert_eq!(name, "backpack", "TRIGGER-BY native metadata key {i}");
            calls.set(calls.get() + 1);
            (trigger(&case["tp"]), trigger(&case["sl"]))
        });
        let expected = match case["result"].as_str().unwrap() {
            "MarkPrice" => BackpackOrderRequestTriggerBy::MarkPrice,
            "LastPrice" => BackpackOrderRequestTriggerBy::LastPrice,
            "IndexPrice" => BackpackOrderRequestTriggerBy::IndexPrice,
            _ => panic!("source result"),
        };
        assert_eq!(actual, expected, "TRIGGER-BY native enum case {i}");
        assert_eq!(
            calls.get(),
            1,
            "TRIGGER-BY independent native metadata calls {i}"
        );
        assert_eq!(
            case["metadataCalls"], 1,
            "TRIGGER-BY independent source metadata calls {i}"
        );
        completed[usize::from(case["kind"] == "sl")] += 1;
    }
    assert_eq!(
        completed,
        [64, 64],
        "TRIGGER-BY completed native comparisons"
    );
}

#[test]
fn actual_typescript_trigger_order_classification() {
    let corpus = oracle();
    let cases = corpus["classifications"].as_array().unwrap();
    assert_eq!(cases.len(), 168, "TRIGGER-ORDER independent source cases");
    let expected_tickers = [
        vec![],
        vec![("OTHER", Some(100.0), Some(100.0))],
        vec![("BTC", None, Some(100.0))],
        vec![("BTC", None, Some(200.0))],
        vec![("BTC", Some(0.0), Some(100.0))],
        vec![("BTC", Some(-0.0), Some(100.0))],
        vec![("BTC", Some(f64::NAN), Some(100.0))],
        vec![("BTC", Some(f64::INFINITY), Some(100.0))],
        vec![("BTC", Some(f64::NEG_INFINITY), Some(100.0))],
        vec![("BTC", Some(100.0), Some(200.0))],
        vec![("BTC", Some(200.0), Some(100.0))],
        vec![
            ("BTC", Some(100.0), Some(200.0)),
            ("BTC", Some(1000.0), Some(2000.0)),
        ],
    ];
    let expected_prices = [
        f64::NEG_INFINITY,
        0.0,
        99.0,
        100.0,
        101.0,
        f64::INFINITY,
        f64::NAN,
    ];
    let mut completed = [0; 2];
    for (i, case) in cases.iter().enumerate() {
        let tickers: Vec<_> = case["tickers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|ticker| TickerPrice {
                symbol: ticker["symbol"].as_str().unwrap(),
                last: optional_number(&ticker["last"]),
                mark: optional_number(&ticker["mark"]),
            })
            .collect();
        assert_eq!(
            case["price"],
            bits(expected_prices[i % 7]),
            "TRIGGER-ORDER independent trigger bits {i}"
        );
        assert_eq!(
            case["side"],
            if (i / 7) % 2 == 0 { "buy" } else { "sell" },
            "TRIGGER-ORDER independent side {i}"
        );
        let expected = &expected_tickers[i / 14];
        assert_eq!(
            tickers.len(),
            expected.len(),
            "TRIGGER-ORDER independent ticker count {i}"
        );
        for (j, ticker) in tickers.iter().enumerate() {
            assert_eq!(
                ticker.symbol, expected[j].0,
                "TRIGGER-ORDER independent ticker symbol {i}:{j}"
            );
            assert_eq!(
                ticker.last.map(bits),
                expected[j].1.map(bits),
                "TRIGGER-ORDER independent last bits {i}:{j}"
            );
            assert_eq!(
                ticker.mark.map(bits),
                expected[j].2.map(bits),
                "TRIGGER-ORDER independent mark bits {i}:{j}"
            );
        }
        let side = if case["side"] == "buy" {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let actual = determine_trigger_order_type(&tickers, "BTC", side, number(&case["price"]));
        let expected = match case["result"].as_str().unwrap() {
            "stop_market" => OrderType::StopLoss,
            "take_profit_market" => OrderType::TakeProfit,
            _ => panic!("source order type {}", case["result"]),
        };
        assert_eq!(actual, expected, "TRIGGER-ORDER native enum case {i}");
        completed[usize::from(side == OrderSide::Sell)] += 1;
    }
    assert_eq!(
        completed,
        [84, 84],
        "TRIGGER-ORDER completed native comparisons"
    );
    let strings = corpus["strings"].as_array().unwrap();
    assert_eq!(strings.len(), 2, "TRIGGER-ORDER independent string cases");
    let symbol: String =
        safe_cex_num::numeric_ingress::js_from_str(strings[0]["raw"].as_str().unwrap()).unwrap();
    assert_eq!(symbol, "λ🚀", "TRIGGER-ORDER independent Unicode input");
    assert_eq!(
        strings[1]["raw"], "\"BTC\\ud800\"",
        "TRIGGER-ORDER independent surrogate input"
    );
    assert_eq!(
        determine_trigger_order_type(
            &[TickerPrice {
                symbol: &symbol,
                last: Some(100.0),
                mark: Some(200.0)
            }],
            &symbol,
            OrderSide::Sell,
            200.0
        ),
        OrderType::TakeProfit,
        "TRIGGER-ORDER Unicode native enum"
    );
    assert_eq!(strings[0]["result"], "take_profit_market");
    typed_utf16::assert_projection(
        strings[1]["raw"].as_str().unwrap(),
        &(safe_cex_num::numeric_ingress::js_from_str::<String>(
            strings[1]["raw"].as_str().unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    assert_eq!(
        strings[1]["result"], "take_profit_market",
        "TRIGGER-ORDER actual surrogate classification"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
