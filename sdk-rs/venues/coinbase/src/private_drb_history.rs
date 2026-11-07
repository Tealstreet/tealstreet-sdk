//! Gateway history is command-owned, bounded and independent of account state.
//! Trades use end_seq/end_id; order history uses native continuation when
//! supplied, or documented offsets. Recent and archive stages share identities.
use super::*;
use safe_cex_engine::history::{IncompleteReason, ReadCompletion};
use safe_cex_engine::{Command, CommandId, Query};
const MAX_PAGES: u32 = 50;
#[derive(Clone, Copy, PartialEq)]
enum Axis {
    Fills,
    Orders,
}
pub(super) struct Request {
    pub owner: ReadCompletion,
    axis: Axis,
    symbol: Option<String>,
    instrument: Option<String>,
    order_id: Option<String>,
    from: Option<f64>,
    to: Option<f64>,
    limit: usize,
    params: Value,
    method: &'static str,
    pages: u32,
    archived: bool,
    rows: Vec<Value>,
    identities: HashMap<String, Value>,
    continuations: HashSet<String>,
}
fn bound(v: Option<f64>) -> Result<Option<f64>, String> {
    v.map(|n| {
        if n.is_finite() && n >= 0. && n.fract() == 0. && n <= 253402300799999. {
            Ok(n)
        } else {
            Err("drb invalid history time bound".into())
        }
    })
    .transpose()
}
impl Coinbase {
    pub(crate) fn drb_history_command(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: Command,
    ) -> Option<Command> {
        let Command::Query { command: id, query } = &command else {
            return Some(command);
        };
        let symbol = match query {
            Query::Fills { symbol, .. } | Query::OrderHistory { symbol, .. } => symbol.as_deref(),
            _ => return Some(command),
        };
        let perpetual = symbol.is_some_and(|s| {
            cx.store
                .market(s)
                .is_some_and(|m| m.id.ends_with("-PERP-INTX") || m.id.ends_with("_USDC-PERPETUAL"))
        });
        if !perpetual && !(symbol.is_none() && self.private_drb.enabled) {
            return Some(command);
        }
        let root = match cx.admit_http_command(policy()) {
            Ok(c) => c,
            Err(_) => return None,
        };
        let owner = match ReadCompletion::from_context(cx, *id, root) {
            Ok(o) => o,
            Err(_) => return None,
        };
        let result = self.drb_history_request(cx, *id, query, owner);
        match result {
            Ok(h) => self.drb_history_page(cx, h),
            Err((owner, e)) => {
                let _ = (*owner).finish(
                    cx,
                    Err(VenueError::new(e).outcome(safe_cex_engine::Outcome::NotSent)),
                );
            }
        }
        None
    }
    fn drb_history_request(
        &self,
        cx: &Cx<'_, Pending>,
        _: CommandId,
        query: &Query,
        owner: ReadCompletion,
    ) -> Result<Request, (Box<ReadCompletion>, String)> {
        let result = (|| -> Result<_, String> {
            if !self.private_drb.active || self.private_drb.http.needs_refresh(cx.now) {
                return Err(
                    "drb private history requires a current authenticated gateway session".into(),
                );
            }
            if self
                .private_drb
                .pending
                .values()
                .filter(|p| matches!(p.kind, Kind::History(_)))
                .count()
                >= 16
            {
                return Err("drb history request limit".into());
            }
            let (axis, symbol, order_id, from, to, limit) = match query {
                Query::Fills {
                    symbol,
                    order_id,
                    from_ms,
                    to_ms,
                    limit,
                } => (
                    Axis::Fills,
                    symbol.clone(),
                    order_id.clone(),
                    *from_ms,
                    *to_ms,
                    *limit,
                ),
                Query::OrderHistory {
                    symbol,
                    from_ms,
                    to_ms,
                    limit,
                } => (Axis::Orders, symbol.clone(), None, *from_ms, *to_ms, *limit),
                _ => return Err("drb unsupported history axis".into()),
            };
            let from = bound(from)?;
            let to = bound(to)?;
            if from.zip(to).is_some_and(|(a, b)| a > b) {
                return Err("drb reversed history range".into());
            }
            if limit.is_some_and(|n| n == 0 || n > 1000) {
                return Err("drb history limit must be 1..1000".into());
            }
            let limit = if limit.is_none() && from.is_some() {
                usize::MAX
            } else {
                limit.unwrap_or(100) as usize
            };
            let instrument = symbol
                .as_deref()
                .map(|s| {
                    let m = cx.store.market(s).ok_or("drb history market unavailable")?;
                    if m.id.ends_with("_USDC-PERPETUAL") {
                        drb::from_instrument(&m.id)?;
                        Ok(m.id.clone())
                    } else {
                        drb::to_instrument(&m.id)
                    }
                })
                .transpose()?;
            if let Some(id) = &order_id {
                wire::identity(&json!({"order_id":id}), "order_id")?;
            }
            let (method, mut params) = match axis {
                Axis::Fills if order_id.is_some() => (
                    "private/get_user_trades_by_order",
                    json!({"order_id":order_id.as_deref().unwrap(),"sorting":"desc"}),
                ),
                Axis::Fills if instrument.is_some() => (
                    "private/get_user_trades_by_instrument",
                    json!({"instrument_name":instrument.as_deref().unwrap(),"sorting":"desc","count":1000}),
                ),
                Axis::Fills => (
                    "private/get_user_trades_by_currency",
                    json!({"currency":"USDC","kind":"future","sorting":"desc","count":1000}),
                ),
                Axis::Orders if instrument.is_some() => (
                    "private/get_order_history_by_instrument",
                    json!({"instrument_name":instrument.as_deref().unwrap(),"include_old":true,"include_unfilled":true,"with_continuation":true,"count":1000}),
                ),
                Axis::Orders => (
                    "private/get_order_history_by_currency",
                    json!({"currency":"USDC","kind":"future","include_old":true,"include_unfilled":true,"with_continuation":true,"count":1000}),
                ),
            };
            params["historical"] = json!(false);
            if axis == Axis::Fills && order_id.is_none() {
                if let Some(from) = from {
                    params["start_timestamp"] = json!(from);
                }
                if let Some(to) = to {
                    params["end_timestamp"] = json!(to);
                }
            }
            Ok((
                axis, symbol, instrument, order_id, from, to, limit, method, params,
            ))
        })();
        match result {
            Ok((axis, symbol, instrument, order_id, from, to, limit, method, params)) => {
                Ok(Request {
                    owner,
                    axis,
                    symbol,
                    instrument,
                    order_id,
                    from,
                    to,
                    limit,
                    method,
                    params,
                    pages: 0,
                    archived: false,
                    rows: vec![],
                    identities: HashMap::new(),
                    continuations: HashSet::new(),
                })
            }
            Err(e) => Err((Box::new(owner), e)),
        }
    }
    fn drb_history_page(&mut self, cx: &mut Cx<'_, Pending>, mut h: Request) {
        if h.pages >= MAX_PAGES {
            h.owner.incomplete(
                IncompleteReason::PageBudget,
                "drb history page budget exhausted",
            );
            Self::drb_history_finish(cx, h);
            return;
        }
        h.pages += 1;
        let method = h.method;
        let params = h.params.clone();
        self.drb_read(cx, Kind::History(Box::new(h)), method, params);
    }
    pub(super) fn drb_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        mut h: Request,
        value: Value,
    ) {
        let result = (|| -> Result<(bool, Option<String>, Vec<Value>), String> {
            let (raw, more, continuation) = match h.axis {
                Axis::Fills if h.order_id.is_some() => {
                    let raw = rows(value)?;
                    let more = raw.len() >= 1000;
                    (raw, more, None)
                }
                Axis::Fills => {
                    #[derive(Deserialize)]
                    struct Page {
                        trades: Vec<Value>,
                        has_more: bool,
                    }
                    let p: Page =
                        serde_json::from_value(value).map_err(|_| "drb invalid trade page")?;
                    if p.trades.len() > 1000 {
                        return Err("drb trade page exceeds count".into());
                    }
                    (p.trades, p.has_more, None)
                }
                Axis::Orders if value.is_array() => {
                    let raw = rows(value)?;
                    let more = raw.len() >= 1000;
                    (raw, more, None)
                }
                Axis::Orders => {
                    #[derive(Deserialize)]
                    struct Page {
                        orders: Vec<Value>,
                        continuation: Option<String>,
                    }
                    let p: Page = serde_json::from_value(value)
                        .map_err(|_| "drb invalid order history page")?;
                    let more = p.continuation.as_ref().is_some_and(|s| !s.is_empty());
                    (p.orders, more, p.continuation)
                }
            };
            if raw.len() > 1000 {
                return Err("drb history page exceeds count".into());
            }
            if more && raw.is_empty() {
                return Err("drb history empty continuation page".into());
            }
            let mut mapped = Vec::new();
            for row in &raw {
                if let Some(instrument) = &h.instrument
                    && wire::text(row, "instrument_name")? != instrument
                {
                    return Err("drb history instrument mismatch".into());
                }
                let mapped_row = match h.axis {
                    Axis::Fills => {
                        if h.order_id
                            .as_ref()
                            .is_some_and(|id| row["order_id"] != id.as_str())
                        {
                            return Err("drb history order mismatch".into());
                        }
                        wire::trade(row)?
                    }
                    Axis::Orders => {
                        let o = order(row.clone())?;
                        if o.status == OrderStatus::Open {
                            continue;
                        }
                        let created = wire::timestamp(row, "creation_timestamp")?;
                        let mut result = json!({"id":o.id,"symbol":o.symbol,"side":if o.side==OrderSide::Buy{"buy"}else{"sell"},"type":o.kind,"status":o.status,"amount":o.amount,"filled":o.filled,"reduceOnly":o.reduce_only,"createdTime":created,"updatedTime":o.last_update});
                        if o.price > 0. {
                            result["price"] = json!(o.price);
                        }
                        if row.get("average_price").is_some() {
                            let n = wire::number(row, "average_price")?;
                            if n > 0. {
                                result["averagePrice"] = json!(n);
                            }
                        }
                        result
                    }
                };
                if h.symbol
                    .as_ref()
                    .is_some_and(|s| mapped_row["symbol"] != s.as_str())
                {
                    // The market symbol may differ from its saved native ID.
                    let mut mapped_row = mapped_row;
                    mapped_row["symbol"] = json!(h.symbol);
                    mapped.push(mapped_row);
                } else {
                    mapped.push(mapped_row);
                }
            }
            // Stage the whole page before accepting rows or identities.
            let mut page_ids = HashMap::new();
            for row in &mapped {
                let id = wire::identity(row, "id")?;
                if let Some(old) = h.identities.get(&id).or_else(|| page_ids.get(&id)) {
                    if old != row {
                        return Err("drb conflicting history identity".into());
                    }
                } else {
                    page_ids.insert(id, row.clone());
                }
            }
            let fresh = page_ids.len();
            for (id, row) in page_ids {
                let time = if h.axis == Axis::Fills {
                    row["timestamp"].as_f64().unwrap() * 1000.
                } else {
                    row["createdTime"].as_f64().unwrap()
                };
                h.identities.insert(id, row.clone());
                if h.from.is_none_or(|f| time >= f) && h.to.is_none_or(|t| time <= t) {
                    h.rows.push(row);
                }
            }
            if more && fresh == 0 {
                return Err("drb history continuation made no progress".into());
            }
            Ok((more, continuation, raw))
        })();
        let (more, continuation, raw) = match result {
            Ok(p) => p,
            Err(e) => {
                let _ = h.owner.finish(cx, Err(VenueError::new(e)));
                return;
            }
        };
        if h.rows.len() >= h.limit {
            Self::drb_history_finish(cx, h);
            return;
        }
        if more {
            let next = match h.axis {
                Axis::Orders => {
                    if let Some(token) = continuation {
                        if token.len() > 2048 || !h.continuations.insert(token.clone()) {
                            let _ = h.owner.finish(
                                cx,
                                Err(VenueError::new(
                                    "drb repeated or oversized history continuation",
                                )),
                            );
                            return;
                        }
                        h.params["continuation"] = json!(token);
                        h.params.as_object_mut().unwrap().remove("offset");
                    } else {
                        let offset = h.params["offset"].as_u64().unwrap_or(0) + raw.len() as u64;
                        h.params["offset"] = json!(offset);
                    }
                    Ok(())
                }
                Axis::Fills if h.order_id.is_some() => {
                    h.owner.incomplete(
                        IncompleteReason::PageFull,
                        "drb order trades reached the native page bound",
                    );
                    Self::drb_history_finish(cx, h);
                    return;
                }
                Axis::Fills => {
                    let last = raw.last().unwrap();
                    if h.instrument.is_some() {
                        match last.get("trade_seq").and_then(Value::as_u64) {
                            Some(seq) => {
                                h.params["end_seq"] = json!(seq);
                                Ok(())
                            }
                            None => Err("drb missing fill sequence"),
                        }
                    } else {
                        match wire::identity(last, "trade_id") {
                            Ok(id) => {
                                h.params["end_id"] = json!(id);
                                Ok(())
                            }
                            Err(_) => Err("drb missing fill identity"),
                        }
                    }
                }
            };
            if let Err(e) = next {
                let _ = h.owner.finish(cx, Err(VenueError::new(e)));
                return;
            }
            self.drb_history_page(cx, h);
            return;
        }
        if !h.archived {
            h.archived = true;
            h.params["historical"] = json!(true);
            for key in ["continuation", "offset", "end_seq", "end_id"] {
                h.params.as_object_mut().unwrap().remove(key);
            }
            self.drb_history_page(cx, h);
        } else {
            Self::drb_history_finish(cx, h);
        }
    }
    fn drb_history_finish(cx: &mut Cx<'_, Pending>, mut h: Request) {
        let timestamp = |v: &Value| {
            if h.axis == Axis::Fills {
                v["timestamp"].as_f64().unwrap()
            } else {
                v["createdTime"].as_f64().unwrap()
            }
        };
        h.rows.sort_by(|a, b| {
            timestamp(b)
                .total_cmp(&timestamp(a))
                .then_with(|| a["id"].as_str().cmp(&b["id"].as_str()))
        });
        h.rows.truncate(h.limit);
        let result = serde_json::to_string(&h.rows).map_err(|e| VenueError::new(e.to_string()));
        let _ = h.owner.finish(cx, result);
    }
}
