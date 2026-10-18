use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::Ticker;
use safe_cex_venue_binance::{public_ticker_events::*, spot_ticker_events::*};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};
#[derive(Clone)]
struct Target {
    tag: u64,
    ticker: Rc<RefCell<Ticker>>,
}
type Dictionary = Rc<RefCell<HashMap<String, Option<Rc<Target>>>>>;
struct Update {
    receiver: u64,
    ticker: Rc<Target>,
    fields: TickerFields,
}
const ALL: [TickerField; 10] = [
    TickerField::Last,
    TickerField::Mark,
    TickerField::Index,
    TickerField::Percentage,
    TickerField::Volume,
    TickerField::QuoteVolume,
    TickerField::FundingRate,
    TickerField::FundingTime,
    TickerField::Bid,
    TickerField::Ask,
];
const RAW: [&str; 10] = ["c", "P", "v", "q", "p", "i", "r", "T", "b", "a"];
fn number(actual: f64, expected: f64, label: &str) {
    if expected.is_nan() {
        assert!(actual.is_nan(), "{label} native NaN");
    } else {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "{label} native number ({actual:?} vs{expected:?})"
        );
    }
}
fn encode(value: f64) -> Value {
    if value.is_nan() {
        json!("nan")
    } else if value == f64::INFINITY {
        json!("infinity")
    } else if value == f64::NEG_INFINITY {
        json!("-infinity")
    } else if value == 0.0 && value.is_sign_negative() {
        json!("-zero")
    } else {
        json!(value)
    }
}
fn field(t: &Ticker, key: TickerField) -> f64 {
    match key {
        TickerField::Last => t.last,
        TickerField::Mark => t.mark,
        TickerField::Index => t.index,
        TickerField::Percentage => t.percentage,
        TickerField::Volume => t.volume,
        TickerField::QuoteVolume => t.quote_volume,
        TickerField::FundingRate => t.funding_rate,
        TickerField::FundingTime => t.funding_time,
        TickerField::Bid => t.bid,
        TickerField::Ask => t.ask,
    }
}
fn apply(t: &mut Ticker, key: TickerField, value: f64) {
    match key {
        TickerField::Last => t.last = value,
        TickerField::Mark => t.mark = value,
        TickerField::Index => t.index = value,
        TickerField::Percentage => t.percentage = value,
        TickerField::Volume => t.volume = value,
        TickerField::QuoteVolume => t.quote_volume = value,
        TickerField::FundingRate => t.funding_rate = value,
        TickerField::FundingTime => t.funding_time = value,
        TickerField::Bid => t.bid = value,
        TickerField::Ask => t.ask = value,
    }
}
fn target(tag: u64) -> Rc<Target> {
    let base = if tag == 1 { 90 } else { 190 };
    let ticker:Ticker=serde_json::from_value(json!({"id":format!("ticker{tag}"),"symbol":if tag==1{"BTCUSDT"}else{"ETHUSDT"},"last":base,"mark":base+1,"index":base+2,"percentage":base+3,"volume":base+4,"quoteVolume":base+5,"fundingRate":base+6,"fundingTime":base+7,"bid":base+8,"ask":base+9,"openInterest":-7})).unwrap();
    Rc::new(Target {
        tag,
        ticker: Rc::new(RefCell::new(ticker)),
    })
}
fn event(second: bool) -> PublicEvent {
    Some(Rc::new(RefCell::new(if second {
        json!({"s":"ETHUSDT","c":"22.3","P":"5.5","v":"7.6","q":"77.28","p":"23.1","i":"24.2","r":".002","T":"1701123456789","b":"22.2","a":"22.4"})
    } else {
        json!({"s":"BTCUSDT","c":"12.3","P":"2.5","v":"3.6","q":"44.28","p":"13.1","i":"14.2","r":".001","T":"1700123456789","b":"12.2","a":"12.4"})
    })))
}
fn expected_schema(lane: &str) -> Vec<TickerField> {
    match lane {
        "futures_ticker" | "pm_ticker" => vec![
            TickerField::Last,
            TickerField::Percentage,
            TickerField::Volume,
            TickerField::QuoteVolume,
        ],
        "futures_mark" | "pm_mark" => vec![
            TickerField::Mark,
            TickerField::Index,
            TickerField::FundingRate,
            TickerField::FundingTime,
        ],
        "spot_ticker" => vec![
            TickerField::Last,
            TickerField::Mark,
            TickerField::Index,
            TickerField::Percentage,
            TickerField::Volume,
            TickerField::QuoteVolume,
        ],
        _ => vec![TickerField::Bid, TickerField::Ask],
    }
}
fn expected_values(lane: &str, name: &str, tag: u64) -> Vec<(TickerField, f64)> {
    let base = if tag == 1 {
        [
            12.3,
            13.1,
            14.2,
            2.5,
            3.6,
            44.28,
            0.001,
            1700123456789.0,
            12.2,
            12.4,
        ]
    } else {
        [
            22.3,
            23.1,
            24.2,
            5.5,
            7.6,
            77.28,
            0.002,
            1701123456789.0,
            22.2,
            22.4,
        ]
    };
    expected_schema(lane)
        .into_iter()
        .map(|key| {
            let mut value = base[ALL.iter().position(|k| *k == key).unwrap()];
            if lane == "spot_ticker" && (key == TickerField::Mark || key == TickerField::Index) {
                value = base[0];
            }
            if tag == 1 {
                match name {
                    "missing-fields" | "nan-values" => value = f64::NAN,
                    "array-values" => value = 8.5,
                    "getter-mutates-event" => value = 31.25,
                    "alias-case" if key == TickerField::Percentage => value = 13.1,
                    "alias-case" if key == TickerField::FundingTime => value = 1700123000000.0,
                    _ => (),
                }
            }
            (key, value)
        })
        .collect()
}
fn expected_tags(name: &str) -> Vec<u64> {
    match name {
        "empty"
        | "unknown"
        | "false-ticker"
        | "capture-fail"
        | "symbol-coercion-fail"
        | "event-null"
        | "event-undefined"
        | "primitive-event"
        | "coercion-fail"
        | "update-getter-fail" => vec![],
        "missing-fields"
        | "alias-case"
        | "numeric-values"
        | "partial-numeric"
        | "nan-values"
        | "array-values"
        | "symbol-null"
        | "symbol-array"
        | "symbol-object"
        | "getter-mutates-event"
        | "first-update-fail"
        | "mutate-dictionary"
        | "replace-event"
        | "truncate-events" => vec![1],
        "append-event" => vec![1, 2, 2],
        _ => vec![1, 2],
    }
}
fn source_value(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path} source decoded number ({:?} vs {:?})",
            a.as_f64(),
            b.as_f64()
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path} source array count");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                source_value(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "{path} source own keys"
            );
            for (key, value) in a {
                source_value(value, &b[key], &format!("{path}/{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path} source decoded value"),
    }
}

