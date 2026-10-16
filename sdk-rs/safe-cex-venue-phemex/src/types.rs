//! Source record shapes from phemex.types.ts. Decode at the boundary with
//! safe_cex_num::numeric_ingress, before any serde_json::Value buffer.
use safe_cex_num::numeric_ingress::JsNumberOutput;
use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhemexAccount {
    #[serde(rename = "userID")]
    pub user_id: f64,
    #[serde(rename = "accountId")]
    pub account_id: f64,
    #[serde(rename = "currency")]
    pub currency: String,
    #[serde(rename = "accountBalanceRv")]
    pub account_balance_rv: String,
    #[serde(rename = "totalUsedBalanceRv")]
    pub total_used_balance_rv: String,
    #[serde(rename = "bonusBalanceRv")]
    pub bonus_balance_rv: String,
    #[serde(rename = "status")]
    pub status: f64,
}
impl<'de> Deserialize<'de> for PhemexAccount {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "userID")]
            user_id: f64,
            #[serde(rename = "accountId")]
            account_id: f64,
            #[serde(rename = "currency")]
            currency: String,
            #[serde(rename = "accountBalanceRv")]
            account_balance_rv: String,
            #[serde(rename = "totalUsedBalanceRv")]
            total_used_balance_rv: String,
            #[serde(rename = "bonusBalanceRv")]
            bonus_balance_rv: String,
            #[serde(rename = "status")]
            status: f64,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            user_id: wire.user_id,
            account_id: wire.account_id,
            currency: wire.currency,
            account_balance_rv: wire.account_balance_rv,
            total_used_balance_rv: wire.total_used_balance_rv,
            bonus_balance_rv: wire.bonus_balance_rv,
            status: wire.status,
        })
    }
}
impl JsNumberOutput for PhemexAccount {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { user_id, account_id, currency, account_balance_rv, total_used_balance_rv, bonus_balance_rv, status });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhemexPosition {
    #[serde(rename = "userID")]
    pub user_id: f64,
    #[serde(rename = "accountID")]
    pub account_id: f64,
    #[serde(rename = "symbol")]
    pub symbol: String,
    #[serde(rename = "currency")]
    pub currency: String,
    #[serde(rename = "side")]
    pub side: String,
    #[serde(rename = "positionStatus")]
    pub position_status: String,
    #[serde(rename = "crossMargin")]
    pub cross_margin: bool,
    #[serde(rename = "leverageRr")]
    pub leverage_rr: String,
    #[serde(rename = "initMarginReqRr")]
    pub init_margin_req_rr: String,
    #[serde(rename = "maintMarginReqRr")]
    pub maint_margin_req_rr: String,
    #[serde(rename = "riskLimitRv")]
    pub risk_limit_rv: String,
    #[serde(rename = "size")]
    pub size: String,
    #[serde(rename = "valueRv")]
    pub value_rv: String,
    #[serde(rename = "avgEntryPriceRp")]
    pub avg_entry_price_rp: String,
    #[serde(rename = "avgEntryPrice")]
    pub avg_entry_price: String,
    #[serde(rename = "posCostRv")]
    pub pos_cost_rv: String,
    #[serde(rename = "assignedPosBalanceRv")]
    pub assigned_pos_balance_rv: String,
    #[serde(rename = "bankruptCommRv")]
    pub bankrupt_comm_rv: String,
    #[serde(rename = "bankruptPriceRp")]
    pub bankrupt_price_rp: String,
    #[serde(rename = "positionMarginRv")]
    pub position_margin_rv: String,
    #[serde(rename = "liquidationPriceRp")]
    pub liquidation_price_rp: String,
    #[serde(rename = "deleveragePercentileRr")]
    pub deleverage_percentile_rr: String,
    #[serde(rename = "buyValueToCostRr")]
    pub buy_value_to_cost_rr: String,
    #[serde(rename = "sellValueToCostRr")]
    pub sell_value_to_cost_rr: String,
    #[serde(rename = "markPriceRp")]
    pub mark_price_rp: String,
    #[serde(rename = "estimatedOrdLossRv")]
    pub estimated_ord_loss_rv: String,
    #[serde(rename = "usedBalanceRv")]
    pub used_balance_rv: String,
    #[serde(rename = "cumClosedPnlRv")]
    pub cum_closed_pnl_rv: String,
    #[serde(rename = "cumFundingFeeRv")]
    pub cum_funding_fee_rv: String,
    #[serde(rename = "cumTransactFeeRv")]
    pub cum_transact_fee_rv: String,
    #[serde(rename = "transactTimeNs")]
    pub transact_time_ns: f64,
    #[serde(rename = "takerFeeRateRr")]
    pub taker_fee_rate_rr: String,
    #[serde(rename = "makerFeeRateRr")]
    pub maker_fee_rate_rr: String,
    #[serde(rename = "term")]
    pub term: f64,
    #[serde(rename = "lastTermEndTimeNs")]
    pub last_term_end_time_ns: f64,
    #[serde(rename = "lastFundingTimeNs")]
    pub last_funding_time_ns: f64,
    #[serde(rename = "curTermRealisedPnlRv")]
    pub cur_term_realised_pnl_rv: String,
    #[serde(rename = "execSeq")]
    pub exec_seq: f64,
    #[serde(rename = "posSide")]
    pub pos_side: String,
    #[serde(rename = "posMode")]
    pub pos_mode: String,
    #[serde(rename = "buyLeavesValueRv")]
    pub buy_leaves_value_rv: String,
    #[serde(rename = "sellLeavesValueRv")]
    pub sell_leaves_value_rv: String,
}
impl<'de> Deserialize<'de> for PhemexPosition {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "userID")]
            user_id: f64,
            #[serde(rename = "accountID")]
            account_id: f64,
            #[serde(rename = "symbol")]
            symbol: String,
            #[serde(rename = "currency")]
            currency: String,
            #[serde(rename = "side")]
            side: String,
            #[serde(rename = "positionStatus")]
            position_status: String,
            #[serde(rename = "crossMargin")]
            cross_margin: bool,
            #[serde(rename = "leverageRr")]
            leverage_rr: String,
            #[serde(rename = "initMarginReqRr")]
            init_margin_req_rr: String,
            #[serde(rename = "maintMarginReqRr")]
            maint_margin_req_rr: String,
            #[serde(rename = "riskLimitRv")]
            risk_limit_rv: String,
            #[serde(rename = "size")]
            size: String,
            #[serde(rename = "valueRv")]
            value_rv: String,
            #[serde(rename = "avgEntryPriceRp")]
            avg_entry_price_rp: String,
            #[serde(rename = "avgEntryPrice")]
            avg_entry_price: String,
            #[serde(rename = "posCostRv")]
            pos_cost_rv: String,
            #[serde(rename = "assignedPosBalanceRv")]
            assigned_pos_balance_rv: String,
            #[serde(rename = "bankruptCommRv")]
            bankrupt_comm_rv: String,
            #[serde(rename = "bankruptPriceRp")]
            bankrupt_price_rp: String,
            #[serde(rename = "positionMarginRv")]
            position_margin_rv: String,
            #[serde(rename = "liquidationPriceRp")]
            liquidation_price_rp: String,
            #[serde(rename = "deleveragePercentileRr")]
            deleverage_percentile_rr: String,
            #[serde(rename = "buyValueToCostRr")]
            buy_value_to_cost_rr: String,
            #[serde(rename = "sellValueToCostRr")]
            sell_value_to_cost_rr: String,
            #[serde(rename = "markPriceRp")]
            mark_price_rp: String,
            #[serde(rename = "estimatedOrdLossRv")]
            estimated_ord_loss_rv: String,
            #[serde(rename = "usedBalanceRv")]
            used_balance_rv: String,
            #[serde(rename = "cumClosedPnlRv")]
            cum_closed_pnl_rv: String,
            #[serde(rename = "cumFundingFeeRv")]
            cum_funding_fee_rv: String,
            #[serde(rename = "cumTransactFeeRv")]
            cum_transact_fee_rv: String,
            #[serde(rename = "transactTimeNs")]
            transact_time_ns: f64,
            #[serde(rename = "takerFeeRateRr")]
            taker_fee_rate_rr: String,
            #[serde(rename = "makerFeeRateRr")]
            maker_fee_rate_rr: String,
            #[serde(rename = "term")]
            term: f64,
            #[serde(rename = "lastTermEndTimeNs")]
            last_term_end_time_ns: f64,
            #[serde(rename = "lastFundingTimeNs")]
            last_funding_time_ns: f64,
            #[serde(rename = "curTermRealisedPnlRv")]
            cur_term_realised_pnl_rv: String,
            #[serde(rename = "execSeq")]
            exec_seq: f64,
            #[serde(rename = "posSide")]
            pos_side: String,
            #[serde(rename = "posMode")]
            pos_mode: String,
            #[serde(rename = "buyLeavesValueRv")]
            buy_leaves_value_rv: String,
            #[serde(rename = "sellLeavesValueRv")]
            sell_leaves_value_rv: String,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            user_id: wire.user_id,
            account_id: wire.account_id,
            symbol: wire.symbol,
            currency: wire.currency,
            side: wire.side,
            position_status: wire.position_status,
            cross_margin: wire.cross_margin,
            leverage_rr: wire.leverage_rr,
            init_margin_req_rr: wire.init_margin_req_rr,
            maint_margin_req_rr: wire.maint_margin_req_rr,
            risk_limit_rv: wire.risk_limit_rv,
            size: wire.size,
            value_rv: wire.value_rv,
            avg_entry_price_rp: wire.avg_entry_price_rp,
            avg_entry_price: wire.avg_entry_price,
            pos_cost_rv: wire.pos_cost_rv,
            assigned_pos_balance_rv: wire.assigned_pos_balance_rv,
            bankrupt_comm_rv: wire.bankrupt_comm_rv,
            bankrupt_price_rp: wire.bankrupt_price_rp,
            position_margin_rv: wire.position_margin_rv,
            liquidation_price_rp: wire.liquidation_price_rp,
            deleverage_percentile_rr: wire.deleverage_percentile_rr,
            buy_value_to_cost_rr: wire.buy_value_to_cost_rr,
            sell_value_to_cost_rr: wire.sell_value_to_cost_rr,
            mark_price_rp: wire.mark_price_rp,
            estimated_ord_loss_rv: wire.estimated_ord_loss_rv,
            used_balance_rv: wire.used_balance_rv,
            cum_closed_pnl_rv: wire.cum_closed_pnl_rv,
            cum_funding_fee_rv: wire.cum_funding_fee_rv,
            cum_transact_fee_rv: wire.cum_transact_fee_rv,
            transact_time_ns: wire.transact_time_ns,
            taker_fee_rate_rr: wire.taker_fee_rate_rr,
            maker_fee_rate_rr: wire.maker_fee_rate_rr,
            term: wire.term,
            last_term_end_time_ns: wire.last_term_end_time_ns,
            last_funding_time_ns: wire.last_funding_time_ns,
            cur_term_realised_pnl_rv: wire.cur_term_realised_pnl_rv,
            exec_seq: wire.exec_seq,
            pos_side: wire.pos_side,
            pos_mode: wire.pos_mode,
            buy_leaves_value_rv: wire.buy_leaves_value_rv,
            sell_leaves_value_rv: wire.sell_leaves_value_rv,
        })
    }
}
impl JsNumberOutput for PhemexPosition {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { user_id, account_id, symbol, currency, side, position_status, cross_margin, leverage_rr, init_margin_req_rr, maint_margin_req_rr, risk_limit_rv, size, value_rv, avg_entry_price_rp, avg_entry_price, pos_cost_rv, assigned_pos_balance_rv, bankrupt_comm_rv, bankrupt_price_rp, position_margin_rv, liquidation_price_rp, deleverage_percentile_rr, buy_value_to_cost_rr, sell_value_to_cost_rr, mark_price_rp, estimated_ord_loss_rv, used_balance_rv, cum_closed_pnl_rv, cum_funding_fee_rv, cum_transact_fee_rv, transact_time_ns, taker_fee_rate_rr, maker_fee_rate_rr, term, last_term_end_time_ns, last_funding_time_ns, cur_term_realised_pnl_rv, exec_seq, pos_side, pos_mode, buy_leaves_value_rv, sell_leaves_value_rv });
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PositionsResponse {
    #[serde(rename = "account")]
    pub account: PhemexAccount,
    #[serde(rename = "positions")]
    pub positions: Vec<PhemexPosition>,
}
impl<'de> Deserialize<'de> for PositionsResponse {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(rename = "account")]
            account: PhemexAccount,
            #[serde(rename = "positions")]
            positions: Vec<PhemexPosition>,
        }
        let wire = Wire::deserialize(safe_cex_types::strict_decode::object(d))?;
        Ok(Self {
            account: wire.account,
            positions: wire.positions,
        })
    }
}
impl JsNumberOutput for PositionsResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { account, positions });
    }
}

