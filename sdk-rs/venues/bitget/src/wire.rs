use serde::Deserialize;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default)]
pub struct Number(pub Option<f64>);

impl Number {
    pub fn get(self) -> f64 {
        self.0.unwrap_or(0.0)
    }
    pub fn or(self, fallback: f64) -> f64 {
        self.0.unwrap_or(fallback)
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<'a> {
            Text(#[serde(borrow)] Cow<'a, str>),
            Numeric(f64),
            Null,
        }
        let value = match Wire::deserialize(d)? {
            Wire::Text(text) => text.parse::<f64>().ok(),
            Wire::Numeric(value) => Some(value),
            Wire::Null => None,
        };
        Ok(Self(value.filter(|v| v.is_finite())))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub base_coin: Cow<'a, str>,
    #[serde(borrow)]
    pub quote_coin: Cow<'a, str>,
    #[serde(borrow)]
    pub symbol_status: Cow<'a, str>,
    pub price_place: Number,
    pub price_end_step: Number,
    pub size_multiplier: Number,
    #[serde(default)]
    pub min_trade_num: Number,
    #[serde(default)]
    pub max_order_qty: Number,
    #[serde(default)]
    pub max_market_order_qty: Number,
    #[serde(default)]
    pub max_trade_num: Number,
    #[serde(default)]
    pub min_lever: Number,
    #[serde(default)]
    pub max_lever: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow<'a> {
    #[serde(default, borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    pub inst_id: Cow<'a, str>,
    #[serde(default, alias = "bidPrice", alias = "bid")]
    pub bid_pr: Number,
    #[serde(default, alias = "askPrice", alias = "ask")]
    pub ask_pr: Number,
    #[serde(default, alias = "lastPrice", alias = "last")]
    pub last_pr: Number,
    #[serde(default, alias = "markPr", alias = "mark")]
    pub mark_price: Number,
    #[serde(default, alias = "indexPr", alias = "index")]
    pub index_price: Number,
    #[serde(default, alias = "change")]
    pub change24h: Number,
    #[serde(default)]
    pub change_utc24h: Number,
    #[serde(default)]
    pub funding_rate: Number,
    #[serde(default, alias = "fundingTime")]
    pub next_funding_time: Number,
    #[serde(default, alias = "volume")]
    pub base_volume: Number,
    #[serde(default)]
    pub quote_volume: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Argument<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, borrow)]
    pub inst_id: Cow<'a, str>,
    #[serde(default, borrow)]
    pub topic: Cow<'a, str>,
}

#[derive(Deserialize)]
pub struct Message<'a, T> {
    #[serde(default, borrow)]
    pub arg: Argument<'a>,
    #[serde(default, borrow)]
    pub action: Cow<'a, str>,
    #[serde(default, borrow)]
    pub event: Cow<'a, str>,

    #[serde(default, borrow)]
    pub msg: Cow<'a, str>,
    pub data: Option<Vec<T>>,
    pub code: Option<Number>,
}

#[derive(Deserialize, Default)]
pub struct Depth<'a> {
    #[serde(default, borrow)]
    pub bids: Option<Vec<[Cow<'a, str>; 2]>>,
    #[serde(default, borrow)]
    pub asks: Option<Vec<[Cow<'a, str>; 2]>>,
    pub checksum: Option<i32>,
    #[serde(default, deserialize_with = "sequence")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "sequence")]
    pub pseq: Option<i64>,
}

pub type CandleRow = [Number; 7];

fn sequence<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Sequence<'a> {
        Text(#[serde(borrow)] Cow<'a, str>),
        Integer(i64),
    }
    Option::<Sequence>::deserialize(d)?
        .map(|value| match value {
            Sequence::Text(text) => text.parse().map_err(serde::de::Error::custom),
            Sequence::Integer(value) => Ok(value),
        })
        .transpose()
}

#[derive(Deserialize)]
pub struct Envelope<T> {
    pub code: Number,
    #[serde(default)]
    pub msg: String,
    pub data: Option<T>,
}
impl<T> Envelope<T> {
    pub fn checked(self) -> Result<T, String> {
        if self.code.0 != Some(0.0) {
            return Err(format!("Bitget {}: {}", self.code.get(), self.msg));
        }
        self.data
            .ok_or_else(|| "Bitget response has no data".to_owned())
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Identifier<'a> {
    Text(#[serde(borrow)] Cow<'a, str>),
    Integer(u64),
}
impl Default for Identifier<'_> {
    fn default() -> Self {
        Self::Text(Cow::Borrowed(""))
    }
}
impl Identifier<'_> {
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            Self::Text(s) => Cow::Borrowed(s),
            Self::Integer(i) => Cow::Owned(i.to_string()),
        }
    }
}

