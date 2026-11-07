//! Exact public numeric overlay semantics retained by the WASM throughput profile.
mod support;

#[test]
fn public_numeric_overlays_preserve_presence_and_float_bits() {
    for (raw, value) in [
        ("null", None),
        (r#""""#, Some(0.0_f64)),
        (r#"" ""#, None),
        ("0", Some(0.0)),
        ("-0.0", Some(-0.0)),
        (r#""-0""#, Some(-0.0)),
        ("1.25", Some(1.25)),
        (r#""1.25""#, Some(1.25)),
        (r#""\u0031.25""#, Some(1.25)),
        (r#"" 1.25 ""#, Some(1.25)),
        (r#""1e-12""#, Some(1e-12)),
        (r#""NaN""#, None),
        (r#""inf""#, None),
        (r#""malformed""#, None),
        ("true", None),
        ("[]", None),
        ("{}", None),
    ] {
        for kind in ["activeAssetCtx", "allAssetCtxs", "ticker"] {
            let (mut engine, _, api) = support::ready();
            let payload = match kind {
                "activeAssetCtx" => format!(r#"{{"coin":"1","ctx":{{"markPx":{raw}}}}}"#),
                "allAssetCtxs" => {
                    format!(r#"{{"ctxs":[{{"coin":"1","ctx":{{"markPx":{raw}}}}}]}}"#)
                }
                _ => format!(r#"{{"instrumentId":1,"lastPrice":{raw}}}"#),
            };
            let text = format!(r#"{{"channel":"{kind}","data":{payload}}}"#);
            support::message(&mut engine, api, 4000., &text);
            let ticker = engine.store().ticker("BTC-USDC").unwrap();
            let (actual, original) = if kind == "ticker" {
                (ticker.last, 65000.0)
            } else {
                (ticker.mark, 65001.0)
            };
            assert_eq!(
                actual.to_bits(),
                value.unwrap_or(original).to_bits(),
                "{text}"
            );
        }
    }
}

#[test]
fn a_malformed_aggregate_suffix_keeps_the_whole_frame_atomic() {
    let (mut engine, _, api) = support::ready();
    support::message(
        &mut engine,
        api,
        4000.,
        r#"{"channel":"allAssetCtxs","data":{"ctxs":[{"coin":"1","ctx":{"markPx":"66000"}},{"coin":"1","ctx":{"markPx":"invalid"}}]}}"#,
    );
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().mark, 65001.);
}
