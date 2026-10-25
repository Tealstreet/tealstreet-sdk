//! v2 publishes myTrades, single-order lookup, transfers and force orders;
//! it has no complete closed-order list or PnL/funding ledger. Public recent
//! trades use the documented websocket history request, not a REST guess.
use crate::wire::{Number, Text, check};
use crate::{Bitrue, Job, Pending};
use safe_cex_engine::history::{HistoryRows, ReadCompletion, ReadHistoryRows};
use safe_cex_engine::request_context::Context;
use safe_cex_engine::request_context::{Operation, Policy, RetryPolicy};
use safe_cex_engine::selected_read::ReadRoot;
use safe_cex_engine::{
    CommandId, Cx, ErrorKind, Method, Outcome, Query, Response, VenueError, json, json::ArrayAt,
    num,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

fn local_read_policy() -> Policy {
    Policy {
        operation: Operation::Read,
        credential: None,
        timeout_ms: 20_000,
        pacing: None,
        retry: RetryPolicy::Never,
    }
}

pub struct Request {
    owner: Box<ReadHistoryRows<Execution>>,
    root: ReadRoot,
    /// The page's rows, admitted together once its leg's answer validates.
    rows: Vec<Execution>,
    symbol: String,
    order_id: Option<String>,
    from_ms: Option<f64>,
    to_ms: Option<f64>,
    limit: usize,
    received: usize,
    seen: HashSet<String>,
    failed: bool,
}

impl Request {
    pub(crate) fn sign_failed(self, cx: &mut Cx<'_, Pending>, mut error: VenueError) {
        error.outcome = None;
        let _ = self.owner.finish(cx, Err(error));
    }
    pub(crate) fn root(&self) -> &ReadRoot {
        &self.root
    }
    pub(crate) fn reset(&mut self) {
        self.owner.discard_staged_page();
        self.rows.clear();
        self.seen.clear();
        self.failed = false;
        self.received = 0;
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Fill<'a> {
    #[serde(borrow)]
    trade_id: Text<'a>,
    #[serde(borrow, default)]
    contract_name: Text<'a>,
    #[serde(borrow)]
    side: Text<'a>,
    #[serde(borrow, default)]
    bid_id: Option<Text<'a>>,
    #[serde(borrow, default)]
    ask_id: Option<Text<'a>>,
    price: Number,
    #[serde(default)]
    qty: Number,
    #[serde(default)]
    amount: Number,
    #[serde(default)]
    fee: Number,
    #[serde(alias = "cTime", alias = "time")]
    ctime: Number,
    #[serde(default)]
    is_maker: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    commission_asset: String,
    position_side: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    maker: Option<bool>,
    #[serde(skip)]
    time_ms: f64,
}

fn encode_executions(rows: &[Execution]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}

fn side(value: &str) -> Option<&'static str> {
    if value.eq_ignore_ascii_case("BUY") {
        Some("buy")
    } else if value.eq_ignore_ascii_case("SELL") {
        Some("sell")
    } else {
        None
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.bytes().any(|byte| byte != b'0')
}

fn time_ms(value: f64) -> f64 {
    if value < 1e12 { value * 1000.0 } else { value }
}

// Bitrue has served qty in both base and contract units. The reported quote
// amount disambiguates them; retain the raw quantity when neither agrees.
fn fill_qty(qty: f64, amount: f64, price: f64, multiplier: f64) -> f64 {
    let implied = if amount > 0.0 { amount / price } else { 0.0 };
    if qty <= 0.0 {
        return implied;
    }
    let tolerance = 1e-8 * implied.max(1.0);
    if implied > 0.0
        && (qty - implied).abs() > tolerance
        && (qty * multiplier - implied).abs() <= tolerance
    {
        qty * multiplier
    } else {
        qty
    }
}

impl Bitrue {
    pub(crate) fn history_query(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        query: Query,
    ) {
        let Query::Fills {
            symbol,
            order_id,
            from_ms,
            to_ms,
            limit,
        } = query
        else {
            match query {
                Query::OrderByClientId {
                    symbol,
                    client_order_id,
                } => self.lookup_order(cx, symbol, client_order_id),
                Query::PublicTrades { symbol, limit } => self.public_trades(cx, symbol, limit),
                Query::OrderHistory { .. } => {
                    let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        VenueError::new(
                            "Bitrue v2 has only single-order lookup, not complete order history",
                        )
                        .kind(ErrorKind::Unsupported)
                        .outcome(Outcome::NotSent),
                    );
                }
                Query::Income { .. } => {
                    let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                        return;
                    };
                    let _ = cx.fail_context(
                        &context,
                        VenueError::new("Bitrue v2 transfer and force-order histories are not a complete PnL/funding ledger")
                            .kind(ErrorKind::Unsupported)
                            .outcome(Outcome::NotSent),
                    );
                }
                Query::Fills { .. } => unreachable!(),
            }
            return;
        };
        if !self.authenticated() {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Bitrue fills require authenticated futures credentials")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        if [from_ms, to_ms]
            .into_iter()
            .flatten()
            .any(|t| !t.is_finite() || t < 0.0)
            || matches!((from_ms,to_ms),(Some(from),Some(to)) if from > to)
        {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Invalid Bitrue fill time window")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        }
        let Some(symbol) = symbol.as_deref().and_then(|s| self.resolve_symbol(cx, s)) else {
            let Ok(context) = cx.admit_http_command(local_read_policy()) else {
                return;
            };
            let _ = cx.fail_context(
                &context,
                VenueError::new("Bitrue fills require a known symbol")
                    .kind(ErrorKind::InvalidInput)
                    .outcome(Outcome::NotSent),
            );
            return;
        };
        let market = cx.store.market(&symbol).unwrap();
        let symbol = market.symbol.clone();
        let limit = limit.unwrap_or(200).clamp(1, 1000) as usize;
        let mut path = format!(
            "/fapi/v2/myTrades?contractName={}&limit={limit}",
            safe_cex_engine::url::encode(&market.id)
        );
        if let Some(from) = from_ms {
            path.push_str(&format!("&startTime={}", num::format(from.floor())));
        }
        if let Some(to) = to_ms {
            path.push_str(&format!("&endTime={}", num::format(to.floor())));
        }
        let Some(root) = self.command_root(cx) else {
            return;
        };
        let completion = match ReadCompletion::from_context(cx, command, root.context().clone()) {
            Ok(completion) => completion,
            Err(error) => {
                let _ = cx.fail_context(root.context(), error.read_venue_error());
                return;
            }
        };
        self.send(
            cx,
            Method::Get,
            &path,
            None,
            Some(ArrayAt::EnvelopeField("data")),
            Job::History(Request {
                owner: Box::new(HistoryRows::new(completion, encode_executions)),
                root,
                rows: Vec::new(),
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
                received: 0,
                seen: HashSet::new(),
                failed: false,
            }),
        );
    }

    pub(crate) fn history_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        request.received += 1;
        if request.failed || request.received > 1000 {
            request.failed = true;
            return;
        }
        let row = match serde_json::from_str::<Fill<'_>>(item) {
            Ok(row) => row,
            Err(_) => {
                request.failed = true;
                return;
            }
        };
        let Some(market) = cx.store.market(&request.symbol) else {
            request.failed = true;
            return;
        };
        if !row.contract_name.is_empty() && *row.contract_name != market.id {
            return;
        }
        let Some(side) = side(&row.side) else {
            request.failed = true;
            return;
        };
        let Some(order_id) = (if side == "buy" {
            row.bid_id.as_ref()
        } else {
            row.ask_id.as_ref()
        })
        .filter(|id| valid_id(id)) else {
            request.failed = true;
            return;
        };
        let order_id = order_id.to_string();
        let time = time_ms(row.ctime.0);
        let qty = fill_qty(
            row.qty.0,
            row.amount.0,
            row.price.0,
            market.contract_size.unwrap_or(market.precision.amount),
        );
        if !valid_id(&row.trade_id)
            || row.price.0 <= 0.0
            || !qty.is_finite()
            || qty <= 0.0
            || !time.is_finite()
            || time <= 0.0
        {
            request.failed = true;
            return;
        }
        if request
            .order_id
            .as_ref()
            .is_some_and(|wanted| wanted != &order_id)
            || request.from_ms.is_some_and(|from| time < from)
            || request.to_ms.is_some_and(|to| time > to)
            || !request.seen.insert(row.trade_id.to_string())
        {
            return;
        }
        request.rows.push(Execution {
            id: row.trade_id.to_string(),
            timestamp: (time / 1000.0).floor(),
            symbol: request.symbol.clone(),
            order_id,
            price: row.price.0,
            qty,
            side,
            commission: row.fee.0.abs(),
            commission_asset: market.quote.clone(),
            position_side: "both",
            maker: row.is_maker,
            time_ms: time,
        });
    }

    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        leg: Option<&Context>,
        response: Response<'_>,
    ) {
        if let Err(error) = check(&response) {
            let _ = request.owner.finish(cx, Err(VenueError::new(error)));
            return;
        }
        if request.failed {
            let _ = request.owner.finish(
                cx,
                Err(VenueError::new("Malformed Bitrue execution history")),
            );
            return;
        }
        // fromId advances forward from a trade ID; no backward cursor is
        // documented for the default recent page. A full filtered page cannot
        // establish absence of older matches, so never present it as complete.
        if request.received >= request.limit && request.rows.len() < request.limit {
            let _ = request.owner.reject(cx, VenueError::new("INCOMPLETE: Bitrue native fill page is full before local filters reach the requested limit"));
            return;
        }
        let Some(leg) = leg else {
            return;
        };
        match request.owner.admit(cx, leg) {
            Ok(mut page) => {
                for row in std::mem::take(&mut request.rows) {
                    page.stage(row);
                }
                page.accept_page();
            }
            Err(_) => return,
        }
        let _ = request.owner.finish_with(cx, Ok(()), |mut rows| {
            rows.sort_by(|a, b| b.time_ms.total_cmp(&a.time_ms));
            rows.truncate(request.limit);
            encode_executions(&rows)
        });
    }

    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let _ = request.owner.finish(cx, Err(VenueError::new(error)));
    }
}

