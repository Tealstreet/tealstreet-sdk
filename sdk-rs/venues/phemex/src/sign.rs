//! Phemex native expiry/query codec; provider recipes own signature work.
pub mod dispatch;
pub(crate) fn expiry(now: f64) -> u64 {
    ((now + 60_000.0) / 1000.0 + 0.5).floor() as u64
}
// TS signs decodeURIComponent(qs.stringify(params)); a plus remains a plus.
fn decoded_query(query: &str) -> String {
    let bytes = query.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            )
        {
            result.push((high * 16 + low) as u8);
            index += 3;
            continue;
        }
        result.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| query.to_owned())
}
