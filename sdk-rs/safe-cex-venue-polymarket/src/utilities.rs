//! Venue wire utilities; domain normalization and payoff belong to prediction-core.
use safe_cex_num::{PrecisionInput, js_number_string};

/// Mirror String(value ?? '') and exact supported-tick membership.
/// None represents the source's null; strings are deliberately not trimmed or parsed.
pub fn normalize_tick_size(value: Option<&PrecisionInput>) -> Result<String, String> {
    let tick_size = match value {
        None => String::new(),
        Some(PrecisionInput::String(value)) => value.clone(),
        Some(PrecisionInput::Number(value)) => js_number_string(*value),
    };
    if !["0.1", "0.01", "0.005", "0.0025", "0.001", "0.0001"].contains(&tick_size.as_str()) {
        return Err(format!(
            "Unsupported Polymarket tick size: {}",
            if tick_size.is_empty() {
                "missing"
            } else {
                &tick_size
            }
        ));
    }
    Ok(tick_size)
}

// Decode only top-level string elements. Ignored containers are consumed without
// recursively materializing them; their content never reaches the source flatMap.
struct ArrayString(Option<String>);
impl<'de> serde::Deserialize<'de> for ArrayString {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = ArrayString;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON array element")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(ArrayString(Some(value.to_owned())))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(ArrayString(Some(value)))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(ArrayString(None))
            }
            fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(ArrayString(None))
            }
            fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(ArrayString(None))
            }
            fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(ArrayString(None))
            }
            fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
                Ok(ArrayString(None))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(ArrayString(None))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                while map
                    .next_entry::<serde::de::IgnoredAny, serde::de::IgnoredAny>()?
                    .is_some()
                {}
                Ok(ArrayString(None))
            }
        }
        decoder.deserialize_any(Visitor)
    }
}

/// Extract the two declared nullable Gamma JSON-string fields, preserving paired order.
/// Tuple entries correspond to the source's { tokenId, outcome }, without domain mapping.
pub fn parse_gamma_market_tokens(
    token_ids: Option<&str>,
    outcomes: Option<&str>,
) -> Vec<(String, String)> {
    let (Some(token_ids), Some(outcomes)) = (token_ids, outcomes) else {
        return Vec::new();
    };
    if token_ids.is_empty() || outcomes.is_empty() {
        return Vec::new();
    }
    let (Ok(token_ids), Ok(outcomes)) = (
        serde_json::from_str::<Vec<ArrayString>>(token_ids),
        serde_json::from_str::<Vec<ArrayString>>(outcomes),
    ) else {
        return Vec::new();
    };
    if token_ids.len() != outcomes.len() {
        return Vec::new();
    }
    token_ids
        .into_iter()
        .zip(outcomes)
        .filter_map(|(token_id, outcome)| match (token_id.0, outcome.0) {
            (Some(token_id), Some(outcome)) => Some((token_id, outcome)),
            _ => None,
        })
        .collect()
}

/// Count the source's fixed-eight-decimal suffix, including its exponent fallback.
pub fn precision_from_tick_size(value: Option<&PrecisionInput>) -> f64 {
    let tick = match value {
        None => 0.0,
        Some(PrecisionInput::Number(value)) => *value,
        Some(PrecisionInput::String(value)) => safe_cex_num::number_from_string(value),
    };
    if !tick.is_finite() || tick <= 0.0 {
        return 2.0;
    }
    let text = if tick >= 1e21 {
        js_number_string(tick)
    } else {
        // Only the trailing-zero count is observed, not the rounded digit.
        // Exact binary ties at eight decimals have scaled floor ending 2 or 7:
        // 2*k+1 = 5^8*m, m odd. Half-even versus half-up cannot add/remove zeros.
        format!("{tick:.8}")
    };
    let text = text.trim_end_matches('0');
    text.find('.')
        .map_or(text.len(), |dot| text.len() - dot - 1) as f64
}
