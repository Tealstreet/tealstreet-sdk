//! Perp-TS formatting/continuation policy on native Spot wire routes.
use super::*;
use safe_cex_engine::{
    Outcome,
    classify::{CodeTable, Intent, Key, Native, NativeCode, Row, Verdict, WriteFact},
    request_context::{
        Plan, PreflightPolicy, PreflightPurpose,
        plan::{CallAuth, SerialCall, SerialPolicy, Slot},
    },
    write_plan::{AppliedEffect, Element, ElementResult, WritePlan},
};
const UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
const ORDER: &str = "/openApi/spot/v1/trade/order";
const CANCEL: &str = "/openApi/spot/v1/trade/cancel";
const CANCEL_ALL: &str = "/openApi/spot/v1/trade/cancelOpenOrders";
const SERIAL: SerialCall = SerialCall {
    auth: CallAuth::Preflight,
    operation: Operation::Mutation,
    timeout_ms: 20_000,
    attempts: 1,
    pacing: None,
};
// Official Spot Error Code Reference; busy, rate limits and duplicate IDs stay Unknown.
const TABLE: CodeTable = CodeTable {
    rows: &[
        Row {
            key: Key::Int(100001),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(100004),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(100413),
            verdict: Verdict::Refused(ErrorKind::AuthInvalid),
            only: None,
        },
        Row {
            key: Key::Int(100419),
            verdict: Verdict::Refused(ErrorKind::AuthPermission),
            only: None,
        },
        Row {
            key: Key::Int(100421),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: None,
        },
        Row {
            key: Key::Int(100202),
            verdict: Verdict::Refused(ErrorKind::InsufficientBalance),
            only: Some(Intent::Place),
        },
        Row {
            key: Key::Int(100440),
            verdict: Verdict::Refused(ErrorKind::InvalidInput),
            only: Some(Intent::Place),
        },
        Row {
            key: Key::Int(100490),
            verdict: Verdict::Refused(ErrorKind::Rejected),
            only: Some(Intent::Place),
        },
    ],
    success: &[Key::Int(0)],
    unlisted: Verdict::Unknown(None),
    status: &[],
};
struct Step {
    params: Params,
    native: String,
    kind: Call,
    receipt: Option<OrderIdentityReceipt>,
}
enum Call {
    Place { side: OrderSide, kind: OrderType },
    Cancel { id: String },
    CancelAll,
}
pub struct Request {
    pub(super) command: CommandId,
    index: usize,
    plan: Plan,
    slot: Slot,
    context: Context,
    step: Step,
}
pub(super) struct Run {
    pub(super) plan: Plan,
    next: Option<Slot>,
    steps: VecDeque<(usize, Step)>,
    ids: Vec<String>,
    error: Option<VenueError>,
    journal: WritePlan,
    total: usize,
}
impl Run {
    fn attach(&self, error: VenueError, index: usize) -> VenueError {
        let mut error = self.journal.attach(error);
        error.partial = Some(Box::new(safe_cex_engine::Partial {
            order_ids: self.ids.clone(),
            total: Some(self.total),
            failed_index: Some(index),
        }));
        error
    }
}
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SingleAck {
    order_id: Id,
    symbol: String,
    status: String,
    side: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
}
#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Ack {
    Single(SingleAck),
    Many { orders: Vec<SingleAck> },
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct AckClaims {
    #[serde(rename = "orderId")]
    order_id: Option<serde::de::IgnoredAny>,
    order: Option<serde::de::IgnoredAny>,
    orders: Option<Vec<serde::de::IgnoredAny>>,
}
fn has_effect_claim(data: &serde_json::value::RawValue) -> bool {
    if let Ok(rows) = serde_json::from_str::<Vec<serde::de::IgnoredAny>>(data.get()) {
        return !rows.is_empty();
    }
    // Unrecognized fields may contain execution evidence without a native ID.
    // Only demonstrably empty data preserves a definite refusal.
    serde_json::from_str::<AckClaims>(data.get()).map_or(true, |claims| {
        claims.order_id.is_some()
            || claims.order.is_some()
            || claims.orders.is_some_and(|rows| !rows.is_empty())
    })
}
fn proven_ids(call: &Call, native: &str, ack: &Ack) -> Option<Vec<String>> {
    let same = |a: &SingleAck| a.order_id.valid() && a.symbol == native;
    let canceled = |a: &SingleAck| same(a) && matches!(a.status.as_str(), "CANCELED" | "CANCELLED");
    match (call, ack) {
        (Call::Place { side, kind }, Ack::Single(a))
            if same(a)
                && matches!(
                    a.status.as_str(),
                    "NEW" | "PENDING" | "PARTIALLY_FILLED" | "FILLED"
                )
                && a.side.as_deref()
                    == Some(if *side == OrderSide::Buy {
                        "BUY"
                    } else {
                        "SELL"
                    })
                && a.kind.as_deref()
                    == Some(if *kind == OrderType::Limit {
                        "LIMIT"
                    } else {
                        "MARKET"
                    }) =>
        {
            Some(vec![a.order_id.text()])
        }
        (Call::Cancel { id }, Ack::Single(a)) if canceled(a) && a.order_id.text() == *id => {
            Some(vec![id.clone()])
        }
        (Call::CancelAll, Ack::Many { orders }) if orders.iter().all(canceled) => {
            let mut ids = HashSet::new();
            orders
                .iter()
                .all(|a| ids.insert(a.order_id.text()))
                .then(|| orders.iter().map(|a| a.order_id.text()).collect())
        }
        _ => None,
    }
}
fn not_sent(msg: impl Into<String>) -> VenueError {
    invalid(msg)
        .outcome(Outcome::NotSent)
        .kind(ErrorKind::InvalidInput)
}
impl BingxSpot {
    fn prepare_place(
        &self,
        cx: &Cx<'_, Pending>,
        o: &PlaceOrderOpts,
    ) -> Result<Vec<Step>, VenueError> {
        if !matches!(o.kind, OrderType::Limit | OrderType::Market)
            || o.stop_loss.is_some()
            || o.take_profit.is_some()
            || o.trigger_price.is_some()
            || o.trigger_type.is_some()
            || o.tp_trigger_type.is_some()
            || o.sl_trigger_type.is_some()
            || o.reduce_only == Some(true)
            || o.close == Some(true)
            || o.stp_mode.is_some()
            || o.bybit_bbo.is_some()
            || o.bybit_slippage_tolerance_ticks.is_some()
            || o.max_slippage_pct.is_some()
            || o.implied_volatility.is_some()
        {
            return Err(not_sent(
                "BingX Spot supports only ordinary limit/market orders",
            ));
        }
        let m = cx
            .store
            .market(&o.symbol)
            .filter(|m| m.active && m.is_spot == Some(true) && m.is_read_only != Some(true))
            .ok_or_else(|| not_sent("Active BingX Spot market required"))?;
        let rules = self
            .rules
            .get(&o.symbol)
            .ok_or_else(|| not_sent("Native BingX Spot trading rules required"))?;
        if match o.side {
            OrderSide::Buy => rules.api_state_buy == Some(false),
            OrderSide::Sell => rules.api_state_sell == Some(false),
        } {
            return Err(not_sent("BingX Spot market disables this side"));
        }
        if !o.amount.is_finite() || o.price.is_some_and(|price| !price.is_finite()) {
            return Err(not_sent("Spot amount and supplied price must be finite"));
        }
        let amount = safe_cex_engine::num::adjust(o.amount, m.precision.amount);
        let price = match (o.kind, o.side) {
            (OrderType::Limit, _) => o
                .price
                .map(|price| safe_cex_engine::num::adjust(price, m.precision.price)),
            (OrderType::Market, OrderSide::Buy) => Some(o.price.ok_or_else(|| {
                not_sent("Spot market buy requires a price for quote conversion")
            })?),
            _ => None,
        };
        let mut quantities = vec![amount];
        if amount > m.limits.amount.max {
            let lots = (amount / m.limits.amount.max).ceil();
            let count = lots
                .to_string()
                .parse::<usize>()
                .map_err(|_| not_sent("Spot lot count exceeds addressable storage"))?;
            let rest =
                safe_cex_engine::num::adjust(amount % m.limits.amount.max, m.precision.amount);
            let lot = safe_cex_engine::num::adjust((amount - rest) / lots, m.precision.amount);
            quantities = vec![lot; count];
            if rest != 0. {
                quantities.push(rest);
            }
        }
        if o.client_order_id.is_some() && quantities.len() != 1 {
            return Err(not_sent(
                "Spot caller clientOrderId requires one native order",
            ));
        }
        let mut steps = Vec::new();
        for quantity in quantities {
            let client = if let Some(id) = &o.client_order_id {
                if !(1..=40).contains(&id.len())
                    || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(not_sent(
                        "BingX Spot clientOrderId requires 1-40 letters, digits or '_'",
                    ));
                }
                id.clone()
            } else {
                safe_cex_crypto::uuid::uuid_with_entropy(|bytes| cx.fill_entropy(bytes))
                    .map_err(not_sent)?
                    .replace('-', "")
            };
            let mut params = Params::new()
                .str("symbol", &m.id)
                .str(
                    "side",
                    if o.side == OrderSide::Buy {
                        "BUY"
                    } else {
                        "SELL"
                    },
                )
                .str(
                    "type",
                    if o.kind == OrderType::Limit {
                        "LIMIT"
                    } else {
                        "MARKET"
                    },
                )
                .str("newClientOrderId", &client);
            if o.kind == OrderType::Market && o.side == OrderSide::Buy {
                let reference = price
                    .ok_or_else(|| not_sent("Spot market buy conversion reference missing"))?;
                let cost = quantity * reference;
                if !cost.is_finite() {
                    return Err(not_sent("Spot quote conversion overflow"));
                }
                params = params.num("quoteOrderQty", cost);
            } else {
                params = params.num("quantity", quantity);
            }
            if o.kind == OrderType::Limit {
                if let Some(price) = price {
                    params = params.num("price", price);
                }
                params = params.str(
                    "timeInForce",
                    match o.time_in_force {
                        Some(TimeInForce::ImmediateOrCancel) => "IOC",
                        Some(TimeInForce::FillOrKill) => "FOK",
                        Some(TimeInForce::PostOnly) => "PostOnly",
                        _ => "GTC",
                    },
                );
            } else if o
                .time_in_force
                .is_some_and(|t| t != TimeInForce::GoodTillCancel)
            {
                return Err(not_sent("Spot market order cannot honor this timeInForce"));
            }
            steps.push(Step {
                params,
                native: m.id.clone(),
                kind: Call::Place {
                    side: o.side,
                    kind: o.kind,
                },
                receipt: Some(OrderIdentityReceipt {
                    symbol: o.symbol.clone(),
                    kind: OrderIdentityKind::ClientOrderId,
                    identity: client,
                }),
            });
        }
        Ok(steps)
    }
    fn prepare_cancel(
        &self,
        cx: &Cx<'_, Pending>,
        id: &str,
        symbol: &str,
    ) -> Result<Step, VenueError> {
        if id.is_empty() || id == "0" || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err(not_sent(
                "Spot cancellation requires a native numeric order ID",
            ));
        }
        let m = cx
            .store
            .market(symbol)
            .ok_or_else(|| not_sent("Spot cancel market missing"))?;
        if cx.store.order(id).is_some_and(|o| o.symbol != symbol) {
            return Err(not_sent("Spot cancel symbol contradicts stored order"));
        }
        Ok(Step {
            params: Params::new().str("symbol", &m.id).str("orderId", id),
            native: m.id.clone(),
            kind: Call::Cancel { id: id.into() },
            receipt: None,
        })
    }
    pub(super) fn write_command(&mut self, cx: &mut Cx<'_, Pending>, command: Command) {
        let Some(identity) = self.credentials().cloned() else {
            self.local_failure(
                cx,
                "BingX Spot credentials required",
                Operation::Mutation,
                ErrorKind::AuthInvalid,
            );
            return;
        };
        let credential = Credential {
            slot: safe_cex_engine::CredentialSlot::Primary,
            reference: identity.credential,
        };
        let Ok(owner) = cx.begin_preflight(PreflightPolicy {
            purpose: PreflightPurpose::WriteAfterReads,
            read_credential: None,
            write_credential: Some(credential.clone()),
        }) else {
            return;
        };
        let result = (|| -> Result<(CommandId, VecDeque<Step>), VenueError> {
            match command {
                Command::PlaceOrders { command, orders } => {
                    let mut ids = HashSet::new();
                    let mut steps = VecDeque::new();
                    for o in orders {
                        if let Some(id) = &o.client_order_id
                            && !ids.insert(id.clone())
                        {
                            return Err(not_sent("Duplicate Spot caller clientOrderId"));
                        }
                        steps.extend(self.prepare_place(cx, &o)?);
                    }
                    Ok((command, steps))
                }
                Command::CancelOrders { command, ids } => {
                    let mut steps = VecDeque::new();
                    for id in ids {
                        let o = cx.store.order(&id).ok_or_else(|| {
                            not_sent(
                                "Spot cancel requires stored symbol or CancelOrdersWithSymbols",
                            )
                        })?;
                        steps.push_back(self.prepare_cancel(cx, &id, &o.symbol)?);
                    }
                    Ok((command, steps))
                }
                Command::CancelOrdersWithSymbols { command, orders } => {
                    let steps = orders
                        .iter()
                        .map(|o| self.prepare_cancel(cx, &o.id, &o.symbol))
                        .collect::<Result<VecDeque<_>, _>>()?;
                    Ok((command, steps))
                }
                Command::CancelSymbolOrders { command, symbol } => {
                    let market = cx
                        .store
                        .market(&symbol)
                        .ok_or_else(|| not_sent("Spot cancel requires a valid symbol"))?;
                    Ok((
                        command,
                        VecDeque::from([Step {
                            params: Params::new().str("symbol", &market.id),
                            native: market.id.clone(),
                            kind: Call::CancelAll,
                            receipt: None,
                        }]),
                    ))
                }
                _ => Err(not_sent("BingX Spot command unsupported").kind(ErrorKind::Unsupported)),
            }
        })();
        let (command, steps) = match result {
            Ok(v) => v,
            Err(e) => {
                let _ = cx.fail_preflight(&owner, e);
                return;
            }
        };
        if steps.is_empty() {
            let _ = cx.done_empty_plan(&owner, "[]".into());
            return;
        }
        match cx.admit_serial_plan(
            &owner,
            SERIAL,
            SerialPolicy {
                credential: Some(credential),
                pacing: None,
                refusals_complete: false,
            },
        ) {
            Ok((plan, slot)) => {
                let total = steps.len();
                let journal = WritePlan::new(
                    steps
                        .iter()
                        .enumerate()
                        .map(|(index, step)| Element {
                            index,
                            client_order_id: step.receipt.as_ref().map(|r| r.identity.clone()),
                        })
                        .collect(),
                );
                self.jobs.insert(
                    command,
                    Run {
                        plan,
                        next: Some(slot),
                        steps: steps.into_iter().enumerate().collect(),
                        ids: vec![],
                        error: None,
                        journal,
                        total,
                    },
                );
                self.advance(cx, command);
            }
            Err(e) => {
                let _ = cx.fail_preflight(&owner, e.venue_error());
            }
        }
    }
    pub(super) fn advance(&mut self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let Some(run) = self.jobs.get_mut(&command) else {
            return;
        };
        let Some((index, step)) = run.steps.pop_front() else {
            let plan = run.plan.clone();
            if let Some(mut error) = run.error.take() {
                error.presents = true;
                error = run.journal.attach(error);
                let _ = cx.fail_plan(&plan, error);
                return;
            }
            let Ok(value) = serde_json::to_string(&run.ids) else {
                let _ = cx.fail_plan(&plan, not_sent("Unable to encode Spot order IDs"));
                return;
            };
            let _ = cx.done_plan(&plan, value);
            return;
        };
        let Some(slot) = run.next.take() else {
            run.steps.push_front((index, step));
            return;
        };
        let plan = run.plan.clone();
        let intent = if matches!(step.kind, Call::Place { .. }) {
            Intent::Place
        } else {
            Intent::Cancel
        };
        let context = match cx.plan_leg_context_with_intent(&plan, &slot, intent) {
            Ok(v) => v,
            Err(e) => {
                let _ = cx.fail_plan(&plan, e.venue_error());
                return;
            }
        };
        let _ = run.journal.record(
            index,
            ElementResult::Unknown {
                error: Box::new(VenueError::new(UNKNOWN_TEXT).outcome(Outcome::Unknown)),
            },
        );
        let _ = cx.present_plan_failure(&plan, UNKNOWN_TEXT);
        if let Some(r) = &step.receipt {
            cx.command_progress(command, vec![r.clone()]);
        }
        let Some(identity) = self.credentials().cloned() else {
            let _ = cx.fail_plan(&plan, not_sent("Spot signing identity missing"));
            return;
        };
        let path = match step.kind {
            Call::Place { .. } => ORDER,
            Call::Cancel { .. } => CANCEL,
            Call::CancelAll => CANCEL_ALL,
        };
        let recipe = Http::spot(identity, Method::Post, path, step.params.clone());
        let (url, _, body) = recipe.unsigned();
        let request = Request {
            command,
            index,
            plan,
            slot,
            context: context.clone(),
            step,
        };
        if let Err((Pending::Write(r), error)) = cx.request_signed_managed(
            context,
            None,
            Method::Post,
            url,
            body,
            Box::new(recipe),
            Pending::Write(Box::new(request)),
        ) {
            self.write_failed(cx, *r, error.venue_error(), true);
        }
    }
    pub(super) fn write_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: Request,
        mut error: VenueError,
        report_unknown: bool,
    ) {
        if !matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
            error.message = UNKNOWN_TEXT.into();
            if report_unknown {
                // Venue decoding owns this diagnostic; engine-sealed failures use its hook.
                let operation = match r.step.kind {
                    Call::Place { .. } => "place",
                    Call::Cancel { .. } => "cancel",
                    Call::CancelAll => "cancelAll",
                };
                cx.event(
                    Level::Error,
                    "order.outcome_unknown",
                    UNKNOWN_TEXT,
                    vec![
                        ("command", r.command.into()),
                        ("outcome", "unknown".into()),
                        ("operation", operation.into()),
                    ],
                );
            }
        }
        error.presents = true;
        if let Some(run) = self.jobs.get_mut(&r.command) {
            let result = if matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
                ElementResult::NotSent {
                    error: Some(Box::new(error.clone())),
                }
            } else {
                ElementResult::Unknown {
                    error: Box::new(error.clone()),
                }
            };
            let _ = run.journal.record(r.index, result);
            error = run.attach(error, r.index);
        }
        let _ = cx.fail_plan(&r.plan, error);
    }
    fn finish_step(&mut self, cx: &mut Cx<'_, Pending>, r: &Request) -> bool {
        if let Some(run) = self.jobs.get_mut(&r.command) {
            let result = if run.steps.is_empty() {
                cx.finish_plan_call(&r.plan, &r.slot, &r.context)
            } else {
                cx.continue_serial_plan(&r.plan, &r.slot, &r.context, SERIAL)
                    .map(|slot| run.next = Some(slot))
            };
            if let Err(e) = result {
                let _ = cx.fail_plan(&r.plan, e.venue_error());
                return false;
            }
        }
        self.advance(cx, r.command);
        true
    }
    pub(super) fn write_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        r: Request,
        response: Response<'_>,
    ) {
        if cx.validate_plan_delivery(&r.plan, &r.context).is_err() {
            return;
        }
        let envelope =
            serde_json::from_str::<Envelope<Box<serde_json::value::RawValue>>>(response.body).ok();
        let ack = envelope
            .as_ref()
            .filter(|e| e.code == 0)
            .and_then(|e| e.data.as_ref())
            .and_then(|data| serde_json::from_str::<Ack>(data.get()).ok());
        let valid = ack
            .as_ref()
            .and_then(|ack| proven_ids(&r.step.kind, &r.step.native, ack));
        if let Some(proven) = valid.filter(|_| (200..300).contains(&response.status)) {
            let ids = if matches!(r.step.kind, Call::Place { .. }) {
                proven.clone()
            } else {
                vec![]
            };
            if let Err(e) = cx.http_write_fact(
                &r.context,
                WriteFact::Accepted {
                    order_ids: ids.clone(),
                },
                None,
            ) {
                let _ = cx.fail_plan(&r.plan, e.venue_error());
                return;
            }
            if !matches!(r.step.kind, Call::Place { .. }) {
                for id in proven {
                    cx.store.set_order_status(&id, OrderStatus::Canceled);
                    self.remember_terminal(id);
                }
            }
            if let Some(run) = self.jobs.get_mut(&r.command) {
                let result = match &r.step.kind {
                    Call::Place { .. } => ElementResult::Placed {
                        order_ids: ids.clone(),
                    },
                    kind => ElementResult::Applied {
                        effect: AppliedEffect::Cancel,
                        target_order_id: match kind {
                            Call::Cancel { id } => Some(id.clone()),
                            _ => None,
                        },
                        position_key: None,
                    },
                };
                let _ = run.journal.record(r.index, result);
                run.ids.extend(ids);
            }
            self.balance_revision = self.balance_revision.saturating_add(1);
            if !self.finish_step(cx, &r) {
                return;
            }
            self.orders(cx, true);
            self.balances(cx);
        } else {
            let native = envelope.as_ref().map(|e| Native {
                code: NativeCode::Int(e.code),
                message: &e.msg,
            });
            let mut fact = match cx.classify_write(
                &r.context,
                &TABLE,
                native.as_ref(),
                "BingX Spot write not proven; check Open Orders",
            ) {
                Ok(f) => f,
                Err(e) => {
                    let _ = cx.fail_plan(&r.plan, e.venue_error());
                    return;
                }
            };
            if envelope
                .as_ref()
                .and_then(|e| e.data.as_ref())
                .is_some_and(|data| has_effect_claim(data))
            {
                fact = fact.unproven();
            }
            let error = fact.error().clone();
            if let Err(e) = cx.http_write_fact(
                &r.context,
                WriteFact::Failed {
                    fact,
                    known_effects: false,
                },
                None,
            ) {
                let _ = cx.fail_plan(&r.plan, e.venue_error());
                return;
            }
            if matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
                if let Some(run) = self.jobs.get_mut(&r.command) {
                    let _ = run.journal.record(
                        r.index,
                        ElementResult::NotSent {
                            error: Some(Box::new(error.clone())),
                        },
                    );
                    run.error.get_or_insert(error);
                }
                self.finish_step(cx, &r);
            } else {
                self.write_failed(cx, r, error, true);
            }
        }
    }
}
