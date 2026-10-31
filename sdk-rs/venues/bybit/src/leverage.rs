//! One symbol's leverage rails. `/v5/position/list` with a symbol answers even
//! with no position, unlike the settle-coin lists the account snapshot reads:
//! a flat row has `size` 0 and `side` "", and `leverage` "" under portfolio margin.
use crate::wire::PositionRow;
use crate::{BybitLinear, Category, Pending, Rest, Then, sign::Query};
use safe_cex_engine::model::{MarginType, PositionMode};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::{Cx, ErrorKind, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};

pub struct Read {
    context: Context,
    symbol: String,
    position_version: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Body<'a> {
    ret_code: i64,
    #[serde(default)]
    ret_msg: String,
    #[serde(borrow)]
    result: Option<Rows<'a>>,
}

#[derive(Deserialize)]
struct Rows<'a> {
    #[serde(borrow)]
    list: Vec<PositionRow<'a>>,
}

#[derive(Debug, PartialEq)]
struct Rails {
    buy: f64,
    sell: f64,
    hedged: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Fetched {
    buy_leverage: f64,
    sell_leverage: f64,
    margin_type: MarginType,
    position_mode: PositionMode,
}

fn leverage_of(row: &PositionRow<'_>, symbol: &str) -> Result<f64, String> {
    let text = row.leverage.as_deref().unwrap_or("");
    if text.is_empty() {
        return Err(format!(
            "Bybit reports no leverage for {symbol}: leverage does not apply under portfolio margin"
        ));
    }
    text.parse::<f64>()
        .ok()
        .filter(|leverage| leverage.is_finite() && *leverage > 0.0)
        .ok_or_else(|| format!("Bybit returned invalid leverage for {symbol}"))
}

/// One-way has one row at index 0; hedged has one row per side at 1 and 2.
fn rails(rows: &[PositionRow<'_>], symbol: &str) -> Result<Rails, String> {
    let (mut one_way, mut buy, mut sell) = (None, None, None);
    for row in rows.iter().filter(|row| row.symbol.as_ref() == symbol) {
        let slot = match row.position_idx {
            Some(0) => &mut one_way,
            Some(1) => &mut buy,
            Some(2) => &mut sell,
            _ => {
                return Err(format!(
                    "Bybit returned an invalid position row for {symbol}"
                ));
            }
        };
        if slot.replace(leverage_of(row, symbol)?).is_some() {
            return Err(format!(
                "Bybit returned duplicate position rows for {symbol}"
            ));
        }
    }
    match (one_way, buy, sell) {
        (Some(leverage), None, None) => Ok(Rails {
            buy: leverage,
            sell: leverage,
            hedged: false,
        }),
        (None, Some(buy), Some(sell)) => Ok(Rails {
            buy,
            sell,
            hedged: true,
        }),
        _ => Err(format!("Bybit returned no complete leverage for {symbol}")),
    }
}

fn parse(response: &Response<'_>, symbol: &str) -> Result<Rails, String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Bybit leverage read HTTP {}", response.status));
    }
    let body: Body<'_> = serde_json::from_str(response.body)
        .map_err(|_| "Bybit leverage response is invalid".to_owned())?;
    if body.ret_code != 0 {
        return Err(if body.ret_msg.is_empty() {
            format!("Bybit leverage read code {}", body.ret_code)
        } else {
            body.ret_msg
        });
    }
    let rows = body
        .result
        .ok_or("Bybit leverage response is missing result")?;
    rails(&rows.list, symbol)
}

impl BybitLinear {
    pub(crate) fn fetch_leverage(&mut self, cx: &mut Cx<'_, Pending>, symbol: String) {
        let policy = self
            .read_policy(true, crate::policy::read_policy().timeout_ms)
            .unwrap_or_else(crate::policy::read_policy);
        // The engine owns an initial admission refusal.
        let Ok(context) = cx.admit_http_command(policy) else {
            return;
        };
        let refused = |message: String, kind| {
            VenueError::new(message)
                .kind(kind)
                .outcome(Outcome::NotSent)
        };
        if let Some((message, kind)) = self.signed_read_refusal("leverage") {
            let _ = cx.fail_context(&context, refused(message, kind));
            return;
        }
        if self.variant.is_spot() || self.category == Category::Option {
            let message = format!("Bybit has no leverage setting for {symbol}");
            let _ = cx.fail_context(&context, refused(message, ErrorKind::InvalidInput));
            return;
        }
        if cx.store.market(&symbol).is_none() {
            let message = format!("unknown market {symbol}");
            let _ = cx.fail_context(&context, refused(message, ErrorKind::InvalidInput));
            return;
        }
        let query = Query::new()
            .str("category", self.paths.category)
            .str("symbol", &symbol);
        let rest = Rest::get("/v5/position/list", query).signed();
        let read = Read {
            context: context.clone(),
            symbol,
            position_version: self.account.position_version,
        };
        self.queue_read(cx, context, rest, Then::Leverage(read));
    }

