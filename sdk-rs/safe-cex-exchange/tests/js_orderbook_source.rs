//! Maintained orderbook utilities through the installed production native host.
#![cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_exchange::{
    js_runtime::{
        client, globals, library,
        scope::ScopeShared,
        value::{Js, Thrown, from_json},
    },
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::Arc,
};
type Trace = Rc<RefCell<Vec<String>>>;
fn input(value: &Value) -> Js {
    match value.as_str() {
        Some("$undefined") => Js::Undefined,
        Some("$nan") => Js::Number(f64::NAN),
        _ => from_json(value),
    }
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_orderbook_source.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn host(run: impl FnOnce(&Js)) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        let runtime = native_runtime();
        let http = client::rest_http_host(
            runtime.clone(),
            Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        );
        let shared = Rc::new(ScopeShared::new(
            runtime,
            http,
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            Js::object(vec![]),
            (),
        ));
        globals::install_builtins(&shared.globals, Rc::downgrade(&shared));
        library::install_library(&shared.globals, Rc::downgrade(&shared));
        run(&shared.globals);
    });
}
fn stringify(g: &Js, value: Js) -> String {
    g.get("JSON")
        .unwrap()
        .call_method("stringify", vec![value])
        .unwrap()
        .to_display()
}
#[test]
fn orderbook_updates_match_source_raw_comparisons_and_lazy_reads() {
    let source = source();
    let cases = source["updates"].as_array().unwrap();
    assert_eq!(cases.len(), 147);
    host(|g| {
        for row in cases {
            let s = &row["spec"];
            let trace: Trace = Rc::default();
            let levels = Js::holey_array(
                s["prices"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, price)| {
                        if price.is_null() {
                            return Some(Js::Null);
                        }
                        if price == "$hole" {
                            return None;
                        }
                        let level =
                            Js::object(vec![("amount", Js::Number(1.)), ("total", Js::Number(0.))]);
                        let log = trace.clone();
                        let price = input(price);
                        level
                            .define_accessor(
                                "price",
                                Some(Rc::new(move |_| {
                                    log.borrow_mut().push(format!("level:{i}"));
                                    Ok(price.clone())
                                })),
                                None,
                            )
                            .expect("test accessor target permits definition");
                        Some(level)
                    })
                    .collect(),
            );
            let update = Js::object(vec![("total", Js::Number(0.))]);
            let log = trace.clone();
            let price = input(&s["price"]);
            update
                .define_accessor(
                    "price",
                    Some(Rc::new(move |_| {
                        log.borrow_mut().push("price".into());
                        Ok(price.clone())
                    })),
                    None,
                )
                .expect("test accessor target permits definition");
            let log = trace.clone();
            let amount = input(&s["amount"]);
            let second = s.get("secondAmount").map(input);
            let throws = s["throwAmount"] == true;
            let reads = Cell::new(0);
            update
                .define_accessor(
                    "amount",
                    Some(Rc::new(move |_| {
                        log.borrow_mut().push("amount".into());
                        if throws {
                            return Err(Thrown::error("RangeError", "amount"));
                        }
                        reads.set(reads.get() + 1);
                        Ok(if reads.get() == 2 {
                            second.clone().unwrap_or_else(|| amount.clone())
                        } else {
                            amount.clone()
                        })
                    })),
                    None,
                )
                .expect("test accessor target permits definition");
            let book = Js::object(vec![(s["side"].as_str().unwrap(), levels.clone())]);
            let outcome = g.get("updateOrderBook").unwrap().call(
                Js::Undefined,
                vec![book, input(&s["side"]), update.clone(), input(&s["upsert"])],
            );
            let (result, error) = match outcome {
                Ok(v) => (Some(v.to_display()), None),
                Err(e) => (None, Some(e.value.get("name").unwrap().to_display())),
            };
            let log = trace.borrow().clone();
            let inserted = levels
                .array_values()
                .unwrap()
                .iter()
                .any(|v| v.strict_equals(&update));
            let observed = json!({"result":result,"error":error,"trace":log,"inserted":inserted,"rows":stringify(g,levels)});
            let expected = json!({"result":row["result"],"error":row["error"],"trace":row["trace"],"inserted":row["inserted"],"rows":row["rows"]});
            assert_eq!(observed, expected, "UPDATE {} {s}", row["index"]);
        }
    });
}
#[test]
fn orderbook_totals_match_source_side_reads_live_rows_and_raw_addition() {
    let source = source();
    let cases = source["totals"].as_array().unwrap();
    assert_eq!(cases.len(), 10);
    host(|g| {
        for row in cases {
            let spec = &row["spec"];
            let trace: Trace = Rc::default();
            let book = Js::object(vec![]);
            for side in ["bids", "asks"] {
                if spec[side] == "$missing" {
                    continue;
                }
                let levels = if spec[side].is_null() {
                    Js::Null
                } else {
                    Js::holey_array(
                        spec[side]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|amount| {
                                if amount == "$hole" {
                                    None
                                } else {
                                    Some(Js::object(vec![
                                        ("amount", input(amount)),
                                        ("total", Js::Number(0.)),
                                    ]))
                                }
                            })
                            .collect(),
                    )
                };
                if let Some(items) = levels.array_items() {
                    for (i, level) in items.into_iter().enumerate() {
                        if let Some(level) = level {
                            let amount = level.get("amount").unwrap();
                            level
                                .delete(&"amount".encode_utf16().collect::<Vec<_>>())
                                .unwrap();
                            let log = trace.clone();
                            let array = levels.clone();
                            let replace = spec["replaceRow"] == true;
                            let append = spec["appendRow"] == true;
                            level
                                .define_accessor(
                                    "amount",
                                    Some(Rc::new(move |_| {
                                        log.borrow_mut().push(format!("{side}:{i}"));
                                        if side == "bids" && i == 0 {
                                            if replace {
                                                array.set(
                                                    "0",
                                                    Js::object(vec![
                                                        ("amount", Js::Number(9.)),
                                                        ("total", Js::Number(0.)),
                                                    ]),
                                                )?;
                                            }
                                            if append && array.get("length")?.as_f64() == Some(1.) {
                                                array.call_method(
                                                    "push",
                                                    vec![Js::object(vec![
                                                        ("amount", Js::Number(2.)),
                                                        ("total", Js::Number(0.)),
                                                    ])],
                                                )?;
                                            }
                                        }
                                        Ok(amount.clone())
                                    })),
                                    None,
                                )
                                .expect("test accessor target permits definition");
                        }
                    }
                }
                let log = trace.clone();
                book.define_accessor(
                    side,
                    Some(Rc::new(move |_| {
                        log.borrow_mut().push(side.into());
                        Ok(levels.clone())
                    })),
                    None,
                )
                .expect("test accessor target permits definition");
            }
            let error = g
                .get("calcOrderBookTotal")
                .unwrap()
                .call(Js::Undefined, vec![book.clone()])
                .err()
                .map(|e| e.value.get("name").unwrap().to_display());
            let log = trace.borrow().clone();
            let observed = json!({"error":error,"trace":log,"book":stringify(g,book)});
            assert_eq!(
                observed,
                json!({"error":row["error"],"trace":row["trace"],"book":row["book"]}),
                "TOTAL {} {spec}",
                row["index"]
            );
        }
    });
}
