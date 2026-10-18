use safe_cex_num::{
    JsCoercionError,
    numeric_ingress::{js_deserialize_passthrough, js_from_str},
};
use safe_cex_types::{Balance, Market, Position, PositionMode, Ticker};
use safe_cex_venue_bitunix::rest_reads::{
    fetch_balance, fetch_markets, fetch_position_mode, fetch_tickers,
};
use serde_json::{Value, json};
use std::{
    future::{Future, ready},
    pin::pin,
    process::Command,
    task::{Context, Poll, Waker},
};

#[derive(Debug)]
struct ReadError {
    cause: JsCoercionError,
    exchange_message: Option<String>,
}
impl From<JsCoercionError> for ReadError {
    fn from(cause: JsCoercionError) -> Self {
        Self {
            cause,
            exchange_message: None,
        }
    }
}
fn resolve<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("REST mocked reads must be immediately ready"),
    }
}
fn number(native: f64, source: &Value, label: &str) {
    assert_eq!(source["type"], "number", "REST source number {label}");
    let expected = u64::from_str_radix(source["bits"].as_str().unwrap(), 16).unwrap();
    let bits = if native.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        native.to_bits()
    };
    assert_eq!(bits, expected, "REST native {label}");
}
fn string(native: &str, source: &Value, label: &str) {
    assert_eq!(source["type"], "string", "REST source string {label}");
    assert_eq!(
        native,
        source["value"].as_str().unwrap(),
        "REST native {label}"
    );
}
fn market(native: &Market, source: &Value, numeric_max: &Value) {
    let f = &source["fields"];
    for (label, value) in [
        ("id", native.id.as_str()),
        ("symbol", native.symbol.as_str()),
        ("base", native.base.as_str()),
        ("quote", native.quote.as_str()),
    ] {
        string(value, &f[label], label);
    }
    assert_eq!(
        native.active,
        f["active"]["value"].as_bool().unwrap(),
        "REST native market active"
    );
    number(
        native.precision.amount,
        &f["precision"]["fields"]["amount"],
        "amount precision",
    );
    number(
        native.precision.price,
        &f["precision"]["fields"]["price"],
        "price precision",
    );
    let limits = &f["limits"]["fields"];
    if let safe_cex_types::NumericValue::Text(value) = &native.limits.amount.max {
        string(
            value,
            &limits["amount"]["fields"]["max"],
            "raw amount maximum",
        );
    }

    number(
        native.limits.amount.min,
        &limits["amount"]["fields"]["min"],
        "amount min",
    );
    number(
        safe_cex_num::numeric_value_to_number(&native.limits.amount.max),
        numeric_max,
        "amount max at shared numeric model boundary",
    );
    number(
        native.limits.leverage.min,
        &limits["leverage"]["fields"]["min"],
        "leverage min",
    );
    number(
        native.limits.leverage.max,
        &limits["leverage"]["fields"]["max"],
        "leverage max",
    );
}
fn ticker(native: &Ticker, source: &Value) {
    assert!(
        native.timestamp.is_none(),
        "REST native absent ticker timestamp"
    );
    let f = &source["fields"];
    string(&native.id, &f["id"], "ticker id");
    string(&native.symbol, &f["symbol"], "ticker symbol");
    for (label, value) in [
        ("bid", native.bid),
        ("ask", native.ask),
        ("last", native.last),
        ("mark", native.mark),
        ("index", native.index),
        ("percentage", native.percentage),
        ("fundingRate", native.funding_rate),
        ("fundingTime", native.funding_time),
        ("volume", native.volume),
        ("quoteVolume", native.quote_volume),
        ("openInterest", native.open_interest),
    ] {
        number(value, &f[label], label);
    }
}
fn balance(native: &Balance, source: &Value) {
    let f = &source["fields"];
    for (label, value) in [
        ("total", native.total),
        ("free", native.free),
        ("used", native.used),
        ("upnl", native.upnl),
    ] {
        number(value, &f[label], label);
    }
}
#[test]
fn actual_typescript_rest_reads() {
    let out = Command::new("node")
        .arg(format!(
            "{}/tests/rest-reads-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "REST actual source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    eprintln!("oracle runtime: {}", String::from_utf8_lossy(&out.stderr));
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 33, "REST independent source total");
    let asset_rows = corpus["assetRows"].as_array().unwrap();
    assert_eq!(asset_rows.len(), 5, "independent source asset cases");
    for (row, expected_type) in
        asset_rows
            .iter()
            .zip(["boolean", "object", "null", "number", "undefined"])
    {
        let outputs = row["result"]["items"].as_array().unwrap();
        assert_eq!(
            outputs.len(),
            3,
            "TS retains the bad row and valid siblings"
        );
        assert_eq!(outputs[1]["fields"]["base"]["type"], expected_type);
        assert_eq!(outputs[0]["fields"]["base"]["value"], "BTC");
        assert_eq!(outputs[2]["fields"]["base"]["value"], "ETH");
        assert_eq!(row["traceCount"], 0);
    }
    let scalar_rows = corpus["scalarRows"].as_array().unwrap();
    assert_eq!(
        scalar_rows.len(),
        6,
        "independent raw symbol and maximum cases"
    );
    for row in scalar_rows {
        assert_eq!(row["result"]["items"].as_array().unwrap().len(), 3);
        assert_eq!(row["traceCount"], 0);
    }
    assert_eq!(
        scalar_rows[5]["result"]["items"][1]["fields"]["limits"]["fields"]["amount"]["fields"]["max"]
            ["type"],
        "object"
    );
    let cached_market: Market = js_from_str(&corpus["market"].to_string()).unwrap();
    let cached_ticker: Ticker = js_from_str(&corpus["ticker"].to_string()).unwrap();
    let cached_balance: Balance = js_from_str(&corpus["balance"].to_string()).unwrap();
    let positions: Vec<Position> = js_from_str(&corpus["positions"].to_string()).unwrap();
    assert_eq!(
        positions.len(),
        1,
        "REST independent decoded position count"
    );
    let expected_lengths = [1, 2, 1, 1, 1, 1, 1, 2, 1, 1, 0, 1, 1];
    let expected_reports = [
        0, 0, 0, 0, 1, 1, 1, 0, 0, 1, 0, 1, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0,
    ];
    let mut counts = [0; 4];
    let mut market_rows = 0;
    let mut ticker_rows = 0;
    let mut numeric_market_boundaries = 0;
    for (index, row) in rows.iter().enumerate() {
        let payload: Value = js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
            row["raw"].as_str().unwrap(),
        ))
        .unwrap();
        let fail = payload.get("fail").and_then(Value::as_bool) == Some(true);
        let mut calls = Vec::new();
        let mut reports = 0;
        let mut emits = Vec::new();
        let mut mode = PositionMode::OneWay;
        let read = |url: &'static str, config: Value| {
            calls.push(json!({"url":url,"config":config}));
            ready(if fail {
                Err(ReadError {
                    cause: JsCoercionError {
                        name: "Error",
                        message: "wire failure",
                    },
                    exchange_message: Some("venue failure".into()),
                })
            } else {
                Ok(Some(payload.clone()))
            })
        };
        match row["method"].as_str().unwrap() {
            "fetchMarkets" => {
                let native = resolve(fetch_markets(
                    || Ok(vec![cached_market.clone()]),
                    read,
                    |_: &ReadError| reports += 1,
                    |message| panic!("source corpus has no typed asset refusals: {message}"),
                ))
                .unwrap();
                assert_eq!(
                    native.len(),
                    expected_lengths[index],
                    "REST independent native markets length {index}"
                );
                assert_eq!(
                    row["result"]["items"].as_array().unwrap().len(),
                    expected_lengths[index],
                    "REST independent source markets length"
                );
                for (i, value) in native.iter().enumerate() {
                    market(value, &row["result"]["items"][i], &row["numericMax"][i]);
                    market_rows += 1;
                    let max = &row["result"]["items"][i]["fields"]["limits"]["fields"]["amount"]["fields"]
                        ["max"];
                    if index == 0 || index == 1 || index == 3 {
                        assert_eq!(
                            max["type"], "string",
                            "REST observed source string amount limit"
                        );
                        numeric_market_boundaries += 1;
                    }
                }
                counts[0] += 1;
            }
            "fetchTickers" => {
                let native = resolve(fetch_tickers(
                    || Ok(vec![cached_ticker.clone()]),
                    |symbol| Ok((symbol == cached_market.symbol).then_some(cached_market.clone())),
                    read,
                    |_: &ReadError| reports += 1,
                ))
                .unwrap();
                assert_eq!(
                    native.len(),
                    expected_lengths[index],
                    "REST independent native tickers length {index}"
                );
                assert_eq!(
                    row["result"]["items"].as_array().unwrap().len(),
                    expected_lengths[index],
                    "REST independent source tickers length"
                );
                for (i, value) in native.iter().enumerate() {
                    ticker(value, &row["result"]["items"][i]);
                    ticker_rows += 1;
                }
                counts[1] += 1;
            }
            "fetchBalance" => {
                let (native, returned_positions) = resolve(fetch_balance(
                    || Ok(cached_balance.clone()),
                    || Ok(positions.clone()),
                    &mut mode,
                    read,
                    |error: &ReadError| {
                        reports += 1;
                        emits.push(
                            error
                                .exchange_message
                                .clone()
                                .unwrap_or_else(|| error.cause.message.into()),
                        );
                    },
                ))
                .unwrap();
                balance(&native, &row["result"]["fields"]["balance"]);
                assert!(
                    returned_positions == positions,
                    "REST native positions identity"
                );
                assert_eq!(
                    returned_positions[0].contracts, 6.0,
                    "REST decoded native returned position"
                );
                assert_eq!(
                    row["result"]["fields"]["positions"]["items"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1,
                    "REST source returned positions length"
                );
                counts[2] += 1;
            }
            "fetchPositionMode" => {
                let result = resolve(fetch_position_mode(read));
                match result {
                    Ok(native) => assert_eq!(
                        native,
                        row["result"]["value"].as_bool().unwrap(),
                        "REST native hedged classification {index}"
                    ),
                    Err(error) => {
                        let source_error = if fail {
                            error.cause.message
                        } else {
                            error.cause.name
                        };
                        assert_eq!(
                            source_error,
                            row["error"].as_str().unwrap(),
                            "REST native propagated error"
                        );
                        if fail {
                            assert_eq!(
                                error.exchange_message.as_deref(),
                                Some("venue failure"),
                                "REST transport error retained"
                            );
                        }
                    }
                }
                counts[3] += 1;
            }
            name => panic!("unexpected source method {name}"),
        }
        let source_mode = if row["mode"] == "oneway" {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        };
        assert_eq!(
            mode, source_mode,
            "REST native account position mode after read {index}"
        );
        assert_eq!(
            reports, expected_reports[index],
            "REST independent native reporting count {index}"
        );
        assert_eq!(
            reports,
            row["traceCount"].as_u64().unwrap(),
            "REST source/native reporting count"
        );
        assert_eq!(
            json!(emits),
            row["emits"],
            "REST native emitted error message {index}"
        );
        assert_eq!(calls.len(), 1, "REST independent native request count");
        assert_eq!(
            row["calls"].as_array().unwrap().len(),
            1,
            "REST independent source request count"
        );
        assert_eq!(
            json!(calls),
            row["calls"],
            "REST native request route/config {index}"
        );
    }
    assert_eq!(
        counts,
        [7, 6, 10, 10],
        "REST independent native method counts"
    );
    assert_eq!(market_rows, 8, "REST independent native market rows");
    assert_eq!(ticker_rows, 6, "REST independent native ticker rows");
    assert_eq!(
        numeric_market_boundaries, 4,
        "REST observed string-limit numeric boundary cases"
    );
}
