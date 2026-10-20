use super::{
    KrakenSpot, Pending,
    account::{Kind, order_kind},
};
use safe_cex_engine::{Command, CommandId, Cx, Query, model::*, num};
use serde::Deserialize;
use std::collections::VecDeque;

#[derive(Clone)]
pub enum Step {
    Place {
        params: Vec<(&'static str, String)>,
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
        getrandom::getrandom(&mut entropy)
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
            ("cl_ord_id", client_id),
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
        Ok(Step::Place { params })
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
            if o.kind == OrderType::Limit {
                for (kind, price) in [
                    (OrderType::StopLoss, o.stop_loss),
                    (OrderType::TakeProfit, o.take_profit),
                ] {
                    if let Some(price) = price
                        && price != 0.
                    {
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
                Ok(if job.cancel {
                    "null".into()
                } else {
                    serde_json::to_string(&job.ids).unwrap()
                }),
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
            self.trade_failed(cx, "Kraken Spot order closed before amend");
            return;
        }
        let (path, params) = match &step {
            Step::Place { params } => ("/0/private/AddOrder", params.clone()),
            Step::Amend { params, .. } => ("/0/private/AmendOrder", params.clone()),
            Step::Cancel { id } => ("/0/private/CancelOrder", vec![("txid", id.clone())]),
        };
        self.trading.waiting = true;
        self.request_private(cx, path, params, Kind::Trade(step));
    }
    pub(super) fn trade_response(&mut self, cx: &mut Cx<'_, Pending>, step: Step, text: &str) {
        self.trading.waiting = false;
        let r = (|| {
            let Some(job) = self.trading.active.as_mut() else {
                return Err("Kraken Spot unexpected trade reply".into());
            };
            match step {
                Step::Place { .. } => {
                    let r: Placed = serde_json::from_str(text)
                        .map_err(|_| "Kraken Spot missing placement txids")?;
                    if r.txid.is_empty() || r.txid.iter().any(String::is_empty) {
                        return Err("Kraken Spot empty placement txid".into());
                    }
                    job.ids.extend(r.txid);
                }
                Step::Amend { id, .. } => {
                    let r: Amended =
                        serde_json::from_str(text).map_err(|_| "Kraken Spot missing amend_id")?;
                    if r.amend_id.is_empty() {
                        return Err("Kraken Spot missing amend_id".into());
                    }
                    job.ids.push(id);
                }
                Step::Cancel { .. } => {
                    let r: Canceled = serde_json::from_str(text)
                        .map_err(|_| "Kraken Spot missing cancel count")?;
                    if r.count == 0 {
                        return Err("Kraken Spot order was not canceled".into());
                    }
                }
            }
            Ok::<_, String>(())
        })();
        match r {
            Ok(()) => {
                self.account.revision += 1;
                self.trade_next(cx)
            }
            Err(e) => self.trade_failed(cx, &e),
        }
    }
    pub(super) fn trade_failed(&mut self, cx: &mut Cx<'_, Pending>, error: &str) {
        self.trading.waiting = false;
        if let Some(job) = self.trading.active.take() {
            cx.done(job.command, Err(error.into()));
        }
        self.trade_next(cx);
    }
    pub(super) fn trade_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        match command {
            Command::PlaceOrders { command, orders } => match self.prepare_batch(cx, orders) {
                Ok(steps) => self.enqueue(cx, *command, steps, false),
                Err(e) => cx.done(*command, Err(e)),
            },
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => match self.amend(cx, id, *price, *amount) {
                Ok(s) => self.enqueue(cx, *command, VecDeque::from([s]), false),
                Err(e) => cx.done(*command, Err(e)),
            },
            Command::CancelOrders { command, ids } => {
                if ids.iter().any(String::is_empty) {
                    cx.done(*command, Err("Kraken Spot empty cancel identity".into()))
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
                    cx.done(*command, Err("Kraken Spot empty cancel identity".into()))
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
            Command::FetchAccount { command } => self.balance_read(cx, Some(*command)),
            Command::FetchOrderBook { command, .. } => cx.done(
                *command,
                Err("Kraken Spot order book snapshots use subscriptions".into()),
            ),
            Command::FetchLeverage { command, .. } => cx.done(*command, Ok("null".into())),
            Command::SetLeverage { command, .. }
            | Command::SetMarginType { command, .. }
            | Command::SetPositionMode { command, .. }
            | Command::FetchStpMode { command }
            | Command::SetStpMode { command, .. } => cx.done(
                *command,
                Err("Kraken Spot does not support this account setting".into()),
            ),
            Command::Query { command, query } => match query {
                Query::OrderByClientId { .. } => {
                    cx.done(*command, Ok(r#"{"found":false,"definitive":false}"#.into()))
                }
                Query::PublicTrades { .. } => cx.done(
                    *command,
                    Err("Kraken Spot public trades are not implemented".into()),
                ),
                _ => cx.done(*command, Ok("[]".into())),
            },
            _ => return false,
        }
        true
    }
}
