#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_venue_okx::exchange::format_market_pretty_symbol;
use serde_json::Value;

fn slot(case: &Value, key: &str) -> Option<Value> {
    case.get(key)
        .map(|raw| serde_json::from_str(raw.as_str().unwrap()).unwrap())
}

#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn maintained_pretty_symbol_branch_presence_and_argument_identity() {
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/pretty-symbol-oracle.mjs"
    );
    let oracle = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let envelope: Value = serde_json::from_slice(&oracle.stdout).unwrap();
    assert!(envelope["capture"]["architecture"].as_str().is_some());
    assert!(envelope["capture"]["nodeMajor"].as_u64().is_some());
    let cases = envelope["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 655);
    cardinality::assert_groups(
        cases,
        "pretty_symbol profiles",
        |case| {
            if case.get("error").is_some_and(|error| !error.is_null()) {
                "error"
            } else if case["defined"] == true {
                "hot"
            } else {
                "cold"
            }
            .to_owned()
        },
        &[("cold", 504), ("hot", 126), ("error", 25)],
    );
    for (index, case) in cases.iter().enumerate() {
        let market = slot(case, "marketRaw");
        let base = slot(case, "baseRaw");
        let quote = slot(case, "quoteRaw");
        let result = format_market_pretty_symbol(market.as_ref(), base.as_ref(), quote.as_ref());
        if case.get("error").is_none() && case["defined"] == false {
            assert_eq!(
                result,
                Ok(None),
                "pretty-symbol cold branch must not coerce {index}"
            );
        }
        match result {
            Ok(value) => {
                assert!(
                    case.get("error").is_none(),
                    "pretty-symbol expected coercion error {index}"
                );
                assert_eq!(
                    value.is_some(),
                    case["defined"].as_bool().unwrap(),
                    "pretty-symbol native optional presence {index}"
                );
                if let Some(value) = value {
                    assert_eq!(
                        value,
                        case["value"].as_str().unwrap(),
                        "pretty-symbol native base quote identity {index}"
                    );
                }
            }
            Err(error) => {
                assert_eq!(
                    error.name, case["error"]["name"],
                    "pretty-symbol error name identity {index}"
                );
                assert_eq!(
                    error.message, case["error"]["message"],
                    "pretty-symbol error message identity {index}"
                );
            }
        }
    }
    let control = std::process::Command::new("node")
        .arg(script)
        .arg("--mutate-base-quote")
        .output()
        .unwrap();
    assert_eq!(control.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&control.stderr).contains("pretty-symbol base quote identity"));
    let source = std::process::Command::new("node")
        .arg(script)
        .arg("--surrogate-witness")
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let witness: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        witness["wire"], "\"BASE\\ud800/QUOTE\\udfff XPERP\"",
        "source pretty-symbol escaped surrogate wire"
    );
    for key in ["baseRaw", "quoteRaw"] {
        assert!(
            serde_json::from_str::<Value>(witness[key].as_str().unwrap()).is_err(),
            "Rust UTF-8 representation refuses valid JSON lone-surrogate escape"
        );
    }
}
