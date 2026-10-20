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

// Bitget sends "" or null for values that do not apply; both stay absent.
impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_engine::de::lenient_optional_number(d).map(Self)
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

#[derive(Default)]
pub struct TickerRow<'a> {
    pub symbol: Cow<'a, str>,
    pub inst_id: Cow<'a, str>,
    // One presence mask avoids a separate optional tag per numeric field.
    values: [f64; 11],
    present: u16,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(field_identifier, rename_all = "camelCase")]
pub enum TickerField {
    BidPr,
    AskPr,
    LastPr,
    MarkPrice,
    IndexPrice,
    Change24h,
    ChangeUtc24h,
    FundingRate,
    NextFundingTime,
    BaseVolume,
    QuoteVolume,
    Symbol,
    InstId,
    #[serde(other)]
    Other,
}

impl TickerRow<'_> {
    pub fn has_values(&self) -> bool {
        self.present != 0
    }
    pub fn update_mask(&self) -> u16 {
        // ChangeUtc24h shares the percentage slot; engine bit6 is open interest.
        (self.present & !(1 << 6))
            | if self.present & (1 << 6) != 0 {
                1 << 5
            } else {
                0
            }
    }
    pub fn value(&self, field: TickerField) -> Option<f64> {
        let index = field as usize;
        (self.present & (1 << index) != 0).then(|| self.values[index])
    }
}

impl<'de> Deserialize<'de> for TickerRow<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Ticker;
        impl<'de> serde::de::Visitor<'de> for Ticker {
            type Value = TickerRow<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a Bitget ticker")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut row = TickerRow::default();
                let mut seen = 0u16;
                while let Some(field) = map.next_key::<TickerField>()? {
                    if matches!(field, TickerField::Other) {
                        let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        continue;
                    }
                    let bit = 1 << field as usize;
                    if seen & bit != 0 {
                        return Err(serde::de::Error::custom("duplicate Bitget ticker field"));
                    }
                    seen |= bit;
                    match field {
                        TickerField::Symbol => row.symbol = map.next_value::<Text<'de>>()?.0,
                        TickerField::InstId => row.inst_id = map.next_value::<Text<'de>>()?.0,
                        field => {
                            if let Some(value) = map.next_value::<Number>()?.0 {
                                row.values[field as usize] = value;
                                row.present |= bit;
                            }
                        }
                    }
                }
                Ok(row)
            }
        }
        d.deserialize_map(Ticker)
    }
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
    #[serde(default, rename = "usdtEquity")]
    pub usdt_equity: Number,
    #[serde(default, rename = "marginCoin", borrow)]
    pub margin_coin: Cow<'a, str>,
    #[serde(default, rename = "coin", borrow)]
    pub coin: Cow<'a, str>,
    #[serde(default, rename = "unionTotalMargin")]
    pub union_total_margin: Number,
    #[serde(default, rename = "accountEquity")]
    pub account_equity: Number,
    #[serde(default)]
    pub equity: Number,
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
    #[serde(default, rename = "callbackRatio", alias = "rangeRate")]
    pub callback_ratio: Number,
    #[serde(default, rename = "type", borrow)]
    pub strategy_type: Cow<'a, str>,
    #[serde(default, rename = "trailingStopParams")]
    pub trailing_stop_params: Option<TrailingRow>,
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

#[derive(Deserialize)]
pub struct PublicHeader<'a> {
    #[serde(borrow)]
    pub arg: Argument<'a>,
}

// Most public messages contain one row; keep it inline rather than allocating
// a vector for every ticker frame. Additional rows retain normal sequence order.
pub struct Rows<T> {
    head: Option<T>,
    tail: Vec<T>,
}
impl<T> IntoIterator for Rows<T> {
    type Item = T;
    type IntoIter = std::iter::Chain<std::option::IntoIter<T>, std::vec::IntoIter<T>>;
    fn into_iter(self) -> Self::IntoIter {
        self.head.into_iter().chain(self.tail)
    }
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Rows<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct RowsVisitor<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for RowsVisitor<T> {
            type Value = Rows<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("an array of feed rows")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let head = seq.next_element()?;
                let mut tail = Vec::new();
                if head.is_none() {
                    return Ok(Rows { head, tail });
                }
                while let Some(row) = seq.next_element()? {
                    tail.push(row);
                }
                Ok(Rows { head, tail })
            }
        }
        d.deserialize_seq(RowsVisitor(std::marker::PhantomData))
    }
}
struct Text<'a>(Cow<'a, str>);
impl<'de> Deserialize<'de> for Text<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct TextVisitor;
        impl<'de> serde::de::Visitor<'de> for TextVisitor {
            type Value = Text<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("text")
            }
            fn visit_borrowed_str<E: serde::de::Error>(
                self,
                value: &'de str,
            ) -> Result<Self::Value, E> {
                Ok(Text(Cow::Borrowed(value)))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value.into())))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Text(Cow::Owned(value)))
            }
        }
        d.deserialize_str(TextVisitor)
    }
}