#[derive(Deserialize, Default)]
pub struct PositionRow<'a> {
    #[serde(default, rename = "marginCoin", borrow)]
    pub margin_coin: Cow<'a, str>,
    #[serde(default, rename = "totalFundingFee")]
    pub total_funding_fee: Number,
    #[serde(default, rename = "symbol", borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, rename = "instId", borrow)]
    pub inst_id: Cow<'a, str>,
    #[serde(default, rename = "holdSide", borrow)]
    pub hold_side: Cow<'a, str>,
    #[serde(default, rename = "posSide", borrow)]
    pub pos_side: Cow<'a, str>,
    #[serde(default, rename = "side", borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, rename = "posMode", borrow)]
    pub pos_mode: Cow<'a, str>,
    #[serde(default, rename = "holdMode", borrow)]
    pub hold_mode: Cow<'a, str>,
    #[serde(default, rename = "marginMode", borrow)]
    pub margin_mode: Cow<'a, str>,
    #[serde(default, rename = "total")]
    pub total: Number,
    #[serde(default, rename = "size")]
    pub size: Number,
    #[serde(default, rename = "pos")]
    pub pos: Number,
    #[serde(default, rename = "openTotalPos")]
    pub open_total_pos: Number,
    #[serde(default, rename = "available")]
    pub available: Number,
    #[serde(default, rename = "openPriceAvg")]
    pub open_price_avg: Number,
    #[serde(default, rename = "openAvgPrice")]
    pub open_avg_price: Number,
    #[serde(default, rename = "avgPrice")]
    pub avg_price: Number,
    #[serde(default, rename = "avgPx")]
    pub avg_px: Number,
    #[serde(default, rename = "averageOpenPrice")]
    pub average_open_price: Number,
    #[serde(default, rename = "markPrice")]
    pub mark_price: Number,
    #[serde(default, rename = "marketPrice")]
    pub market_price: Number,
    #[serde(default, rename = "leverage")]
    pub leverage: Number,
    #[serde(default, rename = "lever")]
    pub lever: Number,
    #[serde(default, rename = "unrealizedPL")]
    pub unrealized_pl: Number,
    #[serde(default, rename = "unrealisedPnl")]
    pub unrealised_pnl: Number,
    #[serde(default, rename = "upl")]
    pub upl: Number,
    #[serde(default, rename = "achievedProfits")]
    pub achieved_profits: Number,
    #[serde(default, rename = "totalFee")]
    pub total_fee: Number,
    #[serde(default, rename = "deductedFee")]
    pub deducted_fee: Number,
    #[serde(default, rename = "curRealisedPnl")]
    pub cur_realised_pnl: Number,
    #[serde(default, rename = "totalFunding")]
    pub total_funding: Number,
    #[serde(default, rename = "openFeeTotal")]
    pub open_fee_total: Number,
    #[serde(default, rename = "closeFeeTotal")]
    pub close_fee_total: Number,
    #[serde(default, rename = "liquidationPrice")]
    pub liquidation_price: Number,
    #[serde(default, rename = "liqPrice")]
    pub liq_price: Number,
    #[serde(default, rename = "liqPx")]
    pub liq_px: Number,
    #[serde(default, rename = "marginSize")]
    pub margin_size: Number,
    #[serde(default, rename = "margin")]
    pub margin: Number,
    #[serde(default, rename = "positionBalance")]
    pub position_balance: Number,
    #[serde(default, rename = "uTime")]
    pub u_time: Number,
    #[serde(default, rename = "updatedTime")]
    pub updated_time: Number,
    #[serde(default, rename = "cTime")]
    pub c_time: Number,
}

#[derive(Deserialize, Default)]
pub struct AccountRow<'a> {
    #[serde(default, rename = "marginCoin", borrow)]
    pub margin_coin: Cow<'a, str>,
    #[serde(default, rename = "coin", borrow)]
    pub coin: Cow<'a, str>,
    #[serde(default, rename = "unionTotalMargin")]
    pub union_total_margin: Number,
    #[serde(default, rename = "accountEquity")]
    pub account_equity: Number,
    #[serde(default, rename = "unionAvailable")]
    pub union_available: Number,
    #[serde(default, rename = "available")]
    pub available: Number,
    #[serde(default, rename = "unrealizedPL")]
    pub unrealized_pl: Number,
    #[serde(default, rename = "crossedUnrealizedPL")]
    pub crossed_unrealized_pl: Number,
    #[serde(default, rename = "isolatedUnrealizedPL")]
    pub isolated_unrealized_pl: Number,
}

