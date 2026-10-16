use safe_cex_types::OrderStopTriggerType;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsWorkerType {
    pub place_order: PlaceOrderSettings,
}

#[derive(Deserialize)]
#[serde(remote = "AppSettingsWorkerType")]
#[serde(rename_all = "camelCase")]
struct AppSettingsWorkerTypeWire {
    place_order: PlaceOrderSettings,
}

impl<'de> Deserialize<'de> for AppSettingsWorkerType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        AppSettingsWorkerTypeWire::deserialize(serde_json::Value::Object(record))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlaceOrderSettings {
    #[serde(rename = "tpTriggerType")]
    pub tp_trigger_type: OrderStopTriggerType,
    #[serde(rename = "slTriggerType")]
    pub sl_trigger_type: OrderStopTriggerType,
}

#[derive(Deserialize)]
#[serde(remote = "PlaceOrderSettings")]
struct PlaceOrderSettingsWire {
    #[serde(rename = "tpTriggerType")]
    tp_trigger_type: OrderStopTriggerType,
    #[serde(rename = "slTriggerType")]
    sl_trigger_type: OrderStopTriggerType,
}

impl<'de> Deserialize<'de> for PlaceOrderSettings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        PlaceOrderSettingsWire::deserialize(serde_json::Value::Object(record))
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExchangeMetadataWorkerType {
    #[serde(rename = "supportedTPSLTriggerTypes")]
    pub supported_tpsl_trigger_types: Vec<OrderStopTriggerType>,
    #[serde(rename = "privateProxyIps")]
    pub private_proxy_ips: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "defaultTPTriggerType"
    )]
    pub default_tp_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "defaultSLTriggerType"
    )]
    pub default_sl_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "fetchLeverageOnMarginSymbolChange"
    )]
    pub fetch_leverage_on_margin_symbol_change: Option<bool>,
}

#[derive(Deserialize)]
#[serde(remote = "ExchangeMetadataWorkerType")]
struct ExchangeMetadataWorkerTypeWire {
    #[serde(rename = "supportedTPSLTriggerTypes")]
    supported_tpsl_trigger_types: Vec<OrderStopTriggerType>,
    #[serde(rename = "privateProxyIps")]
    private_proxy_ips: Vec<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        rename = "defaultTPTriggerType"
    )]
    default_tp_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        rename = "defaultSLTriggerType"
    )]
    default_sl_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none",
        rename = "fetchLeverageOnMarginSymbolChange"
    )]
    fetch_leverage_on_margin_symbol_change: Option<bool>,
}

impl<'de> Deserialize<'de> for ExchangeMetadataWorkerType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        ExchangeMetadataWorkerTypeWire::deserialize(serde_json::Value::Object(record))
            .map_err(serde::de::Error::custom)
    }
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
