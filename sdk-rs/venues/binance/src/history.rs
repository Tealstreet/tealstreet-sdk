//! Rare request/response reads. Results go back as JSON in the command's
//! completion and never enter the flush frame.
use crate::account::{order_side, order_type, venue_error};
use crate::sign::Query;
use crate::wire::{HistoricalOrderRow, IncomeRow, PublicTrade, UserTrade};
use crate::{BinanceFutures, Pending};
use safe_cex_engine::model::{Order, OrderStatus};
use safe_cex_engine::paging::{PageRow, RangeWalk, Step};
use safe_cex_engine::{
    CommandId, Cx, LogLevel, Method, Query as Read, Response, TimerId, json, num,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{HashMap, VecDeque};

const PAGE_LIMIT: u32 = 1000;
/// `/fapi/v1/userTrades` and `/fapi/v1/income` reject wider windows.
const MAX_WINDOW_MS: f64 = 7.0 * 24.0 * 3_600_000.0;
/// Income costs weight 30; chunks are paced to stay inside the weight budget.
const INCOME_CHUNK_DELAY_MS: f64 = 2_000.0;

pub struct Request {
    command: CommandId,
    window: (f64, f64),
}

enum InFlight {
    Lookup {
        symbol: String,
        client_order_id: String,
    },
    Fills {
        symbol: String,
        order_id: Option<String>,
        limit: u32,
        windows: VecDeque<(Option<f64>, Option<f64>)>,
        rows: Vec<Execution>,
    },
    Orders,
    Income {
        symbol: Option<String>,
        kind: Option<String>,
        walk: Option<RangeWalk>,
        limit: u32,
        rows: Vec<Income>,
        delay: Option<TimerId>,
    },
    Trades {
        symbol: String,
    },
}

#[derive(Default)]
pub struct HistoryState {
    reads: HashMap<CommandId, InFlight>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LookupRow<'a> {
    #[serde(flatten, borrow)]
    order: HistoricalOrderRow<'a>,
    #[serde(borrow)]
    client_order_id: std::borrow::Cow<'a, str>,
    #[serde(default)]
    time_in_force: &'a str,
}

#[derive(Serialize)]
struct Found<'a> {
    found: bool,
    order: LookupOrder<'a>,
}
#[derive(Serialize)]
struct LookupOrder<'a> {
    #[serde(flatten)]
    order: Order,
    #[serde(skip_serializing_if = "Option::is_none")]
    info: Option<&'a RawValue>,
}
fn found(order: Order, info: Option<&RawValue>) -> Result<String, String> {
    serde_json::to_string(&Found {
        found: true,
        order: LookupOrder { order, info },
    })
    .map_err(|e| e.to_string())
}
const UNKNOWN: &str = r#"{"found":false,"definitive":false}"#;
const ABSENT: &str = r#"{"found":false,"definitive":true}"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Execution {
    id: String,
    /// Seconds.
    timestamp: f64,
    symbol: String,
    order_id: String,
    price: f64,
    qty: f64,
    side: &'static str,
    commission: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    commission_asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    realized_pnl: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    maker: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoricalOrder {
    id: String,
    symbol: String,
    side: &'static str,
    #[serde(rename = "type")]
    kind: safe_cex_engine::model::OrderType,
    status: OrderStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    average_price: Option<f64>,
    amount: f64,
    filled: f64,
    reduce_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_side: Option<&'static str>,
    created_time: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    updated_time: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Income {
    symbol: String,
    income_type: String,
    income: f64,
    asset: String,
    timestamp: f64,
    tran_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trade_id: Option<String>,
    category: &'static str,
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

fn side_name(side: &str) -> &'static str {
    match order_side(side) {
        safe_cex_engine::model::OrderSide::Buy => "buy",
        safe_cex_engine::model::OrderSide::Sell => "sell",
    }
}

fn hedge_side(side: Option<&str>) -> Option<&'static str> {
    Some(match side?.to_ascii_lowercase().as_str() {
        "long" => "long",
        "short" => "short",
        _ => "both",
    })
}

