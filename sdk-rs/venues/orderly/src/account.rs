use crate::{Connection, Orderly, Pending, Seed, sign, symbol, wire::*};
use safe_cex_engine::json::{self, ArrayAt};
use safe_cex_engine::model::*;
use safe_cex_engine::{Cx, Fill, LogLevel, Method, Response, SocketId, TimerId};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

pub enum Request {
    Info,
    Modes {
        rows: Vec<(String, MarginType)>,
        invalid: bool,
        revision: u64,
    },
    Positions {
        rows: Vec<Position>,
        invalid: bool,
        revision: u64,
        stamps: HashMap<String, (Option<u64>, u64)>,
    },
    Orders {
        algo: bool,
        rows: Vec<Order>,
        proofs: HashMap<String, Proof>,
        invalid: bool,
        revision: u64,
        page: u32,
        seen: HashSet<String>,
        page_count: u32,
        stamps: HashMap<String, (Option<u64>, u64)>,
    },
}
#[derive(Clone)]
pub struct Proof {
    pub mode: MarginType,
    pub amend: bool,
}

pub struct Account {
    pub credentials: Option<sign::Credentials>,
    pub socket: Connection,
    pub defaults: HashMap<String, MarginType>,
    pub leverage: HashMap<(String, bool), f64>,
    pub fallback_leverage: f64,
    pub algo_snapshots: u64,
    pub positions_revision: u64,
    pub order_revisions: [u64; 2],
    pub settings_revision: u64,
    pub proofs: HashMap<String, Proof>,
    authenticated: bool,
    inflight: [bool; 4],
    loaded: [bool; 2],
    tick: Option<TimerId>,
    startup_retry: Option<TimerId>,
    ticks: u32,
    order_stamps: HashMap<String, (Option<u64>, u64)>,
    position_stamps: HashMap<String, (Option<u64>, u64)>,
    executions: HashSet<String>,
    execution_order: VecDeque<String>,
    totals: HashMap<String, (f64, f64)>,
    terminal: HashSet<String>,
    terminal_order: VecDeque<String>,
}
impl Default for Account {
    fn default() -> Self {
        Self {
            credentials: None,
            socket: Connection::default(),
            defaults: HashMap::new(),
            leverage: HashMap::new(),
            fallback_leverage: 1.0,
            algo_snapshots: 0,
            positions_revision: 0,
            order_revisions: [0; 2],
            settings_revision: 0,
            proofs: HashMap::new(),
            authenticated: false,
            inflight: [false; 4],
            loaded: [false; 2],
            tick: None,
            startup_retry: None,
            ticks: 0,
            order_stamps: HashMap::new(),
            position_stamps: HashMap::new(),
            executions: HashSet::new(),
            execution_order: VecDeque::new(),
            totals: HashMap::new(),
            terminal: HashSet::new(),
            terminal_order: VecDeque::new(),
        }
    }
}
pub fn margin(raw: &str) -> Result<MarginType, String> {
    match raw {
        "ISOLATED" => Ok(MarginType::Isolated),
        "CROSS" | "" => Ok(MarginType::Cross),
        _ => Err("Orderly invalid margin mode".into()),
    }
}

pub fn side(raw: &str) -> Result<OrderSide, String> {
    match raw {
        "BUY" => Ok(OrderSide::Buy),
        "SELL" => Ok(OrderSide::Sell),
        _ => Err("Orderly invalid order side".into()),
    }
}
pub fn status(raw: &str) -> Result<OrderStatus, String> {
    match raw {
        "NEW" | "REPLACED" | "PARTIAL_FILLED" | "PARTIALLY_FILLED" | "PENDING_NEW" => {
            Ok(OrderStatus::Open)
        }
        "FILLED" => Ok(OrderStatus::Closed),
        "CANCELLED" | "CANCELED" | "REJECTED" | "EXPIRED" => Ok(OrderStatus::Canceled),
        _ => Err(format!("Orderly unknown order status {raw}")),
    }
}
fn stamp_newer(prior: Option<&(Option<u64>, u64)>, seq: Option<u64>, ts: u64) -> bool {
    prior.is_none_or(|(old_seq, old_ts)| match (*old_seq, seq) {
        (Some(old), Some(new)) => new > old,
        _ => ts >= *old_ts,
    })
}
fn snapshot_older(prior: Option<&(Option<u64>, u64)>, new: (Option<u64>, u64)) -> bool {
    prior.is_some_and(|old| match (old.0, new.0) {
        (Some(a), Some(b)) => b < a,
        _ => new.1 > 0 && new.1 < old.1,
    })
}
fn merge_stamp(
    stamps: &mut HashMap<String, (Option<u64>, u64)>,
    id: String,
    new: (Option<u64>, u64),
) {
    let previous = stamps.get(&id).copied();
    stamps.insert(
        id,
        (
            new.0.or(previous.and_then(|p| p.0)),
            new.1.max(previous.map_or(0, |p| p.1)),
        ),
    );
}

