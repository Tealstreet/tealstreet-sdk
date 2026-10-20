//! Commands are validated in full before the first write and submitted once.
use crate::{Kraken, Pending, sign};
use safe_cex_engine::model::*;
use safe_cex_engine::num;
use safe_cex_engine::{Command, CommandId, Cx, Method, Response};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

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
    amend: Option<Order>,
    amend_filled: bool,
    amend_canceled: bool,
}
#[derive(Clone)]
enum Step {
    Place(Box<Prepared>),
    Cancel(Vec<String>),
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
#[derive(Clone)]
struct Prepared {
    form: String,
    order: Order,
}
pub struct Request {
    step: Step,
    settings_revision: u64,
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
    pub(crate) fn trade_order_observed(&mut self, id: &str, filled: f64, status: OrderStatus) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job
                .amend
                .as_ref()
                .is_some_and(|o| o.id == id && (filled > o.filled || status == OrderStatus::Closed))
            {
                job.amend_filled = true;
            }
        }
    }
    pub(crate) fn trade_execution_observed(&mut self, id: &str) {
        for job in self
            .trading
            .active
            .iter_mut()
            .chain(self.trading.queue.iter_mut())
        {
            if job.amend.as_ref().is_some_and(|o| o.id == id) {
                job.amend_filled = true;
            }
        }
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
                (*command, self.prepare_cancel(&ids))
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
                    Ok((
                        VecDeque::from([Step::Leverage {
                            symbol: symbol.clone(),
                        }]),
                        None,
                    ))
                };
                (*command, result)
            }
            Command::SetPositionMode {
                command,
                hedged,
                symbol: _,
            } => {
                cx.done(
                    *command,
                    if *hedged {
                        Err("Kraken Futures only supports one-way positions".into())
                    } else {
                        Ok("null".into())
                    },
                );
                return true;
            }
            _ => return false,
        };
        match result {
            Err(error) => cx.done(id, Err(error)),
            Ok((steps, amend)) => {
                if self.signer.is_none() || self.options.public_data_only {
                    cx.done(id, Err("Kraken trading requires API credentials".into()));
                } else {
                    self.trading.queue.push_back(Job {
                        command: id,
                        steps,
                        ids: Vec::new(),
                        amend,
                        amend_filled: false,
                        amend_canceled: false,
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
    ) -> Result<(VecDeque<Step>, Option<Order>), String> {
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
        Ok((steps, None))
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
        if opts.trigger_price.is_some() || opts.kind == OrderType::Conditional {
            return Err("Kraken opening conditional orders are unsupported".into());
        }
        let amount = adjust(opts.amount, market.precision.amount)?;
        if amount <= 0.0
            || amount < market.limits.amount.min
            || (market.limits.amount.max > 0.0 && amount > market.limits.amount.max)
        {
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
        let client = format!("ts_{}", safe_cex_crypto::uuid::uuid()?);
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
    fn prepare_cancel(&self, ids: &[String]) -> Result<(VecDeque<Step>, Option<Order>), String> {
        let unique = ids.iter().collect::<HashSet<_>>();
        if unique.len() != ids.len() || ids.iter().any(|id| id.is_empty()) {
            return Err("Kraken cancellation requires distinct nonempty IDs".into());
        }
        Ok((
            if ids.is_empty() {
                VecDeque::new()
            } else {
                VecDeque::from([Step::Cancel(ids.to_vec())])
            },
            None,
        ))
    }
    fn prepare_amend(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        price: Option<f64>,
        amount: Option<f64>,
    ) -> Result<(VecDeque<Step>, Option<Order>), String> {
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
            replacement.remaining = replacement.amount - original.filled;
            params.push(("size", number(replacement.amount)));
        }
        if let Some(price) = price {
            replacement.price = adjust(price, market.precision.price)?;
            if replacement.price <= 0.0 {
                return Err("Invalid Kraken amendment price".into());
            }
            params.push(("limitPrice", number(replacement.price)));
        }
        if matches!(original.kind, OrderType::StopLoss | OrderType::TakeProfit) {
            let trigger = original
                .trigger_type
                .or_else(|| {
                    original.extra.as_ref().and_then(|e| {
                        if original.kind == OrderType::StopLoss {
                            e.sl_trigger_type
                        } else {
                            e.tp_trigger_type
                        }
                    })
                })
                .ok_or("Cannot safely amend Kraken protective order without trigger provenance")?;
            let opts = PlaceOrderOpts {
                symbol: original.symbol.clone(),
                kind: original.kind,
                side: original.side,
                amount: replacement.amount - original.filled,
                price: Some(replacement.price),
                trigger_price: None,
                max_slippage_pct: None,
                stp_mode: None,
                bybit_bbo: None,
                bybit_slippage_tolerance_ticks: None,
                implied_volatility: None,
                stop_loss: None,
                take_profit: None,
                reduce_only: Some(true),
                time_in_force: None,
                trigger_type: Some(trigger),
                tp_trigger_type: None,
                sl_trigger_type: None,
                client_order_id: None,
                close: original.close,
            };
            let prepared = self.prepare_order(cx, &opts)?;
            Ok((
                VecDeque::from([
                    Step::Cancel(vec![original.id.clone()]),
                    Step::Place(Box::new(prepared)),
                ]),
                Some(original),
            ))
        } else if original.kind == OrderType::Limit {
            Ok((
                VecDeque::from([Step::Edit {
                    original: Box::new(original),
                    replacement: Box::new(replacement),
                    form: sign::form(&params),
                }]),
                None,
            ))
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
    ) -> Result<(VecDeque<Step>, Option<Order>), String> {
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
        Ok((
            VecDeque::from([Step::Setting {
                symbol: symbol.into(),
                leverage,
                use_current,
                margin: if isolated {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                },
            }]),
            None,
        ))
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
                cx.done(job.command, Ok(serde_json::to_string(&job.ids).unwrap()));
                continue;
            };
            if matches!(&step, Step::Cancel(_))
                && let Some(old) = job.amend.as_ref()
                && cx.store.order(&old.id).is_none_or(|current| {
                    current.filled > old.filled
                        || current.price != old.price
                        || current.amount != old.amount
                        || current.trigger_type != old.trigger_type
                        || current.kind != old.kind
                })
            {
                self.trading_finish(
                    cx,
                    Err("Kraken protective order changed before amendment dispatch".into()),
                );
                continue;
            }
            if job.amend_filled && matches!(step, Step::Place(_)) {
                self.trading_finish(cx,Err("Kraken protective order executed while cancellation was in flight; replacement suppressed".into()));
                continue;
            }
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
                            sign::encode_uri(&serde_json::to_string(&body).unwrap())
                        ),
                    )
                }
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
            if let Err(error) = self.signed_request(
                cx,
                method,
                path,
                form,
                Pending::Trade(Request {
                    step,
                    settings_revision: self.account_settings_revision(),
                }),
            ) {
                self.trading_finish(cx, Err(error));
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
        let reply = serde_json::from_str::<Reply>(response.body)
            .map_err(|_| "Invalid Kraken trading response".to_owned());
        let result = reply.and_then(|r| {
            if !(200..300).contains(&response.status) {
                return Err(format!("Kraken HTTP {}", response.status));
            }
            if r.result != "success"
                || matches!(r.status.as_deref(), Some("BAD_REQUEST" | "unauthorized"))
                || r.error.as_ref().is_some_and(|e| !e.is_empty())
                || !r.errors.is_empty()
            {
                return Err(r
                    .error
                    .or_else(|| r.errors.first().map(|e| e.message.clone()))
                    .unwrap_or_else(|| "Kraken rejected trading request".into()));
            }
            self.trading_apply(cx, request.step, request.settings_revision, r)
        });
        match result {
            Ok(Some(result)) => self.trading_finish(cx, Ok(result)),
            Ok(None) => {
                self.trading.waiting = false;
            }
            Err(error) => self.trading_finish(cx, Err(error)),
        }
        self.trading_pump(cx);
    }
    fn trading_apply(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        step: Step,
        settings_revision: u64,
        reply: Reply,
    ) -> Result<Option<String>, String> {
        match step {
            Step::Place(mut p) => {
                let status = reply.send.ok_or("Kraken placement missing sendStatus")?;
                if !matches!(status.status.as_str(), "placed" | "filled")
                    || status.order_id.is_empty()
                {
                    return Err(format!(
                        "Kraken placement rejected or incomplete: {}",
                        status.status
                    ));
                }
                if self
                    .trading
                    .active
                    .as_ref()
                    .is_some_and(|j| j.ids.contains(&status.order_id))
                {
                    return Err("Kraken placement returned duplicate order ID".into());
                }
                p.order.id = status.order_id.clone();
                p.order.last_update = cx.now;
                if status.status == "placed" {
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
            Step::Cancel(ids) => {
                let statuses = if ids.len() == 1 {
                    vec![
                        reply
                            .cancel
                            .ok_or("Kraken cancellation missing cancelStatus")?,
                    ]
                } else {
                    reply
                        .batch
                        .ok_or("Kraken batch cancellation missing batchStatus")?
                };
                if statuses.len() != ids.len() {
                    return Err("Kraken batch cancellation response count mismatch".into());
                }
                let mut seen = HashSet::new();
                let mut error = None;
                for status in statuses {
                    if status.status == "cancelled"
                        && ids.contains(&status.order_id)
                        && seen.insert(status.order_id.clone())
                    {
                        self.account_order_removed(cx, &status.order_id);
                    } else {
                        error = Some(format!(
                            "Kraken cancellation rejected or identity mismatch: {}",
                            status.status
                        ));
                    }
                }
                if let Some(error) = error {
                    return Err(error);
                }
                if let Some(job) = self.trading.active.as_mut()
                    && job.amend.as_ref().is_some_and(|old| ids.contains(&old.id))
                {
                    job.amend_canceled = true;
                }
            }
            Step::Edit {
                original,
                mut replacement,
                ..
            } => {
                let status = reply.edit.ok_or("Kraken amendment missing editStatus")?;
                if !matches!(status.status.as_str(), "edited" | "filled")
                    || status.order_id != original.id
                {
                    return Err(format!(
                        "Kraken amendment rejected or identity mismatch: {}",
                        status.status
                    ));
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
    fn trading_finish(&mut self, cx: &mut Cx<'_, Pending>, result: Result<String, String>) {
        self.trading.waiting = false;
        if let Some(job) = self.trading.active.take() {
            let result = result.map_err(|error| {
                if job.amend_canceled {
                    format!(
                        "Kraken canceled the original protective order; replacement failed: {error}"
                    )
                } else if !job.ids.is_empty() {
                    format!("{error}; already accepted order IDs: {}", job.ids.join(","))
                } else {
                    error
                }
            });
            cx.done(job.command, result);
        }
    }
    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        _request: Request,
        error: &str,
    ) {
        self.trading_finish(
            cx,
            Err(format!(
                "Kraken request failed; write outcome may be uncertain: {error}"
            )),
        );
        self.trading_pump(cx);
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
