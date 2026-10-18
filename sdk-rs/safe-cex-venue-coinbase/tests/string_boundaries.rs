use safe_cex_num::numeric_ingress::{JsNumberOutput, js_from_str};
use safe_cex_venue_coinbase::types;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::process::Command;
#[path = "support/capture_metadata.rs"]
mod capture_metadata;

// Standalone Value still rejects in its JSON string parser. Typed numeric roots
// first retain UTF-16, then reject at their explicit Rust String projection.
fn value_lone_unit_diagnostic(id: &str) -> &'static str {
    match id {
        "high" => "unexpected end of hex escape",
        "low" => "lone leading surrogate in hex escape",
        other => panic!("no lone-unit diagnostic for {other}"),
    }
}

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/string-boundaries.cjs"
        ))
        .output()
        .expect("live TypeScript string boundary oracle requires Node");
    assert!(
        output.status.success(),
        "source string boundary oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value =
        serde_json::from_slice(&output.stdout).expect("source native unit captures");
    capture_metadata::assert_capture_metadata(&capture, 1);
    capture
}
fn probe<T: DeserializeOwned + Serialize + JsNumberOutput>(
    case: &Value,
    read: impl Fn(&T) -> &str,
) -> (usize, usize) {
    let raw = case["raw"].as_str().unwrap();
    let id = case["id"].as_str().unwrap();
    let (expected_units, refuses): (&[u16], bool) = match id {
        "high" => (&[0xd800], true),
        "low" => (&[0xdc00], true),
        "ascii" => (&[97, 108, 112, 104, 97], false),
        "scalar" => (&[0x3bb], false),
        "astral" => (&[0xd83d, 0xde80], false),
        "whitespace" => (&[32, 9, 10], false),
        "replacement" => (&[0xfffd], false),
        "literal-ascii" => (&[92, 117, 100, 56, 48, 48], false),
        unexpected => panic!("unexpected native string witness {unexpected}"),
    };
    assert_eq!(
        case["refuses"].as_bool().unwrap(),
        refuses,
        "independent refusal content pin"
    );
    let units: Vec<u16> = case["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|unit| unit.as_u64().unwrap().try_into().unwrap())
        .collect();
    assert_eq!(
        units, expected_units,
        "independent exact UTF16 unit content pin"
    );
    let expected_path = match case["root"].as_str().unwrap() {
        "MARKET_RESPONSE" => serde_json::json!(["products", 0, "product_id"]),
        "INTX_INSTRUMENTS_RESPONSE" => serde_json::json!([0, "instrument_id"]),
        "PORTFOLIO_BREAKDOWN_RESPONSE" => serde_json::json!(["breakdown", "portfolio", "name"]),
        unexpected => panic!("unexpected string witness root {unexpected}"),
    };
    assert_eq!(
        case["path"], expected_path,
        "independent declared string path content pin"
    );
    let routes = [
        ("raw str", serde_json::from_str::<T>(raw)),
        ("raw slice", serde_json::from_slice::<T>(raw.as_bytes())),
        ("shared raw str", js_from_str::<T>(raw)),
    ];
    assert_eq!(
        routes.each_ref().map(|(route, _)| *route),
        ["raw str", "raw slice", "shared raw str"],
        "independent native route content pin"
    );
    let (mut refused_count, mut positive_count) = (0, 0);
    for (route, decoded) in routes {
        if refuses {
            assert!(
                decoded.is_err(),
                "whole record refusal {route} {} {}",
                case["root"],
                case["id"]
            );
            let error = decoded.err().expect("whole record refused").to_string();
            assert!(
                error.starts_with("invalid utf-16: lone surrogate found"),
                "whole record surrogate refusal attribution {route} {} {}: {error}",
                case["root"],
                case["id"]
            );
            refused_count += 1;
        } else {
            let record = decoded.unwrap_or_else(|error| {
                panic!(
                    "positive native string decode {route} {} {}: {error}",
                    case["root"], case["id"]
                )
            });
            assert_eq!(
                read(&record).encode_utf16().collect::<Vec<_>>(),
                units,
                "native returned string units {route} {} {}",
                case["root"],
                case["id"]
            );
            positive_count += 1;
        }
    }
    if refuses {
        let error = serde_json::from_str::<Value>(raw)
            .expect_err("Value boundary refuses before owned or borrowed typed route")
            .to_string();
        assert!(
            error.starts_with(value_lone_unit_diagnostic(id)),
            "upstream Value surrogate refusal attribution {} {}: {error}",
            case["root"],
            case["id"]
        );
    }
    (refused_count, positive_count)
}
#[test]
fn three_declared_string_fields_refuse_whole_records_for_each_lone_unit_and_keep_scalar_positives()
{
    let capture = source();
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24, "independent string witness floor");
    let mut identities = Vec::new();
    let (mut refused, mut positive) = (0, 0);
    for case in cases {
        identities.push((case["root"].as_str().unwrap(), case["id"].as_str().unwrap()));
        let counts = match case["root"].as_str().unwrap() {
            "MARKET_RESPONSE" => {
                probe::<types::MarketResponse>(case, |record| &record.products[0].product_id)
            }
            "INTX_INSTRUMENTS_RESPONSE" => {
                probe::<types::IntxInstrumentsResponse>(case, |record| &record[0].instrument_id)
            }
            "PORTFOLIO_BREAKDOWN_RESPONSE" => {
                probe::<types::PortfolioBreakdownResponse>(case, |record| {
                    &record.breakdown.portfolio.name
                })
            }
            unexpected => panic!("unexpected string witness root {unexpected}"),
        };
        refused += counts.0;
        positive += counts.1;
    }
    let expected: Vec<_> = [
        "MARKET_RESPONSE",
        "INTX_INSTRUMENTS_RESPONSE",
        "PORTFOLIO_BREAKDOWN_RESPONSE",
    ]
    .into_iter()
    .flat_map(|root| {
        [
            "high",
            "low",
            "ascii",
            "scalar",
            "astral",
            "whitespace",
            "replacement",
            "literal-ascii",
        ]
        .into_iter()
        .map(move |id| (root, id))
    })
    .collect();
    assert_eq!(
        identities, expected,
        "independent string witness content pin"
    );
    assert_eq!(
        refused, 18,
        "six whole records refused through three raw routes"
    );
    assert_eq!(
        positive, 54,
        "eighteen positive records retain exact native units through three raw routes"
    );
}

