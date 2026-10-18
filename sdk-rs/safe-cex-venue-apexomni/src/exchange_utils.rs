//! Pure helpers from apexomni.exchange.ts; preserve source Number and Date semantics.
use safe_cex_num::{js_time_clip, number_from_string};

const ONE_HOUR_MS: f64 = 60.0 * 60.0 * 1000.0;
const STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS: f64 = 24.0 * 7.0;

// ts: isoTimestampToEpochHours
pub fn iso_timestamp_to_epoch_hours(iso_timestamp: &str) -> f64 {
    (js_time_clip(number_from_string(iso_timestamp)) / ONE_HOUR_MS).ceil()
}

// ts: addOrderExpirationBufferHours
pub fn add_order_expiration_buffer_hours(expiration_epoch_hours: f64) -> f64 {
    expiration_epoch_hours + STARK_ORDER_SIGNATURE_EXPIRATION_BUFFER_HOURS
}

fn character_at(alphabet: &str, index: f64) -> Option<char> {
    if index >= 0.0 && index < alphabet.len() as f64 {
        alphabet.as_bytes().get(index as usize).map(|c| *c as char)
    } else {
        None
    }
}

/// The source can return undefined or NaN when an injected host draw is invalid.
#[derive(Debug, PartialEq)]
pub enum RandomId {
    Undefined,
    Nan,
    String(String),
}
impl RandomId {
    pub fn as_js_string(&self) -> &str {
        match self {
            Self::Undefined => "undefined",
            Self::Nan => "NaN",
            Self::String(value) => value,
        }
    }
}

// ts: d8
pub fn d8(e: Option<f64>, t: Option<&str>, mut random: impl FnMut() -> f64) -> RandomId {
    let e = e.unwrap_or(16.0);
    let t = t.unwrap_or("D");
    let n = "123456789";
    let r = "abcdef";
    let i = if t == "hex" {
        format!("{n}{r}")
    } else {
        n.to_owned()
    };
    let s = if t == "hex" {
        format!("0{n}{r}")
    } else {
        format!("0{n}")
    };
    let c = if t == "hex" { 15.0 } else { 9.0 };
    let mut l = character_at(&i, (random() * c).floor())
        .map(|c| RandomId::String(c.to_string()))
        .unwrap_or(RandomId::Undefined);
    let mut d = 1.0;
    while d < e {
        let next = character_at(&s, (random() * (c + 1.0)).floor());
        l = match (&l, next) {
            (RandomId::Undefined | RandomId::Nan, None) => RandomId::Nan,
            (_, Some(next)) => RandomId::String(format!("{}{next}", l.as_js_string())),
            (_, None) => RandomId::String(format!("{}undefined", l.as_js_string())),
        };
        d += 1.0;
    }
    l
}

// ts: generateApeXRandomClientId in apexomni.exchange.ts, distinct from utils.ts.
pub fn generate_apex_random_client_id(
    account_id: &str,
    now: impl FnOnce() -> f64,
    random: impl FnMut() -> f64,
) -> String {
    let timestamp = safe_cex_num::js_number_string(now());
    let suffix = d8(Some(6.0), None, random);
    format!(
        "apexomni-{account_id}-{timestamp}-{}",
        suffix.as_js_string()
    )
}

// ts: calculateTpSlPrice
pub fn calculate_tp_sl_price(side: &str, market_price: f64, price_step: f64) -> f64 {
    let slippage_multiplier = if side == "BUY" { 1.005 } else { 0.995 };
    let execution_price = market_price * slippage_multiplier;
    safe_cex_num::adjust(execution_price, price_step)
}

// ts: hashClientOrderId. A code-unit input retains the full source string domain.
pub fn hash_client_order_id(client_order_id: &[u16]) -> [u8; 32] {
    safe_cex_crypto::sha256(String::from_utf16_lossy(client_order_id).as_bytes())
}

fn decimal_multiply(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() || a == 0.0 || b == 0.0 {
        return a * b;
    }
    fn parts(value: f64) -> (num_bigint::BigInt, i32) {
        let text = safe_cex_num::js_number_string(value);
        let (mantissa, exponent) = text.split_once('e').map_or((text.as_str(), 0), |(m, e)| {
            (m, e.parse::<i32>().expect("number exponent"))
        });
        let decimals = mantissa
            .split_once('.')
            .map_or(0, |(_, tail)| tail.len() as i32);
        let integer = mantissa.replace('.', "");
        (integer.parse().expect("number digits"), exponent - decimals)
    }
    let (a, ae) = parts(a);
    let (b, be) = parts(b);
    format!("{}e{}", a * b, ae + be)
        .parse()
        .expect("decimal product")
}

