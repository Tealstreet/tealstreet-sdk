//! Engine custody for repair entitlements and roots. See
//! docs/plans/RUST_REPAIR_MUTATION_AUTHORITY.md and `crate::repair`.
use super::*;
use crate::classify::Intent;
use crate::model::{Order, OrderStatus};
use crate::repair::{
    Binding, Effect, Entitlement, RepairNotice, RepairOrigin, RepairRefusal, RepairRoot, Retire,
    SnapshotReceipt, Subject, SubjectFact,
};
use crate::request_context::{
    Context, Credential, Operation, Owner, Policy, Preflight, RetryPolicy, WriteResult,
};
use crate::store::OrderSnapshotToken;

/// Live entitlements, and separately live roots, per engine.
const CAPACITY: usize = 256;
/// A root sends at most this many exact IDs in one native request.
const ROOT_SUBJECTS: usize = 10;
/// Each repair leg's bound: the audited host default for an untimed request.
const LEG_TIMEOUT_MS: u32 = 20_000;
const UNAVAILABLE: &str = "repair.unavailable";

#[derive(Default)]
pub(super) struct Repairs {
    /// Every handle carries this engine's issuer; a foreign handle never
    /// matches, whatever its number.
    issuer: std::sync::Arc<()>,
    next: u64,
    grants: Vec<Grant>,
    roots: Vec<Root>,
    receipts: Vec<Receipt>,
    notices: Vec<Pending>,
    /// The request whose DATA is being delivered; failures never set it.
    data: Option<RequestId>,
}

struct Grant {
    id: u64,
    origin: RepairOrigin,
    primary: Credential,
    /// Snapshot origin: the symbol its one cancel-then-place is bound to.
    snapshot_symbol: Option<String>,
    /// `Store::snapshot_begins` at mint; only later snapshots can retire.
    minted: u64,
    subjects: Vec<Held>,
}

struct Held {
    subject: Subject,
    state: State,
    order_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum State {
    Unspent,
    Spent(u64),
    Retired,
}

struct Root {
    id: u64,
    replace: bool,
    symbol: String,
    members: Vec<(u64, usize)>,
    cancel: Context,
    place: Option<Context>,
}

struct Receipt {
    id: u64,
    request: RequestId,
    read: Context,
    orders_revision: u64,
    began: u64,
}

enum Pending {
    Settled(RepairRoot, Vec<SubjectFact>),
    MintRefused(RepairOrigin, Vec<Subject>, RepairRefusal),
}

impl Repairs {
    fn issue(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
    fn ours(&self, issuer: &std::sync::Arc<()>) -> bool {
        std::sync::Arc::ptr_eq(&self.issuer, issuer)
    }
    fn grant(&self, entitlement: &Entitlement) -> Option<usize> {
        if !self.ours(&entitlement.issuer) {
            return None;
        }
        self.grants.iter().position(|g| g.id == entitlement.id)
    }
    fn root_live(&self, id: u64) -> bool {
        self.roots.iter().any(|root| root.id == id)
    }
    /// Drop fully retired grants so they stop holding capacity.
    fn collect(&mut self) {
        self.grants.retain(|grant| {
            grant
                .subjects
                .iter()
                .any(|held| held.state != State::Retired)
        });
    }
}

/// A released repair leg: only the Place leg marks its placing scope sent.
pub(super) fn repair_released(
    repairs: &Repairs,
    locks: &mut crate::order_locks::OrderLocks,
    generation: u64,
    context: &Context,
) {
    if repairs
        .roots
        .iter()
        .any(|root| root.id == generation && root.place.as_ref() == Some(context))
    {
        locks.note_repair_write(generation);
    }
}

fn unavailable(reason: RepairRefusal) -> String {
    match reason {
        RepairRefusal::Capacity => {
            "Automatic repair authority unavailable: capacity exhausted".into()
        }
        reason => format!("Automatic repair authority unavailable: {reason:?}"),
    }
}

/// Whether the Store publishes `binding.order_id` on `binding.symbol` as this
/// subject's order. The only refinement from a subject to a native order.
fn bound(store: &Store, held: &Held, binding: &Binding<'_>) -> bool {
    store.order(binding.order_id).is_some_and(|order| {
        order.id == binding.order_id
            && order.symbol == binding.symbol
            && (order.id == held.subject.key
                || order.client_order_id.as_deref() == Some(held.subject.key.as_str()))
    })
}

/// Whether a Store order is the subject's: by native ID, client ID or the
/// native ID bound at spend.
fn matches(order: &Order, held: &Held) -> bool {
    order.id == held.subject.key
        || order.client_order_id.as_deref() == Some(held.subject.key.as_str())
        || held.order_id.as_deref() == Some(order.id.as_str())
}

impl<P> Cx<'_, P> {
    fn installed_primary(&self, credential: &Credential) -> bool {
        credential.slot == crate::CredentialSlot::Primary
            && self
                .io
                .core
                .identities
                .primary
                .as_ref()
                .is_some_and(|identity| identity.owns(&credential.reference))
    }

