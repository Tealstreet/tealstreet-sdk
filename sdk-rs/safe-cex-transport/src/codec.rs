use crate::native_data::{property, text};
use crate::{TransportError, frames::ResponseData, socket::SocketFrame};
use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use flate2::bufread::GzDecoder;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use std::io::Read;

pub fn to_array_buffer(bytes: &[u8]) -> Vec<u8> {
    bytes.to_vec()
}

pub fn decode_base64(encoded: &str) -> Result<Vec<u8>, TransportError> {
    let mut encoded: Vec<_> = encoded
        .bytes()
        .filter(|byte| !matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
        .collect();
    if encoded.len().is_multiple_of(4) {
        for _ in 0..2 {
            if encoded.last() == Some(&b'=') {
                encoded.pop();
            }
        }
    }
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::RequireNone)
            .with_decode_allow_trailing_bits(true),
    );
    engine.decode(encoded).map_err(decode_error)
}

pub fn decode_trade_tunnel_message(
    frame: SocketFrame,
) -> Result<(Input, Option<Vec<u8>>), TransportError> {
    match frame {
        SocketFrame::Close { .. } => Err(decode_error("close event has no message payload")),
        SocketFrame::Text(text) => Ok((crate::how::json_parse(&text), None)),
        SocketFrame::Binary(bytes) => {
            if bytes.len() < 8 {
                return Err(decode_error(
                    "TradeTunnel binary frame is shorter than its prefix",
                ));
            }
            if &bytes[..4] != b"TSB1" {
                return Err(decode_error(
                    "TradeTunnel binary frame has an invalid magic value",
                ));
            }
            let length = u32::from_be_bytes(bytes[4..8].try_into().expect("four bytes")) as usize;
            if length == 0 || length > 1024 * 1024 {
                return Err(decode_error(
                    "TradeTunnel binary frame has an invalid header length",
                ));
            }
            let offset = 8 + length;
            if offset >= bytes.len() {
                return Err(decode_error(
                    "TradeTunnel binary frame is missing its payload",
                ));
            }
            let header = std::str::from_utf8(&bytes[8..offset]).map_err(decode_error)?;
            let header = header.strip_prefix('\u{feff}').unwrap_or(header);
            let value = crate::how::json_parse(header);
            if property(&value, "type").and_then(text).as_deref() != Some("rpc.http")
                || !matches!(property(&value, "nonce"), Some(Input::Text(_)))
                || property(&value, "data")
                    .and_then(|data| property(data, "data_encoding"))
                    .and_then(text)
                    .as_deref()
                    != Some("gzip")
            {
                return Err(decode_error(
                    "TradeTunnel binary frame has an invalid response header",
                ));
            }
            Ok((value, Some(to_array_buffer(&bytes[offset..]))))
        }
    }
}

