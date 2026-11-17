//! Native account UI records composed from engine and algorithm models.
pub use safe_cex_chaser::ActiveChaser;
use safe_cex_engine::model::*;
use serde::{Deserialize, Serialize, ser::SerializeMap};
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTwap {
    pub id: String,
    pub symbol: String,
    pub account_name: String,
    pub active_market: Market,
    pub side: OrderSide,
    pub status: String,
    #[serde(flatten)]
    pub state: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Default, Serialize)]
pub struct BalanceView {
    #[serde(flatten)]
    pub balance: Balance,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coins: Option<BTreeMap<String, CoinBalance>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currencies: Option<BTreeMap<String, Balance>>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionExtra {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contracts_native: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unrealized_pnl_native: Option<f64>,
}
#[derive(Clone)]
pub struct PositionView {
    pub native: Position,
    pub extra: Option<PositionExtra>,
}
impl Deref for PositionView {
    type Target = Position;
    fn deref(&self) -> &Position {
        &self.native
    }
}
impl DerefMut for PositionView {
    fn deref_mut(&mut self) -> &mut Position {
        &mut self.native
    }
}
impl Serialize for PositionView {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let p = &self.native;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("id", &p.id)?;
        m.serialize_entry("symbol", &p.symbol)?;
        m.serialize_entry("side", &p.side)?;
        m.serialize_entry("entryPrice", &p.entry_price)?;
        m.serialize_entry("notional", &p.notional.is_finite().then_some(p.notional))?;
        m.serialize_entry(
            "leverage",
            &if p.leverage.is_finite() {
                p.leverage
            } else {
                0.
            },
        )?;
        m.serialize_entry(
            "unrealizedPnl",
            &if p.unrealized_pnl.is_finite() {
                p.unrealized_pnl
            } else {
                0.
            },
        )?;
        m.serialize_entry("contracts", &p.contracts)?;
        m.serialize_entry(
            "liquidationPrice",
            &((p.liquidation_price > 0.) && p.liquidation_price.is_finite())
                .then_some(p.liquidation_price),
        )?;
        if let Some(v) = p.margin_type {
            m.serialize_entry("marginType", &v)?;
        }
        if let Some(v) = p.position_mode {
            m.serialize_entry("positionMode", &v)?;
        }
        for (key, v) in [
            ("realizedPnl", p.realized_pnl),
            ("marginSize", p.margin_size),
            ("lastUpdate", p.last_update),
        ] {
            if let Some(v) = v.filter(|n| n.is_finite()) {
                m.serialize_entry(key, &v)?;
            }
        }
        if let Some(v) = &self.extra {
            m.serialize_entry("extra", v)?;
        }
        m.end()
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayedPosition {
    #[serde(flatten)]
    pub position: PositionView,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub account: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ticker: Option<Ticker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub market: Option<Market>,
    pub balance: BalanceView,
    pub exchange_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_min_upnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_min_upnl: Option<f64>,
    pub has_active_chaser: bool,
    #[serde(rename = "hasActiveTWAP")]
    pub has_active_twap: bool,
    pub has_full_stop: bool,
    #[serde(rename = "hasFullTP")]
    pub has_full_tp: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fake_position: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chaser_size: Option<f64>,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayedMargin {
    pub long_exposure: Option<f64>,
    pub short_exposure: Option<f64>,
    pub total_notional: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRiskProfile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_looking_like_a_full_stop: Option<bool>,
    pub order_pnl: f64,
    pub total_realized_pnl: f64,
    pub rolling_unrealized_pnl: f64,
    pub order_line_pos_side: PositionSide,
    pub position_size: f64,
    pub order_line_text: String,
    pub fee: f64,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub markets: Vec<Market>,
    pub tickers: Vec<Ticker>,
    pub positions: Vec<Position>,
    pub orders: Vec<Order>,
    pub balance: BalanceView,
    pub active_chasers: Vec<ActiveChaser>,
    pub active_twaps: Vec<ActiveTwap>,
    pub displayed_positions: Vec<DisplayedPosition>,
    pub displayed_margin: DisplayedMargin,
    pub orders_risk: BTreeMap<String, Vec<OrderRiskProfile>>,
}
