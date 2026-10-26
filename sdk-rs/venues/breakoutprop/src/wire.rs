//! Borrowed DXtrade response contracts; timestamps on the engine boundary are milliseconds.
use safe_cex_engine::model::Interval;
use serde::Deserialize;
use std::borrow::Cow;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Login<'a> {
    #[serde(borrow)]
    pub session_token: Cow<'a, str>,
    #[serde(borrow)]
    pub timeout: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UserResponse<'a> {
    #[serde(borrow)]
    pub user_details: Vec<User<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct User<'a> {
    #[serde(borrow)]
    pub login: Cow<'a, str>,
    #[serde(borrow)]
    pub domain: Cow<'a, str>,
    #[serde(borrow)]
    pub accounts: Vec<Account<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Account<'a> {
    #[serde(borrow)]
    pub account: Cow<'a, str>,
    #[serde(borrow)]
    pub account_status: Cow<'a, str>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Instrument<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    pub price_increment: f64,
    pub lot_size: f64,
}
impl Instrument<'_> {
    pub fn owned(self) -> Instrument<'static> {
        Instrument {
            symbol: Cow::Owned(self.symbol.into_owned()),
            price_increment: self.price_increment,
            lot_size: self.lot_size,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Details<'a> {
    #[serde(borrow)]
    pub account: Cow<'a, str>,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub trading_status: Cow<'a, str>,
    pub min_order_size: f64,
    pub max_order_size: MaxSize,
    pub min_order_size_increment: f64,
    pub margin_rate: MarginRate,
}
impl Details<'_> {
    pub fn owned(self) -> Details<'static> {
        Details {
            account: Cow::Owned(self.account.into_owned()),
            symbol: Cow::Owned(self.symbol.into_owned()),
            trading_status: Cow::Owned(self.trading_status.into_owned()),
            min_order_size: self.min_order_size,
            max_order_size: self.max_order_size,
            min_order_size_increment: self.min_order_size_increment,
            margin_rate: self.margin_rate,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MaxSize {
    pub value: f64,
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MarginRate {
    pub tiers: Vec<MarginTier>,
}
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct MarginTier {
    pub value: f64,
}

#[derive(Deserialize)]
pub(crate) struct Quote<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    pub ask: f64,
    pub bid: f64,
    #[serde(borrow)]
    pub time: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bar<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(borrow)]
    pub candle_type: Cow<'a, str>,
    pub open: f64,
    pub close: f64,
    pub high: f64,
    pub low: f64,
    pub volume: f64,
    #[serde(borrow)]
    pub time: Cow<'a, str>,
}

pub(crate) fn timestamp(value: &str) -> Result<f64, String> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map(|date| date.unix_timestamp_nanos() as f64 / 1_000_000.0)
        .map_err(|_| "Invalid DXtrade timestamp".into())
}

pub(crate) fn iso_time(value: f64) -> Result<String, String> {
    if !value.is_finite() || value.abs() > 8_640_000_000_000_000.0 {
        return Err("Invalid DXtrade timestamp".into());
    }
    let date = OffsetDateTime::from_unix_timestamp_nanos(value.trunc() as i128 * 1_000_000)
        .map_err(|_| "Invalid DXtrade timestamp".to_owned())?
        .format(&Rfc3339)
        .map_err(|_| "Invalid DXtrade timestamp".to_owned())?;
    let (seconds, fraction) = date
        .trim_end_matches('Z')
        .split_once('.')
        .unwrap_or((date.trim_end_matches('Z'), ""));
    Ok(format!("{seconds}.{fraction:0<3}Z"))
}

/// DXtrade's native candle type and the interval in seconds.
pub(crate) fn interval(value: Interval) -> Option<(&'static str, f64)> {
    use Interval::*;
    let kind = match value {
        M1 => "m",
        M5 => "5m",
        M15 => "15m",
        M30 => "30m",
        H1 => "h",
        H2 => "2h",
        H4 => "4h",
        D1 => "d",
        W1 => "w",
        _ => return None,
    };
    Some((kind, value.ms() / 1000.0))
}
/// Live candles are built from ticks, so a few intervals with no REST type
/// are still served.
pub(crate) fn synthetic_interval_seconds(value: Interval) -> Option<f64> {
    use Interval::*;
    (matches!(value, M3 | H6 | H12) || interval(value).is_some()).then(|| value.ms() / 1000.0)
}
