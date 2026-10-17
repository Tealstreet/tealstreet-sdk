use safe_cex_exchange::{
    exchange::SubscribedCandle,
    js_runtime::value::{Js, from_typed, to_typed},
};

fn input(interval: Option<Js>) -> Js {
    let value = Js::object(vec![
        ("timestamp", Js::Number(1_700_000_040.)),
        ("open", Js::Number(-0.)),
        ("high", Js::Number(f64::NAN)),
        ("low", Js::Number(99.)),
        ("close", Js::Number(103.)),
        ("volume", Js::Number(7.)),
        ("symbol", Js::str("BTCUSDT")),
    ]);
    if let Some(interval) = interval {
        value.define("interval", interval).unwrap();
    }
    value
}
fn roundtrip(interval: Option<Js>, present: bool, expected: Option<&str>) {
    let decoded: SubscribedCandle =
        to_typed(&input(interval)).expect("source candle interval must cross native codec");
    let native = from_typed(&decoded).unwrap();
    let key: Vec<u16> = "interval".encode_utf16().collect();
    assert_eq!(
        native
            .own_keys()
            .iter()
            .any(|k| k.as_ref() == key.as_slice()),
        present
    );
    assert_eq!(
        native.get("open").unwrap().to_number().unwrap().to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(native.get("high").unwrap().to_number().unwrap().is_nan());
    let json = serde_json::to_value(decoded).unwrap();
    if let Some(expected) = expected {
        assert_eq!(
            native
                .get("interval")
                .unwrap()
                .to_js_string()
                .unwrap()
                .as_ref(),
            expected.encode_utf16().collect::<Vec<_>>().as_slice()
        );
        assert_eq!(json["interval"], expected);
    } else {
        assert!(native.get("interval").unwrap().is_undefined());
        assert!(json.get("interval").is_none());
    }
}
#[test]
fn native_candle_preserves_known_interval() {
    roundtrip(Some(Js::str("1m")), true, Some("1m"));
}
#[test]
fn native_candle_preserves_month_interval() {
    roundtrip(Some(Js::str("1M")), true, Some("1M"));
}
#[test]
fn native_candle_preserves_own_undefined_interval() {
    roundtrip(Some(Js::Undefined), true, None);
}
#[test]
fn native_candle_preserves_absent_interval() {
    roundtrip(None, false, None);
}
