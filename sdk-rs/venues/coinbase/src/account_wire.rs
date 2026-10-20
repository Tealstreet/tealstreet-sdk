use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default)]
pub struct Decimal(pub f64);
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Number<'a> {
            Text(#[serde(borrow)] Cow<'a, str>),
            Bare(f64),
        }
        let value = match Number::deserialize(d)? {
            Number::Bare(v) => v,
            Number::Text(s) => {
                let digits = s.strip_prefix('-').unwrap_or(&s);
                if digits.is_empty()
                    || digits.bytes().filter(|b| *b == b'.').count() > 1
                    || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                    || !digits.bytes().any(|b| b.is_ascii_digit())
                {
                    return Err(serde::de::Error::custom("invalid decimal"));
                }
                s.parse().map_err(serde::de::Error::custom)?
            }
        };
        if !value.is_finite() {
            return Err(serde::de::Error::custom("nonfinite decimal"));
        }
        Ok(Self(value))
    }
}
pub fn optional_decimal<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Decimal>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<'a> {
        Text(#[serde(borrow)] Cow<'a, str>),
        Number(f64),
        Null,
    }
    match Field::deserialize(d)? {
        Field::Null => Ok(None),
        Field::Text(s) if s.is_empty() => Ok(None),
        Field::Text(s) => {
            let value = s.parse::<f64>().map_err(serde::de::Error::custom)?;
            if value.is_finite() {
                Ok(Some(Decimal(value)))
            } else {
                Err(serde::de::Error::custom("nonfinite decimal"))
            }
        }
        Field::Number(value) => {
            if value.is_finite() {
                Ok(Some(Decimal(value)))
            } else {
                Err(serde::de::Error::custom("nonfinite decimal"))
            }
        }
    }
}
#[derive(Clone, Deserialize)]
pub struct Portfolio {
    pub uuid: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub deleted: bool,
}
#[derive(Deserialize)]
pub struct Portfolios {
    pub portfolios: Vec<Portfolio>,
}
#[derive(Deserialize)]
pub struct Money {
    pub value: Decimal,
}
#[derive(Deserialize)]
pub struct IntxBalance {
    pub summary: IntxSummary,
}
#[derive(Deserialize)]
pub struct IntxSummary {
    pub total_balance: Money,
    pub buying_power: Money,
}
#[derive(Deserialize)]
pub struct CfmBalance {
    pub balance_summary: CfmSummary,
}
#[derive(Deserialize)]
pub struct CfmSummary {
    pub total_usd_balance: Money,
    pub available_margin: Money,
    #[serde(default)]
    pub unrealized_pnl: Option<Money>,
}
#[derive(Deserialize)]
pub struct Breakdown {
    pub breakdown: BreakdownBody,
}
#[derive(Deserialize)]
pub struct BreakdownBody {
    pub spot_positions: Vec<SpotBalance>,
}
#[derive(Deserialize)]
pub struct SpotBalance {
    pub asset: String,
    pub total_balance_crypto: Decimal,
    pub available_to_trade_crypto: Decimal,
    #[serde(default, rename = "total_balance_fiat")]
    pub _total_balance_fiat: Option<Decimal>,
}
#[derive(Deserialize)]
pub struct IntxPosition<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    pub entry_vwap: Money,
    pub mark_price: Money,
    pub liquidation_price: Money,
    #[serde(borrow)]
    pub position_side: Cow<'a, str>,
    pub net_size: Decimal,
    #[serde(default)]
    pub leverage: Option<Decimal>,
}
#[derive(Deserialize)]
pub struct CfmPosition<'a> {
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    pub number_of_contracts: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
}
#[derive(Default, Deserialize)]
pub struct Page<'a> {
    #[serde(default)]
    pub has_next: bool,
    #[serde(default, borrow)]
    pub cursor: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    pub client_order_id: Cow<'a, str>,
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(default, borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, borrow)]
    pub order_side: Cow<'a, str>,
    #[serde(default, borrow)]
    pub order_type: Cow<'a, str>,
    #[serde(default)]
    pub filled_size: Option<Decimal>,
    #[serde(default)]
    pub cumulative_quantity: Option<Decimal>,
    #[serde(default)]
    pub leaves_quantity: Option<Decimal>,
    #[serde(default, borrow)]
    pub limit_price: Cow<'a, str>,
    #[serde(default, borrow)]
    pub stop_price: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub average_filled_price: Option<Decimal>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub avg_price: Option<Decimal>,
    #[serde(default, borrow)]
    pub order_configuration: Option<Configuration<'a>>,
    #[serde(default)]
    pub reduce_only: Option<bool>,
}
#[derive(Deserialize)]
pub struct Configuration<'a> {
    #[serde(default, borrow)]
    pub market_market_ioc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub market_market_fok: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_gtc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_gtd: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_fok: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub sor_limit_ioc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub stop_limit_stop_limit_gtc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub stop_limit_stop_limit_gtd: Option<Size<'a>>,
}
#[derive(Deserialize)]
pub struct Size<'a> {
    #[serde(default, borrow)]
    pub base_size: Cow<'a, str>,
    #[serde(default, borrow)]
    pub quote_size: Cow<'a, str>,
    #[serde(default, borrow)]
    pub limit_price: Cow<'a, str>,
    #[serde(default, borrow)]
    pub stop_price: Cow<'a, str>,
    #[serde(default)]
    pub post_only: bool,
}
#[derive(Deserialize)]
pub struct UserFrame<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    pub sequence_num: Option<u64>,
    #[serde(default, borrow)]
    pub events: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct UserEvent<'a> {
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(borrow)]
    pub orders: Option<Vec<&'a RawValue>>,
}
#[derive(Deserialize)]
pub struct EditSource<'a> {
    #[serde(default, borrow)]
    pub filled_size: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub cumulative_quantity: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub leaves_quantity: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub attached_order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    pub attached_order_configuration: Option<&'a RawValue>,
}