#[derive(Deserialize)]
struct PublicEnvelope<'a> {
    #[serde(borrow)]
    status: &'a str,
    ts: Number,
}
#[derive(Deserialize)]
struct PublicRow<'a> {
    #[serde(borrow, default, alias = "tradeId")]
    id: Text<'a>,
    #[serde(borrow)]
    side: Text<'a>,
    price: Number,
    vol: Number,
    #[serde(default)]
    amount: Option<Number>,
    #[serde(default)]
    ts: Option<Number>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Trade {
    id: String,
    symbol: String,
    price: f64,
    qty: f64,
    quote_qty: f64,
    timestamp: f64,
    side: &'static str,
}

pub(crate) fn trade_result(
    symbol: &str,
    contract_size: f64,
    body: &str,
    limit: u32,
) -> Result<String, String> {
    let envelope: PublicEnvelope<'_> = serde_json::from_str(body).map_err(|e| e.to_string())?;
    if envelope.status != "ok"
        || !contract_size.is_finite()
        || contract_size <= 0.0
        || envelope.ts.0 <= 0.0
    {
        return Err("Invalid Bitrue public trade response".into());
    }
    let mut rows = Vec::new();
    let mut failed = false;
    let found = json::each_in_field::<PublicRow<'_>>(body, "data", |row| {
        let qty = row.vol.0 * contract_size;
        let quote_qty = row
            .amount
            .map_or(qty * row.price.0, |n| n.0 * contract_size);
        let timestamp = row.ts.map_or(envelope.ts.0, |n| n.0);
        let Some(side) = side(&row.side) else {
            failed = true;
            return;
        };
        if row.price.0 <= 0.0
            || qty <= 0.0
            || !qty.is_finite()
            || quote_qty < 0.0
            || !quote_qty.is_finite()
            || timestamp <= 0.0
        {
            failed = true;
            return;
        }
        // The documented history rows have no ID/time. An absent ID stays
        // empty; their response timestamp is the only available native time.
        rows.push(Trade {
            id: row.id.to_string(),
            symbol: symbol.into(),
            price: row.price.0,
            qty,
            quote_qty,
            timestamp,
            side,
        });
    })
    .map_err(|e| e.to_string())?;
    if !found || failed {
        return Err("Malformed Bitrue public trade history".into());
    }
    rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
    rows.truncate(limit.clamp(1, 300) as usize);
    serde_json::to_string(&rows).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn public_history_uses_native_snapshot_time_and_contract_units() {
        let out = trade_result("E-BTC-USDT", 0.001,
            r#"{"event_rep":"rep","status":"ok","ts":1721743391000,"data":[{"side":"buy","price":60000,"vol":3,"amount":180000},{"id":1558124009467904992,"side":"SELL","price":61000,"vol":1,"amount":61000,"ts":1721743392000}]}"#, 1).unwrap();
        assert!(out.contains(r#""id":"1558124009467904992""#));
        assert!(out.contains(r#""qty":0.001"#));
        assert!(out.contains(r#""quoteQty":61.0"#));
        assert!(out.contains(r#""timestamp":1721743392000.0"#));
        let out = trade_result("E-BTC-USDT", 0.001,
            r#"{"status":"ok","ts":1721743391000,"data":[{"side":"buy","price":60000,"vol":3,"amount":180000}]}"#, 300).unwrap();
        assert!(out.contains(r#""id":"""#));
        assert!(out.contains(r#""timestamp":1721743391000.0"#));
    }
    #[test]
    fn public_errors_and_malformed_rows_fail_instead_of_looking_empty() {
        for body in [
            r#"{"status":"error","ts":1721743391000,"data":[]}"#,
            r#"{"status":"ok","ts":1721743391000}"#,
            r#"{"status":"ok","ts":1721743391000,"data":[{"side":"other","price":1,"vol":1}]}"#,
            r#"{"status":"ok","ts":1721743391000,"data":[{"side":"buy","price":1,"vol":"NaN"}]}"#,
        ] {
            assert!(trade_result("E-BTC-USDT", 0.001, body, 300).is_err());
        }
    }
    #[test]
    fn fill_quantity_uses_quote_evidence_without_always_scaling_base_qty() {
        assert_eq!(fill_qty(3.0, 180.0, 60000.0, 0.001), 0.003);
        assert_eq!(fill_qty(0.003, 180.0, 60000.0, 0.001), 0.003);
        assert_eq!(fill_qty(0.0, 180.0, 60000.0, 0.001), 0.003);
        assert_eq!(fill_qty(3.0, 0.0, 60000.0, 0.001), 3.0);
    }
}
