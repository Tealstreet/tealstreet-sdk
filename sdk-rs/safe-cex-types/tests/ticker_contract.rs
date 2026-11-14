use safe_cex_types::Ticker;
use serde_json::json;

#[test]
fn receipt_times_round_trip_without_becoming_snapshot_time() {
    let value = json!({"id":"BTC", "symbol":"BTC-PERP", "bid":0, "ask":2,
        "last":3, "mark":4, "index":null, "percentage":0, "openInterest":0,
        "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0,
        "observedAt":[0,null,12.5]});
    let ticker: Ticker = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(ticker.bid, 0.0);
    assert_eq!(
        serde_json::to_value(ticker).unwrap()["observedAt"],
        json!([0.0, null, 12.5])
    );
}

#[test]
fn optional_provenance_refuses_null_and_wrong_tuple_shapes() {
    let base = json!({"id":"BTC", "symbol":"BTC-PERP", "bid":0, "ask":0,
        "last":0, "mark":0, "index":0, "percentage":0, "openInterest":0,
        "fundingRate":0, "fundingTime":0, "volume":0, "quoteVolume":0});
    let ticker: Ticker = serde_json::from_value(base.clone()).unwrap();
    assert!(ticker.observed_at.is_none());
    assert!(ticker.mid_price.is_none());
    assert!(
        serde_json::to_value(ticker)
            .unwrap()
            .get("observedAt")
            .is_none()
    );
    for invalid in [
        json!(null),
        json!([1, 2]),
        json!([1, 2, 3, 4]),
        json!(["1", null, 3]),
    ] {
        let mut value = base.clone();
        value["observedAt"] = invalid;
        assert!(serde_json::from_value::<Ticker>(value).is_err());
    }
    let mut value = base.clone();
    value["observedAt"] = json!([null, 0.0, 3.0]);
    value["midPrice"] = json!(0.0);
    let ticker: Ticker = serde_json::from_value(value).unwrap();
    assert_eq!(ticker.observed_at, Some([None, Some(0.0), Some(3.0)]));
    assert_eq!(ticker.mid_price, Some(0.0));
    let mut value = base;
    value["midPrice"] = json!(null);
    assert!(serde_json::from_value::<Ticker>(value).is_err());
}

#[test]
fn existing_nullable_index_and_retained_info_stay_unchanged() {
    let raw = r#"{"id":"BTC","symbol":"BTC-PERP","bid":0,"ask":0,"last":0,
        "mark":0,"index":null,"percentage":0,"openInterest":0,"fundingRate":0,
        "fundingTime":0,"volume":0,"quoteVolume":0,"timestamp":-0,
        "info":{"quote":null},"observedAt":[null,0,1.5]}"#;
    let ticker: Ticker = serde_json::from_str(raw).unwrap();
    assert!(ticker.index.is_none());
    assert_eq!(ticker.timestamp.unwrap().to_bits(), (-0.0_f64).to_bits());
    let wire = serde_json::to_value(&ticker).unwrap();
    assert_eq!(wire["info"], json!({"quote":null}));
    assert!(serde_json::from_value::<Ticker>(json!([])).is_err());
    let mut missing_index = wire;
    missing_index.as_object_mut().unwrap().remove("index");
    assert!(serde_json::from_value::<Ticker>(missing_index).is_err());
}
