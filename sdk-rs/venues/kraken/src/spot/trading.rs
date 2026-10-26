use super::{
    KrakenSpot, Pending,
    account::{Kind, order_kind},
};
use safe_cex_engine::history::{Completion, HistoryRows};
use safe_cex_engine::request_context::Operation;
use safe_cex_engine::{
    Command, CommandId, Cx, ErrorKind, Outcome, Query, VenueError, model::*, num,
};
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Clone)]
pub enum Step {
    Place {
        params: Vec<(&'static str, String)>,
        receipt: OrderIdentityReceipt,
    },
    Amend {
        id: String,
        native_kind: String,
        params: Vec<(&'static str, String)>,
    },
    Cancel {
        id: String,
    },
}
#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
}
struct Job {
    command: CommandId,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    cancel: bool,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Deserialize)]
struct Placed {
    txid: Vec<String>,
}
#[derive(Deserialize)]
struct Amended {
    amend_id: String,
}
#[derive(Deserialize)]
struct Canceled {
    count: u32,
}
impl KrakenSpot {
    fn prepare(&self, cx: &Cx<'_, Pending>, o: &PlaceOrderOpts) -> Result<Step, String> {
        if o.client_order_id.is_some() {
            return Err("Kraken Spot does not support caller client order IDs".into());
        }
        if o.trigger_price.is_some() && !o.redundant_protective_trigger() {
            return Err("Kraken Spot triggerPrice must match the stop price".into());
        }
        let m = cx
            .store
            .market(&o.symbol)
            .ok_or("Kraken Spot unknown market")?;
        let pair = self
            .pairs
            .get(&o.symbol)
            .ok_or("Kraken Spot missing REST pair identity")?;
        let amount = num::adjust(o.amount, m.precision.amount);
        if !amount.is_finite() || amount <= 0. || amount < m.limits.amount.min {
            return Err("Kraken Spot invalid quantity".into());
        }
        let kind = match o.kind {
            OrderType::Limit => "limit",
            OrderType::Market => "market",
            OrderType::StopLoss => "stop-loss",
            OrderType::TakeProfit => "take-profit",
            OrderType::TrailingStopLoss => "trailing-stop",
            _ => return Err("Kraken Spot unsupported order type".into()),
        };
        let mut entropy = [0; 16];
        cx.fill_entropy(&mut entropy)
            .map_err(|_| "Kraken Spot client ID entropy unavailable")?;
        let client_id = safe_cex_crypto::hex(&entropy);
        let mut params = vec![
            ("pair", pair.clone()),
            (
                "type",
                if o.side == OrderSide::Buy {
                    "buy"
                } else {
                    "sell"
                }
                .into(),
            ),
            ("ordertype", kind.into()),
            ("volume", num::format(amount)),
            ("cl_ord_id", client_id.clone()),
        ];
        if o.kind != OrderType::Market {
            let input_price = o.price.ok_or("Kraken Spot order requires price")?;
            let price = if o.kind == OrderType::TrailingStopLoss {
                input_price
            } else {
                num::adjust(input_price, m.precision.price)
            };
            if !price.is_finite() || price <= 0. {
                return Err("Kraken Spot invalid price".into());
            }
            let value = if o.kind == OrderType::TrailingStopLoss {
                let ticker = cx
                    .store
                    .ticker(&o.symbol)
                    .ok_or("Kraken Spot trailing stop requires ticker")?;
                if !ticker.last.is_finite() || ticker.last <= 0. {
                    return Err("Kraken Spot trailing stop requires ticker".into());
                }
                let deviation = num::adjust((ticker.last - price).abs(), m.precision.price);
                if !deviation.is_finite() || deviation <= 0. {
                    return Err("Kraken Spot trailing price must differ from last".into());
                }
                format!("+{}", num::format(deviation))
            } else {
                num::format(price)
            };
            params.push(("price", value));
        }
        if o.time_in_force == Some(TimeInForce::PostOnly) {
            params.push(("oflags", "post".into()));
        }
        if o.time_in_force == Some(TimeInForce::ImmediateOrCancel) {
            params.push(("timeinforce", "IOC".into()));
        }
        Ok(Step::Place {
            params,
            receipt: OrderIdentityReceipt {
                symbol: o.symbol.clone(),
                kind: OrderIdentityKind::ClientOrderId,
                identity: client_id,
            },
        })
    }
    fn prepare_batch(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<VecDeque<Step>, String> {
        if orders.iter().any(|o| o.client_order_id.is_some()) {
            return Err("Kraken Spot does not support caller client order IDs".into());
        }
        let mut steps = VecDeque::new();
        for o in orders {
            steps.push_back(self.prepare(cx, o)?);
            // A zero leg means "none", matching the TS truthiness check.
            let stop_loss = o.stop_loss.filter(|p| *p != 0.);
            let take_profit = o.take_profit.filter(|p| *p != 0.);
            if stop_loss.is_some() || take_profit.is_some() {
                if o.kind != OrderType::Limit {
                    return Err(
                        "Kraken Spot does not support take-profit/stop-loss legs on a market entry; use a limit entry or place TP/SL separately"
                            .into(),
                    );
                }
                for (kind, price) in [
                    (OrderType::StopLoss, stop_loss),
                    (OrderType::TakeProfit, take_profit),
                ] {
                    if let Some(price) = price {
                        let mut child = o.clone();
                        child.kind = kind;
                        child.side = o.side.opposite();
                        child.price = Some(price);
                        child.time_in_force = None;
                        child.stop_loss = None;
                        child.take_profit = None;
                        steps.push_back(self.prepare(cx, &child)?);
                    }
                }
            }
        }
        Ok(steps)
    }
    fn amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<Step, String> {
        let order = cx
            .store
            .order(id)
            .filter(|o| o.status == OrderStatus::Open)
            .ok_or("Kraken Spot amend requires reconciled open order")?;
        let raw = self
            .account
            .provenance
            .get(id)
            .ok_or("Kraken Spot amend requires reconciled native order type")?;
        if !matches!(&**raw, "limit" | "stop-loss" | "take-profit")
            || order_kind(raw) != Some(order.kind)
            || order.time_in_force == Some(TimeInForce::ImmediateOrCancel)
        {
            return Err("Kraken Spot cannot safely amend this order type".into());
        }
        let m = cx
            .store
            .market(&order.symbol)
            .ok_or("Kraken Spot unknown amend market")?;
        let mut params = vec![("txid", id.into())];
        if let Some(a) = amount {
            let a = num::adjust(a, m.precision.amount);
            if !a.is_finite() || a <= 0. {
                return Err("Kraken Spot amend quantity must be positive".into());
            }
            params.push(("order_qty", num::format(a)));
        }
        if let Some(p) = price {
            let p = num::adjust(p, m.precision.price);
            if !p.is_finite() || p <= 0. {
                return Err("Kraken Spot amend price must be positive".into());
            }
            params.push((
                if raw == "limit" {
                    "limit_price"
                } else {
                    "trigger_price"
                },
                num::format(p),
            ));
            if raw == "limit" && order.time_in_force == Some(TimeInForce::PostOnly) {
                params.push(("post_only", "true".into()));
            }
        }
        if params.len() == 1 {
            return Err("Kraken Spot amend requires quantity or price".into());
        }
        Ok(Step::Amend {
            id: id.into(),
            native_kind: raw.clone(),
            params,
        })
    }
    fn enqueue(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        steps: VecDeque<Step>,
        cancel: bool,
    ) {
        self.trading.queue.push_back(Job {
            command,
            steps,
            ids: vec![],
            cancel,
            effective: false,
        });
        self.trade_next(cx)
    }
    fn trade_next(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting {
            return;
        }
        if self.trading.active.is_none() {
            self.trading.active = self.trading.queue.pop_front();
        }
        let Some(job) = self.trading.active.as_mut() else {
            return;
        };
        let Some(step) = job.steps.pop_front() else {
            let job = self.trading.active.take().unwrap();
            cx.done(
                job.command,
                if job.cancel {
                    "null".into()
                } else {
                    serde_json::to_string(&job.ids).unwrap()
                },
            );
            self.trade_next(cx);
            return;
        };
        // A queued edit can become terminal before it is sent.
        if let Step::Amend {
            id, native_kind, ..
        } = &step
            && (self.account.terminal.contains(id)
                || self.account.provenance.get(id) != Some(native_kind)
                || !cx.store.order(id).is_some_and(|o| {
                    o.status == OrderStatus::Open
                        && Some(o.kind) == order_kind(native_kind)
                        && o.time_in_force != Some(TimeInForce::ImmediateOrCancel)
                }))
        {
            self.trade_failed(cx, not_sent("Kraken Spot order closed before amend"));
            return;
        }
        let (path, params) = match &step {
            Step::Place { params, .. } => ("/0/private/AddOrder", params.clone()),
            Step::Amend { params, .. } => ("/0/private/AmendOrder", params.clone()),
            Step::Cancel { id } => ("/0/private/CancelOrder", vec![("txid", id.clone())]),
        };
        self.trading.waiting = true;
        if let Step::Place { receipt, .. } = &step {
            cx.command_progress(job.command, vec![receipt.clone()]);
        }
        self.request_private(cx, path, params, Kind::Trade(step));
    }
    pub(super) fn trade_response(&mut self, cx: &mut Cx<'_, Pending>, step: Step, text: &str) {
        self.trading.waiting = false;
        let r = (|| {
            let Some(job) = self.trading.active.as_mut() else {
                return Err(unproven("Kraken Spot unexpected trade reply"));
            };
            match step {
                Step::Place { .. } => {
                    let r: Placed = serde_json::from_str(text)
                        .map_err(|_| unproven("Kraken Spot missing placement txids"))?;
                    if r.txid.is_empty() || r.txid.iter().any(String::is_empty) {
                        return Err(unproven("Kraken Spot empty placement txid"));
                    }
                    job.ids.extend(r.txid);
                }
                Step::Amend { id, .. } => {
                    let r: Amended = serde_json::from_str(text)
                        .map_err(|_| unproven("Kraken Spot missing amend_id"))?;
                    if r.amend_id.is_empty() {
                        return Err(unproven("Kraken Spot missing amend_id"));
                    }
                    job.ids.push(id);
                }
                Step::Cancel { id } => {
                    let r: Canceled = serde_json::from_str(text)
                        .map_err(|_| unproven("Kraken Spot missing cancel count"))?;
                    if r.count == 0 {
                        // An explicit answer that nothing was cancelled.
                        return Err(VenueError::new("Kraken Spot order was not canceled")
                            .outcome(Outcome::Rejected));
                    }
                    job.ids.push(id);
                }
            }
            job.effective = true;
            Ok::<_, VenueError>(())
        })();
        match r {
            Ok(()) => {
                self.account.revision += 1;
                self.trade_next(cx)
            }
            Err(e) => self.trade_failed(cx, e),
        }
    }
    pub(super) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, mut error: VenueError) {
        self.trading.waiting = false;
        if let Some(job) = self.trading.active.take() {
            error = error.partial_ids(&job.ids);
            if job.effective || !job.ids.is_empty() {
                error.outcome = Some(Outcome::Unknown);
            }
            if error.outcome == Some(Outcome::Unknown) {
                self.account.uncertain_writes += 1;
            }
            cx.fail(job.command, error);
        }
        self.trade_next(cx);
    }
    pub(super) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        match command {
            Command::PlaceOrders { command, orders } => match self.prepare_batch(cx, orders) {
                Ok(steps) => self.enqueue(cx, *command, steps, false),
                Err(e) => crate::public_context::local(cx, Operation::Mutation, Err(not_sent(e))),
            },
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => match self.amend(cx, id, *price, *amount) {
                Ok(s) => self.enqueue(cx, *command, VecDeque::from([s]), false),
                Err(e) => crate::public_context::local(cx, Operation::Mutation, Err(not_sent(e))),
            },
            Command::CancelOrders { command, ids } => {
                if ids.iter().any(String::is_empty) {
                    crate::public_context::local(
                        cx,
                        Operation::Mutation,
                        Err(not_sent("Kraken Spot empty cancel identity")),
                    )
                } else {
                    self.enqueue(
                        cx,
                        *command,
                        ids.iter()
                            .map(|id| Step::Cancel { id: id.clone() })
                            .collect(),
                        true,
                    )
                }
            }
            Command::CancelOrdersWithSymbols { command, orders } => {
                if orders.iter().any(|o| o.id.is_empty()) {
                    crate::public_context::local(
                        cx,
                        Operation::Mutation,
                        Err(not_sent("Kraken Spot empty cancel identity")),
                    )
                } else {
                    self.enqueue(
                        cx,
                        *command,
                        orders
                            .iter()
                            .map(|o| Step::Cancel { id: o.id.clone() })
                            .collect(),
                        true,
                    )
                }
            }
            Command::CancelSymbolOrders { command, symbol } => {
                let steps = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == *symbol && o.status == OrderStatus::Open)
                    .map(|o| Step::Cancel { id: o.id.clone() })
                    .collect();
                self.enqueue(cx, *command, steps, true)
            }
            // FetchAccount is refused by the engine (AccountLookup::Unsupported).
            Command::FetchAccount { .. } => {}
            Command::FetchOrderBook { .. } => crate::public_context::local(
                cx,
                Operation::Read,
                Err("Kraken Spot order book snapshots use subscriptions".into()),
            ),
            Command::FetchLeverage { .. } => {
                crate::public_context::local(cx, Operation::Read, Ok("null".into()))
            }
            command @ (Command::SetLeverage { .. }
            | Command::SetMarginType { .. }
            | Command::SetPositionMode { .. }
            | Command::FetchStpMode { .. }
            | Command::SetStpMode { .. }) => {
                let operation = if matches!(command, Command::FetchStpMode { .. }) {
                    Operation::Read
                } else {
                    Operation::Mutation
                };
                crate::public_context::local(
                    cx,
                    operation,
                    Err(
                        not_sent("Kraken Spot does not support this account setting")
                            .kind(ErrorKind::Unsupported),
                    ),
                );
            }
            Command::Query { command, query } => {
                let mut completion = Completion::new(*command);
                match query {
                    Query::OrderByClientId { .. } => {
                        completion.finish(cx, Ok(r#"{"found":false,"definitive":false}"#.into()));
                    }
                    Query::PublicTrades { .. } => {
                        completion.finish(
                            cx,
                            Err(
                                VenueError::new("Kraken Spot public trades are not implemented")
                                    .kind(ErrorKind::Unsupported)
                                    .outcome(Outcome::NotSent),
                            ),
                        );
                    }
                    Query::Fills { .. } | Query::OrderHistory { .. } | Query::Income { .. } => {
                        completion.incomplete(
                            "history is INCOMPLETE: Kraken Spot adapter has no native history read",
                        );
                        HistoryRows::<()>::new(completion, |rows| {
                            serde_json::to_string(rows)
                                .map_err(|error| VenueError::new(error.to_string()))
                        })
                        .finish(cx, Ok(()));
                    }
                }
            }
            _ => return false,
        }
        true
    }
}