pub(crate) fn decode_raw_body(
    data: &ResponseData,
    binary: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>, TransportError> {
    let compressed = match data.data_encoding.as_deref() {
        Some("gzip") => binary,
        Some("gzip+base64") => data
            .data_encoded
            .as_ref()
            .map(|encoded| decode_base64(encoded))
            .transpose()?,
        _ => None,
    };
    if let Some(compressed) = compressed {
        let mut body = Vec::new();
        let mut decoder = GzDecoder::new(compressed.as_slice());
        decoder.read_to_end(&mut body).map_err(decode_error)?;
        if !decoder.into_inner().is_empty() {
            return Err(decode_error(
                "Trailing junk found after the end of the compressed stream",
            ));
        }
        // TextDecoder replaces malformed UTF-8 and consumes only the initial BOM.
        let decoded = String::from_utf8_lossy(&body);
        return Ok(Some(
            decoded
                .strip_prefix('\u{feff}')
                .unwrap_or(&decoded)
                .as_bytes()
                .to_vec(),
        ));
    }
    Ok(data.data_raw.as_ref().map(|raw| raw.as_bytes().to_vec()))
}

fn decode_error(error: impl std::fmt::Display) -> TransportError {
    TransportError::Host(format!("TradeTunnelResponseDecodeError: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::json;
    use std::io::Write;

    fn metadata(value: Input) -> crate::frames::RpcHttpResponse {
        // Closed finite/raw-text protocol fixture; native payload delivery is
        // covered separately through actual HOW adapters in tests/tunnel.rs.
        let mut value = value.into_value().unwrap();
        crate::how::normalize_json_numbers(&mut value, false, false);
        serde_json::from_value(value).unwrap()
    }
    fn gzip(raw: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(raw).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn frame_numbers_collapse_through_binary64_before_typed_response_decode() {
        let header = r#"{"type":"rpc.http","nonce":"n","data":{"config":{"url":"/"},"status":200.0,"status_text":"OK","headers":{},"data_encoding":"gzip"},"numbers":[9007199254740993,1.50,1e3]}"#;
        for frame in [
            SocketFrame::Text(header.into()),
            SocketFrame::Binary(envelope(header.as_bytes(), b"x")),
        ] {
            let (value, _) = decode_trade_tunnel_message(frame).unwrap();
            assert_eq!(
                crate::native_data::wire_json(property(&value, "numbers").unwrap())
                    .unwrap()
                    .to_string(),
                "[9007199254740992,1.5,1000]"
            );
            let response: crate::frames::RpcHttpResponse = metadata(value);
            assert_eq!(response.data.status, 200);
        }
    }

    #[test]
    fn owned_binary_views_and_forgiving_base64_match_browser_inputs() {
        let mut storage = [9, 1, 2, 3, 8];
        let buffer = to_array_buffer(&storage[1..4]);
        storage[2] = 77;
        assert_eq!(storage[2], 77);
        assert_eq!(buffer, [1, 2, 3]);
        for input in ["Zg==", "Zg", "Zh==", " Z\tg\n==\r"] {
            assert_eq!(decode_base64(input).unwrap(), b"f");
        }
        for input in ["Z", "Z===", "Zg=", "Zg_", "Zg\u{b}=="] {
            assert!(decode_base64(input).is_err());
        }
        assert!(decode_base64("").unwrap().is_empty());
    }

    #[test]
    fn binary_base64_and_legacy_raw_preserve_exact_integer_text() {
        let raw = br#"{"id":9007199254740993}"#;
        let compressed = gzip(raw);
        for encoding in ["gzip", "gzip+base64", "raw"] {
            let header = json!({"type":"rpc.http","nonce":"test", "data":{
                "config":{"url":"/test"},"status":200,"status_text":"OK","headers":{},
                "data_encoding":encoding,
                "data_raw": if encoding == "raw" { Some(std::str::from_utf8(raw).unwrap()) } else { None },
                "data_encoded":if encoding == "gzip+base64" { Some(STANDARD.encode(&compressed)) } else { None }
            }});
            let frame = if encoding == "gzip" {
                let header = header.to_string().into_bytes();
                let mut bytes = b"TSB1".to_vec();
                bytes.extend_from_slice(&(header.len() as u32).to_be_bytes());
                bytes.extend(header);
                bytes.extend(&compressed);
                SocketFrame::Binary(bytes)
            } else {
                SocketFrame::Text(header.to_string())
            };
            let (value, binary) = decode_trade_tunnel_message(frame).unwrap();
            let response: crate::frames::RpcHttpResponse = metadata(value);
            assert_eq!(
                decode_raw_body(&response.data, binary).unwrap().unwrap(),
                raw
            );
        }
    }

    fn envelope(header: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut bytes = b"TSB1".to_vec();
        bytes.extend_from_slice(&(header.len() as u32).to_be_bytes());
        bytes.extend_from_slice(header);
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn binary_envelope_guards_report_the_specific_invalid_component() {
        let valid = br#"{"type":"rpc.http","nonce":"n","data":{"data_encoding":"gzip"}}"#;
        let mut bad_magic = envelope(valid, b"payload");
        bad_magic[..4].copy_from_slice(b"BAD!");
        let mut zero_length = envelope(valid, b"payload");
        zero_length[4..8].copy_from_slice(&0u32.to_be_bytes());
        let mut oversized_header = valid.to_vec();
        oversized_header.resize(1024 * 1024 + 1, b' ');
        for (bytes, expected) in [
            (b"TSB1".to_vec(), "shorter than its prefix"),
            (b"TSB1\0\0\0".to_vec(), "shorter than its prefix"),
            (bad_magic, "invalid magic value"),
            (zero_length, "invalid header length"),
            (
                envelope(&oversized_header, b"payload"),
                "invalid header length",
            ),
            (envelope(valid, b""), "missing its payload"),
            (
                envelope(
                    br#"{"type":"other","nonce":"n","data":{"data_encoding":"gzip"}}"#,
                    b"payload",
                ),
                "invalid response header",
            ),
            (
                envelope(
                    br#"{"type":"rpc.http","nonce":1,"data":{"data_encoding":"gzip"}}"#,
                    b"payload",
                ),
                "invalid response header",
            ),
            (
                envelope(
                    br#"{"type":"rpc.http","nonce":"n","data":{"data_encoding":"raw"}}"#,
                    b"payload",
                ),
                "invalid response header",
            ),
            (
                envelope(br#"{"type":"rpc.http","nonce":"n"}"#, b"payload"),
                "invalid response header",
            ),
            (
                envelope(
                    br#"{"type":"rpc.http","nonce":"n","data":null}"#,
                    b"payload",
                ),
                "invalid response header",
            ),
            (
                envelope(
                    br#"{"type":"rpc.http","nonce":"n","data":false}"#,
                    b"payload",
                ),
                "invalid response header",
            ),
            (envelope(b"\xff", b"payload"), "invalid utf-8"),
            (envelope(b"{", b"payload"), "invalid response header"),
        ] {
            let error = decode_trade_tunnel_message(SocketFrame::Binary(bytes)).unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "expected {expected}: {error}"
            );
        }
        let (header, payload) =
            decode_trade_tunnel_message(SocketFrame::Binary(envelope(valid, b"payload"))).unwrap();
        assert_eq!(
            property(&header, "nonce").and_then(text).as_deref(),
            Some("n")
        );
        assert_eq!(payload.unwrap(), b"payload");
        let mut maximum_header = valid.to_vec();
        maximum_header.resize(1024 * 1024, b' ');
        assert!(
            decode_trade_tunnel_message(SocketFrame::Binary(envelope(&maximum_header, b"payload")))
                .is_ok()
        );
    }

    #[test]
    fn browser_decoder_strips_only_initial_bom_and_rejects_trailing_gzip_bytes() {
        let data: ResponseData = serde_json::from_value(json!({"config":{"url":"/test"},"status":200,"status_text":"OK","headers":{},"data_encoding":"gzip"})).unwrap();
        let raw = "\u{feff}{\"text\":\"\u{feff}\"}";
        assert_eq!(
            decode_raw_body(&data, Some(gzip(raw.as_bytes())))
                .unwrap()
                .unwrap(),
            "{\"text\":\"\u{feff}\"}".as_bytes()
        );
        for suffix in [vec![0], vec![b'x'], gzip(b"[]")] {
            let mut compressed = gzip(b"{}");
            compressed.extend(suffix);
            assert!(decode_raw_body(&data, Some(compressed)).is_err());
        }
        let header =
            "\u{feff}{\"type\":\"rpc.http\",\"nonce\":\"n\",\"data\":{\"data_encoding\":\"gzip\"}}";
        assert!(
            decode_trade_tunnel_message(SocketFrame::Binary(envelope(header.as_bytes(), b"x")))
                .is_ok()
        );
        assert_eq!(
            decode_trade_tunnel_message(SocketFrame::Text("not json".into())).unwrap(),
            (Input::Null, None)
        );
    }

    #[test]
    fn rejects_corrupt_gzip_payload() {
        let data: ResponseData = serde_json::from_value(json!({"config":{"url":"/test"},"status":200,"status_text":"OK","headers":{},"data_encoding":"gzip+base64","data_encoded":"bm90LWd6aXA="})).unwrap();
        assert!(decode_raw_body(&data, None).is_err());
    }
}
