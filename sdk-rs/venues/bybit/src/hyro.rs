//! Hyro's create identity survives lost acknowledgements and caller retries.
//! Only complete absence on both order axes permits another same-ID dispatch.
use crate::account::{map_order, valid_hyro_order};
use crate::sign::{Body, Query};
use crate::wire::{OrderRow, Placed, Status, present};
use crate::{BybitLinear, Pending, Rest, venue_error};
use safe_cex_engine::model::{Order, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{CommandId, Cx, LogLevel, Response};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

type Boundary = (u32, u32);
const MAX_LOOKUP_PAGES: usize = 100;

#[derive(Default)]
pub(crate) struct State {
    groups: HashMap<String, Group>,
}

struct Create {
    base: Body,
    stop_body: Option<String>,
    blocked: bool,
    link: String,
    symbol: String,
    optimistic: Order,
    uncertain: bool,
    accepted: Option<String>,
    observed: bool,
    attempts: u8,
}

struct Group {
    creates: Vec<Create>,
    index: usize,
    ids: Vec<String>,
    waiters: Vec<CommandId>,
    running: bool,
}

pub(crate) enum Request {
    Stop {
        key: String,
        boundary: Boundary,
    },
    Create {
        key: String,
        boundary: Boundary,
    },
    Lookup {
        key: String,
        boundary: Boundary,
        history: bool,
        seen: HashSet<String>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Page<'a> {
    ret_code: Option<i64>,
    #[serde(borrow)]
    result: PageResult<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PageResult<'a> {
    #[serde(borrow)]
    list: Vec<OrderRow<'a>>,
    #[serde(default)]
    next_page_cursor: Option<String>,
}

impl BybitLinear {
    fn hyro_boundary(&self) -> Boundary {
        (
            self.account.session_version,
            self.account.orders_snapshot_version,
        )
    }

    pub(crate) fn observe_hyro_order(&mut self, row: &OrderRow<'_>) {
        let Some(link) = present(&row.order_link_id) else {
            return;
        };
        for group in self.trading.hyro.groups.values_mut() {
            for create in &mut group.creates {
                if create.link == link {
                    create.observed = true;
                    if !row.order_id.is_empty() && row.order_status != "Rejected" {
                        create.accepted = Some(row.order_id.to_string());
                    }
                }
            }
        }
    }

    pub(crate) fn start_hyro_place(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        orders: &[PlaceOrderOpts],
    ) {
        let planned = (|| {
            self.check_ids(cx, orders)?;
            if self.credentials.is_none() {
                return Err("HyroTrader trading needs an API key".to_owned());
            }
            if !self.private_ready() {
                return Err(
                    "HyroTrader account is reconnecting; wait for private state reconciliation"
                        .to_owned(),
                );
            }
            let mut creates = Vec::new();
            let mut parts = Vec::new();
            for opts in orders {
                let market = cx
                    .store
                    .market(&opts.symbol)
                    .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
                if market.quote != "USDT" {
                    return Err("HyroTrader supports USDT perpetuals only".into());
                }
                if let Some(stop) = self.hyro_stop(cx, opts)? {
                    parts.push(stop.clone());
                    creates.push((Body::new(), opts.clone(), Some(stop)));
                    continue;
                }
                for lot in self.format_lots(cx, opts)? {
                    let base = lot.single;
                    parts.push(base.clone().finish());
                    creates.push((base, opts.clone(), None));
                }
            }
            Ok((serde_json::to_string(&parts).unwrap_or_default(), creates))
        })();
        let (key, creates) = match planned {
            Ok(planned) => planned,
            Err(message) => {
                cx.done(command, Err(message));
                return;
            }
        };
        if let Some(group) = self.trading.hyro.groups.get_mut(&key) {
            group.waiters.push(command);
            if group.running {
                return;
            }
            group.running = true;
            for create in &mut group.creates {
                create.attempts = 0;
            }
        } else {
            let creates: Result<Vec<Create>, String> = creates
                .into_iter()
                .map(|(base, opts, stop_body)| {
                    let link = safe_cex_crypto::uuid::uuid()?;
                    // Publish the sent lot, not the original unsplit quantity.
                    #[derive(Deserialize)]
                    struct Lot {
                        qty: String,
                        #[serde(default)]
                        price: Option<String>,
                        #[serde(default, rename = "triggerPrice")]
                        trigger_price: Option<String>,
                        #[serde(default, rename = "stopLoss")]
                        stop_loss: Option<String>,
                        #[serde(default, rename = "takeProfit")]
                        take_profit: Option<String>,
                    }
                    let sent: Lot = if stop_body.is_some() {
                        Lot {
                            qty: "0".into(),
                            price: None,
                            trigger_price: None,
                            stop_loss: None,
                            take_profit: None,
                        }
                    } else {
                        serde_json::from_str(&base.clone().finish()).map_err(|e| e.to_string())?
                    };
                    let amount = sent.qty.parse::<f64>().map_err(|e| e.to_string())?;
                    let price = sent
                        .price
                        .or(sent.trigger_price)
                        .and_then(|p| p.parse().ok())
                        .or_else(|| cx.store.ticker(&opts.symbol).map(|t| t.last))
                        .unwrap_or(0.0);
                    let optimistic = Order {
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
                        reduce_only: opts.reduce_only.unwrap_or(false),
                        close: opts.close,
                        client_order_id: Some(link.clone()),
                        time_in_force: opts.time_in_force,
                        last_update: cx.now,
                        trigger_type: opts.trigger_type,
                        take_profit: sent.take_profit.and_then(|p| p.parse().ok()),
                        stop_loss: sent.stop_loss.and_then(|p| p.parse().ok()),
                        extra: None,
                    };
                    Ok(Create {
                        base,
                        stop_body,
                        blocked: false,
                        link,
                        symbol: opts.symbol,
                        optimistic,
                        uncertain: false,
                        accepted: None,
                        observed: false,
                        attempts: 0,
                    })
                })
                .collect();
            let creates = match creates {
                Ok(creates) => creates,
                Err(error) => {
                    cx.done(command, Err(error));
                    return;
                }
            };
            self.trading.hyro.groups.insert(
                key.clone(),
                Group {
                    creates,
                    index: 0,
                    ids: Vec::new(),
                    waiters: vec![command],
                    running: true,
                },
            );
        }
        self.advance_hyro(cx, key);
    }

    fn advance_hyro(&mut self, cx: &mut Cx<'_, Pending>, key: String) {
        let Some(group) = self.trading.hyro.groups.get(&key) else {
            return;
        };
        if group.index >= group.creates.len() {
            let group = self.trading.hyro.groups.remove(&key).expect("present");
            let result = serde_json::to_string(&group.ids).unwrap_or_default();
            for command in group.waiters {
                cx.done(command, Ok(result.clone()));
            }
            return;
        }
        if group.creates[group.index].blocked {
            self.pause_hyro(
                cx,
                &key,
                "position-stop outcome requires account reconciliation",
                false,
            );
            return;
        }
        if group.creates[group.index].uncertain {
            self.lookup_hyro(
                cx,
                key,
                false,
                String::new(),
                HashSet::new(),
                self.hyro_boundary(),
            );
            return;
        }
        if !self.private_ready() {
            self.pause_hyro(cx, &key, "private state is recovering", false);
            return;
        }
        self.note_hyro_write();
        let boundary = self.hyro_boundary();
        let group = self.trading.hyro.groups.get_mut(&key).expect("present");
        let create = &mut group.creates[group.index];
        create.attempts += 1;
        if let Some(body) = create.stop_body.clone() {
            self.send_hyro(
                cx,
                Rest::post("/v5/position/trading-stop", body),
                Request::Stop { key, boundary },
            );
            return;
        }
        let body = create
            .base
            .clone()
            .str("orderLinkId", &create.link)
            .finish();
        self.send_hyro(
            cx,
            Rest::post("/v5/order/create", body),
            Request::Create { key, boundary },
        );
    }

    fn lookup_hyro(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        key: String,
        history: bool,
        cursor: String,
        seen: HashSet<String>,
        boundary: Boundary,
    ) {
        let group = self.trading.hyro.groups.get(&key).expect("present");
        let create = &group.creates[group.index];
        let path = if history {
            "/v5/order/history"
        } else {
            "/v5/order/realtime"
        };
        let query = Query::new()
            .str("category", "linear")
            .str("symbol", &create.symbol)
            .num("limit", 50.0)
            .str("cursor", &cursor);
        self.send_hyro(
            cx,
            Rest::get(path, query).signed(),
            Request::Lookup {
                key,
                boundary,
                history,
                seen,
            },
        );
    }

    fn pause_hyro(&mut self, cx: &mut Cx<'_, Pending>, key: &str, cause: &str, rejected: bool) {
        let Some(group) = self.trading.hyro.groups.get_mut(key) else {
            return;
        };
        let create = &mut group.creates[group.index];
        let message = if rejected {
            format!("HyroTrader order rejected: {cause}")
        } else {
            format!(
                "HyroTrader order outcome is unknown (orderLinkId {}). Reconcile before placing a replacement: {cause}",
                create.link
            )
        };
        cx.log(LogLevel::Error, message.clone());
        for command in group.waiters.drain(..) {
            cx.done(command, Err(message.clone()));
        }
        group.running = false;
        if rejected {
            if group.index == 0 {
                self.trading.hyro.groups.remove(key);
            } else if let Ok(link) = safe_cex_crypto::uuid::uuid() {
                create.link = link.clone();
                create.optimistic.client_order_id = Some(link);
                create.uncertain = false;
                create.accepted = None;
                create.observed = false;
            }
        }
    }

    fn accept_hyro(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        key: String,
        id: String,
        rows: Option<Vec<Order>>,
        terminal: bool,
        reconciled: bool,
    ) {
        let group = self.trading.hyro.groups.get_mut(&key).expect("present");
        let create = &group.creates[group.index];
        if !create.observed {
            if terminal {
                let sl = format!("{id}__stop_loss");
                let tp = format!("{id}__take_profit");
                cx.store.remove_orders(&[&id, &sl, &tp]);
            } else if let Some(rows) = rows {
                for row in rows {
                    cx.store.upsert_order(row);
                }
            } else if !reconciled
                && create.optimistic.kind != OrderType::Market
                && cx.store.order(&id).is_none()
            {
                let mut row = create.optimistic.clone();
                row.id = id.clone();
                row.last_update = cx.now;
                let levels = [
                    ("__stop_loss", OrderType::StopLoss, row.stop_loss),
                    ("__take_profit", OrderType::TakeProfit, row.take_profit),
                ];
                row.stop_loss = None;
                row.take_profit = None;
                for (suffix, kind, level) in levels {
                    if let Some(level) = level {
                        cx.store.upsert_order(Order {
                            id: format!("{id}{suffix}"),
                            parent_id: Some(id.clone()),
                            kind,
                            side: row.side.opposite(),
                            price: level,
                            client_order_id: None,
                            ..row.clone()
                        });
                    }
                }
                cx.store.upsert_order(row);
            }
        }
        group.ids.push(id);
        group.index += 1;
        self.advance_hyro(cx, key);
    }

    pub(crate) fn on_hyro_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Result<Response<'_>, &str>,
    ) {
        match request {
            Request::Stop { key, boundary } => {
                if !self.trading.hyro.groups.contains_key(&key) {
                    return;
                }
                let status = response
                    .as_ref()
                    .ok()
                    .and_then(|r| serde_json::from_str::<Status>(r.body).ok());
                let not_modified = status.as_ref().is_some_and(|s| {
                    s.ret_code == Some(34040)
                        || s.ret_msg
                            .as_deref()
                            .is_some_and(|m| m.to_ascii_lowercase().contains("not modified"))
                });
                let success = not_modified
                    || response
                        .as_ref()
                        .ok()
                        .is_some_and(|r| venue_error(r).is_none());
                if success && boundary == self.hyro_boundary() {
                    self.trading
                        .hyro
                        .groups
                        .get_mut(&key)
                        .expect("present")
                        .index += 1;
                    self.advance_hyro(cx, key);
                } else {
                    let definitive = !success
                        && response.as_ref().ok().is_some_and(|r| {
                            r.status < 500
                                && !matches!(r.status, 408 | 409)
                                && status.as_ref().and_then(|s| s.ret_code).is_some_and(|c| {
                                    c != 0 && !matches!(c, 408 | 500..=599 | 10000 | 10016)
                                })
                        });
                    let group = self.trading.hyro.groups.get_mut(&key).expect("present");
                    group.creates[group.index].blocked = !definitive;
                    self.pause_hyro(cx, &key, if definitive { "position-stop request rejected" }
                        else { "position-stop acknowledgement is unknown; this same stop cannot safely be repeated until the account is reconciled" }, definitive);
                }
            }
            Request::Create { key, boundary } => {
                if !self.trading.hyro.groups.contains_key(&key) {
                    return;
                }
                let success = response
                    .as_ref()
                    .ok()
                    .filter(|r| venue_error(r).is_none())
                    .and_then(|r| serde_json::from_str::<Placed>(r.body).ok())
                    .and_then(|p| p.result.and_then(|r| r.order_id))
                    .filter(|id| !id.is_empty())
                    .map(|id| id.into_owned());
                if let Some(id) = success {
                    let group = self.trading.hyro.groups.get_mut(&key).expect("present");
                    let create = &mut group.creates[group.index];
                    if create.accepted.as_ref().is_some_and(|seen| *seen != id) {
                        create.uncertain = true;
                        self.pause_hyro(
                            cx,
                            &key,
                            "acknowledged identity differs from the private observation",
                            false,
                        );
                        return;
                    }
                    create.accepted = Some(id.clone());
                    if boundary == self.hyro_boundary() {
                        self.accept_hyro(cx, key, id, None, false, false);
                        return;
                    }
                } else if let Ok(response) = &response {
                    let meta = serde_json::from_str::<Status>(response.body).ok();
                    let code = meta.as_ref().and_then(|s| s.ret_code);
                    let message = meta
                        .as_ref()
                        .and_then(|s| s.ret_msg.as_deref())
                        .unwrap_or_default();
                    let lower = message.to_ascii_lowercase();
                    let duplicate = matches!(code, Some(10014 | 110072))
                        || lower.contains("duplicate")
                        || lower.contains("already exist");
                    let observed = self
                        .trading
                        .hyro
                        .groups
                        .get(&key)
                        .is_some_and(|g| g.creates[g.index].accepted.is_some());
                    let ambiguous = observed
                        || duplicate
                        || matches!(response.status, 408 | 409)
                        || response.status >= 500
                        || matches!(code, Some(408 | 500..=599 | 10000 | 10016));
                    if !ambiguous
                        && ((400..500).contains(&response.status)
                            || code.is_some_and(|code| code != 0))
                    {
                        self.pause_hyro(
                            cx,
                            &key,
                            if message.is_empty() {
                                "create request rejected"
                            } else {
                                message
                            },
                            true,
                        );
                        return;
                    }
                }
                let group = self.trading.hyro.groups.get_mut(&key).expect("present");
                group.creates[group.index].uncertain = true;
                self.advance_hyro(cx, key);
            }
            Request::Lookup {
                key,
                boundary,
                history,
                mut seen,
            } => {
                if !self.trading.hyro.groups.contains_key(&key) {
                    return;
                }
                let parsed = response
                    .as_ref()
                    .map_err(|e| (*e).to_owned())
                    .and_then(|response| {
                        if let Some((_, message)) = venue_error(response) {
                            return Err(message);
                        }
                        serde_json::from_str::<Page>(response.body).map_err(|e| e.to_string())
                    });
                let page = match parsed {
                    Ok(page) if page.ret_code == Some(0) => page,
                    Ok(_) => {
                        self.pause_hyro(cx, &key, "reconciliation has no successful status", false);
                        return;
                    }
                    Err(error) => {
                        self.pause_hyro(cx, &key, &error, false);
                        return;
                    }
                };
                let group = self.trading.hyro.groups.get_mut(&key).expect("present");
                let create = &mut group.creates[group.index];
                let found = page
                    .result
                    .list
                    .iter()
                    .find(|row| present(&row.order_link_id) == Some(create.link.as_str()));
                if let Some(row) = found {
                    if row.symbol != create.symbol || !valid_hyro_order(row) {
                        self.pause_hyro(
                            cx,
                            &key,
                            "matched order has no valid identity or status",
                            false,
                        );
                        return;
                    }
                    if create
                        .accepted
                        .as_ref()
                        .is_some_and(|id| id != row.order_id.as_ref())
                    {
                        self.pause_hyro(
                            cx,
                            &key,
                            "reconciled identity differs from acknowledged order",
                            false,
                        );
                        return;
                    }
                    if row.order_status != "Rejected" {
                        create.accepted = Some(row.order_id.to_string());
                    }
                }
                if boundary != self.hyro_boundary() {
                    self.pause_hyro(
                        cx,
                        &key,
                        "private session or authoritative snapshot changed during reconciliation",
                        false,
                    );
                    return;
                }
                if let Some(row) = found {
                    if row.order_status == "Rejected" {
                        self.pause_hyro(
                            cx,
                            &key,
                            present(&row.reject_reason).unwrap_or("reconciled rejection"),
                            true,
                        );
                        return;
                    }
                    let terminal = matches!(
                        row.order_status.as_ref(),
                        "Filled" | "Cancelled" | "Deactivated" | "PartiallyFilledCanceled"
                    );
                    let Some(mapped) = map_order(row, cx.store, cx.now) else {
                        self.pause_hyro(cx, &key, "matched order cannot be mapped", false);
                        return;
                    };
                    if mapped.orders.iter().any(|o| {
                        [o.price, o.amount, o.filled, o.remaining]
                            .iter()
                            .any(|n| !n.is_finite() || *n < 0.0)
                    }) {
                        self.pause_hyro(
                            cx,
                            &key,
                            "matched order has invalid numeric fields",
                            false,
                        );
                        return;
                    }
                    let rows = (!terminal).then_some(mapped.orders);
                    self.accept_hyro(cx, key, row.order_id.to_string(), rows, terminal, true);
                    return;
                }
                if let Some(cursor) = page.result.next_page_cursor.filter(|c| !c.is_empty()) {
                    if !seen.insert(cursor.clone()) || seen.len() >= MAX_LOOKUP_PAGES {
                        self.pause_hyro(cx, &key, "reconciliation cursor did not finish", false);
                        return;
                    }
                    self.lookup_hyro(cx, key, history, cursor, seen, boundary);
                } else if !history {
                    self.lookup_hyro(cx, key, true, String::new(), HashSet::new(), boundary);
                } else {
                    let group = self.trading.hyro.groups.get_mut(&key).expect("present");
                    let create = &mut group.creates[group.index];
                    if let Some(id) = create.accepted.clone() {
                        self.accept_hyro(cx, key, id, None, false, true);
                    } else if create.attempts >= 2 {
                        self.pause_hyro(
                            cx,
                            &key,
                            "submission could not be confirmed after complete reconciliation",
                            false,
                        );
                    } else {
                        create.uncertain = false;
                        self.advance_hyro(cx, key);
                    }
                }
            }
        }
    }
}
