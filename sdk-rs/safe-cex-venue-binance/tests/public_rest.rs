use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Candle, MarketLimitsAmount, MarketPrecision, OHLCVOptions, Ticker};
use safe_cex_venue_binance::{
    exchange_error::ErrorEmission, public_rest::*, request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<T>(f: impl Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture did not settle"),
    }
}
fn number(n: f64) -> Value {
    if n.is_nan() {
        json!("nan")
    } else if n == f64::INFINITY {
        json!("infinity")
    } else if n == f64::NEG_INFINITY {
        json!("-infinity")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-zero")
    } else {
        json!(n)
    }
}
fn undefined() -> Value {
    json!({"$undefined":true})
}
fn opt(v: Option<&Value>) -> Value {
    v.cloned().unwrap_or_else(undefined)
}
fn eq_number(a: f64, b: f64, label: &str) {
    assert!(
        if b.is_nan() {
            a.is_nan()
        } else if b == 0.0 {
            a == b && a.is_sign_negative() == b.is_sign_negative()
        } else {
            a == b
        },
        "{label}: native value {a:?} vs expected {b:?}"
    )
}
fn cached_market() -> RestMarket {
    RestMarket {
        id: "cached".into(),
        symbol: Some(json!("cached")),
        base: Some(json!("C")),
        quote: Some(json!("Q")),
        active: false,
        is_spot: None,
        precision: MarketPrecision {
            base: None,
            quote: None,
            amount: 1.0,
            price: 2.0,
        },
        amount: MarketLimitsAmount {
            min: 3.0,
            max: (4.0).into(),
        },
        leverage_min: 1.0,
        leverage_max: json!(5),
    }
}
fn cached_ticker() -> Ticker {
    Ticker {
        id: "cached".into(),
        symbol: "cached".into(),
        bid: 1.0,
        ask: 2.0,
        last: 3.0,
        mark: 4.0,
        index: 5.0,
        percentage: 6.0,
        funding_rate: 7.0,
        funding_time: 8.0,
        volume: 9.0,
        quote_volume: 10.0,
        open_interest: 0.0,
        mid_price: None,
        timestamp: None,
        info: None,
    }
}
fn market_wire(m: &RestMarket) -> Value {
    let mut v = json!({"id":m.id,"symbol":opt(m.symbol.as_ref()),"base":opt(m.base.as_ref()),"quote":opt(m.quote.as_ref()),"active":m.active,"precision":{"amount":number(m.precision.amount),"price":number(m.precision.price)},"limits":{"amount":{"min":number(m.amount.min),"max":number(safe_cex_num::numeric_value_to_number(&m.amount.max))},"leverage":{"min":number(m.leverage_min),"max":m.leverage_max}}});
    if let Some(spot) = m.is_spot {
        v["isSpot"] = json!(spot)
    }
    v
}
fn ticker_wire(t: &Ticker) -> Value {
    json!({"id":t.id,"symbol":t.symbol,"bid":number(t.bid),"ask":number(t.ask),"last":number(t.last),"mark":number(t.mark),"index":number(t.index),"percentage":number(t.percentage),"fundingRate":number(t.funding_rate),"fundingTime":number(t.funding_time),"volume":number(t.volume),"quoteVolume":number(t.quote_volume),"openInterest":number(t.open_interest)})
}
fn candle_wire(c: &Candle) -> Value {
    json!({"timestamp":number(c.timestamp),"open":number(c.open),"high":number(c.high),"low":number(c.low),"close":number(c.close),"volume":number(c.volume)})
}
struct Bindings {
    c: Value,
    calls: usize,
    public_only: bool,
    trace: Rc<RefCell<Vec<Value>>>,
    error: Failure,
    emissions: Vec<ErrorEmission>,
    market_cache: Rc<Vec<RestMarket>>,
    ticker_cache: Rc<Vec<Ticker>>,
    requests: Vec<(RestClient, &'static str, Option<CandleParams>)>,
}
impl Bindings {
    fn new(c: Value) -> Self {
        let mut e = ExchangeError::new("public fixture");
        e.name = "RangeError".into();
        e.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(
            "fixture-code"
        )));
        e = e.with_retained_response((json!({"status":503,"data":{"msg":"venue detail"}})).into());
        Self {
            public_only: c["name"] == "public-only",
            c,
            calls: 0,
            trace: Rc::new(RefCell::new(vec![])),
            error: Rc::new(e),
            emissions: vec![],
            market_cache: Rc::new(vec![cached_market()]),
            ticker_cache: Rc::new(vec![cached_ticker()]),
            requests: vec![],
        }
    }
    fn name(&self) -> &str {
        self.c["name"].as_str().unwrap()
    }
    fn log(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
    fn failure_wire(&self, error: &Failure) -> Value {
        json!({"original":Rc::ptr_eq(error,&self.error),"name":error.name,"message":error.message,"code":opt(observed_error_code(&error.code).as_ref()),"response":opt(error.response_json_view().as_ref())})
    }
    fn emissions_wire(&self) -> Vec<Value> {
        self.emissions.iter().map(|e|match e{ErrorEmission::Error(e,prefix)=>json!({"kind":"error","original":Rc::ptr_eq(e,&self.error),"prefix":prefix,"message":null,"errorName":e.name,"errorMessage":e.message,"errorCode":opt(observed_error_code(&e.code).as_ref()),"errorResponse":opt(e.response_json_view().as_ref())}),ErrorEmission::Message(message)=>json!({"kind":"message","original":false,"prefix":null,"message":opt(message.as_ref()),"errorName":null,"errorMessage":null,"errorCode":null,"errorResponse":null})}).collect()
    }
    fn wire(&self, value: Option<Value>, fallback: bool, error: Option<&Failure>) -> Value {
        json!({"name":self.name(),"trace":*self.trace.borrow(),"emissions":self.emissions_wire(),"fallback":fallback,"value":value,"failure":error.map(|e|self.failure_wire(e))})
    }
}
impl RestBindings for Bindings {
    fn get(
        &mut self,
        client: RestClient,
        ep: &'static str,
        params: Option<CandleParams>,
    ) -> Result<RestFuture<'_>, Failure> {
        self.calls += 1;
        let calls = self.calls;
        let mut p = Value::Null;
        if let Some(params) = &params {
            p = json!({"symbol":params.symbol,"interval":params.interval,"startTime":number(params.start_time),"endTime":number(params.end_time)});
            if let Some(limit) = params.limit {
                p["limit"] = number(limit)
            }
        }
        let label = match client {
            RestClient::Public => "public",
            RestClient::Private => "private",
            RestClient::Papi => "papi",
        };
        self.log(json!(["get", label, ep, p]));
        self.requests.push((client, ep, params));
        if self.name() == "get-sync-fail" {
            return Err(self.error.clone());
        }
        let trace = self.trace.clone();
        let reject = self.name() == "get-reject";
        let error = self.error.clone();
        let data = self.c["payloads"][calls - 1].get("data").cloned();
        if self.name() == "late-public" {
            self.public_only = true
        }
        Ok(Box::pin(async move {
            trace.borrow_mut().push(json!(["resolve", calls]));
            if reject { Err(error) } else { Ok(data) }
        }))
    }
    fn is_public_data_only(&mut self) -> Result<bool, Failure> {
        self.log(json!(["public-only"]));
        Ok(self.public_only)
    }
    fn markets_by_symbol(&mut self) -> Result<Rc<BTreeMap<String, MarketIdentity>>, Failure> {
        self.log(json!(["map"]));
        if matches!(self.name(), "map-fail" | "map-null-order") {
            return Err(self.error.clone());
        }
        Ok(Rc::new(BTreeMap::from([
            (
                "BTCUSDT".into(),
                MarketIdentity {
                    id: "BTC/USDT".into(),
                    symbol: "BTCUSDT".into(),
                },
            ),
            (
                "undefined".into(),
                MarketIdentity {
                    id: "undefined-id".into(),
                    symbol: "undefined".into(),
                },
            ),
        ])))
    }
    fn emit(&mut self, e: ErrorEmission) -> Result<(), Failure> {
        self.log(json!(["emit"]));
        self.emissions.push(e);
        if self.name() == "emit-fail" {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn fallback_markets(&mut self) -> Result<Rc<Vec<RestMarket>>, Failure> {
        self.log(json!(["fallback-markets"]));
        if self.name() == "fallback-fail" {
            Err(self.error.clone())
        } else {
            Ok(self.market_cache.clone())
        }
    }
    fn fallback_tickers(&mut self) -> Result<Rc<Vec<Ticker>>, Failure> {
        self.log(json!(["fallback-tickers"]));
        if self.name() == "fallback-fail" {
            Err(self.error.clone())
        } else {
            Ok(self.ticker_cache.clone())
        }
    }
    fn dayjs(&mut self, value: Option<f64>) -> Result<f64, Failure> {
        self.log(json!(["dayjs", value.map(number)]));
        if self.name() == "date-fail" {
            Err(self.error.clone())
        } else {
            Ok(value.unwrap_or(200_000_000.0).trunc())
        }
    }
    fn subtract(&mut self, end: f64, amount: f64, unit: char) -> Result<f64, Failure> {
        self.log(json!([
            "subtract",
            number(end),
            number(amount),
            unit.to_string()
        ]));
        if self.name() == "subtract-fail" {
            return Err(self.error.clone());
        }
        Ok(match unit {
            'm' => (end - amount * 60_000.0).trunc(),
            'h' => (end - amount * 3_600_000.0).trunc(),
            'd' => end + safe_cex_num::math_round(-amount) * 86_400_000.0,
            'w' => end + safe_cex_num::math_round(-amount * 7.0) * 86_400_000.0,
            _ => panic!("typed test date adapter unit"),
        })
    }
}
fn fixtures(family: &str) -> Vec<Value> {
    let all: Vec<Value> = serde_json::from_str(include_str!("fixtures/public-rest.json")).unwrap();
    assert_eq!(all.len(), 50, "independent17+15+18 input floor");
    let selected = all
        .into_iter()
        .filter(|c| c["family"] == family)
        .collect::<Vec<_>>();
    assert_eq!(
        selected.len(),
        match family {
            "markets" => 17,
            "tickers" => 15,
            "ohlcv" => 18,
            _ => panic!(),
        },
        "independent family floor"
    );
    selected
}
fn venue_name(v: RestVenue) -> &'static str {
    match v {
        RestVenue::Futures => "futures",
        RestVenue::PM => "pm",
        RestVenue::Spot => "spot",
    }
}
fn oracle(v: RestVenue, family: &str) -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-rest.mjs"
        ))
        .args([venue_name(v), family])
        .env("TZ", "UTC")
        .output()
        .expect("public REST source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        cases.len(),
        match family {
            "markets" => 17,
            "tickers" => 15,
            "ohlcv" => 18,
            _ => panic!(),
        },
        "fixed source case floor"
    );
    cases
}
// JS has one Number type. Normalize JSON integer/float spelling only after all
// decoded native value/identity/count/error assertions; never coerce strings.
fn numeric_projection(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(numeric_projection).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (k.clone(), numeric_projection(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn compare(actual: Vec<Value>, v: RestVenue, family: &str) {
    let source = oracle(v, family);
    for (i, (native, source)) in actual.iter().zip(source.iter()).enumerate() {
        assert_eq!(
            numeric_projection(native),
            numeric_projection(source),
            "source {family} {} case{i} {}",
            venue_name(v),
            native["name"]
        );
    }
}
fn assert_original(e: &Failure, b: &Bindings, label: &str) {
    assert!(
        Rc::ptr_eq(e, &b.error),
        "{label}: native original error identity"
    );
    assert_eq!(e.name, "RangeError", "{label}: native original name");
    assert_eq!(
        e.message, "public fixture",
        "{label}: native original message"
    );
    assert_eq!(
        observed_error_code(&e.code),
        Some(json!("fixture-code")),
        "{label}: native original code"
    );
    assert_eq!(
        e.response_json_view(),
        Some(json!({"status":503,"data":{"msg":"venue detail"}})),
        "{label}: native original response"
    );
}
fn emission(b: &Bindings, v: RestVenue, original: bool, expected_message: &str, label: &str) {
    assert_eq!(b.emissions.len(), 1, "{label}: native emission count");
    match &b.emissions[0] {
        ErrorEmission::Error(e, prefix) => {
            assert_eq!(v, RestVenue::Futures, "{label}: native error emission kind");
            if original {
                assert_original(e, b, label)
            } else {
                assert!(
                    !Rc::ptr_eq(e, &b.error),
                    "{label}: native generated error identity"
                );
                // Expected message is independently authored by the case table:
                // validators create Error; coercion/property failures TypeError.
                let expected_name = if expected_message.starts_with("Binance ") {
                    "Error"
                } else {
                    "TypeError"
                };
                assert_eq!(
                    e.name, expected_name,
                    "{label}: native generated error name"
                );
                assert_eq!(
                    e.message, expected_message,
                    "{label}: native generated message"
                );
                assert_eq!(
                    observed_error_code(&e.code),
                    None,
                    "{label}: native generated code"
                );
                assert_eq!(
                    e.response_json_view(),
                    None,
                    "{label}: native generated response"
                );
            }
            assert_eq!(
                prefix.as_deref(),
                Some(format!("Exchange Error: {expected_message}").as_str()),
                "{label}: native error prefix"
            );
        }
        ErrorEmission::Message(message) => {
            assert_ne!(
                v,
                RestVenue::Futures,
                "{label}: native message emission kind"
            );
            assert_eq!(
                message,
                &Some(json!(expected_message)),
                "{label}: native emitted decoded message"
            );
        }
    }
}
fn error_message(v: RestVenue, family: &str, name: &str) -> String {
    let stem = match v {
        RestVenue::Futures => "Binance",
        RestVenue::PM => "Binance PM",
        RestVenue::Spot => "Spot",
    };
    let prefix = match v {
        RestVenue::Futures => "/fapi/v1",
        RestVenue::PM => "/fapi/v1",
        RestVenue::Spot => "/api/v3",
    };
    match (family, name) {
        (_, "get-sync-fail" | "get-reject" | "map-fail" | "map-null-order") => {
            "venue detail".into()
        }
        ("markets", "string-brackets") => {
            "Cannot use 'in' operator to search for 'initialLeverage' in A".into()
        }
        ("markets", "coercion-order") => "Cannot convert object to primitive value".into(),
        ("markets", "bad-symbols") => {
            if v == RestVenue::Spot {
                "symbols.map is not a function".into()
            } else {
                format!(
                    "{stem} {prefix}/exchangeInfo.symbols response malformed: expected array, received object(code,msg) (code=-1 symbols malformed)"
                )
            }
        }
        ("markets", "bad-brackets") => format!(
            "{stem} {}/leverageBracket response malformed: expected array, received object(code,msg) (code=-2 brackets malformed)",
            if v == RestVenue::PM {
                "/papi/v1/um"
            } else {
                "/fapi/v1"
            }
        ),
        ("markets", "null-data") => {
            if v == RestVenue::Spot {
                "Cannot read properties of null (reading 'symbols')".into()
            } else {
                format!(
                    "{stem} {prefix}/exchangeInfo.symbols response malformed: expected array, received undefined"
                )
            }
        }
        ("markets", "emit-fail" | "fallback-fail") => {
            if v == RestVenue::Spot {
                "Cannot read properties of null (reading 'map')".into()
            } else {
                format!(
                    "{stem} {prefix}/exchangeInfo.symbols response malformed: expected array, received null"
                )
            }
        }
        ("tickers", "bad-daily" | "emit-fail" | "fallback-fail") => {
            if v == RestVenue::Spot {
                "dailys.reduce is not a function".into()
            } else {
                format!(
                    "{stem} {prefix}/ticker/24hr response malformed: expected array, received object(code,msg) (code=-3 daily malformed)"
                )
            }
        }
        ("tickers", "bad-book") => format!(
            "{stem} {prefix}/ticker/bookTicker response malformed: expected array, received object"
        ),
        ("tickers", "bad-price") => format!(
            "{stem} {prefix}/premiumIndex response malformed: expected array, received object"
        ),
        ("tickers", "null-row") => "Cannot read properties of null (reading 'symbol')".into(),
        _ => panic!("undefined expected error{family}/{name}"),
    }
}
fn markets_test(v: RestVenue) {
    let mut projected = Vec::new();
    for (i, c) in fixtures("markets").into_iter().enumerate() {
        let mut b = Bindings::new(c);
        let name = b.name().to_string();
        let label = format!("native markets {} case{i} {name}", venue_name(v));
        let out = run(async {
            match v {
                RestVenue::Futures => fetch_futures_markets(&mut b).await,
                RestVenue::PM => fetch_pm_markets(&mut b).await,
                RestVenue::Spot => fetch_spot_markets(&mut b).await,
            }
        });
        let generated = matches!(
            name.as_str(),
            "bad-symbols" | "null-data" | "emit-fail" | "fallback-fail" | "coercion-order"
        ) || v != RestVenue::Spot
            && matches!(name.as_str(), "bad-brackets" | "string-brackets");
        let original = matches!(name.as_str(), "get-sync-fail" | "get-reject");
        let fallback = generated || original;
        let calls = if v == RestVenue::Spot
            || matches!(
                name.as_str(),
                "public-only"
                    | "late-public"
                    | "bad-symbols"
                    | "null-data"
                    | "emit-fail"
                    | "fallback-fail"
                    | "get-sync-fail"
                    | "get-reject"
            ) {
            1
        } else {
            2
        };
        assert_eq!(b.calls, calls, "{label}: native HTTP count");
        let first = if v == RestVenue::Spot {
            "/api/v3/exchangeInfo"
        } else {
            "/fapi/v1/exchangeInfo"
        };
        assert_eq!(b.requests[0].1, first, "{label}: native market endpoint");
        assert_eq!(
            b.requests[0].0,
            if v == RestVenue::Spot {
                RestClient::Private
            } else {
                RestClient::Public
            },
            "{label}: native market client"
        );
        if calls == 2 {
            assert_eq!(
                b.requests[1].1,
                if v == RestVenue::PM {
                    "/papi/v1/um/leverageBracket"
                } else {
                    "/fapi/v1/leverageBracket"
                },
                "{label}: native bracket endpoint"
            );
            assert_eq!(
                b.requests[1].0,
                if v == RestVenue::PM {
                    RestClient::Papi
                } else {
                    RestClient::Private
                },
                "{label}: native bracket client"
            );
        }
        if fallback {
            emission(&b, v, original, &error_message(v, "markets", &name), &label);
        } else {
            assert!(b.emissions.is_empty(), "{label}: native no emission");
        }
        match out {
            Err(e) => {
                assert!(
                    matches!(name.as_str(), "emit-fail" | "fallback-fail"),
                    "{label}: native failure path"
                );
                assert_original(&e, &b, &label);
                projected.push(b.wire(None, false, Some(&e)));
            }
            Ok(out) => {
                assert!(
                    !matches!(name.as_str(), "emit-fail" | "fallback-fail"),
                    "{label}: native success path"
                );
                assert_eq!(
                    Rc::ptr_eq(&out, &b.market_cache),
                    fallback,
                    "{label}: native fallback array identity"
                );
                let count = if fallback {
                    1
                } else if name == "missing-filters" {
                    if v == RestVenue::PM { 0 } else { 1 }
                } else if matches!(name.as_str(), "object-symbols" | "numeric-symbols") {
                    1
                } else if v == RestVenue::Spot {
                    3
                } else {
                    2
                };
                assert_eq!(out.len(), count, "{label}: native market count");
                if fallback {
                    assert_eq!(
                        out[0],
                        cached_market(),
                        "{label}: native unchanged cached value"
                    );
                } else {
                    for (row, m) in out.iter().enumerate() {
                        let row_label = format!("{label} row{row}");
                        let base = if row == 0 {
                            "BTC"
                        } else if row == 1 {
                            "ETH"
                        } else {
                            "SKIP"
                        };
                        let quote = if row == 1 { "USDC" } else { "USDT" };
                        assert_eq!(
                            m.id,
                            if v == RestVenue::Spot {
                                format!("{base}/{quote}")
                            } else {
                                format!("{base}/{quote}:{quote}")
                            },
                            "{row_label}: native id"
                        );
                        assert_eq!(m.base, Some(json!(base)), "{row_label}: native base");
                        assert_eq!(m.quote, Some(json!(quote)), "{row_label}: native quote");
                        assert_eq!(
                            m.symbol,
                            Some(if name == "object-symbols" {
                                json!({"n":1})
                            } else if name == "numeric-symbols" {
                                json!(1)
                            } else {
                                json!(if row == 0 {
                                    "BTCUSDT"
                                } else if row == 1 {
                                    "ETHUSDC"
                                } else {
                                    "SKIP"
                                })
                            }),
                            "{row_label}: native decoded symbol"
                        );
                        assert_eq!(m.active, row != 1, "{row_label}: native active");
                        assert_eq!(
                            m.is_spot,
                            if v == RestVenue::Spot {
                                Some(true)
                            } else {
                                None
                            },
                            "{row_label}: native spot presence"
                        );
                        let missing = name == "missing-filters";
                        eq_number(
                            m.precision.amount,
                            if missing {
                                if v == RestVenue::Spot {
                                    0.001
                                } else {
                                    f64::NAN
                                }
                            } else if row == 1 {
                                2.0
                            } else {
                                0.1
                            },
                            &format!("{row_label}: native amount precision"),
                        );
                        eq_number(
                            m.precision.price,
                            if missing {
                                if v == RestVenue::Spot {
                                    0.0001
                                } else {
                                    f64::NAN
                                }
                            } else if row == 1 {
                                0.05
                            } else {
                                0.25
                            },
                            &format!("{row_label}: native price precision"),
                        );
                        eq_number(
                            m.amount.min,
                            if missing {
                                if v == RestVenue::Spot { 0.0 } else { f64::NAN }
                            } else if v == RestVenue::Spot {
                                if row == 1 { 1.0 } else { 0.1 }
                            } else if row == 1 {
                                3.0
                            } else {
                                0.2
                            },
                            &format!("{row_label}: native minimum"),
                        );
                        eq_number(
                            safe_cex_num::numeric_value_to_number(&m.amount.max),
                            if missing {
                                if v == RestVenue::Spot {
                                    999_999_999.0
                                } else {
                                    f64::NAN
                                }
                            } else if v == RestVenue::Spot {
                                if row == 1 { 60.0 } else { 100.0 }
                            } else if row == 1 {
                                40.0
                            } else {
                                90.0
                            },
                            &format!("{row_label}: native maximum"),
                        );
                        eq_number(
                            m.leverage_min,
                            1.0,
                            &format!("{row_label}: native leverage minimum"),
                        );
                        let leverage = if v == RestVenue::Spot
                            || matches!(
                                name.as_str(),
                                "public-only" | "late-public" | "object-symbols"
                            )
                            || name == "falsy-leverage" && row == 0
                        {
                            json!(1)
                        } else if name == "numeric-symbols" {
                            json!(70)
                        } else if row == 0 {
                            json!("125")
                        } else {
                            json!(20)
                        };
                        assert_eq!(
                            m.leverage_max, leverage,
                            "{row_label}: native uncoerced leverage maximum"
                        );
                    }
                }
                projected.push(b.wire(
                    Some(json!(out.iter().map(market_wire).collect::<Vec<_>>())),
                    fallback,
                    None,
                ));
            }
        }
    }
    compare(projected, v, "markets");
}
fn tickers_test(v: RestVenue) {
    let mut projected = Vec::new();
    for (i, c) in fixtures("tickers").into_iter().enumerate() {
        let mut b = Bindings::new(c);
        let name = b.name().to_string();
        let label = format!("native tickers {} case{i} {name}", venue_name(v));
        let out = run(async {
            match v {
                RestVenue::Futures => fetch_futures_tickers(&mut b).await,
                RestVenue::PM => fetch_pm_tickers(&mut b).await,
                RestVenue::Spot => fetch_spot_tickers(&mut b).await,
            }
        });
        let original = matches!(
            name.as_str(),
            "get-sync-fail" | "get-reject" | "map-fail" | "map-null-order"
        );
        let generated = matches!(
            name.as_str(),
            "bad-daily" | "null-row" | "emit-fail" | "fallback-fail"
        ) || v != RestVenue::Spot
            && matches!(name.as_str(), "bad-book" | "bad-price");
        let fallback = original || generated;
        let calls = if v == RestVenue::Spot
            || matches!(
                name.as_str(),
                "bad-daily" | "get-sync-fail" | "get-reject" | "emit-fail" | "fallback-fail"
            ) {
            1
        } else if name == "bad-book" {
            2
        } else {
            3
        };
        assert_eq!(b.calls, calls, "{label}: native HTTP count");
        let eps = if v == RestVenue::Spot {
            vec!["/api/v3/ticker/24hr"]
        } else {
            vec![
                "/fapi/v1/ticker/24hr",
                "/fapi/v1/ticker/bookTicker",
                "/fapi/v1/premiumIndex",
            ]
        };
        for (r, (client, ep, _)) in b.requests.iter().enumerate() {
            assert_eq!(*ep, eps[r], "{label}: native endpoint{r}");
            assert_eq!(
                *client,
                if v == RestVenue::Spot {
                    RestClient::Private
                } else {
                    RestClient::Public
                },
                "{label}: native client{r}"
            );
        }
        if fallback {
            emission(&b, v, original, &error_message(v, "tickers", &name), &label);
        } else {
            assert!(b.emissions.is_empty(), "{label}: native no emissions");
        }
        match out {
            Err(e) => {
                assert!(
                    matches!(name.as_str(), "emit-fail" | "fallback-fail"),
                    "{label}: native failure path"
                );
                assert_original(&e, &b, &label);
                projected.push(b.wire(None, false, Some(&e)));
            }
            Ok(out) => {
                assert!(
                    !matches!(name.as_str(), "emit-fail" | "fallback-fail"),
                    "{label}: native success path"
                );
                assert_eq!(
                    Rc::ptr_eq(&out, &b.ticker_cache),
                    fallback,
                    "{label}: native fallback identity"
                );
                let count = if fallback {
                    1
                } else if matches!(name.as_str(), "empty" | "falsy-match")
                    || v != RestVenue::Spot && name == "missing-matches"
                {
                    0
                } else if name == "duplicates" {
                    2
                } else {
                    1
                };
                assert_eq!(out.len(), count, "{label}: native ticker count");
                for (row, t) in out.iter().enumerate() {
                    if fallback {
                        assert_eq!(t, &cached_ticker(), "{label}: native cached ticker");
                        continue;
                    }
                    let r = format!("{label} row{row}");
                    assert_eq!(t.id, "BTC/USDT", "{r}: native id");
                    assert_eq!(t.symbol, "BTCUSDT", "{r}: native symbol");
                    eq_number(t.bid, 10.1, &format!("{r}: native bid"));
                    eq_number(t.ask, 10.9, &format!("{r}: native ask"));
                    let last = if v == RestVenue::Spot && name == "duplicates" && row == 1 {
                        99.0
                    } else {
                        12.3
                    };
                    eq_number(t.last, last, &format!("{r}: native last"));
                    eq_number(
                        t.mark,
                        if v == RestVenue::Spot { last } else { 12.4 },
                        &format!("{r}: native mark"),
                    );
                    eq_number(
                        t.index,
                        if v == RestVenue::Spot { last } else { 12.5 },
                        &format!("{r}: native index"),
                    );
                    eq_number(t.percentage, 3.6, &format!("{r}: native percentage"));
                    eq_number(
                        t.funding_rate,
                        if v == RestVenue::Spot { 0.0 } else { 0.002 },
                        &format!("{r}: native funding rate"),
                    );
                    eq_number(
                        t.funding_time,
                        if v == RestVenue::Spot { 0.0 } else { 8000.0 },
                        &format!("{r}: native funding time"),
                    );
                    eq_number(t.volume, 27.1, &format!("{r}: native volume"));
                    eq_number(t.quote_volume, 44.2, &format!("{r}: native quote volume"));
                    eq_number(t.open_interest, 0.0, &format!("{r}: native open interest"));
                    assert!(
                        t.mid_price.is_none() && t.timestamp.is_none() && t.info.is_none(),
                        "{r}: native omitted optional fields"
                    );
                }
                projected.push(b.wire(
                    Some(json!(out.iter().map(ticker_wire).collect::<Vec<_>>())),
                    fallback,
                    None,
                ));
            }
        }
    }
    compare(projected, v, "tickers");
}
fn ohlcv_test(v: RestVenue) {
    let mut projected = Vec::new();
    for (i, c) in fixtures("ohlcv").into_iter().enumerate() {
        let mut b = Bindings::new(c);
        let name = b.name().to_string();
        let label = format!("native ohlcv {} case{i} {name}", venue_name(v));
        let mut opts: OHLCVOptions = serde_json::from_value(b.c["opts"].clone()).unwrap();
        if b.c["nan_limit"] == true {
            opts.limit = Some(f64::NAN)
        }
        let out = run(async {
            match v {
                RestVenue::Futures => fetch_futures_ohlcv(&mut b, &opts).await,
                RestVenue::PM => fetch_pm_ohlcv(&mut b, &opts).await,
                RestVenue::Spot => fetch_spot_ohlcv(&mut b, &opts).await,
            }
        });
        let calls = if matches!(name.as_str(), "date-fail" | "subtract-fail") {
            0
        } else {
            1
        };
        assert_eq!(b.calls, calls, "{label}: native HTTP count");
        assert!(b.emissions.is_empty(), "{label}: native no catch/emission");
        if calls == 1 {
            let (client, ep, params) = &b.requests[0];
            assert_eq!(
                *ep,
                if v == RestVenue::Spot {
                    "/api/v3/klines"
                } else {
                    "/fapi/v1/klines"
                },
                "{label}: native endpoint"
            );
            assert_eq!(
                *client,
                if v == RestVenue::Spot {
                    RestClient::Private
                } else {
                    RestClient::Public
                },
                "{label}: native client"
            );
            let p = params.as_ref().unwrap();
            assert_eq!(p.symbol, "BTCUSDT", "{label}: native request symbol");
            assert_eq!(
                p.interval,
                match name.as_str() {
                    "hour" => "2h",
                    "day" => "1d",
                    "week" => "1w",
                    _ => "5m",
                },
                "{label}: native interval"
            );
            eq_number(
                p.end_time,
                if name == "zero-limit" {
                    200_000_000.0
                } else {
                    120_000_000.0
                },
                &format!("{label}: native request end"),
            );
            eq_number(
                p.start_time,
                match name.as_str() {
                    "default" | "nan-limit" => -30_000_000.0,
                    "zero-limit" => 50_000_000.0,
                    "from" => 110_000_000.0,
                    "capped" => -330_000_000.0,
                    "negative" => 120_600_000.0,
                    "hour" => 105_600_000.0,
                    "day" => 33_600_000.0,
                    "week" => -139_200_000.0,
                    _ => 119_400_000.0,
                },
                &format!("{label}: native request start"),
            );
            assert_eq!(
                p.limit,
                match name.as_str() {
                    "default" | "zero-limit" | "nan-limit" | "from" => None,
                    "capped" => Some(1500.0),
                    "negative" => Some(-2.0),
                    "day" => Some(1.5),
                    "week" => Some(0.5),
                    _ => Some(2.0),
                },
                "{label}: native optional limit"
            );
        }
        match out {
            Err(e) => {
                if matches!(
                    name.as_str(),
                    "get-sync-fail" | "get-reject" | "date-fail" | "subtract-fail"
                ) {
                    assert_original(&e, &b, &label)
                } else {
                    assert!(
                        !Rc::ptr_eq(&e, &b.error),
                        "{label}: native generated error identity"
                    );
                    let (name_e, msg) = if name == "null-row" {
                        (
                            "TypeError",
                            "object null is not iterable (cannot read property Symbol(Symbol.iterator))",
                        )
                    } else if name == "malformed" && v == RestVenue::PM {
                        (
                            "Error",
                            "Binance PM /fapi/v1/klines response malformed: expected array, received object(code,msg) (code=-4 candle malformed)",
                        )
                    } else if name == "malformed" && v == RestVenue::Spot {
                        ("TypeError", "data.map is not a function")
                    } else {
                        panic!("unexpected native error{label}")
                    };
                    assert_eq!(e.name, name_e, "{label}: native generated name");
                    assert_eq!(e.message, msg, "{label}: native generated message");
                    assert_eq!(
                        observed_error_code(&e.code),
                        None,
                        "{label}: native generated code"
                    );
                    assert_eq!(
                        e.response_json_view(),
                        None,
                        "{label}: native generated response"
                    );
                }
                projected.push(b.wire(None, false, Some(&e)));
            }
            Ok(out) => {
                assert!(
                    !matches!(
                        name.as_str(),
                        "get-sync-fail" | "get-reject" | "date-fail" | "subtract-fail" | "null-row"
                    ),
                    "{label}: native success path"
                );
                assert_eq!(
                    out.len(),
                    if name == "malformed" {
                        assert_eq!(v, RestVenue::Futures, "{label}: native nonarray policy");
                        0
                    } else if matches!(name.as_str(), "short-row" | "string-row") {
                        1
                    } else {
                        2
                    },
                    "{label}: native candle count"
                );
                for (row, c) in out.iter().enumerate() {
                    let r = format!("{label} row{row}");
                    let values = if name == "string-row" {
                        [0.001, 2.0, 3.0, 4.0, 5.0, 6.0]
                    } else if name == "short-row" {
                        [121.0, 1.1, f64::NAN, f64::NAN, f64::NAN, f64::NAN]
                    } else if row == 0 {
                        [121.0, 1.1, 5.2, 0.3, 4.4, 9.5]
                    } else {
                        [122.0, 2.2, 6.3, 1.4, 5.5, 10.6]
                    };
                    for (field, a, b) in ["timestamp", "open", "high", "low", "close", "volume"]
                        .into_iter()
                        .zip([c.timestamp, c.open, c.high, c.low, c.close, c.volume])
                        .zip(values)
                        .map(|((f, a), b)| (f, a, b))
                    {
                        eq_number(a, b, &format!("{r}: native {field}"))
                    }
                    assert!(c.last_side.is_none(), "{r}: native omitted lastSide");
                }
                projected.push(b.wire(
                    Some(json!(out.iter().map(candle_wire).collect::<Vec<_>>())),
                    false,
                    None,
                ));
            }
        }
    }
    compare(projected, v, "ohlcv");
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_markets() {
    markets_test(RestVenue::Futures)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_markets() {
    markets_test(RestVenue::PM)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_markets() {
    markets_test(RestVenue::Spot)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_tickers() {
    tickers_test(RestVenue::Futures)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_tickers() {
    tickers_test(RestVenue::PM)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_tickers() {
    tickers_test(RestVenue::Spot)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn futures_ohlcv() {
    ohlcv_test(RestVenue::Futures)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn pm_ohlcv() {
    ohlcv_test(RestVenue::PM)
}
#[test]
#[ignore = "requires direct TypeScript oracle"]
fn spot_ohlcv() {
    ohlcv_test(RestVenue::Spot)
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
