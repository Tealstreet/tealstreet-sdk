use safe_cex_venue_bitrue::{ws_private, ws_symbol_helpers as helpers};
use serde_json::Value;
use std::{cell::RefCell, process::Command, rc::Rc};
#[derive(Debug)]
enum Native {
    String(Vec<u16>),
    Void,
    Original(Rc<()>),
}
impl PartialEq for Native {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Void, Self::Void) => true,
            (Self::Original(a), Self::Original(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn result(value: Result<Vec<u16>, Rc<()>>) -> Native {
    match value {
        Ok(value) => Native::String(value),
        Err(error) => Native::Original(error),
    }
}
fn expected(v: &Value, error: &Rc<()>) -> Native {
    match v["kind"].as_str().unwrap() {
        "string" => Native::String(units(&v["units"])),
        "void" => Native::Void,
        "original" => Native::Original(error.clone()),
        other => panic!("unexpected source result {other}"),
    }
}
const SYMBOLS: [&str; 20] = [
    "",
    "BTCUSDT",
    "E-BTC-USDT",
    "e_BTC/USDT",
    "_E-BTC-USDT",
    "E/BTC-USDT",
    "EE-BTC-USDT",
    "E--BTC__/_USDT",
    "E_ΟΣ",
    "E_İI",
    "E_Σ",
    "E_ΟΣΑ",
    "",
    "",
    "E-",
    "e_",
    "E-BTC-USDT\n",
    "E-БТК-USD",
    "E-K-İ-ẞ",
    "E-BTC",
];
const TOPICS: [&str; 24] = [
    "1m",
    "3m",
    "5m",
    "15m",
    "30m",
    "1h",
    "2h",
    "4h",
    "6h",
    "12h",
    "1d",
    "1w",
    "unknown",
    "constructor",
    "toString",
    "__proto__",
    "symbol-surrogate",
    "converter-number",
    "convert-get-throw",
    "convert-call-throw",
    "convert-coercion-throw",
    "interval-key-throw",
    "interval-get-throw",
    "interval-coercion-throw",
];
const OPENS: [&str; 12] = [
    "live",
    "disposed",
    "truthy-string",
    "falsy-zero",
    "falsy-null",
    "guard-throw",
    "subscribe-get-throw",
    "subscribe-throw",
    "ping-get-throw",
    "ping-throw",
    "dispose-during-subscribe",
    "promise",
];
fn symbol(index: usize) -> Vec<u16> {
    match index {
        12 => [0xd800, 0x45, 0x2d, 0xdfff, 0x2f, 0x42, 0x54, 0x43].into(),
        13 => [0x45, 0x2d, 0xd800, 0x5f, 0x2f, 0xdfff].into(),
        _ => SYMBOLS[index].encode_utf16().collect(),
    }
}
#[test]
fn ws_symbols_open_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/ws_symbols_open_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WS-SYMBOLS source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let mut compared = 0;
    for (group, count, start) in [("convert", 20, 0), ("topic", 24, 20), ("open", 12, 44)] {
        for index in 0..count {
            let row = &rows[start + index];
            let error = Rc::new(());
            let events = RefCell::new(Vec::<String>::new());
            let native = match group {
                "convert" => result(helpers::convert_to_ws_symbol(&symbol(index), |argument| {
                    events.borrow_mut().push("lower".into());
                    assert_eq!(
                        argument,
                        units(&row["normalized"]),
                        "BITRUE-WS-SYMBOLS-NATIVE-NORMALIZED case{index}"
                    );
                    if index == 19 {
                        Err(error.clone())
                    } else {
                        Ok(units(&row["lowered"]))
                    }
                })),
                "topic" => {
                    let input = if index == 16 {
                        vec![0x45, 0x2d, 0xd800, 0x5f, 0xdfff]
                    } else {
                        "E-BTC-USDT".encode_utf16().collect()
                    };
                    let interval = Rc::new(());
                    result(helpers::format_kline_topic(
                        &input,
                        &interval,
                        |argument| {
                            assert_eq!(
                                argument,
                                input.as_slice(),
                                "BITRUE-WS-SYMBOLS native converter argument"
                            );
                            events.borrow_mut().extend(
                                serde_json::from_value::<Vec<String>>(row["convertEvents"].clone())
                                    .unwrap(),
                            );
                            if row["conversionError"].as_bool().unwrap() {
                                Err(error.clone())
                            } else {
                                Ok(units(&row["converted"]))
                            }
                        },
                        |argument| {
                            assert!(
                                Rc::ptr_eq(argument, &interval),
                                "BITRUE-WS-SYMBOLS native interval identity"
                            );
                            events.borrow_mut().extend(
                                serde_json::from_value::<Vec<String>>(
                                    row["intervalEvents"].clone(),
                                )
                                .unwrap(),
                            );
                            if row["intervalError"].as_bool().unwrap() {
                                Err(error.clone())
                            } else {
                                Ok(units(&row["term"]))
                            }
                        },
                    ))
                }
                "open" => {
                    let id = OPENS[index];
                    let disposed = RefCell::new(matches!(id, "disposed" | "truthy-string"));
                    let value = ws_private::on_open(
                        || {
                            events.borrow_mut().push("guard".into());
                            if id == "guard-throw" {
                                Err(error.clone())
                            } else {
                                Ok(*disposed.borrow())
                            }
                        },
                        || {
                            events.borrow_mut().push("resolve-subscribe".into());
                            if id == "subscribe-get-throw" {
                                return Err(error.clone());
                            }
                            events.borrow_mut().push("subscribe".into());
                            if id == "subscribe-throw" {
                                return Err(error.clone());
                            }
                            if id == "dispose-during-subscribe" {
                                *disposed.borrow_mut() = true;
                            }
                            Ok(Rc::new(()))
                        },
                        || {
                            events.borrow_mut().push("resolve-ping".into());
                            if id == "ping-get-throw" {
                                return Err(error.clone());
                            }
                            events.borrow_mut().push("ping".into());
                            if id == "ping-throw" {
                                return Err(error.clone());
                            }
                            Ok(Rc::new(()))
                        },
                    );
                    match value {
                        Ok(()) => Native::Void,
                        Err(value) => Native::Original(value),
                    }
                }
                _ => unreachable!(),
            };
            assert_eq!(
                native,
                expected(&row["result"], &error),
                "BITRUE-WS-SYMBOLS-NATIVE {group} case{index}"
            );
            let source_events: Vec<String> = serde_json::from_value(row["events"].clone()).unwrap();
            assert_eq!(
                *events.borrow(),
                source_events,
                "BITRUE-WS-SYMBOLS-NATIVE-EFFECTS {group} case{index}"
            );
            compared += 1;
        }
    }
    // Every native assertion precedes independent inventories/ingress assertions.
    assert_eq!(compared, 56, "WS-SYMBOLS independent native floor");
    assert_eq!(rows.len(), 56, "WS-SYMBOLS independent source floor");
    for (index, row) in rows.iter().take(20).enumerate() {
        assert_eq!(row["group"], "convert");
        assert_eq!(row["id"], format!("symbol{index}"));
        assert_eq!(units(&row["input"]), symbol(index));
    }
    for (index, id) in TOPICS.iter().enumerate() {
        assert_eq!(rows[20 + index]["group"], "topic");
        assert_eq!(rows[20 + index]["id"], *id);
    }
    for (index, id) in OPENS.iter().enumerate() {
        assert_eq!(rows[44 + index]["group"], "open");
        assert_eq!(rows[44 + index]["id"], *id);
        assert_eq!(
            rows[44 + index]["guard"].as_bool(),
            Some(matches!(*id, "disposed" | "truthy-string"))
        );
    }
    println!("WS-SYMBOLS floor: 56 native cases; last open case11 promise");
}