fn income_category(kind: &str) -> &'static str {
    match kind.to_ascii_uppercase().as_str() {
        "REALIZED_PNL" | "DELIVERED_SETTELMENT" => "realized_pnl",
        "FUNDING_FEE" => "funding",
        "COMMISSION" | "OPTIONS_PREMIUM_FEE" | "POSITION_LIMIT_INCREASE_FEE" => "commission",
        "COMMISSION_REBATE" | "REFERRAL_KICKBACK" | "API_REBATE" => "rebate",
        "TRANSFER"
        | "INTERNAL_TRANSFER"
        | "CROSS_COLLATERAL_TRANSFER"
        | "COIN_SWAP_DEPOSIT"
        | "COIN_SWAP_WITHDRAW"
        | "AUTO_EXCHANGE" => "transfer",
        "INSURANCE_CLEAR" => "liquidation",
        _ => "other",
    }
}

fn nonzero(text: &str) -> Option<f64> {
    Some(num::parse(text)).filter(|v| *v != 0.0)
}

impl BinanceFutures {
    pub(crate) fn start_query(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId, read: Read) {
        if self.is_spot() || (self.is_inverse() && !matches!(read, Read::PublicTrades { .. })) {
            if matches!(
                read,
                Read::OrderByClientId { .. } | Read::PublicTrades { .. }
            ) {
                cx.done(command, Err("Binance Spot query is not supported".into()));
            } else {
                cx.done(command, Ok("[]".into()));
            }
            return;
        }
        if let Read::OrderByClientId {
            symbol,
            client_order_id,
        } = read
        {
            self.lookup(cx, command, symbol, client_order_id);
            return;
        }
        if self.credentials.is_none() && !matches!(read, Read::PublicTrades { .. }) {
            cx.done(command, Ok("[]".into()));
            return;
        }
        match read {
            Read::OrderByClientId { .. } => unreachable!(),
            Read::Fills {
                symbol,
                order_id,
                from_ms,
                to_ms,
                limit,
            } => {
                // userTrades requires a symbol.
                let Some(symbol) = symbol else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                let windows = match (from_ms, to_ms) {
                    (Some(from), Some(to)) => split(from, to),
                    _ => VecDeque::from([(from_ms, to_ms)]),
                };
                self.history.reads.insert(
                    command,
                    InFlight::Fills {
                        symbol,
                        order_id,
                        limit: limit.unwrap_or(PAGE_LIMIT).clamp(1, PAGE_LIMIT),
                        windows,
                        rows: Vec::new(),
                    },
                );
                self.next_fills_window(cx, command);
            }
            Read::OrderHistory {
                symbol,
                from_ms,
                to_ms,
                limit,
            } => {
                let Some(symbol) = symbol else {
                    cx.done(command, Ok("[]".into()));
                    return;
                };
                self.history.reads.insert(command, InFlight::Orders);
                let query = Query::new()
                    .str("symbol", &symbol)
                    .opt_num("startTime", from_ms)
                    .opt_num("endTime", to_ms)
                    .num(
                        "limit",
                        f64::from(limit.unwrap_or(PAGE_LIMIT).min(PAGE_LIMIT)),
                    );
                self.history_request(cx, command, self.paths.all_orders, query, (0.0, 0.0));
            }
            Read::Income {
                symbol,
                kind,
                from_ms,
                to_ms,
                limit,
            } => {
                let walk = match (from_ms, to_ms) {
                    (Some(from), Some(to)) => {
                        Some(RangeWalk::new(from, to, MAX_WINDOW_MS, PAGE_LIMIT as usize))
                    }
                    _ => None,
                };
                let limit = limit.unwrap_or(PAGE_LIMIT).clamp(1, PAGE_LIMIT);
                let single = walk.is_none();
                self.history.reads.insert(
                    command,
                    InFlight::Income {
                        symbol,
                        kind,
                        walk,
                        limit,
                        rows: Vec::new(),
                        delay: None,
                    },
                );
                if single {
                    self.income_request(cx, command, from_ms, to_ms);
                } else {
                    self.next_income_page(cx, command);
                }
            }
            Read::PublicTrades { symbol, limit } => {
                let limit = limit.unwrap_or(PAGE_LIMIT).min(PAGE_LIMIT);
                let path = format!("{}?symbol={symbol}&limit={limit}", self.paths.trades);
                self.history
                    .reads
                    .insert(command, InFlight::Trades { symbol });
                cx.request(
                    Method::Get,
                    format!("{}{path}", self.urls.rest),
                    Vec::new(),
                    None,
                    Pending::History(Request {
                        command,
                        window: (0.0, 0.0),
                    }),
                );
            }
        }
    }

