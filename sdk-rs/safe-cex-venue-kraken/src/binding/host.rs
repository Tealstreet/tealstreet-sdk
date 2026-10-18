//! REST host family over js_runtime values: reads, errors, trading, the API
//! factory and the constructor, all running on `rt.this` (the exchange receiver).
use super::{
    client,
    promise::{Promise, await_js},
    rt::Rt,
    value::{Js, Thrown, arg, from_typed},
};
use crate::{
    api::{self, Api, ApiHost, Form},
    construction::{BaseUrl, ConstructorHost, Factory, RateLimit, Venue},
    rest_account::{Entries, Handle, ReadHost, ReadResult},
    trading::{Client, ErrorHost, Method, TradingHost},
};
use base64::Engine;
use hmac::Mac;
use safe_cex_num::{TableHit, ccxt_precision::precision_to_string, parse_float, parse_int};
use std::{cell::RefCell, future::Future, rc::Rc};

pub(crate) fn h(v: Js) -> Rc<Js> {
    Rc::new(v)
}
pub(crate) fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
pub(crate) fn lossy(s: &[u16]) -> String {
    String::from_utf16_lossy(s)
}

/// A Node `Buffer`: raw bytes behind an object the bodies only pass along.
pub(crate) fn buffer(bytes: &[u8]) -> Js {
    Js::object_with_class(
        "Buffer",
        vec![(
            "bytes",
            Js::array(bytes.iter().map(|b| Js::Number(f64::from(*b))).collect()),
        )],
    )
}
pub(crate) fn bytes_of(v: &Js) -> Result<Vec<u8>, Thrown> {
    if let Js::Object(o) = v
        && o.class == "Buffer"
    {
        return Ok(v
            .get("bytes")?
            .array_values()
            .unwrap_or_default()
            .iter()
            .map(|b| b.as_f64().unwrap_or(0.0) as u8)
            .collect());
    }
    Ok(lossy(&v.to_js_string()?).into_bytes())
}

