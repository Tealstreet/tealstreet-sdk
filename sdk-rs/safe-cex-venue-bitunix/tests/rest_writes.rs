use safe_cex_num::{JsJsonValue, js_truthy, numeric_ingress::js_from_str};
use safe_cex_types::{Market, Position};
use safe_cex_venue_bitunix::rest_writes::{
    cancel_symbol_orders, change_margin_type, change_position_mode, place_orders, set_leverage,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Debug)]
struct Error {
    message: String,
    id: Option<&'static str>,
    exchange_message: Option<Value>,
}
impl From<String> for Error {
    fn from(message: String) -> Self {
        Self {
            message,
            id: None,
            exchange_message: None,
        }
    }
}
fn failure(message: &str, id: &'static str) -> Error {
    Error {
        message: message.into(),
        id: Some(id),
        exchange_message: None,
    }
}
fn resolve<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("WRITE mocked futures must be ready"),
    }
}
fn text(v: &str) -> JsJsonValue {
    JsJsonValue::String(v.into())
}
fn observation(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => json!({"type":"undefined"}),
        JsJsonValue::Null => json!({"type":"null"}),
        JsJsonValue::Bool(value) => json!({"type":"boolean","value":value}),
        JsJsonValue::Number(value) => {
            json!({"type":"number","bits":format!("{:016x}",if value.is_nan(){0x7ff8_0000_0000_0000}else{value.to_bits()})})
        }
        JsJsonValue::String(value) => string_units(&value.encode_utf16().collect::<Vec<_>>()),
        JsJsonValue::Array(values) => {
            json!({"type":"array","items":values.iter().map(observation).collect::<Vec<_>>()})
        }
        JsJsonValue::Object(values) => {
            json!({"type":"object","entries":values.iter().map(|(k,v)|json!([k,observation(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("unexpected witness BigInt"),
    }
}
fn string_units(units: &[u16]) -> Value {
    json!({"type":"string","units":units})
}
fn bits(fixture: &Value, key: &str, default: f64) -> f64 {
    fixture[key].as_str().map_or(default, |v| {
        f64::from_bits(u64::from_str_radix(v, 16).unwrap())
    })
}
fn flag(fixture: &Value, key: &str) -> bool {
    fixture[key].as_bool() == Some(true)
}
fn record(events: &RefCell<Vec<Value>>, row: &Value, values: Vec<JsJsonValue>) {
    record_observed(events, row, values.iter().map(observation).collect());
}
fn record_observed(events: &RefCell<Vec<Value>>, row: &Value, values: Vec<Value>) {
    let index = events.borrow().len();
    let native = Value::Array(values);
    assert_eq!(
        native, row["events"][index],
        "WRITE native effect {}:{index}",
        row["method"]
    );
    events.borrow_mut().push(native);
}
fn emit(events: &RefCell<Vec<Value>>, row: &Value, message: JsJsonValue) -> Result<(), Error> {
    record(events, row, vec![text("emit"), message]);
    if flag(&row["fixture"], "emitFail") {
        Err(failure("emit failed", "emit"))
    } else {
        Ok(())
    }
}
fn error_message(error: &Error) -> JsJsonValue {
    if js_truthy(error.exchange_message.as_ref()) {
        JsJsonValue::from(error.exchange_message.clone().unwrap())
    } else {
        text(&error.message)
    }
}
fn post_error(fixture: &Value) -> Error {
    Error {
        message: "transport failed".into(),
        id: Some("post"),
        exchange_message: Some(if flag(fixture, "falseMessage") {
            json!(false)
        } else {
            json!("venue failed")
        }),
    }
}
fn error_observation(error: &Error) -> Value {
    json!({"name":"Error","message":error.message.encode_utf16().collect::<Vec<_>>(),"id":error.id})
}
#[test]
fn actual_typescript_rest_writes() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/rest-writes-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WRITE actual source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 44, "WRITE independent source case total");
    let expected_events = [
        2, 2, 1, 2, 2, 3, 1, 2, 1, 1, 1, 1, 1, 2, 2, 1, 2, 2, 4, 4, 4, 4, 4, 2, 2, 4, 5, 4, 4, 1,
        4, 3, 1, 3, 3, 2, 6, 1, 2, 4, 6, 8, 7, 6,
    ];
    let mut counts = [0; 5];
    let mut surrogate_cases = 0;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row["events"].as_array().unwrap().len(),
            expected_events[index],
            "WRITE independent source effect count {index}"
        );
        let fixture = &row["fixture"];
        let events = RefCell::new(Vec::new());
        let ack = Rc::new(json!({"ack":true}));
        let mut returned_ack = None;
        let result: Result<(), Error> = match row["method"].as_str().unwrap() {
            "changePositionMode" => {
                counts[0] += 1;
                let contracts: Vec<Result<f64, Error>> = fixture["contractsBits"]
                    .as_array()
                    .map_or_else(Vec::new, |values| {
                        values
                            .iter()
                            .map(|v| {
                                Ok(f64::from_bits(
                                    u64::from_str_radix(v.as_str().unwrap(), 16).unwrap(),
                                ))
                            })
                            .collect()
                    });
                resolve(change_position_mode(
                    flag(fixture, "hedged"),
                    contracts,
                    |url, body| {
                        record(&events, row, vec![text("post"), text(url), body]);
                        ready(if flag(fixture, "postFail") {
                            Err(post_error(fixture))
                        } else {
                            Ok(())
                        })
                    },
                    |key, value| {
                        record(
                            &events,
                            row,
                            vec![text("setting"), text(key), JsJsonValue::Bool(value)],
                        );
                        if flag(fixture, "settingFail") {
                            Err(failure("setting failed", "setting"))
                        } else {
                            Ok(())
                        }
                    },
                    |message| emit(&events, row, text(message)),
                    |error| emit(&events, row, error_message(error)),
                ))
            }
            "changeMarginType" => {
                counts[1] += 1;
                let symbol: Option<Vec<u16>> = fixture
                    .get("symbolUnits")
                    .map(|v| serde_json::from_value(v.clone()).unwrap());
                resolve(change_margin_type(
                    flag(fixture, "isolated"),
                    symbol.as_deref(),
                    |url, units, mode, coin| {
                        assert_eq!(
                            units,
                            symbol.as_deref().unwrap(),
                            "WRITE native margin symbol units"
                        );
                        if units == [0xd800] {
                            surrogate_cases += 1;
                        }
                        let body = json!({"type":"object","entries":[["symbol",string_units(units)],["marginMode",observation(&text(mode))],["marginCoin",observation(&text(coin))]]});
                        record_observed(
                            &events,
                            row,
                            vec![observation(&text("post")), observation(&text(url)), body],
                        );
                        ready(if flag(fixture, "postFail") {
                            Err(post_error(fixture))
                        } else {
                            Ok(())
                        })
                    },
                    |message| {
                        emit(&events, row, text(message))?;
                        Ok(Error::from(message.to_owned()))
                    },
                    |error| {
                        let message = error_message(error);
                        emit(&events, row, message.clone())?;
                        let JsJsonValue::String(message) = message else {
                            panic!("WRITE fixture message must be string")
                        };
                        Ok(Error::from(message))
                    },
                ))
            }
            "setLeverage" => {
                counts[2] += 1;
                let mut market:Market=js_from_str(r#"{"id":"BTC/USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}}"#).unwrap();
                market.limits.leverage.min = bits(fixture, "minBits", 1.0);
                market.limits.leverage.max = bits(fixture, "maxBits", 100.0);
                let mut position:Position=js_from_str(r#"{"id":"BTCUSDT:long","symbol":"BTCUSDT","side":"long","entryPrice":0,"notional":0,"leverage":5,"unrealizedPnl":0,"contracts":0,"liquidationPrice":0}"#).unwrap();
                position.leverage = bits(fixture, "currentBits", 5.0);
                resolve(set_leverage(
                    "BTCUSDT",
                    bits(fixture, "inputBits", 5.0),
                    (
                        |_| {
                            record(&events, row, vec![text("market lookup")]);
                            Ok((!flag(fixture, "noMarket")).then_some(&market))
                        },
                        |_| {
                            record(&events, row, vec![text("position lookup")]);
                            Ok((!flag(fixture, "noPosition")).then_some(&position))
                        },
                    ),
                    |url, body| {
                        record(&events, row, vec![text("post"), text(url), body]);
                        ready(if flag(fixture, "postFail") {
                            Err(post_error(fixture))
                        } else {
                            Ok(())
                        })
                    },
                    |updates| {
                        let values = updates
                            .into_iter()
                            .map(|(id, leverage)| {
                                JsJsonValue::Array(vec![
                                    JsJsonValue::Object(vec![("id".into(), text(&id))]),
                                    JsJsonValue::Object(vec![(
                                        "leverage".into(),
                                        JsJsonValue::Number(leverage),
                                    )]),
                                ])
                            })
                            .collect();
                        record(
                            &events,
                            row,
                            vec![text("updates"), JsJsonValue::Array(values)],
                        );
                        if flag(fixture, "updateFail") {
                            Err(failure("update failed", "update"))
                        } else {
                            Ok(())
                        }
                    },
                    |error| emit(&events, row, error_message(error)),
                ))
            }
            "cancelSymbolOrders" => {
                counts[3] += 1;
                let symbol: Vec<u16> =
                    serde_json::from_value(fixture["symbolUnits"].clone()).unwrap();
                let items = RefCell::new(
                    fixture["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(Value::as_u64)
                        .collect::<Vec<_>>(),
                );
                let native = resolve(cancel_symbol_orders(
                    &symbol,
                    || Ok(items.borrow().len()),
                    |i| Ok(items.borrow()[i]),
                    |id, expected| {
                        record(
                            &events,
                            row,
                            vec![text("symbol"), JsJsonValue::Number(*id as f64)],
                        );
                        if fixture["getterFail"].as_u64() == Some(*id) {
                            return Err(failure("getter failed", "getter"));
                        }
                        if flag(fixture, "deleteOnGet") && *id == 0 {
                            items.borrow_mut()[1] = None;
                        }
                        let units: Vec<u16> = if *id == 1 {
                            "ETHUSDT".encode_utf16().collect()
                        } else if flag(fixture, "surrogate") {
                            vec![0xd800]
                        } else {
                            "BTCUSDT".encode_utf16().collect()
                        };
                        Ok(units == *expected)
                    },
                    |selected| {
                        record(
                            &events,
                            row,
                            vec![
                                text("cancel"),
                                JsJsonValue::Array(
                                    selected
                                        .into_iter()
                                        .map(|v| JsJsonValue::Number(v as f64))
                                        .collect(),
                                ),
                            ],
                        );
                        ready(if flag(fixture, "cancelFail") {
                            Err(failure("cancel failed", "cancel"))
                        } else {
                            Ok(ack.clone())
                        })
                    },
                ));
                native.map(|value| returned_ack = Some(value))
            }
            "placeOrders" => {
                counts[4] += 1;
                let items = RefCell::new(
                    fixture["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(Value::as_u64)
                        .collect::<Vec<_>>(),
                );
                let native = resolve(place_orders(
                    || {
                        record(&events, row, vec![text("assert")]);
                        if flag(fixture, "assertFail") {
                            Err(failure("assert failed", "assert"))
                        } else {
                            Ok(())
                        }
                    },
                    || Ok(items.borrow().len()),
                    |i| Ok(items.borrow()[i]),
                    |id| {
                        record(
                            &events,
                            row,
                            vec![
                                text("hydrate"),
                                id.map_or(JsJsonValue::Undefined, |v| {
                                    JsJsonValue::Number(v as f64)
                                }),
                            ],
                        );
                        let result = if id.is_some() && fixture["hydrateFail"].as_u64() == id {
                            Err(failure("hydrate failed", "hydrate"))
                        } else {
                            if flag(fixture, "appendHydrate") && id == Some(0) {
                                items.borrow_mut().push(Some(2));
                            }
                            Ok(())
                        };
                        ready(result)
                    },
                    |id| {
                        record(
                            &events,
                            row,
                            vec![text("format"), JsJsonValue::Number(id as f64)],
                        );
                        if fixture["formatFail"].as_u64() == Some(id) {
                            return Err(failure("format failed", "format"));
                        }
                        if flag(fixture, "appendFormat") && id == 0 {
                            items.borrow_mut().push(Some(2));
                        }
                        Ok((0..=id).map(|i| format!("{id}:{i}")).collect::<Vec<_>>())
                    },
                    |requests| {
                        record(
                            &events,
                            row,
                            vec![
                                text("batch"),
                                JsJsonValue::Array(requests.iter().map(|v| text(v)).collect()),
                            ],
                        );
                        ready(if flag(fixture, "batchFail") {
                            Err(failure("batch failed", "batch"))
                        } else {
                            Ok(ack.clone())
                        })
                    },
                ));
                native.map(|value| returned_ack = Some(value))
            }
            name => panic!("unexpected method {name}"),
        };
        match result {
            Ok(()) => {
                assert!(row["error"].is_null(), "WRITE native resolution");
                if let Some(value) = returned_ack.as_ref() {
                    assert!(
                        Rc::ptr_eq(value, &ack),
                        "WRITE native acknowledgement identity"
                    );
                    assert_eq!(row["returned"]["type"], "object", "WRITE source ack type");
                } else {
                    assert_eq!(
                        row["returned"]["type"], "undefined",
                        "WRITE source void resolution"
                    );
                }
            }
            Err(error) => assert_eq!(
                error_observation(&error),
                row["error"],
                "WRITE native rejection {index}"
            ),
        }
        assert_eq!(
            returned_ack.is_some(),
            row["ackIdentity"].as_bool().unwrap(),
            "WRITE acknowledgement identity outcome"
        );
        assert_eq!(
            events.borrow().len(),
            expected_events[index],
            "WRITE independent native effect count {index}"
        );
    }
    assert_eq!(
        counts,
        [8, 9, 11, 7, 9],
        "WRITE independent native method totals"
    );
    assert_eq!(
        surrogate_cases, 1,
        "WRITE native lone-surrogate margin symbol retained"
    );
}
