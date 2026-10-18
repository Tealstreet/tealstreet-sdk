use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_transport::provider::RestBody;
use safe_cex_types::OHLCVOptions;
use safe_cex_venue_breakoutprop::candles::fetch_ohlcv;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, ready},
    process::Command,
    sync::LazyLock,
    task::{Context, Poll, Waker},
};
#[derive(serde::Serialize, serde::Deserialize)]
struct Options {
    symbol: String,
    interval: String,
    from: Option<f64>,
    to: Option<f64>,
    limit: Option<f64>,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Options {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self,struct {symbol,interval,from,to,limit});
    }
}
static ORACLE: LazyLock<Value> = LazyLock::new(|| {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/candles_oracle.cjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
});
fn number(n: f64) -> Value {
    json!({"number":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture pending"),
    }
}
#[test]
fn actual_candle_read() {
    const NAMES: [&str; 28] = [
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
        "absent",
        "zero",
        "negative-zero",
        "fractional",
        "invalid-bound",
        "empty",
        "invalid-date",
        "local-date",
        "nonfinite",
        "missing-unused",
        "session",
        "account",
        "http",
        "logger",
        "empty-msg",
        "object-msg",
    ];
    assert_eq!(
        ORACLE["cases"].as_array().unwrap().len(),
        28,
        "CANDLE-SOURCE corpus total"
    );
    let mut comparisons = 0;
    for (index, name) in NAMES[..28].iter().enumerate() {
        let case = &ORACLE["cases"][index];
        assert_eq!(case["name"], *name, "CANDLE-SOURCE case identity");
        let o: Options = js_from_str(case["options"].as_str().unwrap()).unwrap();
        let options = OHLCVOptions {
            symbol: o.symbol,
            interval: serde_json::from_value(json!(o.interval)).unwrap(),
            from: o.from,
            to: o.to,
            limit: o.limit,
        };
        let input: JsNumberInput = js_from_str(case["raw"].as_str().unwrap()).unwrap();
        let trace = RefCell::new(Vec::<Value>::new());
        let errors = RefCell::new(Vec::<Value>::new());
        let mut original = ExchangeError::new("original-rejection");
        original = original.with_retained_response((json!({"data":{"msg":if *name=="empty-msg"{json!("")}else if *name=="object-msg"{json!({"detail":"venue"})}else{json!("venue-message")}}})).into());
        original.store_subscriber_ids = vec![19, 31];
        let mut logger_error = ExchangeError::new("logger-rejection");
        logger_error.store_subscriber_ids = vec![47, 59];
        let iso = |n: f64| {
            let bits = number(n)["number"].as_str().unwrap().to_owned();
            let fixture = case["iso"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["bits"] == bits)
                .expect("CANDLE-DATE exact host input");
            if let Some(s) = fixture["value"].as_str() {
                Ok(s.into())
            } else {
                let mut e = ExchangeError::new(fixture["error"].as_str().unwrap());
                e.name = "RangeError".into();
                Err(e)
            }
        };
        let logger = |e| {
            errors.borrow_mut().push(e);
            if *name == "logger" {
                Err(logger_error.clone())
            } else {
                Ok(())
            }
        };
        let future = fetch_ohlcv(
            options,
            || {
                trace.borrow_mut().push(json!("session"));
                ready(if *name == "session" {
                    Err(original.clone())
                } else {
                    Ok("token".into())
                })
            },
            || {
                trace.borrow_mut().push(json!("account"));
                ready(if *name == "account" {
                    Err(original.clone())
                } else {
                    Ok("account-A".into())
                })
            },
            |request| {
                assert_eq!(
                    request.headers.get("Content-Type").map(String::as_str),
                    Some("application/json"),
                    "CANDLE-REQUEST JSON content type"
                );
                let Some(RestBody::Encoded(bytes)) = request.body else {
                    panic!("CANDLE-REQUEST encoded body")
                };
                trace.borrow_mut().push(json!({"method":request.method,"url":request.url,"wire":String::from_utf8(bytes).unwrap()}));
                ready(
                    if ["http", "logger", "empty-msg", "object-msg"].contains(name) {
                        Err(original.clone())
                    } else {
                        Ok(input)
                    },
                )
            },
            &iso,
            &|n| n,
            &logger,
        );
        assert_eq!(
            *trace.borrow(),
            vec![json!("session")],
            "CANDLE-EAGER session"
        );
        let result = run(future);
        assert_eq!(
            json!(*trace.borrow()),
            case["trace"],
            "CANDLE-REQUEST {name}"
        );
        match result {
            Ok(rows) => {
                let count = if *name == "empty" { 0 } else { 2 };
                assert_eq!(
                    case["expected"]["ok"].as_array().unwrap().len(),
                    count,
                    "CANDLE-SOURCE per-case count"
                );
                assert_eq!(
                    rows.len(),
                    count,
                    "CANDLE-CARDINALITY independent native count"
                );
                let observed:Vec<Value>=rows.iter().map(|c|{assert!(c.last_side.is_none(),"CANDLE-NATIVE absent lastSide");json!({"timestamp":number(c.timestamp),"open":number(c.open),"high":number(c.high),"low":number(c.low),"close":number(c.close),"volume":number(c.volume)})}).collect();
                assert_eq!(
                    json!(observed),
                    case["expected"]["ok"],
                    "CANDLE-NATIVE decoded fields {name}"
                );
            }
            Err(e) => {
                assert_eq!(
                    e.message, case["expected"]["error"],
                    "CANDLE-ERROR message {name}"
                );
                assert_eq!(e.name, case["expected"]["name"], "CANDLE-ERROR name");
                if *name != "invalid-bound" {
                    assert_eq!(
                        e,
                        if *name == "logger" {
                            logger_error
                        } else {
                            original
                        },
                        "CANDLE-ERROR original metadata"
                    );
                }
            }
        }
        assert_eq!(
            json!(*errors.borrow()),
            case["expected"]["errors"],
            "CANDLE-LOG selected value"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 28, "CANDLE-COMPARISONS fixed total");
}

#[test]
fn candle_owned_options_and_drop_boundary() {
    assert_eq!(
        ORACLE["owned"],
        json!({"posted":["LATE"],"completed":1}),
        "CANDLE-SOURCE owned input witness"
    );
    use std::cell::Cell;
    assert_eq!(
        ORACLE["supplements"]["discarded"]["calls"], 2,
        "CANDLE-DISCARDED source continuation"
    );
    let drops = Cell::new(0);
    struct Pending<'a>(&'a Cell<usize>);
    impl Future for Pending<'_> {
        type Output = Result<String, ExchangeError>;
        fn poll(self: std::pin::Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending
        }
    }
    impl Drop for Pending<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let options = || OHLCVOptions {
        symbol: "ORIGINAL".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
        from: None,
        to: None,
        limit: None,
    };
    let iso = |_| panic!("unexpected date");
    let local = |n| n;
    let logger = |_| Ok(());
    for poll in [false, true] {
        let f = fetch_ohlcv(
            options(),
            || Pending(&drops),
            || -> std::future::Ready<Result<String, ExchangeError>> {
                panic!("account after dropped session")
            },
            |_| ready(Err(ExchangeError::new("unreachable"))),
            &iso,
            &local,
            &logger,
        );
        let mut f = Box::pin(f);
        if poll {
            assert!(
                f.as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
        }
        drop(f);
    }
    assert_eq!(drops.get(), 2, "CANDLE-DROP pending resources released");
    let f = fetch_ohlcv(
        options(),
        || ready(Ok("".into())),
        || ready(Ok("".into())),
        |r| {
            let Some(RestBody::Encoded(bytes)) = r.body else {
                panic!("wire")
            };
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                body["symbols"],
                json!(["ORIGINAL"]),
                "CANDLE-OWNED input snapshot"
            );
            ready(Ok(js_from_str::<JsNumberInput>("{\"events\":[]}").unwrap()))
        },
        &iso,
        &local,
        &logger,
    );
    assert!(run(f).unwrap().is_empty());
}

