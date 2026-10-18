use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str, js_from_value_ref};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::provider::RestBody;
use safe_cex_types::{Market, Ticker};
use safe_cex_venue_breakoutprop::quotes::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn, ready},
    process::Command,
    sync::LazyLock,
    task::{Context, Poll, Waker},
};
const SOURCE_CENSUS: &str = include_str!("support/quotes-census.json");
include!("support/cardinality.rs");
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/quotes_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_source_census(&corpus, SOURCE_CENSUS);
    corpus
});
fn number(v: f64) -> Value {
    json!({"number":format!("{:016x}",if v.is_nan(){0x7ff8000000000000}else{v.to_bits()})})
}
#[path = "support/retained_info_values.rs"]
mod retained_info_values;
use retained_info_values::observe;

fn optional(v: &mut Value, key: &str, x: Option<Value>) {
    if let Some(x) = x {
        v[key] = x;
    }
}
fn market(m: &Market) -> Value {
    let mut v = json!({"id":m.id,"symbol":m.symbol,"base":m.base,"quote":m.quote,"active":m.active,"precision":{"amount":number(m.precision.amount),"price":number(m.precision.price)},"limits":{"amount":{"min":number(m.limits.amount.min),"max":number(safe_cex_num::numeric_value_to_number(&m.limits.amount.max))},"leverage":{"min":number(m.limits.leverage.min),"max":number(m.limits.leverage.max)}}});
    optional(
        &mut v,
        "prettySymbol",
        m.pretty_symbol.as_ref().map(|x| json!(x)),
    );
    optional(&mut v, "contractSize", m.contract_size.map(number));
    optional(&mut v, "isExpiring", m.is_expiring.map(|x| json!(x)));
    optional(&mut v, "isInverse", m.is_inverse.map(|x| json!(x)));
    optional(&mut v, "isSpot", m.is_spot.map(|x| json!(x)));
    optional(&mut v, "isReadOnly", m.is_read_only.map(|x| json!(x)));
    optional(&mut v, "info", m.info.as_ref().map(observe));
    optional(&mut v, "takerFee", m.taker_fee.map(number));
    optional(&mut v, "makerFee", m.maker_fee.map(number));
    v
}
fn ticker(t: &Ticker) -> Value {
    let mut v = json!({"id":t.id,"symbol":t.symbol,"bid":number(t.bid),"ask":number(t.ask),"last":number(t.last),"mark":number(t.mark),"index":t.index.map(number).unwrap_or(Value::Null),"percentage":number(t.percentage),"openInterest":number(t.open_interest),"fundingRate":number(t.funding_rate),"fundingTime":number(t.funding_time),"volume":number(t.volume),"quoteVolume":number(t.quote_volume)});
    optional(&mut v, "midPrice", t.mid_price.map(number));
    optional(&mut v, "info", t.info.as_ref().map(observe));
    v
}
fn store(case: &Value) -> DefaultStore {
    let source: Value = serde_json::from_str(case["storeRaw"].as_str().unwrap()).unwrap();
    let store = DefaultStore::new();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(js_from_value_ref(&source["markets"]).unwrap()),
            tickers: Some(js_from_value_ref(&source["tickers"]).unwrap()),
            ..StoreUpdate::default()
        },
        false,
    )
    .expect("fixture Store update");
    store
}
fn run<F: Future>(future: F) -> F::Output {
    let mut f = std::pin::pin!(future);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("ready fixture pending"),
    }
}
#[test]
fn actual_quote_reads() {
    assert_eq!(
        ORACLE["cases"].as_array().unwrap().len(),
        45,
        "QUOTES-SOURCE fixed cases"
    );
    let mut comparisons = 0;
    for index in 0..45 {
        let case = &ORACLE["cases"][index];
        let method = case["method"].as_str().unwrap();
        let name = case["name"].as_str().unwrap();
        let store = store(case);
        let input = js_from_str::<JsNumberInput>(case["raw"].as_str().unwrap());
        if name == "surrogate" {
            assert_eq!(case["expected"]["codeUnit"], 55296);
            let retained: safe_cex_types::retained_json::RetainedJsonInput =
                input.as_ref().unwrap().clone().decode().unwrap();
            let symbol = if method == "fetchMarkets" {
                retained.as_array().unwrap()[0]
                    .get("instrument")
                    .unwrap()
                    .get("symbol")
                    .unwrap()
            } else {
                retained.get("events").unwrap().as_array().unwrap()[0]
                    .get("symbol")
                    .unwrap()
            };
            assert_eq!(symbol.as_text(), Some([0xd800].as_slice()));
        }
        let errors = RefCell::new(Vec::<Value>::new());
        let trace = RefCell::new(Vec::<Value>::new());
        let mut logger_error = ExchangeError::new("logger-rejection");
        logger_error.store_subscriber_ids = vec![47, 59];
        let logger = |e: Value| {
            errors.borrow_mut().push(e);
            if name == "logger" {
                Err(logger_error.clone())
            } else {
                Ok(())
            }
        };
        let mut original = ExchangeError::new("original-rejection");
        original.store_subscriber_ids = vec![19, 31];
        let result: Result<Value, ExchangeError> = if method == "fetchMarkets" {
            let future = fetch_markets(
                || {
                    trace.borrow_mut().push(json!("combined"));
                    ready(if name == "reject" {
                        Err(original.clone())
                    } else {
                        Ok(input.unwrap())
                    })
                },
                &store,
                &logger,
            );
            assert_eq!(
                *trace.borrow(),
                vec![json!("combined")],
                "QUOTES-EAGER combined"
            );
            run(future).map(|rows| {
                if name == "surrogate" {
                    assert_eq!(rows.len(), 2);
                    assert_eq!(rows[0].id, "\u{fffd}");
                    assert_eq!(rows[0].symbol, "\u{fffd}");
                    assert_eq!(rows[0].base, "\u{fffd}");
                    assert_eq!(rows[1].symbol, "ETHUSD");
                    assert_eq!(
                        rows[0]
                            .info
                            .as_ref()
                            .unwrap()
                            .get("instrument")
                            .unwrap()
                            .get("symbol")
                            .unwrap()
                            .as_text(),
                        Some([0xd800].as_slice())
                    );
                    json!({"typedSurrogate":true})
                } else {
                    Value::Array(rows.iter().map(market).collect())
                }
            })
        } else {
            original = original.with_retained_response((json!({"data":{"msg":if name=="empty-msg"{json!("")}else if name=="object-msg"{json!({"detail":"object"})}else{json!("venue-message")}}})).into());
            let session = || {
                trace.borrow_mut().push(json!("session"));
                ready(if name == "session" {
                    Err(original.clone())
                } else {
                    Ok("token".into())
                })
            };
            let account = || {
                trace.borrow_mut().push(json!("account"));
                ready(if name == "account" {
                    Err(original.clone())
                } else {
                    Ok("account-A".into())
                })
            };
            let request = |r: safe_cex_transport::provider::RestRequest| {
                let Some(RestBody::Untransformed(body)) = r.body else {
                    panic!("QUOTES-REQUEST body")
                };
                trace
                    .borrow_mut()
                    .push(json!({"method":r.method,"url":r.url,"body":body.into_value().unwrap()}));
                ready(
                    if ["http", "empty-msg", "object-msg", "logger"].contains(&name) {
                        Err(original.clone())
                    } else {
                        Ok(input.unwrap())
                    },
                )
            };
            let fetch = |symbols| {
                fetch_tickers_for_symbols(symbols, session, account, request, &store, &logger)
            };
            let output = if method == "fetchTickers" {
                let future = fetch_tickers(&store, fetch).unwrap();
                assert_eq!(
                    *trace.borrow(),
                    vec![json!("session")],
                    "QUOTES-EAGER fetchTickers"
                );
                run(future)
            } else {
                let future = fetch(vec!["A".into(), "A".into(), "B".into()]);
                assert_eq!(
                    *trace.borrow(),
                    vec![json!("session")],
                    "QUOTES-EAGER fetchTickersForSymbols"
                );
                run(future)
            };
            if name == "distinct" {
                let native = output.as_ref().unwrap().as_ref().unwrap();
                assert_eq!(native.len(), 2, "QUOTES native distinct ticker count");
                for row in native {
                    assert_eq!(row.timestamp, None, "QUOTES native absent timestamp");
                }
            }
            output.map(|rows| {
                if name == "surrogate" {
                    let rows = rows.unwrap();
                    assert_eq!(rows.len(), 2);
                    assert_eq!(rows[0].id, "\u{fffd}");
                    assert_eq!(rows[0].symbol, "\u{fffd}");
                    assert_eq!(rows[1].symbol, "B");
                    assert_eq!(rows[0].bid.to_bits(), 13.125_f64.to_bits());
                    assert_eq!(rows[0].ask.to_bits(), 17.375_f64.to_bits());
                    assert_eq!(
                        rows[0]
                            .info
                            .as_ref()
                            .unwrap()
                            .get("symbol")
                            .unwrap()
                            .as_text(),
                        Some([0xd800].as_slice())
                    );
                    json!({"typedSurrogate":true})
                } else {
                    rows.map_or_else(
                        || json!({"$undefined":true}),
                        |rows| Value::Array(rows.iter().map(ticker).collect()),
                    )
                }
            })
        };
        if name == "surrogate" {
            assert_eq!(result.unwrap(), json!({"typedSurrogate":true}));
            assert!(
                errors.borrow().is_empty(),
                "typed String view does not reject the retained record"
            );

            assert_eq!(
                json!(*trace.borrow()),
                case["trace"],
                "surrogate request effects {method}"
            );
            comparisons += 1;
            continue;
        }
        let actual = match result {
            Ok(value) => json!({"ok":value,"errors":errors.borrow().clone()}),
            Err(error) => {
                let expected = if name == "logger" {
                    &logger_error
                } else {
                    &original
                };
                assert_eq!(&error, expected, "QUOTES-ERROR original metadata {method}");
                json!({"error":error.message,"sameError":true,"errors":errors.borrow().clone()})
            }
        };
        assert_native_cardinality(
            &actual,
            &case["expected"],
            &format!("QUOTES {method} {name}"),
        );
        assert_eq!(actual, case["expected"], "QUOTES-NATIVE {method} {name}");
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "QUOTES-REQUEST {method} {name}"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 45, "QUOTES-COMPARISONS fixed total");
}
#[test]
fn quote_drop_boundary() {
    assert_eq!(ORACLE["abandoned"].as_array().unwrap().len(), 3);
    for (index, method) in ["fetchMarkets", "fetchTickersForSymbols", "fetchTickers"]
        .iter()
        .enumerate()
    {
        assert_eq!(ORACLE["abandoned"][index]["method"], *method);
        assert_eq!(
            ORACLE["abandoned"][index]["calls"],
            if index == 0 { 1 } else { 3 },
            "QUOTES-SOURCE discarded promise continues"
        );
        let store = DefaultStore::new();
        let polls = Cell::new(0);
        let report = |_| Ok(());
        if index == 0 {
            let future = fetch_markets(
                || {
                    poll_fn(|_| {
                        polls.set(polls.get() + 1);
                        Poll::Pending::<Result<JsNumberInput, ExchangeError>>
                    })
                },
                &store,
                &report,
            );
            drop(future);
        } else {
            let make = |symbols| {
                fetch_tickers_for_symbols(
                    symbols,
                    || {
                        poll_fn(|_| {
                            polls.set(polls.get() + 1);
                            Poll::Pending::<Result<String, ExchangeError>>
                        })
                    },
                    || ready(Ok("".into())),
                    |_| ready(Err(ExchangeError::new("unreached"))),
                    &store,
                    &report,
                )
            };
            if index == 1 {
                drop(make(vec![]));
            } else {
                drop(fetch_tickers(&store, make).unwrap());
            }
        }
        assert_eq!(polls.get(), 0, "QUOTES-DROP unpolled {method}");
    }
}

