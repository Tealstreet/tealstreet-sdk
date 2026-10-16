//! GRVT builder protocol constants, measured against the source module.
pub const TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET: &str =
    "0xee8f3559f298fffa79b8253437969d15b5dccff3";
pub const TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET: &str =
    "0xb0108ae4c07218ce320e5e22dd911a01c2042162";
pub const TEALSTREET_GRVT_BUILDER_ADDRESS: &str = TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET;
pub const DEFAULT_GRVT_BUILDER_FEE_RATE: &str = "0.03";

/// ts: getGrvtBuilderAddress
pub fn get_grvt_builder_address(testnet: bool) -> &'static str {
    if testnet {
        TEALSTREET_GRVT_BUILDER_ADDRESS_TESTNET
    } else {
        TEALSTREET_GRVT_BUILDER_ADDRESS_MAINNET
    }
}

/// ts: grvtBuilderFeeRateToRaw
pub fn grvt_builder_fee_rate_to_raw(fee_rate: &str) -> Result<f64, String> {
    let parsed = safe_cex_num::parse_float(fee_rate);
    if !parsed.is_finite() {
        return Err(format!("Invalid GRVT builder fee rate: {fee_rate}"));
    }
    // Source checks the input only; derived infinity and signed zero survive.
    Ok(safe_cex_num::math_round(parsed * 10_000.0))
}

/// ts: createGrvtBuilderInfo
/// None selects the source default; an explicitly empty string remains present.
pub fn create_grvt_builder_info(
    testnet: Option<bool>,
    builder_address: Option<&str>,
    builder_fee_rate: Option<&str>,
) -> Result<crate::records::GrvtBuilderInfo, String> {
    let builder = builder_address
        .unwrap_or_else(|| get_grvt_builder_address(testnet.unwrap_or(false)))
        .to_lowercase();
    let fee = builder_fee_rate.unwrap_or(DEFAULT_GRVT_BUILDER_FEE_RATE);
    Ok(crate::records::GrvtBuilderInfo {
        builder,
        builder_fee: fee.to_owned(),
        builder_fee_int: grvt_builder_fee_rate_to_raw(fee)?,
    })
}

/// ts: GrvtBuilderUnauthorizedError
#[derive(Debug, Clone)]
pub struct GrvtBuilderUnauthorizedError {
    pub name: &'static str,
    pub code: &'static str,
    pub reason: crate::types::GrvtBuilderUnauthorizedReason,
    pub required_raw: f64,
    pub authorized_raw: Option<f64>,
    pub message: String,
}

impl GrvtBuilderUnauthorizedError {
    /// ts: GrvtBuilderUnauthorizedError.constructor
    pub fn new(
        reason: crate::types::GrvtBuilderUnauthorizedReason,
        required_raw: f64,
        authorized_raw: Option<f64>,
        message: Option<&str>,
    ) -> Self {
        let message = message.map_or_else(
            || match reason {
                crate::types::GrvtBuilderUnauthorizedReason::CapTooLow => format!(
                    "GRVT builder authorization cap ({}) is below required ({}). Please re-authorize.",
                    authorized_raw.map_or_else(|| "unknown".to_owned(), safe_cex_num::js_number_string),
                    safe_cex_num::js_number_string(required_raw),
                ),
                crate::types::GrvtBuilderUnauthorizedReason::Unauthorized =>
                    "GRVT builder is not authorized for this account. Please authorize before placing orders.".to_owned(),
            },
            str::to_owned,
        );
        Self {
            name: "GrvtBuilderUnauthorizedError",
            code: "GRVT_BUILDER_UNAUTHORIZED",
            reason,
            required_raw,
            authorized_raw,
            message,
        }
    }
}

impl std::fmt::Display for GrvtBuilderUnauthorizedError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            formatter.write_str(self.name)
        } else if self.name.is_empty() {
            formatter.write_str(&self.message)
        } else {
            write!(formatter, "{}: {}", self.name, self.message)
        }
    }
}

impl std::error::Error for GrvtBuilderUnauthorizedError {}

/// ts: getEIP712Domain
pub fn get_eip712_domain(testnet: bool) -> serde_json::Value {
    serde_json::json!({
        "name": "GRVT Exchange",
        "version": "0",
        "chainId": if testnet { 326 } else { 325 },
    })
}

/// ts: getStringField
pub fn get_string_field(value: Option<&serde_json::Value>) -> Option<&str> {
    value.and_then(serde_json::Value::as_str)
}

/// ts: getValidAddress
pub fn get_valid_address<'a>(values: &[Option<&'a str>]) -> Option<&'a str> {
    values
        .iter()
        .flatten()
        .copied()
        .find(|value| safe_cex_crypto::ethereum::get_address(value).is_ok())
}

/// ts: grvtProxy
pub fn grvt_proxy(
    registry: &safe_cex_transport::TransportRegistry,
) -> Option<std::sync::Arc<dyn safe_cex_transport::PrivateProxy>> {
    registry
        .get_transport(None)
        .private_proxy(&safe_cex_types::ExchangeName::Grvt)
}

/// ts: viaGrvtProxy
pub fn via_grvt_proxy(registry: &safe_cex_transport::TransportRegistry, base_url: &str) -> String {
    match grvt_proxy(registry) {
        Some(proxy) => format!("{}/{base_url}", proxy.url()),
        None => base_url.to_owned(),
    }
}

/// ts: resolveGrvtProxyHeaders
pub fn resolve_grvt_proxy_headers(
    registry: &safe_cex_transport::TransportRegistry,
) -> safe_cex_transport::TransportFuture<'static, std::collections::BTreeMap<String, String>> {
    use std::task::{Context, Poll, Waker};
    let proxy = grvt_proxy(registry);
    let mut task: safe_cex_transport::TransportFuture<'static, _> = Box::pin(async move {
        match proxy {
            Some(proxy) => proxy.resolve_headers().await,
            None => Ok(std::collections::BTreeMap::new()),
        }
    });
    // Enter the imported resolver before returning, retaining its pending future.
    match task.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => Box::pin(std::future::ready(result)),
        Poll::Pending => task,
    }
}

/// ts: getGrvtEdgeApiUrl
pub fn get_grvt_edge_api_url(
    registry: &safe_cex_transport::TransportRegistry,
    testnet: bool,
) -> String {
    via_grvt_proxy(
        registry,
        crate::protocol::BASE_URLS[if testnet { 5 } else { 2 }].1,
    )
}

/// ts: getGrvtTradingApiUrl
pub fn get_grvt_trading_api_url(
    registry: &safe_cex_transport::TransportRegistry,
    testnet: bool,
) -> String {
    via_grvt_proxy(
        registry,
        crate::protocol::BASE_URLS[if testnet { 4 } else { 1 }].1,
    )
}
