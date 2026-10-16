use safe_cex_types::object_map::JsObjectMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum CrossTradWeightMode {
    Balance,
    Absolute,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum CrossTradeExecution {
    LimitSlippage,
    Market,
    Chase,
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossTradeCopier {
    pub enabled: bool,
    pub account: Option<String>,
    pub weight: f64,
    pub execution: CrossTradeExecution,
    pub weight_mode: CrossTradWeightMode,
    pub symbol_mappings: JsObjectMap<String>,
}
impl Default for CrossTradeCopier {
    fn default() -> Self {
        default_cross_trade_copier()
    }
}
pub fn default_cross_trade_copier() -> CrossTradeCopier {
    CrossTradeCopier {
        enabled: false,
        account: None,
        weight: 1.0,
        execution: CrossTradeExecution::LimitSlippage,
        weight_mode: CrossTradWeightMode::Absolute,
        symbol_mappings: JsObjectMap::new(),
    }
}
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrossTradeConfig {
    pub enabled: bool,
    pub master_account: Option<String>,
    pub master_symbol: Option<String>,
    pub master_weight: f64,
    pub copiers: Vec<CrossTradeCopier>,
}
impl Default for CrossTradeConfig {
    fn default() -> Self {
        default_cross_trade_config()
    }
}
pub fn default_cross_trade_config() -> CrossTradeConfig {
    CrossTradeConfig {
        enabled: false,
        master_account: None,
        master_symbol: None,
        master_weight: 1.0,
        copiers: vec![],
    }
}

#[derive(Deserialize)]
#[serde(remote = "CrossTradeCopier", rename_all = "camelCase")]
struct CrossTradeCopierWire {
    enabled: bool,
    account: Option<String>,
    weight: f64,
    execution: CrossTradeExecution,
    weight_mode: CrossTradWeightMode,
    symbol_mappings: JsObjectMap<String>,
}
impl<'de> Deserialize<'de> for CrossTradeCopier {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                CrossTradeCopierWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Deserialize)]
#[serde(remote = "CrossTradeConfig", rename_all = "camelCase")]
struct CrossTradeConfigWire {
    enabled: bool,
    master_account: Option<String>,
    master_symbol: Option<String>,
    master_weight: f64,
    copiers: Vec<CrossTradeCopier>,
}
impl<'de> Deserialize<'de> for CrossTradeConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                CrossTradeConfigWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CrossTradeConfig {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { enabled, master_account, master_symbol, master_weight, copiers });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CrossTradeCopier {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { enabled, account, weight, execution, weight_mode, symbol_mappings });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CrossTradeExecution {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { LimitSlippage, Market, Chase });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for CrossTradWeightMode {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Balance, Absolute });
    }
}
