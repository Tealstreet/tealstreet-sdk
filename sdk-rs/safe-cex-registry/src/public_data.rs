pub const EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS: u64 = 6_000;
pub const EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS: u64 = 20_000;

pub fn get_candle_subscription_key(symbol: &str, interval: &str) -> String {
    format!("{symbol}:{interval}")
}

pub fn parse_candle_subscription_key(key: &str) -> (String, String) {
    match key.rfind(':') {
        Some(separator) => (key[..separator].into(), key[separator + 1..].into()),
        None => {
            let mut units: Vec<_> = key.encode_utf16().collect();
            units.pop();
            (String::from_utf16_lossy(&units), key.into())
        }
    }
}
