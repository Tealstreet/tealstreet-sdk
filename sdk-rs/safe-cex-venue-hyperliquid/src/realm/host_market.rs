//! Scalar-market and fast-asset-context host primitives for the imported
//! market helpers (price precision, volume/OI parsing, fastAssetCtxs decode).
use super::{Realm, ccxt, value::Value};
use crate::{
    market_helpers::{FastAssetCtxHost, MarketScalarHost, PreciseComparison},
    spot_json::JsString,
};
use std::cmp::Ordering;

fn units(text: &str) -> JsString {
    JsString(text.encode_utf16().collect())
}

/// Exact comparison of plain decimal strings (ccxt Precise.stringEq/Gt/Lt).
pub(crate) fn compare_decimal(a: &str, b: &str) -> Option<Ordering> {
    fn parts(s: &str) -> Option<(bool, String, String)> {
        let s = s.trim();
        let (neg, s) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int, frac) = s.split_once('.').unwrap_or((s, ""));
        if int.is_empty() && frac.is_empty()
            || !int.bytes().chain(frac.bytes()).all(|c| c.is_ascii_digit())
        {
            return None;
        }
        let int = int.trim_start_matches('0').to_owned();
        let frac = frac.trim_end_matches('0').to_owned();
        let zero = int.is_empty() && frac.is_empty();
        Some((neg && !zero, int, frac))
    }
    let (an, ai, af) = parts(a)?;
    let (bn, bi, bf) = parts(b)?;
    let magnitude = ai
        .len()
        .cmp(&bi.len())
        .then_with(|| ai.cmp(&bi))
        .then_with(|| {
            let width = af.len().max(bf.len());
            format!("{af:0<width$}").cmp(&format!("{bf:0<width$}"))
        });
    Some(match (an, bn) {
        (false, false) => magnitude,
        (true, true) => magnitude.reverse(),
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
    })
}

impl MarketScalarHost for Realm {
    fn number_value(&self, value: &Value) -> Option<f64> {
        value.as_number()
    }
    fn value_by_key(&mut self, value: &Value, key: &str) -> Result<Value, Value> {
        let v = self.import("v")?;
        Realm::call(
            self,
            &v,
            &Value::Undefined,
            &[value.clone(), Value::str(key)],
        )
    }
    fn parse_float(&mut self, text: &[u16]) -> Result<f64, Value> {
        Ok(safe_cex_num::parse_float(&String::from_utf16_lossy(text)))
    }
    fn lowercase(&mut self, text: &[u16]) -> Result<JsString, Value> {
        Ok(units(&String::from_utf16_lossy(text).to_lowercase()))
    }
    fn number_to_string(&mut self, number: f64) -> Result<Option<JsString>, Value> {
        Ok(Some(units(&ccxt::number_to_string(number))))
    }
    fn precise_compare(
        &mut self,
        kind: PreciseComparison,
        left: &[u16],
        right: &str,
    ) -> Result<bool, Value> {
        let left = String::from_utf16_lossy(left);
        let order = compare_decimal(&left, right)
            .ok_or_else(|| self.error(&format!("Precise: invalid number {left}")))?;
        Ok(match kind {
            PreciseComparison::Equal => order == Ordering::Equal,
            PreciseComparison::Greater => order == Ordering::Greater,
            PreciseComparison::Less => order == Ordering::Less,
        })
    }
    /// ccxt parseToInt: parseInt(parseFloat(numberToString(n))).
    fn parse_to_int(&mut self, number: f64) -> Result<f64, Value> {
        let float = safe_cex_num::parse_float(&ccxt::number_to_string(number));
        Ok(safe_cex_num::parse_int(
            &safe_cex_num::js_number_string(float),
            None,
        ))
    }
}

impl FastAssetCtxHost for Realm {
    fn atob(&mut self, text: &[u16]) -> Result<JsString, Value> {
        use base64::{
            Engine, alphabet,
            engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
        };
        // atob ignores ASCII whitespace and accepts missing padding.
        const ENGINE: GeneralPurpose = GeneralPurpose::new(
            &alphabet::STANDARD,
            GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
        );
        let cleaned: String = String::from_utf16_lossy(text)
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        let bytes = ENGINE.decode(cleaned).map_err(|_| {
            self.error_of(
                "InvalidCharacterError",
                "The string to be decoded is not correctly encoded.",
            )
        })?;
        // atob yields a binary string: one code unit per byte.
        Ok(JsString(bytes.into_iter().map(u16::from).collect()))
    }
    fn inflate_raw(&mut self, compressed: &[u8]) -> Result<Vec<u8>, Value> {
        use std::io::Read;
        let mut out = vec![];
        flate2::read::DeflateDecoder::new(compressed)
            .read_to_end(&mut out)
            .map_err(|e| self.error(&format!("invalid raw deflate data: {e}")))?;
        Ok(out)
    }
    fn decode_text(&mut self, bytes: &[u8]) -> Result<JsString, Value> {
        let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
        Ok(units(&String::from_utf8_lossy(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::compare_decimal;
    use std::cmp::Ordering::*;
    #[test]
    fn decimal_comparison_is_exact() {
        assert_eq!(compare_decimal("0.10", "0.1"), Some(Equal));
        assert_eq!(compare_decimal("-0", "0"), Some(Equal));
        assert_eq!(compare_decimal("0.0000001", "0"), Some(Greater));
        assert_eq!(compare_decimal("12.5", "9.99"), Some(Greater));
        assert_eq!(compare_decimal("-2", "-10"), Some(Greater));
        assert_eq!(compare_decimal("0.999999999999999999", "1"), Some(Less));
    }
}
