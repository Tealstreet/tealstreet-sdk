use safe_cex_venue_binance::response_shape::*;
use serde_json::{Value, json};
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn response_shapes_and_array_errors_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/response-shape.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 1188, "SOURCE-CARDINALITY response_shape total");
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        let (describe, require) = if c["variant"] == "binance" {
            (
                describe_binance_response_shape as fn(Option<&Value>) -> String,
                require_binance_array_response
                    as for<'a> fn(
                        &str,
                        Option<&'a Value>,
                    ) -> Result<&'a [Value], BinanceResponseError>,
            )
        } else {
            (
                describe_binance_pm_response_shape as _,
                require_binance_pm_array_response as _,
            )
        };
        let owned = if let Some(entries) = c.get("entries") {
            Some(Value::Object(
                entries
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|entry| (entry[0].as_str().unwrap().to_owned(), entry[1].clone()))
                    .collect(),
            ))
        } else {
            c.get("input").cloned()
        };
        let input = owned.as_ref();
        assert_eq!(describe(input), c["shape"], "shape case {index}: {c}");
        let actual = match require(c["endpoint"].as_str().unwrap(), input) {
            Ok(rows) => {
                assert!(std::ptr::eq(
                    rows,
                    input.unwrap().as_array().unwrap().as_slice()
                ));
                json!({"value":rows})
            }
            Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
        };
        assert_eq!(actual, c["expected"], "array case {index}: {c}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 1188,
        "NATIVE-CARDINALITY response_shape completed comparisons"
    );
    eprintln!("{} live response-shape comparisons", cases.len());
}
