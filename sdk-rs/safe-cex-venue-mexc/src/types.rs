//! Protocol constants and response envelope from `mexc.types.ts`.
use safe_cex_num::{
    js_number_output,
    numeric_ingress::{JsNumberOutput, js_from_str, js_from_value},
};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;

pub const RECV_WINDOW: u64 = 20_000;
pub const MEXC_WS_HEARTBEAT_INTERVAL_MS: u64 = 8_000;
pub static BASE_URL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "livenet":"https://api.mexc.com", "testnet":"https://api.mexc.com"
    })
});
pub static BASE_WS_URL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "public":{"livenet":"wss://contract.mexc.com/edge","testnet":"wss://contract.mexc.com/edge"},
        "private":{"livenet":"wss://contract.mexc.com/edge","testnet":"wss://contract.mexc.com/edge"}
    })
});
pub fn is_mexc_control_channel(channel: &str) -> bool {
    channel.starts_with("rs.")
}
pub static ENDPOINTS: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "CONTRACT_DETAIL":"/api/v1/contract/detail",
        "TICKER":"/api/v1/contract/ticker",
        "DEPTH":"/api/v1/contract/depth",
        "KLINE":"/api/v1/contract/kline",
        "ASSETS":"/api/v1/private/account/assets",
        "POSITION_MODE":"/api/v1/private/position/position_mode",
        "POSITION_LEVERAGE":"/api/v1/private/position/leverage",
        "CHANGE_POSITION_MODE":"/api/v1/private/position/change_position_mode",
        "CHANGE_LEVERAGE":"/api/v1/private/position/change_leverage",
        "OPEN_POSITIONS":"/api/v1/private/position/open_positions",
        "OPEN_ORDERS":"/api/v1/private/order/list/open_orders",
        "ORDER_DEALS":"/api/v1/private/order/list/order_deals",
        "ORDER_SUBMIT":"/api/v1/private/order/submit",
        "ORDER_SUBMIT_BATCH":"/api/v1/private/order/submit_batch",
        "ORDER_CANCEL":"/api/v1/private/order/cancel",
        "ORDER_CANCEL_ALL":"/api/v1/private/order/cancel_all",
        "PLAN_ORDER_LIST":"/api/v1/private/planorder/list/orders",
        "PLAN_ORDER_PLACE":"/api/v1/private/planorder/place/v2",
        "PLAN_ORDER_CANCEL":"/api/v1/private/planorder/cancel",
        "STOP_ORDER_LIST":"/api/v1/private/stoporder/list/orders",
        "STOP_ORDER_PLACE":"/api/v1/private/stoporder/place",
        "STOP_ORDER_CANCEL":"/api/v1/private/stoporder/cancel"
    })
});
pub static PUBLIC_ENDPOINTS: LazyLock<Value> = LazyLock::new(|| {
    json!([
        ENDPOINTS["CONTRACT_DETAIL"],
        ENDPOINTS["TICKER"],
        ENDPOINTS["DEPTH"],
        ENDPOINTS["KLINE"]
    ])
});
pub static GRAFANA_ENDPOINTS_EVERY: LazyLock<Value> = LazyLock::new(|| {
    json!([
        ENDPOINTS["ORDER_SUBMIT"],
        ENDPOINTS["ORDER_SUBMIT_BATCH"],
        ENDPOINTS["PLAN_ORDER_PLACE"],
        ENDPOINTS["PLAN_ORDER_CANCEL"],
        ENDPOINTS["STOP_ORDER_PLACE"],
        ENDPOINTS["STOP_ORDER_CANCEL"],
        ENDPOINTS["ORDER_CANCEL"],
        ENDPOINTS["ORDER_CANCEL_ALL"],
        ENDPOINTS["CHANGE_POSITION_MODE"],
        ENDPOINTS["CHANGE_LEVERAGE"]
    ])
});
pub static GRAFANA_ENDPOINTS_PERIODIC: LazyLock<Value> = LazyLock::new(|| {
    json!([
        ENDPOINTS["ASSETS"],
        ENDPOINTS["POSITION_MODE"],
        ENDPOINTS["POSITION_LEVERAGE"],
        ENDPOINTS["OPEN_POSITIONS"],
        ENDPOINTS["OPEN_ORDERS"]
    ])
});
pub static INTERVAL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "1m":"Min1","5m":"Min5","15m":"Min15","30m":"Min30","1h":"Min60","4h":"Hour4","1d":"Day1","1w":"Week1"
    })
});
pub static ORDER_SIDE: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "1":{"side":"buy","positionSide":"long","reduceOnly":false},
        "2":{"side":"buy","positionSide":"short","reduceOnly":true},
        "3":{"side":"sell","positionSide":"short","reduceOnly":false},
        "4":{"side":"sell","positionSide":"long","reduceOnly":true}
    })
});
pub static REVERSE_ORDER_SIDE: LazyLock<Value> =
    LazyLock::new(|| json!({"buy":{"open":1,"close":2},"sell":{"open":3,"close":4}}));