fn depth_probe<T: DeserializeOwned + Serialize + JsNumberOutput>(
    case: &Value,
    read: impl Fn(&T) -> &str,
) -> usize {
    let raw = case["raw"].as_str().unwrap();
    let depth = case["depth"].as_u64().unwrap();
    let routes = [
        ("raw str", serde_json::from_str::<T>(raw)),
        ("raw slice", serde_json::from_slice::<T>(raw.as_bytes())),
        ("shared raw str", js_from_str::<T>(raw)),
    ];
    assert_eq!(
        routes.each_ref().map(|(route, _)| *route),
        ["raw str", "raw slice", "shared raw str"],
        "independent native depth route content pin"
    );
    let mut comparisons = 0;
    for (route, decoded) in routes {
        match depth {
            3 => {
                let record = decoded.unwrap_or_else(|error| {
                    panic!(
                        "shallow ignored field native decode {route} {}: {error}",
                        case["root"]
                    )
                });
                assert_eq!(
                    read(&record),
                    case["field"].as_str().unwrap(),
                    "native declared field with shallow ignored field {route} {}",
                    case["root"]
                );
            }
            140 => {
                assert!(
                    decoded.is_err(),
                    "whole record refusal for deep ignored field {route} {}",
                    case["root"]
                );
                let error = decoded
                    .err()
                    .expect("deep ignored field refused")
                    .to_string();
                assert!(
                    error.contains("recursion limit"),
                    "whole record depth refusal attribution {route} {}: {error}",
                    case["root"]
                );
            }
            unexpected => panic!("unexpected depth witness {unexpected}"),
        }
        comparisons += 1;
    }
    comparisons
}
#[test]
fn retained_numeric_roots_measure_deep_ignored_field_refusal_separately_from_strings() {
    let capture = source();
    let depths = capture["depths"].as_array().unwrap();
    assert_eq!(depths.len(), 6, "independent depth witness floor");
    let mut identities = Vec::new();
    let mut comparisons = 0;
    for case in depths {
        identities.push((
            case["root"].as_str().unwrap(),
            case["depth"].as_u64().unwrap(),
        ));
        comparisons += match case["root"].as_str().unwrap() {
            "MARKET_RESPONSE" => {
                depth_probe::<types::MarketResponse>(case, |record| &record.products[0].product_id)
            }
            "INTX_INSTRUMENTS_RESPONSE" => {
                depth_probe::<types::IntxInstrumentsResponse>(case, |record| {
                    &record[0].instrument_id
                })
            }
            "PORTFOLIO_BREAKDOWN_RESPONSE" => {
                depth_probe::<types::PortfolioBreakdownResponse>(case, |record| {
                    &record.breakdown.portfolio.name
                })
            }
            unexpected => panic!("unexpected depth witness root {unexpected}"),
        };
    }
    let expected: Vec<_> = [
        "MARKET_RESPONSE",
        "INTX_INSTRUMENTS_RESPONSE",
        "PORTFOLIO_BREAKDOWN_RESPONSE",
    ]
    .into_iter()
    .flat_map(|root| [3, 140].into_iter().map(move |depth| (root, depth)))
    .collect();
    assert_eq!(
        identities, expected,
        "independent ignored-depth content pin"
    );
    assert_eq!(
        comparisons, 18,
        "six depth witnesses through three raw routes"
    );
}

