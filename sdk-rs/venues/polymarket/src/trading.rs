//! CLOB V2 writes: preflight whole batches, serialize exact signed bodies, never retry writes.
//! Oracle: official clob-client-v2 v1.1.0 src/types/ordersV2.ts and client.ts.
use crate::account::OrderRow;
use crate::sign::{self, OrderSigning};
use crate::{Pending, Polymarket, encode, success};
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, CommandId, Cx, ErrorKind, Method, Outcome, Response, VenueError};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Clone)]
pub struct Prepared {
    options: PlaceOrderOpts,
    token: String,
    neg_risk: bool,
    maker_amount: String,
    taker_amount: String,
    price: f64,
    amount: f64,
}
pub struct Replacement {
    old: Order,
    options: PlaceOrderOpts,
    filled: f64,
    reads: u8,
}
pub enum CancelScope {
    Ids(Vec<String>),
    Token {
        symbol: String,
        expected: Vec<String>,
    },
    All {
        expected: Vec<String>,
    },
}
pub enum Request {
    Place {
        command: CommandId,
        placement: Box<Prepared>,
    },
    Cancel {
        command: CommandId,
        scope: CancelScope,
    },
    ReplaceCancel {
        command: CommandId,
        replacement: Box<Replacement>,
    },
    ReplaceRead {
        command: CommandId,
        replacement: Box<Replacement>,
    },
}
impl Request {
    fn command(&self) -> CommandId {
        match self {
            Self::Place { command, .. }
            | Self::Cancel { command, .. }
            | Self::ReplaceCancel { command, .. }
            | Self::ReplaceRead { command, .. } => *command,
        }
    }
}
struct Job {
    queue: VecDeque<Prepared>,
    ids: Vec<String>,
    locks: Vec<String>,
    symbols: HashSet<String>,
    scope: Option<String>,
    total: usize,
    /// A step took effect, so a later refusal cannot mean nothing happened.
    effective: bool,
}
#[derive(Default)]
pub struct Trading {
    jobs: HashMap<CommandId, Job>,
    locked: HashSet<String>,
    scopes: HashSet<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WireOrder<'a> {
    salt: u64,
    maker: &'a str,
    signer: &'a str,
    token_id: &'a str,
    maker_amount: &'a str,
    taker_amount: &'a str,
    side: &'static str,
    signature_type: u8,
    timestamp: &'a str,
    expiration: &'static str,
    metadata: String,
    builder: String,
    signature: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<'a> {
    defer_exec: bool,
    post_only: bool,
    order: WireOrder<'a>,
    owner: &'a str,
    order_type: &'static str,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlacementAck<'a> {
    success: bool,
    #[serde(rename = "orderID")]
    order_id: &'a str,
    #[serde(default)]
    error_msg: &'a str,
    #[serde(default)]
    status: &'a str,
}
#[derive(Deserialize)]
struct CancelAck {
    canceled: Vec<String>,
    not_canceled: HashMap<String, String>,
}

impl Polymarket {
    fn prepare(&self, cx: &Cx<'_, Pending>, options: PlaceOrderOpts) -> Result<Prepared, String> {
        // Caller identity rejection is first, including present-but-empty IDs.
        if options.client_order_id.is_some() {
            return Err("Polymarket does not support clientOrderId".into());
        }
        if options.kind != OrderType::Limit
            || options.trigger_price.is_some()
            || options.max_slippage_pct.is_some()
            || options.stop_loss.is_some()
            || options.take_profit.is_some()
            || options.trigger_type.is_some()
            || options.tp_trigger_type.is_some()
            || options.sl_trigger_type.is_some()
            || options.reduce_only == Some(true)
            || options.close == Some(true)
            || options.stp_mode.is_some()
            || options.bybit_bbo.is_some()
            || options.bybit_slippage_tolerance_ticks.is_some()
            || !matches!(
                options.time_in_force,
                None | Some(TimeInForce::GoodTillCancel)
            )
        {
            return Err("Polymarket supports only plain GTC limit orders".into());
        }
        let market = cx
            .store
            .market(&options.symbol)
            .ok_or("Unknown Polymarket symbol")?;
        alloy_primitives::U256::from_str_radix(&market.id, 10)
            .map_err(|_| "Invalid Polymarket uint256 token ID")?;
        if !market.active {
            return Err("Polymarket market is inactive".into());
        }
        let index = cx
            .store
            .market_index(&options.symbol)
            .ok_or("Unknown Polymarket symbol")?;
        let metadata = cx
            .store
            .prediction_markets()
            .and_then(|rows| rows.iter().find(|m| m.market == index))
            .ok_or("Missing Polymarket signing metadata")?;
        let price = options.price.ok_or("Polymarket limit price is required")?;
        if !options.amount.is_finite()
            || options.amount <= 0.0
            || options.amount > market.limits.amount.max
            || !price.is_finite()
            || !(0.0..=1.0).contains(&price)
        {
            return Err("Polymarket order amount is outside market limits".into());
        }
        let price = safe_cex_engine::num::adjust(price, metadata.tick_size);
        let size = safe_cex_engine::num::adjust(options.amount, 0.01);
        if size < market.limits.amount.min || size > market.limits.amount.max {
            return Err("Polymarket rounded amount is outside market limits".into());
        }
        let side = u8::from(options.side == OrderSide::Sell);
        let (maker_amount, taker_amount) =
            sign::limit_amounts(side, size, price, metadata.tick_size)?;
        let amount = if side == 0 {
            &taker_amount
        } else {
            &maker_amount
        }
        .parse::<f64>()
        .map_err(|_| "Invalid Polymarket amount")?
            / 1e6;
        if amount < market.limits.amount.min {
            return Err("Polymarket rounded amount is below market minimum".into());
        }
        Ok(Prepared {
            options,
            token: market.id.clone(),
            neg_risk: metadata.neg_risk,
            maker_amount,
            taker_amount,
            price,
            amount,
        })
    }
    fn trading_auth(&self) -> Result<(), String> {
        if self.options.public_data_only {
            return Err("Polymarket public-data-only mode cannot trade".into());
        }
        let key = self
            .options
            .key
            .as_deref()
            .ok_or("Polymarket signing key unavailable")?;
        safe_cex_crypto::ethereum::private_key_address(key)?;
        let credentials = self
            .credentials
            .as_ref()
            .ok_or("Polymarket credentials unavailable")?;
        sign::l2_signature(&credentials.secret, 0, "GET", "/", "")?;
        if credentials.api_key.is_empty() || credentials.passphrase.is_empty() {
            return Err("Incomplete Polymarket credentials".into());
        }
        Ok(())
    }
    fn placement_body(&self, cx: &Cx<'_, Pending>, placement: &Prepared) -> Result<String, String> {
        let key = self
            .options
            .key
            .as_deref()
            .ok_or("Polymarket signing key unavailable")?;
        let address = self
            .address
            .as_deref()
            .ok_or("Polymarket signer unavailable")?;
        let signer_type = self
            .options
            .polymarket
            .as_ref()
            .map_or(0, |p| p.signer_type);
        let maker = self
            .options
            .polymarket
            .as_ref()
            .and_then(|p| p.funder_address.as_deref())
            .unwrap_or(address);
        if signer_type > 0
            && self
                .options
                .polymarket
                .as_ref()
                .and_then(|p| p.funder_address.as_deref())
                .is_none()
        {
            return Err("Polymarket contract signatures require a funder".into());
        }
        let signer = sign::order_signer(key, maker, signer_type)?;
        if !cx.now.is_finite() || cx.now < 0.0 || cx.now > 9_007_199_254_740_991.0 {
            return Err("Invalid Polymarket order timestamp".into());
        }
        let mut salt = 0u64;
        for _ in 0..4 {
            let mut bytes = [0; 8];
            cx.fill_signing_entropy(&mut bytes)
                .map_err(|_| "Polymarket salt entropy unavailable")?;
            salt = u64::from_le_bytes(bytes) & 0x001f_ffff_ffff_ffff;
            if salt != 0 {
                break;
            }
        }
        if salt == 0 {
            return Err("Polymarket salt entropy returned zero".into());
        }
        let order = OrderSigning {
            salt: salt.to_string(),
            maker: maker.into(),
            signer,
            token_id: placement.token.clone(),
            maker_amount: placement.maker_amount.clone(),
            taker_amount: placement.taker_amount.clone(),
            side: u8::from(placement.options.side == OrderSide::Sell),
            signature_type: signer_type,
            timestamp: (cx.now.floor() as u64).to_string(),
            metadata: [0; 32],
            builder: [0; 32],
        };
        let signature = sign::order_signature(&order, key, placement.neg_risk)?;
        let owner = &self
            .credentials
            .as_ref()
            .ok_or("Polymarket credentials unavailable")?
            .api_key;
        serde_json::to_string(&Envelope {
            defer_exec: false,
            post_only: false,
            order: WireOrder {
                salt,
                maker: &order.maker,
                signer: &order.signer,
                token_id: &order.token_id,
                maker_amount: &order.maker_amount,
                taker_amount: &order.taker_amount,
                side: if order.side == 0 { "BUY" } else { "SELL" },
                signature_type: order.signature_type,
                timestamp: &order.timestamp,
                expiration: "0",
                metadata: format!("0x{}", safe_cex_crypto::hex(&order.metadata)),
                builder: format!("0x{}", safe_cex_crypto::hex(&order.builder)),
                signature,
            },
            owner,
            order_type: "GTC",
        })
        .map_err(|e| e.to_string())
    }
    fn trading_finish(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        result: Result<String, VenueError>,
    ) {
        if let Some(job) = self.trading.jobs.remove(&command) {
            for id in job.locks {
                self.trading.locked.remove(&id);
            }
            if let Some(scope) = job.scope {
                self.trading.scopes.remove(&scope);
            }
            match result {
                Ok(result) => cx.done(command, result),
                Err(mut error) => {
                    // A later refusal after a step took effect cannot mean nothing happened.
                    if (job.effective || !job.ids.is_empty())
                        && matches!(error.outcome, Some(Outcome::Rejected | Outcome::NotSent))
                    {
                        error.outcome = None;
                    }
                    cx.fail(command, error.partial(&job.ids, job.total, job.ids.len()));
                }
            }
        }
    }
    fn trading_next(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let next = self
            .trading
            .jobs
            .get_mut(&command)
            .and_then(|j| j.queue.pop_front());
        let Some(placement) = next else {
            let Some(job) = self.trading.jobs.get(&command) else {
                return;
            };
            let result = serde_json::to_string(&job.ids).map_err(|e| e.to_string().into());
            self.trading_finish(cx, command, result);
            return;
        };
        // Market tick/activation may change while an earlier batch element is in flight.
        let result = self.prepare(cx, placement.options).and_then(|placement| {
            let body = self.placement_body(cx, &placement)?;
            self.signed_request(
                cx,
                Method::Post,
                "/order",
                Some(body),
                Pending::Trade(Request::Place {
                    command,
                    placement: Box::new(placement),
                }),
            )
            .map(|_| ())
        });
        if let Err(error) = result {
            self.trading_finish(cx, command, Err(not_sent(error)));
        }
    }
    fn start_job(
        &mut self,
        command: CommandId,
        queue: VecDeque<Prepared>,
        locks: Vec<String>,
        symbols: HashSet<String>,
        scope: Option<String>,
    ) -> Result<(), String> {
        if self.trading.jobs.contains_key(&command) {
            return Err("Polymarket command is already pending".into());
        }
        if self.trading.scopes.contains("")
            || scope
                .as_ref()
                .is_some_and(|s| s.is_empty() && !self.trading.jobs.is_empty())
            || locks.iter().any(|id| self.trading.locked.contains(id))
            || symbols.iter().any(|s| self.trading.scopes.contains(s))
            || scope.as_ref().is_some_and(|s| {
                self.trading
                    .jobs
                    .values()
                    .any(|job| job.symbols.contains(s))
            })
        {
            return Err("Conflicting Polymarket mutation is already pending".into());
        }
        self.trading.locked.extend(locks.iter().cloned());
        if let Some(s) = &scope {
            self.trading.scopes.insert(s.clone());
        }
        self.trading.jobs.insert(
            command,
            Job {
                total: queue.len(),
                queue,
                ids: vec![],
                locks,
                symbols,
                scope,
                effective: false,
            },
        );
        Ok(())
    }
    pub(crate) fn trading_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let command_id = match &command {
            Command::PlaceOrders { command, .. }
            | Command::CancelOrders { command, .. }
            | Command::CancelSymbolOrders { command, .. }
            | Command::AmendOrder { command, .. } => *command,
            _ => return,
        };
        if let Command::PlaceOrders { orders, .. } = &command
            && orders.iter().any(|order| order.client_order_id.is_some())
        {
            cx.fail(command_id, "Polymarket does not support clientOrderId");
            return;
        }
        if self.trading.jobs.contains_key(&command_id) {
            cx.fail(command_id, "Polymarket command is already pending");
            return;
        }
        if matches!(&command, Command::PlaceOrders { orders, .. } if orders.is_empty()) {
            cx.done(command_id, "[]".into());
            return;
        }
        if matches!(&command, Command::CancelOrders { ids, .. } if ids.is_empty()) {
            cx.done(command_id, "[]".into());
            return;
        }
        let result = (|| -> Result<(), String> {
            self.trading_auth()?;
            match command {
                Command::PlaceOrders { command, orders } => {
                    let plans = orders
                        .into_iter()
                        .map(|o| self.prepare(cx, o))
                        .collect::<Result<VecDeque<_>, _>>()?;
                    let symbols = plans.iter().map(|p| p.options.symbol.clone()).collect();
                    self.start_job(command, plans, vec![], symbols, None)?;
                    self.trading_next(cx, command);
                }
                Command::CancelOrders { command, ids } => {
                    if ids.iter().any(|id| id.is_empty()) {
                        return Err("Polymarket cancellation ID is empty".into());
                    }
                    let mut seen = HashSet::new();
                    let ids = ids
                        .into_iter()
                        .filter(|id| seen.insert(id.clone()))
                        .collect::<Vec<_>>();
                    let symbols = ids
                        .iter()
                        .filter_map(|id| cx.store.order(id).map(|o| o.symbol.clone()))
                        .collect();
                    self.start_job(command, VecDeque::new(), ids.clone(), symbols, None)?;
                    if ids.is_empty() {
                        self.trading_finish(cx, command, Ok("[]".into()));
                        return Ok(());
                    }
                    let body = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
                    self.signed_request(
                        cx,
                        Method::Delete,
                        "/orders",
                        Some(body),
                        Pending::Trade(Request::Cancel {
                            command,
                            scope: CancelScope::Ids(ids),
                        }),
                    )?;
                }
                Command::CancelSymbolOrders { command, symbol } => {
                    if symbol.is_empty() {
                        let expected = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|order| order.status == OrderStatus::Open)
                            .map(|order| order.id.clone())
                            .collect::<Vec<_>>();
                        self.start_job(
                            command,
                            VecDeque::new(),
                            expected.clone(),
                            HashSet::new(),
                            Some(String::new()),
                        )?;
                        self.signed_request(
                            cx,
                            Method::Delete,
                            "/cancel-all",
                            None,
                            Pending::Trade(Request::Cancel {
                                command,
                                scope: CancelScope::All { expected },
                            }),
                        )?;
                        return Ok(());
                    }
                    let token = cx
                        .store
                        .market(&symbol)
                        .ok_or("Unknown Polymarket symbol")?
                        .id
                        .clone();
                    let expected = cx
                        .store
                        .orders()
                        .iter()
                        .filter(|o| o.symbol == symbol)
                        .map(|o| o.id.clone())
                        .collect::<Vec<_>>();
                    self.start_job(
                        command,
                        VecDeque::new(),
                        expected.clone(),
                        HashSet::from([symbol.clone()]),
                        Some(symbol.clone()),
                    )?;
                    #[derive(Serialize)]
                    struct Scope<'a> {
                        asset_id: &'a str,
                    }
                    let body = serde_json::to_string(&Scope { asset_id: &token })
                        .map_err(|e| e.to_string())?;
                    self.signed_request(
                        cx,
                        Method::Delete,
                        "/cancel-market-orders",
                        Some(body),
                        Pending::Trade(Request::Cancel {
                            command,
                            scope: CancelScope::Token { symbol, expected },
                        }),
                    )?;
                }
                Command::AmendOrder {
                    command,
                    id,
                    price,
                    amount,
                } => {
                    let old = cx
                        .store
                        .order(&id)
                        .filter(|o| o.status == OrderStatus::Open)
                        .ok_or("Polymarket amend requires a known open order")?
                        .clone();
                    if price.is_none() && amount.is_none() {
                        return Err("Polymarket amend has no changes".into());
                    }
                    let mut options = PlaceOrderOpts {
                        symbol: old.symbol.clone(),
                        kind: OrderType::Limit,
                        side: old.side,
                        amount: amount.unwrap_or(old.amount),
                        price: Some(price.unwrap_or(old.price)),
                        trigger_price: None,
                        max_slippage_pct: None,
                        stop_loss: None,
                        take_profit: None,
                        reduce_only: None,
                        time_in_force: Some(TimeInForce::GoodTillCancel),
                        trigger_type: None,
                        tp_trigger_type: None,
                        sl_trigger_type: None,
                        client_order_id: None,
                        stp_mode: None,
                        close: None,
                        bybit_bbo: None,
                        bybit_slippage_tolerance_ticks: None,
                        implied_volatility: None,
                    };
                    let prepared = self.prepare(cx, options.clone())?;
                    options.amount = prepared.amount;
                    options.price = Some(prepared.price);
                    self.start_job(
                        command,
                        VecDeque::new(),
                        vec![id.clone()],
                        HashSet::from([old.symbol.clone()]),
                        None,
                    )?;
                    #[derive(Serialize)]
                    struct Cancel<'a> {
                        #[serde(rename = "orderID")]
                        id: &'a str,
                    }
                    let body =
                        serde_json::to_string(&Cancel { id: &id }).map_err(|e| e.to_string())?;
                    let filled = old.filled.max(self.account_filled(&id));
                    self.signed_request(
                        cx,
                        Method::Delete,
                        "/order",
                        Some(body),
                        Pending::Trade(Request::ReplaceCancel {
                            command,
                            replacement: Box::new(Replacement {
                                old,
                                options,
                                filled,
                                reads: 0,
                            }),
                        }),
                    )?;
                }
                _ => unreachable!(),
            }
            Ok(())
        })();
        if let Err(error) = result {
            if self.trading.jobs.contains_key(&command_id) {
                self.trading_finish(cx, command_id, Err(not_sent(error)));
            } else {
                cx.fail(command_id, not_sent(error));
            }
        }
    }
    fn replace_read(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        replacement: Box<Replacement>,
    ) -> Result<(), String> {
        let path = format!("/data/order/{}", encode(&replacement.old.id));
        self.signed_request(
            cx,
            Method::Get,
            &path,
            None,
            Pending::Trade(Request::ReplaceRead {
                command,
                replacement,
            }),
        )
        .map(|_| ())
    }
    pub(crate) fn trading_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let command = request.command();
        if !self.trading.jobs.contains_key(&command) {
            return;
        }
        // Set before a step whose failure happens before any request is sent.
        let mut before_send = false;
        let mut refused = false;
        let result = (|| {
            success(response)?;
            match request {
                Request::Place { placement, .. } => {
                    let ack: PlacementAck = serde_json::from_str(response.body).map_err(|e| {
                        format!("Invalid Polymarket placement acknowledgement: {e}")
                    })?;
                    if !ack.success
                        || ack.order_id.is_empty()
                        || !ack.error_msg.is_empty()
                        || !["", "live", "matched", "delayed", "unmatched"].contains(&ack.status)
                    {
                        refused = true;
                        return Err(format!("Polymarket placement rejected: {}", ack.error_msg));
                    }
                    if ack.status == "matched" {
                        // "matched" also acknowledges a partially crossing GTC order.
                        // Its remaining quantity requires authoritative order details;
                        // it is never proof that the order is fully filled/terminal.
                        self.account_refresh(cx);
                    } else if cx.store.order(ack.order_id).is_none() {
                        self.account_placed(
                            cx,
                            Order {
                                id: ack.order_id.into(),
                                parent_id: None,
                                status: OrderStatus::Open,
                                symbol: placement.options.symbol,
                                kind: OrderType::Limit,
                                side: placement.options.side,
                                price: placement.price,
                                amount: placement.amount,
                                filled: 0.0,
                                remaining: placement.amount,
                                reduce_only: false,
                                close: None,
                                client_order_id: None,
                                time_in_force: Some(TimeInForce::GoodTillCancel),
                                last_update: cx.now,
                                trigger_type: None,
                                take_profit: None,
                                stop_loss: None,
                                extra: None,
                            },
                        );
                    }
                    self.trading
                        .jobs
                        .get_mut(&command)
                        .expect("active job")
                        .ids
                        .push(ack.order_id.into());
                    self.trading_next(cx, command);
                }
                Request::Cancel { scope, .. } => {
                    let ack: CancelAck = serde_json::from_str(response.body).map_err(|e| {
                        format!("Invalid Polymarket cancellation acknowledgement: {e}")
                    })?;
                    for id in &ack.canceled {
                        let outside = match &scope {
                            CancelScope::Ids(ids) => !ids.contains(id),
                            CancelScope::Token { symbol, .. } => cx
                                .store
                                .order(id)
                                .is_some_and(|order| &order.symbol != symbol),
                            CancelScope::All { .. } => false,
                        };
                        if id.is_empty() || outside {
                            return Err(
                                "Polymarket cancellation response has out-of-scope IDs".into()
                            );
                        }
                    }
                    for id in &ack.canceled {
                        self.account_terminal(cx, id);
                    }
                    let expected = match &scope {
                        CancelScope::Ids(ids)
                        | CancelScope::All { expected: ids }
                        | CancelScope::Token { expected: ids, .. } => ids.as_slice(),
                    };
                    if !ack.not_canceled.is_empty()
                        || expected.iter().any(|id| !ack.canceled.contains(id))
                    {
                        return Err(format!(
                            "Polymarket cancellation incomplete: {:?}",
                            ack.not_canceled
                        ));
                    }
                    self.trading_finish(cx, command, Ok("[]".into()));
                }
                Request::ReplaceCancel {
                    mut replacement, ..
                } => {
                    let ack: CancelAck = serde_json::from_str(response.body)
                        .map_err(|e| format!("Invalid Polymarket replacement cancellation: {e}"))?;
                    if ack.canceled != [replacement.old.id.clone()] || !ack.not_canceled.is_empty()
                    {
                        return Err("Polymarket replacement cancellation was not confirmed".into());
                    }
                    replacement.filled = replacement
                        .filled
                        .max(self.account_filled(&replacement.old.id))
                        .max(
                            cx.store
                                .order(&replacement.old.id)
                                .map_or(0.0, |o| o.filled),
                        );
                    self.account_terminal(cx, &replacement.old.id);
                    self.trading
                        .jobs
                        .get_mut(&command)
                        .expect("active job")
                        .effective = true;
                    before_send = true;
                    self.replace_read(cx, command, replacement)?;
                }
                Request::ReplaceRead {
                    mut replacement, ..
                } => {
                    let row: OrderRow = serde_json::from_str(response.body)
                        .map_err(|e| format!("Invalid Polymarket final order details: {e}"))?;
                    if row.id != replacement.old.id {
                        return Err("Polymarket final order ID mismatch".into());
                    }
                    if !row.has_fill_size() {
                        return Err("Incomplete Polymarket final fill size".into());
                    }
                    let order = row
                        .order(cx.store, cx.now)
                        .ok_or("Incomplete Polymarket final order details")?;
                    if order.symbol != replacement.old.symbol || order.side != replacement.old.side
                    {
                        return Err("Polymarket final order identity mismatch".into());
                    }
                    let known_filled = replacement
                        .filled
                        .max(self.account_filled(&replacement.old.id));
                    if !matches!(order.status, OrderStatus::Canceled | OrderStatus::Closed)
                        || order.filled < known_filled
                        || !self.account_fill_proof(&replacement.old.id, &row)
                    {
                        replacement.reads += 1;
                        if replacement.reads >= 3 {
                            return Err(
                                "Polymarket final cancellation/fill proof is not yet coherent"
                                    .into(),
                            );
                        }
                        before_send = true;
                        self.replace_read(cx, command, replacement)?;
                        return Ok(());
                    }
                    replacement.filled = replacement
                        .filled
                        .max(order.filled)
                        .max(self.account_filled(&replacement.old.id));
                    // Partial executions may have six decimal places. Replacement
                    // shares must round DOWN to 2dp, never exceed the unfilled target.
                    // Compare/subtract integer token base units before the cent floor.
                    let target_units = (replacement.options.amount * 1e6).round() as u64;
                    let filled_units = (replacement.filled * 1e6).round() as u64;
                    let remaining =
                        (target_units.saturating_sub(filled_units) / 10_000) as f64 / 100.0;
                    if remaining == 0.0 {
                        self.trading_finish(cx, command, Ok("[]".into()));
                        return Ok(());
                    }
                    replacement.options.amount = remaining;
                    before_send = true;
                    let placement = self.prepare(cx, replacement.options)?;
                    self.trading
                        .jobs
                        .get_mut(&command)
                        .expect("active job")
                        .queue
                        .push_back(placement);
                    self.trading_next(cx, command);
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                if let Some(job) = self.trading.jobs.get_mut(&command) {
                    job.effective = true;
                }
            }
            Err(error) => {
                let error = if !(200..300).contains(&response.status) {
                    http_failure(response.status, error)
                } else if before_send {
                    not_sent(error)
                } else if refused {
                    // An explicit refusal; its messages are not a documented code table.
                    VenueError::new(error)
                } else {
                    // A 2xx answer that does not prove what happened.
                    VenueError::new(error).outcome(Outcome::Unknown)
                };
                self.trading_finish(cx, command, Err(error));
            }
        }
    }
    pub(crate) fn trading_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        let error = VenueError::new(error)
            .kind(ErrorKind::Network)
            .outcome(Outcome::Unknown);
        self.trading_finish(cx, request.command(), Err(error));
    }
}

/// A rejected HTTP write: 408 and 5xx do not prove the venue refused it.
fn http_failure(status: u16, message: String) -> VenueError {
    let error = VenueError::new(message);
    match status {
        408 | 500.. => error.outcome(Outcome::Unknown),
        401 => error
            .kind(ErrorKind::AuthInvalid)
            .outcome(Outcome::Rejected),
        403 => error
            .kind(ErrorKind::AuthPermission)
            .outcome(Outcome::Rejected),
        429 => error
            .kind(ErrorKind::RateLimited)
            .outcome(Outcome::Rejected),
        _ => error.kind(ErrorKind::Rejected).outcome(Outcome::Rejected),
    }
}

/// Refused before any request was sent.
fn not_sent(message: impl Into<String>) -> VenueError {
    VenueError::new(message).outcome(Outcome::NotSent)
}