#[derive(Deserialize, Default)]
pub struct OrderRow<'a> {
    #[serde(default, borrow)]
    pub category: Cow<'a, str>,
    #[serde(default, rename = "holdMode", borrow)]
    pub hold_mode: Cow<'a, str>,
    #[serde(default, rename = "createdTime")]
    pub created_time: Number,
    #[serde(default, rename = "updatedTime")]
    pub updated_time: Number,
    #[serde(default, rename = "symbol", borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, rename = "instId", borrow)]
    pub inst_id: Cow<'a, str>,
    #[serde(default, rename = "status", borrow)]
    pub status: Cow<'a, str>,
    #[serde(default, rename = "state", borrow)]
    pub state: Cow<'a, str>,
    #[serde(default, rename = "planStatus", borrow)]
    pub plan_status: Cow<'a, str>,
    #[serde(default, rename = "orderStatus", borrow)]
    pub order_status: Cow<'a, str>,
    #[serde(default, rename = "orderType", borrow)]
    pub order_type: Cow<'a, str>,
    #[serde(default, rename = "ordType", borrow)]
    pub ord_type: Cow<'a, str>,
    #[serde(default, rename = "planType", borrow)]
    pub plan_type: Cow<'a, str>,
    #[serde(default, rename = "side", borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, rename = "tradeSide", borrow)]
    pub trade_side: Cow<'a, str>,
    #[serde(default, rename = "tS", borrow)]
    pub t_s: Cow<'a, str>,
    #[serde(default, rename = "holdSide", borrow)]
    pub hold_side: Cow<'a, str>,
    #[serde(default, rename = "posSide", borrow)]
    pub pos_side: Cow<'a, str>,
    #[serde(default, rename = "posMode", borrow)]
    pub pos_mode: Cow<'a, str>,
    #[serde(default, rename = "force", borrow)]
    pub force: Cow<'a, str>,
    #[serde(default, rename = "timeInForce", borrow)]
    pub time_in_force: Cow<'a, str>,
    #[serde(default, rename = "tiF", borrow)]
    pub ti_f: Cow<'a, str>,
    #[serde(default, rename = "triggerType", borrow)]
    pub trigger_type: Cow<'a, str>,
    #[serde(default, rename = "tpTriggerBy", borrow)]
    pub tp_trigger_by: Cow<'a, str>,
    #[serde(default, rename = "slTriggerBy", borrow)]
    pub sl_trigger_by: Cow<'a, str>,
    #[serde(default, rename = "size")]
    pub size: Number,
    #[serde(default, rename = "sz")]
    pub sz: Number,
    #[serde(default, rename = "qty")]
    pub qty: Number,
    #[serde(default, rename = "amount")]
    pub amount: Number,
    #[serde(default, rename = "baseVolume")]
    pub base_volume: Number,
    #[serde(default, rename = "filledQty")]
    pub filled_qty: Number,
    #[serde(default, rename = "accFillSz")]
    pub acc_fill_sz: Number,
    #[serde(default, rename = "cumExecQty")]
    pub cum_exec_qty: Number,
    #[serde(default, rename = "accBaseVolume")]
    pub acc_base_volume: Number,
    #[serde(default, rename = "price")]
    pub price: Number,
    #[serde(default, rename = "px")]
    pub px: Number,
    #[serde(default, rename = "triggerPrice")]
    pub trigger_price: Number,
    #[serde(default, rename = "triggerPx")]
    pub trigger_px: Number,
    #[serde(default, rename = "presetStopLossPrice")]
    pub preset_stoploss_price: Number,
    #[serde(default, rename = "stopLossTriggerPrice")]
    pub stoploss_trigger_price: Number,
    #[serde(default, rename = "stopLoss")]
    pub stoploss: Number,
    #[serde(default, rename = "presetStopSurplusPrice")]
    pub preset_stop_surplus_price: Number,
    #[serde(default, rename = "stopSurplusTriggerPrice")]
    pub stop_surplus_trigger_price: Number,
    #[serde(default, rename = "presetTakeProfitPrice")]
    pub preset_take_profit_price: Number,
    #[serde(default, rename = "takeProfit")]
    pub take_profit: Number,
    #[serde(default, rename = "priceAvg")]
    pub price_avg: Number,
    #[serde(default, rename = "avgPrice")]
    pub avg_price: Number,
    #[serde(default, rename = "fillPrice")]
    pub fill_price: Number,
    #[serde(default, rename = "fillTime")]
    pub fill_time: Number,
    #[serde(default, rename = "uTime")]
    pub u_time: Number,
    #[serde(default, rename = "cTime")]
    pub c_time: Number,
    #[serde(default, rename = "orderId", borrow)]
    pub order_id: Identifier<'a>,
    #[serde(default, rename = "clientOid", borrow)]
    pub client_oid: Identifier<'a>,
    #[serde(default, rename = "ordId", borrow)]
    pub ord_id: Identifier<'a>,
    #[serde(default, rename = "id", borrow)]
    pub id: Identifier<'a>,
    #[serde(default, rename = "tradeId", borrow)]
    pub trade_id: Identifier<'a>,
    #[serde(default, rename = "execId", borrow)]
    pub exec_id: Identifier<'a>,
    #[serde(default, rename = "reduceOnly")]
    pub reduce_only: Flag,
}

