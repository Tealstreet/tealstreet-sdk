use safe_cex_venue_okx::exchange::{
    TickerMarkInput, find_ticker_mark_for_currency, format_market_pretty_symbol,
};
use serde_json::{Value, json};
use std::process::Command;

fn source_observations(script: &str, profile: &str, assertion: &str) -> [Value; 2] {
    let run = |control| {
        let mut command = Command::new("node");
        command
            .arg(format!(
                "{}/../parity/evidence/okx-kraken-weex-audit/{script}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .arg(profile);
        if control {
            command.arg("--control");
        }
        command
            .output()
            .expect("Node must execute the actual source")
    };
    let baseline = run(false);
    assert!(
        baseline.status.success(),
        "OKX source baseline {profile}: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let mutant = run(true);
    assert_eq!(mutant.status.code(), Some(1), "OKX source control exit");
    assert!(
        String::from_utf8_lossy(&mutant.stderr).contains(assertion),
        "OKX source control named assertion {profile}: {}",
        String::from_utf8_lossy(&mutant.stderr)
    );
    let restored = run(false);
    assert!(
        restored.status.success(),
        "OKX source restored {profile}: {}",
        String::from_utf8_lossy(&restored.stderr)
    );
    [baseline, restored].map(|output| {
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        let observations = envelope["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 1, "OKX selected source profile count");
        observations[0].clone()
    })
}

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn currency_key_observability_and_typed_replacement() {
    let cases = [
        (r#"{"id":"A-USDT-SWAP","mark":17}"#, "A", "A-USDT-SWAP"),
        (
            r#"{"id":"\ud83d\ude00-USDT-SWAP","mark":17}"#,
            "😀",
            "😀-USDT-SWAP",
        ),
        (
            r#"{"id":"\ufeff-USDT-SWAP","mark":17}"#,
            "\u{feff}",
            "\u{feff}-USDT-SWAP",
        ),
        (r#"{"id":"\ufffd-USDT-SWAP","mark":17}"#, "�", "�-USDT-SWAP"),
        (
            r#"{"id":"\\ud800-USDT-SWAP","mark":17}"#,
            "\\ud800",
            "\\ud800-USDT-SWAP",
        ),
    ];
    let mut compared = 0;
    for (raw, currency, id) in cases {
        let ticker: TickerMarkInput = serde_json::from_str(raw).unwrap();
        assert_eq!(ticker.id.as_deref(), Some(id), "OKX decoded currency key");
        assert_eq!(ticker.mark, Some(17.0), "OKX decoded native mark");
        let tickers = [ticker];
        assert_eq!(
            find_ticker_mark_for_currency(&tickers, currency),
            17.0,
            "OKX native exact currency comparison"
        );
        assert_eq!(
            find_ticker_mark_for_currency(&tickers, "unmatched"),
            1.0,
            "OKX native unmatched currency fallback"
        );
        compared += 1;
    }
    assert_eq!(compared, 5, "OKX independent native currency case floor");
    let raw = r#"{"id":"\ud800-USDT-SWAP","mark":17}"#;
    let ticker: TickerMarkInput = serde_json::from_str(raw).unwrap();
    assert_eq!(ticker.id.as_deref(), Some("\u{fffd}-USDT-SWAP"));
    assert_eq!(ticker.mark, Some(17.0));
    typed_utf16::assert_projection(raw, &ticker);
    let raw = r#"[{"id":"A-USDT-SWAP","mark":3},{"id":"\ud800-USDT-SWAP","mark":17}]"#;
    let batch: Vec<TickerMarkInput> = serde_json::from_str(raw).unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].id.as_deref(), Some("A-USDT-SWAP"));
    assert_eq!(batch[0].mark, Some(3.0));
    assert_eq!(batch[1].id.as_deref(), Some("\u{fffd}-USDT-SWAP"));
    assert_eq!(batch[1].mark, Some(17.0));
    typed_utf16::assert_projection(raw, &batch);
    for observation in source_observations(
        "full-source-observations.mjs",
        "okx-currency-key",
        "FULL-OKX-CURRENCY-KEY comparison result",
    ) {
        assert_eq!(observation["id"], "okx-currency-key");
        assert_eq!(
            observation["units"],
            json!([0xd800, 45, 85, 83, 68, 84, 45, 83, 87, 65, 80]),
            "OKX source materialized compared identifier"
        );
        assert_eq!(observation["result"], 17, "OKX source retained-unit match");
    }
}

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn pretty_symbol_observability_and_argument_refusal() {
    let market = json!({"instType":"FUTURES","ruleType":"xperp"});
    let quote = json!("Q");
    let cases = [
        ("A", "A/Q XPERP"),
        ("😀", "😀/Q XPERP"),
        ("\u{feff}", "\u{feff}/Q XPERP"),
        ("�", "�/Q XPERP"),
        ("\\ud800", "\\ud800/Q XPERP"),
    ];
    let mut compared = 0;
    for (base, expected) in cases {
        let base = Value::String(base.into());
        assert_eq!(
            format_market_pretty_symbol(Some(&market), Some(&base), Some(&quote)),
            Ok(Some(expected.into())),
            "OKX native pretty-symbol base quote identity"
        );
        compared += 1;
    }
    assert_eq!(
        compared, 5,
        "OKX independent native pretty-symbol case floor"
    );
    assert!(
        serde_json::from_str::<Value>(r#""\ud800""#).is_err(),
        "OKX native pretty-symbol argument refusal"
    );
    for observation in source_observations(
        "source-observations.mjs",
        "okx-pretty-unit",
        "SOURCE-OKX-PRETTY-UNIT returned String",
    ) {
        assert_eq!(observation["id"], "okx-pretty-unit");
        assert_eq!(
            observation["units"],
            json!([0xd800, 47, 81, 32, 88, 80, 69, 82, 80]),
            "OKX source inspectable pretty-symbol units"
        );
    }
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
