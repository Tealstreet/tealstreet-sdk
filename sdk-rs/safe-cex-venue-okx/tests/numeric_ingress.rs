//! Existing map-only ingress audit; no Value-first repair is introduced here.
use safe_cex_venue_okx::exchange::{TickerMarkInput, find_ticker_mark_for_currency};
use serde_json::{Value, json};

#[test]
fn ticker_mark_ingress_retains_integer_negative_zero() {
    let raw = r#"{"id":"BTC-USDT-SWAP","mark":-0}"#;
    let parsed: Value = serde_json::from_str(raw).unwrap();
    assert_eq!(
        parsed["mark"].as_f64().unwrap().to_bits(),
        (-0.0_f64).to_bits(),
        "input lexeme still carries the source sign"
    );
    let raw_ticker: TickerMarkInput = serde_json::from_str(raw).unwrap();
    assert_eq!(raw_ticker.mark.unwrap().to_bits(), (-0.0_f64).to_bits());
    let profiles = [
        (
            "owned Value integer -0",
            serde_json::from_value::<TickerMarkInput>(parsed.clone()).unwrap(),
        ),
        (
            "borrowed Value integer -0",
            serde::Deserialize::deserialize(&parsed).unwrap(),
        ),
    ];
    for (name, ticker) in profiles {
        // Shared whole-entry capture now retains the source sign.
        assert_eq!(
            ticker.mark.unwrap().to_bits(),
            (-0.0_f64).to_bits(),
            "ingress must retain -0 for {name}"
        );
        assert_eq!(
            find_ticker_mark_for_currency(&[ticker], "BTC").to_bits(),
            (-0.0_f64).to_bits(),
            "returned mark retains the source sign for {name}"
        );
    }
    for (name, value) in [
        (
            "decimal -0.0",
            serde_json::from_str(r#"{"id":"BTC-USDT-SWAP","mark":-0.0}"#).unwrap(),
        ),
        ("native -0.0", json!({"id":"BTC-USDT-SWAP","mark":-0.0_f64})),
        (
            "overflow",
            serde_json::from_str(r#"{"id":"BTC-USDT-SWAP","mark":1e400}"#).unwrap(),
        ),
        ("decimal", json!({"id":"BTC-USDT-SWAP","mark":1.25})),
    ] {
        let expected = value["mark"].as_f64().unwrap_or(f64::INFINITY).to_bits();
        let ticker: TickerMarkInput = serde_json::from_value(value).unwrap();
        assert_eq!(
            ticker.mark.unwrap().to_bits(),
            expected,
            "positive bit control {name}"
        );
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct Nested {
    ticker: TickerMarkInput,
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Nested {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { ticker });
    }
}
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(tag = "kind")]
enum Tagged {
    #[serde(rename = "ticker")]
    Ticker { ticker: TickerMarkInput },
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for Tagged {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Ticker { ticker } });
    }
}

#[test]
fn ticker_mark_shared_ingress_preserves_nested_raw_and_tagged_paths() {
    use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
    for lexeme in ["-0", "-0.0", "-1e-4000", "1e400", "-1e400", "1.25"] {
        let expected = lexeme.parse::<f64>().unwrap().to_bits();
        let raw = format!(r#"{{"ticker":{{"mark":{lexeme}}}}}"#);
        let value: Value = serde_json::from_str(&raw).unwrap();
        for model in [
            js_from_str::<Nested>(&raw).unwrap(),
            js_from_value::<Nested>(value.clone()).unwrap(),
            js_from_value_ref::<Nested>(&value).unwrap(),
        ] {
            assert_eq!(
                model.ticker.mark.unwrap().to_bits(),
                expected,
                "nested {lexeme}"
            );
        }
        let raw = format!(r#"{{"kind":"ticker","ticker":{{"mark":{lexeme}}}}}"#);
        let value: Value = serde_json::from_str(&raw).unwrap();
        for model in [
            js_from_str::<Tagged>(&raw).unwrap(),
            js_from_value::<Tagged>(value.clone()).unwrap(),
            js_from_value_ref::<Tagged>(&value).unwrap(),
        ] {
            let Tagged::Ticker { ticker } = model;
            assert_eq!(ticker.mark.unwrap().to_bits(), expected, "tagged {lexeme}");
        }
    }
}

#[test]
fn ticker_mark_representation_and_depth_boundaries_are_explicit() {
    typed_utf16::assert_projection(
        r#"{"id":"\ud800USD","mark":7}"#,
        &(serde_json::from_str::<TickerMarkInput>(r#"{"id":"\ud800USD","mark":7}"#))
            .expect("typed surrogate record is retained"),
    );
    for currency in ["BTC", "Ω", "🚀", "\u{feff}"] {
        let raw = json!({"id":format!("{currency}USD"),"mark":7}).to_string();
        let ticker: TickerMarkInput = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            find_ticker_mark_for_currency(&[ticker], currency).to_bits(),
            7_f64.to_bits()
        );
    }
    let raw = format!(
        r#"{{"id":"BTCUSD","mark":7,"extra":{}0{}}}"#,
        "[".repeat(140),
        "]".repeat(140)
    );
    assert!(
        serde_json::from_str::<TickerMarkInput>(&raw).is_err(),
        "shared input depth bound stays explicit even for ignored fields"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