impl Orderly {
    pub(crate) fn request_signed(
        &self,
        cx: &mut Cx<'_, Pending>,
        method: Method,
        path: &str,
        body: Option<String>,
        pending: Pending,
        stream: bool,
    ) -> Result<(), String> {
        let credentials = self
            .account
            .credentials
            .as_ref()
            .ok_or("Orderly credentials unavailable")?;
        let headers = credentials.headers(method, path, body.as_deref(), cx.now)?;
        let url = format!("{}{path}", self.rest());
        if stream {
            cx.request_streamed_body(
                method,
                url,
                headers,
                body,
                ArrayAt::Path(&["data", "rows"]),
                pending,
            );
        } else {
            cx.request(method, url, headers, body, pending);
        }
        Ok(())
    }
    pub(crate) fn initialize_account(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.options.public_data_only || self.options.key.as_ref().is_none_or(String::is_empty) {
            self.seed(cx, Seed::Markets);
            return;
        }
        let credentials = sign::Credentials::new(
            self.options.key.as_deref().unwrap_or(""),
            self.options.secret.as_deref().unwrap_or(""),
            self.options.application_id.as_deref().unwrap_or(""),
        );
        match credentials {
            Ok(c) => {
                self.account.credentials = Some(c);
                self.account_read(cx, Request::Info);
            }
            Err(e) => cx.log(LogLevel::Error, e),
        }
    }
    pub(crate) fn account_start(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.credentials.is_none() {
            cx.store.set_orders(vec![]);
            cx.store.set_positions(vec![]);
            cx.store.set_balance(Default::default());
            return;
        }
        cx.store.retain_position_ids();
        cx.store.set_hedged(false);
        self.account_read(
            cx,
            Request::Modes {
                rows: vec![],
                invalid: false,
                revision: self.account.settings_revision,
            },
        );
    }
    fn private_url(&self) -> String {
        let host = if self.options.testnet {
            "testnet-ws-private-evm.orderly.org"
        } else {
            "ws-private-evm.orderly.org"
        };
        format!(
            "wss://{host}/v2/ws/private/stream/{}",
            self.account.credentials.as_ref().unwrap().account
        )
    }
    fn account_ready(&mut self, cx: &mut Cx<'_, Pending>) {
        if self.account.socket.socket.is_none() {
            self.private_connect(cx);
        }
        let symbols: Vec<_> = cx
            .store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect();
        for s in symbols {
            self.sync_setting(cx, &s);
        }
        self.account_poll(cx, true);
        if self.account.tick.is_none() {
            self.account.tick = Some(cx.set_timer(self.poll_interval()));
        }
    }
    fn poll_interval(&self) -> f64 {
        self.options
            .tick_interval
            .filter(|v| v.is_finite())
            .unwrap_or(2000.0)
            .clamp(1000.0, 3600000.0)
    }
    pub(crate) fn sync_setting(&self, cx: &mut Cx<'_, Pending>, symbol: &str) {
        let mode = self.selected_mode(cx, symbol);
        let leverage = self
            .account
            .leverage
            .get(&(symbol.into(), mode == MarginType::Isolated))
            .copied()
            .unwrap_or(self.account.fallback_leverage);
        cx.store.update_setting(symbol, |s| {
            s.margin_type = Some(mode);
            s.leverage = leverage;
        });
    }
    pub(crate) fn selected_mode(&self, cx: &Cx<'_, Pending>, symbol: &str) -> MarginType {
        if let Some(m) = self.account.defaults.get(symbol) {
            return *m;
        }
        let mut rows = cx
            .store
            .positions()
            .iter()
            .filter(|p| p.symbol == symbol && p.contracts > 0.0);
        let first = rows
            .next()
            .and_then(|p| p.margin_type)
            .unwrap_or(MarginType::Cross);
        if rows.all(|p| p.margin_type == Some(first)) {
            first
        } else {
            MarginType::Cross
        }
    }
    pub(crate) fn account_poll(&mut self, cx: &mut Cx<'_, Pending>, orders: bool) {
        if !self.account.inflight[1] {
            self.account.inflight[1] = true;
            self.account_read(
                cx,
                Request::Positions {
                    rows: vec![],
                    invalid: false,
                    revision: self.account.positions_revision,
                    stamps: HashMap::new(),
                },
            );
        }
        if orders {
            for algo in [false, true] {
                let i = algo as usize;
                if !self.account.inflight[2 + i] {
                    self.account.inflight[2 + i] = true;
                    self.account_read(
                        cx,
                        Request::Orders {
                            algo,
                            rows: vec![],
                            proofs: HashMap::new(),
                            invalid: false,
                            revision: self.account.order_revisions[i],
                            page: 1,
                            seen: HashSet::new(),
                            page_count: 0,
                            stamps: HashMap::new(),
                        },
                    );
                }
            }
        }
    }
    fn account_read(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let path = match &request {
            Request::Info => "/v1/client/info".into(),
            Request::Modes { .. } => "/v1/client/margin_modes".into(),
            Request::Positions { .. } => "/v1/positions".into(),
            Request::Orders { algo, page, .. } => format!(
                "/v1/{}orders?status=INCOMPLETE&page={page}&size=500",
                if *algo { "algo/" } else { "" }
            ),
        };
        let stream = !matches!(request, Request::Info);
        if let Err(e) = self.request_signed(
            cx,
            Method::Get,
            &path,
            None,
            Pending::Account(request),
            stream,
        ) {
            cx.log(LogLevel::Error, e);
        }
    }
    pub(crate) fn position(
        &self,
        cx: &Cx<'_, Pending>,
        r: &PositionRow<'_>,
    ) -> Result<Option<Position>, String> {
        let Some(symbol) = symbol(&r.symbol).filter(|s| cx.store.market(s).is_some()) else {
            return Ok(None);
        };
        let mode = margin(&r.margin_mode)?;
        let mark = if r.mark_price > 0.0 {
            r.mark_price
        } else {
            cx.store.ticker(&symbol).map_or(0.0, |t| t.mark)
        };
        let leverage = if r.leverage > 0.0 {
            r.leverage
        } else if r.imr > 0.0 {
            1.0 / r.imr
        } else {
            self.account
                .leverage
                .get(&(symbol.clone(), mode == MarginType::Isolated))
                .copied()
                .unwrap_or(self.account.fallback_leverage)
        };
        let size = r.position_qty.abs();
        let notional = size * mark;
        Ok(Some(Position {
            id: format!(
                "{symbol}{}",
                if mode == MarginType::Isolated {
                    ":isolated"
                } else {
                    ""
                }
            ),
            symbol,
            side: if r.position_qty >= 0.0 {
                PositionSide::Long
            } else {
                PositionSide::Short
            },
            entry_price: r.average_open_price,
            notional,
            leverage,
            margin_type: Some(mode),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: safe_cex_engine::num::round_half_up(
                (mark - r.average_open_price) * r.position_qty * 100.0,
            ) / 100.0,
            realized_pnl: None,
            contracts: size,
            liquidation_price: r.est_liq_price,
            margin_size: Some(if r.margin > 0.0 {
                r.margin
            } else if leverage > 0.0 {
                notional / leverage
            } else {
                0.0
            }),
            last_update: Some(cx.now),
        }))
    }
    pub(crate) fn orders(
        &self,
        cx: &Cx<'_, Pending>,
        r: &OrderRow<'_>,
        algo: bool,
    ) -> Result<Vec<Order>, String> {
        let Some(symbol) = symbol(&r.symbol).filter(|s| cx.store.market(s).is_some()) else {
            return Ok(vec![]);
        };
        let mut out = vec![];
        let children = if r.child_orders.is_empty() {
            std::slice::from_ref(r)
        } else {
            r.child_orders.as_slice()
        };
        for row in children {
            let id = if algo {
                &row.algo_order_id.0
            } else {
                &row.order_id.0
            };
            if algo && row.trigger_price.is_none() && row.algo_type != "TRAILING_STOP" {
                continue;
            }
            if id.is_empty() || id == "0" {
                return Err("Orderly missing order ID".into());
            }
            let (kind, tif) = match if algo {
                row.algo_type.as_ref()
            } else {
                row.kind.as_ref()
            } {
                "LIMIT" => (OrderType::Limit, Some(TimeInForce::GoodTillCancel)),
                "MARKET" => (OrderType::Market, None),
                "POST_ONLY" => (OrderType::Limit, Some(TimeInForce::PostOnly)),
                "IOC" => (OrderType::Limit, Some(TimeInForce::ImmediateOrCancel)),
                "FOK" => (OrderType::Limit, Some(TimeInForce::FillOrKill)),
                "STOP_LOSS" => (OrderType::StopLoss, None),
                "TAKE_PROFIT" => (OrderType::TakeProfit, None),
                "TRAILING_STOP" => (OrderType::TrailingStopLoss, None),
                _ => return Err("Orderly unsupported order type".into()),
            };
            if algo
                && !matches!(
                    row.trigger_price_type.as_ref(),
                    "" | "MARK_PRICE" | "USELESS"
                )
            {
                return Err("Orderly unsupported algo trigger source".into());
            }
            let filled = row
                .total_executed_quantity
                .or(row.executed)
                .map_or(0.0, |n| n.0);
            if row.quantity < 0.0 || filled < 0.0 {
                return Err("Orderly invalid quantity".into());
            }
            let parent = if !r.root_algo_order_id.0.is_empty() {
                &r.root_algo_order_id.0
            } else if !row.root_algo_order_id.0.is_empty() {
                &row.root_algo_order_id.0
            } else {
                &r.algo_order_id.0
            };
            let raw_status = if !r.root_algo_status.is_empty()
                && matches!(
                    r.root_algo_status.as_ref(),
                    "CANCELLED" | "FILLED" | "REJECTED"
                ) {
                r.root_algo_status.as_ref()
            } else if algo && !row.algo_status.is_empty() {
                row.algo_status.as_ref()
            } else if !row.status.is_empty() {
                row.status.as_ref()
            } else if algo && !r.algo_status.is_empty() {
                r.algo_status.as_ref()
            } else {
                r.status.as_ref()
            };
            let tif = match row.time_in_force.as_ref() {
                "IOC" => Some(TimeInForce::ImmediateOrCancel),
                "FOK" => Some(TimeInForce::FillOrKill),
                "POST_ONLY" => Some(TimeInForce::PostOnly),
                _ => tif,
            };
            out.push(Order {
                id: id.clone(),
                parent_id: algo.then(|| parent.clone()),
                symbol: symbol.clone(),
                status: status(raw_status)?,
                kind,
                side: side(if row.side.is_empty() {
                    &r.side
                } else {
                    &row.side
                })?,
                price: row.trigger_price.map_or(row.price, |n| n.0),
                amount: row.quantity,
                filled,
                remaining: (row.quantity - filled).max(0.0),
                reduce_only: row.reduce_only.or(r.reduce_only).unwrap_or(algo),
                close: None,
                client_order_id: row.client_order_id.as_ref().map(|s| s.to_string()),
                time_in_force: tif,
                last_update: cx.now,
                trigger_type: algo.then_some(StopTriggerType::MarkPrice),
                take_profit: None,
                stop_loss: None,
                extra: None,
            });
        }
        Ok(out)
    }
    fn proof(&self, cx: &Cx<'_, Pending>, item: &str) -> Option<(String, Proof)> {
        let r: RestProof<'_> = serde_json::from_str(item).ok()?;
        let symbol = symbol(&r.symbol)?;
        let market = cx.store.market(&symbol)?;
        let valid = r.kind == "LIMIT"
            && r.status == "NEW"
            && matches!(r.side.as_ref(), "BUY" | "SELL")
            && r.order_id.0 != "0"
            && r.price > 0.0
            && r.quantity > 0.0
            && r.visible_quantity == r.quantity
            && r.total_executed_quantity == 0.0
            && r.executed.is_none_or(|n| n == 0.0)
            && r.executed_quantity.is_none_or(|n| n == 0.0)
            && matches!(r.margin_mode.as_ref(), "CROSS" | "ISOLATED")
            && matches!(r.time_in_force.as_ref(), "" | "GTC")
            && r.trigger_price.is_none()
            && r.algo_order_id.is_none()
            && r.root_algo_order_id.is_none()
            && r.algo_type.is_empty()
            && safe_cex_engine::num::adjust(r.price, market.precision.price) == r.price
            && safe_cex_engine::num::adjust(r.quantity, market.precision.amount) == r.quantity;
        let _reduce_only = r.reduce_only;
        Some((
            r.order_id.0,
            Proof {
                mode: margin(&r.margin_mode).ok()?,
                amend: valid,
            },
        ))
    }
    pub(crate) fn account_item(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: &mut Request,
        item: &str,
    ) {
        let result: Result<(), String> = (|| {
            match request {
                Request::Info => {}
                Request::Modes { rows, .. } => {
                    let r: ModeRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    if let Some(s) = symbol(&r.symbol) {
                        rows.push((s, margin(&r.default_margin_mode)?));
                    }
                }
                Request::Positions { rows, stamps, .. } => {
                    let r: PositionRow<'_> =
                        serde_json::from_str(item).map_err(|e| e.to_string())?;
                    if let Some(p) = self.position(cx, &r)? {
                        stamps.insert(p.id.clone(), (r.seq, r.timestamp.unwrap_or(0)));
                        rows.push(p);
                    }
                }
                Request::Orders {
                    algo,
                    rows,
                    proofs,
                    seen,
                    page_count,
                    stamps,
                    ..
                } => {
                    *page_count += 1;
                    let r: OrderRow<'_> = serde_json::from_str(item).map_err(|e| e.to_string())?;
                    let key = if *algo {
                        &r.algo_order_id.0
                    } else {
                        &r.order_id.0
                    };
                    if !key.is_empty() && !seen.insert(key.clone()) {
                        return Err("Orderly repeated order page".into());
                    }
                    if let Some((id, proof)) = self.proof(cx, item).filter(|_| !*algo) {
                        proofs.insert(id, proof);
                    }
                    let mapped = self.orders(cx, &r, *algo)?;
                    if *algo {
                        for o in &mapped {
                            proofs.insert(
                                o.id.clone(),
                                Proof {
                                    mode: margin(
                                        r.child_orders
                                            .iter()
                                            .find(|c| c.algo_order_id.0 == o.id)
                                            .filter(|c| !c.margin_mode.is_empty())
                                            .map(|c| c.margin_mode.as_ref())
                                            .unwrap_or(&r.margin_mode),
                                    )?,
                                    amend: false,
                                },
                            );
                        }
                    }
                    for o in &mapped {
                        let source = r
                            .child_orders
                            .iter()
                            .find(|c| c.algo_order_id.0 == o.id)
                            .unwrap_or(&r);
                        stamps.insert(
                            o.id.clone(),
                            (source.seq, source.timestamp.or(r.timestamp).unwrap_or(0)),
                        );
                    }
                    rows.extend(mapped);
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            match request {
                Request::Modes { invalid, .. }
                | Request::Positions { invalid, .. }
                | Request::Orders { invalid, .. } => *invalid = true,
                Request::Info => {}
            };
            cx.log(LogLevel::Error, e);
        }
    }
    pub(crate) fn account_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let parsed = checked(response);
        if let Err(e) = parsed {
            if matches!(request, Request::Info)
                && (400..500).contains(&response.status)
                && !matches!(response.status, 408 | 429)
            {
                cx.log(LogLevel::Error, "Orderly API key expired or rejected");
                cx.deactivate();
                return;
            }
            self.account_failed(cx, request, &e);
            return;
        }
        let rest = parsed.unwrap();
        let Some(data) = rest.data.map(|d| d.get()) else {
            self.account_failed(cx, request, "Orderly missing account response data");
            return;
        };
        match request {
            Request::Info => {
                let Ok(info) = serde_json::from_str::<Info>(data) else {
                    self.account_failed(cx, Request::Info, "Orderly invalid account info");
                    return;
                };
                self.account.fallback_leverage = if info.max_leverage > 0.0 {
                    info.max_leverage
                } else {
                    20.0
                };
                self.seed(cx, Seed::Markets);
            }
            Request::Modes {
                rows,
                invalid,
                revision,
            } => {
                if !invalid && revision == self.account.settings_revision {
                    self.account.defaults = rows.into_iter().collect();
                }
                self.account_ready(cx);
            }
            Request::Positions {
                rows,
                invalid,
                revision,
                stamps,
            } => {
                self.account.inflight[1] = false;
                if invalid || revision != self.account.positions_revision {
                    return;
                }
                let Ok(meta) = serde_json::from_str::<PositionMeta>(data) else {
                    cx.log(LogLevel::Error, "Orderly invalid collateral response");
                    return;
                };
                let upnl = safe_cex_engine::num::round_half_up(
                    rows.iter().map(|p| p.unrealized_pnl).sum::<f64>() * 100.0,
                ) / 100.0;
                if stamps
                    .iter()
                    .any(|(id, new)| snapshot_older(self.account.position_stamps.get(id), *new))
                {
                    return;
                }
                for p in &rows {
                    if let Some(m) = p.margin_type {
                        self.account
                            .leverage
                            .insert((p.symbol.clone(), m == MarginType::Isolated), p.leverage);
                    }
                }
                for (id, new) in stamps {
                    merge_stamp(&mut self.account.position_stamps, id, new);
                }
                cx.store.set_positions(rows);
                cx.store.set_balance(Balance {
                    total: meta.total_collateral_value - upnl,
                    free: meta.free_collateral,
                    used: meta.total_collateral_value - meta.free_collateral - upnl,
                    upnl,
                });
                for s in cx
                    .store
                    .markets()
                    .iter()
                    .map(|m| m.symbol.clone())
                    .collect::<Vec<_>>()
                {
                    self.sync_setting(cx, &s);
                }
            }
            Request::Orders {
                algo,
                rows,
                proofs,
                invalid,
                revision,
                page,
                seen,
                page_count,
                stamps,
            } => {
                let i = algo as usize;
                if invalid || revision != self.account.order_revisions[i] {
                    self.account.inflight[2 + i] = false;
                    return;
                }
                let Ok(meta) = serde_json::from_str::<Page>(data) else {
                    self.account.inflight[2 + i] = false;
                    cx.log(LogLevel::Error, "Orderly invalid order page");
                    return;
                };
                if meta.meta.as_ref().is_some_and(|m| {
                    m.current_page.is_some_and(|p| p != page)
                        || m.records_per_page.is_some_and(|n| n > 500)
                }) {
                    self.account.inflight[2 + i] = false;
                    cx.log(LogLevel::Error, "Orderly inconsistent pagination");
                    return;
                }
                let more = meta
                    .meta
                    .and_then(|m| m.total)
                    .map_or(page_count > 0, |total| (seen.len() as u64) < total);
                if more {
                    if page >= 50 || page_count == 0 {
                        self.account.inflight[2 + i] = false;
                        cx.log(LogLevel::Error, "Orderly order pagination incomplete");
                        return;
                    }
                    self.account_read(
                        cx,
                        Request::Orders {
                            algo,
                            rows,
                            proofs,
                            invalid: false,
                            revision,
                            page: page + 1,
                            seen,
                            page_count: 0,
                            stamps,
                        },
                    );
                    return;
                }
                self.account.inflight[2 + i] = false;
                if stamps
                    .iter()
                    .any(|(id, new)| snapshot_older(self.account.order_stamps.get(id), *new))
                {
                    return;
                }
                self.account.loaded[i] = true;
                if algo {
                    self.account.algo_snapshots += 1;
                }
                let mut combined: Vec<_> = cx
                    .store
                    .orders()
                    .iter()
                    .filter(|o| o.parent_id.is_some() != algo || o.status == OrderStatus::Creating)
                    .cloned()
                    .collect();
                self.account
                    .proofs
                    .retain(|id, _| combined.iter().any(|o| &o.id == id));
                self.account.proofs.extend(proofs);
                for (id, new) in stamps {
                    merge_stamp(&mut self.account.order_stamps, id, new);
                }
                combined.extend(rows.into_iter().filter(|o| {
                    !self.account.terminal.contains(&o.id)
                        && !o
                            .parent_id
                            .as_ref()
                            .is_some_and(|p| self.account.terminal.contains(&format!("root:{p}")))
                }));
                if self.account.loaded.iter().all(|b| *b) {
                    cx.store.set_orders(combined);
                } else {
                    for o in combined {
                        cx.store.upsert_order(o);
                    }
                }
            }
        }
    }
    pub(crate) fn account_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        error: &str,
    ) {
        cx.log(LogLevel::Error, error);
        match request {
            Request::Info => {
                if self.account.startup_retry.is_none() {
                    self.account.startup_retry = Some(cx.set_timer(5000.0));
                }
            }
            Request::Modes { .. } => self.account_ready(cx),
            Request::Positions { .. } => self.account.inflight[1] = false,
            Request::Orders { algo, .. } => self.account.inflight[2 + algo as usize] = false,
        }
    }
    fn private_connect(&mut self, cx: &mut Cx<'_, Pending>) {
        self.account.authenticated = false;
        self.account.socket.open = false;
        self.account.socket.received = cx.now;
        if let Some(timer) = self.account.socket.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.account.socket.socket =
            Some(cx.connect_channel(self.private_url(), safe_cex_engine::SocketChannel::Private));
        // Cover a transport that never produces either open or close.
        self.account.socket.watchdog = Some(cx.set_timer(10000.0));
    }
    pub(crate) fn private_open(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) || self.account.socket.open {
            return;
        }
        if let Some(timer) = self.account.socket.watchdog.take() {
            cx.clear_timer(timer);
        }
        self.account.authenticated = false;
        self.account.socket.open = true;
        self.account.socket.received = cx.now;
        let timestamp = cx.now as u64;
        let credentials = self.account.credentials.as_ref().unwrap();
        #[derive(Serialize)]
        struct Params<'a> {
            orderly_key: &'a str,
            sign: String,
            timestamp: u64,
        }
        #[derive(Serialize)]
        struct Auth<'a> {
            event: &'a str,
            params: Params<'a>,
        }
        match credentials.signature(&timestamp.to_string()) {
            Ok(sign) => cx.send(
                socket,
                serde_json::to_string(&Auth {
                    event: "auth",
                    params: Params {
                        orderly_key: &credentials.key,
                        sign,
                        timestamp,
                    },
                })
                .unwrap(),
            ),
            Err(e) => {
                cx.log(LogLevel::Error, e);
                cx.close(socket);
                self.private_close(cx, socket);
                return;
            }
        }
        self.account.socket.watchdog = Some(cx.set_timer(10000.0));
    }
    pub(crate) fn private_close(&mut self, cx: &mut Cx<'_, Pending>, socket: SocketId) {
        if self.account.socket.socket != Some(socket) {
            return;
        }
        self.account.authenticated = false;
        self.account.socket.socket = None;
        self.account.socket.open = false;
        if let Some(t) = self.account.socket.watchdog.take() {
            cx.clear_timer(t);
        }
        self.account.socket.attempts = self.account.socket.attempts.saturating_add(1);
        self.account.socket.reconnect = Some(cx.set_timer(
            (1000.0 * 2f64.powi(self.account.socket.attempts.min(6) as i32 - 1)).min(30000.0),
        ));
    }
    pub(crate) fn order_terminal(&self, id: &str) -> bool {
        self.account.terminal.contains(id)
    }
    pub(crate) fn root_terminal(&self, parent: &str) -> bool {
        self.account.terminal.contains(&format!("root:{parent}"))
    }
    pub(crate) fn terminal_order(&mut self, id: &str) {
        if self.account.terminal.insert(id.into()) {
            self.account.terminal_order.push_back(id.into());
        }
        if self.account.terminal_order.len() > 4096 {
            let id = self.account.terminal_order.pop_front().unwrap();
            self.account.terminal.remove(&id);
            self.account.order_stamps.remove(&id);
            self.account.totals.remove(&id);
        }
    }
    fn execution(&mut self, cx: &mut Cx<'_, Pending>, r: &OrderRow<'_>, order: &Order) {
        let filled = r
            .total_executed_quantity
            .or(r.executed)
            .map_or(0.0, |n| n.0);
        let previous = self
            .account
            .totals
            .get(&order.id)
            .copied()
            .unwrap_or((0.0, 0.0));
        let quantity = r
            .executed_quantity
            .map_or((filled - previous.0).max(0.0), |n| n.0);
        let total_value = r.average_executed_price.map_or(0.0, |n| n.0 * filled);
        let price = r.executed_price.map_or(
            if quantity > 0.0 {
                (total_value - previous.1) / quantity
            } else {
                0.0
            },
            |n| n.0,
        );
        if filled > previous.0 {
            self.account
                .totals
                .insert(order.id.clone(), (filled, total_value));
        }
        if quantity <= 0.0 || price <= 0.0 || !price.is_finite() {
            return;
        }
        let event = if !r.trade_id.0.is_empty() {
            format!("trade:{}:{}", order.id, r.trade_id.0)
        } else if !r.match_id.0.is_empty() {
            format!("match:{}:{}", order.id, r.match_id.0)
        } else {
            format!("cumulative:{}:{}", order.id, filled)
        };
        if !self.account.executions.insert(event.clone()) {
            return;
        }
        self.account.execution_order.push_back(event);
        if self.account.execution_order.len() > 4096 {
            self.account
                .executions
                .remove(&self.account.execution_order.pop_front().unwrap());
        }
        self.account.positions_revision += 1;
        cx.fill(Fill {
            order_id: order.id.clone(),
            symbol: order.symbol.clone(),
            side: order.side,
            price,
            amount: quantity,
        });
    }
    fn execution_row(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: &OrderRow<'_>,
        algo: bool,
        ts: u64,
    ) -> Result<(), String> {
        if algo {
            let raw_status = if matches!(
                r.root_algo_status.as_ref(),
                "FILLED" | "CANCELLED" | "REJECTED"
            ) {
                r.root_algo_status.as_ref()
            } else if r.algo_type == "POSITIONAL_TP_SL"
                || (r.child_orders.is_empty() && r.trigger_price.is_none())
            {
                if r.algo_status.is_empty() {
                    r.status.as_ref()
                } else {
                    r.algo_status.as_ref()
                }
            } else {
                ""
            };
            if let Ok(final_status @ (OrderStatus::Closed | OrderStatus::Canceled)) =
                status(raw_status)
            {
                let parent = if r.root_algo_order_id.0.is_empty() {
                    &r.algo_order_id.0
                } else {
                    &r.root_algo_order_id.0
                };
                if !parent.is_empty() && parent != "0" {
                    let key = format!("root:{parent}");
                    let timestamp = r.timestamp.unwrap_or(ts);
                    if stamp_newer(self.account.order_stamps.get(&key), r.seq, timestamp) {
                        self.account
                            .order_stamps
                            .insert(key.clone(), (r.seq, timestamp));
                        self.terminal_order(&key);
                        self.account.order_revisions[1] += 1;
                        let ids: Vec<_> = cx
                            .store
                            .orders()
                            .iter()
                            .filter(|o| o.parent_id.as_deref() == Some(parent.as_str()))
                            .map(|o| o.id.clone())
                            .collect();
                        for id in ids {
                            self.terminal_order(&id);
                            cx.store.update_order(&id, |o| o.status = final_status);
                        }
                    }
                }
            }
        }
        let orders = self.orders(cx, r, algo)?;
        for order in orders {
            let row = if r.child_orders.is_empty() {
                r
            } else {
                r.child_orders
                    .iter()
                    .find(|c| c.algo_order_id.0 == order.id)
                    .unwrap()
            };
            if matches!(
                if algo && !row.algo_status.is_empty() {
                    row.algo_status.as_ref()
                } else {
                    row.status.as_ref()
                },
                "PARTIAL_FILLED" | "PARTIALLY_FILLED" | "FILLED"
            ) {
                self.execution(cx, row, &order);
            }
            let timestamp = row.timestamp.unwrap_or(ts);
            if !stamp_newer(self.account.order_stamps.get(&order.id), row.seq, timestamp)
                || ((self.account.terminal.contains(&order.id)
                    || order
                        .parent_id
                        .as_ref()
                        .is_some_and(|p| self.account.terminal.contains(&format!("root:{p}"))))
                    && order.status == OrderStatus::Open)
            {
                continue;
            }
            self.account
                .order_stamps
                .insert(order.id.clone(), (row.seq, timestamp));
            self.account.order_revisions[algo as usize] += 1;
            if matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled) {
                self.terminal_order(&order.id);
            }
            let mode = margin(if row.margin_mode.is_empty() {
                &r.margin_mode
            } else {
                &row.margin_mode
            })?;
            self.account
                .proofs
                .insert(order.id.clone(), Proof { mode, amend: false });
            cx.store.upsert_order(order);
        }
        Ok(())
    }
    pub(crate) fn private_message(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        text: &str,
    ) -> Result<(), String> {
        let env: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let socket = self.account.socket.socket.unwrap();
        self.account.socket.received = cx.now;
        if env.event == "auth" {
            if env.success != Some(true) {
                cx.close(socket);
                self.private_close(cx, socket);
                return Err(format!(
                    "Orderly socket authentication rejected: {}",
                    env.message
                ));
            }
            if !self.account.authenticated {
                self.account.authenticated = true;
                self.account.socket.attempts = 0;
                for topic in ["executionreport", "algoexecutionreportv2", "position"] {
                    #[derive(Serialize)]
                    struct Sub<'a> {
                        topic: &'a str,
                        event: &'a str,
                    }
                    cx.send(
                        socket,
                        serde_json::to_string(&Sub {
                            topic,
                            event: "subscribe",
                        })
                        .unwrap(),
                    );
                }
                self.account.socket.ping_at = cx.now;
                cx.send(socket, "{\"event\":\"ping\"}");
                self.account_poll(cx, true);
            }
            return Ok(());
        }
        if env.event == "ping" {
            cx.send(socket, "{\"event\":\"pong\"}");
            return Ok(());
        }
        if env.event == "pong" {
            return Ok(());
        }
        if env.success == Some(false) {
            cx.close(socket);
            self.private_close(cx, socket);
            return Err(format!(
                "Orderly private subscription rejected: {}",
                env.message
            ));
        }
        if !self.account.authenticated {
            return Ok(());
        }
        let Some(data) = env.data else {
            return Ok(());
        };
        match env.topic.as_ref() {
            "executionreport" | "algoexecutionreportv2" => {
                let algo = env.topic == "algoexecutionreportv2";
                if data.get().starts_with('[') {
                    let mut result = Ok(());
                    json::each::<OrderRow<'_>>(data.get(), |r| {
                        if result.is_ok() {
                            result = self.execution_row(cx, &r, algo, env.ts);
                        }
                    })
                    .map_err(|e| e.to_string())?;
                    result?;
                } else {
                    let r: OrderRow<'_> =
                        serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                    self.execution_row(cx, &r, algo, env.ts)?;
                }
            }
            "position" => {
                let push: PositionPush<'_> =
                    serde_json::from_str(data.get()).map_err(|e| e.to_string())?;
                for r in push.positions {
                    if let Some(p) = self.position(cx, &r)? {
                        let ts = r.timestamp.unwrap_or(env.ts);
                        if stamp_newer(self.account.position_stamps.get(&p.id), r.seq, ts) {
                            self.account
                                .position_stamps
                                .insert(p.id.clone(), (r.seq, ts));
                            self.account.positions_revision += 1;
                            cx.store.upsert_position(p);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn account_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) -> bool {
        if self.account.startup_retry == Some(timer) {
            self.account.startup_retry = None;
            self.account_read(cx, Request::Info);
            return true;
        }
        if self.account.tick == Some(timer) {
            self.account.tick = None;
            self.account.ticks = self.account.ticks.wrapping_add(1);
            self.account_poll(cx, self.account.ticks.is_multiple_of(4));
            self.account.tick = Some(cx.set_timer(self.poll_interval()));
            return true;
        }
        if self.account.socket.reconnect == Some(timer) {
            self.account.socket.reconnect = None;
            self.private_connect(cx);
            return true;
        }
        if self.account.socket.watchdog == Some(timer) {
            self.account.socket.watchdog = None;
            let socket = self.account.socket.socket.unwrap();
            if !self.account.socket.open
                || !self.account.authenticated
                || cx.now - self.account.socket.received >= 20000.0
            {
                cx.close(socket);
                self.private_close(cx, socket);
            } else {
                self.account.socket.ping_at = cx.now;
                cx.send(socket, "{\"event\":\"ping\"}");
                self.account.socket.watchdog = Some(cx.set_timer(10000.0));
            }
            return true;
        }
        false
    }
}