#[test]
fn quotes_owned_info_boundary() {
    assert_eq!(
        ORACLE["aliases"].as_array().unwrap().len(),
        3,
        "QUOTES-ALIASES fixed count"
    );
    for (index, method) in ["fetchMarkets", "fetchTickersForSymbols", "fetchTickers"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            ORACLE["aliases"][index],
            json!({"method":method,"changed":"changed"})
        );
        let case = &ORACLE["cases"][index * 15];
        let store = store(case);
        let input: JsNumberInput = js_from_str(case["raw"].as_str().unwrap()).unwrap();
        let saved = input.clone();
        let report = |_| Ok(());
        if index == 0 {
            let mut rows = run(fetch_markets(|| ready(Ok(input)), &store, &report)).unwrap();
            *rows[0]
                .info
                .as_mut()
                .unwrap()
                .get_mut("instrument")
                .unwrap()
                .get_mut("description")
                .unwrap() = json!("changed").into();
            let unchanged = run(fetch_markets(|| ready(Ok(saved)), &store, &report)).unwrap();
            assert!(
                unchanged[0]
                    .info
                    .as_ref()
                    .unwrap()
                    .get("instrument")
                    .unwrap()
                    .get("description")
                    .unwrap()
                    .text_equals("instrument"),
                "QUOTES-OWNED market info"
            );
        } else {
            let fetch = |symbols| {
                fetch_tickers_for_symbols(
                    symbols,
                    || ready(Ok("".into())),
                    || ready(Ok("".into())),
                    |_| ready(Ok(input)),
                    &store,
                    &report,
                )
            };
            let mut rows = if index == 1 {
                run(fetch(vec![]))
            } else {
                run(fetch_tickers(&store, fetch).unwrap())
            }
            .unwrap()
            .unwrap();
            *rows[0].info.as_mut().unwrap().get_mut("symbol").unwrap() = json!("changed").into();
            let unchanged = run(fetch_tickers_for_symbols(
                vec![],
                || ready(Ok("".into())),
                || ready(Ok("".into())),
                |_| ready(Ok(saved)),
                &store,
                &report,
            ))
            .unwrap()
            .unwrap();
            assert!(
                unchanged[0]
                    .info
                    .as_ref()
                    .unwrap()
                    .get("symbol")
                    .unwrap()
                    .text_equals("A"),
                "QUOTES-OWNED ticker info"
            );
        }
    }
}

