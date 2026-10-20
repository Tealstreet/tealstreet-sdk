//! Percent-encoding dialects venues sign, and one query builder.
//!
//! Signatures cover the exact request bytes, so each dialect is named after
//! the encoder whose output a venue signs; they differ only in which ASCII
//! punctuation stays literal and how a space is written.
use crate::num;

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// RFC 3986: only unreserved characters (`A-Z a-z 0-9 - _ . ~`) stay literal.
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    encode_into(&mut out, value);
    out
}

pub fn encode_into(out: &mut String, value: &str) {
    escape(
        out,
        value,
        |b| matches!(b, b'-' | b'_' | b'.' | b'~'),
        false,
    );
}

/// JavaScript `encodeURIComponent`: RFC 3986 plus `! ' ( ) *` literal.
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    escape(&mut out, value, component, false);
    out
}

/// `URLSearchParams` (WHATWG form encoding): `* - . _` literal, space as `+`.
pub fn encode_form(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    escape(
        &mut out,
        value,
        |b| matches!(b, b'*' | b'-' | b'.' | b'_'),
        true,
    );
    out
}

/// The TS HTTP client's parameter spelling: `encodeURIComponent`, then
/// `: $ ,` restored and space written as `+`.
pub fn encode_http_client_into(out: &mut String, value: &str) {
    escape(
        out,
        value,
        |b| component(b) || matches!(b, b':' | b'$' | b','),
        true,
    );
}

fn component(b: u8) -> bool {
    matches!(
        b,
        b'-' | b'_' | b'.' | b'~' | b'!' | b'\'' | b'(' | b')' | b'*'
    )
}

fn escape(out: &mut String, value: &str, literal: impl Fn(u8) -> bool, plus: bool) {
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || literal(b) {
            out.push(char::from(b));
        } else if plus && b == b' ' {
            out.push('+');
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(b >> 4)]));
            out.push(char::from(HEX[usize::from(b & 15)]));
        }
    }
}

/// A query string built in call order: names are written as given, values
/// RFC 3986 encoded.
#[derive(Clone, Debug, Default)]
pub struct Query(String);

impl Query {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn str(mut self, name: &str, value: &str) -> Self {
        if !self.0.is_empty() {
            self.0.push('&');
        }
        self.0.push_str(name);
        self.0.push('=');
        encode_into(&mut self.0, value);
        self
    }

    pub fn num(self, name: &str, value: f64) -> Self {
        self.str(name, &num::format(value))
    }

    pub fn opt_str(self, name: &str, value: Option<&str>) -> Self {
        match value {
            Some(value) => self.str(name, value),
            None => self,
        }
    }

    pub fn opt_num(self, name: &str, value: Option<f64>) -> Self {
        match value {
            Some(value) => self.num(name, value),
            None => self,
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "a Z-_.~!'()*:$,;@/?&=+[]\"%é";

    #[test]
    fn dialects_differ_only_where_their_encoders_do() {
        assert_eq!(
            encode(SAMPLE),
            "a%20Z-_.~%21%27%28%29%2A%3A%24%2C%3B%40%2F%3F%26%3D%2B%5B%5D%22%25%C3%A9"
        );
        assert_eq!(
            encode_component(SAMPLE),
            "a%20Z-_.~!'()*%3A%24%2C%3B%40%2F%3F%26%3D%2B%5B%5D%22%25%C3%A9"
        );
        assert_eq!(
            encode_form(SAMPLE),
            "a+Z-_.%7E%21%27%28%29*%3A%24%2C%3B%40%2F%3F%26%3D%2B%5B%5D%22%25%C3%A9"
        );
        let mut client = String::new();
        encode_http_client_into(&mut client, SAMPLE);
        assert_eq!(
            client,
            "a+Z-_.~!'()*:$,%3B%40%2F%3F%26%3D%2B%5B%5D%22%25%C3%A9"
        );
    }

    #[test]
    fn rfc3986_escapes_utf8_bytes_once() {
        assert_eq!(encode("AZaz09-._~"), "AZaz09-._~");
        assert_eq!(encode("default:00002"), "default%3A00002");
        assert_eq!(
            encode(" /?#[]@!$&'()*+,;=%\\\r\n\0"),
            "%20%2F%3F%23%5B%5D%40%21%24%26%27%28%29%2A%2B%2C%3B%3D%25%5C%0D%0A%00"
        );
        assert_eq!(encode("café/東京"), "caf%C3%A9%2F%E6%9D%B1%E4%BA%AC");
        assert_eq!(encode("%3A"), "%253A");
        assert_eq!(encode(""), "");
    }

    #[test]
    fn query_keeps_call_order_and_encodes_values() {
        let query = Query::new()
            .str("symbol", "BTC-USDT")
            .num("price", 0.000_000_1)
            .opt_str("clientOrderId", Some("a b"))
            .opt_num("stopPrice", None);
        assert_eq!(
            query.as_str(),
            "symbol=BTC-USDT&price=0.0000001&clientOrderId=a%20b"
        );
        assert_eq!(Query::new().into_string(), "");
    }
}