#[allow(clippy::large_enum_variant)] // The common ticker row stays inline on the hot path.
pub enum PublicData<'a> {
    None,
    Tickers(Rows<TickerRow<'a>>),
    Books(Vec<Depth<'a>>),
    Candles(Vec<CandleRow>),
}

pub struct PublicMessage<'a> {
    pub arg: Argument<'a>,
    pub action: Cow<'a, str>,
    pub event: Cow<'a, str>,
    pub msg: Cow<'a, str>,
    pub data: PublicData<'a>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(field_identifier, rename_all = "camelCase")]
enum PublicField {
    Arg,
    Action,
    Event,
    Msg,
    Data,
    #[serde(other)]
    Other,
}

impl<'de> Deserialize<'de> for PublicMessage<'de> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Public;
        impl<'de> serde::de::Visitor<'de> for Public {
            type Value = PublicMessage<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a Bitget public envelope")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut message = PublicMessage {
                    arg: Argument::default(),
                    action: Cow::Borrowed(""),
                    event: Cow::Borrowed(""),
                    msg: Cow::Borrowed(""),
                    data: PublicData::None,
                };
                let mut deferred: Option<&'de serde_json::value::RawValue> = None;
                let mut seen = 0u8;
                while let Some(key) = map.next_key::<PublicField>()? {
                    let (bit, name) = match key {
                        PublicField::Arg => (1, "arg"),
                        PublicField::Action => (2, "action"),
                        PublicField::Event => (4, "event"),
                        PublicField::Msg => (8, "msg"),
                        PublicField::Data => (16, "data"),
                        PublicField::Other => (0, ""),
                    };
                    if bit != 0 && seen & bit != 0 {
                        return Err(serde::de::Error::duplicate_field(name));
                    }
                    seen |= bit;
                    match key {
                        PublicField::Arg => message.arg = map.next_value()?,
                        PublicField::Action => message.action = map.next_value::<Text<'de>>()?.0,
                        PublicField::Event => message.event = map.next_value::<Text<'de>>()?.0,
                        PublicField::Msg => message.msg = map.next_value::<Text<'de>>()?.0,
                        PublicField::Data => {
                            message.data = match message.arg.channel.as_ref() {
                                "ticker" => map
                                    .next_value::<Option<Rows<TickerRow<'de>>>>()?
                                    .map(PublicData::Tickers)
                                    .unwrap_or(PublicData::None),
                                "books" => map
                                    .next_value::<Option<Vec<Depth<'de>>>>()?
                                    .map(PublicData::Books)
                                    .unwrap_or(PublicData::None),
                                channel if channel.starts_with("candle") => map
                                    .next_value::<Option<Vec<CandleRow>>>()?
                                    .map(PublicData::Candles)
                                    .unwrap_or(PublicData::None),
                                _ => {
                                    deferred = map.next_value()?;
                                    PublicData::None
                                }
                            }
                        }
                        PublicField::Other => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }
                // Field order is not contractual. The usual arg-before-data path
                // reads each typed row once; reordered envelopes borrow raw data.
                if let Some(raw) = deferred {
                    message.data = match message.arg.channel.as_ref() {
                        "ticker" => PublicData::Tickers(
                            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?,
                        ),
                        "books" => PublicData::Books(
                            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?,
                        ),
                        channel if channel.starts_with("candle") => PublicData::Candles(
                            serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?,
                        ),
                        _ => PublicData::None,
                    };
                }
                Ok(message)
            }
        }
        d.deserialize_map(Public)
    }
}

#[cfg(test)]
mod ticker_tests {
    use super::{TickerField, TickerRow};
    use std::borrow::Cow;

    #[test]
    fn compact_tickers_preserve_absence_signed_zero_and_native_fields() {
        let row: TickerRow<'_> = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","instId":"BTC\u0055SDT","bidPr":null,"askPr":"bad","lastPr":"-0","markPrice":"NaN","indexPrice":103.5,"change24h":"0.2","changeUtc24h":"0.1","fundingRate":"-0.01","nextFundingTime":"0","baseVolume":"1.5","quoteVolume":"2.5","futureField":{"nested":[1,2]}}"#,
        )
        .unwrap();
        assert!(matches!(row.symbol, Cow::Borrowed("BTCUSDT")));
        assert!(matches!(&row.inst_id, Cow::Owned(value) if value == "BTCUSDT"));
        assert_eq!(row.value(TickerField::BidPr), None);
        assert_eq!(row.value(TickerField::AskPr), None);
        assert_eq!(row.value(TickerField::MarkPrice), None);
        assert_eq!(
            row.value(TickerField::LastPr).unwrap().to_bits(),
            (-0.0f64).to_bits()
        );
        for (field, value) in [
            (TickerField::IndexPrice, 103.5),
            (TickerField::Change24h, 0.2),
            (TickerField::ChangeUtc24h, 0.1),
            (TickerField::FundingRate, -0.01),
            (TickerField::NextFundingTime, 0.0),
            (TickerField::BaseVolume, 1.5),
            (TickerField::QuoteVolume, 2.5),
        ] {
            assert_eq!(row.value(field), Some(value));
        }
        let empty: TickerRow<'_> = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.value(TickerField::LastPr), None);
    }

    #[test]
    fn compact_tickers_reject_duplicates_and_invalid_numeric_types() {
        for text in [
            r#"{"bidPr":null,"bidPr":"1"}"#,
            r#"{"lastPr":"bad","lastPr":"1"}"#,
            r#"{"symbol":"BTCUSDT","symbol":"ETHUSDT"}"#,
            r#"{"instId":"BTCUSDT","instId":"ETHUSDT"}"#,
            r#"{"lastPr":true}"#,
        ] {
            assert!(
                serde_json::from_str::<TickerRow<'_>>(text).is_err(),
                "{text}"
            );
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct TrailingRow {
    pub activation_price: Number,
    pub activation_type: String,
    pub trail_type: String,
    pub pre_order_type: String,
    pub trail_variance: Number,
}
