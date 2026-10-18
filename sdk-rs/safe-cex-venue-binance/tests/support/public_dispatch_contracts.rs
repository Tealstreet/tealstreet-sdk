use super::*;
use safe_cex_types::Ticker;
use safe_cex_venue_binance::ticker_capture::Capture;

fn ticker_bits(row: &Ticker) -> Value {
    json!({
        "bid":format!("{:016x}",row.bid.to_bits()),
        "ask":format!("{:016x}",row.ask.to_bits()),
        "last":format!("{:016x}",row.last.to_bits()),
        "mark":format!("{:016x}",row.mark.to_bits()),
        "index":format!("{:016x}",row.index.to_bits()),
        "percentage":format!("{:016x}",row.percentage.to_bits()),
        "volume":format!("{:016x}",row.volume.to_bits()),
        "quoteVolume":format!("{:016x}",row.quote_volume.to_bits()),
        "fundingRate":format!("{:016x}",row.funding_rate.to_bits()),
        "fundingTime":format!("{:016x}",row.funding_time.to_bits()),
        "openInterest":format!("{:016x}",row.open_interest.to_bits()),
    })
}

#[test]
fn production_native_dispatch_lookup_and_capture_follow_source_prototypes() {
    let mut cases = Vec::new();
    for (venue, name) in [
        (Venue::Futures, "binance"),
        (Venue::PM, "binancepm"),
        (Venue::Spot, "binancespot"),
    ] {
        for key in ["constructor", "toString", "__proto__", "not-owned"] {
            let owner = production_native_owner(venue);
            let capture = Capture::new(owner.base.store.clone());
            let dictionary = capture.dictionary();
            let row = dictionary.borrow().get("BTCUSDT").unwrap().clone();
            let before = ticker_bits(&row.borrow());
            let frame = json!({"e":"bookTicker","s":key,"b":{"toString":null},"a":"123.25"});
            let failure = owner.public.on_message(&frame.to_string()).err();
            assert_eq!(
                failure.is_some(),
                key != "not-owned",
                "actual native lookup must preserve inherited target coercion {venue:?} {key}"
            );
            let error = failure.map(|error| {
                assert_eq!(error.name, "TypeError");
                json!({"name":error.name,"message":error.message})
            });
            let after_bad = ticker_bits(&owner.base.store.tickers()[0]);
            assert_eq!(after_bad, before, "failed lookup frame cannot patch Store");
            owner
                .public
                .on_message(r#"{"e":"bookTicker","s":"BTCUSDT","b":"-0","a":"123.25"}"#)
                .unwrap();
            assert_eq!(row.borrow().bid.to_bits(), (-0.0_f64).to_bits());
            assert_eq!(row.borrow().ask.to_bits(), 123.25_f64.to_bits());
            cases.push(json!({
                "venue":name,"key":key,"error":error,"before":before,"afterBad":after_bad,
                "afterValid":ticker_bits(&owner.base.store.tickers()[0]),
                "capturedBid":format!("{:016x}",row.borrow().bid.to_bits()),
                "capturedAsk":format!("{:016x}",row.borrow().ask.to_bits()),
            }));
            owner.base.dispose().unwrap();
        }
    }
    assert_eq!(cases.len(), 12);
    let output = std::process::Command::new("node")
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .args([
            "--import",
            "tsx",
            "sdk-rs/safe-cex-venue-binance/scripts/public-dispatch-source.cjs",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source prototype/capture witness: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 12);
    assert_eq!(
        cases, source,
        "constructed native/source prototype and capture outputs"
    );
}
