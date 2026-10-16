//! Blob.arrayBuffer adapter, wrapped inflate and pako's byte-to-character projection.
use std::{future::Future, io::Read};
pub async fn read_blob(
    buffer: impl Future<Output = Result<Vec<u8>, String>>,
) -> Result<String, String> {
    let bytes = buffer.await?;
    if bytes.is_empty() {
        return Err("unexpected end of compressed input".into());
    }
    let mut remaining = bytes.as_slice();
    let mut inflated = Vec::new();
    loop {
        let before = remaining.len();
        let gzip_member = remaining.starts_with(&[0x1f, 0x8b]);
        if gzip_member {
            let mut decoder = flate2::bufread::GzDecoder::new(remaining);
            decoder
                .read_to_end(&mut inflated)
                .map_err(|error| error.to_string())?;
            remaining = decoder.into_inner();
        } else {
            let mut decoder = flate2::bufread::ZlibDecoder::new(remaining);
            decoder
                .read_to_end(&mut inflated)
                .map_err(|error| error.to_string())?;
            remaining = decoder.into_inner();
        }
        if remaining.len() == before {
            return Err("compressed stream did not advance".into());
        }
        // Installed pako3 continues after gzip members, but stops after a zlib member.
        if !gzip_member || remaining.is_empty() || remaining[0] == 0 {
            break;
        }
    }
    // Uint16Array(Uint8Array) widens EACH byte, not pairs or UTF-8 characters.
    Ok(inflated.into_iter().map(char::from).collect())
}