// ts: feeHelper. The host supplies the source's dynamically selected Math call.
pub fn fee_helper(
    input: &crate::order_records::Iso8601,
    digits: Option<f64>,
    math: impl FnOnce(f64) -> Result<f64, String>,
) -> Result<String, String> {
    let number = match input {
        crate::order_records::Iso8601::Number(value) => *value,
        crate::order_records::Iso8601::String(value) => safe_cex_num::number_from_string(value),
    };
    let digits = digits.unwrap_or(2.0);
    if number.is_nan() {
        return safe_cex_num::js_to_fixed(0.0, digits).map_err(str::to_owned);
    }
    let scale = 10.0_f64.powf(digits);
    safe_cex_num::js_to_fixed(math(decimal_multiply(number, scale))? / scale, digits)
        .map_err(str::to_owned)
}

#[cfg(test)]
mod accepted_scalar_helper_tests {
    use super::*;
    use crate::accepted_scalar_helper_support::*;
    #[test]
    fn iso_timestamp_to_epoch_hours_native_output() {
        check(
            "exchange_utils::iso_timestamp_to_epoch_hours",
            17,
            |value| {
                serde_json::json!(native_bits(iso_timestamp_to_epoch_hours(
                    value.as_str().unwrap()
                )))
            },
        );
    }
    #[test]
    fn character_at_native_output() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/accepted-character-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "actual d8 character fixtures: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        check("exchange_utils::character_at", 11, |value| {
            serde_json::json!(character_at(
                value["alphabet"].as_str().unwrap(),
                float(&value["index"])
            ))
        });
    }
    #[test]
    fn d8_native_output() {
        check("exchange_utils::d8", 9, |value| {
            let mut calls = 0usize;
            let draws = value["draws"].as_array().unwrap();
            let actual = d8(
                value["length"].as_str().map(|_| float(&value["length"])),
                value["mode"].as_str(),
                || {
                    let v = float(&draws[calls % draws.len()]);
                    calls += 1;
                    v
                },
            );
            let variant = match &actual {
                crate::exchange_utils::RandomId::Undefined => "undefined",
                crate::exchange_utils::RandomId::Nan => "nan",
                crate::exchange_utils::RandomId::String(_) => "string",
            };
            serde_json::json!({"variant":variant,"text":actual.as_js_string(),"calls":calls})
        });
    }
    #[test]
    fn generate_apex_random_client_id_native_output() {
        check(
            "exchange_utils::generate_apex_random_client_id",
            4,
            |value| {
                let mut clock = 0;
                let mut entropy = 0;
                let actual = generate_apex_random_client_id(
                    value["account"].as_str().unwrap(),
                    || {
                        clock += 1;
                        float(&value["now"])
                    },
                    || {
                        entropy += 1;
                        float(&value["draw"])
                    },
                );
                serde_json::json!({"text":actual,"clockCalls":clock,"entropyCalls":entropy})
            },
        );
    }
    #[test]
    fn calculate_tp_sl_price_native_output() {
        check("exchange_utils::calculate_tp_sl_price", 8, |value| {
            serde_json::json!(native_bits(calculate_tp_sl_price(
                value["side"].as_str().unwrap(),
                float(&value["market"]),
                float(&value["step"]),
            )))
        });
    }
    #[test]
    fn fee_helper_native_output() {
        check("exchange_utils::fee_helper", 14, |value| {
            let input = if value["kind"] == "number" {
                crate::order_records::Iso8601::Number(float(&value["value"]))
            } else {
                crate::order_records::Iso8601::String(value["value"].as_str().unwrap().to_owned())
            };
            let mut seen = Vec::new();
            let actual = fee_helper(
                &input,
                value["digits"].as_str().map(|_| float(&value["digits"])),
                |number| {
                    seen.push(native_bits(number));
                    match value["operation"].as_str().unwrap() {
                        "floor" => Ok(number.floor()),
                        "ceil" => Ok(number.ceil()),
                        "round" => Ok(safe_cex_num::math_round(number)),
                        "error" => Err("host math failed".to_owned()),
                        _ => unreachable!(),
                    }
                },
            );
            let result = match actual {
                Ok(value) => serde_json::json!({"ok":true,"value":value}),
                Err(message) => serde_json::json!({"ok":false,"message":message}),
            };
            serde_json::json!({"result":result,"seen":seen})
        });
    }
}
