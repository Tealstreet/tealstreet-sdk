#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::{
    builder::*, markets::fetch_markets, order_semantics::symbol_to_instrument_id,
};
use serde_json::Value;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

#[test]
fn actual_returned_units_and_native_admission_are_distinct() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-helpers-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GRVT AUDIT helper source success: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let names = [
        "via",
        "edge-false",
        "trading-false",
        "edge-true",
        "trading-true",
        "field",
        "error",
        "fee-error",
        "builder",
        "cancel-instrument",
        "markets-base",
    ];
    let mut completed = 0;
    assert_eq!(
        source["observations"].as_array().unwrap().len(),
        22,
        "GRVT AUDIT helper observation floor"
    );
    for (index, row) in source["observations"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(
            row["name"],
            names[index % 11],
            "GRVT AUDIT helper ordered identity"
        );
        let unit = if index < 11 { 0xd800 } else { 0xdc00 };
        assert_eq!(row["unit"], unit, "GRVT AUDIT helper exact source unit");
        assert!(
            row["units"]
                .as_array()
                .unwrap()
                .iter()
                .any(|value| value == unit),
            "GRVT AUDIT source returned unit observation"
        );
        assert!(
            serde_json::from_str::<Value>(row["raw"].as_str().unwrap()).is_err(),
            "GRVT AUDIT required string surface or whole response refusal"
        );
        completed += 1;
    }
    assert_eq!(
        completed, 22,
        "GRVT AUDIT native completed helper observations"
    );
    for text in ["scalaré", "😀", "\u{fffd}", "\\ud800"] {
        assert_eq!(
            get_string_field(Some(&Value::String(text.into()))),
            Some(text),
            "GRVT AUDIT native returned scalar"
        );
        assert_eq!(
            create_grvt_builder_info(Some(false), Some(text), Some("0.03"))
                .unwrap()
                .builder,
            text.to_lowercase(),
            "GRVT AUDIT native builder scalar"
        );
        assert_eq!(
            symbol_to_instrument_id(&format!("{text}_PERP")),
            format!("{text}_Perp"),
            "GRVT AUDIT native outgoing instrument scalar"
        );
    }
    let raw = r#"{"r":[{"i":"btc_perp","b":"scalaré","q":"USD","k":"PERPETUAL"},{"i":"eth_perp","b":"ETH","q":"USD"}]}"#;
    let future = fetch_markets(
        |_, _| {
            std::future::ready(Ok(
                safe_cex_types::retained_json::RetainedJsonInput::parse_json(
                    &raw.encode_utf16().collect::<Vec<_>>(),
                )
                .unwrap(),
            ))
        },
        |_, _| Ok(()),
    );
    let mut future = std::pin::pin!(future);
    let result = match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result.unwrap(),
        Poll::Pending => panic!("GRVT AUDIT ready market hook"),
    };
    assert_eq!(result.len(), 2, "GRVT AUDIT native admitted market batch");
    assert_eq!(
        result[0].base, "scalaré",
        "GRVT AUDIT native admitted market base"
    );
    assert_eq!(result[1].base, "ETH", "GRVT AUDIT native unaffected market");
    assert_eq!(
        source["completed"], 22,
        "GRVT AUDIT source completed helper observations"
    );
}