/// Node's lenient `Buffer.from(text, 'base64')`: url-safe alphabet accepted,
/// other characters skipped, decoding stops at the first padding character.
pub(crate) fn node_base64(text: &str) -> Vec<u8> {
    let mut out = vec![];
    let mut acc: u32 = 0;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// `encodeURIComponent`.
pub(crate) fn encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(*b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// qs's encoder: RFC3986 unreserved stays, RFC1738 also keeps parentheses and spells space `+`.
fn qs_encode(s: &str, rfc1738: bool) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b'(' | b')' if rfc1738 => out.push(*b as char),
            b' ' if rfc1738 => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// qs decodes '+' first and returns the entire input unchanged if UTF-8 or a
// percent escape is invalid, rather than replacing individual invalid octets.
fn qs_decode(s: &str) -> String {
    let input = s.replace('+', " ");
    let mut bytes = Vec::new();
    let mut cursor = input.bytes();
    while let Some(byte) = cursor.next() {
        if byte == b'%' {
            let digit = |b: u8| char::from(b).to_digit(16).map(|n| n as u8);
            let pair = cursor
                .next()
                .and_then(digit)
                .zip(cursor.next().and_then(digit));
            let Some((high, low)) = pair else {
                return input;
            };
            bytes.push(high * 16 + low);
        } else {
            bytes.push(byte);
        }
    }
    String::from_utf8(bytes).unwrap_or(input)
}

fn qs_parse_form(value: &Js) -> Result<Js, Thrown> {
    // The typed venue replays the flat form fields produced by its own
    // Spot request formatter, rather than accepting arbitrary qs syntax.
    let result = Js::object(vec![]);
    for part in lossy(&value.to_js_string()?).split('&').take(1000) {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        let key = units(&qs_decode(key));
        if key.is_empty() {
            continue;
        }
        let value = Js::str(&qs_decode(value));
        let value = if result.has_own(&lossy(&key)) {
            let previous = result.get_key(&key)?;
            let mut values = previous.array_values().unwrap_or_else(|| vec![previous]);
            values.push(value);
            Js::array(values)
        } else {
            value
        };
        result.define_key(&key, value)?;
    }
    Ok(result)
}

/// `qs.stringify(value, { arrayFormat: 'repeat', sort?, format? })`.
pub(crate) fn qs_stringify(value: &Js, sorted: bool, rfc1738: bool) -> Result<String, Thrown> {
    let collator = if sorted {
        Some(safe_cex_num::JsRootCollator::try_new().map_err(Thrown::from)?)
    } else {
        None
    };
    fn keys(
        v: &Js,
        collator: &Option<safe_cex_num::JsRootCollator>,
    ) -> Result<Vec<(String, Js)>, Thrown> {
        let mut entries: Vec<(String, Js)> = v
            .own_entries()?
            .into_iter()
            .map(|(k, v)| (lossy(&k), v))
            .collect();
        if let Some(c) = collator {
            entries.sort_by(|a, b| c.compare(&a.0, &b.0));
        }
        Ok(entries)
    }
    fn walk(
        prefix: &str,
        value: &Js,
        collator: &Option<safe_cex_num::JsRootCollator>,
        rfc1738: bool,
        out: &mut Vec<String>,
    ) -> Result<(), Thrown> {
        match value {
            Js::Undefined => {}
            Js::Null => out.push(format!("{}=", qs_encode(prefix, rfc1738))),
            Js::Array(_) => {
                for v in value.array_values().unwrap_or_default() {
                    walk(prefix, &v, collator, rfc1738, out)?;
                }
            }
            Js::Object(o) if o.class != "Buffer" => {
                for (k, v) in keys(value, collator)? {
                    walk(&format!("{prefix}[{k}]"), &v, collator, rfc1738, out)?;
                }
            }
            other => out.push(format!(
                "{}={}",
                qs_encode(prefix, rfc1738),
                qs_encode(&lossy(&other.to_js_string()?), rfc1738)
            )),
        }
        Ok(())
    }
    let mut out = vec![];
    if value.is_object() {
        for (k, v) in keys(value, &collator)? {
            walk(&k, &v, &collator, rfc1738, &mut out)?;
        }
    }
    Ok(out.join("&"))
}

/// `new Date(ms).toISOString()`.
pub(crate) fn iso(ms: f64) -> Result<String, Thrown> {
    if !ms.is_finite() || ms.abs() > 8_640_000_000_000_000.0 {
        return Err(Thrown::error("RangeError", "Invalid time value"));
    }
    let ms = ms.trunc() as i64;
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + i64::from(m <= 2);
    let time = ms.rem_euclid(86_400_000);
    let year = if (0..=9999).contains(&y) {
        format!("{y:04}")
    } else if y < 0 {
        format!("-{:06}", -y)
    } else {
        format!("+{y:06}")
    };
    Ok(format!(
        "{year}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600_000,
        (time / 60_000) % 60,
        (time / 1000) % 60,
        time % 1000
    ))
}

impl ReadHost for Rt {
    type Value = Js;
    type Error = Thrown;
    fn field(&self, value: &Js, key: &[u16], optional: bool) -> ReadResult<Self, Handle<Self>> {
        if optional && value.is_nullish() {
            return Ok(h(Js::Undefined));
        }
        Ok(h(value.get_key(key)?))
    }
    // ts: utils/get-key `v(obj, key)` — own key, then snake_case, then camelCase.
    fn keyed(&self, value: &Js, key: &str) -> ReadResult<Self, Handle<Self>> {
        if value.is_object() {
            for name in [
                key.to_owned(),
                safe_cex_num::lodash_snake_case(key),
                safe_cex_num::lodash_camel_case(key),
            ] {
                if value.has_own(&name) {
                    return Ok(h(value.get(&name)?));
                }
            }
        }
        self.field(value, &units(key), true)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn undefined(&self, value: &Js) -> bool {
        value.is_undefined()
    }
    fn strict_eq(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn number(&self, value: f64) -> Handle<Self> {
        h(Js::Number(value))
    }
    fn boolean(&self, value: bool) -> Handle<Self> {
        h(Js::Bool(value))
    }
    fn text(&self, value: &[u16]) -> Handle<Self> {
        h(Js::utf16(value))
    }
    fn string(&self, value: &Js) -> ReadResult<Self, Vec<u16>> {
        Ok(value.to_js_string()?.to_vec())
    }
    fn lower(&self, value: &[u16]) -> Vec<u16> {
        units(&lossy(value).to_lowercase())
    }
    fn upper(&self, value: &[u16]) -> Vec<u16> {
        units(&lossy(value).to_uppercase())
    }
    fn starts_with(&self, value: &Js, prefix: &str) -> ReadResult<Self, bool> {
        Ok(value
            .call_method("startsWith", vec![Js::str(prefix)])?
            .truthy())
    }
    fn split_pair(&self, value: &Js) -> ReadResult<Self, (Handle<Self>, Handle<Self>)> {
        let parts = value.call_method("split", vec![Js::str("/")])?;
        Ok((h(parts.get("0")?), h(parts.get("1")?)))
    }
    // ts: krakenspot.types normalizeAsset — `ASSET_MAP[asset] || asset`.
    fn normalize_asset(&self, value: &Js) -> ReadResult<Self, Handle<Self>> {
        if value.is_undefined() {
            return Ok(h(Js::Undefined));
        }
        let key = value.to_js_string()?;
        Ok(h(match crate::spot_types::normalize_asset(&key) {
            TableHit::Mapped(s) => Js::utf16(&s),
            TableHit::Inherited(_) => {
                return Err(Thrown::type_error(
                    "Kraken asset normalization reached an Object.prototype member",
                ));
            }
            TableHit::Missing => value.clone(),
        }))
    }
    // ts: krakenspot.types ORDER_TYPE lookup with OrderType.Limit fallback.
    fn order_type(&self, value: &Js) -> ReadResult<Self, Handle<Self>> {
        let key = lossy(&value.to_js_string()?);
        let t = crate::spot_types::ORDER_TYPE
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| *v)
            .unwrap_or(safe_cex_types::OrderType::Limit);
        Ok(h(from_typed(&t)?))
    }
    fn parse_float(&self, value: &Js) -> ReadResult<Self, f64> {
        value.parse_float()
    }
    fn numeric(&self, value: &Js) -> ReadResult<Self, f64> {
        value.to_number()
    }
    // ts: `parseFloat(precisionToString(parseInt(decimals)))`.
    fn amount_precision(&self, value: &Js) -> ReadResult<Self, f64> {
        Ok(parse_float(&precision_to_string(parse_int(
            &lossy(&value.to_js_string()?),
            None,
        ))))
    }
    fn includes(&self, value: &Js, text: &str) -> ReadResult<Self, bool> {
        Ok(value.call_method("includes", vec![Js::str(text)])?.truthy())
    }
    fn items(&self, value: &Js) -> ReadResult<Self, Vec<Handle<Self>>> {
        match value.array_values() {
            Some(items) => Ok(items.into_iter().map(h).collect()),
            None => Err(Thrown::type_error(&format!(
                "{} is not iterable",
                value.to_display()
            ))),
        }
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn entries(&self, value: &Js) -> ReadResult<Self, Entries<Self>> {
        if value.is_nullish() {
            return Err(Thrown::type_error(
                "Cannot convert undefined or null to object",
            ));
        }
        Ok(value
            .own_entries()?
            .into_iter()
            .map(|(k, v)| (k.to_vec(), h(v)))
            .collect())
    }
    fn is_object(&self, value: &Js) -> bool {
        matches!(value, Js::Null) || value.is_object()
    }
    fn array(&self, values: Vec<Handle<Self>>) -> Handle<Self> {
        h(Js::array(values.iter().map(|v| (**v).clone()).collect()))
    }
    fn object(&self, values: Entries<Self>) -> ReadResult<Self, Handle<Self>> {
        let out = Js::object(vec![]);
        for (k, v) in values {
            out.define_key(&k, (*v).clone())?;
        }
        Ok(h(out))
    }
    fn spread(&self, value: &Js, values: Entries<Self>) -> ReadResult<Self, Handle<Self>> {
        let out = value.spread_object()?;
        for (k, v) in values {
            out.define_key(&k, (*v).clone())?;
        }
        Ok(h(out))
    }
    fn assign(&self, object: &Js, key: &[u16], value: Handle<Self>) -> ReadResult<Self, ()> {
        object.set_key(key, (*value).clone())
    }
    fn current(&self, slot: &str) -> ReadResult<Self, Handle<Self>> {
        Ok(h(match slot {
            "disposed" => self.this.get("isDisposed")?,
            "publicDataOnly" => self.this.get("isPublicDataOnly")?,
            other => self.this.get("store")?.get(other)?,
        }))
    }
    fn preference(&self, cache: &str, symbol: &Js) -> ReadResult<Self, Handle<Self>> {
        Ok(h(
            cache_map(self, cache)?.call_method("get", vec![symbol.clone()])?
        ))
    }
    fn set_preference(&self, symbol: Handle<Self>, margin: Handle<Self>) -> ReadResult<Self, ()> {
        cache_map(self, "margin")?
            .call_method("set", vec![(*symbol).clone(), (*margin).clone()])?;
        Ok(())
    }
    fn now(&self) -> ReadResult<Self, f64> {
        Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms())
    }
    fn read(
        &self,
        spot: bool,
        endpoint: &str,
        post: bool,
        body: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let xhr = self.this.get("xhr")?;
        let fills = !spot && endpoint == fills_endpoint();
        let promise = if post {
            xhr.call_method("post", vec![Js::str(endpoint), (*body).clone()])?
        } else if fills {
            xhr.call_method(
                "get",
                vec![
                    Js::str(endpoint),
                    Js::object(vec![("params", (*body).clone())]),
                ],
            )?
        } else {
            xhr.call_method("get", vec![Js::str(endpoint)])?
        };
        Ok(async move {
            Ok(h(await_js(&self.exchange_runtime(), promise)
                .await?
                .get("data")?))
        })
    }
    fn fetch_balance(
        &self,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self.this.call_method("fetchBalance", vec![])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    fn update_balance(&self, patch: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .get("store")?
            .call_method("update", vec![(*patch).clone()])?;
        Ok(())
    }
    // ts: `console.trace(err)`.
    fn trace(&self, error: &Thrown) -> ReadResult<Self, ()> {
        self.shared
            .runtime
            .console("trace", &error.value.to_display());
        Ok(())
    }
    // ts: `err?.message || fallback`.
    fn error_message(&self, error: &Thrown, fallback: &str) -> ReadResult<Self, Handle<Self>> {
        let message = if error.value.is_nullish() {
            Js::Undefined
        } else {
            error.value.get("message")?
        };
        Ok(h(if message.truthy() {
            message
        } else {
            Js::str(fallback)
        }))
    }
    fn error(&self, message: Handle<Self>) -> Thrown {
        new_error(&message, None, None)
    }
    fn emit_error(&self, message: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .call_method("emitAndLogError", vec![(*message).clone()])?;
        Ok(())
    }
    // ts: kraken.fills mapKrakenFills(rows, resolveSymbol) over the live market index.
    fn map_fills(&self, rows: Handle<Self>) -> ReadResult<Self, Vec<Handle<Self>>> {
        let input = safe_cex_num::numeric_ingress::js_from_value(super::value::to_json(&rows)?)
            .map_err(|e| Thrown::type_error(&e.to_string()))?;
        let markets = self.this.get("store")?.get("marketsBySymbol")?;
        let mut resolve = |symbol: &str| -> Result<Option<String>, Box<dyn std::error::Error>> {
            let found = markets
                .get(&symbol.to_uppercase())
                .map_err(|e| e.message())?;
            let symbol = if found.is_nullish() {
                Js::Undefined
            } else {
                found.get("symbol").map_err(|e| e.message())?
            };
            Ok(if symbol.is_nullish() {
                None
            } else {
                Some(symbol.to_display())
            })
        };
        let fills = crate::fills::map_kraken_fills(input, &mut resolve, &mut |n| n)
            .map_err(|e| Thrown::error("Error", &e.to_string()))?;
        fills.iter().map(|f| Ok(h(from_typed(f)?))).collect()
    }
    fn iso(&self, milliseconds: f64) -> ReadResult<Self, Vec<u16>> {
        Ok(units(&iso(milliseconds)?))
    }
    fn warn_fills(&self, page: usize, from_iso: Vec<u16>) -> ReadResult<Self, ()> {
        let message = format!(
            "fetchHistoricalFills stopped after {page} page(s) without reaching {} — the result is a lower bound, not the whole window.",
            lossy(&from_iso)
        );
        self.this
            .call_method("log", vec![Js::str(&message), Js::str("warning")])?;
        Ok(())
    }
    fn slice_limit(&self, values: Vec<Handle<Self>>, limit: &Js) -> ReadResult<Self, Handle<Self>> {
        let all = Js::array(values.iter().map(|v| (**v).clone()).collect());
        Ok(h(all.call_method(
            "slice",
            vec![Js::Number(0.0), limit.clone()],
        )?))
    }
}

fn fills_endpoint() -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == "FILLS")
        .expect("FILLS endpoint")
        .1
}

fn cache_map(rt: &Rt, cache: &str) -> Result<Js, Thrown> {
    super::host_more::owner(rt)?.get(if cache == "margin" {
        "marginTypePreferences"
    } else {
        "leverageSettings"
    })
}

/// `new Error(message, { cause })` with an optional Axios `response` attached.
pub(crate) fn new_error(message: &Js, response: Option<&Js>, cause: Option<&Thrown>) -> Thrown {
    let text = message.to_display();
    let error = super::value::error_object("Error", &text);
    if let Some(response) = response {
        let _ = error.define("response", response.clone());
    }
    if let Some(cause) = cause {
        let _ = error.define("cause", cause.value.clone());
    }
    Thrown::new(error)
}

impl ErrorHost for Rt {
    fn is_string(&self, value: &Js) -> bool {
        value.is_string()
    }
    fn lower_method(&self, value: &Js, expression: &str) -> ReadResult<Self, Handle<Self>> {
        if !value.is_string() {
            return Err(Thrown::type_error(&format!(
                "{expression}.toLowerCase is not a function"
            )));
        }
        Ok(h(value.call_method("toLowerCase", vec![])?))
    }
    fn upper_method(&self, value: &Js, expression: &str) -> ReadResult<Self, Handle<Self>> {
        if !value.is_string() {
            return Err(Thrown::type_error(&format!(
                "{expression}.toUpperCase is not a function"
            )));
        }
        Ok(h(value.call_method("toUpperCase", vec![])?))
    }
    fn error_value(&self, error: &Thrown) -> ReadResult<Self, Handle<Self>> {
        Ok(h(error.value.clone()))
    }
    // ts: `error.response`.
    fn error_response(&self, error: &Thrown) -> ReadResult<Self, Handle<Self>> {
        Ok(h(if error.value.is_object() {
            error.value.get("response")?
        } else {
            Js::Undefined
        }))
    }
    // ts: `err?.response?.data`.
    fn error_details(&self, error: &Thrown) -> ReadResult<Self, Handle<Self>> {
        let response = self.error_response(error)?;
        Ok(h(if response.is_nullish() {
            Js::Undefined
        } else {
            response.get("data")?
        }))
    }
    fn new_error(
        &self,
        message: Handle<Self>,
        response: Option<Handle<Self>>,
        cause: Option<&Thrown>,
    ) -> Thrown {
        new_error(&message, response.as_deref(), cause)
    }
    fn emit_and_log(&self, message: Handle<Self>, context: Option<&str>) -> ReadResult<Self, ()> {
        let mut args = vec![(*message).clone()];
        if let Some(context) = context {
            args.push(Js::str(context));
        }
        self.this.call_method("emitAndLogError", args)?;
        Ok(())
    }
}

/// The JS Promise of an `async` member body run on the exchange runtime.
pub(crate) fn spawn_body(rt: &Rt, body: impl Future<Output = Result<Js, Thrown>> + 'static) -> Js {
    Js::Promise(Promise::spawn(&rt.exchange_runtime(), Box::pin(body)))
}

impl TradingHost for Rt {
    fn resolve_caller_id(&self, options: &Js) -> ReadResult<Self, ()> {
        let opts: safe_cex_types::PlaceOrderOpts = super::value::to_typed(options)?;
        self.base().resolve_caller_client_order_id(&opts)?;
        Ok(())
    }
    // ts: utils/safe-math `adjust(value, step)`.
    fn adjust(&self, value: &Js, precision: &Js) -> ReadResult<Self, f64> {
        let step = match precision {
            Js::String(s) => safe_cex_num::PrecisionInput::String(lossy(s)),
            other => safe_cex_num::PrecisionInput::Number(other.to_number()?),
        };
        Ok(safe_cex_num::adjust(value.to_number()?, step))
    }
    // ts: utils/uuid removes the nanoid URL separators.
    fn uuid(&self) -> ReadResult<Self, Vec<u16>> {
        Ok(units(
            &self.shared.runtime.unique_id().replace(['-', '_'], ""),
        ))
    }
    fn spot_client_id(&self) -> ReadResult<Self, Handle<Self>> {
        let id = (self.shared.extra.spot_ids.borrow_mut())().map_err(Thrown::from)?;
        Ok(h(Js::str(&id)))
    }
    fn iterable(&self, value: &Js, expression: &str) -> ReadResult<Self, Vec<Handle<Self>>> {
        match value {
            Js::Array(_) | Js::String(_) => {
                Ok(value.iterable_values()?.into_iter().map(h).collect())
            }
            _ => Err(Thrown::type_error(&if expression == "txids" {
                "txids is not iterable".to_owned()
            } else {
                "Spread syntax requires ...iterable[Symbol.iterator] to be a function".to_owned()
            })),
        }
    }
    fn send(
        &self,
        _spot: bool,
        client: Client,
        method: Method,
        endpoint: &str,
        body: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let xhr = self.this.get(match client {
            Client::Xhr => "xhr",
            Client::Unlimited => "unlimitedXHR",
        })?;
        let verb = match method {
            Method::Post => "post",
            Method::Put => "put",
        };
        let promise = xhr.call_method(verb, vec![Js::str(endpoint), (*body).clone()])?;
        Ok(async move {
            Ok(h(await_js(&self.exchange_runtime(), promise)
                .await?
                .get("data")?))
        })
    }
    fn derive(&self, options: Vec<Handle<Self>>) -> ReadResult<Self, Handle<Self>> {
        let opts: Vec<safe_cex_types::PlaceOrderOpts> = options
            .iter()
            .map(|o| super::value::to_typed(o))
            .collect::<Result<_, _>>()?;
        let derived = self
            .base()
            .derive_algo_orders_from_normal_orders_opts(&opts);
        Ok(h(from_typed(&derived)?))
    }
    // ts: `this.placeOrders(derived)` — BaseExchange.placeOrders on the Rust core,
    // which validates every caller ID before dispatching to this venue.
    fn place_orders(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let orders: Vec<safe_cex_types::PlaceOrderOpts> = super::value::to_typed(&options)?;
        let base = self.base().clone();
        let venue = self.shared.sockets.venue.borrow().clone();
        Ok(async move {
            let venue = venue
                .and_then(|v| v.upgrade())
                .ok_or_else(|| Thrown::error("Error", "Kraken exchange is not assembled"))?;
            let ids = base.place_orders(venue.as_ref(), orders).await?;
            Ok(h(from_typed(&ids)?))
        })
    }
    fn cancel_orders(
        &self,
        orders: Vec<Handle<Self>>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, ()>>> {
        let list = Js::array(orders.iter().map(|o| (**o).clone()).collect());
        let promise = self.this.call_method("cancelOrders", vec![list])?;
        Ok(async move {
            await_js(&self.exchange_runtime(), promise).await?;
            Ok(())
        })
    }
    fn place_order(
        &self,
        options: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self
            .this
            .call_method("placeOrder", vec![(*options).clone()])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    fn finite_number(&self, value: &Js) -> bool {
        value.as_f64().is_some_and(f64::is_finite)
    }
    fn set_leverage(
        &self,
        symbol: Handle<Self>,
        leverage: Option<Handle<Self>>,
    ) -> ReadResult<Self, ()> {
        let map = cache_map(self, "leverage")?;
        match leverage {
            Some(v) => map.call_method("set", vec![(*symbol).clone(), (*v).clone()])?,
            None => map.call_method("delete", vec![(*symbol).clone()])?,
        };
        Ok(())
    }
    fn info(&self, message: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .call_method("log", vec![(*message).clone(), Js::str("info")])?;
        Ok(())
    }
    fn fetch_positions(
        &self,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let promise = self.this.call_method("fetchPositions", vec![])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), promise).await?)) })
    }
    // ts: `this.store.addOrUpdatePositions(positions)` — an upsert, not a replacement.
    fn add_positions(&self, positions: Handle<Self>) -> ReadResult<Self, ()> {
        self.this
            .get("store")?
            .call_method("addOrUpdatePositions", vec![(*positions).clone()])?;
        Ok(())
    }
}

