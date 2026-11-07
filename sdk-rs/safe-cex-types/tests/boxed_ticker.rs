use safe_cex_types::{Ticker, gateway::GatewayTickerData};
use serde_json::json;

#[test]
fn boxed_ticker_keeps_native_fields_and_untagged_wire_shapes() {
    let first = json!({"id":"first","symbol":"FIRST","bid":1.0,"ask":2.0,"last":3.0,
        "mark":4.0,"index":5.0,"percentage":6.0,"openInterest":7.0,"fundingRate":8.0,
        "fundingTime":9.0,"volume":10.0,"quoteVolume":11.0,"timestamp":123.25});
    let second = json!({"id":"second","symbol":"SECOND","bid":13.0,"ask":14.0,"last":15.0,
        "mark":16.0,"index":17.0,"percentage":18.0,"openInterest":19.0,"fundingRate":20.0,
        "fundingTime":21.0,"volume":22.0,"quoteVolume":23.0,"timestamp":456.75});
    let singular: GatewayTickerData =
        serde_json::from_value(first.clone()).expect("BOXED-TICKER singular native decode");
    let plural: GatewayTickerData = serde_json::from_value(json!([first.clone(), second.clone()]))
        .expect("BOXED-TICKER plural native decode");
    let GatewayTickerData::Ticker(ticker) = &singular else {
        panic!("BOXED-TICKER singular native variant");
    };
    let GatewayTickerData::Tickers(tickers) = &plural else {
        panic!("BOXED-TICKER plural native variant");
    };
    assert_eq!(tickers.len(), 2, "BOXED-TICKER independent plural count");
    let mut comparisons = 0;
    for (native, id, symbol, start, timestamp) in [
        (ticker.as_ref(), "first", "FIRST", 1.0_f64, 123.25_f64),
        (&tickers[0], "first", "FIRST", 1.0, 123.25),
        (&tickers[1], "second", "SECOND", 13.0, 456.75),
    ] {
        assert_eq!(native.id, id, "BOXED-TICKER native id identity");
        assert_eq!(native.symbol, symbol, "BOXED-TICKER native symbol identity");
        for (field, actual, expected) in numeric_fields(native, start, timestamp) {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "BOXED-TICKER native field identity {id}.{field}"
            );
            comparisons += 1;
        }
        comparisons += 2;
    }
    assert_eq!(comparisons, 42, "BOXED-TICKER independent comparison count");
    // Every native assertion above precedes these independently specified wire assertions.
    assert_eq!(
        serde_json::to_value(singular).unwrap(),
        first,
        "BOXED-TICKER singular object wire shape"
    );
    assert_eq!(
        serde_json::to_value(plural).unwrap(),
        json!([first, second]),
        "BOXED-TICKER plural array wire shape"
    );
}

fn numeric_fields(ticker: &Ticker, start: f64, timestamp: f64) -> [(&'static str, f64, f64); 12] {
    [
        ("bid", ticker.bid, start),
        ("ask", ticker.ask, start + 1.0),
        ("last", ticker.last, start + 2.0),
        ("mark", ticker.mark, start + 3.0),
        ("index", ticker.index.unwrap(), start + 4.0),
        ("percentage", ticker.percentage, start + 5.0),
        ("openInterest", ticker.open_interest, start + 6.0),
        ("fundingRate", ticker.funding_rate, start + 7.0),
        ("fundingTime", ticker.funding_time, start + 8.0),
        ("volume", ticker.volume, start + 9.0),
        ("quoteVolume", ticker.quote_volume, start + 10.0),
        (
            "timestamp",
            ticker
                .timestamp
                .expect("BOXED-TICKER native timestamp presence"),
            timestamp,
        ),
    ]
}
