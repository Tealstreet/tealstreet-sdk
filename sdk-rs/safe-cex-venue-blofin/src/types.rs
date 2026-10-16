//! BloFin protocol declarations. Source: blofin.types.ts.
use safe_cex_num::{js_number_output, numeric_ingress::JsNumberOutput};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;

pub const RECV_WINDOW: f64 = 20000.0;
pub const BROKER_ID: &str = "b3cdedc0a20880ba";
pub const USE_NON_BATCH_ENDPOINT_FOR_REDUCE_ONLY: bool = false;
pub static BASE_URL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "livenet": "https://openapi.blofin.com",
        "testnet": "https://demo-trading-openapi.blofin.com"
    })
});
pub static BASE_WS_URL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "public": {
            "livenet": "wss://openapi.blofin.com/ws/public",
            "testnet": "wss://demo-trading-openapi.blofin.com/ws/public"
        },
        "private": {
            "livenet": "wss://openapi.blofin.com/ws/private",
            "testnet": "wss://demo-trading-openapi.blofin.com/ws/private"
        }
    })
});
pub static ENDPOINTS: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "MARKETS": "/api/v1/market/instruments",
        "TICKERS": "/api/v1/market/tickers",
        "BALANCE": "/api/v1/asset/balances",
        "POSITIONS": "/api/v1/account/positions",
        "KLINE": "/api/v1/market/candles",
        "FILLS": "/api/v1/trade/fills-history",
        "POSITIONS_HISTORY": "/api/v1/account/positions-history",
        "ORDERS_HISTORY": "/api/v1/trade/orders-history",
        "UNFILLED_ORDERS": "/api/v1/trade/orders-pending",
        "UNFILLED_ALGO_ORDERS": "/api/v1/trade/orders-tpsl-pending",
        "UNFILLED_TRIGGER_ORDERS": "/api/v1/trade/orders-algo-pending",
        "CANCEL_ORDERS": "/api/v1/trade/cancel-batch-orders",
        "CANCEL_ALGO_ORDER": "/api/v1/trade/cancel-tpsl",
        "CANCEL_TRIGGER_ORDER": "/api/v1/trade/cancel-algo",
        "PLACE_ORDERS": "/api/v1/trade/order",
        "PLACE_ORDERS_BATCH": "/api/v1/trade/batch-orders",
        "PLACE_TPSL_ORDER": "/api/v1/trade/order-tpsl",
        "PLACE_TRIGGER_ORDER": "/api/v1/trade/order-algo",
        "SET_LEVERAGE": "/api/v1/account/set-leverage",
        "LEVERAGE": "/api/v1/account/batch-leverage-info",
        "MARGIN_MODE": "/api/v1/account/margin-mode",
        "POSITION_MODE": "/api/v1/account/position-mode",
        "SET_MARGIN_MODE": "/api/v1/account/set-margin-mode",
        "SET_POSITION_MODE": "/api/v1/account/set-position-mode"
    })
});
pub static GRAFANA_ENDPOINTS_EVERY: LazyLock<Value> = LazyLock::new(|| {
    json!([
        ENDPOINTS["CANCEL_ORDERS"],
        ENDPOINTS["CANCEL_ALGO_ORDER"],
        ENDPOINTS["CANCEL_TRIGGER_ORDER"],
        ENDPOINTS["PLACE_ORDERS"],
        ENDPOINTS["PLACE_ORDERS_BATCH"],
        ENDPOINTS["PLACE_TPSL_ORDER"],
        ENDPOINTS["PLACE_TRIGGER_ORDER"],
        ENDPOINTS["SET_LEVERAGE"],
        ENDPOINTS["LEVERAGE"],
        ENDPOINTS["MARGIN_MODE"],
        ENDPOINTS["POSITION_MODE"],
        ENDPOINTS["SET_MARGIN_MODE"],
        ENDPOINTS["SET_POSITION_MODE"]
    ])
});
pub static GRAFANA_ENDPOINTS_PERIODIC: LazyLock<Value> = LazyLock::new(|| {
    json!([
        ENDPOINTS["BALANCE"],
        ENDPOINTS["POSITIONS"],
        ENDPOINTS["UNFILLED_ORDERS"],
        ENDPOINTS["UNFILLED_ALGO_ORDERS"],
        ENDPOINTS["UNFILLED_TRIGGER_ORDERS"]
    ])
});
pub static INTERVAL: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "1m": "1m",
        "3m": "3m",
        "5m": "5m",
        "15m": "15m",
        "30m": "30m",
        "1h": "1H",
        "2h": "2H",
        "4h": "4H",
        "6h": "6H",
        "12h": "12H",
        "1d": "1D",
        "1w": "1W"
    })
});
pub static ORDER_STATUS: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "live": "open",
        "partially_filled": "open"
    })
});
pub static ORDER_TYPE: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "market": "market",
        "limit": "limit",
        "post_only": "limit",
        "fok": "limit",
        "ioc": "limit",
        "optimal_limit_ioc": "market"
    })
});
pub static REVERSE_ORDER_TYPE: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "market": "market",
        "limit": "limit",
        "conditional": "trigger",
        "stop_market": "trigger",
        "take_profit_market": "trigger",
        "trailing_stop_market": "move_order_stop"
    })
});
pub static ORDER_SIDE: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "buy": "buy",
        "sell": "sell"
    })
});
pub static POSITION_SIDE: LazyLock<Value> = LazyLock::new(|| {
    json!({
        "long": "long",
        "short": "short"
    })
});

fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchPositionResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "data")]
    pub data: Vec<FetchPositionResponseData>,
}
impl<'de> Deserialize<'de> for FetchPositionResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg")]
            msg: String,
            #[serde(rename = "data")]
            data: Vec<FetchPositionResponseData>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for FetchPositionResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchPositionResponseData {
    #[serde(rename = "positionId")]
    pub position_id: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "instType")]
    pub inst_type: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "adl")]
    pub adl: String,
    #[serde(rename = "positions")]
    pub positions: String,
    #[serde(rename = "availablePositions")]
    pub available_positions: String,
    #[serde(rename = "averagePrice")]
    pub average_price: String,
    #[serde(rename = "margin")]
    pub margin: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "unrealizedPnl")]
    pub unrealized_pnl: String,
    #[serde(rename = "unrealizedPnlRatio")]
    pub unrealized_pnl_ratio: String,
    #[serde(rename = "maintenanceMargin")]
    pub maintenance_margin: String,
    #[serde(rename = "createTime")]
    pub create_time: String,
    #[serde(rename = "updateTime")]
    pub update_time: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
}
impl<'de> Deserialize<'de> for FetchPositionResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "positionId")]
            position_id: String,
            #[serde(rename = "instId")]
            inst_id: String,
            #[serde(rename = "instType")]
            inst_type: String,
            #[serde(rename = "marginMode")]
            margin_mode: String,
            #[serde(rename = "positionSide")]
            position_side: String,
            #[serde(rename = "adl")]
            adl: String,
            #[serde(rename = "positions")]
            positions: String,
            #[serde(rename = "availablePositions")]
            available_positions: String,
            #[serde(rename = "averagePrice")]
            average_price: String,
            #[serde(rename = "margin")]
            margin: String,
            #[serde(rename = "markPrice")]
            mark_price: String,
            #[serde(rename = "marginRatio")]
            margin_ratio: String,
            #[serde(rename = "liquidationPrice")]
            liquidation_price: String,
            #[serde(rename = "unrealizedPnl")]
            unrealized_pnl: String,
            #[serde(rename = "unrealizedPnlRatio")]
            unrealized_pnl_ratio: String,
            #[serde(rename = "maintenanceMargin")]
            maintenance_margin: String,
            #[serde(rename = "createTime")]
            create_time: String,
            #[serde(rename = "updateTime")]
            update_time: String,
            #[serde(rename = "leverage")]
            leverage: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            position_id: wire.position_id,
            inst_id: wire.inst_id,
            inst_type: wire.inst_type,
            margin_mode: wire.margin_mode,
            position_side: wire.position_side,
            adl: wire.adl,
            positions: wire.positions,
            available_positions: wire.available_positions,
            average_price: wire.average_price,
            margin: wire.margin,
            mark_price: wire.mark_price,
            margin_ratio: wire.margin_ratio,
            liquidation_price: wire.liquidation_price,
            unrealized_pnl: wire.unrealized_pnl,
            unrealized_pnl_ratio: wire.unrealized_pnl_ratio,
            maintenance_margin: wire.maintenance_margin,
            create_time: wire.create_time,
            update_time: wire.update_time,
            leverage: wire.leverage,
        })
    }
}
impl JsNumberOutput for FetchPositionResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { position_id, inst_id, inst_type, margin_mode, position_side, adl, positions, available_positions, average_price, margin, mark_price, margin_ratio, liquidation_price, unrealized_pnl, unrealized_pnl_ratio, maintenance_margin, create_time, update_time, leverage })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionModeResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "data")]
    pub data: PositionModeResponseData,
}
impl<'de> Deserialize<'de> for PositionModeResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg")]
            msg: String,
            #[serde(rename = "data")]
            data: PositionModeResponseData,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for PositionModeResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionModeResponseData {
    #[serde(rename = "positionMode")]
    pub position_mode: String,
}
impl<'de> Deserialize<'de> for PositionModeResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "positionMode")]
            position_mode: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            position_mode: wire.position_mode,
        })
    }
}
impl JsNumberOutput for PositionModeResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { position_mode })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchMarginModeResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "data")]
    pub data: FetchMarginModeResponseData,
}
impl<'de> Deserialize<'de> for FetchMarginModeResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg")]
            msg: String,
            #[serde(rename = "data")]
            data: FetchMarginModeResponseData,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for FetchMarginModeResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchMarginModeResponseData {
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
}
impl<'de> Deserialize<'de> for FetchMarginModeResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "marginMode")]
            margin_mode: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            margin_mode: wire.margin_mode,
        })
    }
}
impl JsNumberOutput for FetchMarginModeResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { margin_mode })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchBatchLevResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "data")]
    pub data: Vec<FetchBatchLevResponseData>,
}
impl<'de> Deserialize<'de> for FetchBatchLevResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg")]
            msg: String,
            #[serde(rename = "data")]
            data: Vec<FetchBatchLevResponseData>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for FetchBatchLevResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FetchBatchLevResponseData {
    #[serde(rename = "leverage")]
    pub leverage: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "positionSide")]
    pub position_side: BatchPositionSide,
}
impl<'de> Deserialize<'de> for FetchBatchLevResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "leverage")]
            leverage: String,
            #[serde(rename = "marginMode")]
            margin_mode: String,
            #[serde(rename = "instId")]
            inst_id: String,
            #[serde(rename = "positionSide")]
            position_side: BatchPositionSide,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            leverage: wire.leverage,
            margin_mode: wire.margin_mode,
            inst_id: wire.inst_id,
            position_side: wire.position_side,
        })
    }
}
impl JsNumberOutput for FetchBatchLevResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { leverage, margin_mode, inst_id, position_side })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrdersResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg", skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(rename = "data", skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<OrdersResponseData>>,
}
impl<'de> Deserialize<'de> for OrdersResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg", default, deserialize_with = "present")]
            msg: Option<String>,
            #[serde(rename = "data", default, deserialize_with = "present")]
            data: Option<Vec<OrdersResponseData>>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for OrdersResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrdersResponseData {
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "tdMode")]
    pub td_mode: String,
    #[serde(rename = "cTime")]
    pub c_time: String,
    #[serde(rename = "uTime")]
    pub u_time: String,
    #[serde(rename = "price")]
    pub price: String,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    #[serde(rename = "state")]
    pub state: String,
    #[serde(rename = "filledSize")]
    pub filled_size: String,
    #[serde(rename = "avgFillPrice")]
    pub avg_fill_price: String,
    #[serde(rename = "fee")]
    pub fee: String,
    #[serde(rename = "feeCurrency")]
    pub fee_currency: String,
}
impl<'de> Deserialize<'de> for OrdersResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "orderId")]
            order_id: String,
            #[serde(rename = "instId")]
            inst_id: String,
            #[serde(rename = "tdMode")]
            td_mode: String,
            #[serde(rename = "cTime")]
            c_time: String,
            #[serde(rename = "uTime")]
            u_time: String,
            #[serde(rename = "price")]
            price: String,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "side")]
            side: String,
            #[serde(rename = "orderType")]
            order_type: String,
            #[serde(rename = "state")]
            state: String,
            #[serde(rename = "filledSize")]
            filled_size: String,
            #[serde(rename = "avgFillPrice")]
            avg_fill_price: String,
            #[serde(rename = "fee")]
            fee: String,
            #[serde(rename = "feeCurrency")]
            fee_currency: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            order_id: wire.order_id,
            inst_id: wire.inst_id,
            td_mode: wire.td_mode,
            c_time: wire.c_time,
            u_time: wire.u_time,
            price: wire.price,
            size: wire.size,
            side: wire.side,
            order_type: wire.order_type,
            state: wire.state,
            filled_size: wire.filled_size,
            avg_fill_price: wire.avg_fill_price,
            fee: wire.fee,
            fee_currency: wire.fee_currency,
        })
    }
}
impl JsNumberOutput for OrdersResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { order_id, inst_id, td_mode, c_time, u_time, price, size, side, order_type, state, filled_size, avg_fill_price, fee, fee_currency })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionsResponse {
    #[serde(rename = "code")]
    pub code: String,
    #[serde(rename = "msg")]
    pub msg: String,
    #[serde(rename = "data", skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<PositionsResponseData>>,
}
impl<'de> Deserialize<'de> for PositionsResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "code")]
            code: String,
            #[serde(rename = "msg")]
            msg: String,
            #[serde(rename = "data", default, deserialize_with = "present")]
            data: Option<Vec<PositionsResponseData>>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            code: wire.code,
            msg: wire.msg,
            data: wire.data,
        })
    }
}
impl JsNumberOutput for PositionsResponse {
    fn check_number_output(&self) {
        js_number_output!(self, struct { code, msg, data })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionsResponseData {
    #[serde(rename = "positionId")]
    pub position_id: String,
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "instType")]
    pub inst_type: String,
    #[serde(rename = "marginMode")]
    pub margin_mode: String,
    #[serde(rename = "positionSide")]
    pub position_side: String,
    #[serde(rename = "adl")]
    pub adl: String,
    #[serde(rename = "positions")]
    pub positions: String,
    #[serde(rename = "availablePositions")]
    pub available_positions: String,
    #[serde(rename = "averagePrice")]
    pub average_price: String,
    #[serde(rename = "markPrice")]
    pub mark_price: String,
    #[serde(rename = "marginRatio")]
    pub margin_ratio: String,
    #[serde(rename = "liquidationPrice")]
    pub liquidation_price: String,
    #[serde(rename = "unrealizedPnl")]
    pub unrealized_pnl: String,
    #[serde(rename = "unrealizedPnlRatio")]
    pub unrealized_pnl_ratio: String,
    #[serde(rename = "initialMargin")]
    pub initial_margin: String,
    #[serde(rename = "maintenanceMargin")]
    pub maintenance_margin: String,
    #[serde(rename = "createTime")]
    pub create_time: String,
    #[serde(rename = "updateTime")]
    pub update_time: String,
    #[serde(rename = "leverage")]
    pub leverage: String,
}
impl<'de> Deserialize<'de> for PositionsResponseData {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "positionId")]
            position_id: String,
            #[serde(rename = "instId")]
            inst_id: String,
            #[serde(rename = "instType")]
            inst_type: String,
            #[serde(rename = "marginMode")]
            margin_mode: String,
            #[serde(rename = "positionSide")]
            position_side: String,
            #[serde(rename = "adl")]
            adl: String,
            #[serde(rename = "positions")]
            positions: String,
            #[serde(rename = "availablePositions")]
            available_positions: String,
            #[serde(rename = "averagePrice")]
            average_price: String,
            #[serde(rename = "markPrice")]
            mark_price: String,
            #[serde(rename = "marginRatio")]
            margin_ratio: String,
            #[serde(rename = "liquidationPrice")]
            liquidation_price: String,
            #[serde(rename = "unrealizedPnl")]
            unrealized_pnl: String,
            #[serde(rename = "unrealizedPnlRatio")]
            unrealized_pnl_ratio: String,
            #[serde(rename = "initialMargin")]
            initial_margin: String,
            #[serde(rename = "maintenanceMargin")]
            maintenance_margin: String,
            #[serde(rename = "createTime")]
            create_time: String,
            #[serde(rename = "updateTime")]
            update_time: String,
            #[serde(rename = "leverage")]
            leverage: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            position_id: wire.position_id,
            inst_id: wire.inst_id,
            inst_type: wire.inst_type,
            margin_mode: wire.margin_mode,
            position_side: wire.position_side,
            adl: wire.adl,
            positions: wire.positions,
            available_positions: wire.available_positions,
            average_price: wire.average_price,
            mark_price: wire.mark_price,
            margin_ratio: wire.margin_ratio,
            liquidation_price: wire.liquidation_price,
            unrealized_pnl: wire.unrealized_pnl,
            unrealized_pnl_ratio: wire.unrealized_pnl_ratio,
            initial_margin: wire.initial_margin,
            maintenance_margin: wire.maintenance_margin,
            create_time: wire.create_time,
            update_time: wire.update_time,
            leverage: wire.leverage,
        })
    }
}
impl JsNumberOutput for PositionsResponseData {
    fn check_number_output(&self) {
        js_number_output!(self, struct { position_id, inst_id, inst_type, margin_mode, position_side, adl, positions, available_positions, average_price, mark_price, margin_ratio, liquidation_price, unrealized_pnl, unrealized_pnl_ratio, initial_margin, maintenance_margin, create_time, update_time, leverage })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum BatchPositionSide {
    #[serde(rename = "net")]
    Net,
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}
impl<'de> Deserialize<'de> for BatchPositionSide {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(remote = "BatchPositionSide")]
        enum Wire {
            #[serde(rename = "net")]
            Net,
            #[serde(rename = "long")]
            Long,
            #[serde(rename = "short")]
            Short,
        }
        let value = String::deserialize(d)?;
        Wire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl JsNumberOutput for BatchPositionSide {
    fn check_number_output(&self) {
        js_number_output!(self, enum { Net, Long, Short })
    }
}
