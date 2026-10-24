//! Opaque dispatch recipe and request continuation data.
pub mod dispatch;
#[cfg(test)]
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_engine::{Method, json::ArrayAt};
#[derive(Clone)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub body: Option<String>,
    pub at: Option<ArrayAt>,
    pub floor: Option<safe_cex_engine::StampFloor>,
}
#[cfg(test)]
pub fn signature(secret: &str, timestamp: u64, method: Method, path: &str, body: &str) -> String {
    hmac_sha256(
        secret.as_bytes(),
        format!("{timestamp}{}{path}{body}", method.as_str()).as_bytes(),
        Encoding::Hex,
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documented_signature() {
        assert_eq!(
            signature(
                "902ae3cb34ecee2779aa4d3e1d226686",
                1588591856950,
                Method::Post,
                "/sapi/v1/order/test",
                r#"{"symbol":"BTCUSDT","price":"9300","volume":"1","side":"BUY","type":"LIMIT"}"#
            ),
            "c50d0a74bb9427a9a03933d0eded03af9bf50115dc5b706882a4fcf07a26b761"
        );
    }
}
