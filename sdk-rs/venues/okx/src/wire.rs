//! Borrowed OKX v5 wire records. Unused fields are skipped by serde.
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: &'a str,
    #[serde(default)]
    pub msg: &'a str,
    #[serde(borrow, default)]
    pub data: Option<&'a RawValue>,
}

pub fn envelope(text: &str) -> Result<&str, String> {
    let e: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if e.code != "0" {
        return Err(format!("OKX {}: {}", e.code, e.msg));
    }
    e.data
        .map(RawValue::get)
        .ok_or_else(|| "missing OKX data".into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Instrument<'a> {
    #[serde(deserialize_with = "nullable")]
    pub inst_id: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub inst_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub rule_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub uly: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub inst_family: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub base_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub quote_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_type: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_val: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ct_val_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub settle_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub state: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub lot_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub tick_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub min_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_iceberg_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_lmt_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_mkt_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_stop_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_trigger_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub max_twap_sz: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub lever: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Prices<'a> {
    #[serde(deserialize_with = "nullable")]
    pub inst_id: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub bid_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub ask_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub last: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub open24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub vol_ccy24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub vol24h: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub mark_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub idx_px: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub oi_ccy: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub funding_rate: &'a str,
    #[serde(deserialize_with = "nullable")]
    pub funding_time: &'a str,
}

// OKX sends `null` for some unset quote/catalog fields; treat it like a blank.
fn nullable<'de: 'a, 'a, D: serde::Deserializer<'de>>(d: D) -> Result<&'a str, D::Error> {
    Ok(Option::<&'a str>::deserialize(d)?.unwrap_or_default())
}

/// Identifies a catalog row that failed to decode as an [`Instrument`].
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentId<'a> {
    #[serde(default, deserialize_with = "nullable")]
    pub inst_id: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Arg<'a> {
    pub channel: &'a str,
    pub inst_id: &'a str,
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default)]
    pub event: &'a str,
    #[serde(default)]
    pub code: &'a str,
    #[serde(default)]
    pub msg: &'a str,
    #[serde(default)]
    pub arg: Arg<'a>,
    #[serde(default)]
    pub action: &'a str,
    #[serde(borrow, default)]
    pub data: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth<'a> {
    pub seq_id: i64,
    #[serde(default)]
    pub prev_seq_id: Option<i64>,
    #[serde(borrow)]
    pub bids: Vec<Vec<&'a str>>,
    #[serde(borrow)]
    pub asks: Vec<Vec<&'a str>>,
}

pub fn candle(row: &[&str]) -> Option<safe_cex_engine::model::Candle> {
    use safe_cex_engine::{model::Candle, num::parse};
    (row.len() >= 8
        && [0, 1, 2, 3, 4, 7]
            .iter()
            .all(|&i| row[i].parse::<f64>().is_ok_and(|v| v.is_finite())))
    .then(|| Candle {
        timestamp: parse(row[0]) / 1000.0,
        open: parse(row[1]),
        high: parse(row[2]),
        low: parse(row[3]),
        close: parse(row[4]),
        volume: parse(row[7]),
    })
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config<'a> {
    pub pos_mode: &'a str,
    pub acct_lv: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LeverageRow<'a> {
    pub inst_id: &'a str,
    pub mgn_mode: &'a str,
    pub lever: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PositionRow<'a> {
    pub inst_type: &'a str,
    pub margin: &'a str,
    pub inst_id: &'a str,
    pub pos: &'a str,
    pub pos_side: &'a str,
    pub avg_px: &'a str,
    pub mark_px: &'a str,
    pub ccy: &'a str,
    pub lever: &'a str,
    pub upl: &'a str,
    pub liq_px: &'a str,
    pub mgn_mode: &'a str,
    pub imr: &'a str,
    pub mmr: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct OrderRow<'a> {
    pub inst_type: &'a str,
    pub inst_id: &'a str,
    pub ord_id: &'a str,
    pub algo_id: &'a str,
    pub cl_ord_id: &'a str,
    pub ccy: &'a str,
    pub algo_cl_ord_id: &'a str,
    pub ord_type: &'a str,
    pub state: &'a str,
    pub side: &'a str,
    pub sz: &'a str,
    pub acc_fill_sz: &'a str,
    pub px: &'a str,
    pub avg_px: &'a str,
    pub fill_px: &'a str,
    pub fill_sz: &'a str,
    pub fill_time: &'a str,
    pub u_time: &'a str,
    pub trade_id: &'a str,
    pub trigger_px: &'a str,
    pub trigger_px_type: &'a str,
    pub sl_trigger_px: &'a str,
    pub sl_ord_px: &'a str,
    pub sl_trigger_px_type: &'a str,
    pub tp_trigger_px: &'a str,
    pub tp_ord_px: &'a str,
    pub tp_trigger_px_type: &'a str,
    pub order_px: &'a str,
    pub ord_px: &'a str,
    pub callback_spread: &'a str,
    pub move_trigger_px: &'a str,
    pub last: &'a str,
    pub lever: &'a str,
    #[serde(deserialize_with = "boolean")]
    pub reduce_only: bool,
}

fn boolean<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Bool<'a> {
        Boolean(bool),
        Text(&'a str),
    }
    Ok(match Bool::deserialize(d)? {
        Bool::Boolean(b) => b,
        Bool::Text(s) => s == "true",
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Equity<'a> {
    pub ccy: &'a str,
    pub eq: &'a str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Risk<'a> {
    #[serde(borrow)]
    pub bal_data: Vec<Equity<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPush<'a> {
    #[serde(default)]
    pub total_eq: &'a str,
    #[serde(borrow, default)]
    pub details: Vec<Equity<'a>>,
}