#[test]
fn quote_symbols_owned_input() {
    assert_eq!(ORACLE["inputAlias"]["posted"], json!(["ORIGINAL", "LATE"]));
    let store = DefaultStore::new();
    let settled = Cell::new(false);
    let posted = RefCell::new(Value::Null);
    let report = |_| Ok(());
    let mut symbols = vec!["ORIGINAL".to_string()];
    let future = fetch_tickers_for_symbols(
        symbols.clone(),
        || {
            poll_fn(|_| {
                if settled.get() {
                    Poll::Ready(Ok("".into()))
                } else {
                    Poll::Pending
                }
            })
        },
        || ready(Ok("".into())),
        |r| {
            let Some(RestBody::Untransformed(body)) = r.body else {
                panic!("body")
            };
            *posted.borrow_mut() = body.into_value().unwrap()["symbols"].clone();
            ready(Ok(js_from_str("{\"events\":[]}").unwrap()))
        },
        &store,
        &report,
    );
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    symbols.push("LATE".into());
    settled.set(true);
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Ready(Ok(_))));
    assert_eq!(symbols.len(), 2);
    assert_eq!(
        *posted.borrow(),
        json!(["ORIGINAL"]),
        "QUOTES-INPUT owned symbol snapshot"
    );
}
#[test]
fn quotes_deferred_owned_fallback() {
    assert_eq!(ORACLE["fallbacks"].as_array().unwrap().len(), 3);
    for (index, method) in ["fetchMarkets", "fetchTickersForSymbols", "fetchTickers"]
        .iter()
        .enumerate()
    {
        let source = &ORACLE["fallbacks"][index];
        assert_eq!(source["method"], *method);
        assert_eq!(source["aliasedLength"], 2);
        let store = store(&ORACLE["cases"][index * 15]);
        let settled = Cell::new(false);
        let report = |_| Ok(());
        let mut cx = Context::from_waker(Waker::noop());
        let update = || {
            let markets = Store::markets(&store).unwrap()[..1].to_vec();
            let tickers = Store::tickers(&store).unwrap()[..1].to_vec();
            Store::update(
                &store,
                StoreUpdate {
                    markets: Some(markets),
                    tickers: Some(tickers),
                    ..StoreUpdate::default()
                },
                false,
            )
            .expect("fixture deferred update");
            settled.set(true);
        };
        if index == 0 {
            let mut future = Box::pin(fetch_markets(
                || {
                    poll_fn(|_| {
                        if settled.get() {
                            Poll::Ready(Ok(js_from_str("[]").unwrap()))
                        } else {
                            Poll::Pending
                        }
                    })
                },
                &store,
                &report,
            ));
            assert!(future.as_mut().poll(&mut cx).is_pending());
            update();
            let Poll::Ready(Ok(mut output)) = future.as_mut().poll(&mut cx) else {
                panic!("fallback ready")
            };
            assert_eq!(output.len(), 1, "QUOTES-FALLBACK native market count");
            assert_eq!(
                Value::Array(output.iter().map(market).collect()),
                source["expected"],
                "QUOTES-FALLBACK latest market snapshot"
            );
            output.push(output[0].clone());
            assert_eq!(
                Store::markets(&store).unwrap().len(),
                1,
                "QUOTES-OWNED market fallback"
            );
        } else {
            let make = |symbols| {
                fetch_tickers_for_symbols(
                    symbols,
                    || {
                        poll_fn(|_| {
                            if settled.get() {
                                Poll::Ready(Err(ExchangeError::new("fallback")))
                            } else {
                                Poll::Pending
                            }
                        })
                    },
                    || ready(Ok("".into())),
                    |_| ready(Err(ExchangeError::new("unreached"))),
                    &store,
                    &report,
                )
            };
            let mut future = Box::pin(if index == 1 {
                make(vec![])
            } else {
                fetch_tickers(&store, make).unwrap()
            });
            assert!(future.as_mut().poll(&mut cx).is_pending());
            update();
            let Poll::Ready(Ok(Some(mut output))) = future.as_mut().poll(&mut cx) else {
                panic!("fallback ready")
            };
            assert_eq!(output.len(), 1, "QUOTES-FALLBACK native ticker count");
            assert_eq!(
                Value::Array(output.iter().map(ticker).collect()),
                source["expected"],
                "QUOTES-FALLBACK latest ticker snapshot"
            );
            output.push(output[0].clone());
            assert_eq!(
                Store::tickers(&store).unwrap().len(),
                1,
                "QUOTES-OWNED ticker fallback"
            );
        }
    }
}
struct PendingInput<'a, T> {
    polls: &'a Cell<usize>,
    drops: &'a Cell<usize>,
    marker: std::marker::PhantomData<T>,
}
impl<T> Future for PendingInput<'_, T> {
    type Output = Result<T, ExchangeError>;
    fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        self.polls.set(self.polls.get() + 1);
        Poll::Pending
    }
}
impl<T> Drop for PendingInput<'_, T> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
#[test]
fn quotes_pending_drop_releases_child() {
    let store = DefaultStore::new();
    let report = |_| Ok(());
    let mut cx = Context::from_waker(Waker::noop());
    for method in 0..3 {
        for poll in [false, true] {
            let polls = Cell::new(0);
            let drops = Cell::new(0);
            if method == 0 {
                let mut future = Box::pin(fetch_markets(
                    || PendingInput {
                        polls: &polls,
                        drops: &drops,
                        marker: std::marker::PhantomData,
                    },
                    &store,
                    &report,
                ));
                if poll {
                    assert!(future.as_mut().poll(&mut cx).is_pending());
                }
                drop(future);
            } else {
                let make = |symbols| {
                    fetch_tickers_for_symbols(
                        symbols,
                        || PendingInput {
                            polls: &polls,
                            drops: &drops,
                            marker: std::marker::PhantomData,
                        },
                        || ready(Ok("".into())),
                        |_| ready(Err(ExchangeError::new("unreached"))),
                        &store,
                        &report,
                    )
                };
                let mut future = Box::pin(if method == 1 {
                    make(vec![])
                } else {
                    fetch_tickers(&store, make).unwrap()
                });
                if poll {
                    assert!(future.as_mut().poll(&mut cx).is_pending());
                }
                drop(future);
            }
            assert_eq!(polls.get(), usize::from(poll), "QUOTES-DROP polling state");
            assert_eq!(drops.get(), 1, "QUOTES-DROP releases child resource");
        }
    }
}

