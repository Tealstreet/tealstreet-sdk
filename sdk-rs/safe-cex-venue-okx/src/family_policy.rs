//! Endpoint and position policies shared by the OKX family.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{OrderSide, OrderType, PositionMode};

fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .expect("known OKX endpoint")
        .1
}

// ts: OKXExchange.getMarketInstrumentsEndpoint
pub fn get_market_instruments_endpoint() -> &'static str {
    endpoint("MARKETS")
}

// ts: OKXEUExchange.getInstrumentTypes
pub fn get_eu_instrument_types() -> [crate::types::OkxInstrumentType; 2] {
    [
        crate::types::OkxInstrumentType::Swap,
        crate::types::OkxInstrumentType::Futures,
    ]
}

// ts: OKXEUExchange.getMarketInstrumentsEndpoint
pub fn get_eu_market_instruments_endpoint(public_data_only: bool) -> &'static str {
    if public_data_only {
        endpoint("MARKETS")
    } else {
        endpoint("ACCOUNT_INSTRUMENTS")
    }
}

// ts: OKXExchange#getPositionSide
pub fn get_position_side(
    account_position_mode: Option<PositionMode>,
    order_type: OrderType,
    side: OrderSide,
    reduce_only: Option<bool>,
) -> Result<Option<&'static str>, ExchangeError> {
    let mode = account_position_mode
        .ok_or_else(|| ExchangeError::new("Account position mode not loaded"))?;
    if mode == PositionMode::OneWay {
        return Ok(None);
    }
    let mut long = side == OrderSide::Buy;
    if matches!(
        order_type,
        OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
    ) || (order_type != OrderType::Conditional && reduce_only == Some(true))
    {
        long = !long;
    }
    Ok(Some(if long { "long" } else { "short" }))
}

// ts: ORDER_STATS_TOPIC
pub const ORDER_STATS_TOPIC: &str = "orders";

// ts: OKXBusinessWebsocket.get resyncsPrivateStateOnReopen
pub fn business_resyncs_private_state_on_reopen() -> bool {
    true
}

fn websocket_endpoint(
    region: crate::types::OkxRegion,
    role: usize,
    testnet: Option<bool>,
) -> &'static str {
    crate::types::BASE_WS_URLS
        .iter()
        .find(|(key, _)| *key == region)
        .expect("closed OKX region")
        .1[role][usize::from(testnet == Some(true))]
}

// ts: OKXPrivateWebsocket.get endpoint
pub fn get_private_websocket_endpoint(
    region: crate::types::OkxRegion,
    testnet: Option<bool>,
) -> &'static str {
    websocket_endpoint(region, 1, testnet)
}

// ts: OKXBusinessWebsocket.get endpoint
pub fn get_business_websocket_endpoint(
    region: crate::types::OkxRegion,
    testnet: Option<bool>,
) -> &'static str {
    websocket_endpoint(region, 2, testnet)
}
