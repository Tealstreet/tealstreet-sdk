use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{Market, Ticker};
use safe_cex_venue_blofin::reads::*;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[allow(dead_code)]
#[path = "support/native_runtime.rs"]
mod retained_native_support;

#[test]
fn maintained_ticker_info_capture() {
    let output = Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/retained_ticker_source.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        rows,
        json!([
            {"native":false,"textUnits":[55296],"keyUnits":[56320],"overflow":"infinity","negativeZero":true,"ownUndefined":false,"nan":false},
            {"native":true,"textUnits":[55296],"keyUnits":[56320],"overflow":"infinity","negativeZero":true,"ownUndefined":true,"nan":true}
        ])
    );
}

#[test]
fn native_ticker_fetch_retains_opaque_info_before_projection() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    for phase in 0..3 {
        let (venue, rt, store, wire) = retained_native_support::fixture();
        let market: Market = serde_json::from_value(json!({"id":"BTC-USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0.001,"max":1000},"leverage":{"min":1,"max":50}}})).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let raw = r#"{"code":"0","data":[{"instId":"BTC-USDT","open24h":"100","last":"110","bidPrice":"109","askPrice":"111","volCurrency24h":"12","vol24h":"1320","opaque":{"text":"\ud800","\udc00":"opaque-key-value","overflow":1e400,"negativeZero":-0}}]}"#;
        let mut input = Input::parse_json(&raw.encode_utf16().collect::<Vec<_>>()).unwrap();
        if phase > 0 {
            let opaque = input.get_mut("data").unwrap().as_array_mut().unwrap()[0]
                .get_mut("opaque")
                .unwrap();
            if let Input::Object(fields) = opaque {
                fields.push(("nan".encode_utf16().collect(), Input::Float(f64::NAN)));
                fields.push(("ownUndefined".encode_utf16().collect(), Input::Undefined));
            } else {
                panic!("opaque fixture must be object");
            }
        }
        let body = match phase {
            0 => RestResponseBody::Encoded(raw.as_bytes().to_vec()),
            1 => RestResponseBody::Untransformed(input),
            _ => RestResponseBody::Transformed(input),
        };
        wire.retained_bodies
            .borrow_mut()
            .insert("/api/v1/market/tickers".into(), body);
        let mut pending = venue.fetch_tickers();
        let result = (0..64)
            .find_map(|_| {
                match pending
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                {
                    Poll::Ready(value) => Some(value),
                    Poll::Pending => {
                        rt.pump();
                        None
                    }
                }
            })
            .expect("actual native ticker fetch must settle")
            .expect("opaque info must not refuse native ticker response");
        assert_eq!(
            result.len(),
            1,
            "native phase{phase}: opaque row must survive"
        );
        assert_eq!(result[0].last, 110.0);
        let opaque = result[0].info.as_ref().unwrap().get("opaque").unwrap();
        assert_eq!(
            opaque.get("text").unwrap().as_text(),
            Some([0xd800].as_slice())
        );
        assert!(
            opaque
                .as_object()
                .unwrap()
                .iter()
                .any(|(key, value)| key == &[0xdc00] && value.text_equals("opaque-key-value"))
        );
        assert_eq!(
            opaque.get("overflow").unwrap().as_f64(),
            Some(f64::INFINITY)
        );
        assert_eq!(
            opaque
                .get("negativeZero")
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        if phase > 0 {
            assert!(opaque.get("nan").unwrap().as_f64().unwrap().is_nan());
            assert!(matches!(opaque.get("ownUndefined"), Some(Input::Undefined)));
        }
        assert_eq!(wire.requests.borrow().len(), 1, "real configured REST path");
        venue.dispose().unwrap();
        rt.pump();
    }
}
fn bits(value: f64) -> Value {
    json!({"number":if value.is_nan(){"nan".into()}else{format!("{:016x}",value.to_bits())}})
}
fn info(value: safe_cex_types::retained_json::RetainedJsonInput) -> Value {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Float(v) => bits(v),
        Input::Lexeme(v) => bits(v.parse().unwrap()),
        Input::Array(v) => Value::Array(v.into_iter().map(info).collect()),
        Input::Object(v) => Value::Object(
            v.into_iter()
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), info(v)))
                .collect(),
        ),
        Input::Text(v) => Value::String(String::from_utf16(&v).unwrap()),
        Input::Bool(v) => Value::Bool(v),
        Input::Null => Value::Null,
        Input::Undefined => panic!("source corpus has no native undefined in JSON info"),
    }
}

fn native(ticker: Ticker) -> Value {
    assert!(ticker.mid_price.is_none(), "ticker_mid_price_absent");
    let mut value = json!({"id":ticker.id,"symbol":ticker.symbol,"bid":bits(ticker.bid),"ask":bits(ticker.ask),
        "last":bits(ticker.last),"mark":bits(ticker.mark),"index":bits(ticker.index),"percentage":bits(ticker.percentage),
        "fundingRate":bits(ticker.funding_rate),"fundingTime":bits(ticker.funding_time),"volume":bits(ticker.volume),
        "quoteVolume":bits(ticker.quote_volume),"openInterest":bits(ticker.open_interest)});
    if let Some(raw) = ticker.info {
        value["info"] = info(raw);
    }
    value
}

