//! Trusted decoding only; no engine/venue receives the decoded key.
use super::Secret;
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_engine::signing::SignError;
use zeroize::Zeroizing;

const LIMIT: usize = 1024 * 1024;

pub use safe_cex_engine::signing::KeyEncoding;

fn base64(input: &[u8]) -> Result<Zeroizing<Vec<u8>>, SignError> {
    // Decode into an initialized guarded buffer: partial error output is wiped.
    let mut output = Zeroizing::new(vec![0; input.len().div_ceil(4) * 3]);
    let length = STANDARD
        .decode_slice(input, &mut output)
        .map_err(|_| SignError::InvalidRequest)?;
    output.truncate(length);
    Ok(output)
}

pub(super) fn decode(
    input: Secret,
    encoding: KeyEncoding,
) -> Result<Zeroizing<Vec<u8>>, SignError> {
    let input = Zeroizing::new(input.into_bytes());
    if input.len() > LIMIT {
        return Err(SignError::InvalidRequest);
    }
    match encoding {
        KeyEncoding::Utf8 => Ok(input),
        KeyEncoding::Base64 => base64(&input),
        KeyEncoding::Base64Text => {
            // Encode into one initialized guarded buffer; nothing reallocates.
            let mut output = Zeroizing::new(vec![0; input.len().div_ceil(3) * 4]);
            let length = STANDARD
                .encode_slice(&*input, &mut output)
                .map_err(|_| SignError::InvalidRequest)?;
            output.truncate(length);
            Ok(output)
        }
        KeyEncoding::Hex => {
            // Wallet keys are commonly pasted 0x-prefixed; the prefix is optional.
            let input = match input.as_slice() {
                [b'0', b'x' | b'X', body @ ..] => body,
                body => body,
            };
            if !input.len().is_multiple_of(2) {
                return Err(SignError::InvalidRequest);
            }
            let mut output = Zeroizing::new(Vec::with_capacity(input.len() / 2));
            let digit = |byte: u8| match byte {
                b'0'..=b'9' => Ok(byte - b'0'),
                b'a'..=b'f' => Ok(byte - b'a' + 10),
                b'A'..=b'F' => Ok(byte - b'A' + 10),
                _ => Err(SignError::InvalidRequest),
            };
            for pair in input.as_chunks::<2>().0 {
                output.push(digit(pair[0])? * 16 + digit(pair[1])?);
            }
            Ok(output)
        }
        KeyEncoding::Pem => {
            let text = std::str::from_utf8(&input)
                .map_err(|_| SignError::InvalidRequest)?
                .trim();
            let mut lines = text.lines();
            let label = lines
                .next()
                .and_then(|line| line.strip_prefix("-----BEGIN "))
                .and_then(|line| line.strip_suffix("-----"))
                .filter(|label| ["PRIVATE KEY", "EC PRIVATE KEY"].contains(label))
                .ok_or(SignError::InvalidRequest)?;
            let closing = format!("-----END {label}-----");
            let mut encoded = Zeroizing::new(String::with_capacity(text.len()));
            let mut closed = false;
            for line in lines {
                if line == closing {
                    if closed {
                        return Err(SignError::InvalidRequest);
                    }
                    closed = true;
                } else {
                    if closed || line.starts_with("-----") {
                        return Err(SignError::InvalidRequest);
                    }
                    encoded.push_str(line.trim());
                }
            }
            if !closed || encoded.is_empty() {
                return Err(SignError::InvalidRequest);
            }
            base64(encoded.as_bytes())
        }
    }
}
