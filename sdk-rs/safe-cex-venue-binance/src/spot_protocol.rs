use safe_cex_num::js_number_string;

// ts: BINANCE_SPOT_USER_DATA_REQUEST_ID
pub const BINANCE_SPOT_USER_DATA_REQUEST_ID: &str = "tealstreet-user-data";
// ts: BINANCE_SPOT_HEARTBEAT_REQUEST_ID
pub const BINANCE_SPOT_HEARTBEAT_REQUEST_ID: f64 = 42.0;

// ts: buildBinanceSpotUserDataSignaturePayload
pub fn build_binance_spot_user_data_signature_payload(
    api_key: &str,
    timestamp: f64,
    recv_window: Option<f64>,
) -> String {
    format!(
        "apiKey={api_key}&recvWindow={}&timestamp={}",
        js_number_string(recv_window.unwrap_or(crate::binancespot_types::USER_DATA_RECV_WINDOW)),
        js_number_string(timestamp)
    )
}