/// Refused before any request was sent.
pub(super) fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A 2xx answer that does not prove what happened.
fn unproven(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

/// A refused private write, classified by HTTP status and Kraken's documented
/// error prefixes. 408/5xx and malformed answers do not prove a refusal.
pub(super) fn write_failure(status: u16, body: &str, message: String) -> VenueError {
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    match status {
        200..=299 => {}
        401 => return rejected(error, ErrorKind::AuthInvalid),
        403 => return rejected(error, ErrorKind::AuthPermission),
        429 => return rejected(error, ErrorKind::RateLimited),
        408 => return error.outcome(Outcome::Unknown),
        400..=499 => return rejected(error, ErrorKind::Rejected),
        _ => return error.outcome(Outcome::Unknown),
    }
    let Some(code) = serde_json::from_str::<super::wire::Envelope<'_>>(body)
        .ok()
        .and_then(|e| e.error.first().map(|code| code.to_string()))
    else {
        return error.outcome(Outcome::Unknown);
    };
    let error = match code.as_str() {
        "EAPI:Invalid key" | "EAPI:Invalid signature" | "EAPI:Invalid nonce" => {
            rejected(error, ErrorKind::AuthInvalid)
        }
        "EGeneral:Permission denied" => rejected(error, ErrorKind::AuthPermission),
        "EAPI:Rate limit exceeded" | "EOrder:Rate limit exceeded" => {
            rejected(error, ErrorKind::RateLimited)
        }
        "EService:Unavailable"
        | "EService:Busy"
        | "EService:Deadline elapsed"
        | "EGeneral:Internal error" => error.outcome(Outcome::Unknown),
        code if code.starts_with("EOrder:") || code.starts_with("EGeneral:Invalid arguments") => {
            rejected(error, ErrorKind::Rejected)
        }
        _ => error.outcome(Outcome::Unknown),
    };
    error.venue_code(code)
}