    fn lookup(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        symbol: String,
        client_order_id: String,
    ) {
        let max = if self.options.portfolio_margin {
            32
        } else {
            36
        };
        if client_order_id.is_empty()
            || client_order_id.len() > max
            || !client_order_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
        {
            cx.done(command, Err("Invalid Binance client order ID".into()));
            return;
        }
        if cx.store.market(&symbol).is_none() {
            cx.done(command, Ok(UNKNOWN.into()));
            return;
        }
        // Secondary GET snapshots may belong to another account. Their cached
        // client IDs cannot establish primary placement identity.
        if self.options.read_credentials.is_none()
            && let Some(order) = cx.store.orders().iter().find(|order| {
                order.symbol == symbol && order.client_order_id.as_deref() == Some(&client_order_id)
            })
        {
            cx.done(command, found(order.clone(), None));
            return;
        }
        if self.credentials.is_none() || self.options.public_data_only {
            cx.done(
                command,
                Err("Binance client order lookup requires credentials".into()),
            );
            return;
        }
        let query = Query::new()
            .str("symbol", &symbol)
            .str("origClientOrderId", &client_order_id);
        self.history.reads.insert(
            command,
            InFlight::Lookup {
                symbol,
                client_order_id,
            },
        );
        // Placement identity belongs to the primary account, even when reads use a secondary key.
        let credentials = self
            .credentials
            .as_ref()
            .expect("lookup credentials checked");
        let query = query.signed(credentials, cx.now);
        let request = cx.request(
            Method::Get,
            format!("{}{}?{query}", self.urls.account_rest, self.paths.order),
            vec![("X-MBX-APIKEY", credentials.key.clone())],
            None,
            Pending::History(Request {
                command,
                window: (0.0, 0.0),
            }),
        );
        cx.route_request(request, safe_cex_engine::RequestRoute::Primary);
    }

