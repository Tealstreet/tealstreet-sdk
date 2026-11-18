//! Commands are validated in full before the first write and submitted once.
use crate::{Kraken, Pending, sign};
use safe_cex_engine::classify::{
    self, CodeTable, Intent, Key, Native, NativeCode, Released, Row, Verdict,
};
use safe_cex_engine::model::*;
use safe_cex_engine::num;
use safe_cex_engine::request_context::Operation;
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Default)]
pub struct Trading {
    queue: VecDeque<Job>,
    active: Option<Job>,
    waiting: bool,
    deferred_failures: Vec<(safe_cex_engine::request_context::Context, VenueError)>,
}
struct Job {
    command: CommandId,
    root: Option<safe_cex_engine::request_context::Context>,
    steps: VecDeque<Step>,
    ids: Vec<String>,
    canceled_ids: Vec<String>,
    /// A write took effect, so a later failure cannot mean nothing happened.
    effective: bool,
}
#[derive(Clone)]
enum Step {
    Place(Box<Prepared>),
    Cancel(Vec<String>),
    /// Reaches orders the store has not seen; their rows follow from the stream.
    CancelSymbol(String),
    Edit {
        original: Box<Order>,
        replacement: Box<Order>,
        form: String,
    },
    Setting {
        symbol: String,
        leverage: f64,
        margin: MarginType,
        use_current: bool,
    },
    Leverage {
        symbol: String,
    },
}
impl Step {
    fn is_read(&self) -> bool {
        matches!(self, Self::Leverage { .. })
    }
}
#[derive(Clone)]
struct Prepared {
    form: String,
    order: Order,
}
pub struct Request {
    step: Step,
    settings_revision: u64,
    pub(super) context: Option<safe_cex_engine::request_context::Context>,
}
#[derive(Deserialize, Default)]
struct Reply {
    #[serde(default)]
    result: String,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    errors: Vec<ApiError>,
    #[serde(default, rename = "sendStatus")]
    send: Option<Status>,
    #[serde(default, rename = "editStatus")]
    edit: Option<Status>,
    #[serde(default, rename = "cancelStatus")]
    cancel: Option<Status>,
    #[serde(default, rename = "batchStatus")]
    batch: Option<Vec<Status>>,
    #[serde(default, rename = "leveragePreferences")]
    preferences: Option<Vec<Preference>>,
}
#[derive(Deserialize)]
struct ApiError {
    message: String,
}
#[derive(Deserialize)]
struct Status {
    #[serde(default)]
    status: String,
    #[serde(default, alias = "orderId")]
    order_id: String,
    #[serde(default, rename = "orderEvents", deserialize_with = "nullable_events")]
    events: Vec<OrderEvent>,
}
fn nullable_events<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<OrderEvent>, D::Error> {
    Ok(Option::<Vec<OrderEvent>>::deserialize(d)?.unwrap_or_default())
}
fn nullable_kind<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}
#[derive(Deserialize)]
struct OrderEvent {
    #[serde(default, rename = "type", deserialize_with = "nullable_kind")]
    kind: String,
    // Other event types carry other fields; only an execution's number counts.
    #[serde(default)]
    amount: Option<Box<serde_json::value::RawValue>>,
}
#[derive(Deserialize)]
struct Preference {
    symbol: String,
    #[serde(rename = "maxLeverage")]
    leverage: Option<f64>,
}
#[derive(Serialize)]
struct Batch<'a> {
    #[serde(rename = "batchOrder")]
    orders: Vec<Cancel<'a>>,
}
#[derive(Serialize)]
struct Cancel<'a> {
    order: &'static str,
    order_id: &'a str,
}

