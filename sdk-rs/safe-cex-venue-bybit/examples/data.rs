use safe_cex_venue_bybit::{rate_limits, spot_types, types};
use std::io::{self, Read};
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let origins: Vec<Option<String>> = serde_json::from_str(&input).unwrap();
    let helpers: Vec<_> = origins
        .iter()
        .map(|origin| {
            let normalized = types::normalize_configured_origin(origin.as_deref());
            serde_json::json!([
                normalized,
                types::rest_origin_to_ws_url(&normalized, "/v5/private")
            ])
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"linear": {"RECV_WINDOW": types::RECV_WINDOW,
"BROKER_ID": types::BROKER_ID,
"BASE_URL": &*types::BASE_URL,
"BASE_WS_URL": &*types::BASE_WS_URL,
"DEMO_URL": &*types::DEMO_URL,
"DEMO_WS_URL": &*types::DEMO_WS_URL,
"ENDPOINTS": &*types::ENDPOINTS,
"GRAFANA_ENDPOINTS_EVERY": &*types::GRAFANA_ENDPOINTS_EVERY,
"GRAFANA_ENDPOINTS_PERIODIC": &*types::GRAFANA_ENDPOINTS_PERIODIC,
"INTERVAL": &*types::INTERVAL,
"ORDER_STATUS": &*types::ORDER_STATUS,
"ORDER_TYPE": &*types::ORDER_TYPE,
"POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE": &*types::POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE,
"ORDER_SIDE": &*types::ORDER_SIDE,
"POSITION_SIDE": &*types::POSITION_SIDE,
"ORDER_TIME_IN_FORCE": &*types::ORDER_TIME_IN_FORCE,
"ORDER_TIME_IN_FORCE_OUTBOUND": &*types::ORDER_TIME_IN_FORCE_OUTBOUND,
"ORDER_TRIGGER_TYPE": &*types::ORDER_TRIGGER_TYPE,
"ORDER_HISTORY_MAX_WINDOW_MS": types::ORDER_HISTORY_MAX_WINDOW_MS,
"HYROTRADER_PROD_URL": &*types::HYROTRADER_PROD_URL,
"HYROTRADER_PROD_WS_URL": &*types::HYROTRADER_PROD_WS_URL,
"HYROTRADER_SIMULATOR_URL": &*types::HYROTRADER_SIMULATOR_URL,
"HYROTRADER_SIMULATOR_WS_URL": &*types::HYROTRADER_SIMULATOR_WS_URL},"spot": {"RECV_WINDOW": spot_types::RECV_WINDOW,
"BROKER_ID": spot_types::BROKER_ID,
"BASE_URL": &*spot_types::BASE_URL,
"BASE_WS_URL": &*spot_types::BASE_WS_URL,
"DEMO_URL": &*spot_types::DEMO_URL,
"DEMO_WS_URL": &*spot_types::DEMO_WS_URL,
"ENDPOINTS": &*spot_types::ENDPOINTS,
"GRAFANA_ENDPOINTS_EVERY": &*spot_types::GRAFANA_ENDPOINTS_EVERY,
"GRAFANA_ENDPOINTS_PERIODIC": &*spot_types::GRAFANA_ENDPOINTS_PERIODIC,
"INTERVAL": &*spot_types::INTERVAL,
"ORDER_STATUS": &*spot_types::ORDER_STATUS,
"ORDER_TYPE": &*spot_types::ORDER_TYPE,
"ORDER_SIDE": &*spot_types::ORDER_SIDE,
"POSITION_SIDE": &*spot_types::POSITION_SIDE,
"ORDER_TIME_IN_FORCE": &*spot_types::ORDER_TIME_IN_FORCE,
"ORDER_TIME_IN_FORCE_OUTBOUND": &*spot_types::ORDER_TIME_IN_FORCE_OUTBOUND,
"ORDER_TRIGGER_TYPE": &*spot_types::ORDER_TRIGGER_TYPE}, "rate_limits": {"BYBIT_RATE_LIMIT_TABLE": &*rate_limits::BYBIT_RATE_LIMIT_TABLE, "BybitVipLevel": {"NoVIP": rate_limits::BybitVipLevel::NoVIP, "VIP1": rate_limits::BybitVipLevel::VIP1, "VIP2": rate_limits::BybitVipLevel::VIP2, "VIP3": rate_limits::BybitVipLevel::VIP3, "VIP4": rate_limits::BybitVipLevel::VIP4, "VIP5": rate_limits::BybitVipLevel::VIP5, "VIPSupreme": rate_limits::BybitVipLevel::VIPSupreme, "PRO1": rate_limits::BybitVipLevel::PRO1, "PRO2": rate_limits::BybitVipLevel::PRO2, "PRO3": rate_limits::BybitVipLevel::PRO3, "PRO4": rate_limits::BybitVipLevel::PRO4, "PRO5": rate_limits::BybitVipLevel::PRO5, "PRO6": rate_limits::BybitVipLevel::PRO6}, "BybitProductType": {"Futures": rate_limits::BybitProductType::Futures, "Option": rate_limits::BybitProductType::Option, "Spot": rate_limits::BybitProductType::Spot}, "BybitAccountType": {"ClassicAndUTA": rate_limits::BybitAccountType::ClassicAndUTA, "UTAPro": rate_limits::BybitAccountType::UTAPro}}, "helpers": helpers})
    );
}
