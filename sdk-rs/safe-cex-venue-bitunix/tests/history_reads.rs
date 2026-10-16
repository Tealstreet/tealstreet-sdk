use safe_cex_num::{js_number, js_number_string};
use safe_cex_venue_bitunix::history_reads as h;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Error = Rc<()>;
#[derive(Clone)]
struct Row {
    id: usize,
    fields: Value,
}
type Record = Rc<Value>;
#[derive(Clone)]
enum Body {
    Response(usize),
    Object(Vec<Rc<Row>>),
    Array(Vec<Rc<Row>>),
    Missing,
}
fn undef() -> Value {
    json!({"undefined":true})
}
fn text(v: &str) -> Value {
    json!({"units":v.encode_utf16().collect::<Vec<_>>()})
}
fn units(v: &Value) -> Vec<u16> {
    serde_json::from_value(v["units"].clone()).unwrap()
}
fn num(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
fn number(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn prop(v: &Value, k: &str) -> Value {
    v["entries"]
        .as_array()
        .and_then(|a| a.iter().find(|e| e[0] == k))
        .map_or_else(undef, |e| e[1].clone())
}
fn truth(v: &Value) -> bool {
    if v.is_null() || v == &undef() || v == false {
        false
    } else if v.get("bits").is_some() {
        let n = number(v);
        n != 0.0 && !n.is_nan()
    } else if v.get("units").is_some() {
        !units(v).is_empty()
    } else {
        true
    }
}
fn string(v: &Value) -> Vec<u16> {
    if v.get("units").is_some() {
        units(v)
    } else if v.get("bits").is_some() {
        js_number_string(number(v)).encode_utf16().collect()
    } else if v == &undef() {
        "undefined".encode_utf16().collect()
    } else if v.is_null() {
        "null".encode_utf16().collect()
    } else if let Some(b) = v.as_bool() {
        b.to_string().encode_utf16().collect()
    } else {
        "[object Object]".encode_utf16().collect()
    }
}
fn obs(v: &Value) -> Value {
    match v {
        Value::Number(n) => num(n.as_f64().unwrap()),
        Value::String(s) => text(s),
        Value::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        Value::Object(o) if o.contains_key("$bits") => json!({"bits":v["$bits"]}),
        Value::Object(o) if o.contains_key("$units") => json!({"units":v["$units"]}),
        Value::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        _ => v.clone(),
    }
}
fn option(v: Option<&Value>) -> Value {
    v.map_or_else(undef, obs)
}
fn optnum(v: &Value) -> Option<f64> {
    if v == &undef() || v.is_null() {
        None
    } else {
        Some(number(v))
    }
}
fn settle<T>(f: impl Future<Output = T>) -> T {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("HREAD fixture future must be ready"),
    }
}
fn request<S: AsRef<Value>>(r: h::HistoryPageRequest<S>, orders: bool) -> Value {
    let mut fields = vec![];
    if let Some(s) = r.symbol {
        fields.push(json!(["symbol", s.as_ref()]));
    }
    if let Some(v) = r.start_time {
        fields.push(json!(["startTime", num(v)]));
    }
    if let Some(v) = r.end_time {
        fields.push(json!(["endTime", num(v)]));
    }
    fields.push(json!(["limit", num(r.limit)]));
    if !orders {
        if let Some(v) = r.skip {
            fields.push(json!(["skip", num(v)]));
        }
    }
    json!({"entries":[["params",{"entries":fields}]]})
}
// clippy::possible_missing_else is a false positive on these minified harness
// lines: the flagged `}if` and `}{` are a guard-clause chain of early returns
// followed by a scoped block, not a dropped `else`. Inserting one would change
// control flow the source does not have.
#[allow(clippy::possible_missing_else)]
fn actual_typescript_history_reads(family: &str) {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/history-reads-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg(family)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "HREAD actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    let expected = match family {
        "fetchBitunixHistoryPages" => 22,
        "fetchHistoricalOrders" => 17,
        _ => 11,
    };
    assert_eq!(rows.len(), expected, "HREAD source floor");
    let mut counts = [0; 6];
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let f = &row["fixture"];
        let events = RefCell::new(Vec::<Value>::new());
        let token = Rc::new(());
        let raw: Vec<Rc<Row>> = row["raw"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                Rc::new(Row {
                    id: r["id"].as_u64().unwrap() as usize,
                    fields: r["row"].clone(),
                })
            })
            .collect();
        let mapped = RefCell::new(Vec::<Record>::new());
        let fetch_count = RefCell::new(0usize);
        let note = |name: &str, args: Vec<Value>| -> Result<(), Error> {
            let mut e = vec![json!(name)];
            e.extend(args);
            let e = json!(e);
            let i = events.borrow().len();
            assert_eq!(
                e, row["events"][i],
                "HREAD native effect {name}:{i} in {}",
                row["name"]
            );
            events.borrow_mut().push(e);
            if f["fail"] == name {
                Err(token.clone())
            } else {
                Ok(())
            }
        };
        let report = |e: Error| {
            assert!(Rc::ptr_eq(&e, &token), "HREAD original error identity");
            note("report", vec![text("venue error")])
        };
        let options = f.get("options").cloned().unwrap_or(json!({}));
        let get = |k: &str| {
            note("option", vec![text(k)])?;
            Ok(option(options.get(k)))
        };
        let public = || {
            note("public", vec![])?;
            Ok(f["public"] == true)
        };
        let page_rows = |p: usize| -> Vec<Rc<Row>> {
            let (offset, length) = if let Some(pages) = f["pages"].as_array() {
                (
                    pages
                        .iter()
                        .take(p)
                        .map(|v| v.as_array().unwrap().len())
                        .sum(),
                    pages.get(p).map_or(100, |v| v.as_array().unwrap().len()),
                )
            } else {
                let size = if f["full"] == true {
                    100
                } else {
                    f["size"].as_u64().unwrap_or(3) as usize
                };
                (p * size, size)
            };
            raw.iter().skip(offset).take(length).cloned().collect()
        };
        let fetch = |endpoint: &str, r: h::HistoryPageRequest<Record>| {
            let p = *fetch_count.borrow();
            *fetch_count.borrow_mut() += 1;
            let result = note(
                "get",
                vec![text(endpoint), request(r, name == "fetchHistoricalOrders")],
            )
            .and_then(|()| {
                if f["rejectAt"].as_u64() == Some(p as u64) {
                    Err(token.clone())
                } else {
                    Ok(Body::Response(p))
                }
            });
            ready(result)
        };
        let body = |b: Body| {
            note("body", vec![])?;
            let Body::Response(p) = b else {
                panic!("HREAD reply carrier")
            };
            let rows = page_rows(p);
            Ok(if f["shape"] == "array" {
                Body::Array(rows)
            } else {
                Body::Object(rows)
            })
        };
        let field = |b: &Body, k: &'static str| {
            note("field", vec![text(k)])?;
            let Body::Object(rows) = b else {
                return Ok(Body::Missing);
            };
            let matched = if f["shape"] == "none" {
                false
            } else if f["shape"] == "trade" {
                k == "tradeList"
            } else if f["shape"] == "order" {
                k == "orderList"
            } else {
                k == "positionList"
            };
            Ok(if matched {
                Body::Array(rows.clone())
            } else {
                Body::Missing
            })
        };
        let array = |b: &Body| matches!(b, Body::Array(_));
        let iterator = |b: Body| match b {
            Body::Array(rows) => Ok(rows),
            _ => Err(token.clone()),
        };
        let next = |l: &Vec<Rc<Row>>, i| Ok(l.get(i).cloned());
        let length = |l: &Vec<Rc<Row>>| Ok(l.len() as f64);
        let read = |r: &Rc<Row>, key: &'static str| {
            let v = prop(&r.fields, key);
            Ok(if v == undef() || v.is_null() {
                None
            } else {
                Some(string(&v))
            })
        };
        let selector_fetch = |ep: &'static str,
                              from: Option<f64>,
                              to: Option<f64>,
                              s: h::HistoryRowSelectors<Rc<Row>, Error>,
                              symbol: Option<Record>| {
            let result = (|| {
                note(
                    "delegate",
                    vec![
                        text(ep),
                        from.map_or_else(undef, num),
                        to.map_or_else(undef, num),
                        symbol.as_ref().map_or_else(undef, |s| (**s).clone()),
                    ],
                )?;
                assert!(
                    s.time_of as usize
                        == h::history_row_time::<Rc<Row>, Error> as *const () as usize,
                    "HREAD real callable time selector"
                );
                assert!(
                    s.id_of as usize == h::history_row_id::<Rc<Row>, Error> as *const () as usize,
                    "HREAD real callable id selector"
                );
                let rows = page_rows(0);
                for r in &rows {
                    let time = (s.time_of)(r, s.time_fields, &mut |r, k| read(r, k))?;
                    note("selectorTime", vec![num(r.id as f64), num(time)])?;
                    let id = (s.id_of)(r, s.id_field, &mut |r, k| read(r, k))?;
                    note("selectorId", vec![num(r.id as f64), json!({"units":id})])?;
                }
                Ok(rows)
            })();
            ready(result)
        };
        let mut mapped_results = vec![Value::Null; raw.len()];
        for result in row["mapped"].as_array().unwrap() {
            mapped_results[result["row"].as_u64().unwrap() as usize] = result["result"].clone();
        }
        let mapped_leaf = |r: &Rc<Row>| mapped_results[r.id].clone();
        let map = |kind: &str, r: Rc<Row>, args: Vec<Value>| {
            let mut values = vec![text(kind), num(r.id as f64)];
            values.extend(args);
            note("map", values)?;
            let result = mapped_leaf(&r);
            if result.is_null() {
                Ok(vec![])
            } else {
                let outputs = if kind == "income" {
                    result.as_array().unwrap().clone()
                } else {
                    vec![result]
                };
                let outputs: Vec<Record> = outputs.into_iter().map(Rc::new).collect();
                mapped.borrow_mut().extend(outputs.iter().cloned());
                Ok(outputs)
            }
        };
        let time = |r: &Record| {
            Ok(number(&prop(
                r,
                if name == "fetchClosedPnl" {
                    "closedTime"
                } else if name == "fetchHistoricalOrders" {
                    "createdTime"
                } else {
                    "timestamp"
                },
            )))
        };
        let params = |a, b| {
            let from = optnum(&get(a)?);
            let to = optnum(&get(b)?);
            let symbol = get("symbol")?;
            Ok((from, to, (symbol != undef()).then(|| Rc::new(symbol))))
        };
        let result: Result<Value, Error> = match name {
            "fetchBitunixHistoryPages" => {
                counts[0] += 1;
                let ep = "EP".encode_utf16().collect::<Vec<_>>();
                let start = optnum(&option(f.get("start")));
                let end = optnum(&option(f.get("end")));
                let symbol = f.get("symbol").map(|s| Rc::new(obs(s)));
                settle(h::fetch_bitunix_history_pages(
                    &ep,
                    start,
                    end,
                    symbol,
                    |ep, r| fetch(&String::from_utf16(ep).unwrap(), r),
                    (body, field, array),
                    (iterator, next, length),
                    |r| {
                        note("time", vec![num(r.id as f64)])?;
                        let v = prop(&r.fields, "ts");
                        if v != undef() && !v.is_null() {
                            Ok(number(&v))
                        } else {
                            let t = prop(&r.fields, "ctime");
                            let t = String::from_utf16_lossy(&string(&t));
                            Ok(js_number(Some(&Value::String(t))).unwrap())
                        }
                    },
                    |r| {
                        note("id", vec![num(r.id as f64)])?;
                        let id = prop(&r.fields, "id");
                        let id = if id != undef() && !id.is_null() {
                            id
                        } else {
                            prop(&r.fields, "tradeId")
                        };
                        Ok(if id == undef() || id.is_null() {
                            vec![]
                        } else {
                            string(&id)
                        })
                    },
                    |message, level| note("warning", vec![json!({"units":message}), text(level)]),
                    report,
                ))
                .map(|rows| {
                    for r in &rows {
                        assert!(Rc::ptr_eq(r, &raw[r.id]), "HREAD raw row identity");
                    }
                    json!(rows.iter().map(|r| r.id).collect::<Vec<_>>())
                })
            }
            "fetchClosedPositionRows" => {
                counts[1] += 1;
                settle(h::fetch_closed_position_rows(
                    public,
                    || params("from", "to"),
                    selector_fetch,
                ))
                .map(|rows| {
                    for r in &rows {
                        assert!(
                            Rc::ptr_eq(r, &raw[r.id]),
                            "HREAD delegated raw row identity"
                        );
                    }
                    json!(rows.iter().map(|r| r.id).collect::<Vec<_>>())
                })
            }
            "fetchIncomeHistory" => {
                counts[2] += 1;
                settle(h::fetch_income_history(
                    public,
                    || params("startTime", "endTime"),
                    selector_fetch,
                    |r| map("income", r, vec![]),
                    time,
                ))
                .map(|records| {
                    for r in &records {
                        assert!(
                            mapped.borrow().iter().any(|m| Rc::ptr_eq(r, m)),
                            "HREAD income record identity"
                        );
                    }
                    json!(records.iter().map(|r| r.as_ref()).collect::<Vec<_>>())
                })
            }
            "fetchClosedPnl" => {
                counts[3] += 1;
                settle(h::fetch_closed_pnl(
                    public,
                    || params("from", "to"),
                    selector_fetch,
                    || {
                        let account = get("accountName")?;
                        Ok(if truth(&account) { account } else { text("") })
                    },
                    |r, a| Ok(map("closed", r, vec![a])?.into_iter().next()),
                    time,
                ))
                .map(|records| {
                    for r in &records {
                        assert!(
                            mapped.borrow().iter().any(|m| Rc::ptr_eq(r, m)),
                            "HREAD pnl record identity"
                        );
                    }
                    json!(records.iter().map(|r| r.as_ref()).collect::<Vec<_>>())
                })
            }
            "fetchHistoricalPositions" => {
                counts[4] += 1;
                settle(h::fetch_historical_positions(||Ok((optnum(&get("from")?),optnum(&get("to")?))),|from,to|ready(note("closedRows",vec![json!({"entries":[["from",from.map_or_else(undef,num)],["to",to.map_or_else(undef,num)]]})]).map(|()|page_rows(0))),||get("account"),|symbol:Value|{note("quote",vec![symbol])?;Ok(if f["quote"].is_null()&&f.get("quote").is_some(){None}else{Some(string(&f.get("quote").map(obs).unwrap_or_else(||text("USD"))))})},|r,a,resolve|{note("map",vec![text("position"),num(r.id as f64),a])?;let leaf=mapped_leaf(&r);if leaf.is_null(){return Ok(None);}{let symbol=prop(&r.fields,"symbol");let q=resolve(symbol.clone())?;note("resolved",vec![symbol,json!({"units":q})])?;}let record=Rc::new(leaf);mapped.borrow_mut().push(record.clone());Ok(Some(record))})).map(|records|json!(records.iter().map(|r|r.as_ref()).collect::<Vec<_>>()))
            }
            "fetchHistoricalOrders" => {
                counts[5] += 1;
                settle(h::fetch_historical_orders(
                    public,
                    |k| Ok(optnum(&get(k)?)),
                    || {
                        let symbol = get("symbol")?;
                        Ok((symbol != undef()).then(|| Rc::new(symbol)))
                    },
                    fetch,
                    (
                        |d| {
                            note("body", vec![])?;
                            let Body::Response(p) = d else {
                                panic!("HREAD order body")
                            };
                            note("field", vec![text("orderList")])?;
                            note("body", vec![])?;
                            note("field", vec![text("orderList")])?;
                            Ok(page_rows(p))
                        },
                        next,
                        length,
                    ),
                    read,
                    |r| Ok(map("order", r, vec![])?.into_iter().next()),
                    time,
                    |m, l| note("warning", vec![text(m), text(l)]),
                    report,
                ))
                .map(|records| {
                    for r in &records {
                        assert!(
                            mapped.borrow().iter().any(|m| Rc::ptr_eq(r, m)),
                            "HREAD order record identity"
                        );
                    }
                    json!(records.iter().map(|r| r.as_ref()).collect::<Vec<_>>())
                })
            }
            _ => panic!("Unknown history family"),
        };
        assert_eq!(
            result.as_ref().err().map(|e| {
                assert!(Rc::ptr_eq(e, &token));
                json!({"opaque":true})
            }),
            if row["error"].is_null() {
                None
            } else {
                Some(row["error"].clone())
            },
            "HREAD native error"
        );
        if let Ok(result) = result {
            assert_eq!(result, row["result"], "HREAD native result {}", row["name"]);
        }
        assert_eq!(
            events.borrow().len(),
            row["events"].as_array().unwrap().len(),
            "HREAD native event count {}",
            row["name"]
        );
    }
    let index = match family {
        "fetchBitunixHistoryPages" => 0,
        "fetchClosedPositionRows" => 1,
        "fetchIncomeHistory" => 2,
        "fetchClosedPnl" => 3,
        "fetchHistoricalPositions" => 4,
        "fetchHistoricalOrders" => 5,
        _ => panic!("unknown family"),
    };
    let mut expected_counts = [0; 6];
    expected_counts[index] = expected;
    assert_eq!(counts, expected_counts, "HREAD family floors");
}

#[test]
fn actual_typescript_history_pager() {
    actual_typescript_history_reads("fetchBitunixHistoryPages");
}
#[test]
fn actual_typescript_closed_rows() {
    actual_typescript_history_reads("fetchClosedPositionRows");
}
#[test]
fn actual_typescript_income_history() {
    actual_typescript_history_reads("fetchIncomeHistory");
}
#[test]
fn actual_typescript_closed_pnl() {
    actual_typescript_history_reads("fetchClosedPnl");
}
#[test]
fn actual_typescript_historical_positions() {
    actual_typescript_history_reads("fetchHistoricalPositions");
}
#[test]
fn actual_typescript_historical_orders() {
    actual_typescript_history_reads("fetchHistoricalOrders");
}