    pub(crate) fn leverage_response(
        &self,
        cx: &mut Cx<'_, Pending>,
        read: Read,
        response: Result<Response<'_>, VenueError>,
    ) {
        let rails = match response {
            Ok(response) => parse(&response, &read.symbol).map_err(VenueError::new),
            Err(error) => Err(error),
        };
        let rails = match rails {
            Ok(rails) => rails,
            Err(error) => {
                let _ = cx.fail_context(&read.context, error);
                return;
            }
        };
        let margin_type = self.margin_type();
        // A leverage write since this read was sent owns the setting; report it.
        let (buy, sell) = if self.account.position_version == read.position_version {
            cx.store.update_setting(&read.symbol, |setting| {
                setting.leverage = rails.buy;
                setting.buy_leverage = rails.hedged.then_some(rails.buy);
                setting.sell_leverage = rails.hedged.then_some(rails.sell);
                setting.margin_type = Some(margin_type);
            });
            (rails.buy, rails.sell)
        } else {
            cx.store
                .setting(&read.symbol)
                .map_or((rails.buy, rails.sell), |setting| {
                    (
                        setting.buy_leverage.unwrap_or(setting.leverage),
                        setting.sell_leverage.unwrap_or(setting.leverage),
                    )
                })
        };
        let fetched = Fetched {
            buy_leverage: buy,
            sell_leverage: sell,
            margin_type,
            position_mode: if rails.hedged {
                PositionMode::Hedged
            } else {
                PositionMode::Oneway
            },
        };
        let _ = match serde_json::to_string(&fetched) {
            Ok(data) => cx.done_context(&read.context, data),
            Err(error) => cx.fail_context(&read.context, VenueError::new(error.to_string())),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(body: &str) -> Result<Rails, String> {
        parse(
            &Response {
                status: 200,
                headers: "",
                body,
            },
            "BTCUSDT",
        )
    }

    #[test]
    fn a_flat_one_way_row_gives_its_leverage() {
        let body = r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","side":"","size":"0","positionIdx":0,"leverage":"50"}]}}"#;
        assert_eq!(
            read(body),
            Ok(Rails {
                buy: 50.0,
                sell: 50.0,
                hedged: false
            })
        );
    }

    #[test]
    fn flat_hedged_rows_give_each_side() {
        let body = r#"{"retCode":0,"result":{"list":[
            {"symbol":"BTCUSDT","side":"","size":"0","positionIdx":2,"leverage":"10"},
            {"symbol":"BTCUSDT","side":"","size":"0","positionIdx":1,"leverage":"25.5"}]}}"#;
        assert_eq!(
            read(body),
            Ok(Rails {
                buy: 25.5,
                sell: 10.0,
                hedged: true
            })
        );
    }

    #[test]
    fn unusable_answers_fail_rather_than_guess() {
        for (body, expected) in [
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"leverage":""}]}}"#,
                "portfolio margin",
            ),
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"leverage":"0"}]}}"#,
                "invalid leverage",
            ),
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":1,"leverage":"5"}]}}"#,
                "no complete",
            ),
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":7,"leverage":"5"}]}}"#,
                "invalid position",
            ),
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"leverage":"5"},{"symbol":"BTCUSDT","positionIdx":0,"leverage":"6"}]}}"#,
                "duplicate",
            ),
            (
                r#"{"retCode":0,"result":{"list":[{"symbol":"ETHUSDT","positionIdx":0,"leverage":"5"}]}}"#,
                "no complete",
            ),
            (
                r#"{"retCode":10001,"retMsg":"params error"}"#,
                "params error",
            ),
            ("not json", "invalid"),
        ] {
            let error = read(body).unwrap_err();
            assert!(error.contains(expected), "{body}: {error}");
        }
    }
}
