//! Raw flat-symbol metadata, with the TS resolver's leg selection and defaults.
use crate::wire::number;
use safe_cex_engine::model::{MarginType, PositionMode};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Envelope {
    data: Option<Data>,
}
#[derive(Deserialize)]
struct Data {
    positions: Option<Vec<Row>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    symbol: String,
    #[serde(default, alias = "pos_side")]
    pos_side: String,
    #[serde(default, alias = "pos_mode")]
    pos_mode: String,
    #[serde(default, deserialize_with = "number", alias = "leverage_rr")]
    leverage_rr: f64,
    #[serde(default, alias = "cross_margin")]
    cross_margin: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Rails {
    buy_leverage: f64,
    sell_leverage: f64,
    margin_type: MarginType,
    position_mode: PositionMode,
}
pub(crate) fn parse(status: u16, body: &str, symbol: &str) -> Result<String, String> {
    if !(200..300).contains(&status) {
        return Err(format!("Phemex leverage context HTTP {status}"));
    }
    let envelope: Envelope =
        serde_json::from_str(body).map_err(|_| "Phemex leverage context response is invalid")?;
    let rows: Vec<_> = envelope
        .data
        .and_then(|data| data.positions)
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.symbol.to_ascii_uppercase() == symbol)
        .collect();
    let context = rows.first().map(|first| {
        let long = rows
            .iter()
            .find(|row| row.pos_side == "Long")
            .unwrap_or(first);
        let short = rows
            .iter()
            .find(|row| row.pos_side == "Short")
            .unwrap_or(first);
        Rails {
            buy_leverage: long.leverage_rr.abs(),
            sell_leverage: short.leverage_rr.abs(),
            margin_type: if long.leverage_rr < 0. || long.cross_margin {
                MarginType::Cross
            } else {
                MarginType::Isolated
            },
            position_mode: if long.pos_mode == "Hedged" {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            },
        }
    });
    serde_json::to_string(&context).map_err(|error| error.to_string())
}
