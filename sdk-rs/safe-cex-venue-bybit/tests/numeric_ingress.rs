//! Existing map-only decoders must not silently lose a numeric lexeme's sign.
use safe_cex_venue_bybit::{
    fill_event::BybitExecutionAmounts, rate_limits::*, response::BybitResponseMeta, types::*,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

fn check_with<T: DeserializeOwned>(
    case: &Value,
    project: impl Fn(&T, &str) -> f64,
    from_value: impl Fn(Value) -> Result<T, serde_json::Error>,
) {
    let text = case["text"].as_str().unwrap();
    let path = case["path"].as_str().unwrap();
    let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
    let raw: T = serde_json::from_str(text).unwrap_or_else(|error| panic!("raw {case}: {error}"));
    assert_eq!(project(&raw, path).to_bits(), expected, "raw {case}");
    let input: Value = serde_json::from_str(text).unwrap();
    let decoded: T = from_value(input).unwrap();
    let actual = project(&decoded, path).to_bits();
    assert_eq!(actual, expected, "Value {case}");
}

fn check<T: DeserializeOwned + serde::Serialize + safe_cex_num::numeric_ingress::JsNumberOutput>(
    case: &Value,
    project: impl Fn(&T, &str) -> f64,
) {
    let input: Value = serde_json::from_str(case["text"].as_str().unwrap()).unwrap();
    let borrowed: T = safe_cex_num::numeric_ingress::js_from_value_ref(&input)
        .unwrap_or_else(|e| panic!("borrowed {case}: {e}"));
    let expected = u64::from_str_radix(case["bits"].as_str().unwrap(), 16).unwrap();
    assert_eq!(
        project(&borrowed, case["path"].as_str().unwrap()).to_bits(),
        expected,
        "borrowed {case}"
    );
    let raw: T =
        safe_cex_num::numeric_ingress::js_from_str(case["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        project(&raw, case["path"].as_str().unwrap()).to_bits(),
        expected,
        "shared raw {case}"
    );
    if case["lexeme"] == "-0" {
        let mut native = input;
        *native.pointer_mut(case["path"].as_str().unwrap()).unwrap() = serde_json::json!(-0.0);
        let native: T = safe_cex_num::numeric_ingress::js_from_value(native).unwrap();
        assert_eq!(
            project(&native, case["path"].as_str().unwrap()).to_bits(),
            expected,
            "native negative zero {case}"
        );
    }
    check_with(
        case,
        project,
        safe_cex_num::numeric_ingress::js_from_value::<T>,
    );
}

#[test]
#[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
fn numeric_ingress_preserves_all_registered_records() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/numeric-ingress-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 306);
    for case in &cases {
        match case["record"].as_str().unwrap() {
            "BybitApiKeyInfo" => check::<BybitApiKeyInfo>(case, |v, p| match p {
                "/readOnly" => v.read_only,
                "/type" => v.r#type,
                "/deadlineDay" => v.deadline_day,
                "/unified" => v.unified,
                "/uta" => v.uta,
                "/userID" => v.user_i_d,
                "/inviterID" => v.inviter_i_d,
                "/affiliateID" => v.affiliate_i_d,
                _ => panic!("new numeric field {p}"),
            }),
            "BybitTransactionHistoryOptions" => {
                check::<BybitTransactionHistoryOptions>(case, |v, p| match p {
                    "/startTime" => v.start_time.unwrap(),
                    "/endTime" => v.end_time.unwrap(),
                    "/limit" => v.limit.unwrap(),
                    _ => panic!("new numeric field {p}"),
                })
            }
            "BybitRateLimitAccountRates" => {
                check::<BybitRateLimitAccountRates>(case, |v, p| match p {
                    "/classicAndUTA" => v.classic_and_uta,
                    "/utaPro" => v.uta_pro,
                    _ => panic!("new numeric field {p}"),
                })
            }
            "BybitRateLimitRow" => check::<BybitRateLimitRow>(case, |v, p| {
                let (category, field) = p[1..].split_once('/').unwrap();
                let rates = match category {
                    "futures" => &v.futures,
                    "option" => &v.option,
                    "spot" => &v.spot,
                    _ => panic!("new rate category {category}"),
                };
                match field {
                    "classicAndUTA" => rates.classic_and_uta,
                    "utaPro" => rates.uta_pro,
                    _ => panic!("new numeric field {p}"),
                }
            }),
            "BybitResponseMeta" => check::<BybitResponseMeta>(case, |v, p| {
                if p == "/retCode" {
                    v.ret_code.unwrap()
                } else {
                    let raw_path = p.strip_prefix("/rawResponse").unwrap();
                    raw_path
                        .split('/')
                        .skip(1)
                        .fold(v.raw_response.as_ref().unwrap(), |value, key| {
                            if let Some(array) = value.as_array() {
                                &array[key.parse::<usize>().unwrap()]
                            } else {
                                value.get(key).unwrap()
                            }
                        })
                        .as_f64()
                        .unwrap()
                }
            }),
            "BybitExecutionAmounts" => check::<BybitExecutionAmounts>(case, |v, p| match p {
                "/price" => v.price,
                "/amount" => v.amount,
                _ => panic!("new numeric field {p}"),
            }),
            "BybitPlaceOrderOptsPlain"
            | "BybitPlaceOrderOptsBbo"
            | "BybitPlaceOrderOptsTickSize" => {
                use safe_cex_venue_bybit::order_params::BybitPlaceOrderOpts;
                check::<BybitPlaceOrderOpts>(case, |v, p| {
                    let base: &safe_cex_types::PlaceOrderOpts = std::borrow::Borrow::borrow(v);
                    match p {
                        "/amount" => base.amount,
                        "/price" => base.price.unwrap(),
                        "/triggerPrice" => base.trigger_price.unwrap(),
                        "/stopLoss" => base.stop_loss.unwrap(),
                        "/takeProfit" => base.take_profit.unwrap(),
                        "/maxSlippagePct" => base.max_slippage_pct.unwrap(),
                        "/impliedVolatility" => base.implied_volatility.unwrap(),
                        "/bybitSlippageToleranceTicks" => match v {
                            BybitPlaceOrderOpts::TickSize(v) => v.ticks,
                            _ => panic!("wrong variant"),
                        },
                        _ => panic!("new numeric field {p}"),
                    }
                });
            }
            record => panic!("new numeric record {record}"),
        }
    }
}
