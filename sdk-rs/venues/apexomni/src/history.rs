//! Bounded execution reads walk venue symbol alternatives before using WS cache.
use crate::{ApexOmni, Pending, history_wire as wire, read, sign};
use safe_cex_engine::history::{HistoryRows, ReadCompletion, ReadHistoryRows};
use safe_cex_engine::json::ArrayAt;
use safe_cex_engine::model::OrderSide;
use safe_cex_engine::request_context::{CommandEntryError, Context, Operation};
use safe_cex_engine::selected_read::{
    CommandReadError, LocalRefusal, ReadLegPolicy, ReadRole, ReadRoot, SelectedRead,
};
use safe_cex_engine::{Command, Cx, Query, Response, TimerId, VenueError, num};
use serde::Serialize;
use std::collections::HashSet;

pub struct Request {
    /// The caller's original receipt; every alias and fallback page is a leg of it.
    root: ReadRoot,
    owner: Box<ReadHistoryRows<Execution>>,
    symbol: String,
    candidates: Vec<String>,
    candidate: usize,
    fallback: bool,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    quote: String,
    count: usize,
    current: Vec<Entry>,
    validated_response: bool,
}
impl Request {
    pub(crate) fn root(&self) -> &ReadRoot {
        &self.root
    }
    pub(crate) fn reset(&mut self) {
        self.count = 0;
        self.current.clear();
        self.owner.discard_staged_page();
    }
}
struct Entry {
    key: String,
    timestamp_ms: f64,
    row: Execution,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    timestamp: f64,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_id: Option<String>,
    price: f64,
    qty: f64,
    side: OrderSide,
    #[serde(skip_serializing_if = "Option::is_none")]
    commission: Option<f64>,
    commission_asset: String,
}
fn encode_executions(rows: &[Execution]) -> Result<String, VenueError> {
    serde_json::to_string(rows).map_err(|error| VenueError::new(error.to_string()))
}
const ARRAYS: ArrayAt = ArrayAt::Envelope(&[
    &["data", "fills"],
    &["data", "orders"],
    &["data", "list"],
    &["data", "rows"],
    &["data", "results"],
    &["data"],
    &["result", "fills"],
    &["result", "orders"],
    &["result", "list"],
    &["result", "rows"],
    &["result", "results"],
    &["result"],
    &["body", "data", "fills"],
    &["body", "data", "orders"],
    &["body", "data", "list"],
    &["body", "data", "rows"],
    &["body", "data", "results"],
    &["body", "data"],
    &["body", "result", "fills"],
    &["body", "result", "orders"],
    &["body", "result", "list"],
    &["body", "result", "rows"],
    &["body", "result", "results"],
    &["body", "result"],
    &["body", "fills"],
    &["body", "orders"],
    &["body", "list"],
    &["body", "rows"],
    &["body", "results"],
    &["body"],
    &["data", "data", "fills"],
    &["data", "data", "orders"],
    &["data", "data", "list"],
    &["data", "data", "rows"],
    &["data", "data", "results"],
    &["data", "data"],
    &["data", "result", "fills"],
    &["data", "result", "orders"],
    &["data", "result", "list"],
    &["data", "result", "rows"],
    &["data", "result", "results"],
    &["data", "result"],
    &["fills"],
    &["orders"],
    &["list"],
    &["rows"],
    &["results"],
    &[],
]);
fn key(symbol: &str) -> String {
    let symbol: String = symbol
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect();
    let symbol = symbol.strip_prefix("PERP").unwrap_or(&symbol);
    symbol
        .strip_suffix("PERP")
        .or_else(|| symbol.strip_suffix("SWAP"))
        .unwrap_or(symbol)
        .to_owned()
}
fn first<'a>(values: &[&'a Option<String>]) -> Option<&'a str> {
    values
        .iter()
        .find_map(|v| v.as_deref().filter(|s| !s.is_empty()))
}
fn numeric(value: Option<&str>) -> Option<f64> {
    value?.parse::<f64>().ok().filter(|v| v.is_finite())
}
fn milliseconds(value: &str) -> Option<f64> {
    if let Ok(value) = value.parse::<f64>() {
        return (value.is_finite() && value > 0.0)
            .then_some(crate::account::execution_milliseconds(value));
    }
    iso_milliseconds(value)
}
fn iso_milliseconds(value: &str) -> Option<f64> {
    let (date, time) = value.split_once('T').or_else(|| value.split_once(' '))?;
    let mut date = date.split('-');
    let year = date.next()?.parse::<i64>().ok()?;
    let month = date.next()?.parse::<i64>().ok()?;
    let day = date.next()?.parse::<i64>().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let (time, offset) = if let Some(time) = time.strip_suffix('Z') {
        (time, 0)
    } else {
        let index = time
            .char_indices()
            .skip(1)
            .find(|(_, c)| *c == '+' || *c == '-')?
            .0;
        let (time, zone) = time.split_at(index);
        let sign = if zone.starts_with('+') { 1 } else { -1 };
        let mut zone = zone[1..].split(':');
        let hours = zone.next()?.parse::<i64>().ok()?;
        let minutes = zone.next()?.parse::<i64>().ok()?;
        if hours > 23 || minutes > 59 {
            return None;
        }
        (time, sign * (hours * 3600 + minutes * 60))
    };
    let mut time = time.split(':');
    let hour = time.next()?.parse::<i64>().ok()?;
    let minute = time.next()?.parse::<i64>().ok()?;
    let second = time.next()?.parse::<f64>().ok()?;
    if !(0..24).contains(&hour) || !(0..60).contains(&minute) || !(0.0..60.0).contains(&second) {
        return None;
    }
    let days = safe_cex_engine::time::days_from_civil(year, month, day);
    Some((days * 86400 + hour * 3600 + minute * 60 - offset) as f64 * 1000.0 + second * 1000.0)
        .filter(|n| *n > 0.0)
}
impl Request {
    fn row(&self, raw: wire::Execution) -> Option<Entry> {
        if let Some(symbol) = first(&[
            &raw.symbol,
            &raw.s,
            &raw.inst_id,
            &raw.instrument,
            &raw.market,
            &raw.cross_symbol_name,
            &raw.market_symbol,
        ]) && key(symbol) != key(&self.symbol)
            && !self.candidates.iter().any(|s| key(s) == key(symbol))
        {
            return None;
        }
        let side = match first(&[&raw.side, &raw.side_compact])?
            .trim()
            .to_ascii_uppercase()
            .as_str()
        {
            "BUY" | "B" | "LONG" => OrderSide::Buy,
            "SELL" | "S" | "SHORT" => OrderSide::Sell,
            _ => return None,
        };
        let cumulative = numeric(first(&[
            &raw.cum_success_fill_size,
            &raw.cum_match_fill_size,
        ]));
        if self.fallback && numeric(raw.cum_success_fill_size.as_deref()).unwrap_or(0.0) <= 0.0 {
            return None;
        }
        let derived = numeric(first(&[
            &raw.cum_success_fill_value,
            &raw.cum_match_fill_value,
        ]))
        .zip(cumulative)
        .filter(|(_, q)| *q > 0.0)
        .map(|(v, q)| v / q);
        let price_text = first(&[
            &raw.fill_price,
            &raw.fill_px,
            &raw.exec_price,
            &raw.latest_match_fill_price,
            &raw.avg_price,
            &raw.price,
            &raw.p,
        ]);
        let price = if price_text.is_some() {
            numeric(price_text)
        } else {
            derived
        }?;
        let qty = if self.fallback {
            numeric(raw.cum_success_fill_size.as_deref())
        } else {
            None
        }
        .or_else(|| {
            numeric(first(&[
                &raw.fill_size,
                &raw.exec_qty,
                &raw.match_fill_size,
                &raw.last_fill_size,
                &raw.qty,
                &raw.q,
                &raw.amount,
                &raw.size,
                &raw.v,
                &raw.cum_success_fill_size,
                &raw.cum_match_fill_size,
            ]))
        })?;
        if price <= 0.0 || qty <= 0.0 {
            return None;
        }
        let source_time = first(&[
            &raw.created_at,
            &raw.fill_time,
            &raw.created_time,
            &raw.timestamp,
            &raw.ts,
            &raw.time_stamp,
            &raw.time,
            &raw.updated_at,
            &raw.updated_time,
            &raw.trade_time,
            &raw.transact_time,
            &raw.e,
            &raw.t,
            &raw.t_upper,
        ])?;
        let timestamp_ms = milliseconds(source_time)?;
        if self.from.is_some_and(|n| timestamp_ms < n) || self.to.is_some_and(|n| timestamp_ms > n)
        {
            return None;
        }
        let order_id = first(&[
            &raw.order_id,
            &raw.order_i_d,
            &raw.id,
            &raw.i,
            &raw.client_order_id,
            &raw.client_id,
        ])
        .map(str::to_owned);
        if self
            .order_id
            .as_ref()
            .is_some_and(|id| order_id.as_ref().is_some_and(|raw| raw != id))
        {
            return None;
        }
        let fee = first(&[
            &raw.fee,
            &raw.fill_fee,
            &raw.exec_fee,
            &raw.cum_success_fill_fee,
            &raw.cum_match_fill_fee,
        ]);
        let commission = if fee.is_none() {
            Some(0.0)
        } else {
            numeric(fee).map(f64::abs)
        };
        let commission_asset = first(&[
            &raw.token,
            &raw.fee_token,
            &raw.fee_asset,
            &raw.fee_currency,
            &raw.settle_asset_id,
        ])
        .unwrap_or(&self.quote)
        .to_owned();
        let native_id = first(&[
            &raw.trade_id,
            &raw.trade_i_d,
            &raw.fill_id,
            &raw.exec_id,
            &raw.id,
            &raw.i,
        ]);
        let key = if let Some(id) = native_id {
            format!("execution:{id}")
        } else {
            format!(
                "{}:{source_time}:{price}:{qty}:{side:?}",
                order_id.as_deref().unwrap_or("no-order")
            )
        };
        Some(Entry {
            key,
            timestamp_ms,
            row: Execution {
                timestamp: (timestamp_ms / 1000.0).floor(),
                symbol: self.symbol.clone(),
                order_id,
                price,
                qty,
                side,
                commission,
                commission_asset,
            },
        })
    }
}
impl ApexOmni {
    pub(crate) fn history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Result<(), Command> {
        let Command::Query { command, query } = command else {
            return Err(command);
        };
        match query {
            Query::OrderByClientId { .. } => cx.fail(
                command,
                "ApeX Omni cannot provide definitive client order ID lookup",
            ),
            Query::OrderHistory { .. } | Query::Income { .. } => cx.done(command, "[]".into()),
            Query::PublicTrades { .. } => {
                cx.fail(command, "ApeX Omni does not support public trade history")
            }
            Query::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                if self.options.public_data_only || self.options.identity.is_none() {
                    cx.done(command, "[]".into());
                    return Ok(());
                }
                let Some(market) = symbol.as_deref().and_then(|s| cx.store.market(s)) else {
                    cx.done(command, "[]".into());
                    return Ok(());
                };
                let mut candidates = vec![market.id.clone()];
                if market.id != market.symbol {
                    candidates.push(market.symbol.clone());
                }
                let (symbol, quote) = (market.symbol.clone(), market.quote.clone());
                let selected = SelectedRead::select(&self.slots, ReadRole::Read { scope: None });
                let missing = LocalRefusal {
                    diagnostic: None,
                    error: VenueError::new(read::MISSING),
                };
                let root = match SelectedRead::command_root(cx, selected, missing) {
                    Ok(root) => root,
                    Err(CommandReadError::Entry(CommandEntryError::Revoked)) => {
                        let _ = cx.reject_expired_command(Operation::Read);
                        return Ok(());
                    }
                    Err(_) => return Ok(()),
                };
                let completion =
                    match ReadCompletion::from_context(cx, command, root.context().clone()) {
                        Ok(completion) => completion,
                        Err(error) => {
                            let _ = cx.fail_context(root.context(), error.read_venue_error());
                            return Ok(());
                        }
                    };
                let request = Request {
                    root,
                    owner: Box::new(HistoryRows::new(completion, encode_executions)),
                    symbol,
                    candidates,
                    candidate: 0,
                    fallback: false,
                    order_id,
                    from: from_ms.filter(|n| n.is_finite()),
                    to: to_ms.filter(|n| n.is_finite()),
                    limit: limit.unwrap_or(100).clamp(1, 100) as usize,
                    quote,
                    count: 0,
                    current: vec![],
                    validated_response: false,
                };
                self.request_history(cx, request);
            }
        }
        Ok(())
    }
    fn request_history(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let mut params = vec![
            (
                "symbol".into(),
                request.candidates[request.candidate].clone(),
            ),
            ("limit".into(), request.limit.to_string()),
        ];
        if let Some(from) = request.from {
            params.push(("beginTimeInclusive".into(), num::format(from.floor())));
        }
        if let Some(to) = request.to {
            params.push(("endTimeExclusive".into(), num::format(to.floor() + 1.0)));
        }
        if request.fallback {
            params.push(("orderType".into(), "HISTORY".into()));
        }
        let path = if request.fallback {
            "/v3/history-orders"
        } else {
            "/v3/fills"
        };
        let leg = request.root.leg(
            cx,
            ReadLegPolicy {
                timeout_ms: read::TIMEOUT_MS,
                pacing: None,
            },
        );
        let leg = match leg {
            Ok(leg) => leg,
            Err(error) => {
                let _ = request.owner.finish(cx, Err(error.read_venue_error()));
                return;
            }
        };
        self.send_read(
            cx,
            read::Custody::Leg(leg),
            sign::Request::get(path, params),
            Some(ARRAYS),
            read::Native::History(Box::new(request)),
        );
    }
    pub(crate) fn history_item(
        &mut self,
        _cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        let parsed = serde_json::from_str::<wire::Execution>(item).ok();
        if !request.fallback
            || parsed
                .as_ref()
                .is_some_and(|r| numeric(r.cum_success_fill_size.as_deref()).unwrap_or(0.0) > 0.0)
        {
            request.count += 1;
        }
        let Some(row) = parsed.and_then(|row| request.row(row)) else {
            return;
        };
        if let Some(index) = request.current.iter().position(|old| old.key == row.key) {
            request.current[index] = row;
        } else {
            request.current.push(row);
        }
        request
            .current
            .sort_by(|a, b| b.timestamp_ms.total_cmp(&a.timestamp_ms));
        request.current.truncate(request.limit);
    }
    pub(crate) fn history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        leg: &Context,
        _response: Response<'_>,
    ) {
        // The signed wrapper and required-list decoder validated this response.
        // Keep that evidence across later alias failures and signature resets.
        request.validated_response = true;
        // Nonempty primary data is authoritative even when its rows are filtered.
        if request.count > 0 {
            self.finish_history(cx, request, leg);
            return;
        }
        request.candidate += 1;
        request.reset();
        if request.candidate == request.candidates.len() {
            if request.fallback {
                self.finish_history(cx, request, leg);
                return;
            }
            request.fallback = true;
            request.candidate = 0;
        }
        self.request_history(cx, request);
    }
    pub(crate) fn history_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut request: Request,
        leg: &Context,
        error: &str,
    ) {
        request.reset();
        request.candidate += 1;
        if request.candidate == request.candidates.len() {
            if request.fallback {
                if request.validated_response {
                    self.finish_history(cx, request, leg);
                } else {
                    let _ = request.owner.finish(cx, Err(VenueError::new(error)));
                }
                return;
            }
            request.fallback = true;
            request.candidate = 0;
        }
        self.request_history(cx, request);
    }
    fn finish_history(&self, cx: &mut Cx<'_, Pending>, mut request: Request, leg: &Context) {
        if request.current.is_empty() {
            request.current = self
                .account
                .recent_executions
                .iter()
                .filter(|r| {
                    key(&r.symbol) == key(&request.symbol)
                        && request
                            .order_id
                            .as_ref()
                            .is_none_or(|id| r.order_id.as_ref().is_none_or(|raw| raw == id))
                        && request.from.is_none_or(|from| r.timestamp_ms >= from)
                        && request.to.is_none_or(|to| r.timestamp_ms <= to)
                })
                .map(|r| Entry {
                    key: r.identity.clone(),
                    timestamp_ms: r.timestamp_ms,
                    row: Execution {
                        timestamp: r.timestamp,
                        symbol: r.symbol.clone(),
                        order_id: r.order_id.clone(),
                        price: r.price,
                        qty: r.qty,
                        side: r.side,
                        commission: Some(r.commission),
                        commission_asset: r
                            .commission_asset
                            .clone()
                            .unwrap_or_else(|| request.quote.clone()),
                    },
                })
                .collect();
        }
        let mut seen = HashSet::new();
        request.current.retain(|row| seen.insert(row.key.clone()));
        request
            .current
            .sort_by(|a, b| b.timestamp_ms.total_cmp(&a.timestamp_ms));
        request.current.truncate(request.limit);
        // Rows enter coverage only under this exact delivery of the original root.
        let admitted = request.owner.admit(cx, leg).map(|mut page| {
            for entry in request.current {
                page.stage(entry.row);
            }
            page.accept_page();
        });
        let _ = match admitted {
            Ok(()) => request.owner.finish(cx, Ok(())),
            Err(error) => request.owner.reject(cx, error.read_venue_error()),
        };
    }
    pub(crate) fn history_timer(&mut self, _cx: &mut Cx<'_, Pending>, _timer: TimerId) -> bool {
        false
    }
}