    fn owned_credential(&self, credential: Option<&Credential>) -> bool {
        let Some(credential) = credential else {
            return false;
        };
        let identities = &self.io.core.identities;
        match credential.slot {
            crate::CredentialSlot::Primary => identities.primary.as_ref(),
            crate::CredentialSlot::Secondary => identities.secondary.as_ref(),
        }
        .is_some_and(|identity| identity.owns(&credential.reference))
    }

    /// The exact live DATA delivery's receipt, if `receipt` is one.
    fn current_receipt(&self, receipt: &SnapshotReceipt) -> Option<&Receipt> {
        let delivery = self.io.core.http_delivery.as_ref()?;
        let data = self.io.core.repairs.data?;
        if !self.io.core.repairs.ours(&receipt.issuer) {
            return None;
        }
        self.io
            .core
            .repairs
            .receipts
            .iter()
            .find(|held| held.id == receipt.id)
            .filter(|held| held.request == data && delivery.context == held.read)
    }

    /// Outside any command entry, and inside no delivery other than a
    /// Background Read's or a repair leg's.
    fn repair_custody(&self) -> bool {
        self.io.core.command_entry.is_none()
            && self.io.core.http_delivery.as_ref().is_none_or(|delivery| {
                matches!(
                    delivery.context.owner(),
                    Owner::Background { .. } | Owner::Repair { .. }
                )
            })
    }

    fn command_mint_refused(&mut self, command: CommandId, reason: RepairRefusal) {
        let message = unavailable(reason);
        self.io
            .core
            .actions
            .push(Action::Notice(Notice::CommandError {
                command,
                message: message.clone(),
            }));
        self.notice(Level::Warn, UNAVAILABLE, message);
    }

    fn snapshot_mint_refused(
        &mut self,
        origin: RepairOrigin,
        subject: Subject,
        reason: RepairRefusal,
    ) -> RepairRefusal {
        // Every poll re-reads a still-covered order; that is not news.
        if reason != RepairRefusal::Duplicate {
            self.notice(Level::Warn, UNAVAILABLE, unavailable(reason));
            self.io
                .core
                .repairs
                .notices
                .push(Pending::MintRefused(origin, vec![subject], reason));
        }
        reason
    }

    /// Mint cancel authority for subjects owed by the current admitted
    /// command. The Preflight must be this Bound entry's WriteAfterReads
    /// Preflight; its primary write credential becomes the repair's signer.
    pub fn mint_command_repair(
        &mut self,
        preflight: &Preflight,
        subjects: Vec<Subject>,
    ) -> Result<Entitlement, RepairRefusal> {
        let Owner::Command { id: command, .. } = preflight.owner() else {
            return Err(RepairRefusal::Invalid);
        };
        if !self.io.entry_bound(command) {
            return Err(RepairRefusal::NotInEntry);
        }
        let primary = self
            .io
            .core
            .request_contexts
            .admitted_write_credential(preflight)
            .filter(|credential| self.installed_primary(credential))
            .ok_or(RepairRefusal::NoPrimary)?;
        if subjects.is_empty()
            || subjects
                .iter()
                .enumerate()
                .any(|(i, s)| subjects[..i].iter().any(|t| t.key == s.key))
        {
            return Err(RepairRefusal::Invalid);
        }
        if self.io.core.repairs.grants.len() >= CAPACITY {
            self.command_mint_refused(command, RepairRefusal::Capacity);
            return Err(RepairRefusal::Capacity);
        }
        let minted = self.store.snapshot_begins();
        let repairs = &mut self.io.core.repairs;
        let id = repairs.issue();
        repairs.grants.push(Grant {
            id,
            origin: RepairOrigin::Command(command),
            primary,
            snapshot_symbol: None,
            minted,
            subjects: subjects
                .into_iter()
                .map(|subject| Held {
                    subject,
                    state: State::Unspent,
                    order_id: None,
                })
                .collect(),
        });
        Ok(Entitlement {
            issuer: self.io.core.repairs.issuer.clone(),
            id,
        })
    }

