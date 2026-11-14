//! Public numeric overlays must agree on first decode and repeated-frame cache hits.
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
            support::message(&mut engine, api, 5000., &text);
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

#[test]
fn reordered_context_envelopes_keep_null_blank_and_escaped_numbers() {
    for (raw, expected) in [
        ("null", 65001.0_f64),
        (r#""""#, 0.0),
        (r#""-0""#, -0.0),
        (r#""\u0031.25""#, 1.25),
    ] {
        let (mut engine, _, api) = support::ready();
        let text = format!(
            r#"{{"data":{{"coin":"1","ctx":{{"markPx":{raw}}}}},"channel":"activeAssetCtx"}}"#
        );
        support::message(&mut engine, api, 4000., &text);
        assert_eq!(
            engine.store().ticker("BTC-USDC").unwrap().mark.to_bits(),
            expected.to_bits(),
            "{text}"
        );
    }
}

#[test]
fn repeated_tickers_reapply_their_fields_after_a_context_update() {
    let (mut engine, node, api) = support::ready();
    let ticker = r#"{"channel":"ticker","data":{"instrumentId":1,"lastPrice":"66000","nodeCtx":{"markPx":"66001"}}}"#;
    support::message(&mut engine, api, 4000., ticker);
    support::message(
        &mut engine,
        node,
        5000.,
        r#"{"channel":"activeAssetCtx","data":{"coin":"1","ctx":{"lastPrice":"67000","markPx":"67001"}}}"#,
    );
    assert_eq!(engine.store().ticker("BTC-USDC").unwrap().last, 67000.);
    support::message(&mut engine, api, 6000., ticker);
    let row = engine.store().ticker("BTC-USDC").unwrap();
    assert_eq!((row.last, row.mark), (66000., 66001.));
}