impl Kraken {
    pub(super) fn active_trading_command(&self) -> Option<CommandId> {
        self.trading.active.as_ref().map(|job| job.command)
    }
    pub(super) fn trading_root(&self) -> Option<&safe_cex_engine::request_context::Context> {
        self.trading
            .active
            .as_ref()
            .and_then(|job| job.root.as_ref())
    }
    pub(super) fn discard_provider_trade(
        &mut self,
        owner: safe_cex_engine::request_context::Owner,
    ) {
        if self
            .trading_root()
            .is_some_and(|root| root.owner() == owner)
        {
            self.trading.active = None;
            self.trading.waiting = false;
            let failures = &mut self.trading.deferred_failures;
            self.trading.queue.retain(|job| {
                if blocks_after_uncertainty(job)
                    && let Some(root) = &job.root
                {
                    failures.push((root.clone(), queued_uncertainty()));
                    false
                } else {
                    true
                }
            });
        }
        self.trading
            .queue
            .retain(|job| job.root.as_ref().is_none_or(|root| root.owner() != owner));
    }
    pub(super) fn trading_finish_deferred(&mut self, cx: &mut Cx<'_, Pending>) {
        for (root, error) in std::mem::take(&mut self.trading.deferred_failures) {
            let _ = cx.fail_context(&root, error);
        }
        self.trading_pump(cx);
    }
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: &Command) -> bool {
        let (id, result) = match command {
            Command::PlaceOrders { command, orders } => (*command, self.prepare_orders(cx, orders)),
            Command::CancelOrders { command, ids } => (*command, self.prepare_cancel(ids)),
            Command::CancelSymbolOrders { command, symbol } => {
                let ids = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.symbol == *symbol)
                    .map(|o| o.id.clone())
                    .collect::<Vec<_>>();
                let mut steps = self.prepare_cancel(&ids);
                if let Ok(steps) = &mut steps
                    && !symbol.is_empty()
                {
                    steps.push_back(Step::CancelSymbol(symbol.clone()));
                }
                (*command, steps)
            }
            Command::AmendOrder {
                command,
                id,
                price,
                amount,
            } => (*command, self.prepare_amend(cx, id, *price, *amount)),
            Command::SetLeverage {
                command,
                symbol,
                leverage,
                side: _,
            } => (
                *command,
                self.prepare_setting(cx, symbol, Some(*leverage), true),
            ),
            Command::SetMarginType {
                command,
                symbol,
                isolated,
            } => (*command, self.prepare_setting(cx, symbol, None, *isolated)),
            Command::FetchLeverage { command, symbol } => {
                let result = if cx.store.market(symbol).is_none() {
                    Err(format!("Unknown Kraken market {symbol}"))
                } else {
                    Ok(VecDeque::from([Step::Leverage {
                        symbol: symbol.clone(),
                    }]))
                };
                (*command, result)
            }
            Command::SetPositionMode { hedged, .. } => {
                if *hedged {
                    crate::public_context::local(
                        cx,
                        Operation::Mutation,
                        Err(not_sent("Kraken Futures only supports one-way positions")
                            .kind(ErrorKind::Unsupported)),
                    );
                } else {
                    let _ = cx.done_local_setting("null".into(), |_| {});
                }
                return true;
            }
            _ => return false,
        };
        match result {
            Err(error) => crate::public_context::local(
                cx,
                if matches!(command, Command::FetchLeverage { .. }) {
                    Operation::Read
                } else {
                    Operation::Mutation
                },
                Err(not_sent(error)),
            ),
            Ok(steps) => {
                if !self.can_sign() || self.options.public_data_only {
                    crate::public_context::local(
                        cx,
                        if matches!(command, Command::FetchLeverage { .. }) {
                            Operation::Read
                        } else {
                            Operation::Mutation
                        },
                        Err(not_sent("Kraken trading requires API credentials")
                            .kind(ErrorKind::AuthInvalid)),
                    );
                } else {
                    let root = if let Some(ids) = &self.identities {
                        use safe_cex_engine::credential_selection::CredentialRole;
                        let read = matches!(command, Command::FetchLeverage { .. });
                        let slots = sign::setup::futures_slots(ids);
                        let Ok((selection, identity)) = slots.select(if read {
                            CredentialRole::Read { scope: None }
                        } else {
                            CredentialRole::Mutation
                        }) else {
                            return true;
                        };
                        let mut policy = crate::public_context::policy(if read {
                            Operation::Read
                        } else {
                            Operation::Mutation
                        });
                        policy.credential = Some(selection.authority(identity));
                        let Ok(root) = cx.admit_http_command(policy) else {
                            return true;
                        };
                        Some(root)
                    } else {
                        None
                    };
                    self.trading.queue.push_back(Job {
                        command: id,
                        root,
                        steps,
                        ids: Vec::new(),
                        canceled_ids: Vec::new(),
                        effective: false,
                    });
                    self.trading_pump(cx);
                }
            }
        }
        true
    }
    fn prepare_orders(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<VecDeque<Step>, String> {
        if orders.iter().any(|o| o.client_order_id.is_some()) {
            return Err("Kraken does not support caller client order IDs".into());
        }
        if orders
            .iter()
            .any(|o| o.bybit_bbo.is_some() || o.bybit_slippage_tolerance_ticks.is_some())
        {
            return Err("Bybit execution controls are supported only on Bybit".into());
        }
        if orders.iter().any(|o| o.stp_mode.is_some()) {
            return Err("Kraken per-order self-trade prevention is unsupported".into());
        }
        let mut steps = VecDeque::new();
        for opts in orders {
            steps.push_back(Step::Place(Box::new(self.prepare_order(cx, opts)?)));
            // A zero bracket price means "no leg", as on the other venues.
            let leg = |price: Option<f64>| price.filter(|p| *p != 0.0);
            let (stop_loss, take_profit) = (leg(opts.stop_loss), leg(opts.take_profit));
            if stop_loss.is_some() || take_profit.is_some() {
                if opts.kind != OrderType::Limit {
                    return Err(
                        "Kraken does not support take-profit/stop-loss legs on a market entry; use a limit entry or place TP/SL separately"
                            .into(),
                    );
                }
                for (kind, price) in [
                    (OrderType::StopLoss, stop_loss),
                    (OrderType::TakeProfit, take_profit),
                ] {
                    if let Some(price) = price {
                        let mut leg = opts.clone();
                        leg.kind = kind;
                        leg.side = opts.side.opposite();
                        leg.price = Some(price);
                        leg.trigger_price = None;
                        leg.reduce_only = Some(true);
                        leg.stop_loss = None;
                        leg.take_profit = None;
                        leg.time_in_force = None;
                        steps.push_back(Step::Place(Box::new(self.prepare_order(cx, &leg)?)));
                    }
                }
            }
        }
        Ok(steps)
    }
    fn prepare_order(
        &self,
        cx: &Cx<'_, Pending>,
        opts: &PlaceOrderOpts,
    ) -> Result<Prepared, String> {
        if opts.client_order_id.is_some() {
            return Err("Kraken does not support caller client order IDs".into());
        }
        let market = cx
            .store
            .market(&opts.symbol)
            .ok_or_else(|| format!("Unknown Kraken market {}", opts.symbol))?;
        if !market.active || market.is_read_only == Some(true) {
            return Err("Kraken market is not tradable".into());
        }
        if (opts.trigger_price.is_some() && !opts.redundant_protective_trigger())
            || opts.kind == OrderType::Conditional
        {
            return Err("Kraken opening conditional orders are unsupported".into());
        }
        // Legacy TS kraken.exchange.ts:452 sends one rounded size, even above max.
        let amount = adjust(opts.amount, market.precision.amount)?;
        if amount <= 0.0 || amount < market.limits.amount.min {
            return Err("Invalid Kraken order size".into());
        }
        let kind = match (opts.kind, opts.time_in_force) {
            (OrderType::Limit, Some(TimeInForce::PostOnly)) => "post",
            (OrderType::Limit, Some(TimeInForce::ImmediateOrCancel)) => "ioc",
            (OrderType::Limit, Some(TimeInForce::FillOrKill)) => "fok",
            (OrderType::Limit, _) => "lmt",
            (OrderType::Market, _) => "mkt",
            (OrderType::StopLoss, _) => "stp",
            (OrderType::TakeProfit, _) => "take_profit",
            (OrderType::TrailingStopLoss, _) => "trailing_stop",
            _ => unreachable!(),
        };
        let protective = matches!(
            opts.kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        );
        let reduce_only = protective || opts.reduce_only == Some(true);
        let client = format!(
            "ts_{}",
            safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?
        );
        let mut params = vec![
            ("broker", "AA73 N84G 7YOF J26Y".to_owned()),
            ("cliOrdId", client.clone()),
            ("orderType", kind.into()),
            ("symbol", market.symbol.clone()),
            (
                "side",
                if opts.side == OrderSide::Buy {
                    "buy"
                } else {
                    "sell"
                }
                .into(),
            ),
            ("size", number(amount)),
        ];
        let price = if opts.kind == OrderType::Market {
            0.0
        } else {
            adjust(
                opts.price.ok_or("Kraken order requires a price")?,
                market.precision.price,
            )?
        };
        if opts.kind != OrderType::Market && price <= 0.0 {
            return Err("Invalid Kraken order price".into());
        }
        let trigger =
            if opts.kind == OrderType::StopLoss || opts.kind == OrderType::TrailingStopLoss {
                opts.sl_trigger_type.or(opts.trigger_type)
            } else {
                opts.tp_trigger_type.or(opts.trigger_type)
            };
        match opts.kind {
            OrderType::Limit => params.push(("limitPrice", number(price))),
            OrderType::StopLoss | OrderType::TakeProfit => {
                params.push(("stopPrice", number(price)))
            }
            OrderType::TrailingStopLoss => {
                let last = cx
                    .store
                    .ticker(&opts.symbol)
                    .map(|t| t.last)
                    .filter(|v| v.is_finite() && *v > 0.0)
                    .ok_or("Kraken trailing stop requires a current last price")?;
                let deviation = adjust((last - price).abs(), market.precision.price)?;
                if deviation <= 0.0 {
                    return Err("Kraken trailing price must differ from last".into());
                }
                params.push(("trailingStopMaxDeviation", number(deviation)));
                params.push(("trailingStopDeviationUnit", "QUOTE_CURRENCY".into()));
            }
            _ => {}
        }
        if protective && let Some(trigger) = trigger {
            params.push(("triggerSignal", signal(trigger).into()));
        }
        if reduce_only {
            params.push(("reduceOnly", "true".into()));
        }
        let order = Order {
            id: String::new(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: opts.symbol.clone(),
            kind: opts.kind,
            side: opts.side,
            price,
            amount,
            filled: 0.0,
            remaining: amount,
            reduce_only,
            close: opts.close,
            client_order_id: Some(client),
            time_in_force: opts.time_in_force,
            last_update: cx.now,
            trigger_type: trigger,
            take_profit: None,
            stop_loss: None,
            extra: None,
        };
        Ok(Prepared {
            form: sign::form(&params),
            order,
        })
    }
    fn prepare_cancel(&self, ids: &[String]) -> Result<VecDeque<Step>, String> {
        let unique = ids.iter().collect::<HashSet<_>>();
        if unique.len() != ids.len() || ids.iter().any(|id| id.is_empty()) {
            return Err("Kraken cancellation requires distinct nonempty IDs".into());
        }
        Ok(if ids.is_empty() {
            VecDeque::new()
        } else {
            VecDeque::from([Step::Cancel(ids.to_vec())])
        })
    }
    fn prepare_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<VecDeque<Step>, String> {
        let original = cx
            .store
            .order(id)
            .filter(|o| matches!(o.status, OrderStatus::Open | OrderStatus::Updating))
            .cloned()
            .ok_or("Kraken amendment requires a reconciled open order")?;
        if original.kind == OrderType::TrailingStopLoss {
            return Err(
                "Cannot safely amend a Kraken trailing stop: it would reset trailing state".into(),
            );
        }
        if price.is_none() && amount.is_none() {
            return Err("Kraken amendment requires a price or amount".into());
        }
        let market = cx
            .store
            .market(&original.symbol)
            .ok_or("Unknown Kraken market")?;
        let mut replacement = original.clone();
        let mut params = vec![("orderId", original.id.clone())];
        if let Some(amount) = amount {
            replacement.amount = adjust(amount, market.precision.amount)?;
            if replacement.amount <= original.filled
                || replacement.amount < market.limits.amount.min
                || (market.limits.amount.max > 0.0 && replacement.amount > market.limits.amount.max)
            {
                return Err("Invalid Kraken amendment size".into());
            }
            replacement.remaining = adjust(
                replacement.amount - original.filled,
                market.precision.amount,
            )?;
            // ABSOLUTE sets the total including fills: a fill the socket has not reported cannot grow it.
            params.push(("qtyMode", "ABSOLUTE".into()));
            params.push(("size", number(replacement.amount)));
        }
        let protective = matches!(original.kind, OrderType::StopLoss | OrderType::TakeProfit);
        if let Some(price) = price {
            replacement.price = adjust(price, market.precision.price)?;
            if replacement.price <= 0.0 {
                return Err("Invalid Kraken amendment price".into());
            }
            let field = if protective {
                "stopPrice"
            } else {
                "limitPrice"
            };
            params.push((field, number(replacement.price)));
        }
        if protective || original.kind == OrderType::Limit {
            Ok(VecDeque::from([Step::Edit {
                original: Box::new(original),
                replacement: Box::new(replacement),
                form: sign::form(&params),
            }]))
        } else {
            Err("Kraken amendment supports limit and reconciled protective orders only".into())
        }
    }
    fn prepare_setting(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        leverage: Option<f64>,
        isolated: bool,
    ) -> Result<VecDeque<Step>, String> {
        let market = cx.store.market(symbol).ok_or("Unknown Kraken market")?;
        let use_current = leverage.is_none() && isolated;
        let leverage = leverage
            .or(if !isolated {
                Some(market.limits.leverage.max)
            } else {
                None
            })
            .or_else(|| self.account_setting(symbol).map(|s| s.0))
            .or_else(|| {
                cx.store
                    .positions()
                    .iter()
                    .find(|p| p.symbol == symbol)
                    .map(|p| p.leverage)
            })
            .unwrap_or(market.limits.leverage.max);
        if !leverage.is_finite() || leverage <= 0.0 {
            return Err("Invalid Kraken leverage".into());
        }
        let leverage = num::clamp_leverage(
            leverage,
            market.limits.leverage.min,
            market.limits.leverage.max,
        )?;
        Ok(VecDeque::from([Step::Setting {
            symbol: symbol.into(),
            leverage,
            use_current,
            margin: if isolated {
                MarginType::Isolated
            } else {
                MarginType::Cross
            },
        }]))
    }
    fn trading_pump(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.trading.waiting {
            return;
        }
        loop {
            if self.trading.active.is_none() {
                self.trading.active = self.trading.queue.pop_front();
            }
            let Some(job) = self.trading.active.as_mut() else {
                return;
            };
            let Some(mut step) = job.steps.pop_front() else {
                let job = self.trading.active.take().unwrap();
                let result = serde_json::to_string(&job.ids).unwrap();
                if let Some(root) = &job.root {
                    let _ = cx.done_context(root, result);
                } else {
                    cx.done(job.command, serde_json::to_string(&job.ids).unwrap());
                }
                continue;
            };
            let command = job.command;
            if let Step::Setting {
                symbol,
                leverage,
                use_current: true,
                ..
            } = &mut step
                && let Some((confirmed, _)) = self.account_setting(symbol)
            {
                *leverage = confirmed;
            }
            let (method, path, form) = match &step {
                Step::Place(p) => (
                    Method::Post,
                    "/derivatives/api/v3/sendorder",
                    p.form.clone(),
                ),
                Step::Cancel(ids) if ids.len() == 1 => (
                    Method::Post,
                    "/derivatives/api/v3/cancelorder",
                    sign::form(&[("order_id", ids[0].clone())]),
                ),
                Step::Cancel(ids) => {
                    let body = Batch {
                        orders: ids
                            .iter()
                            .map(|id| Cancel {
                                order: "cancel",
                                order_id: id,
                            })
                            .collect(),
                    };
                    (
                        Method::Post,
                        "/derivatives/api/v3/batchorder",
                        format!(
                            "json={}",
                            safe_cex_engine::url::encode_component(
                                &serde_json::to_string(&body).unwrap()
                            )
                        ),
                    )
                }
                Step::CancelSymbol(symbol) => (
                    Method::Post,
                    "/derivatives/api/v3/cancelallorders",
                    sign::form(&[("symbol", symbol.to_uppercase())]),
                ),
                Step::Edit { form, .. } => {
                    (Method::Post, "/derivatives/api/v3/editorder", form.clone())
                }
                Step::Setting {
                    symbol,
                    leverage,
                    margin,
                    ..
                } => {
                    let mut params = vec![("symbol", symbol.to_uppercase())];
                    if *margin == MarginType::Isolated {
                        params.push(("maxLeverage", number(*leverage)));
                    }
                    (
                        Method::Put,
                        "/derivatives/api/v3/leveragepreferences",
                        sign::form(&params),
                    )
                }
                Step::Leverage { symbol } => {
                    if let Some((leverage, margin)) = self.account_setting(symbol) {
                        let data = leverage_result(cx, symbol, leverage, margin);
                        self.trading_finish(cx, Ok(data));
                        continue;
                    }
                    (
                        Method::Get,
                        "/derivatives/api/v3/leveragepreferences",
                        String::new(),
                    )
                }
            };
            self.trading.waiting = true;
            if let Step::Place(prepared) = &step {
                cx.command_progress(
                    command,
                    prepared
                        .order
                        .client_order_id
                        .iter()
                        .map(|identity| OrderIdentityReceipt {
                            symbol: prepared.order.symbol.clone(),
                            kind: OrderIdentityKind::ClientOrderId,
                            identity: identity.clone(),
                        })
                        .collect(),
                );
            }
            if let Err(error) = self.signed_request(
                cx,
                method,
                path,
                form,
                Pending::Trade(Request {
                    step,
                    settings_revision: self.account_settings_revision(),
                    context: None,
                }),
            ) {
                self.trading_finish(cx, Err(not_sent(error)));
                continue;
            }
            return;
        }
    }
    pub(crate) fn trading_http(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let context = request.context.clone();
        let before = self
            .trading
            .active
            .as_ref()
            .map_or(0, |job| job.ids.len() + job.canceled_ids.len());
        let ok = (200..300).contains(&response.status);
        let leverage_read = request.step.is_read();
        let intent = match &request.step {
            Step::Place(_) => Intent::Place,
            Step::Cancel(_) | Step::CancelSymbol(_) => Intent::Cancel,
            Step::Edit { .. } => Intent::Amend,
            Step::Setting { .. } => Intent::Setting,
            Step::Leverage { .. } => Intent::Read,
        };
        let reply = serde_json::from_str::<Reply>(response.body).map_err(|_| {
            let message = "Invalid Kraken trading response";
            if ok {
                unproven(message)
            } else {
                classified_failure(&response, intent, None, message)
            }
        });
        let native_code = reply
            .as_ref()
            .ok()
            .and_then(|r| {
                r.error
                    .as_ref()
                    .filter(|code| !code.is_empty())
                    .or(r.status.as_ref())
                    .or_else(|| {
                        r.send
                            .as_ref()
                            .or(r.edit.as_ref())
                            .or(r.cancel.as_ref())
                            .map(|status| &status.status)
                    })
            })
            .cloned();
        let result = reply
            .and_then(|r| {
                if !ok
                    || r.result != "success"
                    || matches!(r.status.as_deref(), Some("BAD_REQUEST" | "unauthorized"))
                    || r.error.as_ref().is_some_and(|e| !e.is_empty())
                    || !r.errors.is_empty()
                {
                    let code = r
                        .error
                        .as_deref()
                        .filter(|code| !code.is_empty())
                        .or_else(|| r.errors.first().map(|e| e.message.as_str()));
                    let message = if !ok {
                        let detail = code.map(|code| format!(": {code}")).unwrap_or_default();
                        format!("Kraken HTTP {}{detail}", response.status)
                    } else {
                        code.unwrap_or("Kraken rejected trading request").to_owned()
                    };
                    let code = code.or(r
                        .status
                        .as_deref()
                        .filter(|status| matches!(*status, "BAD_REQUEST" | "unauthorized")));
                    return Err(classified_failure(&response, intent, code, message));
                }
                self.trading_apply(cx, request.step, request.settings_revision, r)
            })
            .map_err(|error| {
                let partial = self
                    .trading
                    .active
                    .as_ref()
                    .is_some_and(|job| job.effective || !job.ids.is_empty());
                trading_failure(error, partial)
            });
        if let Some(context) = context.as_ref().filter(|_| !leverage_read) {
            let ids = self
                .trading
                .active
                .as_ref()
                .map(|job| {
                    job.ids
                        .iter()
                        .chain(&job.canceled_ids)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let fact = match &result {
                Ok(_) => safe_cex_engine::classify::WriteFact::Accepted { order_ids: ids },
                Err(error) => {
                    let code = error
                        .venue_code
                        .as_deref()
                        .or(native_code.as_deref())
                        .map(|code| code.split(':').next().unwrap_or(code).trim());
                    let native = code.map(|code| Native {
                        code: NativeCode::Text(code),
                        message: code,
                    });
                    let table = if code.is_some_and(documented_status_refusal) {
                        &STATUS_CODES
                    } else {
                        &ERROR_CODES
                    };
                    let Ok(fact) =
                        cx.classify_write(context, table, native.as_ref(), &error.message)
                    else {
                        self.trading_finish(
                            cx,
                            Err(unproven("Kraken write evidence is unavailable")),
                        );
                        return;
                    };
                    let prior_effects = before > 0
                        || self
                            .trading
                            .active
                            .as_ref()
                            .is_some_and(|job| job.effective);
                    safe_cex_engine::classify::WriteFact::Failed {
                        fact: if error.outcome == Some(Outcome::Rejected) && !prior_effects {
                            fact
                        } else {
                            fact.unproven()
                        }
                        .with_order_ids(&ids),
                        known_effects: ids.len() > before,
                    }
                }
            };
            if let Err(reason) = cx.http_write_fact(context, fact, None) {
                self.trading_finish(cx, Err(reason.venue_error().outcome(Outcome::Unknown)));
                return;
            }
        }
        match result {
            Ok(Some(result)) => self.trading_finish(cx, Ok(result)),
            Ok(None) => {
                self.trading.waiting = false;
                if let Some(job) = self.trading.active.as_mut() {
                    job.effective = true;
                }
            }
            Err(mut error) => {
                // A leverage read has no write outcome.
                if leverage_read {
                    error.outcome = None;
                }
                self.trading_finish(cx, Err(error))
            }
        }
        if self.identities.is_none() {
            self.trading_pump(cx);
        }
    }
    fn trading_apply(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        step: Step,
        settings_revision: u64,
        reply: Reply,
    ) -> Result<Option<String>, VenueError> {
        match step {
            Step::Place(mut p) => {
                let status = reply
                    .send
                    .ok_or_else(|| unproven("Kraken placement missing sendStatus"))?;
                // The sendStatus enum lists partiallyFilled: the entry is live and its brackets follow.
                if !matches!(
                    status.status.as_str(),
                    "placed" | "partiallyFilled" | "filled"
                ) || status.order_id.is_empty()
                {
                    return Err(status_failure(
                        format!("Kraken placement rejected or incomplete: {}", status.status),
                        &status.status,
                        false,
                    ));
                }
                if self
                    .trading
                    .active
                    .as_ref()
                    .is_some_and(|j| j.ids.contains(&status.order_id))
                {
                    return Err(unproven("Kraken placement returned duplicate order ID"));
                }
                p.order.id = status.order_id.clone();
                p.order.last_update = cx.now;
                let executed: f64 = status
                    .events
                    .iter()
                    .filter(|event| event.kind == "EXECUTION")
                    .filter_map(|event| event.amount.as_ref()?.get().parse::<f64>().ok())
                    .sum();
                // A market or IOC order never rests, whatever its status says.
                let rests = !matches!(p.order.kind, OrderType::Market)
                    && !matches!(
                        p.order.time_in_force,
                        Some(TimeInForce::ImmediateOrCancel | TimeInForce::FillOrKill)
                    )
                    && executed < p.order.amount;
                if status.status == "placed" && rests {
                    let order = cx
                        .store
                        .order(&p.order.id)
                        .cloned()
                        .unwrap_or_else(|| p.order.clone());
                    self.account_order_accepted(cx, &order);
                } else {
                    self.account_order_removed(cx, &p.order.id);
                }
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .ids
                    .push(status.order_id);
            }
            Step::CancelSymbol(_) => {
                let status = reply
                    .cancel
                    .ok_or_else(|| unproven("Kraken cancel-all missing cancelStatus"))?;
                if !matches!(status.status.as_str(), "cancelled" | "noOrdersToCancel") {
                    return Err(unproven(format!(
                        "Kraken cancel-all not confirmed: {}",
                        status.status
                    )));
                }
            }
            Step::Cancel(ids) => {
                let statuses = if ids.len() == 1 {
                    vec![
                        reply
                            .cancel
                            .ok_or_else(|| unproven("Kraken cancellation missing cancelStatus"))?,
                    ]
                } else {
                    reply
                        .batch
                        .ok_or_else(|| unproven("Kraken batch cancellation missing batchStatus"))?
                };
                if statuses.len() != ids.len() {
                    return Err(unproven(
                        "Kraken batch cancellation response count mismatch",
                    ));
                }
                let mut seen = HashSet::new();
                let mut error: Option<VenueError> = None;
                let mut cancelled = false;
                for mut status in statuses {
                    // Already gone counts as done; the reconcile settles how it ended.
                    let gone = matches!(status.status.as_str(), "notFound" | "filled");
                    // A lone cancel's notFound may omit order_id, as the docs allow.
                    if gone && ids.len() == 1 && status.order_id.is_empty() {
                        status.order_id = ids[0].clone();
                    }
                    if (status.status == "cancelled" || gone)
                        && ids.contains(&status.order_id)
                        && seen.insert(status.order_id.clone())
                    {
                        cancelled = true;
                        if let Some(job) = self.trading.active.as_mut() {
                            job.canceled_ids.push(status.order_id.clone());
                        }
                        self.account_order_removed(cx, &status.order_id);
                    } else {
                        let message = format!(
                            "Kraken cancellation rejected or identity mismatch: {}",
                            status.status
                        );
                        let failure = if ids.contains(&status.order_id) {
                            status_failure(message, &status.status, false)
                        } else {
                            unproven(message)
                        };
                        // Keep an unknown leg's text and native diagnostics together.
                        error = match error {
                            Some(first) if first.outcome == Some(Outcome::Unknown) => Some(first),
                            _ => Some(failure),
                        };
                    }
                }
                if let Some(mut error) = error {
                    // Other orders in the batch were cancelled.
                    if cancelled && error.outcome == Some(Outcome::Rejected) {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    return Err(error);
                }
            }
            Step::Edit {
                original,
                mut replacement,
                ..
            } => {
                let status = reply
                    .edit
                    .ok_or_else(|| unproven("Kraken amendment missing editStatus"))?;
                if !matches!(status.status.as_str(), "edited" | "filled")
                    || status.order_id != original.id
                {
                    let message = format!(
                        "Kraken amendment rejected or identity mismatch: {}",
                        status.status
                    );
                    return Err(if status.order_id == original.id {
                        status_failure(message, &status.status, true)
                    } else {
                        unproven(message)
                    });
                }
                if status.status == "edited"
                    && let Some(current) = cx.store.order(&original.id).cloned()
                {
                    if current.price == original.price
                        && current.amount == original.amount
                        && current.kind == original.kind
                        && current.side == original.side
                        && current.trigger_type == original.trigger_type
                    {
                        replacement.filled = current.filled.max(replacement.filled);
                        replacement.remaining = (replacement.amount - replacement.filled).max(0.0);
                        replacement.last_update = cx.now;
                        self.account_order_accepted(cx, &replacement);
                    } else {
                        self.account_order_accepted(cx, &current);
                    }
                }
                if status.status == "filled" {
                    self.account_order_removed(cx, &original.id);
                }
                self.trading
                    .active
                    .as_mut()
                    .unwrap()
                    .ids
                    .push(status.order_id);
            }
            Step::Setting {
                symbol,
                leverage,
                margin,
                ..
            } => self.account_settings_accepted(cx, &symbol, leverage, margin),
            Step::Leverage { symbol } => {
                let prefs = reply
                    .preferences
                    .ok_or("Kraken leverage response missing preferences")?;
                let market = cx.store.market(&symbol).ok_or("Unknown Kraken market")?;
                // A read: no outcome to classify.
                let leverage = prefs
                    .iter()
                    .find(|p| p.symbol.eq_ignore_ascii_case(&market.symbol))
                    .and_then(|p| p.leverage)
                    .filter(|n| n.is_finite() && *n > 0.0);
                let margin = if leverage.is_some() {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                };
                let leverage = leverage.unwrap_or(market.limits.leverage.max);
                if settings_revision == self.account_settings_revision() {
                    self.account_settings_accepted(cx, &symbol, leverage, margin);
                }
                let (leverage, margin) =
                    self.account_setting(&symbol).unwrap_or((leverage, margin));
                return Ok(Some(leverage_result(cx, &symbol, leverage, margin)));
            }
        }
        Ok(None)
    }
    fn trading_finish(&mut self, cx: &mut Cx<'_, Pending>, result: Result<String, VenueError>) {
        self.trading.waiting = false;
        if let Some(job) = self.trading.active.take() {
            match result {
                Ok(result) => {
                    if let Some(root) = &job.root {
                        let _ = cx.done_context(root, result);
                    } else {
                        cx.done(job.command, result);
                    }
                }
                Err(mut error) => {
                    let ids: Vec<_> = job
                        .canceled_ids
                        .iter()
                        .cloned()
                        .chain(job.ids.iter().cloned())
                        .collect();
                    error = error.partial_ids(&ids);
                    error = trading_failure(error, job.effective || !job.ids.is_empty());
                    if error.outcome == Some(Outcome::Rejected) {
                        cx.placement_resolved(job.command);
                    }
                    let unknown = error.outcome == Some(Outcome::Unknown);
                    fail_job(cx, &job, error);
                    if unknown {
                        // Admission preceded this uncertainty; never pump those
                        // placements or edits. Cancels, settings and reads remain.
                        let failures = &mut self.trading.deferred_failures;
                        self.trading.queue.retain(|queued| {
                            if !blocks_after_uncertainty(queued) {
                                return true;
                            }
                            if let Some(root) = &queued.root {
                                failures.push((root.clone(), queued_uncertainty()));
                            } else {
                                fail_job(cx, queued, queued_uncertainty());
                            }
                            false
                        });
                    }
                }
            }
        }
    }
    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let error = VenueError::new(format!(
            "Kraken request failed; write outcome may be uncertain: {error}"
        ))
        .kind(ErrorKind::Network);
        let error = if request.step.is_read() {
            error
        } else {
            error.outcome(Outcome::Unknown)
        };
        self.trading_finish(cx, Err(error));
        if self.identities.is_none() {
            self.trading_pump(cx);
        }
    }
    pub(crate) fn trading_sign_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _request: Request,
        error: VenueError,
    ) {
        self.trading_finish(cx, Err(error.outcome(Outcome::NotSent)));
        if self.identities.is_none() {
            self.trading_pump(cx);
        }
    }
}

fn fail_job(cx: &mut Cx<'_, Pending>, job: &Job, error: VenueError) {
    if let Some(root) = &job.root {
        let _ = cx.fail_context(root, error);
    } else {
        cx.fail(job.command, error);
    }
}
fn blocks_after_uncertainty(job: &Job) -> bool {
    job.steps
        .iter()
        .any(|step| matches!(step, Step::Place(_) | Step::Edit { .. }))
}
fn queued_uncertainty() -> VenueError {
    not_sent(
        "Kraken Futures queued write was not sent: earlier write outcome is uncertain. Check Open Orders before retrying",
    )
}
fn trading_failure(mut error: VenueError, partial: bool) -> VenueError {
    if partial {
        error.outcome = Some(Outcome::Unknown);
    }
    if error.outcome == Some(Outcome::Unknown) && !error.message.contains("Check Open Orders") {
        error.message = format!(
            "Kraken write outcome is unknown. Check Open Orders: {}",
            error.message
        );
    }
    error
}
/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}

/// A 2xx answer that does not prove what happened.
fn unproven(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::Unknown)
}