    /// Commit an accepted whole Orders snapshot inside the exact live
    /// Background Read DATA delivery of `read`, as `Store::commit_orders_snapshot`.
    pub fn commit_repair_snapshot<I, F>(
        &mut self,
        read: &Context,
        tokens: I,
        publish: F,
    ) -> Option<SnapshotReceipt>
    where
        I: IntoIterator<Item = OrderSnapshotToken>,
        F: FnOnce(&[Order]) -> Vec<Order>,
    {
        self.validate_background_read_http_delivery(read).ok()?;
        let request = self.io.core.http_delivery.as_ref()?.attempt.request;
        if self.io.core.repairs.data != Some(request) {
            return None;
        }
        let tokens: Vec<OrderSnapshotToken> = tokens.into_iter().collect();
        let began = tokens.iter().map(|token| token.1).min()?;
        if !self.store.commit_orders_snapshot(tokens, publish) {
            return None;
        }
        let orders_revision = self.store.orders_revision();
        let repairs = &mut self.io.core.repairs;
        let id = repairs.issue();
        repairs.receipts.push(Receipt {
            id,
            request,
            read: read.clone(),
            orders_revision,
            began,
        });
        Some(SnapshotReceipt {
            issuer: self.io.core.repairs.issuer.clone(),
            id,
        })
    }

    /// Mint cancel-then-place authority for one order that this delivery's
    /// committed snapshot published, before the Read retires.
    pub fn mint_snapshot_repair(
        &mut self,
        receipt: &SnapshotReceipt,
        primary: Credential,
        subject: Subject,
        symbol: &str,
    ) -> Result<Entitlement, RepairRefusal> {
        let Some(held) = self.current_receipt(receipt) else {
            let origin = RepairOrigin::Snapshot(
                self.io
                    .core
                    .http_delivery
                    .as_ref()
                    .map_or(RequestId(0), |delivery| delivery.attempt.request),
            );
            return Err(self.snapshot_mint_refused(origin, subject, RepairRefusal::NotData));
        };
        let origin = RepairOrigin::Snapshot(held.request);
        let reason = if self.store.orders_revision() != held.orders_revision {
            Some(RepairRefusal::Stale)
        } else if !self.owned_credential(held.read.credential()) {
            Some(RepairRefusal::NotOwned)
        } else if self
            .store
            .order(&subject.key)
            .is_none_or(|order| order.symbol != symbol)
        {
            Some(RepairRefusal::StoreMissing)
        } else if !self.installed_primary(&primary) {
            Some(RepairRefusal::NoPrimary)
        } else if self.io.core.repairs.grants.iter().any(|grant| {
            grant.snapshot_symbol.is_some()
                && grant
                    .subjects
                    .iter()
                    .any(|held| held.subject.key == subject.key && held.state != State::Retired)
        }) {
            Some(RepairRefusal::Duplicate)
        } else if self.io.core.repairs.grants.len() >= CAPACITY {
            Some(RepairRefusal::Capacity)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(self.snapshot_mint_refused(origin, subject, reason));
        }
        let minted = self.store.snapshot_begins();
        let repairs = &mut self.io.core.repairs;
        let id = repairs.issue();
        repairs.grants.push(Grant {
            id,
            origin,
            primary,
            snapshot_symbol: Some(symbol.to_owned()),
            minted,
            subjects: vec![Held {
                subject,
                state: State::Unspent,
                order_id: None,
            }],
        });
        Ok(Entitlement {
            issuer: self.io.core.repairs.issuer.clone(),
            id,
        })
    }

