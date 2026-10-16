use safe_cex_venue_bitunix::websocket_public as ws;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command};
fn obs(v: &Value) -> Value {
    match v {
        Value::Number(n) => json!({"bits":format!("{:016x}",n.as_f64().unwrap().to_bits())}),
        Value::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), obs(v))).collect()),
        _ => v.clone(),
    }
}
fn number(v: &Value, default: f64) -> f64 {
    if v["$number"] == "NaN" {
        f64::NAN
    } else if v["$number"] == "Infinity" {
        f64::INFINITY
    } else {
        v.as_f64().unwrap_or(default)
    }
}
fn native_number(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
#[test]
fn actual_typescript_public_subscriptions() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/public-subscriptions-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "SUB actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(
        corpus["utf16sets"].as_array().unwrap().len(),
        2,
        "SUB UTF16 source floor"
    );
    for row in corpus["utf16sets"].as_array().unwrap() {
        let input: Vec<Vec<u16>> = serde_json::from_value(row["input"].clone()).unwrap();
        ws::set_market_symbols(Some(input.into_iter().map(Ok::<_, String>)), |set| {
            assert_eq!(
                json!(set.unwrap()),
                row["output"],
                "SUB native full UTF16 set"
            );
            Ok(())
        })
        .unwrap();
    }
    assert_eq!(rows.len(), 49, "SUB independent case count");
    let mut counts = [0; 6];
    for (index, row) in rows.iter().enumerate() {
        let fixture: Value = serde_json::from_str(row["fixture"].as_str().unwrap()).unwrap();
        let events = RefCell::new(Vec::<Value>::new());
        let note = |name: &str, args: Vec<Value>| -> Result<(), String> {
            let mut event = vec![json!(name)];
            event.extend(args);
            let e = json!(event);
            let i = events.borrow().len();
            assert_eq!(
                e, row["events"][i],
                "SUB native effect {}:{i} case {index}",
                row["name"]
            );
            events.borrow_mut().push(e);
            if fixture["fail"] == name {
                Err(format!("{name} failed"))
            } else {
                Ok(())
            }
        };
        let initial_topics=fixture.get("topics").cloned().unwrap_or(json!({"{\"symbol\":\"X\",\"ch\":\"ticker\"}":0,"{\"symbol\":\"Y\",\"ch\":\"depth_book1\"}":0,"{\"symbol\":\"Z\",\"ch\":\"price\"}":-1}));
        let topics = RefCell::new(
            initial_topics
                .as_object()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), number(v, 0.)))
                .collect::<Vec<_>>(),
        );
        let queue = RefCell::new(Vec::<Value>::new());
        let symbols = RefCell::new(fixture.get("symbols").cloned().unwrap_or(Value::Null));
        let markets = fixture
            .get("markets")
            .cloned()
            .unwrap_or(json!([{"symbol":"A"},{"symbol":"B"},{"symbol":"C"}]));
        let markets = markets.as_array().unwrap().clone();
        let allowed = |m: &Value| -> Result<bool, String> {
            Ok(fixture
                .get("filter")
                .is_none_or(|f| f.as_array().unwrap().contains(&m["symbol"])))
        };
        let track = |arg: Value, count: f64| -> Result<(), String> {
            note("frame", vec![obs(&arg)])?;
            let key = serde_json::to_string(&arg).unwrap();
            let mut topics = topics.borrow_mut();
            if let Some(v) = topics.iter_mut().find(|(k, _)| *k == key) {
                v.1 = count
            } else {
                topics.push((key, count));
            }
            Ok(())
        };
        let result = match row["name"].as_str().unwrap() {
            "setMarketSymbols" => {
                counts[0] += 1;
                let input = fixture.get("symbols").map(|s| {
                    s.as_array()
                        .unwrap()
                        .iter()
                        .map(|s| Ok(s.as_str().unwrap().encode_utf16().collect::<Vec<_>>()))
                        .collect::<Vec<Result<_, String>>>()
                });
                ws::set_market_symbols(input, |set| {
                    let value = set.map_or(Value::Null, |set| {
                        json!(
                            set.into_iter()
                                .map(|units| String::from_utf16(&units).unwrap())
                                .collect::<Vec<_>>()
                        )
                    });
                    note("assign", vec![value.clone()])?;
                    *symbols.borrow_mut() = value;
                    Ok(())
                })
            }
            "enqueueSubscriptions" => {
                counts[1] += 1;
                let args = fixture["args"].as_array().unwrap();
                let batch = number(&fixture["batch"], 20.);
                ws::enqueue_subscriptions(
                    fixture.get("batch").map(|_| batch),
                    || Ok(args.len() as f64),
                    |start, end| {
                        let slice_index = |v: f64| {
                            if v.is_nan() {
                                0
                            } else {
                                v.trunc().clamp(0., args.len() as f64) as usize
                            }
                        };
                        let start = slice_index(start);
                        let end = slice_index(end);
                        Ok(json!(if start < end {
                            args[start..end].to_vec()
                        } else {
                            vec![]
                        }))
                    },
                    |a| {
                        note("push", vec![obs(&a)])?;
                        queue.borrow_mut().push(a);
                        Ok(())
                    },
                    || Ok(number(&fixture["timer"], 0.) != 0.),
                    || note("drain", vec![]),
                )
            }
            "subscribe" => {
                counts[2] += 1;
                ws::subscribe(
                    || Ok(topics.borrow().iter().map(|(k, _)| k.clone()).collect()),
                    |k: &String| {
                        let v: Value = serde_json::from_str(k).map_err(|_| "syntax".to_string())?;
                        Ok(v["ch"].as_str().unwrap().starts_with("depth_"))
                    },
                    |k: &String| serde_json::from_str(k).map_err(|_| "syntax".to_string()),
                    |a: Vec<Value>, b| {
                        note(
                            "enqueue",
                            vec![obs(&json!(a)), b.map_or(Value::Null, native_number)],
                        )
                    },
                )
            }
            "subscribeMarketsToChannel" => {
                counts[3] += 1;
                ws::subscribe_markets_to_channel(
                    fixture["channel"].as_str().unwrap_or("ticker"),
                    || Ok(markets.clone()),
                    allowed,
                    |m| Ok(m["symbol"].clone()),
                    |s, c| Ok(json!({"symbol":s,"ch":c})),
                    track,
                )
            }
            "subscribeToBook" => {
                counts[4] += 1;
                ws::subscribe_to_book(
                    "BOOK",
                    "depth_book5",
                    |s, c| Ok(json!({"symbol":s,"ch":c})),
                    track,
                    |args, b| note("enqueue", vec![obs(&json!(args)), native_number(b)]),
                )
            }
            "onMarketsChanged" => {
                counts[5] += 1;
                ws::on_markets_changed(
                    || Ok(!markets.is_empty()),
                    || Ok(fixture["noSocket"] != true),
                    || Ok(fixture["disposed"] == true),
                    || Ok(fixture["secondary"] == true),
                    || Ok(markets.clone()),
                    allowed,
                    |m, c| Ok(json!({"symbol":m["symbol"],"ch":c})),
                    |a, n| track(a.clone(), n),
                    |args| note("enqueue", vec![obs(&json!(args)), Value::Null]),
                )
            }
            _ => panic!(),
        };
        if row["error"].is_null() {
            assert!(result.is_ok(), "SUB native resolution {index}: {result:?}");
        } else {
            let error = result.unwrap_err();
            if error == "syntax" {
                assert_eq!(
                    row["error"]["name"], "SyntaxError",
                    "SUB native JSON parse rejection"
                );
            } else {
                assert_eq!(
                    json!({"name":"Error","message":error}),
                    row["error"],
                    "SUB native rejection {index}"
                );
            }
        }
        assert_eq!(
            json!(*events.borrow()),
            row["events"],
            "SUB native event count {index}"
        );
        assert_eq!(
            obs(&json!(*queue.borrow())),
            row["queue"],
            "SUB native batches {index}"
        );
        assert_eq!(
            *symbols.borrow(),
            row["symbols"],
            "SUB native ordered set {index}"
        );
        assert_eq!(
            json!(
                topics
                    .borrow()
                    .iter()
                    .map(|(k, n)| json!([k, native_number(*n)]))
                    .collect::<Vec<_>>()
            ),
            row["topics"],
            "SUB native tracked topics {index}"
        );
    }
    assert_eq!(counts, [5, 24, 5, 5, 2, 8], "SUB independent method totals");
}
