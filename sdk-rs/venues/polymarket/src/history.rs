//! Rare queries stay explicit: an upstream endpoint is not a safe-cex contract.
use crate::{Pending, Polymarket};
use safe_cex_engine::{Command, CommandId, Cx, Method, Query, Response};

#[derive(serde::Deserialize)]
struct BookSnapshot {
    asset_id: String,
    bids: Vec<crate::wire::Level>,
    asks: Vec<crate::wire::Level>,
}

#[derive(serde::Serialize)]
struct BookLevel {
    price: f64,
    amount: f64,
    total: f64,
}

fn book_side(
    mut levels: Vec<crate::wire::Level>,
    bids: bool,
    limit: Option<u32>,
) -> Result<Vec<BookLevel>, String> {
    if levels
        .iter()
        .any(|level| !(0.0..=1.0).contains(&level.price.0) || level.size.0 < 0.0)
    {
        return Err("Invalid Polymarket order-book level".into());
    }
    levels.sort_by(|a, b| {
        if bids {
            b.price.0.total_cmp(&a.price.0)
        } else {
            a.price.0.total_cmp(&b.price.0)
        }
    });
    if levels
        .windows(2)
        .any(|pair| pair[0].price.0 == pair[1].price.0)
    {
        return Err("Duplicate Polymarket order-book price".into());
    }
    let mut total = 0.0;
    let mut result = Vec::new();
    for level in levels {
        total += level.size.0;
        if !total.is_finite() {
            return Err("Invalid Polymarket order-book total".into());
        }
        if level.size.0 > 0.0 && limit.is_none_or(|limit| result.len() < limit as usize) {
            result.push(BookLevel {
                price: level.price.0,
                amount: level.size.0,
                total,
            });
        }
    }
    Ok(result)
}

impl Polymarket {
    pub(crate) fn read_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        match command {
            Command::FetchOrderBook {
                command,
                symbol,
                limit,
            } => {
                if limit == Some(0) {
                    cx.fail(command, "Invalid Polymarket order-book limit");
                    return;
                }
                let Some(market) = cx.store.market(&symbol) else {
                    cx.fail(command, "Unknown Polymarket symbol");
                    return;
                };
                let token = market.id.clone();
                cx.request(
                    Method::Get,
                    format!("{}/book?token_id={}", crate::REST, crate::encode(&token)),
                    vec![],
                    None,
                    Pending::Book {
                        command,
                        token,
                        limit,
                    },
                );
            }
            Command::FetchAccount { command } => {
                #[derive(serde::Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Account<'a> {
                    user_id: &'a str,
                    affiliate_id: Option<&'a str>,
                }
                {
                    let result: Result<String, String> = serde_json::to_string(&Account {
                        user_id: self.address.as_deref().unwrap_or("unknown"),
                        affiliate_id: None,
                    })
                    .map_err(|error| error.to_string());
                    match result {
                        Ok(data) => cx.done(command, data),
                        Err(error) => cx.fail(command, error),
                    }
                };
            }
            Command::FetchStpMode { command } | Command::SetStpMode { command, .. } => {
                cx.fail(
                    command,
                    "Polymarket self-trade prevention settings are unsupported",
                );
            }
            Command::FetchLeverage { command, .. } => {
                cx.done(command, r#"{"buyLeverage":1,"sellLeverage":1}"#.into())
            }
            Command::FetchMaxLeverage { command, .. } => cx.done(command, "1".into()),
            Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. } => {
                let _ = cx.done_local_setting("[]".into(), |_| {});
            }
            Command::Query { command, query } => {
                let result: Result<String, String> = match query {
                    Query::OrderByClientId { .. } => {
                        Ok(r#"{"found":false,"definitive":false}"#.into())
                    }
                    Query::Fills { .. } => {
                        Err("Polymarket historical fills are unsupported".into())
                    }
                    Query::OrderHistory { .. } => {
                        Err("Polymarket historical orders are unsupported".into())
                    }
                    Query::Income { .. } => Err("Polymarket income history is unsupported".into()),
                    Query::PublicTrades { .. } => {
                        Err("Polymarket public trades are unsupported".into())
                    }
                };
                match result {
                    Ok(data) => cx.done(command, data),
                    Err(error) => cx.fail(command, error),
                }
            }
            _ => unreachable!("only rare reads and immutable account settings"),
        }
    }

    pub(crate) fn book_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        token: &str,
        limit: Option<u32>,
        response: Response<'_>,
    ) {
        #[derive(serde::Serialize)]
        struct OrderBook {
            bids: Vec<BookLevel>,
            asks: Vec<BookLevel>,
        }
        let result = (|| {
            if !(200..300).contains(&response.status) {
                return Err(format!(
                    "Polymarket order-book request failed ({})",
                    response.status
                ));
            }
            let snapshot = serde_json::from_str::<BookSnapshot>(response.body)
                .map_err(|_| "Invalid Polymarket order-book snapshot".to_owned())?;
            if snapshot.asset_id != token {
                return Err("Polymarket order-book asset mismatch".into());
            }
            let book = OrderBook {
                bids: book_side(snapshot.bids, true, limit)?,
                asks: book_side(snapshot.asks, false, limit)?,
            };
            serde_json::to_string(&book).map_err(|error| error.to_string())
        })();
        match result {
            Ok(data) => cx.done(command, data),
            Err(error) => cx.fail(command, error),
        };
    }
}
