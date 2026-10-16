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
