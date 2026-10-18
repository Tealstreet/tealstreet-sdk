use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_venue_phemex::{
    ticker_reads::fetch_tickers_raw_value,
    utilities::{format_symbol_in_value, format_symbol_out_value},
};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
#[test]
fn symbol_in_preserves_every_lone_surrogate_unit() {
    for unit in 0xd800..=0xdfff {
        let v = format_symbol_in_value(Js::utf16(&[unit, 97])).unwrap();
        assert_eq!(
            v.to_js_string().unwrap().as_ref(),
            [unit, 65],
            "PHEMEX lossless symbol in"
        );
    }
}
#[test]
fn symbol_out_preserves_raw_info_units() {
    let symbol = Js::str("BTCUSDT");
    let raw = Js::utf16(&[0xd800]);
    let markets = Js::array(vec![Js::object(vec![
        ("symbol", symbol.clone()),
        ("info", Js::object(vec![("symbol", raw.clone())])),
    ])]);
    let v = format_symbol_out_value(&markets, symbol).unwrap();
    assert_eq!(
        v.to_js_string().unwrap().as_ref(),
        [0xd800],
        "PHEMEX lossless symbol out"
    );
}
#[test]
fn raw_tickers_preserve_response_identity_and_units() {
    let raw = Js::object(vec![
        ("error", Js::Null),
        (
            "result",
            Js::array(vec![Js::object(vec![(
                "symbol",
                Js::utf16(&[0xd800, 85, 83, 68, 84]),
            )])]),
        ),
    ]);
    let future = fetch_tickers_raw_value(
        |endpoint| {
            assert_eq!(endpoint, "/md/v3/ticker/24hr/all");
            let raw = raw.clone();
            async move { Ok(raw) }
        },
        |data| data.get("error"),
        |_| panic!("null error must not emit"),
    );
    let Poll::Ready(Ok(value)) = std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("PHEMEX raw ticker future settles")
    };
    assert!(value.same_reference(&raw), "PHEMEX raw identity");
    let symbol = value
        .get("result")
        .unwrap()
        .get("0")
        .unwrap()
        .get("symbol")
        .unwrap();
    assert_eq!(
        symbol.to_js_string().unwrap().as_ref(),
        [0xd800, 85, 83, 68, 84],
        "PHEMEX lossless raw symbol"
    );
}

#[test]
fn value_helpers_match_actual_typescript_surrogate_observations() {
    let run = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/binding-values-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "PHEMEX actual source: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    let uppercase = source["uppercase"].as_array().unwrap();
    assert_eq!(uppercase.len(), 2048, "PHEMEX source surrogate census");
    for (i, expected) in uppercase.iter().enumerate() {
        let units = vec![0xd800 + i as u16, 97];
        let actual = format_symbol_in_value(Js::utf16(&units)).unwrap();
        assert_eq!(
            serde_json::to_value(actual.to_js_string().unwrap().as_ref()).unwrap(),
            *expected,
            "PHEMEX actual source uppercase surrogate"
        );
    }
    let raw = Js::utf16(&[0xd800]);
    let markets = Js::array(vec![Js::object(vec![
        ("symbol", Js::str("BTCUSDT")),
        ("info", Js::object(vec![("symbol", raw)])),
    ])]);
    let actual = format_symbol_out_value(&markets, Js::str("BTCUSDT")).unwrap();
    assert_eq!(
        serde_json::to_value(actual.to_js_string().unwrap().as_ref()).unwrap(),
        source["output"],
        "PHEMEX actual source raw output symbol"
    );
    assert_eq!(source["raw"], serde_json::json!([0xd800, 85, 83, 68, 84]));
    assert_eq!(
        source["same"], true,
        "PHEMEX actual source raw response identity"
    );
}
