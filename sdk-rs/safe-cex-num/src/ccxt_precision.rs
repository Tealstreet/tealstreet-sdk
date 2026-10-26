use super::{PrecisionInput, js_number_string, parse_integer};

fn string(value: PrecisionInput) -> String {
    match value {
        PrecisionInput::Number(n) => js_number_string(n),
        PrecisionInput::String(s) => s,
    }
}

pub fn integer_precision_to_amount(value: Option<PrecisionInput>) -> Option<String> {
    let number = match value? {
        PrecisionInput::Number(n) => n,
        PrecisionInput::String(s) => parse_integer(&s, 0),
    };
    Some(if number == 0.0 {
        "1".into()
    } else {
        js_number_string(1.0 / number)
    })
}

pub fn count_decimals(value: Option<PrecisionInput>) -> f64 {
    let Some(value) = value else { return 0.0 };
    let text = string(value);
    if let Some(index) = text.find("e-").into_iter().chain(text.find("E-")).min() {
        let mantissa = &text[..index];
        let exponent = &text[index + 2..];
        return mantissa
            .split('.')
            .nth(1)
            .unwrap_or("")
            .encode_utf16()
            .count() as f64
            + parse_integer(exponent, 0);
    }
    text.split('.').nth(1).map_or(0.0, |s| {
        s.trim_end_matches('0').encode_utf16().count() as f64
    })
}

pub fn decimal_to_precision(value: Option<PrecisionInput>) -> f64 {
    let Some(value) = value else { return 0.0 };
    let text = string(value);
    let text_trimmed = text.trim_start_matches(super::js_whitespace);
    let parsed = (1..=text_trimmed.len())
        .rev()
        .filter(|n| text_trimmed.is_char_boundary(*n))
        .find_map(|n| text_trimmed[..n].parse::<f64>().ok());
    if parsed == Some(1.0) {
        0.0
    } else {
        count_decimals(Some(text.into()))
    }
}

pub fn parse_precision(value: Option<PrecisionInput>, is_integer: bool) -> f64 {
    if is_integer {
        decimal_to_precision(integer_precision_to_amount(value).map(Into::into))
    } else {
        decimal_to_precision(value)
    }
}

pub fn precision_to_string(precision: f64) -> String {
    if precision == 0.0 {
        return "1".into();
    }
    format!(
        "0.{}1",
        "0".repeat((precision - 1.0).ceil().max(0.0) as usize)
    )
}
