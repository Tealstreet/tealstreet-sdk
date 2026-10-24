//! Hyro's retained placement intents. Each lot keeps its link across lost
//! acknowledgements and caller retries and is created at most twice (R1(a));
//! only a fresh complete absence on both order axes permits the second create.
use crate::account::{map_order, valid_hyro_order};
use crate::sign::{Body, Query};
use crate::trading::acknowledgement::{self as ack, Answer};
use crate::trading::ordinary::Ordinary;
use crate::wire::{OrderRow, present};
use crate::{BybitLinear, Pending, Rest, venue_error};
use safe_cex_engine::model::{Order, OrderIdentityKind, OrderIdentityReceipt, PlaceOrderOpts};
use safe_cex_engine::{CommandId, Cx, Response, VenueError};
use serde::Deserialize;
use std::cell::RefCell;
use std::collections::HashMap;

type Boundary = (u32, u32);
/// Same-link creates one lot may ever send.
pub(crate) const MAX_SENDS: u8 = 2;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) intents: HashMap<String, Intent>,
    /// Late create ACKs (link, order id) left by the immutable late codec.
    late: RefCell<Vec<(String, String)>>,
}

/// One caller intent (its canonical orders), retained across calls.
pub(crate) struct Intent {
    /// The execution key identical callers join on.
    pub(crate) digest: String,
    pub(crate) lots: Vec<Lot>,
    pub(crate) phase: Phase,
}

