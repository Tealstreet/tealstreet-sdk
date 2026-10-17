//! Public read paths on the constructed Bybit binding and its signed HTTP client.
#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_types::ExchangeName;
use serde_json::json;
use support::{harness_as, options};

fn candle_case(name: ExchangeName, category: &str) {
    let h = harness_as(name, options(), |wire| {
        wire.ok(
            "GET",
            "/v5/market/kline",
            json!({"list":[
                ["1700000060000","102","108","101","107","5"],
                ["1700000000000","100","103","99","102","3"]
            ]}),
        );
    });
    let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","from":1700000000000u64,"to":1700000060000u64,"limit":2})).unwrap();
    let candles = h
        .drive(h.venue().fetch_ohlcv(opts))
        .expect("production fetchOHLCV");
    assert_eq!(candles.len(), 2, "fetchOHLCV must use the Bybit member");
    assert_eq!(candles[0].timestamp, 1700000000.0);
    assert_eq!(
        (
            candles[0].open,
            candles[0].high,
            candles[0].low,
            candles[0].close,
            candles[0].volume
        ),
        (100.0, 103.0, 99.0, 102.0, 3.0)
    );
    assert_eq!(candles[1].timestamp, 1700000060.0);
    let calls = h.wire.calls("/v5/market/kline");
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].params["category"], category);
    assert_eq!(calls[0].params["interval"], "1");
    assert_eq!(calls[0].params["limit"], "2");
    assert_eq!(calls[0].params["start"], "1699999940000");
    assert_eq!(calls[0].params["end"], "1700000060000");
    h.venue().dispose().unwrap();
}

#[test]
fn primary_fetch_ohlcv_reaches_signed_http_and_maps_candles() {
    candle_case(ExchangeName::Bybit, "linear");
}
#[test]
fn demo_fetch_ohlcv_reaches_signed_http_and_maps_candles() {
    candle_case(ExchangeName::Bybitdemo, "linear");
}
#[test]
fn inverse_fetch_ohlcv_reaches_signed_http_and_maps_candles() {
    candle_case(ExchangeName::Bybitinverse, "inverse");
}
#[test]
fn spot_fetch_ohlcv_reaches_signed_http_and_maps_candles() {
    candle_case(ExchangeName::Bybitspot, "spot");
}

#[test]
fn primary_candle_errors_surface_the_source_api_and_shape_messages() {
    for (body, message) in [
        (
            json!({"retCode":10001,"retMsg":"bad interval","result":{}}),
            "Bybit KLINE error: bad interval (retCode=10001)",
        ),
        (
            json!({"retCode":0,"retMsg":"OK","result":{}}),
            "Bybit KLINE: no candle list in response (keys=retCode,retMsg,result)",
        ),
        (
            serde_json::Value::Null,
            "Bybit KLINE: null response data (status=200)",
        ),
    ] {
        let h = harness_as(ExchangeName::Bybit, options(), |wire| {
            wire.route("GET", "/v5/market/kline", 200, body)
        });
        let opts =
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","limit":1})).unwrap();
        let error = h.drive(h.venue().fetch_ohlcv(opts)).unwrap_err();
        assert_eq!(error.message, message);
        h.venue().dispose().unwrap();
    }
}

#[test]
fn dedicated_candle_readers_keep_the_source_optional_list_mapping() {
    for name in [ExchangeName::Bybitspot, ExchangeName::Bybitinverse] {
        let h = harness_as(name, options(), |wire| {
            wire.route("GET", "/v5/market/kline", 200, serde_json::Value::Null)
        });
        let opts =
            serde_json::from_value(json!({"symbol":"BTCUSDT","interval":"1m","limit":1})).unwrap();
        assert!(h.drive(h.venue().fetch_ohlcv(opts)).unwrap().is_empty());
        assert_eq!(h.wire.calls("/v5/market/kline").len(), 1);
        h.venue().dispose().unwrap();
    }
}
