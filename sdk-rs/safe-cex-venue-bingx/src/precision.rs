//! BingX's decimal precision-string builder, before its callers' parseFloat.
use safe_cex_num::{JsPrimitive, js_number_string, parse_int};

/// None rejects inputs outside the source's string | number declaration.
pub fn parse_precision(precision: &JsPrimitive) -> Option<String> {
    let text = match precision {
        JsPrimitive::String(text) => text.clone(),
        JsPrimitive::Number(number) => js_number_string(*number),
        _ => return None,
    };
    let precision_number = parse_int(&text, None);
    let mut parsed_precision = String::from("0.");
    let mut i = 0.0;
    while i < precision_number - 1.0 {
        parsed_precision.push('0');
        i += 1.0;
    }
    parsed_precision.push('1');
    Some(parsed_precision)
}
