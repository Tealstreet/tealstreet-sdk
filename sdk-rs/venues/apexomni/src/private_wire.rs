//! Borrowed account records and exact venue identifiers.
use crate::public_wire::{number, optional_number_or_blank};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

pub fn identifier<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Unsigned(u64),
        Signed(i64),
    }
    Ok(match Id::deserialize(d)? {
        Id::Text(v) => v,
        Id::Unsigned(v) => v.to_string(),
        Id::Signed(v) => v.to_string(),
    })
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub data: &'a RawValue,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata<'a> {
    #[serde(default, deserialize_with = "identifier")]
    pub id: String,
    #[serde(borrow)]
    pub positions: &'a RawValue,
    pub contract_account: Option<Fees>,
}

#[derive(Deserialize)]
pub struct Identity {
    #[serde(deserialize_with = "identifier")]
    pub id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fees {
    pub maker_fee_rate: String,
    pub taker_fee_rate: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow {
    #[serde(deserialize_with = "number")]
    pub total_equity_value: f64,
    #[serde(deserialize_with = "number")]
    pub initial_margin: f64,
    #[serde(deserialize_with = "number")]
    pub maintenance_margin: f64,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "avg_price")]
    pub avg_price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "entry_price")]
    pub entry_price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub size: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "custom_imr")]
    pub custom_imr: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "custom_initial_margin_rate"
    )]
    pub custom_initial_margin_rate: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "unrealised_pnl"
    )]
    pub unrealised_pnl: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "cur_realised_pnl"
    )]
    pub cur_realised_pnl: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "realized_pnl")]
    pub realized_pnl: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "liq_price")]
    pub liq_price: Option<f64>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Leg {
    #[serde(deserialize_with = "optional_number_or_blank", alias = "trigger_price")]
    pub trigger_price: Option<f64>,
    pub trigger_price_type: String,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(deserialize_with = "identifier")]
    pub id: String,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(borrow, rename = "type")]
    pub kind: Cow<'a, str>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub size: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "cum_success_fill_size"
    )]
    pub cum_success_fill_size: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "cum_success_fill_fee"
    )]
    pub cum_success_fill_fee: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank", alias = "trigger_price")]
    pub trigger_price: Option<f64>,
    #[serde(
        deserialize_with = "optional_number_or_blank",
        alias = "latest_match_fill_price"
    )]
    pub latest_match_fill_price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub average_price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub updated_at: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub updated_time: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub created_at: Option<f64>,
    #[serde(borrow)]
    pub time_in_force: Cow<'a, str>,
    #[serde(borrow)]
    pub trigger_price_type: Cow<'a, str>,
    #[serde(borrow)]
    pub client_order_id: Cow<'a, str>,
    #[serde(borrow)]
    pub client_id: Cow<'a, str>,
    #[serde(borrow)]
    pub cancel_reason: Cow<'a, str>,
    pub reduce_only: bool,
    pub post_only: bool,
    pub open_sl_params: Option<Leg>,
    pub open_sl_param: Option<Leg>,
    pub open_tp_params: Option<Leg>,
    pub open_tp_param: Option<Leg>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct FillRow<'a> {
    #[serde(deserialize_with = "identifier")]
    pub id: String,
    #[serde(deserialize_with = "identifier")]
    pub match_fill_id: String,
    #[serde(deserialize_with = "identifier")]
    pub order_id: String,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub price: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub size: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub fee: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub created_at: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub updated_at: Option<f64>,
    #[serde(borrow)]
    pub token: Cow<'a, str>,
    #[serde(borrow)]
    pub fee_currency: Cow<'a, str>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Contents<'a> {
    #[serde(borrow)]
    pub fills: Option<Vec<FillRow<'a>>>,
    #[serde(borrow)]
    pub orders: Option<Vec<OrderRow<'a>>>,
    #[serde(borrow)]
    pub positions: Option<Vec<PositionRow<'a>>>,
    #[serde(borrow)]
    pub wallets: Option<Vec<WalletRow<'a>>>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WalletRow<'a> {
    #[serde(borrow)]
    pub account_id: Cow<'a, str>,
    #[serde(borrow)]
    pub token: Cow<'a, str>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub balance: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub pending_deposit_amount: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub pending_withdraw_amount: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub pending_transfer_in_amount: Option<f64>,
    #[serde(deserialize_with = "optional_number_or_blank")]
    pub pending_transfer_out_amount: Option<f64>,
}

impl WalletRow<'_> {
    pub fn valid(&self) -> bool {
        !self.account_id.trim().is_empty()
            && matches!(self.token.as_ref(), "USDC" | "USDT")
            && self.balance.is_some_and(f64::is_finite)
            && [
                self.pending_deposit_amount,
                self.pending_withdraw_amount,
                self.pending_transfer_in_amount,
                self.pending_transfer_out_amount,
            ]
            .iter()
            .all(|amount| amount.is_none_or(|amount| amount >= 0.0))
    }
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default, borrow)]
    pub topic: Cow<'a, str>,
    #[serde(default, borrow)]
    pub op: Cow<'a, str>,
    #[serde(default, borrow, rename = "type")]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow)]
    pub ret_msg: Cow<'a, str>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default, borrow)]
    pub ping: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub pong: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub args: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub contents: Option<Contents<'a>>,
}