fn ignored_string_probe<T: DeserializeOwned + Serialize + JsNumberOutput>(
    case: &Value,
    read: impl Fn(&T) -> &str,
) -> (usize, usize) {
    let raw = case["raw"].as_str().unwrap();
    let expected_units: &[u16] = match case["id"].as_str().unwrap() {
        "high" => &[0xd800],
        "low" => &[0xdc00],
        "ascii" => &[97, 108, 112, 104, 97],
        other => panic!("unreviewed ignored string {other}"),
    };
    assert_eq!(
        case["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap().try_into().unwrap())
            .collect::<Vec<u16>>(),
        expected_units,
        "independent ignored string units"
    );
    assert_eq!(case["field"], "kept-field");
    let routes = [
        ("raw str", serde_json::from_str::<T>(raw)),
        ("raw slice", serde_json::from_slice::<T>(raw.as_bytes())),
        ("shared raw str", js_from_str::<T>(raw)),
    ];
    assert_eq!(
        routes.each_ref().map(|(route, _)| *route),
        ["raw str", "raw slice", "shared raw str"],
        "independent ignored string route floor"
    );
    let (mut refused, mut positive) = (0, 0);
    for (route, result) in routes {
        if case["id"] == "ascii" {
            let record = result.unwrap_or_else(|error| {
                panic!("positive ignored string native decode {route}: {error}")
            });
            assert_eq!(
                read(&record),
                "kept-field",
                "native declared field survives ignored ASCII string"
            );
            positive += 1;
        } else {
            let error = result.err().unwrap_or_else(|| {
                panic!("expected retained-capture refusal for ignored lone unit {route}")
            });
            let message = error.to_string();
            assert!(
                message.starts_with("invalid utf-16: lone surrogate found"),
                "ignored string refusal attribution {route}: {message}"
            );
            refused += 1;
        }
    }
    (refused, positive)
}
#[test]
fn ignored_string_capture_policy_is_measured_separately_from_materialized_string_fields() {
    let capture = source();
    let cases = capture["ignoredStrings"].as_array().unwrap();
    assert_eq!(
        cases
            .iter()
            .map(|case| format!(
                "{}:{}",
                case["root"].as_str().unwrap(),
                case["id"].as_str().unwrap()
            ))
            .collect::<Vec<_>>(),
        [
            "MARKET_RESPONSE:high",
            "MARKET_RESPONSE:low",
            "MARKET_RESPONSE:ascii",
            "INTX_INSTRUMENTS_RESPONSE:high",
            "INTX_INSTRUMENTS_RESPONSE:low",
            "INTX_INSTRUMENTS_RESPONSE:ascii",
            "PORTFOLIO_BREAKDOWN_RESPONSE:high",
            "PORTFOLIO_BREAKDOWN_RESPONSE:low",
            "PORTFOLIO_BREAKDOWN_RESPONSE:ascii"
        ],
        "independent ignored string witness content floor"
    );
    let (mut refused, mut positive) = (0, 0);
    for case in cases {
        let counts = match case["root"].as_str().unwrap() {
            "MARKET_RESPONSE" => {
                ignored_string_probe::<types::MarketResponse>(case, |r| &r.products[0].product_id)
            }
            "INTX_INSTRUMENTS_RESPONSE" => {
                ignored_string_probe::<types::IntxInstrumentsResponse>(case, |r| {
                    &r[0].instrument_id
                })
            }
            "PORTFOLIO_BREAKDOWN_RESPONSE" => ignored_string_probe::<
                types::PortfolioBreakdownResponse,
            >(case, |r| &r.breakdown.portfolio.name),
            other => panic!("unreviewed ignored string root {other}"),
        };
        refused += counts.0;
        positive += counts.1;
    }
    assert_eq!(
        refused, 18,
        "six ignored-lone-unit records refused through three routes"
    );
    assert_eq!(
        positive, 9,
        "three ignored ASCII records accepted through three routes"
    );
}
