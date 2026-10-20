//! ApeX API canonicalization and zkLink contract bytes; no provider or JS runtime.
use crate::Options;
use safe_cex_crypto::{Encoding, encode as crypto_encode, hex, hmac_sha256, sha256, zklink};
pub use safe_cex_engine::url::encode;
use safe_cex_engine::{Method, num};

#[derive(Clone)]
pub enum Body {
    None,
    Form(Vec<(String, String)>),
    Raw(String),
    Multipart(Vec<(String, String)>),
}

#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub params: Vec<(String, String)>,
    pub body: Body,
}

impl Request {
    pub fn get(path: impl Into<String>, params: Vec<(String, String)>) -> Self {
        Self {
            method: Method::Get,
            path: path.into(),
            params,
            body: Body::None,
        }
    }
    pub fn form(path: impl Into<String>, params: Vec<(String, String)>) -> Self {
        Self {
            method: Method::Post,
            path: path.into(),
            params: vec![],
            body: Body::Form(params),
        }
    }
    pub fn raw(path: impl Into<String>, body: String) -> Self {
        Self {
            method: Method::Post,
            path: path.into(),
            params: vec![],
            body: Body::Raw(body),
        }
    }
    pub fn multipart(path: impl Into<String>, params: Vec<(String, String)>) -> Self {
        Self {
            method: Method::Post,
            path: path.into(),
            params: vec![],
            body: Body::Multipart(params),
        }
    }
}

pub struct Signed {
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}

