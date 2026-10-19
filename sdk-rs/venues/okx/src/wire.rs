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
    pub inst_id: &'a str,
    pub ct_type: &'a str,
    pub ct_val: &'a str,
    pub ct_val_ccy: &'a str,
    pub settle_ccy: &'a str,
    pub state: &'a str,
    pub lot_sz: &'a str,
    pub tick_sz: &'a str,
    pub min_sz: &'a str,
    pub max_iceberg_sz: &'a str,
    pub max_lmt_sz: &'a str,
    pub max_mkt_sz: &'a str,
    pub max_stop_sz: &'a str,
    pub max_trigger_sz: &'a str,
    pub max_twap_sz: &'a str,
    pub lever: &'a str,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Prices<'a> {
    pub inst_id: &'a str,
    pub bid_px: &'a str,
    pub ask_px: &'a str,
    pub last: &'a str,
    pub open24h: &'a str,
    pub vol_ccy24h: &'a str,
    pub vol24h: &'a str,
    pub mark_px: &'a str,
    pub idx_px: &'a str,
    pub oi_ccy: &'a str,
    pub funding_rate: &'a str,
    pub funding_time: &'a str,
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
    (row.len() >= 8).then(|| Candle {
        timestamp: parse(row[0]) / 1000.0,
        open: parse(row[1]),
        high: parse(row[2]),
        low: parse(row[3]),
        close: parse(row[4]),
        volume: parse(row[7]),
    })
}
