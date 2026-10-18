use safe_cex_venue_coinbase::rest_reads::format_spot_cfm_tickers;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

#[path = "support/capture_metadata.rs"]
mod capture_metadata;

fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-cfm-tickers.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}

fn tagged_number(value: f64) -> Value {
    if value.is_nan() {
        json!({ "$number": "NaN" })
    } else if value == f64::INFINITY {
        json!({ "$number": "Infinity" })
    } else if value == f64::NEG_INFINITY {
        json!({ "$number": "-Infinity" })
    } else {
        json!({ "$f64bits": format!("{:016x}", value.to_bits()) })
    }
}

#[test]
fn spot_cfm_tickers_preserve_field_identity_and_cardinality() {
    capture_metadata::assert_capture_metadata(oracle(), 1);
    let cases = oracle()["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25, "spot CFM source corpus total");
    let expected_counts = [
        1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 0, 1, 1, 2,
    ];
    assert_eq!(expected_counts.len(), cases.len());
    let mut comparisons = 0;

    for (case, expected_count) in cases.iter().zip(expected_counts) {
        let input = &case["input"];
        let expected = case["result"].as_array().unwrap();
        assert_eq!(case["expectedCount"].as_u64(), Some(expected_count));
        assert_eq!(
            input["products"].as_array().unwrap().len(),
            expected_count as usize
        );
        assert_eq!(
            expected.len(),
            expected_count as usize,
            "source result count {case}"
        );

        let actual = format_spot_cfm_tickers(&input.clone().into()).unwrap();
        assert_eq!(
            actual.len(),
            expected_count as usize,
            "spot CFM native cardinality {}",
            case["label"]
        );
        for index in 0..expected_count as usize {
            let ticker = &actual[index];
            assert_eq!(ticker.id, expected[index]["id"].as_str().unwrap());
            assert_eq!(ticker.symbol, expected[index]["symbol"].as_str().unwrap());
            assert_eq!(ticker.timestamp, None, "Coinbase spot CFM omits timestamp");
            assert!(
                expected[index].get("timestamp").is_none(),
                "live source omits timestamp"
            );
            for (field, value) in [
                ("bid", ticker.bid),
                ("ask", ticker.ask),
                ("last", ticker.last),
                ("mark", ticker.mark),
                ("index", ticker.index),
                ("percentage", ticker.percentage),
                ("fundingRate", ticker.funding_rate),
                ("fundingTime", ticker.funding_time),
                ("volume", ticker.volume),
                ("quoteVolume", ticker.quote_volume),
                ("openInterest", ticker.open_interest),
            ] {
                assert_eq!(
                    tagged_number(value),
                    expected[index][field],
                    "spot CFM decoded {field} identity {}[{index}]",
                    case["label"]
                );
            }
            comparisons += 1;
        }
    }

    assert_eq!(comparisons, 26, "spot CFM native comparison count");
}