#[test]
fn candle_nan_and_utf8_boundaries() {
    let s = &ORACLE["supplements"];
    assert_eq!(
        s.as_object().unwrap().len(),
        3,
        "CANDLE-SOURCE supplement count"
    );
    assert_eq!(
        s["surrogate"]["count"], 2,
        "CANDLE-UTF8 source accepted ignored surrogate"
    );
    let input: JsNumberInput = js_from_str(s["surrogate"]["raw"].as_str().unwrap()).unwrap();
    let retained: safe_cex_types::retained_json::RetainedJsonInput =
        input.clone().decode().unwrap();
    assert_eq!(
        retained.get("events").unwrap().as_array().unwrap()[0]
            .get("symbol")
            .unwrap()
            .as_text(),
        Some([0xd800].as_slice())
    );
    let rows = run(fetch_ohlcv(
        OHLCVOptions {
            symbol: "BTCUSD".into(),
            interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
            from: None,
            to: None,
            limit: Some(1.0),
        },
        || ready(Ok("token".into())),
        || ready(Ok("account".into())),
        |_| ready(Ok(input)),
        &|_| panic!("no requested date bounds"),
        &|n| n,
        &|_| panic!("ignored symbol must not report a failure"),
    ))
    .unwrap();
    assert_eq!(rows.len(), 2, "source ignored-surrogate candle count");
    for (row, expected) in rows
        .iter()
        .zip(ORACLE["cases"][0]["expected"]["ok"].as_array().unwrap())
    {
        for (field, value) in [
            ("timestamp", row.timestamp),
            ("open", row.open),
            ("high", row.high),
            ("low", row.low),
            ("close", row.close),
            ("volume", row.volume),
        ] {
            assert_eq!(
                number(value),
                expected[field],
                "retained candle field {field}"
            );
        }
    }
    let options = OHLCVOptions {
        symbol: "BTCUSD".into(),
        interval: safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1),
        from: Some(f64::NAN),
        to: Some(f64::NAN),
        limit: None,
    };
    let iso = |_| panic!("CANDLE-NAN must not call Date");
    let local = |n| n;
    let logger = |_| Ok(());
    let f = fetch_ohlcv(
        options,
        || ready(Ok("".into())),
        || ready(Ok("".into())),
        |r| {
            let Some(RestBody::Encoded(bytes)) = r.body else {
                panic!("wire")
            };
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                s["nan"]["wire"],
                "CANDLE-NAN wire null bounds"
            );
            ready(Ok(js_from_str::<JsNumberInput>(
                s["nan"]["raw"].as_str().unwrap(),
            )
            .unwrap()))
        },
        &iso,
        &local,
        &logger,
    );
    assert_eq!(s["nan"]["count"], 2, "CANDLE-NAN source count");
    assert_eq!(run(f).unwrap().len(), 2, "CANDLE-NAN native count");
}