#[test]
fn quotes_market_error_precedence() {
    let case = &ORACLE["precedence"];
    let store = store(case);
    let errors = RefCell::new(Vec::new());
    let logger = |e: Value| {
        errors.borrow_mut().push(e);
        Ok(())
    };
    let result = run(fetch_markets(
        || ready(Ok(js_from_str(case["raw"].as_str().unwrap()).unwrap())),
        &store,
        &logger,
    ));
    assert!(
        result.is_ok(),
        "QUOTES-PRECEDENCE fallback before later malformed row"
    );
    let rows = result.unwrap();
    assert_eq!(rows.len(), 4, "QUOTES-PRECEDENCE native fallback count");
    assert_eq!(
        json!({"ok":rows.iter().map(market).collect::<Vec<_>>(),"errors":errors.borrow().clone()}),
        case["expected"],
        "QUOTES-PRECEDENCE native fields and logger"
    );
}

#[test]
fn quotes_consumed_fields_match_source() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/quotes_consumed_fields_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "minimal",
        "poisoned-unused",
        "null-row",
        "scalar-events",
        "null-root",
        "null-events",
        "missing-events",
        "empty",
        "valid-then-null",
        "projection-before-null",
        "logger",
    ];
    assert_eq!(cases.len(), 22, "QUOTES-CONSUMED source census");
    for (index, case) in cases.iter().enumerate() {
        let method = if index < 11 {
            "fetchTickersForSymbols"
        } else {
            "fetchTickers"
        };
        let name = names[index % 11];
        assert_eq!(case["method"], method);
        assert_eq!(case["name"], name);
        let store = store(case);
        let errors = RefCell::new(Vec::<Value>::new());
        let mut reporter_error = ExchangeError::new("logger-rejection");
        reporter_error.store_subscriber_ids = vec![29, 31];
        let logger = |value| {
            errors.borrow_mut().push(value);
            if name == "logger" {
                Err(reporter_error.clone())
            } else {
                Ok(())
            }
        };
        let input = js_from_str::<JsNumberInput>(case["raw"].as_str().unwrap()).unwrap();
        let fetch = |symbols| {
            fetch_tickers_for_symbols(
                symbols,
                || ready(Ok("token".into())),
                || ready(Ok("account".into())),
                |_| ready(Ok(input)),
                &store,
                &logger,
            )
        };
        let result = if method == "fetchTickers" {
            run(fetch_tickers(&store, fetch).unwrap())
        } else {
            run(fetch(vec!["A".into(), "B".into()]))
        };
        let actual = match result {
            Ok(rows) => {
                json!({"ok":rows.map_or_else(||json!({"$undefined":true}), |rows| Value::Array(rows.iter().map(ticker).collect())),"errors":*errors.borrow()})
            }
            Err(error) => {
                assert_eq!(error, reporter_error, "QUOTES-CONSUMED reporter metadata");
                json!({"error":error.message,"name":error.name,"errors":*errors.borrow()})
            }
        };
        assert_eq!(actual, case["expected"], "QUOTES-CONSUMED {method} {name}");
    }
}