pub static ORDER_TYPE: LazyLock<Value> =
    LazyLock::new(|| json!({"1":"limit","2":"limit","3":"limit","4":"limit","5":"market"}));
pub static ORDER_TIME_IN_FORCE: LazyLock<Value> = LazyLock::new(
    || json!({"1":"GoodTillCancel","2":"PostOnly","3":"ImmediateOrCancel","4":"FillOrKill","5":"GoodTillCancel"}),
);
pub static REVERSE_ORDER_TYPE: LazyLock<Value> = LazyLock::new(
    || json!({"limit":1,"market":5,"conditional":1,"stop_market":1,"take_profit_market":1,"trailing_stop_market":1}),
);
pub static REVERSE_TIME_IN_FORCE: LazyLock<Value> =
    LazyLock::new(|| json!({"GoodTillCancel":1,"PostOnly":2,"ImmediateOrCancel":3,"FillOrKill":4}));
pub static ORDER_STATUS: LazyLock<Value> =
    LazyLock::new(|| json!({"1":"open","2":"open","3":"closed","4":"canceled","5":"canceled"}));
pub static POSITION_SIDE: LazyLock<Value> = LazyLock::new(|| json!({"1":"long","2":"short"}));
pub static POSITION_MODE: LazyLock<Value> = LazyLock::new(|| json!({"1":"hedged","2":"oneway"}));
pub static REVERSE_POSITION_MODE: LazyLock<Value> =
    LazyLock::new(|| json!({"hedged":1,"oneway":2}));
pub static MARGIN_TYPE: LazyLock<Value> = LazyLock::new(|| json!({"1":"isolated","2":"cross"}));
pub static REVERSE_MARGIN_TYPE: LazyLock<Value> = LazyLock::new(|| json!({"isolated":1,"cross":2}));

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MexcCode {
    Number(f64),
    String(String),
}

impl JsNumberOutput for MexcCode {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Number(value), String(value) });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Generic API response. Numeric-safe consumers should use
/// [`decode_mexc_envelope`] or [`decode_mexc_envelope_value`] at ingress.
pub struct MexcEnvelope<T = Value> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<MexcCode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: JsNumberOutput> JsNumberOutput for MexcEnvelope<T> {
    fn check_number_output(&self) {
        js_number_output!(self, struct { success, code, message, msg, data });
    }
}

/// Decode a MEXC response while retaining numeric lexemes through the envelope
/// and its generic payload. Decode `data` into its concrete model afterwards.
pub fn decode_mexc_envelope<T: serde::de::DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
) -> Result<MexcEnvelope<T>, serde_json::Error> {
    js_from_str(raw)
}

/// Decode an existing MEXC JSON value through the checked numeric buffer.
pub fn decode_mexc_envelope_value<T: serde::de::DeserializeOwned + Serialize + JsNumberOutput>(
    value: Value,
) -> Result<MexcEnvelope<T>, serde_json::Error> {
    js_from_value(value)
}
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for MexcEnvelope<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(bound(deserialize = "T: Deserialize<'de>"))]
        struct Wire<T> {
            #[serde(default, deserialize_with = "present")]
            success: Option<bool>,
            #[serde(default, deserialize_with = "present")]
            code: Option<MexcCode>,
            #[serde(default, deserialize_with = "present")]
            message: Option<String>,
            #[serde(default, deserialize_with = "present")]
            msg: Option<String>,
            #[serde(default, deserialize_with = "present")]
            data: Option<T>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        let (message, msg) = (wire.message, wire.msg);
        Ok(Self {
            success: wire.success,
            code: wire.code,
            message,
            msg,
            data: wire.data,
        })
    }
}