/// Known ApeX fields are flat ASCII camelCase names; qs sorts these lexically.
pub fn query(params: &[(String, String)]) -> String {
    let mut sorted: Vec<_> = params.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
        .into_iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn form(params: &[(String, String)]) -> String {
    query(params)
}

fn hmac(secret: &str, text: &str) -> String {
    // The base64 TEXT is the key, rather than decoded API-secret bytes.
    let key = crypto_encode(secret.as_bytes(), Encoding::Base64);
    hmac_sha256(key.as_bytes(), text.as_bytes(), Encoding::Base64)
}

fn body_canonical(body: &Body, legacy: bool) -> String {
    match body {
        Body::None => String::new(),
        Body::Raw(raw) if !legacy => raw.clone(),
        // qs.stringify(string) is empty on the legacy path.
        Body::Raw(_) => String::new(),
        Body::Form(params) | Body::Multipart(params) => query(params),
    }
}

pub fn request(
    options: &Options,
    now: f64,
    request: &Request,
    legacy: bool,
) -> Result<Signed, String> {
    if !now.is_finite() || now < 0.0 {
        return Err("Invalid ApeX timestamp".into());
    }
    let serialized_query = query(&request.params);
    let path = if serialized_query.is_empty() {
        request.path.clone()
    } else {
        format!("{}?{serialized_query}", request.path)
    };
    if request.path == "/v3/klines" && request.method == Method::Get {
        return Ok(Signed {
            path,
            headers: vec![],
            body: None,
        });
    }
    let public = request.path == "/v3/symbols";
    if !public
        && (options.key.as_deref().unwrap_or("").is_empty()
            || options.secret.as_deref().unwrap_or("").is_empty()
            || options.passphrase.as_deref().unwrap_or("").is_empty())
    {
        return Err("ApeX API key, secret and passphrase are required".into());
    }
    let timestamp = num::format(if legacy {
        (now / 1000.0).floor() * 1000.0
    } else {
        now.floor()
    });
    let query_method = matches!(request.method, Method::Get | Method::Delete);
    let canonical = if query_method {
        if legacy {
            // These API bounds originate as numbers; the compatibility signer
            // omits zero while the request URL keeps the primary serialization.
            query(
                &request
                    .params
                    .iter()
                    .filter(|(key, value)| {
                        !matches!(key.as_str(), "beginTimeInclusive" | "endTimeExclusive")
                            || value != "0"
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        } else {
            serialized_query
        }
    } else {
        body_canonical(&request.body, legacy)
    };
    let mut text = format!(
        "{timestamp}{}/api{}{}{}",
        request.method.as_str(),
        request.path,
        if query_method && !canonical.is_empty() {
            "?"
        } else {
            ""
        },
        canonical
    );
    if legacy && request.path.contains("/v3/order") {
        match &request.body {
            Body::Raw(raw) => text.push_str(raw),
            Body::None => {}
            Body::Form(_) | Body::Multipart(_) => text.push_str("[object Object]"),
        }
    }
    let mut headers = vec![
        (
            "APEX-SIGNATURE",
            hmac(options.secret.as_deref().unwrap_or(""), &text),
        ),
        ("APEX-API-KEY", options.key.clone().unwrap_or_default()),
        (
            "APEX-PASSPHRASE",
            options.passphrase.clone().unwrap_or_default(),
        ),
        ("APEX-TIMESTAMP", timestamp),
    ];
    let body = match &request.body {
        Body::None => {
            headers.push(("Content-Type", "application/json, charset=utf-8".into()));
            None
        }
        Body::Raw(raw) => {
            headers.push((
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8".into(),
            ));
            Some(raw.clone())
        }
        Body::Form(params) => {
            headers.push((
                "Content-Type",
                "application/x-www-form-urlencoded; charset=UTF-8".into(),
            ));
            Some(query(params))
        }
        Body::Multipart(params) => {
            const BOUNDARY: &str = "safe-cex-apexomni";
            let mut body = String::new();
            for (key, value) in params {
                if key.contains(['\r', '\n', '"']) || value.contains(BOUNDARY) {
                    return Err("Invalid ApeX multipart field".into());
                }
                body.push_str(&format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{key}\"\r\n\r\n{value}\r\n"));
            }
            body.push_str(&format!("--{BOUNDARY}--\r\n"));
            headers.push((
                "Content-Type",
                format!("multipart/form-data; boundary={BOUNDARY}"),
            ));
            Some(body)
        }
    };
    Ok(Signed {
        path,
        headers,
        body,
    })
}

pub fn ws_login(options: &Options, now: f64) -> Result<String, String> {
    let key = options
        .key
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or("ApeX API key is required")?;
    let secret = options
        .secret
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or("ApeX API secret is required")?;
    let passphrase = options
        .passphrase
        .as_deref()
        .filter(|v| !v.is_empty())
        .ok_or("ApeX passphrase is required")?;
    if !now.is_finite() || now < 0.0 {
        return Err("Invalid ApeX timestamp".into());
    }
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Login<'a> {
        r#type: &'a str,
        http_method: &'a str,
        request_path: &'a str,
        api_key: &'a str,
        passphrase: &'a str,
        timestamp: u64,
        signature: String,
    }
    #[derive(serde::Serialize)]
    struct Envelope {
        op: &'static str,
        args: [String; 1],
    }
    let login = Login {
        r#type: "login",
        http_method: "GET",
        request_path: "/ws/accounts",
        api_key: key,
        passphrase,
        timestamp: now.floor() as u64,
        signature: hmac(
            secret,
            &format!("{}GET/ws/accounts", num::format(now.floor())),
        ),
    };
    let args = [serde_json::to_string(&login).map_err(|e| e.to_string())?];
    serde_json::to_string(&Envelope { op: "login", args }).map_err(|e| e.to_string())
}

fn seed_bytes(seed: &str) -> Result<[u8; 65], String> {
    let seed = seed.strip_prefix("0x").unwrap_or(seed);
    if seed.len() != 130 || !seed.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid OmniKey seed: expected 65-byte Ethereum signature".into());
    }
    let mut bytes = [0; 65];
    for (target, pair) in bytes
        .iter_mut()
        .zip(seed.as_bytes().as_chunks::<2>().0.iter())
    {
        *target = u8::from_str_radix(std::str::from_utf8(pair).map_err(|e| e.to_string())?, 16)
            .map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

pub fn validate_seed(seed: &str) -> Result<(), String> {
    seed_bytes(seed).map(|_| ())
}

fn decimal_mod(text: &str, modulus: u128) -> Result<u128, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("Invalid ApeX account ID".into());
    }
    Ok(text
        .bytes()
        .fold(0, |acc, b| (acc * 10 + u128::from(b - b'0')) % modulus))
}

fn hash_mod(bytes: &[u8], modulus: u128) -> u128 {
    bytes
        .iter()
        .fold(0, |acc, b| (acc * 256 + u128::from(*b)) % modulus)
}

/// BigNumber(decimal) * 1e18, ROUND_DOWN; parse decimal text before scaling.
pub fn fixed_18(text: &str) -> Result<u128, String> {
    let (base, exponent) =
        text.split_once(['e', 'E'])
            .map_or(Ok((text, 0)), |(base, exponent)| {
                exponent
                    .parse::<i32>()
                    .map(|e| (base, e))
                    .map_err(|_| "Invalid ApeX decimal".to_owned())
            })?;
    let (integer, fraction) = base.split_once('.').unwrap_or((base, ""));
    if integer.is_empty()
        || !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
    {
        return Err("Invalid ApeX decimal".into());
    }
    let shift = 18i64 + i64::from(exponent) - fraction.len() as i64;
    let mut digits = format!("{integer}{fraction}");
    if shift < 0 {
        digits.truncate(digits.len().saturating_sub((-shift) as usize));
    }
    let mut value = if digits.is_empty() {
        0
    } else {
        digits
            .parse::<u128>()
            .map_err(|_| "ApeX decimal exceeds u128")?
    };
    if shift > 0 {
        if value == 0 {
            return Ok(0);
        }
        let power = u32::try_from(shift)
            .ok()
            .and_then(|s| 10u128.checked_pow(s))
            .ok_or("ApeX decimal exceeds u128")?;
        value = value
            .checked_mul(power)
            .ok_or("ApeX decimal exceeds u128")?;
    }
    Ok(value)
}

fn decimal_parts(text: &str) -> Result<(String, i32), String> {
    let (base, exponent) =
        text.split_once(['e', 'E'])
            .map_or(Ok((text, 0)), |(base, exponent)| {
                exponent
                    .parse::<i32>()
                    .map(|e| (base, e))
                    .map_err(|_| "Invalid ApeX decimal".to_owned())
            })?;
    let (integer, fraction) = base.split_once('.').unwrap_or((base, ""));
    if integer.is_empty()
        || !integer
            .bytes()
            .chain(fraction.bytes())
            .all(|b| b.is_ascii_digit())
        || integer.len() + fraction.len() > 1000
    {
        return Err("Invalid ApeX decimal".into());
    }
    let exponent = exponent
        .checked_sub(fraction.len() as i32)
        .ok_or("Invalid ApeX decimal exponent")?;
    let digits = format!("{integer}{fraction}");
    let digits = digits.trim_start_matches('0');
    Ok((
        if digits.is_empty() { "0" } else { digits }.into(),
        exponent,
    ))
}

fn multiply_digits(left: &str, right: &str) -> String {
    let mut product = vec![0u32; left.len() + right.len()];
    for (i, l) in left.bytes().rev().enumerate() {
        for (j, r) in right.bytes().rev().enumerate() {
            product[i + j] += u32::from(l - b'0') * u32::from(r - b'0');
        }
    }
    for index in 0..product.len() - 1 {
        product[index + 1] += product[index] / 10;
        product[index] %= 10;
    }
    while product.len() > 1 && product.last() == Some(&0) {
        product.pop();
    }
    product
        .iter()
        .rev()
        .map(|digit| char::from(b'0' + *digit as u8))
        .collect()
}

/// Decimal multiplication precedes the two Number boundaries in TS feeHelper.
pub fn limit_fee(price: &str, size: &str, taker_fee: &str) -> Result<String, String> {
    let mut product = "1".to_owned();
    let mut exponent = 0i32;
    for value in [price, size, taker_fee] {
        let (digits, power) = decimal_parts(value)?;
        product = multiply_digits(&product, &digits);
        exponent = exponent
            .checked_add(power)
            .ok_or("Invalid ApeX fee exponent")?;
    }
    let value = format!("{product}e{exponent}")
        .parse::<f64>()
        .map_err(|_| "Invalid ApeX fee")?;
    if !value.is_finite() {
        return Err("Invalid ApeX fee".into());
    }
    // BigNumber(Number(product)) * 1e6, converted back to Number before ceil.
    let (digits, exponent) = decimal_parts(&value.to_string())?;
    let scaled = format!("{digits}e{}", i64::from(exponent) + 6)
        .parse::<f64>()
        .map_err(|_| "Invalid ApeX fee")?;
    if !scaled.is_finite() {
        return Err("Invalid ApeX fee".into());
    }
    Ok(format!("{:.6}", scaled.ceil() / 1e6))
}

fn packed_amount(amount: u128) -> Result<[u8; 5], String> {
    const MAX: u128 = (1 << 35) - 1;
    let mut exponent = 0u32;
    let mut scale = 1u128;
    while amount / scale > MAX || (amount / scale == MAX && !amount.is_multiple_of(scale)) {
        exponent += 1;
        scale = scale
            .checked_mul(10)
            .ok_or("ApeX size exceeds packed amount range")?;
    }
    if exponent > 31 {
        return Err("ApeX size exceeds packed amount range".into());
    }
    let mut mantissa = amount / scale;
    if exponent > 0 {
        let previous = MAX
            .checked_mul(scale / 10)
            .ok_or("ApeX packed amount overflow")?;
        if amount - mantissa * scale >= amount - previous {
            exponent -= 1;
            mantissa = MAX;
        }
    }
    let encoded = ((mantissa << 5) | u128::from(exponent)).to_be_bytes();
    Ok(encoded[11..].try_into().expect("five-byte amount"))
}

fn fee_rate(text: &str, maker: bool) -> Result<u8, String> {
    let fee = text.parse::<f64>().map_err(|_| "Invalid ApeX fee rate")? * 1e4;
    let fee = fee.ceil();
    if !fee.is_finite() || !(-255.0..=255.0).contains(&fee) {
        return Err("Invalid ApeX fee rate".into());
    }
    Ok(if maker {
        fee.abs()
    } else {
        fee.rem_euclid(256.0)
    } as u8)
}

#[allow(clippy::too_many_arguments)]
pub fn contract_payload(
    client_id: &str,
    account_id: &str,
    pair_id: u16,
    size: &str,
    price: &str,
    maker_fee: &str,
    taker_fee: &str,
    buy: bool,
) -> Result<[u8; 36], String> {
    let hash = sha256(client_id.as_bytes());
    let nonce = hash_mod(&hash, u128::from(u32::MAX)) as u32;
    let remainder = hash_mod(&hash, u128::from(u64::MAX));
    // BigNumber's quotient becomes a JS Number, then wasm's u32 parameter.
    let divisor = u128::from(u32::MAX);
    let mut slot = remainder / divisor;
    // Only rounding across the next integer changes the later u32 conversion.
    // Compare that half-ULP boundary as a rational, before any f64 rounding.
    if slot > 0 {
        let integer_bits = 127 - slot.leading_zeros();
        let distance = divisor - remainder % divisor;
        if distance << (53 - integer_bits) < divisor {
            slot += 1;
        }
    }
    let slot = slot as u32;
    let account = decimal_mod(account_id, u128::from(u32::MAX))? as u32;
    if account == 0 {
        return Err("Invalid ApeX L2 account ID".into());
    }
    let price = fixed_18(price)?;
    if price >= 1u128 << 120 {
        return Err("ApeX price exceeds 120 bits".into());
    }
    let mut bytes = [0; 36];
    bytes[0] = 0xfe;
    bytes[1..5].copy_from_slice(&account.to_be_bytes());
    // Contract wire widths are narrower than the WASM builder arguments.
    bytes[6..8].copy_from_slice(&(slot as u16).to_be_bytes());
    bytes[8..11].copy_from_slice(&nonce.to_be_bytes()[1..]);
    bytes[11] = pair_id as u8;
    bytes[12] = u8::from(buy);
    bytes[13..18].copy_from_slice(&packed_amount(fixed_18(size)?)?);
    bytes[18..33].copy_from_slice(&price.to_be_bytes()[1..]);
    bytes[33] = fee_rate(maker_fee, true)?;
    bytes[34] = fee_rate(taker_fee, false)?;
    // Byte 35 is hasSubsidy=false, explicitly supplied by the TS exchange.
    Ok(bytes)
}

#[allow(clippy::too_many_arguments)]
pub fn contract_signature(
    seed: &str,
    client_id: &str,
    account_id: &str,
    pair_id: u16,
    size: &str,
    price: &str,
    maker_fee: &str,
    taker_fee: &str,
    buy: bool,
) -> Result<String, String> {
    let seed = seed_bytes(seed)?;
    let payload = contract_payload(
        client_id, account_id, pair_id, size, price, maker_fee, taker_fee, buy,
    )?;
    zklink::sign(&seed, &payload)
        .map(|signature| hex(&signature.signature))
        .map_err(str::to_owned)
}