struct ApiState {
    api: Rc<Api<Rt>>,
    success_errors: RefCell<Vec<Js>>,
}

impl ApiHost for Rt {
    // ts: safeCexAxiosCreate(config, options, exchange).
    fn create_client(
        &self,
        config: Handle<Self>,
        options: Handle<Self>,
        _exchange: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>> {
        Ok(h(client::factory::create(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            (*config).clone(),
            &options,
            self.base(),
        )?))
    }
    // ts: utils/retryReadsOnly networkErrorOnRead(isRead): reads (and the listed
    // posted reads) retried on axios-retry's isNetworkError only.
    fn retry_condition(&self, posted: Option<Vec<&'static str>>) -> ReadResult<Self, Handle<Self>> {
        let posted: Vec<String> = posted
            .unwrap_or_default()
            .into_iter()
            .map(str::to_owned)
            .collect();
        Ok(h(Js::function("retryCondition", move |_, a| {
            let error = arg(&a, 0);
            if !error.is_object() {
                return Ok(Js::Bool(false));
            }
            let config = error.get("config")?;
            if config.is_undefined() {
                return Ok(Js::Bool(false));
            }
            let method = config.get("method")?;
            let method = if method.is_nullish() {
                String::new()
            } else {
                lossy(&method.to_js_string()?).to_lowercase()
            };
            let url = config.get("url")?;
            let path = if url.is_nullish() {
                String::new()
            } else {
                lossy(&url.to_js_string()?)
                    .split('?')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            };
            let read = method == "get" || method == "head" || posted.contains(&path);
            let code = error.get("code")?;
            let network = error.get("response")?.is_nullish()
                && code.truthy()
                && code.to_display() != "ECONNABORTED";
            Ok(Js::Bool(read && network))
        })))
    }
    fn retry(&self, client: Handle<Self>, config: Handle<Self>) -> ReadResult<Self, ()> {
        client.call_method("__configureRetry", vec![(*config).clone()])?;
        Ok(())
    }
    fn spot_retry_condition(&self, posted: Vec<&'static str>) -> ReadResult<Self, Handle<Self>> {
        let read = self.retry_condition(Some(posted))?;
        let host = self.clone();
        Ok(h(Js::function("retryCondition", move |_, a| {
            use safe_cex_transport::signature_rejection::SIGNATURE_RETRY_CONFIG_KEY;
            let error = arg(&a, 0);
            if read.call(Js::Undefined, vec![error.clone()])?.truthy() {
                return Ok(Js::Bool(true));
            }
            let config = error.get("config")?;
            let response = error.get("response")?;
            if !config.truthy()
                || !response.truthy()
                || config.get(SIGNATURE_RETRY_CONFIG_KEY)?.truthy()
                || !api::is_kraken_spot_signature_rejection(&host, &response.get("data")?)?
            {
                return Ok(Js::Bool(false));
            }
            config.set(SIGNATURE_RETRY_CONFIG_KEY, Js::Bool(true))?;
            Ok(Js::Bool(true))
        })))
    }
    fn qs_parse(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        Ok(h(qs_parse_form(&value)?))
    }
    fn resend(
        &self,
        client: Handle<Self>,
        config: Handle<Self>,
    ) -> ReadResult<Self, impl Future<Output = ReadResult<Self, Handle<Self>>>> {
        let pending = client.call_method("request", vec![(*config).clone()])?;
        Ok(async move { Ok(h(await_js(&self.exchange_runtime(), pending).await?)) })
    }
    // ts: installTransportAdapter(xhr, options, exchange.name).
    fn install_transport(
        &self,
        client: Handle<Self>,
        options: Handle<Self>,
        name: Handle<Self>,
    ) -> ReadResult<Self, ()> {
        client::factory::install_transport_adapter(
            self.shared.http.clone(),
            self.shared.runtime.transport().as_ref(),
            &client,
            &options,
            super::value::to_typed(&name)?,
            safe_cex_transport::RestAdapterVariant::Default,
        )
    }
    fn request_use(&self, client: Handle<Self>, api: Rc<Api<Self>>) -> ReadResult<Self, ()> {
        let interceptor = Js::function("krakenRequest", move |_, a| {
            Ok((*api.request(h(arg(&a, 0)))?).clone())
        });
        client
            .get("interceptors")?
            .get("request")?
            .call_method("use", vec![interceptor])?;
        Ok(())
    }
    // Axios runs the rejected handler on transport failures only, never on a throw
    // from its own fulfilled handler: the client verbs are wrapped to reproduce that.
    fn response_use(&self, client: Handle<Self>, api: Rc<Api<Self>>) -> ReadResult<Self, ()> {
        let state = Rc::new(ApiState {
            api,
            success_errors: RefCell::default(),
        });
        let rt = self.clone();
        let fulfilled_state = state.clone();
        let fulfilled = Js::function("krakenResponse", move |_, a| {
            let state = fulfilled_state.clone();
            let response = arg(&a, 0);
            Ok(spawn_body(&rt, async move {
                match state.api.success(h(response)).await {
                    Ok(v) => Ok((*v).clone()),
                    Err(e) => {
                        state.success_errors.borrow_mut().push(e.value.clone());
                        Err(e)
                    }
                }
            }))
        });
        client
            .get("interceptors")?
            .get("response")?
            .call_method("use", vec![fulfilled])?;
        for verb in ["get", "post", "put", "delete", "request"] {
            let original = client.get(verb)?;
            let owner = (*client).clone();
            let state = state.clone();
            let rt = self.clone();
            client.define(
                verb,
                Js::function(verb, move |_, a| {
                    let promise = original.call(owner.clone(), a)?;
                    let state = state.clone();
                    Ok(spawn_body(&rt, {
                        let await_runtime = rt.exchange_runtime();
                        async move {
                            match await_js(&await_runtime, promise).await {
                                Ok(v) => Ok(v),
                                Err(e) => {
                                    let own = state
                                        .success_errors
                                        .borrow()
                                        .iter()
                                        .any(|x| x.same_reference(&e.value));
                                    if own {
                                        Err(e)
                                    } else {
                                        Ok((*state.api.rejected(e).await?).clone())
                                    }
                                }
                            }
                        }
                    }))
                }),
            )?;
        }
        Ok(())
    }
    fn strip_slash(&self, value: &Js) -> ReadResult<Self, Vec<u16>> {
        let s = value
            .as_str()
            .ok_or_else(|| Thrown::type_error("(config.url || \"\").replace is not a function"))?;
        Ok(if s.first() == Some(&47) {
            s[1..].to_vec()
        } else {
            s.to_vec()
        })
    }
    fn qs(&self, value: Handle<Self>, form: Form) -> ReadResult<Self, Handle<Self>> {
        let sorted = form == Form::Sorted1738;
        Ok(h(Js::str(&qs_stringify(&value, sorted, sorted)?)))
    }
    // ts: `encodeURIComponent(JSON.stringify(dataObj))`.
    fn json_component(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        let json = self
            .global("JSON.stringify")?
            .call(Js::Undefined, vec![(*value).clone()])?
            .to_js_string()?;
        Ok(h(Js::str(&encode_uri_component(&lossy(&json)))))
    }
    fn sha256(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        use sha2::Digest;
        Ok(h(buffer(&sha2::Sha256::digest(bytes_of(&value)?))))
    }
    // ts: `Buffer.from(options.secret, 'base64')`.
    fn decode_secret(&self, value: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        let Js::String(s) = &*value else {
            return Err(Thrown::type_error(
                "The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object.",
            ));
        };
        Ok(h(buffer(&node_base64(&lossy(s)))))
    }
    // ts: `Buffer.concat([Buffer.from(urlPath, 'utf8'), sha256Hash])`.
    fn path_hash(&self, path: Handle<Self>, hash: Handle<Self>) -> ReadResult<Self, Handle<Self>> {
        let mut bytes = lossy(&path.to_js_string()?).into_bytes();
        bytes.extend(bytes_of(&hash)?);
        Ok(h(buffer(&bytes)))
    }
    fn hmac512(
        &self,
        secret: Handle<Self>,
        payload: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>> {
        let mut mac = hmac::Hmac::<sha2::Sha512>::new_from_slice(&bytes_of(&secret)?)
            .map_err(|e| Thrown::type_error(&e.to_string()))?;
        mac.update(&bytes_of(&payload)?);
        Ok(h(Js::str(
            &base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
        )))
    }
    fn join(&self, value: &Js, separator: &str) -> ReadResult<Self, Handle<Self>> {
        Ok(h(value.call_method("join", vec![Js::str(separator)])?))
    }
    // ts: remoteSigningAdapter markSkipRemoteSigning.
    fn mark_skip(&self, config: Handle<Self>) -> ReadResult<Self, ()> {
        config.set("safeCexSkipRemoteSigning", Js::Bool(true))
    }
}

impl ConstructorHost for Rt {
    fn super_exchange(
        &self,
        name: Handle<Self>,
        options: Handle<Self>,
        store: Handle<Self>,
    ) -> ReadResult<Self, Handle<Self>> {
        let base = self.global("BaseExchange")?;
        Ok(h(base.construct(
            vec![(*name).clone(), (*options).clone(), (*store).clone()],
            None,
        )?))
    }
    fn secondary_client<'a>(
        &'a self,
        options: Handle<Self>,
        receiver: Handle<Self>,
        factory: Factory<'a, Self>,
        rate_limit: RateLimit<'a, Self>,
    ) -> ReadResult<Self, Handle<Self>> {
        let xhr = factory(options, receiver.clone(), BaseUrl::Derivatives)?;
        let xhr = rate_limit(xhr)?;
        receiver.set("_primaryXhr", (*xhr).clone())?;
        receiver.set("_secondaryXhr", (*xhr).clone())?;
        Ok(xhr)
    }
    fn create_api(
        &self,
        venue: Venue,
        options: Handle<Self>,
        receiver: Handle<Self>,
        base: BaseUrl,
    ) -> ReadResult<Self, Handle<Self>> {
        let host = Rc::new(self.with_this((*receiver).clone()));
        let api = match venue {
            Venue::Futures => {
                let kind = Js::str(match base {
                    BaseUrl::Derivatives => "derivatives",
                    BaseUrl::History => "history",
                });
                api::futures_create_api(
                    host,
                    options,
                    receiver,
                    &kind,
                    self.shared.extra.module.clone(),
                )?
            }
            Venue::Spot => api::spot_create_api(host, options, receiver)?,
        };
        Ok(api.client.clone())
    }
    // ts: axios-rate-limit `rateLimit(xhr, { maxRPS })`.
    fn rate_limit(&self, client: Handle<Self>, max_rps: f64) -> ReadResult<Self, Handle<Self>> {
        client.call_method(
            "__configureRateLimit",
            vec![Js::object(vec![("maxRPS", Js::Number(max_rps))])],
        )?;
        Ok(client)
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;

    #[test]
    fn spot_form_replays_match_the_actual_qs_dependency() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/spot_form_oracle.cjs");
        let output = std::process::Command::new("node")
            .arg(path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(cases.len(), 6, "replay input inventory");
        for case in cases {
            let parsed = qs_parse_form(&Js::str(case["input"].as_str().unwrap())).unwrap();
            parsed.delete(&units("nonce"));
            parsed.define("nonce", Js::str("next")).unwrap();
            assert_eq!(
                qs_stringify(&parsed, false, false).unwrap(),
                case["expected"].as_str().unwrap(),
                "{case}"
            );
        }
    }
}