fn compare_source(lane: &str, native: Vec<Value>) {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-ticker-events.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 224, "fixed source floor8*28");
    let rows: Vec<_> = rows.into_iter().filter(|r| r["lane"] == lane).collect();
    assert_eq!(rows.len(), 28, "fixed per-lane case floor");
    for (index, (actual, expected)) in rows.into_iter().zip(native).enumerate() {
        source_value(
            &actual,
            &expected,
            &format!("TICKER {lane} source case{index}"),
        );
    }
}
fn check(lane: &str) {
    let names = [
        "normal",
        "empty",
        "unknown",
        "missing-fields",
        "alias-case",
        "numeric-values",
        "partial-numeric",
        "nan-values",
        "array-values",
        "coercion-fail",
        "symbol-null",
        "symbol-array",
        "symbol-object",
        "symbol-coercion-fail",
        "event-null",
        "event-undefined",
        "primitive-event",
        "first-update-fail",
        "second-update-fail",
        "capture-fail",
        "update-getter-fail",
        "getter-mutates-event",
        "swap-store",
        "mutate-dictionary",
        "append-event",
        "replace-event",
        "truncate-events",
        "false-ticker",
    ];
    let mut native = Vec::new();
    for (index, name) in names.into_iter().enumerate() {
        let btc = target(1);
        let eth = target(2);
        let dictionary: Dictionary = Rc::new(RefCell::new(HashMap::from([
            ("BTCUSDT".into(), Some(btc.clone())),
            ("ETHUSDT".into(), Some(eth.clone())),
        ])));
        let secondary: Dictionary = Rc::new(RefCell::new(HashMap::new()));
        let a = event(false);
        let b = event(true);
        let unknown = Some(Rc::new(RefCell::new(
            json!({"s":"UNKNOWN","c":{"toString":0},"p":{"toString":0},"b":{"toString":0}}),
        )));
        let events: PublicEvents =
            Rc::new(RefCell::new(vec![a.clone(), unknown.clone(), b.clone()]));
        if name == "empty" {
            events.borrow_mut().clear();
        }
        if name == "unknown" {
            *events.borrow_mut() = vec![unknown.clone()];
        }
        if [
            "missing-fields",
            "alias-case",
            "numeric-values",
            "partial-numeric",
            "nan-values",
            "array-values",
            "coercion-fail",
            "symbol-null",
            "symbol-array",
            "symbol-object",
            "symbol-coercion-fail",
            "event-null",
            "event-undefined",
            "primitive-event",
            "getter-mutates-event",
            "false-ticker",
        ]
        .contains(&name)
        {
            *events.borrow_mut() = vec![a.clone()];
        }
        if name == "missing-fields" {
            *a.as_ref().unwrap().borrow_mut() = json!({"s":"BTCUSDT"});
        }
        if name == "alias-case" {
            let mut value = a.as_ref().unwrap().borrow_mut();
            let fields = value.as_object_mut().unwrap();
            fields.remove("P");
            fields.remove("T");
            fields.insert("t".into(), json!("1700123000000"));
        }
        for key in RAW {
            let mut value = a.as_ref().unwrap().borrow_mut();
            let fields = value.as_object_mut().unwrap();
            if name == "numeric-values" {
                let number = fields[key].as_str().unwrap().parse::<f64>().unwrap();
                fields.insert(key.into(), json!(number));
            }
            if name == "partial-numeric" {
                fields.insert(
                    key.into(),
                    json!(format!("{}tail", fields[key].as_str().unwrap())),
                );
            }
            if name == "nan-values" {
                fields.insert(key.into(), json!({}));
            }
            if name == "array-values" {
                fields.insert(key.into(), json!(["8.5tail", null]));
            }
            if name == "coercion-fail" {
                fields.insert(key.into(), json!({"toString":0}));
            }
        }
        for (case, symbol, key) in [
            ("symbol-null", Value::Null, "null"),
            ("symbol-array", json!([1, 2]), "1,2"),
            ("symbol-object", json!({}), "[object Object]"),
        ] {
            if name == case {
                a.as_ref().unwrap().borrow_mut()["s"] = symbol;
                dictionary
                    .borrow_mut()
                    .insert(key.into(), Some(btc.clone()));
            }
        }
        if name == "symbol-coercion-fail" {
            a.as_ref().unwrap().borrow_mut()["s"] = json!({"toString":0});
        }
        if name == "event-null" {
            *events.borrow_mut() = vec![Some(Rc::new(RefCell::new(Value::Null)))];
        }
        if name == "event-undefined" {
            *events.borrow_mut() = vec![None];
        }
        if name == "primitive-event" {
            *events.borrow_mut() = vec![Some(Rc::new(RefCell::new(json!(7))))];
            dictionary
                .borrow_mut()
                .insert("undefined".into(), Some(btc.clone()));
        }
        if name == "false-ticker" {
            dictionary.borrow_mut().insert("BTCUSDT".into(), None);
        }
        let trace = RefCell::new(Vec::<Value>::new());
        let capture_count = Cell::new(0);
        let getters = Cell::new(0);
        let updates = RefCell::new(Vec::<Update>::new());
        let lookup_keys = RefCell::new(Vec::<String>::new());
        let current = Cell::new(10);
        let mut error = ExchangeError::new("ticker fixture");
        error.name = "RangeError".into();
        error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "fixture-code"
        )));
        let error = Rc::new(error);
        let capture = || {
            capture_count.set(capture_count.get() + 1);
            trace.borrow_mut().push(json!(["capture", current.get()]));
            if name == "capture-fail" {
                Err(error.clone())
            } else {
                Ok(if current.get() == 10 {
                    dictionary.clone()
                } else {
                    secondary.clone()
                })
            }
        };
        let lookup = |captured: &Dictionary, key: &str| {
            assert!(
                Rc::ptr_eq(captured, &dictionary),
                "native captured dictionary identity"
            );
            lookup_keys.borrow_mut().push(key.into());
            trace.borrow_mut().push(json!(["lookup", key]));
            Ok(captured.borrow().get(key).cloned().flatten())
        };
        let get_update = || {
            getters.set(getters.get() + 1);
            let receiver = current.get();
            trace.borrow_mut().push(json!(["get-update", receiver]));
            if name == "update-getter-fail" {
                return Err(error.clone());
            }
            if name == "getter-mutates-event" {
                for key in RAW {
                    a.as_ref().unwrap().borrow_mut()[key] = json!("31.25");
                }
            }
            let trace = &trace;
            let updates = &updates;
            let current = &current;
            let dictionary = &dictionary;
            let events = &events;
            let b = &b;
            let error = &error;
            Ok(move |ticker: Rc<Target>, fields: TickerFields| {
                trace
                    .borrow_mut()
                    .push(json!(["update", receiver, ticker.tag]));
                for (key, value) in &fields {
                    apply(&mut ticker.ticker.borrow_mut(), *key, *value);
                }
                updates.borrow_mut().push(Update {
                    receiver,
                    ticker,
                    fields,
                });
                let count = updates.borrow().len();
                if name == "swap-store" && count == 1 {
                    current.set(20);
                }
                if name == "mutate-dictionary" && count == 1 {
                    dictionary.borrow_mut().remove("ETHUSDT");
                }
                if name == "append-event" && count == 1 {
                    events.borrow_mut().push(event(true));
                }
                if name == "replace-event" && count == 1 {
                    let mut value = b.as_ref().unwrap().borrow().clone();
                    value["s"] = json!("UNKNOWN");
                    events.borrow_mut()[2] = Some(Rc::new(RefCell::new(value)));
                }
                if name == "truncate-events" && count == 1 {
                    events.borrow_mut().truncate(1);
                }
                if (name == "first-update-fail" && count == 1)
                    || (name == "second-update-fail" && count == 2)
                {
                    Err(error.clone())
                } else {
                    Ok(())
                }
            })
        };
        let result = match lane {
            "futures_ticker" => handle_futures_ticker_events(&events, capture, lookup, get_update),
            "futures_mark" => handle_futures_mark_events(&events, capture, lookup, get_update),
            "futures_book" => handle_futures_book_events(&events, capture, lookup, get_update),
            "pm_ticker" => handle_pm_ticker_events(&events, capture, lookup, get_update),
            "pm_mark" => handle_pm_mark_events(&events, capture, lookup, get_update),
            "pm_book" => handle_pm_book_events(&events, capture, lookup, get_update),
            "spot_ticker" => handle_spot_ticker_events(&events, capture, lookup, get_update),
            "spot_book" => handle_spot_book_events(&events, capture, lookup, get_update),
            _ => panic!("unknown authored lane"),
        };
        let tags = expected_tags(name);
        assert_eq!(
            updates.borrow().len(),
            tags.len(),
            "TICKER {lane} native update count case{index}"
        );
        assert_eq!(
            capture_count.get(),
            1,
            "native capture once including empty/error input"
        );
        assert_eq!(
            events.borrow().len(),
            match name {
                "empty" => 0,
                "append-event" => 4,
                "truncate-events" => 1,
                "unknown"
                | "missing-fields"
                | "alias-case"
                | "numeric-values"
                | "partial-numeric"
                | "nan-values"
                | "array-values"
                | "coercion-fail"
                | "symbol-null"
                | "symbol-array"
                | "symbol-object"
                | "symbol-coercion-fail"
                | "event-null"
                | "event-undefined"
                | "primitive-event"
                | "getter-mutates-event"
                | "false-ticker" => 1,
                _ => 3,
            },
            "native final live event count"
        );
        let keys: Vec<&str> = match name {
            "empty"
            | "capture-fail"
            | "symbol-coercion-fail"
            | "event-null"
            | "event-undefined" => vec![],
            "unknown" => vec!["UNKNOWN"],
            "symbol-null" => vec!["null"],
            "symbol-array" => vec!["1,2"],
            "symbol-object" => vec!["[object Object]"],
            "primitive-event" => vec!["undefined"],
            "first-update-fail"
            | "update-getter-fail"
            | "coercion-fail"
            | "truncate-events"
            | "missing-fields"
            | "alias-case"
            | "numeric-values"
            | "partial-numeric"
            | "nan-values"
            | "array-values"
            | "getter-mutates-event"
            | "false-ticker" => vec!["BTCUSDT"],
            "replace-event" => vec!["BTCUSDT", "UNKNOWN", "UNKNOWN"],
            "append-event" => vec!["BTCUSDT", "UNKNOWN", "ETHUSDT", "ETHUSDT"],
            _ => vec!["BTCUSDT", "UNKNOWN", "ETHUSDT"],
        };
        assert_eq!(
            *lookup_keys.borrow(),
            keys,
            "native live captured dictionary lookups"
        );
        let getter_count = tags.len()
            + usize::from(
                ["update-getter-fail", "coercion-fail", "primitive-event"].contains(&name),
            );
        assert_eq!(
            getters.get(),
            getter_count,
            "native current update getter count"
        );
        assert_eq!(
            current.get(),
            if name == "swap-store" { 20 } else { 10 },
            "native current Store receiver"
        );
        for (row, (update, tag)) in updates.borrow().iter().zip(&tags).enumerate() {
            assert_eq!(update.ticker.tag, *tag);
            assert!(
                Rc::ptr_eq(&update.ticker, if *tag == 1 { &btc } else { &eth }),
                "native original ticker handle identity"
            );
            assert_eq!(
                update.receiver,
                if name == "swap-store" && row > 0 {
                    20
                } else {
                    10
                },
                "native current update receiver"
            );
            let expected = expected_values(lane, name, *tag);
            assert_eq!(
                update.fields.len(),
                if lane == "spot_ticker" {
                    6
                } else if lane.ends_with("book") {
                    2
                } else {
                    4
                },
                "native independent patch field count"
            );
            assert_eq!(
                update
                    .fields
                    .iter()
                    .map(|(key, _)| *key)
                    .collect::<Vec<_>>(),
                expected_schema(lane),
                "native source patch property order"
            );
            for (column, ((key, actual), (expected_key, expected))) in
                update.fields.iter().zip(expected).enumerate()
            {
                assert_eq!(*key, expected_key);
                number(
                    *actual,
                    expected,
                    &format!("TICKER {lane} native value case{index} row{row} field{column}"),
                );
            }
        }
        for (target, base) in [(&btc, 90.0), (&eth, 190.0)] {
            let changed = tags.contains(&target.tag);
            let expected = expected_values(lane, name, target.tag);
            let ticker = target.ticker.borrow();
            for (index, key) in ALL.into_iter().enumerate() {
                let expected = if changed {
                    expected
                        .iter()
                        .find(|(k, _)| *k == key)
                        .map(|(_, v)| *v)
                        .unwrap_or(base + index as f64)
                } else {
                    base + index as f64
                };
                number(
                    field(&ticker, key),
                    expected,
                    "native final ticker fields and untouched omissions",
                );
            }
            assert_eq!(ticker.id, format!("ticker{}", target.tag));
            assert_eq!(
                ticker.open_interest.to_bits(),
                (-7.0_f64).to_bits(),
                "native untouched extra model field"
            );
        }
        let original_failure = [
            "capture-fail",
            "update-getter-fail",
            "first-update-fail",
            "second-update-fail",
        ]
        .contains(&name);
        let generated_failure = [
            "coercion-fail",
            "symbol-coercion-fail",
            "event-null",
            "event-undefined",
            "primitive-event",
        ]
        .contains(&name);
        let (failure, error_name, error_message) = match &result {
            Ok(()) => {
                assert!(!original_failure && !generated_failure);
                (Value::Null, Value::Null, Value::Null)
            }
            Err(e) => {
                if original_failure {
                    assert!(
                        Rc::ptr_eq(e, &error),
                        "native original callback error identity"
                    );
                    assert_eq!(e.name, "RangeError");
                    assert_eq!(e.message, "ticker fixture");
                    assert_eq!(observed_error_code(&e.code), Some(json!("fixture-code")));
                } else {
                    assert!(generated_failure);
                    assert_eq!(e.name, "TypeError");
                    let expected = match name {
                        "event-null" => "Cannot read properties of null (reading 's')".into(),
                        "event-undefined" => {
                            "Cannot read properties of undefined (reading 's')".into()
                        }
                        "primitive-event" => format!(
                            "Cannot use 'in' operator to search for '{}' in 7",
                            if lane.ends_with("book") {
                                "b"
                            } else if lane.ends_with("mark") {
                                "p"
                            } else {
                                "c"
                            }
                        ),
                        _ => "Cannot convert object to primitive value".into(),
                    };
                    assert_eq!(e.message, expected, "native generated error message");
                }
                (
                    json!(if original_failure {
                        "original"
                    } else {
                        "generated"
                    }),
                    json!(e.name),
                    json!(e.message),
                )
            }
        };
        assert_eq!(
            trace.borrow().len(),
            1 + keys.len() + getter_count + tags.len(),
            "native independent effect count"
        );
        let wire_updates:Vec<Value>=updates.borrow().iter().map(|u|json!({"receiver":u.receiver,"ticker":u.ticker.tag,"fields":u.fields.iter().map(|(key,value)|json!([key.name(),encode(*value)])).collect::<Vec<_>>(),"identity":true})).collect();
        let wire_ticker = |t: &Target| {
            let t = t.ticker.borrow();
            ALL.into_iter()
                .map(|key| json!([key.name(), encode(field(&t, key))]))
                .collect::<Vec<_>>()
        };
        native.push(json!({"lane":lane,"name":name,"trace":*trace.borrow(),"updates":wire_updates,"currentStore":current.get(),"length":events.borrow().len(),"failure":failure,"errorName":error_name,"errorMessage":error_message,"btc":wire_ticker(&btc),"eth":wire_ticker(&eth)}));
    }
    assert_eq!(
        native.len(),
        28,
        "authored native case floor stopped false-ticker"
    );
    compare_source(lane, native);
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_ticker() {
    check("futures_ticker");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_mark() {
    check("futures_mark");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn futures_book() {
    check("futures_book");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_ticker() {
    check("pm_ticker");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_mark() {
    check("pm_mark");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn pm_book() {
    check("pm_book");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn spot_ticker() {
    check("spot_ticker");
}
#[test]
#[ignore = "requires configured local TypeScript"]
fn spot_book() {
    check("spot_book");
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
