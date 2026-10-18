use safe_cex_types::{Market, Position, Ticker};
use safe_cex_venue_bybit::exchange_state::*;
use serde::Deserialize;
use serde_json::Value;
use std::io::{self, Read};
#[derive(Deserialize)]
struct TickerCase {
    #[serde(default)]
    spot: bool,
    #[serde(default, rename = "inverseClass")]
    inverse_class: bool,
    raw: Option<Vec<Value>>,
    markets: Vec<Market>,
    cached: Vec<Ticker>,
}
#[derive(Deserialize)]
struct Cases {
    positions: Vec<Vec<Position>>,
    tickers: Vec<TickerCase>,
}
fn outcome<T: serde::Serialize>(value: Result<T, &str>) -> Value {
    match value {
        Ok(value) => serde_json::to_value(value).unwrap(),
        Err(_) => serde_json::json!({"error":true}),
    }
}
fn main() {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let cases: Cases = serde_json::from_str(&input).unwrap();
    let wire: Value = serde_json::from_str(&input).unwrap();
    for (i, rows) in cases.positions.iter().enumerate() {
        for (j, row) in rows.iter().enumerate() {
            assert_eq!(
                row.id,
                wire["positions"][i][j]["id"].as_str().unwrap(),
                "native position identity id"
            );
            assert_eq!(
                row.symbol,
                wire["positions"][i][j]["symbol"].as_str().unwrap(),
                "native position identity symbol"
            );
            assert_eq!(
                row.entry_price,
                wire["positions"][i][j]["entryPrice"].as_f64().unwrap(),
                "native position identity entry_price"
            );
            assert_eq!(
                row.notional,
                wire["positions"][i][j]["notional"].as_f64().unwrap(),
                "native position identity notional"
            );
            assert_eq!(
                row.leverage,
                wire["positions"][i][j]["leverage"].as_f64().unwrap(),
                "native position identity leverage"
            );
            assert_eq!(
                row.unrealized_pnl,
                wire["positions"][i][j]["unrealizedPnl"].as_f64().unwrap(),
                "native position identity unrealized_pnl"
            );
            assert_eq!(
                row.contracts,
                wire["positions"][i][j]["contracts"].as_f64().unwrap(),
                "native position identity contracts"
            );
            assert_eq!(
                row.liquidation_price,
                wire["positions"][i][j]["liquidationPrice"]
                    .as_f64()
                    .unwrap(),
                "native position identity liquidation_price"
            );
        }
    }
    for (i, case) in cases.tickers.iter().enumerate() {
        for (j, row) in case.cached.iter().enumerate() {
            assert_eq!(
                row.id,
                wire["tickers"][i]["cached"][j]["id"].as_str().unwrap(),
                "native cached ticker identity id"
            );
            assert_eq!(
                row.symbol,
                wire["tickers"][i]["cached"][j]["symbol"].as_str().unwrap(),
                "native cached ticker identity symbol"
            );
            assert_eq!(
                row.bid,
                wire["tickers"][i]["cached"][j]["bid"].as_f64().unwrap(),
                "native cached ticker identity bid"
            );
            assert_eq!(
                row.ask,
                wire["tickers"][i]["cached"][j]["ask"].as_f64().unwrap(),
                "native cached ticker identity ask"
            );
            assert_eq!(
                row.last,
                wire["tickers"][i]["cached"][j]["last"].as_f64().unwrap(),
                "native cached ticker identity last"
            );
            assert_eq!(
                row.mark,
                wire["tickers"][i]["cached"][j]["mark"].as_f64().unwrap(),
                "native cached ticker identity mark"
            );
            assert_eq!(
                row.index.unwrap(),
                wire["tickers"][i]["cached"][j]["index"].as_f64().unwrap(),
                "native cached ticker identity index"
            );
            assert_eq!(
                row.percentage,
                wire["tickers"][i]["cached"][j]["percentage"]
                    .as_f64()
                    .unwrap(),
                "native cached ticker identity percentage"
            );
            assert_eq!(
                row.open_interest,
                wire["tickers"][i]["cached"][j]["openInterest"]
                    .as_f64()
                    .unwrap(),
                "native cached ticker identity open_interest"
            );
            assert_eq!(
                row.funding_rate,
                wire["tickers"][i]["cached"][j]["fundingRate"]
                    .as_f64()
                    .unwrap(),
                "native cached ticker identity funding_rate"
            );
            assert_eq!(
                row.funding_time,
                wire["tickers"][i]["cached"][j]["fundingTime"]
                    .as_f64()
                    .unwrap(),
                "native cached ticker identity funding_time"
            );
            assert_eq!(
                row.volume,
                wire["tickers"][i]["cached"][j]["volume"].as_f64().unwrap(),
                "native cached ticker identity volume"
            );
            assert_eq!(
                row.quote_volume,
                wire["tickers"][i]["cached"][j]["quoteVolume"]
                    .as_f64()
                    .unwrap(),
                "native cached ticker identity quote_volume"
            );
        }
    }
    let positions: Vec<_> = cases
        .positions
        .into_iter()
        .map(|positions| {
            let scores: Vec<_> = positions
                .iter()
                .map(|p| {
                    (
                        outcome(get_position_updated_time(p)),
                        outcome(get_position_reliability_score(p)),
                    )
                })
                .collect();
            serde_json::json!({"scores":scores,"positions":outcome(dedupe_positions_by_id(positions))})
        })
        .collect();
    let tickers: Vec<_> = cases
        .tickers
        .into_iter()
        .map(|case| {
            let parser = if case.spot {
                parse_spot_tickers
            } else if case.inverse_class {
                parse_inverse_class_tickers
            } else {
                parse_tickers
            };
            let Ok(rows) = parser(case.raw.as_deref(), &case.markets, &case.cached) else {
                return serde_json::json!({"error":true});
            };
            let negative_zero: Vec<_> = rows
                .iter()
                .map(|t| {
                    [
                        t.bid,
                        t.ask,
                        t.last,
                        t.mark,
                        t.index.unwrap(),
                        t.percentage,
                        t.open_interest,
                        t.funding_rate,
                        t.funding_time,
                        t.volume,
                        t.quote_volume,
                    ]
                    .map(|v| v == 0. && v.is_sign_negative())
                })
                .collect();
            let nonfinite: Vec<_> = rows
                .iter()
                .map(|t| {
                    [
                        t.bid,
                        t.ask,
                        t.last,
                        t.mark,
                        t.index.unwrap(),
                        t.percentage,
                        t.open_interest,
                        t.funding_rate,
                        t.funding_time,
                        t.volume,
                        t.quote_volume,
                    ]
                    .map(|value| {
                        if value.is_nan() {
                            "NaN"
                        } else if value == f64::INFINITY {
                            "Infinity"
                        } else if value == f64::NEG_INFINITY {
                            "-Infinity"
                        } else {
                            "finite"
                        }
                    })
                })
                .collect();
            serde_json::json!({"rows":rows,"negativeZero":negative_zero,"nonfinite":nonfinite})
        })
        .collect();
    println!(
        "{}",
        serde_json::json!({"positions":positions,"tickers":tickers})
    );
}
