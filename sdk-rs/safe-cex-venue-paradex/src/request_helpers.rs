//! Small source helpers with explicit JavaScript runtime capability boundaries.

// ts: describeError
/// Bind the source JSON.stringify and String calls to the same error value.
/// JSON.stringify may return undefined; only a throw invokes String, whose
/// replacement error escapes unchanged. T preserves the host string model.
pub fn describe_error<T, E>(
    json_stringify: impl FnOnce() -> Result<Option<T>, E>,
    string: impl FnOnce() -> Result<T, E>,
) -> Result<Option<T>, E> {
    match json_stringify() {
        Ok(value) => Ok(value),
        Err(_) => string().map(Some),
    }
}

// ts: signerHeaders
#[derive(Debug)]
pub struct SignerRequestConfig<'a, T> {
    pub headers: &'a T,
}

// ts: signerHeaders
/// The declared headers object is truthy even when empty. Borrow it unchanged:
/// the source creates the config/array but does not clone the supplied object.
pub fn signer_headers<T>(headers: Option<&T>) -> Vec<SignerRequestConfig<'_, T>> {
    match headers {
        Some(headers) => vec![SignerRequestConfig { headers }],
        None => Vec::new(),
    }
}