#[derive(Deserialize, Default)]
pub struct SymbolConfig<'a> {
    #[serde(default, rename = "symbol", borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, rename = "category", borrow)]
    pub category: Cow<'a, str>,
    #[serde(default, rename = "marginMode", borrow)]
    pub margin_mode: Cow<'a, str>,
    #[serde(default, rename = "posMode", borrow)]
    pub pos_mode: Cow<'a, str>,
    #[serde(default, rename = "leverage")]
    pub leverage: Number,
    #[serde(default, rename = "longLeverage")]
    pub long_leverage: Number,
    #[serde(default, rename = "shortLeverage")]
    pub short_leverage: Number,
    #[serde(default, rename = "crossedMarginLeverage")]
    pub crossed_margin_leverage: Number,
    #[serde(default, rename = "isolatedLongLever")]
    pub isolated_long_lever: Number,
    #[serde(default, rename = "isolatedShortLever")]
    pub isolated_short_lever: Number,
}

#[derive(Deserialize, Default)]
pub struct UtaSettings<'a> {
    #[serde(default, rename = "holdMode", borrow)]
    pub hold_mode: Cow<'a, str>,
    #[serde(default, rename = "symbolConfigList", borrow)]
    pub symbol_config_list: Vec<SymbolConfig<'a>>,
}

#[derive(Deserialize, Default)]
pub struct UtaAsset<'a> {
    #[serde(default, rename = "coin", borrow)]
    pub coin: Cow<'a, str>,
    #[serde(default, rename = "usdValue")]
    pub usd_value: Number,
    #[serde(default, rename = "equity")]
    pub equity: Number,
    #[serde(default, rename = "available")]
    pub available: Number,
}

#[derive(Deserialize, Default)]
pub struct UtaBalance<'a> {
    #[serde(default, rename = "accountEquity")]
    pub account_equity: Number,
    #[serde(default, rename = "usdtEquity")]
    pub usdt_equity: Number,
    #[serde(default, rename = "effEquity")]
    pub eff_equity: Number,
    #[serde(default, rename = "unrealisedPnl")]
    pub unrealised_pnl: Number,
    #[serde(default, rename = "usdtUnrealisedPnl")]
    pub usdt_unrealised_pnl: Number,
    #[serde(default, rename = "assets", borrow)]
    pub assets: Vec<UtaAsset<'a>>,
}

#[derive(Default)]
pub struct Flag(pub bool);
impl<'de> Deserialize<'de> for Flag {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<'a> {
            Bool(bool),
            Text(#[serde(borrow)] Cow<'a, str>),
        }
        Ok(Self(match Wire::deserialize(d)? {
            Wire::Bool(v) => v,
            Wire::Text(v) => matches!(v.as_ref(), "YES" | "yes" | "true"),
        }))
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    pub end_id: Option<String>,
    pub cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRow<'a> {
    #[serde(borrow)]
    pub category: Cow<'a, str>,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub order_id: Identifier<'a>,
    #[serde(borrow)]
    pub exec_id: Identifier<'a>,
    pub exec_price: Number,
    pub exec_qty: Number,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
}