    /// Issue one leg of `root`; refusal leaves nothing allocated or held.
    fn issue_leg(
        &mut self,
        root: u64,
        primary: &Credential,
        intent: Intent,
    ) -> Result<Context, RepairRefusal> {
        // Refuse before reserving an ID, so a full registry spends nothing.
        if !self.io.core.request_contexts.repair_leg_capacity() {
            return Err(RepairRefusal::Capacity);
        }
        let identity = self.io.reserve_ids(1).ok_or(RepairRefusal::Capacity)?.start;
        self.io
            .core
            .request_contexts
            .issue_repair(
                identity,
                root,
                intent,
                Policy {
                    operation: Operation::Mutation,
                    credential: Some(primary.clone()),
                    timeout_ms: LEG_TIMEOUT_MS,
                    pacing: None,
                    retry: RetryPolicy::Never,
                },
                self.now,
            )
            .map_err(|error| match error {
                crate::request_context::Error::Capacity => RepairRefusal::Capacity,
                _ => RepairRefusal::Invalid,
            })
    }

    /// Spend compatible subjects as one cancel request.
    pub fn admit_repair_cancel(
        &mut self,
        members: &[(&Entitlement, &Subject, Binding<'_>)],
    ) -> Result<(RepairRoot, Context), RepairRefusal> {
        if !self.repair_custody() {
            return Err(RepairRefusal::NotInEntry);
        }
        let Some((_, _, first)) = members.first() else {
            return Err(RepairRefusal::Invalid);
        };
        if members.len() > ROOT_SUBJECTS {
            return Err(RepairRefusal::TooMany);
        }
        let repairs = &self.io.core.repairs;
        let mut spend = Vec::with_capacity(members.len());
        let mut primary: Option<&Credential> = None;
        for (entitlement, subject, binding) in members {
            let grant = repairs.grant(entitlement).ok_or(RepairRefusal::Invalid)?;
            let held = &repairs.grants[grant];
            let index = held
                .subjects
                .iter()
                .position(|h| h.subject == **subject)
                .ok_or(RepairRefusal::Invalid)?;
            match held.subjects[index].state {
                State::Unspent => {}
                State::Spent(_) => return Err(RepairRefusal::Spent),
                State::Retired => return Err(RepairRefusal::Retired),
            }
            if !matches!(held.origin, RepairOrigin::Command(_))
                || primary.is_some_and(|p| *p != held.primary)
                || binding.symbol != first.symbol
                || binding.side != first.side
            {
                return Err(RepairRefusal::Incompatible);
            }
            if spend.contains(&(grant, index)) {
                return Err(RepairRefusal::Invalid);
            }
            if !bound(self.store, &held.subjects[index], binding) {
                return Err(RepairRefusal::StoreMissing);
            }
            primary = Some(&held.primary);
            spend.push((grant, index));
        }
        let primary = primary.expect("one member").clone();
        if !self.installed_primary(&primary) {
            return Err(RepairRefusal::NoPrimary);
        }
        if repairs.roots.len() >= CAPACITY {
            return Err(RepairRefusal::Capacity);
        }
        let symbol = first.symbol.to_owned();
        let ids = members
            .iter()
            .map(|(_, _, b)| b.order_id.to_owned())
            .collect();
        self.open_root(primary, symbol, ids, false, &spend, members)
    }

    /// The cancel leg of one snapshot subject's cancel-then-place.
    pub fn admit_repair_replace(
        &mut self,
        entitlement: &Entitlement,
        subject: &Subject,
        binding: Binding<'_>,
    ) -> Result<(RepairRoot, Context), RepairRefusal> {
        if !self.repair_custody() {
            return Err(RepairRefusal::NotInEntry);
        }
        let repairs = &self.io.core.repairs;
        let grant = repairs.grant(entitlement).ok_or(RepairRefusal::Invalid)?;
        let held = &repairs.grants[grant];
        let index = held
            .subjects
            .iter()
            .position(|h| h.subject == *subject)
            .ok_or(RepairRefusal::Invalid)?;
        match held.subjects[index].state {
            State::Unspent => {}
            State::Spent(_) => return Err(RepairRefusal::Spent),
            State::Retired => return Err(RepairRefusal::Retired),
        }
        if held.snapshot_symbol.as_deref() != Some(binding.symbol) {
            return Err(RepairRefusal::Incompatible);
        }
        if !bound(self.store, &held.subjects[index], &binding) {
            return Err(RepairRefusal::StoreMissing);
        }
        let primary = held.primary.clone();
        if !self.installed_primary(&primary) {
            return Err(RepairRefusal::NoPrimary);
        }
        if repairs.roots.len() >= CAPACITY {
            return Err(RepairRefusal::Capacity);
        }
        self.open_root(
            primary,
            binding.symbol.to_owned(),
            vec![binding.order_id.to_owned()],
            true,
            &[(grant, index)],
            &[(entitlement, subject, binding)],
        )
    }

    fn open_root(
        &mut self,
        primary: Credential,
        symbol: String,
        ids: Vec<String>,
        replace: bool,
        spend: &[(usize, usize)],
        members: &[(&Entitlement, &Subject, Binding<'_>)],
    ) -> Result<(RepairRoot, Context), RepairRefusal> {
        let id = self.io.core.repairs.next + 1;
        self.io
            .core
            .order_locks
            .acquire_repair(id, ids, vec![symbol.clone()], replace)
            .map_err(|_| RepairRefusal::Conflict)?;
        let cancel = match self.issue_leg(id, &primary, Intent::Cancel) {
            Ok(context) => context,
            Err(reason) => {
                self.io.core.order_locks.finish_repair(id, false);
                return Err(reason);
            }
        };
        let repairs = &mut self.io.core.repairs;
        repairs.next = id;
        let mut owners = Vec::with_capacity(spend.len());
        for (&(grant, index), (_, _, binding)) in spend.iter().zip(members) {
            let grant = &mut repairs.grants[grant];
            owners.push((grant.id, index));
            let held = &mut grant.subjects[index];
            held.state = State::Spent(id);
            held.order_id = Some(binding.order_id.to_owned());
        }
        repairs.roots.push(Root {
            id,
            replace,
            symbol,
            members: owners,
            cancel: cancel.clone(),
            place: None,
        });
        Ok((
            RepairRoot {
                issuer: self.io.core.repairs.issuer.clone(),
                id,
            },
            cancel,
        ))
    }

    /// The single placement leg after an engine-recorded accepted cancel.
    pub fn repair_successor(&mut self, root: &RepairRoot) -> Result<Context, RepairRefusal> {
        let repairs = &self.io.core.repairs;
        if !repairs.ours(&root.issuer) {
            return Err(RepairRefusal::Invalid);
        }
        let held = repairs
            .roots
            .iter()
            .find(|r| r.id == root.id && r.replace)
            .ok_or(RepairRefusal::Invalid)?;
        if held.place.is_some() {
            return Err(RepairRefusal::Spent);
        }
        let accepted = matches!(
            self.io
                .core
                .request_contexts
                .repair_leg_outcome(root.id, held.cancel.leg()),
            Some((true, Some(WriteResult::Accepted { .. })))
        );
        if !accepted {
            return Err(RepairRefusal::Invalid);
        }
        let (grant, _) = held.members[0];
        let primary = repairs
            .grants
            .iter()
            .find(|g| g.id == grant)
            .map(|g| g.primary.clone())
            .ok_or(RepairRefusal::Invalid)?;
        // The predecessor's slot is retired before the successor is issued.
        let cancel = held.cancel.clone();
        if self.io.core.request_contexts.repair_leg_live(&cancel) {
            self.io.retire_http_leg(&cancel);
            self.io.core.request_contexts.remove_repair_leg(&cancel);
        }
        let place = self.issue_leg(root.id, &primary, Intent::Place)?;
        let held = self
            .io
            .core
            .repairs
            .roots
            .iter_mut()
            .find(|r| r.id == root.id)
            .expect("root checked above");
        held.place = Some(place.clone());
        Ok(place)
    }

    pub fn retire_repair_subject(
        &mut self,
        entitlement: &Entitlement,
        key: &str,
        proof: Retire<'_>,
    ) -> Result<(), RepairRefusal> {
        let repairs = &self.io.core.repairs;
        let grant = repairs.grant(entitlement).ok_or(RepairRefusal::Invalid)?;
        let index = repairs.grants[grant]
            .subjects
            .iter()
            .position(|h| h.subject.key == key)
            .ok_or(RepairRefusal::Invalid)?;
        let held = &repairs.grants[grant].subjects[index];
        match held.state {
            State::Retired => return Ok(()),
            State::Spent(root) if repairs.root_live(root) => return Err(RepairRefusal::Spent),
            State::Unspent | State::Spent(_) => {}
        }
        let mut orders = self
            .store
            .orders()
            .iter()
            .filter(|order| matches(order, held));
        match proof {
            Retire::Terminal => {
                if !orders.any(|order| {
                    matches!(order.status, OrderStatus::Closed | OrderStatus::Canceled)
                }) {
                    return Err(RepairRefusal::Invalid);
                }
            }
            Retire::SourceOrders { since } => {
                if self
                    .io
                    .core
                    .source_orders_cover
                    .is_none_or(|cover| cover < since)
                {
                    return Err(RepairRefusal::NotData);
                }
                if orders.next().is_some() {
                    return Err(RepairRefusal::Invalid);
                }
            }
            Retire::CompleteRead(receipt) => {
                let minted = repairs.grants[grant].minted;
                let Some(read) = self.current_receipt(receipt) else {
                    return Err(RepairRefusal::NotData);
                };
                if read.began <= minted || self.store.orders_revision() != read.orders_revision {
                    return Err(RepairRefusal::Stale);
                }
                if orders.next().is_some() {
                    return Err(RepairRefusal::Invalid);
                }
            }
        }
        let repairs = &mut self.io.core.repairs;
        repairs.grants[grant].subjects[index].state = State::Retired;
        repairs.collect();
        Ok(())
    }

    /// Unspent subjects still covered by this entitlement.
    pub fn repair_subjects(&self, entitlement: &Entitlement) -> Vec<Subject> {
        let repairs = &self.io.core.repairs;
        repairs.grant(entitlement).map_or_else(Vec::new, |grant| {
            repairs.grants[grant]
                .subjects
                .iter()
                .filter(|held| held.state == State::Unspent)
                .map(|held| held.subject.clone())
                .collect()
        })
    }
}

impl<P> Io<P> {
    /// One subject's effect from its leg's latest attempt (`None`: the whole
    /// leg). A native ACK proves only the orders it names.
    fn subject_effect(&self, root: u64, leg: &Context, order_id: Option<&str>) -> Effect {
        let named = |ids: &[String]| order_id.is_some_and(|id| ids.iter().any(|named| named == id));
        let partial = |error: &VenueError| -> Vec<String> {
            error
                .partial
                .as_ref()
                .map_or_else(Vec::new, |partial| partial.order_ids.clone())
        };
        match self
            .core
            .request_contexts
            .repair_leg_outcome(root, leg.leg())
        {
            Some((_, Some(WriteResult::Accepted { order_ids }))) => {
                if order_ids.is_empty() || order_id.is_none() || named(order_ids) {
                    Effect::Accepted
                } else {
                    Effect::Unknown
                }
            }
            Some((_, Some(WriteResult::KnownPartial(error)))) => {
                if named(&partial(error)) {
                    Effect::Accepted
                } else {
                    Effect::Rejected
                }
            }
            Some((released, Some(WriteResult::PartialUnknown(error)))) => {
                if named(&partial(error)) {
                    Effect::Accepted
                } else if released {
                    Effect::Unknown
                } else {
                    Effect::NotSent
                }
            }
            Some((_, Some(WriteResult::Rejected(_)))) => Effect::Rejected,
            Some((true, _)) => Effect::Unknown,
            Some((false, _)) | None => Effect::NotSent,
        }
    }

    /// A venue finished a repair leg outside its own root's delivery (a refused
    /// send, or a revocation from a stream or another Background delivery): end
    /// the root once it holds no live leg, so `Settled` still comes. Inside the
    /// root's own delivery, `repair_delivered` ends it after the callback.
    /// Terminal by contract: a later leg exists only as `repair_successor`
    /// inside its predecessor's delivery, never from a timer or ready callback.
    pub(super) fn repair_leg_finished(&mut self, context: &Context, now: f64) {
        let Owner::Repair { generation } = context.owner() else {
            return;
        };
        let own_delivery = self
            .core
            .http_delivery
            .as_ref()
            .is_some_and(|delivery| delivery.context.owner() == Owner::Repair { generation });
        if !own_delivery && self.core.request_contexts.repair_live_legs(generation) == 0 {
            self.end_repair_root(generation, now);
        }
    }

    /// End a root whose last leg was delivered: release its scope, reconcile
    /// any uncertainty and queue its notice. Spent subjects stay spent.
    fn end_repair_root(&mut self, generation: u64, now: f64) {
        let repairs = &mut self.core.repairs;
        let Some(index) = repairs.roots.iter().position(|root| root.id == generation) else {
            return;
        };
        let root = repairs.roots.swap_remove(index);
        let place = root
            .place
            .as_ref()
            .map(|leg| self.subject_effect(generation, leg, None));
        let mut facts = Vec::with_capacity(root.members.len());
        for &(grant, index) in &root.members {
            let Some(grant) = self.core.repairs.grants.iter().find(|g| g.id == grant) else {
                continue;
            };
            let held = &grant.subjects[index];
            facts.push(SubjectFact {
                key: held.subject.key.clone(),
                order_id: held.order_id.clone(),
                origin: grant.origin,
                cancel: self.subject_effect(generation, &root.cancel, held.order_id.as_deref()),
                place,
            });
        }
        let failed_place = place.is_some_and(|effect| effect != Effect::Accepted);
        let marked = self
            .core
            .order_locks
            .finish_repair(generation, failed_place);
        let unknown = facts
            .iter()
            .any(|fact| fact.cancel == Effect::Unknown || fact.place == Some(Effect::Unknown));
        if unknown || !marked.is_empty() {
            self.queue_reconcile(Some(vec![root.symbol]), now);
        }
        self.core.request_contexts.close_repair(generation);
        let root = RepairRoot {
            issuer: self.core.repairs.issuer.clone(),
            id: generation,
        };
        self.core
            .repairs
            .notices
            .push(Pending::Settled(root, facts));
    }
}

impl<V: Venue> Engine<V> {
    /// Mark DATA delivery of `request` for snapshot receipts; failures never do.
    pub(super) fn begin_repair_data(&mut self, request: Option<RequestId>) -> Option<RequestId> {
        std::mem::replace(&mut self.io.core.repairs.data, request)
    }

    pub(super) fn end_repair_data(&mut self, previous: Option<RequestId>) {
        let repairs = &mut self.io.core.repairs;
        if let Some(request) = repairs.data {
            repairs
                .receipts
                .retain(|receipt| receipt.request != request);
        }
        repairs.data = previous;
    }

    /// After a repair leg's delivery: retire the leg, and end its root once no
    /// leg (such as a successor issued in this delivery) remains.
    pub(super) fn repair_delivered(
        &mut self,
        delivery: &request_context_registry::Delivery,
        now: f64,
    ) {
        let Owner::Repair { generation } = delivery.context.owner() else {
            return;
        };
        let registry = &mut self.io.core.request_contexts;
        if registry.repair_leg_live(&delivery.context) {
            self.io.retire_http_leg(&delivery.context);
            self.io
                .core
                .request_contexts
                .remove_repair_leg(&delivery.context);
        }
        if self.io.core.request_contexts.repair_live_legs(generation) == 0 {
            self.io.end_repair_root(generation, now);
        }
    }

    /// Deliver queued notices outside any command entry or delivery.
    pub(super) fn flush_repair_notices(&mut self, now: f64) {
        while self.io.core.command_entry.is_none()
            && self.io.core.http_delivery.is_none()
            && !self.io.core.repairs.notices.is_empty()
        {
            let pending = self.io.core.repairs.notices.remove(0);
            let mut cx = Cx {
                now,
                store: &mut self.store,
                io: &mut self.io,
            };
            match &pending {
                Pending::Settled(root, facts) => self.venue.on_repair_notice(
                    &mut cx,
                    RepairNotice::Settled {
                        root,
                        subjects: facts,
                    },
                ),
                Pending::MintRefused(origin, subjects, reason) => self.venue.on_repair_notice(
                    &mut cx,
                    RepairNotice::MintRefused {
                        origin: *origin,
                        subjects,
                        reason: *reason,
                    },
                ),
            }
        }
    }
}
