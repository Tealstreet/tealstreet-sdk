//! Signature byte conversion; encoder capability selection stays source ordered.
pub trait Base64Host {
    type Error;
    fn has_btoa(&self) -> bool;
    fn btoa(&self, bytes: &[u8]) -> Result<Vec<u16>, Self::Error>;
    fn has_buffer(&self) -> bool;
    fn buffer_base64(&self, bytes: &[u8]) -> Result<Vec<u16>, Self::Error>;
    fn error(&self, message: &'static str) -> Self::Error;
}
fn whitespace(u: u16) -> bool {
    matches!(u, 9..=13 | 32 | 160 | 0x1680 | 0x2000..=0x200a | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff)
}
fn parse_pair(mut pair: &[u16]) -> u8 {
    while pair.first().is_some_and(|u| whitespace(*u)) {
        pair = &pair[1..];
    }
    let negative = pair.first() == Some(&45);
    if matches!(pair.first(), Some(43 | 45)) {
        pair = &pair[1..];
    }
    if pair.starts_with(&[48, 120]) || pair.starts_with(&[48, 88]) {
        pair = &pair[2..];
    }
    let mut n = 0u8;
    for u in pair {
        let digit = match *u {
            48..=57 => *u - 48,
            65..=70 => *u - 55,
            97..=102 => *u - 87,
            _ => break,
        };
        n = n.wrapping_mul(16).wrapping_add(digit as u8);
    }
    if negative { n.wrapping_neg() } else { n }
}
pub fn to_base64_from_hex<H: Base64Host>(host: &H, hex: &[u16]) -> Result<Vec<u16>, H::Error> {
    let normalized = if hex.starts_with(&[48, 120]) {
        &hex[2..]
    } else {
        hex
    };
    if normalized.len() % 2 != 0 {
        return Err(host.error("Invalid hex signature length"));
    }
    let bytes: Vec<u8> = normalized
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| parse_pair(pair))
        .collect();
    if host.has_btoa() {
        return host.btoa(&bytes);
    }
    if host.has_buffer() {
        return host.buffer_base64(&bytes);
    }
    Err(host.error("No base64 encoder available in current runtime"))
}
