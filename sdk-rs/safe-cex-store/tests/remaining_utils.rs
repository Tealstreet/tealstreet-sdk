use safe_cex_store::{
    read_blob::read_blob,
    remaining_utils::*,
    virtual_clock::{TIME_SYNC_INTERVAL_MS, VIRTUAL_CLOCK, VirtualClock},
};
use serde_json::json;
use std::{
    future::Future,
    io::Write,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("ready fixture"),
    }
}
#[test]
fn native_secondary_runtime_and_capture_flags() {
    assert_eq!(EXCHANGES_WITHOUT_SECONDARY_KEY.len(), 16);
    assert_eq!(
        EXCHANGES_WITHOUT_SECONDARY_KEY,
        [
            "apexomni",
            "breakoutprop",
            "grvt",
            "hyperliquid",
            "hyperliquidspot",
            "lighter",
            "lighter-robinhood",
            "paradex",
            "polymarket",
            "woofi",
            "kodiakfi",
            "mode",
            "salsadex",
            "tealstreet-dex",
            "txflow",
            "what"
        ]
    );
    assert!(!exchange_supports_secondary_key("grvt"));
    assert!(exchange_supports_secondary_key("asterdex"));
    assert!(exchange_supports_secondary_key("GRVT"));
    assert!(!is_development(Some("production")));
    assert!(is_development(None));
    assert!(is_development(Some("Production")));
    assert!(!is_node_or_bun_runtime(None));
    assert!(is_node_or_bun_runtime(Some(
        &json!({"versions":{"node":"22"}})
    )));
    assert!(is_node_or_bun_runtime(Some(&json!({"isBun":true}))));
    assert!(!is_node_or_bun_runtime(Some(
        &json!({"versions":{"node":""},"isBun":false})
    )));
    assert_eq!(SIGNING_CAPTURE_FLAG, "__tealstreetSigningCapture");
    for value in [json!(true), json!(1), json!({}), json!([]), json!("yes")] {
        assert!(is_signing_capture_request(Some(
            &json!({SIGNING_CAPTURE_FLAG:value})
        )));
    }
    for value in [json!(false), json!(0), json!(null), json!("")] {
        assert!(!is_signing_capture_request(Some(
            &json!({SIGNING_CAPTURE_FLAG:value})
        )));
    }
    assert!(!is_signing_capture_request(None));
    assert_eq!(TIME_SYNC_INTERVAL_MS, 300_000);
    let first: &'static VirtualClock = Default::default();
    let second: &'static VirtualClock = Default::default();
    assert!(std::ptr::eq(first, second));
    assert!(std::ptr::eq(first, &*VIRTUAL_CLOCK));
}
#[test]
fn native_merge_identity_first_match_cutoff_nan_and_dedupe() {
    struct Row {
        id: String,
        last: Option<f64>,
        value: i32,
    }
    let row = |id: &str, last: Option<f64>, value| {
        Rc::new(Row {
            id: id.into(),
            last,
            value,
        })
    };
    let a = row("A", Some(1500.0), 1);
    let b = row("B", Some(0.0), 2);
    let c = row("C", Some(f64::NAN), 3);
    let d = row("D", Some(999.0), 4);
    let a2 = row("A", Some(1900.0), 5);
    let updates = vec![
        row("A", Some(1990.0), 11),
        row("B", None, 12),
        row("C", None, 13),
        row("D", None, 14),
        row("A", None, 15),
    ];
    let out = merge_updates_using_last_update(
        &updates,
        &[a.clone(), b, c, d, a2],
        2000.0,
        1000.0,
        |r| r.id.as_str(),
        |r| r.last,
    );
    assert_eq!(out.len(), 4);
    assert!(Rc::ptr_eq(&out[0], &a));
    assert!(Rc::ptr_eq(&out[1], &updates[1]));
    assert!(Rc::ptr_eq(&out[2], &updates[2]));
    assert!(Rc::ptr_eq(&out[3], &updates[3]));
    assert_eq!(
        out.iter().map(|r| r.value).collect::<Vec<_>>(),
        vec![1, 12, 13, 14]
    );
    let at = row("X", Some(1000.0), 20);
    let update = row("X", None, 21);
    assert!(Rc::ptr_eq(
        &merge_updates_using_last_update(
            std::slice::from_ref(&update),
            std::slice::from_ref(&at),
            2000.0,
            1000.0,
            |r| r.id.as_str(),
            |r| r.last
        )[0],
        &at
    ));
    assert!(Rc::ptr_eq(
        &merge_updates_using_last_update(
            &[update],
            std::slice::from_ref(&at),
            f64::NAN,
            1000.0,
            |r| r.id.as_str(),
            |r| r.last
        )[0],
        &at
    ));
}
#[test]
fn native_unknown_send_veto_response_precedence_and_codes() {
    let cases = [
        (json!(null), false),
        (json!("timeout"), false),
        (json!([]), false),
        (json!({"code":"ECONNRESET"}), true),
        (json!({"code":"econnreset"}), false),
        (json!({"message":"TIMEOUT"}), true),
        (json!({"name":"TradeTunnelError"}), true),
        (
            json!({"message":"Remote signer unavailable timeout","response":{"status":503}}),
            false,
        ),
        (json!({"message":"remote signer unavailable timeout"}), true),
        (
            json!({"message":"Signing request abc timed out","code":"ECONNRESET"}),
            false,
        ),
        (
            json!({"message":"Signing request abc\u{feff} timed out"}),
            true,
        ),
        (json!({"response":{},"message":"timeout"}), false),
        (json!({"response":{"status":"503"}}), true),
        (json!({"response":{"data":{"code":"-1007"}}}), true),
        (
            json!({"response":{"status":400,"data":{"code":0}},"code":"ERR_NETWORK"}),
            false,
        ),
        (json!({"response":0,"code":"ERR_NETWORK"}), true),
        (
            json!({"message":"WebSocket disconnected — signing aborted"}),
            false,
        ),
    ];
    assert_eq!(cases.len(), 17);
    for (error, expected) in cases {
        assert_eq!(
            is_unknown_outcome(&error).unwrap(),
            expected,
            "native unknown outcome {error}"
        );
    }
    for (message, expected) in [
        (Some("Signing request abc timed out"), true),
        (Some("Remote signer unavailable"), true),
        (Some("WebSocket disconnected — signing aborted"), true),
        (Some("Signing Aborted"), false),
        (Some("timeout"), false),
        (None, false),
    ] {
        assert_eq!(
            is_unsent_signing_failure(message),
            expected,
            "native unsent signing {message:?}"
        );
    }
    assert!(is_unknown_outcome(&json!({"message":{"toString":"no"}})).is_err());
}
fn zlib(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(vec![], flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(vec![], flate2::Compression::default());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}
#[test]
fn native_blob_char_codes_members_padding_and_failures() {
    let bytes = [0, 65, 127, 128, 255, 13];
    for packed in [zlib(&bytes), gzip(&bytes)] {
        let text = ready(read_blob(async { Ok(packed) })).unwrap();
        assert_eq!(
            text.encode_utf16().collect::<Vec<_>>(),
            vec![0, 65, 127, 128, 255, 13]
        );
        assert_eq!(text.chars().count(), 6);
    }
    for (mut packed, expected) in [(zlib(b"A"), "A"), (gzip(b"A"), "AB")] {
        packed.extend(zlib(b"B"));
        assert_eq!(ready(read_blob(async { Ok(packed) })).unwrap(), expected);
    }
    let mut padded = zlib(b"C");
    padded.extend([0, 0, 4]);
    assert_eq!(ready(read_blob(async { Ok(padded) })).unwrap(), "C");
    for bytes in [vec![], vec![1, 2, 3], vec![0x1f, 0x8b]] {
        assert!(ready(read_blob(async { Ok(bytes) })).is_err());
    }
    assert_eq!(
        ready(read_blob(async { Err("arrayBuffer rejected".into()) })).unwrap_err(),
        "arrayBuffer rejected"
    );
}