/// The declared intersection, with metadata reserved outside the payload.
/// Payload types must not declare their own `code` or `msg` fields. This is a
/// caller precondition, not a runtime disjointness check: optional/defaulted
/// overlaps and manually constructed duplicate metadata are unsupported.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhemexApiResponse<T> {
    pub code: f64,
    pub msg: String,
    #[serde(flatten)]
    pub payload: T,
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for PhemexApiResponse<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{DeserializeSeed, Error, IgnoredAny, MapAccess, Visitor, value};
        use std::{fmt, marker::PhantomData};

        // Forward payload fields to their original deserializer. Serde's derive
        // for flatten first buffers them through Content, which sees lexical
        // JSON numbers as maps under arbitrary_precision and loses typed f64s.
        struct PayloadFields<A> {
            map: A,
            code: Option<f64>,
            msg: Option<String>,
        }
        impl<'de, A: MapAccess<'de>> MapAccess<'de> for PayloadFields<A> {
            type Error = A::Error;
            fn next_key_seed<K: DeserializeSeed<'de>>(
                &mut self,
                seed: K,
            ) -> Result<Option<K::Value>, Self::Error> {
                while let Some(key) = self.map.next_key::<String>()? {
                    match key.as_str() {
                        "code" => {
                            if self.code.is_some() {
                                return Err(Error::duplicate_field("code"));
                            }
                            self.code = Some(self.map.next_value()?);
                        }
                        "msg" => {
                            if self.msg.is_some() {
                                return Err(Error::duplicate_field("msg"));
                            }
                            self.msg = Some(self.map.next_value()?);
                        }
                        _ => {
                            return seed
                                .deserialize(value::StringDeserializer::<A::Error>::new(key))
                                .map(Some);
                        }
                    }
                }
                Ok(None)
            }
            fn next_value_seed<V: DeserializeSeed<'de>>(
                &mut self,
                seed: V,
            ) -> Result<V::Value, Self::Error> {
                self.map.next_value_seed(seed)
            }
        }
        struct ResponseVisitor<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for ResponseVisitor<T> {
            type Value = PhemexApiResponse<T>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Phemex response object")
            }
            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let mut fields = PayloadFields {
                    map,
                    code: None,
                    msg: None,
                };
                let payload = T::deserialize(value::MapAccessDeserializer::new(&mut fields))?;
                // A custom payload may finish early. Still inspect metadata and
                // consume the remaining original map, as the former derive did.
                while fields.next_key::<IgnoredAny>()?.is_some() {
                    fields.next_value::<IgnoredAny>()?;
                }
                Ok(PhemexApiResponse {
                    code: fields.code.ok_or_else(|| Error::missing_field("code"))?,
                    msg: fields.msg.ok_or_else(|| Error::missing_field("msg"))?,
                    payload,
                })
            }
        }
        d.deserialize_map(ResponseVisitor(PhantomData))
    }
}
impl<T: JsNumberOutput> JsNumberOutput for PhemexApiResponse<T> {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { code, msg, payload });
    }
}
