//! Borrowed account records and exact venue identifiers.
use safe_cex_engine::de::{number, optional_number_or_blank};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;
use std::fmt;

pub fn identifier<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id {
        Text(String),
        Unsigned(u64),
        Signed(i64),
    }
    Ok(match Option::<Id>::deserialize(d)? {
        None => String::new(),
        Some(Id::Text(v)) => v,
        Some(Id::Unsigned(v)) => v.to_string(),
        Some(Id::Signed(v)) => v.to_string(),
    })
}

/// Text field where an explicit `null` reads as absent.
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Cow<'de, str>, D::Error> {
    struct Text;
    impl<'de> serde::de::Visitor<'de> for Text {
        type Value = Cow<'de, str>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string or null")
        }
        fn visit_borrowed_str<E: serde::de::Error>(self, s: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(s))
        }
        fn visit_str<E: serde::de::Error>(self, s: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(s.into()))
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(""))
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(""))
        }
        fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
            d.deserialize_any(self)
        }
    }
    d.deserialize_option(Text)
}

fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(Option::<bool>::deserialize(d)?.unwrap_or(false))
}

/// A record list that keeps every well-formed row and labels the rest.
pub struct Rows<T> {
    pub rows: Vec<T>,
    pub skipped: Vec<String>,
}

impl<T> std::ops::Deref for Rows<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        &self.rows
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Rows<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut rows = vec![];
        let mut skipped = vec![];
        for item in Vec::<&'de RawValue>::deserialize(d)? {
            match serde_json::from_str::<T>(item.get()) {
                Ok(row) => rows.push(row),
                Err(_) => skipped.push(row_key(item.get()).label()),
            }
        }
        Ok(Self { rows, skipped })
    }
}

/// Identity of a record that failed to parse, for logging and state retention.
#[derive(Default)]
pub struct RowKey {
    pub id: String,
    pub symbol: String,
}

impl RowKey {
    pub fn label(&self) -> String {
        match (self.id.trim(), self.symbol.trim()) {
            ("", "") => "unidentified".into(),
            ("", symbol) => symbol.into(),
            (id, _) => id.into(),
        }
    }
}

/// Reads `id` and `symbol` independently, so one malformed field keeps the other.
pub fn row_key(item: &str) -> RowKey {
    #[derive(Deserialize, Default)]
    #[serde(default)]
    struct Raw<'a> {
        #[serde(borrow)]
        id: Option<&'a RawValue>,
        #[serde(borrow)]
        symbol: Option<&'a RawValue>,
    }
    fn scalar(raw: Option<&RawValue>) -> String {
        let Some(raw) = raw else {
            return String::new();
        };
        if let Ok(text) = serde_json::from_str::<String>(raw.get()) {
            return text;
        }
        if serde_json::from_str::<f64>(raw.get()).is_ok() {
            return raw.get().trim().to_owned();
        }
        String::new()
    }
    let raw = serde_json::from_str::<Raw<'_>>(item).unwrap_or_default();
    RowKey {
        id: scalar(raw.id),
        symbol: scalar(raw.symbol),
    }
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
    #[serde(borrow, deserialize_with = "text")]
    pub symbol: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
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
    #[serde(deserialize_with = "identifier")]
    pub trigger_price_type: String,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(deserialize_with = "identifier")]
    pub id: String,
    #[serde(borrow, deserialize_with = "text")]
    pub symbol: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub side: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub status: Cow<'a, str>,
    #[serde(borrow, rename = "type", deserialize_with = "text")]
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
    #[serde(borrow, deserialize_with = "text")]
    pub time_in_force: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub trigger_price_type: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub client_order_id: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub client_id: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub cancel_reason: Cow<'a, str>,
    #[serde(deserialize_with = "flag")]
    pub reduce_only: bool,
    #[serde(deserialize_with = "flag")]
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
    #[serde(borrow, deserialize_with = "text")]
    pub symbol: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
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
    #[serde(borrow, deserialize_with = "text")]
    pub token: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
    pub fee_currency: Cow<'a, str>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Contents<'a> {
    #[serde(borrow)]
    pub fills: Option<Rows<FillRow<'a>>>,
    #[serde(borrow)]
    pub orders: Option<Rows<OrderRow<'a>>>,
    #[serde(borrow)]
    pub positions: Option<Rows<PositionRow<'a>>>,
    #[serde(borrow)]
    pub wallets: Option<Rows<WalletRow<'a>>>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct WalletRow<'a> {
    #[serde(borrow, deserialize_with = "text")]
    pub account_id: Cow<'a, str>,
    #[serde(borrow, deserialize_with = "text")]
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
    #[serde(default, borrow, deserialize_with = "text")]
    pub topic: Cow<'a, str>,
    #[serde(default, borrow, deserialize_with = "text")]
    pub op: Cow<'a, str>,
    #[serde(default, borrow, rename = "type", deserialize_with = "text")]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow, deserialize_with = "text")]
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