struct Adapter {
    phase: usize,
    fixture: Value,
    requests: RefCell<Vec<RestRequest>>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.requests.borrow_mut().push(request.clone());
        Box::pin(async move {
            let response = self.fixture["body"].as_str().map(|s| RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: match self.phase {
                    0 => RestResponseBody::Encoded(s.as_bytes().to_vec()),
                    1 => RestResponseBody::Untransformed(serde_json::from_str(s).unwrap()),
                    _ => RestResponseBody::Transformed(serde_json::from_str(s).unwrap()),
                },
            });
            if self.fixture["fail"] == true {
                let mut error = RestFailure::pre_transmit("network-message".into(), request);
                error.response = response.map(Box::new);
                Err(error)
            } else {
                Ok(response.expect("response fixture"))
            }
        })
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("READ adapter unexpectedly pending"),
    }
}
#[test]
fn live_reads_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reads_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "READ actual TypeScript {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        v => v,
    };
    assert_eq!(corpus["header"]["architecture"], arch, "READ architecture");
    assert!(
        corpus["header"]["nodeMajor"]
            .as_u64()
            .is_some_and(|v| v > 0),
        "READ node major"
    );
    eprintln!("reads capture header: {}", corpus["header"]);
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        156,
        "READ-SOURCE corpus total"
    );
    // Independent of oracle output length: fixture slots are fixed, including all error handlers.
    let raw_lengths: [Option<usize>; 26] = [
        Some(2),
        Some(1),
        Some(0),
        None,
        None,
        Some(0),
        None,
        None,
        None,
        Some(1),
        Some(2),
        None,
        None,
        Some(1),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
    ];
    let inputs: Value = serde_json::from_str(include_str!("support/reads-inputs.json")).unwrap();
    assert_eq!(inputs.as_array().unwrap().len(), 26, "READ input pin count");
    let mut visits = [0_usize; 468];
    let mut comparisons = 0;
    for fixture in 0..26 {
        for mode in 0..3 {
            for method in 0..2 {
                let i = fixture * 6 + mode * 2 + method;
                let case = &corpus["cases"][i];
                assert_eq!(
                    case["fixture"], inputs[fixture],
                    "READ source fixture identity"
                );
                assert_eq!(case["mode"], mode, "READ source case coverage");
                assert_eq!(
                    case["method"],
                    if method == 0 {
                        "fetchTickersRaw"
                    } else {
                        "fetchTickers"
                    },
                    "READ source method coverage"
                );
                let raw_logs = fixture == 5 || fixture >= 14;
                let fallback = method == 1 && matches!(fixture, 3 | 4 | 6 | 7 | 8 | 9 | 11 | 12);
                let log_count = usize::from(raw_logs || fallback);
                assert_eq!(
                    case["logs"].as_array().unwrap().len(),
                    log_count,
                    "READ-SOURCE per-case logs"
                );
                assert_eq!(
                    case["requests"].as_array().unwrap().len(),
                    1,
                    "READ-SOURCE per-case requests"
                );
                let throws = mode == 1 && log_count == 1;
                assert_eq!(
                    case["rawAlias"],
                    method == 0 && matches!(fixture, 0 | 1 | 2 | 6 | 9 | 10 | 13),
                    "READ source response alias boundary"
                );
                let count = if throws {
                    None
                } else if method == 0 {
                    raw_lengths[fixture]
                } else if fallback {
                    Some(if mode == 2 { 1 } else { 2 })
                } else {
                    Some(match fixture {
                        0 => 2,
                        1 | 10 | 13 => 1,
                        _ => 0,
                    })
                };
                if let Some(count) = count {
                    assert_eq!(
                        case["result"]["ok"].as_array().unwrap().len(),
                        count,
                        "READ-SOURCE per-case output"
                    );
                }
                for phase in 0..3 {
                    let store = DefaultStore::new();
                    let market: Market = js_from_value(corpus["market"].clone()).unwrap();
                    let stored: Vec<Ticker> = js_from_value(corpus["stored"].clone()).unwrap();
                    Store::update(
                        &store,
                        StoreUpdate {
                            markets: Some(vec![market]),
                            tickers: Some(stored.clone()),
                            ..Default::default()
                        },
                        false,
                    )
                    .unwrap();
                    let adapter = Adapter {
                        phase,
                        fixture: case["fixture"].clone(),
                        requests: RefCell::new(Vec::new()),
                    };
                    let sentinel = Rc::new("host:unique".to_owned());
                    let mut logs = Vec::new();
                    let emitter = |message: &Value| {
                        logs.push(info(message.clone().into()));
                        if mode == 2 {
                            let mut ticker = stored[0].clone();
                            ticker.id = "after-log".into();
                            ticker.symbol = "after-log-symbol".into();
                            ticker.last = 37.125;
                            Store::update(
                                &store,
                                StoreUpdate {
                                    tickers: Some(vec![ticker]),
                                    ..Default::default()
                                },
                                false,
                            )
                            .unwrap();
                        }
                        if mode == 1 {
                            Err(sentinel.clone())
                        } else {
                            Ok(())
                        }
                    };
                    let result = if method == 0 {
                        run(fetch_tickers_raw(&adapter, emitter)).map(|raw| {
                            raw.map_or(json!({"undefined":true}), |v| {
                                info(v.retained().clone().into())
                            })
                        })
                    } else {
                        run(fetch_tickers(&adapter, &store, emitter))
                            .map(|rows| Value::Array(rows.into_iter().map(native).collect()))
                    };
                    let observed = match result {
                        Ok(value) => {
                            if let Some(count) = count {
                                assert_eq!(
                                    value.as_array().expect("READ native array").len(),
                                    count,
                                    "READ-CARDINALITY native outputs case {i}"
                                );
                            }
                            json!({"ok":value})
                        }
                        Err(ReadError::Emitter(error)) => {
                            assert!(Rc::ptr_eq(&error, &sentinel), "READ emitter identity");
                            json!({"error":"Error","message":"host:unique","identity":true})
                        }
                        Err(ReadError::Ingress(error)) => {
                            panic!("READ unexpected ingress {i}: {error}")
                        }
                    };
                    assert_eq!(observed, case["result"], "READ native fields case {i}");
                    assert_eq!(logs.len(), log_count, "READ native log count");
                    assert_eq!(json!(logs), case["logs"], "READ native messages case {i}");
                    assert_eq!(
                        *adapter.requests.borrow(),
                        vec![RestRequest {
                            method: "GET".into(),
                            url: "/api/v1/market/tickers".into(),
                            ..Default::default()
                        }],
                        "READ request payload"
                    );
                    visits[i * 3 + phase] += 1;
                    comparisons += 1;
                }
            }
        }
    }
    assert_eq!(comparisons, 468, "READ native comparison count");
    assert_eq!(visits, [1; 468], "READ native case coverage");
    assert_eq!(
        corpus["boundaries"].as_array().unwrap().len(),
        4,
        "READ boundary total"
    );
    assert_eq!(
        corpus["boundaryResults"].as_array().unwrap().len(),
        4,
        "READ source boundary total"
    );
    assert_eq!(
        corpus["boundaryResults"],
        json!([{"undefined":true},{"undefined":true},{"codeUnit":55296},{"undefined":true}]),
        "READ source boundary outcomes"
    );
    assert_eq!(
        corpus["phaseProbes"].as_array().unwrap().len(),
        3,
        "READ source phase total"
    );
    let phase_inputs = [
        serde_json::to_string(&inputs[1]["body"]).unwrap(),
        inputs[1]["body"].as_str().unwrap().to_owned(),
        "[1,2]".to_owned(),
    ];
    let mut phase_visits = [0_usize; 9];
    for i in 0..3 {
        let probe = &corpus["phaseProbes"][i];
        assert_eq!(probe["body"], phase_inputs[i], "READ phase input identity");
        if i == 1 {
            assert_eq!(
                probe["result"].as_array().unwrap().len(),
                1,
                "READ source phase output count"
            );
        } else {
            assert_eq!(
                probe["result"],
                json!({"undefined":true}),
                "READ source phase undefined"
            );
        }
        for phase in 0..3 {
            let adapter = Adapter {
                phase,
                fixture: json!({"body":probe["body"],"fail":false}),
                requests: RefCell::new(Vec::new()),
            };
            let result = run(fetch_tickers_raw(&adapter, |_| -> Result<(), ()> {
                panic!("READ phase must not emit")
            }))
            .unwrap();
            let native = result.map_or(json!({"undefined":true}), |v| {
                info(v.retained().clone().into())
            });
            if i == 1 {
                assert_eq!(
                    native.as_array().unwrap().len(),
                    1,
                    "READ native phase output count"
                );
            }
            assert_eq!(
                native, probe["result"],
                "READ response phase native value {i}/{phase}"
            );
            assert_eq!(
                adapter.requests.borrow().len(),
                1,
                "READ phase request count"
            );
            phase_visits[i * 3 + phase] += 1;
        }
    }
    assert_eq!(phase_visits, [1; 9], "READ phase comparison count");
    let mut boundary_visits = [0_usize; 4];
    for i in 0..4 {
        let adapter = Adapter {
            phase: 0,
            fixture: json!({"body":corpus["boundaries"][i],"fail":false}),
            requests: RefCell::new(Vec::new()),
        };
        assert!(
            matches!(
                run(fetch_tickers_raw(&adapter, |_| -> Result<(), ()> {
                    panic!("READ ingress must not emit")
                })),
                Err(ReadError::Ingress(_))
            ),
            "READ explicit ingress boundary {i}"
        );
        assert_eq!(
            adapter.requests.borrow().len(),
            1,
            "READ boundary request count"
        );
        boundary_visits[i] += 1;
    }
    assert_eq!(boundary_visits, [1; 4], "READ boundary comparison count");
}
