use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_coinbase::types;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::process::Command;
#[path = "support/capture_metadata.rs"]
mod capture_metadata;

fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-ingress.cjs"
        ))
        .output()
        .expect("live TypeScript numeric ingress oracle requires Node");
    assert!(
        output.status.success(),
        "live TypeScript numeric ingress oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).expect("source numeric captures");
    capture_metadata::assert_capture_metadata(&value, 1);
    value
}
fn routes<T>(case: &Value, read: impl Fn(&T) -> f64) -> usize
where
    T: DeserializeOwned + Serialize + JsNumberOutput,
{
    let raw = case["raw"].as_str().unwrap();
    let value: Value = serde_json::from_str(raw).unwrap();
    let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
    let mut numeric_input = &value;
    for key in case["path"].as_array().unwrap() {
        numeric_input = if let Some(index) = key.as_u64() {
            &numeric_input[index as usize]
        } else {
            &numeric_input[key.as_str().unwrap()]
        };
    }
    assert_eq!(
        safe_cex_num::numeric_ingress::js_value_as_f64(numeric_input)
            .expect("retained native input number")
            .to_bits(),
        expected,
        "retained input Value native bits before typed decode"
    );
    let decoded = [
        ("bare raw str", serde_json::from_str::<T>(raw)),
        (
            "bare raw slice",
            serde_json::from_slice::<T>(raw.as_bytes()),
        ),
        (
            "bare owned Value",
            serde_json::from_value::<T>(value.clone()),
        ),
        ("bare borrowed Value", T::deserialize(&value)),
        ("shared raw str", js_from_str::<T>(raw)),
        ("shared owned Value", js_from_value::<T>(value.clone())),
        ("shared borrowed Value", js_from_value_ref::<T>(&value)),
    ];
    let mut comparisons = 0;
    for (route, result) in decoded {
        let record = result.unwrap_or_else(|error| {
            panic!(
                "retained numeric decode {route}, {} {} {}: {error}",
                case["root"], case["path"], case["spelling"]
            )
        });
        assert_eq!(
            read(&record).to_bits(),
            expected,
            "retained numeric native bits {route}, {} {} {}",
            case["root"],
            case["path"],
            case["spelling"]
        );
        comparisons += 1;
    }
    comparisons
}
#[test]
fn all_three_numeric_roots_retain_thirteen_source_fields_through_seven_routes() {
    let capture = source();
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 260, "independent numeric source case floor");
    assert_eq!(
        capture["numeric"].as_array().unwrap().len(),
        13,
        "independent numeric field floor"
    );
    assert_eq!(
        capture["spellings"].as_array().unwrap().len(),
        20,
        "independent spelling floor"
    );
    let spellings = [
        "-0",
        "-0.0",
        "-0e0",
        "-1e-400",
        "0",
        "0.0",
        "1",
        "-1",
        "1.25",
        "-3.5",
        "1e3",
        "1e-3",
        "9007199254740993",
        "-9007199254740993",
        "1e400",
        "-1e400",
        "5e-324",
        "-5e-324",
        "1.7976931348623157e308",
        "-1.7976931348623157e308",
    ];
    assert_eq!(
        capture["spellings"],
        serde_json::json!(spellings),
        "numeric spelling content pin"
    );
    let expected_paths = [
        "MARKET_RESPONSE.num_products",
        "INTX_INSTRUMENTS_RESPONSE.0.price_band_percent",
        "INTX_INSTRUMENTS_RESPONSE.0.market_order_percent",
        "INTX_INSTRUMENTS_RESPONSE.0.position_limit_adq_pct",
        "INTX_INSTRUMENTS_RESPONSE.0.base_imf",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.total_balance_fiat",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.total_balance_crypto",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_trade_fiat",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.allocation",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_trade_crypto",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.unrealized_pnl",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_transfer_fiat",
        "PORTFOLIO_BREAKDOWN_RESPONSE.breakdown.spot_positions.0.available_to_transfer_crypto",
    ];
    let identity = |item: &Value| {
        std::iter::once(item["root"].as_str().unwrap().to_owned())
            .chain(
                item["path"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|part| match part {
                        Value::String(text) => text.clone(),
                        Value::Number(number) => number.to_string(),
                        _ => panic!("unsupported numeric path key"),
                    }),
            )
            .collect::<Vec<_>>()
            .join(".")
    };
    assert_eq!(
        capture["numeric"]
            .as_array()
            .unwrap()
            .iter()
            .map(identity)
            .collect::<Vec<_>>(),
        expected_paths,
        "numeric path content pin"
    );
    let expected_cases = expected_paths
        .iter()
        .flat_map(|path| {
            spellings
                .iter()
                .map(move |spelling| (path.to_string(), spelling.to_string()))
        })
        .collect::<Vec<_>>();
    let actual_cases = cases
        .iter()
        .map(|case| {
            (
                identity(case),
                case["spelling"].as_str().unwrap().to_owned(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        actual_cases, expected_cases,
        "numeric source case content pin"
    );
    let mut comparisons = 0;
    for case in cases {
        let field = case["path"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .as_str()
            .unwrap();
        comparisons += match case["root"].as_str().unwrap() {
            "MARKET_RESPONSE" => {
                assert_eq!(field, "num_products");
                routes::<types::MarketResponse>(case, |record| record.num_products)
            }
            "INTX_INSTRUMENTS_RESPONSE" => {
                routes::<types::IntxInstrumentsResponse>(case, |record| match field {
                    "price_band_percent" => record[0].price_band_percent,
                    "market_order_percent" => record[0].market_order_percent,
                    "position_limit_adq_pct" => record[0].position_limit_adq_pct,
                    "base_imf" => record[0].base_imf,
                    other => panic!("unreviewed INTX numeric field {other}"),
                })
            }
            "PORTFOLIO_BREAKDOWN_RESPONSE" => {
                routes::<types::PortfolioBreakdownResponse>(case, |record| {
                    let spot = &record.breakdown.spot_positions[0];
                    match field {
                        "total_balance_fiat" => spot.total_balance_fiat,
                        "total_balance_crypto" => spot.total_balance_crypto,
                        "available_to_trade_fiat" => spot.available_to_trade_fiat,
                        "allocation" => spot.allocation,
                        "available_to_trade_crypto" => spot.available_to_trade_crypto,
                        "unrealized_pnl" => spot.unrealized_pnl,
                        "available_to_transfer_fiat" => spot.available_to_transfer_fiat,
                        "available_to_transfer_crypto" => spot.available_to_transfer_crypto,
                        other => panic!("unreviewed portfolio numeric field {other}"),
                    }
                })
            }
            other => panic!("unreviewed numeric root {other}"),
        };
    }
    assert_eq!(comparisons, 1820, "independent native bit comparison floor");
}
