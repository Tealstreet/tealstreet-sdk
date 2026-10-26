//! Snapshot-issued cancel-then-place custody; full-size replacement needs zero-fill proof.
use crate::{Backpack, Pending, REST, account, sign, trading::Repair, wire::Number};
use safe_cex_engine::classify::{CodeTable, WriteFact};
use safe_cex_engine::log::code;
use safe_cex_engine::model::{Order, OrderStatus};
use safe_cex_engine::repair::{
    Binding, Effect, Entitlement, RepairNotice, RepairRefusal, RepairRoot, Retire, SnapshotReceipt,
    Subject,
};
use safe_cex_engine::request_context::{Context, Credential};
use safe_cex_engine::{CredentialSlot, Cx, Level, Method, Response, VenueError};
use serde::Deserialize;

const MANUAL: &str =
    "Backpack trigger repair could not prove a safe replacement; check the order manually";
const UNKNOWN_CODES: CodeTable = CodeTable {
    rows: &[],
    success: &[],
    unlisted: safe_cex_engine::classify::Verdict::Unknown(None),
    status: &[],
};
#[derive(Default)]
pub(crate) struct Repairs {
    grants: Vec<Grant>,
    active: Vec<Active>,
}
struct Grant {
    entitlement: Entitlement,
    subject: Subject,
    candidate: Option<Repair>,
    unknown: bool,
}
struct Active {
    root: RepairRoot,
    candidate: Repair,
    previous: OrderStatus,
    stream_time: f64,
    successor: Option<Context>,
}
#[derive(Clone, Copy)]
enum Stage {
    Cancel,
    Place,
}
pub struct Request {
    context: Context,
    root: RepairRoot,
    stage: Stage,
}
#[derive(Debug)]
pub(crate) enum LocalRefusal {
    NoPrimary,
    NotReady,
}
#[derive(Deserialize)]
struct FillProof {
    #[serde(rename = "executedQuantity")]
    fill: Option<Number>,
}
pub(crate) fn fill_proof(text: &str) -> Option<f64> {
    serde_json::from_str::<FillProof>(text)
        .ok()?
        .fill
        .map(|n| n.0)
        .filter(|n| n.is_finite() && *n >= 0.)
}
pub(crate) struct Observation {
    id: Option<String>,
    symbol: Option<String>,
    fill: Option<f64>,
}
impl Observation {
    pub(crate) fn capture(text: &str) -> Self {
        #[derive(Deserialize)]
        struct Identity {
            id: Option<String>,
            symbol: Option<String>,
        }
        let identity = serde_json::from_str::<Identity>(text).ok();
        Self {
            id: identity
                .as_ref()
                .and_then(|row| row.id.clone())
                .filter(|id| !id.is_empty()),
            symbol: identity.and_then(|row| row.symbol),
            fill: fill_proof(text),
        }
    }
    pub(crate) fn rejected(&mut self) {
        self.fill = None;
    }
}
impl Repair {
    fn observe(&mut self, fill: Option<f64>, terminal: bool) {
        match fill.filter(|n| n.is_finite() && *n >= 0.) {
            Some(n) => {
                if self.high_water.is_some_and(|old| n < old) {
                    self.uncertain = true;
                }
                self.high_water = Some(self.high_water.map_or(n, |old| old.max(n)));
            }
            None => self.uncertain = true,
        }
        self.terminal |= terminal;
    }
    fn observe_snapshot(&mut self, observations: &[Observation]) {
        for row in observations {
            if row.id.as_deref().is_none_or(|id| id == self.old_id) {
                self.observe(row.id.as_ref().and(row.fill), false);
                self.uncertain |= row.symbol.as_deref() != Some(self.order.symbol.as_str());
            }
        }
    }
    fn stable_zero(&self, current: Option<&Order>, accepted_cancel: bool) -> bool {
        !self.uncertain
            && !self.terminal
            && self.high_water == Some(0.)
            && current.is_some_and(|o| {
                o.filled == 0.
                    && o.amount == self.amount
                    && o.symbol == self.order.symbol
                    && o.status != OrderStatus::Closed
                    && (accepted_cancel || o.status != OrderStatus::Canceled)
            })
    }
}
impl Backpack {
    pub(crate) fn repair_primary(&self) -> Result<Credential, LocalRefusal> {
        if self.options.public_data_only || self.account.credentials.is_none() {
            return Err(LocalRefusal::NotReady);
        }
        self.options
            .identity
            .as_ref()
            .map(|identity| Credential {
                slot: CredentialSlot::Primary,
                reference: identity.credential.clone(),
            })
            .ok_or(LocalRefusal::NoPrimary)
    }
    pub(crate) fn repair_local_refusal(&self, cx: &mut Cx<'_, Pending>, reason: LocalRefusal) {
        match reason {
            LocalRefusal::NoPrimary => cx.notice(
                Level::Warn,
                "repair.unavailable",
                "Automatic repair authority unavailable: NoPrimary",
            ),
            LocalRefusal::NotReady => cx.notice(
                Level::Warn,
                code::ORDER_REJECTED,
                "Backpack automatic trigger repair awaits managed account signing",
            ),
        }
    }
    #[cfg(test)]
    pub(crate) fn repairs_is_empty(&self) -> bool {
        self.repairs.grants.is_empty() && self.repairs.active.is_empty()
    }
    pub(crate) fn repair_snapshot(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        receipt: &SnapshotReceipt,
        generation: u64,
        candidates: Vec<Repair>,
        observations: Vec<Observation>,
        complete: bool,
    ) {
        self.repairs.grants.retain(|grant| {
            let terminal = !grant.unknown
                && cx
                    .retire_repair_subject(&grant.entitlement, &grant.subject.key, Retire::Terminal)
                    .is_ok();
            let absent = complete
                && cx
                    .retire_repair_subject(
                        &grant.entitlement,
                        &grant.subject.key,
                        Retire::CompleteRead(receipt),
                    )
                    .is_ok();
            !terminal && !absent
        });
        for grant in &mut self.repairs.grants {
            if let Some(candidate) = &mut grant.candidate {
                candidate.observe_snapshot(&observations);
            }
        }
        for active in &mut self.repairs.active {
            active.candidate.observe_snapshot(&observations);
            let current = cx.store.order(&active.candidate.old_id);
            active.candidate.observe(
                current.map(|o| o.filled),
                current.is_some_and(|o| {
                    o.status == OrderStatus::Closed
                        || (o.status == OrderStatus::Canceled && active.successor.is_none())
                }),
            );
            if let Some(new) = candidates
                .iter()
                .find(|new| new.old_id == active.candidate.old_id)
            {
                active.candidate.observe(new.high_water, new.terminal);
                active.candidate.uncertain |= new.uncertain;
            }
        }
        self.retire_unsafe_replacements(cx);
        if candidates.is_empty() {
            return;
        }
        let primary = match self.repair_primary() {
            Ok(primary) => primary,
            Err(reason) => {
                self.repair_local_refusal(cx, reason);
                return;
            }
        };
        for mut candidate in candidates {
            candidate.observe_snapshot(&observations);
            let subject = Subject {
                key: candidate.old_id.clone(),
                generation,
            };
            match cx.mint_snapshot_repair(
                receipt,
                primary.clone(),
                subject.clone(),
                &candidate.order.symbol,
            ) {
                Ok(entitlement) => self.repairs.grants.push(Grant {
                    entitlement,
                    subject,
                    candidate: Some(candidate),
                    unknown: false,
                }),
                Err(
                    RepairRefusal::Duplicate
                    | RepairRefusal::Stale
                    | RepairRefusal::NotOwned
                    | RepairRefusal::StoreMissing
                    | RepairRefusal::NoPrimary
                    | RepairRefusal::Capacity
                    | RepairRefusal::NotData
                    | RepairRefusal::NotInEntry
                    | RepairRefusal::Spent
                    | RepairRefusal::Retired
                    | RepairRefusal::Incompatible
                    | RepairRefusal::Conflict
                    | RepairRefusal::TooMany
                    | RepairRefusal::Invalid,
                ) => {}
            }
        }
    }
    pub(crate) fn start_repairs(&mut self, cx: &mut Cx<'_, Pending>) {
        if !cx.now.is_finite() {
            return;
        }
        for i in 0..self.repairs.grants.len() {
            let grant = &mut self.repairs.grants[i];
            let Some(candidate) = &mut grant.candidate else {
                continue;
            };
            let Some(current) = cx.store.order(&candidate.old_id) else {
                candidate.uncertain = true;
                continue;
            };
            if matches!(current.status, OrderStatus::Closed | OrderStatus::Canceled) {
                continue;
            }
            candidate.observe(Some(current.filled), false);
            let previous = current.status;
            let binding = Binding {
                order_id: &candidate.old_id,
                symbol: &candidate.order.symbol,
                side: candidate.order.side,
            };
            let (root, context) =
                match cx.admit_repair_replace(&grant.entitlement, &grant.subject, binding) {
                    Ok(issued) => issued,
                    Err(reason) => {
                        cx.notice(
                            Level::Warn,
                            code::ORDER_REJECTED,
                            format!("Backpack repair admission refused: {reason:?}"),
                        );
                        continue;
                    }
                };
            let candidate = grant.candidate.take().expect("candidate checked");
            let stream_time = self.order_stream_time(&candidate.old_id);
            cx.store.invalidate_order_snapshots();
            cx.store
                .set_order_status(&candidate.old_id, OrderStatus::Canceling);
            self.repairs.active.push(Active {
                root: root.clone(),
                candidate,
                previous,
                stream_time,
                successor: None,
            });
            self.send_repair(
                cx,
                Request {
                    context,
                    root,
                    stage: Stage::Cancel,
                },
            );
        }
    }
    pub(crate) fn repair_stream(&mut self, id: &str, symbol: &str, fill: f64, terminal: bool) {
        for active in &mut self.repairs.active {
            if active.candidate.old_id == id {
                active.candidate.observe(Some(fill), terminal);
                active.candidate.uncertain |= active.candidate.order.symbol != symbol;
            }
        }
        for grant in &mut self.repairs.grants {
            if let Some(candidate) = &mut grant.candidate
                && candidate.old_id == id
            {
                candidate.observe(Some(fill), terminal);
                candidate.uncertain |= candidate.order.symbol != symbol;
            }
        }
    }
    pub(crate) fn repair_stream_unknown(&mut self, cx: &mut Cx<'_, Pending>) {
        for active in &mut self.repairs.active {
            active.candidate.uncertain = true;
        }
        for grant in &mut self.repairs.grants {
            if let Some(candidate) = &mut grant.candidate {
                candidate.uncertain = true;
            }
        }
        self.retire_unsafe_replacements(cx);
    }
    pub(crate) fn retire_unsafe_replacements(&mut self, cx: &mut Cx<'_, Pending>) {
        let revoked: Vec<_> = self
            .repairs
            .active
            .iter()
            .filter_map(|active| {
                let context = active.successor.as_ref()?;
                (!active
                    .candidate
                    .stable_zero(cx.store.order(&active.candidate.old_id), true))
                .then_some(context.clone())
            })
            .collect();
        for context in revoked {
            let _ = cx.finish_http_context(&context);
        }
    }
    pub(crate) fn repair_reconciliation(
        &self,
        request: &Request,
    ) -> Result<
        safe_cex_engine::request_context::ReconciliationData,
        safe_cex_engine::request_context::Error,
    > {
        #[derive(serde::Serialize)]
        struct Correlation<'a> {
            kind: &'static str,
            old_id: &'a str,
            symbol: &'a str,
        }
        let active = self
            .repairs
            .active
            .iter()
            .find(|a| a.root == request.root)
            .ok_or(safe_cex_engine::request_context::Error::Revoked)?;
        let kind = match request.stage {
            Stage::Cancel => "repairCancel",
            Stage::Place => "repairPlace",
        };
        let text = serde_json::to_string(&Correlation {
            kind,
            old_id: &active.candidate.old_id,
            symbol: &active.candidate.order.symbol,
        })
        .map_err(|_| safe_cex_engine::request_context::Error::Invalid)?;
        safe_cex_engine::request_context::ReconciliationData::new(text)
    }
    fn send_repair(&mut self, cx: &mut Cx<'_, Pending>, request: Request) {
        let Some(active) = self.repairs.active.iter().find(|a| a.root == request.root) else {
            return;
        };
        let Some(identity) = self.options.identity.clone() else {
            self.repair_failed(
                cx,
                request,
                VenueError::new("Backpack repair primary unavailable"),
            );
            return;
        };
        let (method, params, body) = match request.stage {
            Stage::Cancel => {
                let params: sign::Params = [
                    ("symbol".into(), active.candidate.order.symbol.clone()),
                    ("orderId".into(), active.candidate.old_id.clone()),
                ]
                .into();
                (
                    Method::Delete,
                    params.clone(),
                    serde_json::to_string(&params).expect("native cancel params"),
                )
            }
            Stage::Place => (
                Method::Post,
                active.candidate.order.params(),
                serde_json::to_string(&active.candidate.order).expect("native replacement"),
            ),
        };
        let recipe = sign::Http::new(
            identity,
            sign::Request {
                method,
                path: "/api/v1/order".into(),
                rows: vec![params],
                body: Some(body.clone()),
                window: self
                    .account
                    .credentials
                    .as_ref()
                    .map_or(60000, |c| c.window),
            },
        );
        if let Err((Pending::Repair(request), error)) = cx.request_signed_managed(
            request.context.clone(),
            None,
            method,
            format!("{REST}/api/v1/order"),
            Some(body),
            Box::new(recipe),
            Pending::Repair(Box::new(request)),
        ) {
            self.repair_failed(cx, *request, error.venue_error());
        }
    }
    pub(crate) fn repair_response(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        response: Response<'_>,
    ) {
        let Some(index) = self
            .repairs
            .active
            .iter()
            .position(|a| a.root == request.root)
        else {
            return;
        };
        let active = &self.repairs.active[index];
        #[derive(Deserialize)]
        struct Ack<'a> {
            #[serde(borrow)]
            id: &'a str,
            #[serde(default, borrow)]
            status: &'a str,
            #[serde(borrow)]
            symbol: Option<&'a str>,
            #[serde(borrow)]
            error: Option<&'a serde_json::value::RawValue>,
            #[serde(borrow)]
            code: Option<&'a serde_json::value::RawValue>,
        }
        let ack = serde_json::from_str::<Ack<'_>>(response.body).ok();
        let accepted = (200..300).contains(&response.status)
            && ack.as_ref().is_some_and(|a| {
                !a.id.trim().is_empty()
                    && a.error.is_none()
                    && a.code.is_none()
                    && a.symbol
                        .is_none_or(|symbol| symbol == active.candidate.order.symbol)
                    && match request.stage {
                        Stage::Cancel => {
                            a.id == active.candidate.old_id
                                && matches!(a.status, "Cancelled" | "Canceled")
                        }
                        Stage::Place => a.id != active.candidate.old_id,
                    }
            });
        if !accepted {
            let recorded = if (200..300).contains(&response.status) {
                cx.http_write_unproven(
                    &request.context,
                    "Backpack repair acknowledgement is not definitive",
                )
            } else {
                cx.http_write_failure(
                    &request.context,
                    &UNKNOWN_CODES,
                    Some(&safe_cex_engine::classify::Native {
                        code: safe_cex_engine::classify::NativeCode::None,
                        message: response.body,
                    }),
                    "Backpack repair HTTP failure",
                    None,
                )
                .map(|_| ())
            };
            if recorded.is_ok() {
                let _ = cx.finish_http_context(&request.context);
            }
            return;
        }
        let id = ack.expect("accepted acknowledgement").id.to_owned();
        if cx
            .http_write_fact(
                &request.context,
                WriteFact::Accepted {
                    order_ids: vec![id.clone()],
                },
                None,
            )
            .is_err()
            || cx.validate_http_write_effects(&request.context).is_err()
        {
            return;
        }
        match request.stage {
            Stage::Cancel => {
                let active = &mut self.repairs.active[index];
                active.candidate.observe(fill_proof(response.body), false);
                let safe = active
                    .candidate
                    .stable_zero(cx.store.order(&active.candidate.old_id), false);
                let old = active.candidate.old_id.clone();
                cx.store.invalidate_order_snapshots();
                if self.order_stream_time(&old) == self.repairs.active[index].stream_time {
                    cx.store.update_order(&old, |o| {
                        if !matches!(o.status, OrderStatus::Closed | OrderStatus::Canceled) {
                            o.status = OrderStatus::Canceled;
                        }
                    });
                }
                if safe {
                    match cx.repair_successor(&request.root) {
                        Ok(context) => {
                            self.repairs.active[index].successor = Some(context.clone());
                            self.send_repair(
                                cx,
                                Request {
                                    context,
                                    root: request.root,
                                    stage: Stage::Place,
                                },
                            );
                            return; // The core retired the cancel credit before issuing its successor.
                        }
                        Err(reason) => cx.notice(
                            Level::Warn,
                            code::ORDER_REJECTED,
                            format!("Backpack repair successor refused: {reason:?}"),
                        ),
                    }
                }
            }
            Stage::Place => {
                let mapped = serde_json::from_str::<crate::wire::OrderRow<'_>>(response.body)
                    .ok()
                    .and_then(|row| account::order(row, cx.store).ok());
                cx.store.invalidate_order_snapshots();
                if let Some(order) = mapped {
                    cx.store
                        .remove_orders(&[self.repairs.active[index].candidate.old_id.as_str()]);
                    if cx.store.order(&order.id).is_none() {
                        cx.store.upsert_order(order);
                    }
                }
            }
        }
        let _ = cx.finish_http_context(&request.context);
    }
    pub(crate) fn repair_failed(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        request: Request,
        _error: VenueError,
    ) {
        let _ = cx.finish_http_context(&request.context);
    }
    pub(crate) fn repair_notice(&mut self, cx: &mut Cx<'_, Pending>, notice: RepairNotice<'_>) {
        let RepairNotice::Settled { root, subjects } = notice else {
            return;
        };
        let Some(index) = self.repairs.active.iter().position(|a| a.root == *root) else {
            return;
        };
        let active = self.repairs.active.swap_remove(index);
        for fact in subjects {
            if let Some(grant) = self
                .repairs
                .grants
                .iter_mut()
                .find(|g| g.subject.key == fact.key)
            {
                grant.unknown |=
                    fact.cancel == Effect::Unknown || fact.place == Some(Effect::Unknown);
            }
        }
        if !cx.now.is_finite() {
            return;
        }
        if subjects
            .iter()
            .any(|fact| fact.cancel != Effect::Accepted || fact.place != Some(Effect::Accepted))
        {
            if subjects
                .iter()
                .all(|fact| matches!(fact.cancel, Effect::NotSent | Effect::Rejected))
                && self.order_stream_time(&active.candidate.old_id) == active.stream_time
            {
                cx.store.update_order(&active.candidate.old_id, |o| {
                    if o.status == OrderStatus::Canceling {
                        o.status = active.previous;
                    }
                });
            }
            cx.notice(Level::Warn, code::ORDER_REJECTED, MANUAL);
        }
        self.repair_refresh(cx);
    }
}
