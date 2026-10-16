//! Complete formatCryptocomApiError body at native property/record/template boundaries.
//! isRecord/stringifyResponseBody and JS operators remain explicit dependencies.
pub trait ApiErrorHost {
    type Value: Clone;
    type Error;
    fn undefined_value(&self) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn undefined(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn zero_number(&self, value: &Self::Value) -> bool;
    fn primitive_string(&self, value: &Self::Value) -> bool;
    fn get(&self, value: &Self::Value, key: &'static str) -> Result<Self::Value, Self::Error>;
    fn is_record(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn stringify_response_body(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn template(&self, value: Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn at_least_500(&self, value: &Self::Value) -> Result<bool, Self::Error>;
    fn string_value(&self, units: Vec<u16>) -> Self::Value;
}
fn optional_get<H: ApiErrorHost>(
    h: &H,
    value: H::Value,
    key: &'static str,
) -> Result<H::Value, H::Error> {
    if h.nullish(&value) {
        Ok(h.undefined_value())
    } else {
        h.get(&value, key)
    }
}
fn append(text: &mut Vec<u16>, literal: &str) {
    text.extend(literal.encode_utf16());
}
fn http_message<H: ApiErrorHost>(
    h: &H,
    prefix: &str,
    status: H::Value,
    body: H::Value,
) -> Result<H::Value, H::Error> {
    let mut message = prefix.encode_utf16().collect::<Vec<_>>();
    message.extend(h.template(status)?);
    append(&mut message, "): ");
    message.extend(h.template(body)?);
    Ok(h.string_value(message))
}
pub fn format_cryptocom_api_error<H: ApiErrorHost>(
    h: &H,
    error: H::Value,
) -> Result<H::Value, H::Error> {
    let response = optional_get(h, error.clone(), "response")?;
    let status = optional_get(h, response, "status")?;
    let response = optional_get(h, error, "response")?;
    let body = optional_get(h, response, "data")?;
    if h.undefined(&body) {
        return Ok(h.undefined_value());
    }
    if h.is_record(&body)? {
        let code = h.get(&body, "code")?;
        let message_test = h.get(&body, "message")?;
        let message = if h.primitive_string(&message_test) {
            h.get(&body, "message")?
        } else {
            h.undefined_value()
        };
        if !h.undefined(&code) || h.truthy(&message) {
            let mut text = "Crypto.com API Error".encode_utf16().collect::<Vec<_>>();
            if !h.undefined(&code) && !h.zero_number(&code) {
                append(&mut text, " ");
                text.extend(h.template(code)?);
            }
            if h.truthy(&message) {
                append(&mut text, ": ");
                text.extend(h.template(message)?);
            }
            return Ok(h.string_value(text));
        }
    }
    let body_string = h.stringify_response_body(body)?;
    if h.truthy(&status) && h.at_least_500(&status)? {
        return http_message(h, "Crypto.com proxy/transport error (", status, body_string);
    }
    if h.truthy(&status) {
        return http_message(h, "Crypto.com HTTP error (", status, body_string);
    }
    let mut message = "Crypto.com API error: ".encode_utf16().collect::<Vec<_>>();
    message.extend(h.template(body_string)?);
    Ok(h.string_value(message))
}