pub(crate) enum Phase {
    Idle,
    /// This command's pre-admission link reads are running.
    Reading(CommandId),
    /// The shared execution, parked between its callbacks.
    Live(Box<Ordinary>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LotState {
    Absent,
    Unknown,
    /// The native order ID; empty for a position stop.
    Accepted(String),
    Rejected,
}

pub(crate) struct Lot {
    pub(crate) link: String,
    pub(crate) symbol: String,
    /// The create body without its link; `None` for a position stop.
    base: Option<Body>,
    pub(crate) stop: Option<String>,
    pub(crate) sends: u8,
    pub(crate) state: LotState,
    /// Sent and not yet resolved: reconcile it once its call seals.
    pub(crate) reconcile: bool,
    /// The private stream reported this link.
    observed: bool,
}

impl Lot {
    pub(crate) fn body(&self) -> Option<String> {
        self.base
            .clone()
            .map(|base| base.str("orderLinkId", &self.link).finish())
    }

    /// A create or stop that may have reached the venue without a resolution.
    pub(crate) fn maybe_sent(&self) -> bool {
        self.sends > 0 && matches!(self.state, LotState::Absent | LotState::Unknown)
    }

    pub(crate) fn receipt(&self) -> Option<OrderIdentityReceipt> {
        self.base.as_ref().map(|_| OrderIdentityReceipt {
            symbol: self.symbol.clone(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: self.link.clone(),
        })
    }

    fn dispatched(&self) -> bool {
        self.sends > 0 || matches!(self.state, LotState::Accepted(_))
    }
}

impl Intent {
    pub(crate) fn maybe_sent(&self) -> bool {
        self.lots.iter().any(Lot::maybe_sent)
    }

    pub(crate) fn ids(&self) -> String {
        let ids: Vec<&str> = self
            .lots
            .iter()
            .filter_map(|lot| match &lot.state {
                LotState::Accepted(id) if !id.is_empty() => Some(id.as_str()),
                _ => None,
            })
            .collect();
        serde_json::to_string(&ids).unwrap_or_default()
    }

    /// The identities already dispatched, replayed to a joining or retrying caller.
    pub(crate) fn receipts(&self) -> Vec<OrderIdentityReceipt> {
        self.lots
            .iter()
            .filter(|lot| lot.dispatched())
            .filter_map(Lot::receipt)
            .collect()
    }
}

/// A link-filtered reconcile read, outside any command.
pub(crate) struct Lookup {
    key: String,
    link: String,
    history: bool,
    boundary: Boundary,
}

/// What one link-filtered order read proves.
pub(crate) enum Found {
    Absent,
    Accepted {
        id: String,
        /// Mapped rows of an open order; `None` when terminal.
        rows: Option<Vec<Order>>,
    },
    Rejected,
    Incomplete(&'static str),
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

pub(crate) fn unknown(link: &str, cause: &str) -> VenueError {
    VenueError::new(format!(
        "HyroTrader order outcome is unknown (orderLinkId {link}). Reconcile before placing a replacement: {cause}"
    ))
    .outcome(safe_cex_engine::Outcome::Unknown)
}

pub(crate) fn link_query(link: &str) -> Query {
    Query::new()
        .str("category", "linear")
        .str("orderLinkId", link)
}

/// A definitively refused lot is placed under a new identity next time.
pub(crate) fn remint(cx: &Cx<'_, Pending>, lot: &mut Lot) -> Result<(), String> {
    if lot.state != LotState::Rejected {
        return Ok(());
    }
    lot.link = safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?;
    lot.sends = 0;
    lot.state = LotState::Absent;
    lot.reconcile = false;
    lot.observed = false;
    Ok(())
}

impl BybitLinear {
    pub(crate) fn hyro_boundary(&self) -> Boundary {
        (
            self.account.session_version,
            self.account.orders_snapshot_version,
        )
    }

    pub(crate) fn observe_hyro_order(&mut self, row: &OrderRow<'_>) {
        let Some(link) = present(&row.order_link_id) else {
            return;
        };
        for lot in self
            .trading
            .hyro
            .intents
            .values_mut()
            .flat_map(|intent| &mut intent.lots)
            .filter(|lot| lot.link == link)
        {
            lot.observed = true;
            if !row.order_id.is_empty()
                && row.order_status != "Rejected"
                && !matches!(lot.state, LotState::Accepted(_))
            {
                lot.state = LotState::Accepted(row.order_id.to_string());
                lot.reconcile = false;
            }
        }
    }

    /// The late codec has only `&self`; it leaves the ACK for the ledger.
    pub(crate) fn hyro_late_ack(&self, link: &str, response: &Response<'_>) {
        if !self.variant.is_hyro() || link.is_empty() {
            return;
        }
        if let Ok(Answer::Success { result, .. }) = ack::answer(response.body)
            && let Ok(id) = ack::created(result, Some(link))
        {
            self.trading
                .hyro
                .late
                .borrow_mut()
                .push((link.to_owned(), id));
        }
    }

    pub(crate) fn drain_hyro_late(&mut self) {
        let late = std::mem::take(&mut *self.trading.hyro.late.borrow_mut());
        for (link, id) in late {
            for lot in self
                .trading
                .hyro
                .intents
                .values_mut()
                .flat_map(|intent| &mut intent.lots)
                .filter(|lot| lot.link == link && !matches!(lot.state, LotState::Accepted(_)))
            {
                lot.state = LotState::Accepted(id.clone());
                lot.reconcile = false;
            }
        }
    }

    /// The caller-side checks the legacy gate made before any lot exists.
    pub(crate) fn hyro_intent_key(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<String, String> {
        self.check_ids(cx, orders)?;
        for opts in orders {
            if !opts.amount.is_finite()
                || [
                    opts.price,
                    opts.trigger_price,
                    opts.stop_loss,
                    opts.take_profit,
                    opts.max_slippage_pct,
                    opts.bybit_slippage_tolerance_ticks,
                    opts.bybit_bbo.as_ref().map(|bbo| bbo.level),
                ]
                .into_iter()
                .flatten()
                .any(|n| !n.is_finite())
            {
                return Err("Order numbers must be finite".into());
            }
            let market = cx
                .store
                .market(&opts.symbol)
                .ok_or_else(|| format!("Market {} not found", opts.symbol))?;
            if market.quote != "USDT" {
                return Err("HyroTrader supports USDT perpetuals only".into());
            }
        }
        serde_json::to_string(orders).map_err(|e| e.to_string())
    }

    /// Plan the intent's lots once; retries keep the original lots and trigger
    /// direction even when prices, precision or the position have changed.
    pub(crate) fn plan_hyro_lots(
        &self,
        cx: &Cx<'_, Pending>,
        orders: &[PlaceOrderOpts],
    ) -> Result<Vec<Lot>, String> {
        let mut lots = Vec::new();
        for opts in orders {
            if let Some(stop) = self.hyro_stop(cx, opts)? {
                lots.push((None, opts.symbol.clone(), Some(stop)));
                continue;
            }
            for lot in self.format_lots(cx, opts)? {
                lots.push((Some(lot.single), opts.symbol.clone(), None));
            }
        }
        lots.into_iter()
            .map(|(base, symbol, stop)| {
                Ok(Lot {
                    link: safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))?,
                    symbol,
                    base,
                    stop,
                    sends: 0,
                    state: LotState::Absent,
                    reconcile: false,
                    observed: false,
                })
            })
            .collect()
    }

    /// After a call: a success retires the intent; one that placed nothing
    /// that could exist is forgotten, so the next call plans afresh.
    pub(crate) fn settle_hyro(&mut self, key: &str, success: bool) {
        let Some(intent) = self.trading.hyro.intents.get(key) else {
            return;
        };
        if !matches!(intent.phase, Phase::Idle) {
            return;
        }
        let placed = intent
            .lots
            .iter()
            .any(|lot| lot.maybe_sent() || matches!(lot.state, LotState::Accepted(_)));
        if success || !placed {
            self.trading.hyro.intents.remove(key);
        }
    }

    /// One link read per lost ACK, issued once its call has sealed.
    pub(crate) fn reconcile_hyro_links(&mut self, cx: &mut Cx<'_, Pending>) {
        self.drain_hyro_late();
        self.retire_sealed_hyro(cx);
        let boundary = self.hyro_boundary();
        let mut reads = Vec::new();
        for (key, intent) in &mut self.trading.hyro.intents {
            if !matches!(intent.phase, Phase::Idle) {
                continue;
            }
            for lot in &mut intent.lots {
                if lot.reconcile && lot.base.is_some() && lot.maybe_sent() {
                    lot.reconcile = false;
                    reads.push(Lookup {
                        key: key.clone(),
                        link: lot.link.clone(),
                        history: false,
                        boundary,
                    });
                }
            }
        }
        for read in reads {
            self.send_hyro_lookup(cx, read);
        }
    }

    fn send_hyro_lookup(&mut self, cx: &mut Cx<'_, Pending>, read: Lookup) {
        let path = if read.history {
            "/v5/order/history"
        } else {
            "/v5/order/realtime"
        };
        let rest = Rest::get(path, link_query(&read.link)).signed();
        self.hyro_read(cx, rest, read);
    }

    /// Classify one link-filtered order read for this lot.
    pub(crate) fn hyro_found(
        &self,
        cx: &Cx<'_, Pending>,
        symbol: &str,
        link: &str,
        response: Result<&Response<'_>, &str>,
    ) -> Found {
        let Ok(response) = response else {
            return Found::Incomplete("reconciliation read failed");
        };
        if venue_error(response).is_some() {
            return Found::Incomplete("reconciliation has no successful status");
        }
        let Ok(page) = serde_json::from_str::<Page>(response.body) else {
            return Found::Incomplete("reconciliation response is malformed");
        };
        if page.ret_code != Some(0) {
            return Found::Incomplete("reconciliation has no successful status");
        }
        if page.result.list.is_empty() {
            // A link filter names at most one order; another page proves nothing.
            return if page.result.next_page_cursor.is_none_or(|c| c.is_empty()) {
                Found::Absent
            } else {
                Found::Incomplete("reconciliation read did not finish")
            };
        }
        let Some(row) = page
            .result
            .list
            .iter()
            .find(|row| present(&row.order_link_id) == Some(link))
        else {
            return Found::Incomplete("reconciliation returned another order");
        };
        if row.symbol != symbol || !valid_hyro_order(row) {
            return Found::Incomplete("matched order has no valid identity or status");
        }
        if row.order_status == "Rejected" {
            return Found::Rejected;
        }
        let Some(mapped) = map_order(row, cx.store, cx.now) else {
            return Found::Incomplete("matched order cannot be mapped");
        };
        if mapped.orders.iter().any(|o| {
            [o.price, o.amount, o.filled, o.remaining]
                .iter()
                .any(|n| !n.is_finite() || *n < 0.0)
        }) {
            return Found::Incomplete("matched order has invalid numeric fields");
        }
        let terminal = matches!(
            row.order_status.as_ref(),
            "Filled" | "Cancelled" | "Deactivated" | "PartiallyFilledCanceled"
        );
        Found::Accepted {
            id: row.order_id.to_string(),
            rows: (!terminal).then_some(mapped.orders),
        }
    }

    /// Record what a link read proved. A found order is published only when
    /// no snapshot or session moved since the read was issued.
    pub(crate) fn apply_hyro_found(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        key: &str,
        link: &str,
        found: &Found,
        boundary: Boundary,
    ) {
        let current = self.hyro_boundary();
        let Some(lot) = self
            .trading
            .hyro
            .intents
            .get_mut(key)
            .and_then(|intent| intent.lots.iter_mut().find(|lot| lot.link == link))
        else {
            return;
        };
        match found {
            Found::Accepted { id, rows } => {
                if !matches!(lot.state, LotState::Accepted(_)) {
                    lot.state = LotState::Accepted(id.clone());
                }
                lot.reconcile = false;
                if lot.observed || boundary != current {
                    return;
                }
                match rows {
                    Some(rows) => {
                        for row in rows {
                            cx.store.upsert_order(row.clone());
                        }
                    }
                    None => {
                        let sl = format!("{id}__stop_loss");
                        let tp = format!("{id}__take_profit");
                        cx.store.remove_orders(&[id, &sl, &tp]);
                    }
                }
            }
            Found::Rejected if !matches!(lot.state, LotState::Accepted(_)) => {
                lot.state = LotState::Rejected;
                lot.reconcile = false;
            }
            Found::Absent if lot.state == LotState::Unknown => lot.state = LotState::Absent,
            _ => {}
        }
    }

    pub(crate) fn on_hyro_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        read: Lookup,
        response: Result<Response<'_>, &str>,
    ) {
        let Some(symbol) = self
            .trading
            .hyro
            .intents
            .get(&read.key)
            .and_then(|intent| intent.lots.iter().find(|lot| lot.link == read.link))
            .map(|lot| lot.symbol.clone())
        else {
            return;
        };
        let found = self.hyro_found(cx, &symbol, &read.link, response.as_ref().map_err(|e| *e));
        self.drain_hyro_late();
        self.apply_hyro_found(cx, &read.key, &read.link, &found, read.boundary);
        if matches!(found, Found::Absent) && !read.history {
            self.send_hyro_lookup(
                cx,
                Lookup {
                    history: true,
                    ..read
                },
            );
        }
    }
}
