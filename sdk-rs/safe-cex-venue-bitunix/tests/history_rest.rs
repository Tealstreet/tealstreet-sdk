use safe_cex_num::JsJsonValue;
use safe_cex_types::MarginType;
use safe_cex_venue_bitunix::history_rest as rest;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn undef() -> Value {
    json!({"undefined":true})
}
fn number(v: f64) -> Value {
    json!({"bits":format!("{:016x}",if v.is_nan(){0x7ff8_0000_0000_0000}else{v.to_bits()})})
}
fn bits(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v["bits"].as_str().unwrap(), 16).unwrap())
}
fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => undef(),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => json!(v),
        JsJsonValue::Number(v) => number(*v),
        JsJsonValue::String(v) => json!(v),
        JsJsonValue::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("Unexpected BigInt"),
    }
}
fn vobs(v: &Value) -> Value {
    obs(&JsJsonValue::from(v.clone()))
}
fn margin(v: &MarginType) -> Value {
    json!(match v {
        MarginType::Isolated => "isolated",
        MarginType::Cross => "cross",
    })
}
fn settle<T>(f: impl Future<Output = T>) -> T {
    match pin!(f).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("RREST fixture pending"),
    }
}
#[test]
fn actual_typescript_history_rest() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/history-rest-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "RREST actual source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let c: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = c["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 35, "RREST source floor");
    let mut counts = [0; 3];
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let f = &row["fixture"];
        let events = RefCell::new(vec![]);
        let note = |name: &str, args: Vec<Value>| -> Result<(), String> {
            let mut e = vec![json!(name)];
            e.extend(args);
            let e = json!(e);
            let i = events.borrow().len();
            assert_eq!(
                e, row["events"][i],
                "RREST native effect {name}:{i} in {}",
                row["name"]
            );
            events.borrow_mut().push(e);
            if f["fail"] == name {
                Err(format!("{name} failed"))
            } else {
                Ok(())
            }
        };
        let get = |endpoint: &str, config: JsJsonValue| {
            ready(
                note("get", vec![json!(endpoint), obs(&config)]).and_then(|()| {
                    if f["getFail"] == true {
                        Err(if f["missingMessage"] == true {
                            String::new()
                        } else {
                            "get failed".into()
                        })
                    } else {
                        Ok(row["data"].clone())
                    }
                }),
            )
        };
        let result: Result<Value, String> = match name {
            "fetchOHLCV" => {
                counts[0] += 1;
                let options = f.get("options").cloned().unwrap_or(json!({}));
                let n = |key| {
                    options.get(key).map(|v| {
                        v.as_f64().unwrap_or_else(|| {
                            f64::from_bits(
                                u64::from_str_radix(v["$bits"].as_str().unwrap(), 16).unwrap(),
                            )
                        })
                    })
                };
                let leaf = RefCell::new(0);
                let next = || {
                    let i = *leaf.borrow();
                    *leaf.borrow_mut() += 1;
                    bits(&row["leaves"][i]["value"])
                };
                settle(rest::fetch_ohlcv(rest::OhlcvReadOptions{symbol:options["symbol"].as_str().unwrap_or("A").into(),interval:options["interval"].as_str().unwrap_or("1m").into(),limit:n("limit"),from:n("from"),to:n("to")},
     |v|{note("date",vec![v.map_or_else(undef,number)])?;Ok(next())},
     |_end,amount,unit|{note("subtract",vec![number(amount),unit.map_or_else(undef,|s|json!(s))])?;Ok(next())},
     |date|{note("valueOf",vec![number(*date)])?;Ok(*date)},get)).map(|candles|json!(candles.iter().map(|c|json!({"entries":[["timestamp",number(c.timestamp)],["open",number(c.open)],["high",number(c.high)],["low",number(c.low)],["close",number(c.close)],["volume",number(c.volume)]]})).collect::<Vec<_>>()))
            }
            "updateSymbolMetadata" => {
                counts[1] += 1;
                let original = vec![
                    Rc::new(json!({"entries":[["id","a"],["symbol","A"],["leverage",number(1.)]]})),
                    Rc::new(json!({"entries":[["id","b"],["symbol","B"],["leverage",number(2.)]]})),
                ];
                let updated = RefCell::new(None);
                let result = settle(rest::update_symbol_metadata(
                    "A",
                    get,
                    |symbol, m, lev| {
                        note(
                            "set",
                            vec![
                                json!(symbol),
                                json!({"entries":[["marginType",margin(&m)],["leverage",lev.as_ref().map_or_else(undef,vobs)]]}),
                            ],
                        )
                    },
                    || {
                        note("positions", vec![])?;
                        Ok(original.clone())
                    },
                    |p: &Rc<Value>, symbol| Ok(p["entries"][1][1] == symbol),
                    |p, lev, m| {
                        let mut entries = p["entries"].as_array().unwrap().clone();
                        entries[2] = json!(["leverage", lev.as_ref().map_or_else(undef, vobs)]);
                        entries.push(json!(["marginType", margin(&m)]));
                        Ok(Rc::new(json!({"entries":entries})))
                    },
                    |positions| {
                        note(
                            "update",
                            vec![
                                json!({"entries":[["positions",positions.iter().map(|p|p.as_ref()).collect::<Vec<_>>()]]}),
                            ],
                        )?;
                        *updated.borrow_mut() = Some(positions);
                        Ok(())
                    },
                ));
                if let Some(p) = updated.borrow().as_ref() {
                    assert!(
                        !Rc::ptr_eq(&p[0], &original[0]),
                        "RREST matching position copied"
                    );
                    assert!(
                        Rc::ptr_eq(&p[1], &original[1]),
                        "RREST unchanged position identity"
                    );
                    assert_eq!(row["identity"], json!([false, true]));
                }
                result.map(|()| undef())
            }
            "validateAccount" => {
                counts[2] += 1;
                let balance = Rc::new(json!({"total":12.5,"free":3}));
                let result = settle(rest::validate_account(
                    get,
                    |body| note("extract", vec![vobs(body)]),
                    String::from,
                    |e| {
                        Ok(if e.is_empty() {
                            JsJsonValue::Undefined
                        } else {
                            JsJsonValue::String(e.clone())
                        })
                    },
                    |message| note("report", vec![obs(message)]),
                    || ready(note("balance", vec![]).map(|()| balance.clone())),
                ));
                result.map(|out| {
                    if f["fail"] == "balance" {
                        assert_eq!(
                            out.balance_error.as_deref(),
                            Some("balance failed"),
                            "RREST balance rejection retained"
                        );
                    } else {
                        assert!(out.balance_error.is_none());
                    }
                    let mut entries = vec![json!(["error", obs(&out.error)])];
                    if out.includes_balance {
                        if let Some(b) = &out.balance {
                            assert!(Rc::ptr_eq(b, &balance), "RREST balance identity");
                        }
                        entries.push(json!([
                            "balance",
                            out.balance.as_ref().map_or_else(undef, |v| vobs(v))
                        ]));
                    }
                    json!({"entries":entries})
                })
            }
            _ => panic!("Unknown REST read"),
        };
        assert_eq!(
            result.as_ref().err().map(String::as_str),
            row["error"].get("message").and_then(Value::as_str),
            "RREST native error"
        );
        if let Ok(r) = result {
            assert_eq!(r, row["result"], "RREST native result {name}");
        }
        assert_eq!(
            events.borrow().len(),
            row["events"].as_array().unwrap().len(),
            "RREST native event count {name}"
        );
    }
    assert_eq!(counts, [15, 10, 10], "RREST family floors");
}