    fn lookup_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        response: Result<Response<'_>, &str>,
    ) {
        let Some(InFlight::Lookup {
            symbol,
            client_order_id,
        }) = self.history.reads.remove(&command)
        else {
            return;
        };
        let result = (|| {
            let response = response.map_err(str::to_owned)?;
            if let Ok(error) = serde_json::from_str::<crate::wire::ErrorBody<'_>>(response.body) {
                if error.code == -2013
                    && ((200..300).contains(&response.status) || response.status == 400)
                {
                    return Ok(ABSENT.into());
                }
                if error.code != 0 {
                    return Err(format!("Binance {}: {}", error.code, error.msg));
                }
            }
            if !(200..300).contains(&response.status) {
                return Err(format!("Binance HTTP {}", response.status));
            }
            let native: LookupRow<'_> = serde_json::from_str(response.body)
                .map_err(|e| format!("Malformed Binance lookup: {e}"))?;
            let row = native.order;
            if row.symbol != symbol || native.client_order_id.as_ref() != client_order_id {
                return Err("Binance lookup identity mismatch".into());
            }
            let status = match row.status.as_ref() {
                "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
                "FILLED" => OrderStatus::Closed,
                "CANCELED" | "EXPIRED" | "EXPIRED_IN_MATCH" | "REJECTED" => OrderStatus::Canceled,
                _ => return Err("Invalid Binance lookup status".into()),
            };
            let parse = |text: &str| {
                text.parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .ok_or_else(|| "Invalid Binance lookup number".to_owned())
            };
            let amount = parse(&row.orig_qty)?;
            let filled = parse(&row.executed_qty)?;
            if row.order_id == 0
                || !matches!(row.side.as_ref(), "BUY" | "SELL")
                || amount <= 0.0
                || filled > amount
                || !row.time.is_finite()
                || row.time <= 0.0
                || !row.update_time.is_finite()
                || row.update_time < 0.0
            {
                return Err("Invalid Binance lookup order".into());
            }
            let kind = row
                .orig_type
                .as_deref()
                .and_then(order_type)
                .or_else(|| order_type(&row.kind))
                .ok_or_else(|| "Invalid Binance lookup type".to_owned())?;
            let order = Order {
                id: if self.options.portfolio_margin {
                    client_order_id.clone()
                } else {
                    row.order_id.to_string()
                },
                parent_id: None,
                status,
                symbol,
                kind,
                side: order_side(&row.side),
                price: parse(&row.price)?,
                amount,
                filled,
                remaining: (amount - filled).max(0.0),
                reduce_only: row.reduce_only,
                close: None,
                client_order_id: Some(client_order_id),
                time_in_force: crate::account::time_in_force(native.time_in_force),
                last_update: if row.update_time > 0.0 {
                    row.update_time
                } else {
                    row.time
                },
                trigger_type: None,
                take_profit: None,
                stop_loss: None,
                extra: None,
            };
            let info =
                serde_json::from_str::<&RawValue>(response.body).map_err(|e| e.to_string())?;
            found(order, Some(info))
        })();
        cx.done(command, result);
    }

    fn history_request(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        path: &str,
        query: Query,
        window: (f64, f64),
    ) {
        self.signed(
            cx,
            Method::Get,
            path,
            query,
            Pending::History(Request { command, window }),
        );
    }

    fn next_fills_window(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(InFlight::Fills {
            symbol,
            order_id,
            limit,
            windows,
            rows,
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        let Some((from, to)) = windows.pop_front() else {
            let json = serde_json::to_string(rows).unwrap_or_default();
            self.history.reads.remove(&command);
            cx.done(command, Ok(json));
            return;
        };
        let query = Query::new()
            .str("symbol", symbol)
            .num("limit", f64::from(*limit))
            .opt_str("orderId", order_id.as_deref())
            .opt_num("startTime", from)
            .opt_num("endTime", to);
        self.history_request(cx, command, self.paths.user_trades, query, (0.0, 0.0));
    }

    fn income_request(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        from: Option<f64>,
        to: Option<f64>,
    ) {
        let Some(InFlight::Income {
            symbol,
            kind,
            limit,
            ..
        }) = self.history.reads.get(&command)
        else {
            return;
        };
        let query = Query::new()
            .num("limit", f64::from(*limit))
            .opt_str("symbol", symbol.as_deref())
            .opt_str("incomeType", kind.as_deref())
            .opt_num("startTime", from)
            .opt_num("endTime", to);
        let window = (from.unwrap_or(f64::NAN), to.unwrap_or(f64::NAN));
        self.history_request(cx, command, self.paths.income, query, window);
    }

    fn next_income_page(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(InFlight::Income {
            walk, rows, delay, ..
        }) = self.history.reads.get_mut(&command)
        else {
            return;
        };
        let Some(walk) = walk else {
            return;
        };
        match walk.step() {
            Step::Fetch { from, to } => self.income_request(cx, command, Some(from), Some(to)),
            Step::ChunkDone => *delay = Some(cx.set_timer(INCOME_CHUNK_DELAY_MS)),
            Step::Finished => {
                if walk.truncated {
                    cx.log(
                        LogLevel::Warning,
                        format!(
                            "Income history is INCOMPLETE: {} chunk(s) could not be walked to completion. Any total derived from it is a lower bound.",
                            walk.exhausted_chunks
                        ),
                    );
                }
                rows.sort_by(|a, b| b.timestamp.total_cmp(&a.timestamp));
                let json = serde_json::to_string(rows).unwrap_or_default();
                self.history.reads.remove(&command);
                cx.done(command, Ok(json));
            }
        }
    }

    pub(crate) fn on_history_timer(&mut self, cx: &mut Cx<'_, Pending>, timer: TimerId) {
        let due: Vec<CommandId> = self
            .history
            .reads
            .iter_mut()
            .filter_map(|(command, read)| match read {
                InFlight::Income { delay, .. } if *delay == Some(timer) => {
                    *delay = None;
                    Some(*command)
                }
                _ => None,
            })
            .collect();
        for command in due {
            self.next_income_page(cx, command);
        }
    }

    pub(crate) fn on_history_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        let Request { command, window } = request;
        if matches!(
            self.history.reads.get(&command),
            Some(InFlight::Lookup { .. })
        ) {
            self.lookup_response(cx, command, response);
            return;
        }
        let body = match response {
            Ok(response) => match venue_error(&response) {
                None => Ok(response.body),
                Some((_, message)) => Err(message),
            },
            Err(error) => Err(error.to_owned()),
        };
        let Some(read) = self.history.reads.get_mut(&command) else {
            return;
        };
        match read {
            InFlight::Lookup { .. } => unreachable!(),
            InFlight::Fills { rows, .. } => match body {
                Ok(body) => {
                    let _ = json::each::<UserTrade>(body, |t| {
                        let realized = t.realized_pnl.as_deref().map(num::parse);
                        rows.push(Execution {
                            id: t.id.to_string(),
                            timestamp: (t.time / 1000.0).floor(),
                            symbol: t.symbol.into_owned(),
                            order_id: t.order_id.to_string(),
                            price: num::parse(&t.price),
                            qty: num::parse(&t.qty),
                            side: side_name(&t.side),
                            commission: t.commission.as_deref().map_or(0.0, num::parse),
                            commission_asset: t.commission_asset.map(|a| a.into_owned()),
                            position_side: hedge_side(t.position_side.as_deref()),
                            realized_pnl: realized,
                            maker: t.maker,
                        });
                    });
                    self.next_fills_window(cx, command);
                }
                Err(message) => {
                    cx.command_error(command, format!("Error fetching executions: {message}"));
                    self.history.reads.remove(&command);
                    cx.done(command, Ok("[]".into()));
                }
            },
            InFlight::Orders => {
                self.history.reads.remove(&command);
                match body {
                    Ok(body) => {
                        let mut orders = Vec::new();
                        let _ = json::each::<HistoricalOrderRow>(body, |o| {
                            if o.time == 0.0 || !matches!(o.side.as_ref(), "BUY" | "SELL") {
                                return;
                            }
                            let kind = o
                                .orig_type
                                .as_deref()
                                .and_then(order_type)
                                .or_else(|| order_type(&o.kind))
                                .unwrap_or(safe_cex_engine::model::OrderType::Limit);
                            let status = match o.status.as_ref() {
                                "NEW" | "PARTIALLY_FILLED" => OrderStatus::Open,
                                "CANCELED" | "EXPIRED" | "REJECTED" => OrderStatus::Canceled,
                                _ => OrderStatus::Closed,
                            };
                            orders.push(HistoricalOrder {
                                id: o.order_id.to_string(),
                                symbol: o.symbol.into_owned(),
                                side: side_name(&o.side),
                                kind,
                                status,
                                price: nonzero(&o.price),
                                average_price: o.avg_price.as_deref().and_then(nonzero),
                                amount: num::parse(&o.orig_qty),
                                filled: num::parse(&o.executed_qty),
                                reduce_only: o.reduce_only,
                                position_side: hedge_side(o.position_side.as_deref()),
                                created_time: o.time,
                                updated_time: (o.update_time > 0.0).then_some(o.update_time),
                            });
                        });
                        orders.sort_by(|a, b| b.created_time.total_cmp(&a.created_time));
                        cx.done(
                            command,
                            Ok(serde_json::to_string(&orders).unwrap_or_default()),
                        );
                    }
                    Err(message) => {
                        cx.command_error(command, message);
                        cx.done(command, Ok("[]".into()));
                    }
                }
            }
            InFlight::Income { walk, rows, .. } => {
                let mut page = Vec::new();
                let parsed = body.and_then(|body| {
                    json::each::<IncomeRow>(body, |r| {
                        page.push(Income {
                            symbol: r.symbol.into_owned(),
                            category: income_category(&r.income_type),
                            income_type: r.income_type.into_owned(),
                            income: num::parse(&r.income),
                            asset: r.asset.into_owned(),
                            timestamp: r.time,
                            tran_id: r.tran_id.to_string(),
                            trade_id: r.trade_id.map(|t| t.into_owned()).filter(|t| !t.is_empty()),
                        });
                    })
                    .map_err(|error| error.to_string())
                });
                match walk {
                    None => {
                        let result = match parsed {
                            Ok(()) => serde_json::to_string(&page).unwrap_or_default(),
                            Err(message) => {
                                cx.command_error(
                                    command,
                                    format!("Error fetching income history: {message}"),
                                );
                                "[]".into()
                            }
                        };
                        self.history.reads.remove(&command);
                        cx.done(command, Ok(result));
                    }
                    Some(walk) => {
                        match parsed {
                            Ok(()) => {
                                let identities: Vec<PageRow> = page
                                    .iter()
                                    .map(|r| PageRow {
                                        timestamp: r.timestamp,
                                        identity: format!(
                                            "{}|{}|{}|{}",
                                            r.tran_id, r.category, r.income_type, r.income
                                        ),
                                    })
                                    .collect();
                                let fresh = walk.page(window.0, window.1, &identities);
                                let mut page: Vec<Option<Income>> =
                                    page.into_iter().map(Some).collect();
                                rows.extend(fresh.into_iter().filter_map(|i| page[i].take()));
                            }
                            Err(_) => walk.failed(),
                        }
                        self.next_income_page(cx, command);
                    }
                }
            }
            InFlight::Trades { symbol } => {
                let symbol = symbol.clone();
                self.history.reads.remove(&command);
                let mut trades = Vec::new();
                if let Ok(body) = body {
                    let _ = json::each::<PublicTrade>(body, |t| {
                        trades.push(Trade {
                            id: t.id.to_string(),
                            symbol: symbol.clone(),
                            price: num::parse(&t.price),
                            qty: num::parse(&t.qty),
                            quote_qty: num::parse(&t.quote_qty),
                            timestamp: t.time,
                            // A buyer-maker trade was an aggressive sell.
                            side: if t.is_buyer_maker { "sell" } else { "buy" },
                        });
                    });
                }
                cx.done(
                    command,
                    Ok(serde_json::to_string(&trades).unwrap_or_default()),
                );
            }
        }
    }
}

/// Split `[from, to]` into windows the venue accepts.
fn split(from: f64, to: f64) -> VecDeque<(Option<f64>, Option<f64>)> {
    let mut windows = VecDeque::new();
    let mut start = from;
    while start < to {
        let end = (start + MAX_WINDOW_MS).min(to);
        windows.push_back((Some(start), Some(end)));
        start = end + 1.0;
    }
    if windows.is_empty() {
        windows.push_back((Some(from), Some(to)));
    }
    windows
}