// Native error codes retain authority on non-2xx replies; the shared classifier
// keeps transport, 408 and 5xx ahead of every native refusal.
const ERROR_CODES: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Text("authenticationError"),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Text("unauthorized"),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Text("apiLimitExceeded"),
            verdict: Verdict::Refused(ErrorKind::RateLimited),
            only: None,
        },
        Row {
            key: Key::Text("accountInactive"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("insufficientFunds"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidAccount"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidAmount"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        // "The request failed to pass valid JSON": docs.kraken.com send-order, Error schema.
        Row {
            key: Key::Text("Json Parse Error"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidArgument"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidSide"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("MAX_LEVERAGE_OUT_OF_BOUNDS"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidUnit"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("marketUnavailable"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("nonceBelowThreshold"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("nonceDuplicate"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("notFound"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("requiredArgumentMissing"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("BAD_REQUEST"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("Server Error"),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Text("Unavailable"),
            verdict: Verdict::Unknown(None),
            only: None,
        },
        Row {
            key: Key::Text("unknownError"),
            verdict: Verdict::Unknown(None),
            only: None,
        },
    ],
    success: &[Key::Text("success")],
    unlisted: Verdict::Unknown(None),
    status: &[
        (400, Verdict::Refused(ErrorKind::Rejected)),
        (401, Verdict::Refused(ErrorKind::AuthInvalid)),
        (403, Verdict::Refused(ErrorKind::AuthPermission)),
        (404, Verdict::Refused(ErrorKind::Rejected)),
        (429, Verdict::Refused(ErrorKind::RateLimited)),
    ],
};
fn classified_failure(
    response: &Response<'_>,
    intent: Intent,
    code: Option<&str>,
    message: impl Into<String>,
) -> VenueError {
    // "nonceBelowThreshold: TOO_SMALL": the code is what precedes the detail.
    let native = code
        .map(|code| code.split(':').next().unwrap_or(code).trim())
        .map(|code| Native {
            code: NativeCode::Text(code),
            message: code,
        });
    let released = Released::Response(response);
    let verdict = classify::verdict(&ERROR_CODES, intent, released, native.as_ref());
    let mut error = classify::error(verdict, intent, released, native.as_ref(), message);
    if intent != Intent::Read
        && error.outcome == Some(Outcome::Unknown)
        && (matches!(response.status, 408 | 429 | 500..)
            || error.kind == Some(ErrorKind::RateLimited))
    {
        error.message = "Kraken write outcome is unknown. Check Open Orders".into();
    }
    error
}

// Native order-status vocabulary shared by local and provider classifications.
const STATUS_CODES: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Text("invalidOrderType"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidSide"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("clientOrderIdTooLong"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidDomainConfiguration"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("marketSuspended"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("marketInactive"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidPrice"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("invalidSize"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("tooManySmallOrders"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("insufficientAvailableFunds"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("wouldCauseLiquidation"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("clientOrderIdTooBig"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("clientOrderIdInvalid"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("maxPositionViolation"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("outsidePriceCollar"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("wouldIncreasePriceDispersion"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("wouldIncreasePriceDislocation"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("notFound"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("orderForEditNotAStop"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("orderForEditNotFound"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("postWouldExecute"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("iocWouldNotExecute"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("selfFill"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("wouldNotReducePosition"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("marketIsPostOnly"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("tooManyOrders"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("fixedLeverageTooHigh"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("cannotEditTriggerPriceOfTrailingStop"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
        Row {
            key: Key::Text("cannotEditLimitPriceOfTrailingStop"),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: None,
        },
    ],
    success: &[],
    unlisted: Verdict::Unknown(None),
    status: ERROR_CODES.status,
};
fn documented_status_refusal(status: &str) -> bool {
    STATUS_CODES
        .rows
        .iter()
        .any(|row| matches!(row.key, Key::Text(code) if code == status))
}

/// A send/edit/cancel status that is not success. Only Kraken's documented
/// refusal statuses prove nothing happened.
fn status_failure(message: String, status: &str, edit: bool) -> VenueError {
    let error = VenueError::new(message);
    // A success status with a missing or mismatched id does not prove what happened.
    if status.is_empty() || matches!(status, "placed" | "filled" | "cancelled" | "edited") {
        return error.outcome(Outcome::Unknown);
    }
    // On an edit these cancel the resting order, possibly after fills.
    if edit && matches!(status, "postWouldExecute" | "iocWouldNotExecute") {
        return error.outcome(Outcome::Unknown).venue_code(status);
    }
    // Documented: docs.kraken.com/api/docs/futures-api/trading/send-order
    let refused = documented_status_refusal(status);
    if refused {
        error
            .kind(ErrorKind::Rejected)
            .outcome(Outcome::Rejected)
            .venue_code(status)
    } else {
        error.outcome(Outcome::Unknown).venue_code(status)
    }
}

fn number(n: f64) -> String {
    safe_cex_engine::num::format(n)
}
fn adjust(value: f64, step: f64) -> Result<f64, String> {
    if !value.is_finite() || !step.is_finite() || step <= 0.0 {
        return Err("Kraken requires finite numbers and valid precision".into());
    }
    let rounded = safe_cex_engine::num::adjust(value, step);
    if !rounded.is_finite() {
        return Err("Kraken numeric value overflow".into());
    }
    Ok(rounded)
}
fn signal(trigger: StopTriggerType) -> &'static str {
    match trigger {
        StopTriggerType::MarkPrice => "mark",
        StopTriggerType::LastPrice => "last",
        StopTriggerType::IndexPrice => "index",
    }
}
fn leverage_result(
    cx: &Cx<'_, Pending>,
    symbol: &str,
    leverage: f64,
    margin: MarginType,
) -> String {
    let _ = cx.store.market(symbol).unwrap();
    format!(
        "{{\"buyLeverage\":{leverage},\"sellLeverage\":{leverage},\"marginType\":\"{}\",\"positionMode\":\"oneway\"}}",
        if margin == MarginType::Isolated {
            "isolated"
        } else {
            "cross"
        }
    )
}
