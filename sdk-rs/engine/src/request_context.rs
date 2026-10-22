//! Semantic HTTP ownership and the final controllable transport handoff.
//!
//! This module owns no requests, credentials, clocks or network executor. A host
//! prepares its route first; the engine checks a single-use permit immediately
//! before that prepared executor hands the request to the transport API.
use crate::signing::{KeyRef, SignReceipt};
use crate::{CommandId, CredentialSlot, Outcome, RequestId, VenueError};

#[path = "http_reconciliation.rs"]
mod reconciliation;
pub use reconciliation::ReconciliationData;
pub(crate) use reconciliation::{
    Record as ReconciliationRecord, Responses as ReconciliationResponses,
};

/// Late ACK codec view. Order changes are allowed only when neither another
/// released write nor any Store order update has superseded the release fence.
/// No command, auth, retry, scheduler or order-lock authority is exposed.
pub struct Reconciliation<'a> {
    pub owner: Owner,
    pub leg: u64,
    pub release: HttpRelease,
    store: &'a mut crate::Store,
    may_update: bool,
}

impl<'a> Reconciliation<'a> {
    pub(crate) fn new(
        record: &ReconciliationRecord,
        store: &'a mut crate::Store,
        write_revision: u64,
    ) -> Self {
        Self {
            owner: record.owner,
            leg: record.leg,
            release: record.release.expect("only released writes are reconciled"),
            may_update: record.order_revision != u64::MAX
                && record.write_revision != u64::MAX
                && record.order_revision == store.orders_revision()
                && record.write_revision == write_revision,
            store,
        }
    }

    pub fn orders(&self) -> &[crate::model::Order] {
        self.store.orders()
    }

    /// All patches in this one callback share its initial atomic fence. Its
    /// first accepted patch does not invalidate the remaining patches.
    pub fn upsert_order(&mut self, order: crate::model::Order) -> bool {
        if !self.may_update {
            return false;
        }
        self.store.upsert_order(order);
        true
    }

    pub fn set_order_status(&mut self, id: &str, status: crate::model::OrderStatus) -> bool {
        self.may_update && self.store.set_order_status(id, status)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Command { id: CommandId, generation: u64 },
    Background { generation: u64 },
}

/// Independent of HTTP method: private POST snapshots are still reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Read,
    Mutation,
}

/// Route preparation cannot inspect credentials which have not been signed or
/// presented yet. This intent is nonsecret and independent of HTTP method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutingIntent {
    Public,
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RetryPolicy {
    #[default]
    Never,
    ReadFailure,
    DefinitiveRefusal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryEvidence {
    ReadFailure,
    DefinitiveRefusal,
}

/// A venue declares semantics; the engine assigns ownership generations, leg
/// and attempt identities and captures the original raw deadline.
#[derive(Clone, Debug, PartialEq)]
pub struct Policy {
    pub operation: Operation,
    pub credential: Option<Credential>,
    pub timeout_ms: u32,
    pub pacing: Option<Pacing>,
    pub retry: RetryPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Command(CommandId),
    Background,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub slot: CredentialSlot,
    pub reference: KeyRef,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pacing {
    pub group: u32,
    pub minimum_gap_ms: f64,
}

/// Immutable across physical retries of a logical leg.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    issuer: Option<std::sync::Arc<()>>,
    owner: Owner,
    leg: u64,
    operation: Operation,
    credential: Option<Credential>,
    deadline: f64,
    pacing: Option<Pacing>,
    retry: RetryPolicy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Capacity,
    Duplicate,
    State,
    TimedOut,
    Stale,
    Revoked,
    RetryForbidden,
}

impl Context {
    pub fn new(
        owner: Owner,
        leg: u64,
        operation: Operation,
        credential: Option<Credential>,
        created_at: f64,
        deadline: f64,
        pacing: Option<Pacing>,
    ) -> Result<Self, Error> {
        if !created_at.is_finite()
            || !deadline.is_finite()
            || deadline <= created_at
            || pacing.is_some_and(|p| !p.minimum_gap_ms.is_finite() || p.minimum_gap_ms < 0.)
            || credential
                .as_ref()
                .is_some_and(|c| c.reference.0.is_empty() || c.reference.0.len() > 256)
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            issuer: None,
            owner,
            leg,
            operation,
            credential,
            deadline,
            pacing,
            retry: RetryPolicy::Never,
        })
    }

    pub fn owner(&self) -> Owner {
        self.owner
    }
    pub(crate) fn bind_issuer(&mut self, issuer: &std::sync::Arc<()>) {
        self.issuer = Some(issuer.clone());
    }
    pub(crate) fn issued_by(&self, issuer: &std::sync::Arc<()>) -> bool {
        self.issuer
            .as_ref()
            .is_some_and(|own| std::sync::Arc::ptr_eq(own, issuer))
    }
    pub fn leg(&self) -> u64 {
        self.leg
    }
    pub fn operation(&self) -> Operation {
        self.operation
    }
    pub fn credential(&self) -> Option<&Credential> {
        self.credential.as_ref()
    }
    pub fn routing_intent(&self) -> RoutingIntent {
        match self.credential.as_ref().map(|credential| credential.slot) {
            None => RoutingIntent::Public,
            Some(CredentialSlot::Primary) => RoutingIntent::Primary,
            Some(CredentialSlot::Secondary) => RoutingIntent::Secondary,
        }
    }
    pub fn deadline(&self) -> f64 {
        self.deadline
    }

    pub fn with_retry(mut self, policy: RetryPolicy) -> Result<Self, Error> {
        if policy == RetryPolicy::ReadFailure && self.operation != Operation::Read {
            return Err(Error::RetryForbidden);
        }
        self.retry = policy;
        Ok(self)
    }

    /// Native refusal evidence comes from a venue codec, never HTTP status or
    /// exception text. The engine still checks the previous physical attempt.
    pub fn retry(&self, evidence: RetryEvidence, now: f64) -> Result<Self, Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        if now >= self.deadline {
            return Err(Error::TimedOut);
        }
        if !matches!(
            (self.retry, evidence),
            (RetryPolicy::ReadFailure, RetryEvidence::ReadFailure)
                | (
                    RetryPolicy::DefinitiveRefusal,
                    RetryEvidence::DefinitiveRefusal
                )
        ) {
            return Err(Error::RetryForbidden);
        }
        Ok(self.clone())
    }
}

/// Monotonic engine-issued identity, never a command id alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt {
    pub request: RequestId,
    pub generation: u64,
}

/// Opaque host preparation identity. The host retains the route, authorization
/// and connection; no credential value enters this metadata.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prepared {
    pub id: u64,
    pub expires_at: Option<f64>,
}

/// Preparation and binding use the host's existing HTTP executor. Neither
/// message grants permission to send; the bound executor must claim its permit.
#[derive(Clone, Debug, PartialEq)]
pub enum HttpAction {
    Prepare {
        preparation: Preparation,
        intent: RoutingIntent,
        request: crate::HttpRequest,
    },
    Bind {
        preparation: Preparation,
        permit: Permit,
        request: crate::HttpRequest,
    },
    Cancel {
        preparation: Preparation,
        signing: Option<RequestId>,
    },
}

/// One observation supplies both clock domains. Durations remain raw time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReleaseTime {
    pub raw_ms: f64,
    pub protocol_ms: f64,
    pub revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HttpRelease {
    pub attempt: Attempt,
    pub time: ReleaseTime,
    pub signature: SignReceipt,
    /// Immutable raw lifecycle bound, including preparation and response work.
    pub deadline: f64,
    /// Snapshot for observers only, capped by the lifecycle. The scheduler owns
    /// renewable logical-read expiry; this never installs a physical timeout.
    pub response_lease: Option<f64>,
    /// Protocol signature expiry projected onto raw time, for repair scheduling.
    /// Signature validity is not a bound on when an accepted response may arrive.
    pub signature_valid_until: Option<f64>,
}

/// A validated raw lease. Construct before committing custom admission state.
/// Shared Scheduler admission constructs this only after its checked arithmetic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResponseLease(f64);
impl ResponseLease {
    pub(crate) fn from_scheduler(deadline: f64) -> Self {
        Self(deadline)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Admission {
    Unscheduled,
    Scheduled(ResponseLease),
    Denied,
}

impl HttpRelease {
    pub fn response_lease(&self, deadline: f64) -> Result<ResponseLease, Error> {
        if !deadline.is_finite() || deadline <= self.time.raw_ms {
            return Err(Error::Revoked);
        }
        Ok(ResponseLease(deadline.min(self.deadline)))
    }
    fn new(
        attempt: Attempt,
        time: ReleaseTime,
        signature: SignReceipt,
        deadline: f64,
    ) -> Result<Self, Error> {
        if !time.raw_ms.is_finite() || !time.protocol_ms.is_finite() {
            return Err(Error::Invalid);
        }
        // The raw lifecycle bound wins even when protocol metadata is stale.
        if time.raw_ms >= deadline {
            return Err(Error::TimedOut);
        }
        signature.validate().map_err(|_| Error::Invalid)?;
        let signature_valid_until = if let Some(expiry) = signature.expiry_ms {
            let remaining = expiry - time.protocol_ms;
            let projected = time.raw_ms + remaining;
            if !remaining.is_finite() || !projected.is_finite() {
                return Err(Error::Invalid);
            }
            if remaining <= 0. {
                return Err(Error::Stale);
            }
            Some(deadline.min(projected))
        } else {
            None
        };
        if !deadline.is_finite() || deadline <= time.raw_ms {
            return Err(Error::Invalid);
        }
        Ok(Self {
            attempt,
            time,
            signature,
            deadline,
            response_lease: None,
            signature_valid_until,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permit {
    attempt: Attempt,
    preparation: u64,
    sequence: u64,
}

/// Returned to asynchronous route/sign work and checked again on completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preparation {
    attempt: Attempt,
    sequence: u64,
}

impl Preparation {
    pub fn attempt(self) -> Attempt {
        self.attempt
    }

    /// Lossless opaque host token: wide counters must never cross JS as numbers.
    pub fn to_wire(self) -> String {
        format!(
            "p1:{}:{}:{}",
            self.attempt.request.0, self.attempt.generation, self.sequence
        )
    }

    pub fn from_wire(token: &str) -> Result<Self, Error> {
        let parts = token_parts(token.strip_prefix("p1:").ok_or(Error::Invalid)?, 3)?;
        Ok(Self {
            attempt: token_attempt(parts[0], parts[1])?,
            sequence: parts[2],
        })
    }
}

impl Permit {
    pub fn attempt(self) -> Attempt {
        self.attempt
    }

    pub fn prepared_id(self) -> u64 {
        self.preparation
    }

    pub fn to_wire(self) -> String {
        format!(
            "r1:{}:{}:{}:{}",
            self.attempt.request.0, self.attempt.generation, self.preparation, self.sequence
        )
    }

    pub fn from_wire(token: &str) -> Result<Self, Error> {
        let parts = token_parts(token.strip_prefix("r1:").ok_or(Error::Invalid)?, 4)?;
        if parts[3] == 0 {
            return Err(Error::Invalid);
        }
        Ok(Self {
            attempt: token_attempt(parts[0], parts[1])?,
            preparation: parts[2],
            sequence: parts[3],
        })
    }
}

fn token_attempt(request: u64, generation: u64) -> Result<Attempt, Error> {
    if request == 0 || generation == 0 {
        return Err(Error::Invalid);
    }
    Ok(Attempt {
        request: RequestId(u32::try_from(request).map_err(|_| Error::Invalid)?),
        generation,
    })
}

fn token_parts(token: &str, count: usize) -> Result<[u64; 4], Error> {
    if token.len() > 80 {
        return Err(Error::Invalid);
    }
    let mut values = [0; 4];
    let mut words = token.split(':');
    for value in &mut values[..count] {
        let word = words.next().ok_or(Error::Invalid)?;
        // Reject aliases, whitespace, signs and non-ASCII numerals. The host's
        // resource-map key and the engine's decoded identity must agree exactly.
        if word.is_empty()
            || (word.len() > 1 && word.starts_with('0'))
            || !word.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(Error::Invalid);
        }
        *value = word.parse().map_err(|_| Error::Invalid)?;
    }
    if words.next().is_some() {
        return Err(Error::Invalid);
    }
    Ok(values)
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Preparing,
    Prepared(Prepared),
    Signing(Prepared),
    Ready(Prepared),
    Permitted(Prepared, Permit),
    Released(HttpRelease),
    Settled(Option<HttpRelease>),
}

/// One physical attempt. A retry constructs another lifecycle with the same
/// context; command evidence is retained separately until command sealing.
pub struct Lifecycle {
    context: Context,
    attempt: Attempt,
    state: State,
    signature: SignReceipt,
    signed_revision: Option<u64>,
    fresh_until: f64,
    permits: u64,
    preparation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Claim {
    Released(HttpRelease),
    WaitUntil(f64),
}

impl Lifecycle {
    pub fn new(context: Context, attempt: Attempt) -> Self {
        Self {
            context,
            attempt,
            state: State::Preparing,
            signature: SignReceipt::default(),
            signed_revision: None,
            fresh_until: f64::MAX,
            permits: 0,
            preparation: 0,
        }
    }
    pub fn context(&self) -> &Context {
        &self.context
    }
    pub fn attempt(&self) -> Attempt {
        self.attempt
    }

    pub fn preparation(&self) -> Preparation {
        Preparation {
            attempt: self.attempt,
            sequence: self.preparation,
        }
    }

    pub fn prepared(&mut self, token: Preparation, preparation: Prepared) -> Result<(), Error> {
        if token != self.preparation() {
            return Err(Error::Revoked);
        }
        if self.state != State::Preparing {
            return Err(Error::State);
        }
        if preparation.expires_at.is_some_and(|at| !at.is_finite()) {
            return Err(Error::Invalid);
        }
        self.state = State::Prepared(preparation);
        Ok(())
    }

    pub fn expects_preparation(&self, token: Preparation) -> bool {
        token == self.preparation() && self.state == State::Preparing
    }

    pub fn is_current_permit(&self, permit: Permit) -> bool {
        matches!(self.state, State::Permitted(_, expected) if expected == permit)
    }

    pub fn begin_signing(&mut self) -> Result<Preparation, Error> {
        let State::Prepared(route) = self.state else {
            return Err(Error::State);
        };
        self.state = State::Signing(route);
        Ok(self.preparation())
    }

    pub fn signed(
        &mut self,
        preparation: Preparation,
        receipt: SignReceipt,
        revision: u64,
        fresh_until: f64,
    ) -> Result<(), Error> {
        if preparation != self.preparation() {
            return Err(Error::Revoked);
        }
        let State::Signing(route) = self.state else {
            return Err(Error::State);
        };
        receipt.validate().map_err(|_| Error::Invalid)?;
        if !fresh_until.is_finite() {
            return Err(Error::Invalid);
        }
        self.signature = receipt;
        self.signed_revision = Some(revision);
        self.fresh_until = fresh_until;
        self.state = State::Ready(route);
        Ok(())
    }

    pub fn unsigned(&mut self) -> Result<(), Error> {
        let State::Prepared(route) = self.state else {
            return Err(Error::State);
        };
        self.state = State::Ready(route);
        Ok(())
    }

    /// Issuing/delivering a permit consumes neither admission nor pacing.
    pub fn permit(&mut self) -> Result<Permit, Error> {
        let State::Ready(route) = self.state else {
            return Err(Error::State);
        };
        self.permits = self.permits.checked_add(1).ok_or(Error::Capacity)?;
        let permit = Permit {
            attempt: self.attempt,
            preparation: route.id,
            sequence: self.permits,
        };
        self.state = State::Permitted(route, permit);
        Ok(permit)
    }

    /// Synchronous final claim, immediately followed by prepared transport
    /// handoff. `admit` is called only after every other fallible check passes.
    /// A denied/expired request consumes no pacing slot. There are no fallible
    /// checks after admission commits; its lease must be validated beforehand.
    pub fn claim(
        &mut self,
        permit: Permit,
        time: ReleaseTime,
        pacing: &mut SendPacing,
        admit: &mut dyn FnMut(&HttpRelease) -> Result<Admission, Error>,
    ) -> Result<Claim, Error> {
        let State::Permitted(route, expected) = self.state else {
            return Err(Error::State);
        };
        if permit != expected {
            return Err(Error::Revoked);
        }
        let mut release =
            HttpRelease::new(self.attempt, time, self.signature, self.context.deadline)?;
        if route.expires_at.is_some_and(|at| time.raw_ms >= at)
            || self.signed_revision.is_some_and(|rev| rev != time.revision)
            || time.raw_ms >= self.fresh_until
        {
            return Err(Error::Stale);
        }
        if let Some(at) = pacing.check(self.context.pacing, time.raw_ms)? {
            return if at >= self.context.deadline {
                Err(Error::TimedOut)
            } else {
                Ok(Claim::WaitUntil(at))
            };
        }
        let admission = match admit(&release) {
            Ok(admission) => admission,
            Err(error) => {
                self.state = State::Settled(None);
                return Err(error);
            }
        };
        match admission {
            Admission::Unscheduled => {}
            Admission::Scheduled(lease) => {
                release.response_lease = Some(release.deadline.min(lease.0))
            }
            Admission::Denied => {
                self.state = State::Settled(None);
                return Err(Error::Revoked);
            }
        }
        pacing.released(self.context.pacing, time.raw_ms);
        self.state = State::Released(release);
        Ok(Claim::Released(release))
    }

    /// Reprepare after stale route/signature metadata; the old permit becomes
    /// unusable. The immutable deadline and owner generation are retained.
    pub fn reprepare(&mut self) -> Result<(), Error> {
        if !matches!(
            self.state,
            State::Prepared(_) | State::Signing(_) | State::Ready(_) | State::Permitted(..)
        ) {
            return Err(Error::State);
        }
        self.preparation = self.preparation.checked_add(1).ok_or(Error::Capacity)?;
        self.state = State::Preparing;
        self.signature = SignReceipt::default();
        self.signed_revision = None;
        self.fresh_until = f64::MAX;
        Ok(())
    }

    pub fn release(&self) -> Option<HttpRelease> {
        match self.state {
            State::Released(release) | State::Settled(Some(release)) => Some(release),
            _ => None,
        }
    }

    /// Returns false for duplicate completion/cancellation. Retained release
    /// facts let the owner reconcile a late ACK without reopening the command.
    pub fn settle(&mut self) -> bool {
        if matches!(self.state, State::Settled(_)) {
            return false;
        }
        self.state = State::Settled(self.release());
        true
    }

    pub fn accepts_response(&self) -> bool {
        matches!(self.state, State::Released(_))
    }

    /// One deadline also bounds a lost route/sign/provider/permit callback.
    /// Expiring a released request retains its evidence for late reconciliation.
    pub fn expire(&mut self, now: f64) -> Result<bool, Error> {
        if !now.is_finite() {
            return Err(Error::Invalid);
        }
        Ok(now >= self.context.deadline && self.settle())
    }
}

/// Fixed per-engine group floors. A request may extend the gap after its own
/// handoff, never weaken the group floor or erase the previous release's gap.
#[derive(Default)]
pub struct SendPacing {
    groups: Vec<(Pacing, Option<f64>)>,
}
impl SendPacing {
    pub fn new(groups: &[Pacing]) -> Result<Self, Error> {
        if groups
            .iter()
            .any(|p| !p.minimum_gap_ms.is_finite() || p.minimum_gap_ms < 0.)
        {
            return Err(Error::Invalid);
        }
        if groups
            .iter()
            .enumerate()
            .any(|(i, group)| groups[..i].iter().any(|p| p.group == group.group))
        {
            return Err(Error::Duplicate);
        }
        Ok(Self {
            groups: groups.iter().map(|group| (*group, None)).collect(),
        })
    }
    fn check(&self, pacing: Option<Pacing>, now: f64) -> Result<Option<f64>, Error> {
        let Some(pacing) = pacing else {
            return Ok(None);
        };
        let eligible = self
            .groups
            .iter()
            .find(|(group, _)| group.group == pacing.group)
            .ok_or(Error::Invalid)?
            .1;
        Ok(eligible.filter(|at| now < *at))
    }
    fn released(&mut self, pacing: Option<Pacing>, now: f64) {
        if let Some(pacing) = pacing {
            // claim checked the fixed registry before invoking admission.
            if let Some((group, next)) = self
                .groups
                .iter_mut()
                .find(|(group, _)| group.group == pacing.group)
            {
                *next = Some((now + group.minimum_gap_ms.max(pacing.minimum_gap_ms)).min(f64::MAX));
            }
        }
    }
}

/// Native codecs supply definitive facts. HTTP status and exception text are
/// deliberately absent from this contract.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteResult {
    Accepted { order_ids: Vec<String> },
    Rejected(VenueError),
    Failed(VenueError),
}

struct WriteAttempt {
    id: RequestId,
    leg: u64,
    released: bool,
    result: Option<WriteResult>,
}

/// Bounded evidence retained for the whole command, including prior retries.
/// Registration order is logical planning order, never callback arrival order.
pub struct CommandWrites {
    attempts: Vec<WriteAttempt>,
    capacity: usize,
    sealed: bool,
}
impl CommandWrites {
    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 {
            return Err(Error::Capacity);
        }
        Ok(Self {
            attempts: Vec::new(),
            capacity,
            sealed: false,
        })
    }
    pub fn register(&mut self, id: RequestId, leg: u64) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        if self.attempts.iter().any(|a| a.id == id || a.leg == leg) {
            return Err(Error::Duplicate);
        }
        if self.attempts.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        self.attempts.push(WriteAttempt {
            id,
            leg,
            released: false,
            result: None,
        });
        Ok(())
    }

    /// Only a typed definitive refusal may authorize another write attempt.
    /// The request context's policy/deadline must also authorize this retry.
    pub fn retry(&mut self, previous: RequestId, id: RequestId) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        if self.attempts.iter().any(|a| a.id == id) {
            return Err(Error::Duplicate);
        }
        if self.attempts.len() >= self.capacity {
            return Err(Error::Capacity);
        }
        let old = self
            .attempts
            .iter()
            .find(|a| a.id == previous)
            .ok_or(Error::Invalid)?;
        if !matches!(&old.result, Some(WriteResult::Rejected(error)) if effects_free(error))
            || self.attempts.iter().rposition(|a| a.leg == old.leg)
                != self.attempts.iter().position(|a| a.id == previous)
        {
            return Err(Error::RetryForbidden);
        }
        let leg = old.leg;
        self.attempts.push(WriteAttempt {
            id,
            leg,
            released: false,
            result: None,
        });
        Ok(())
    }
    pub(crate) fn latest(&self, leg: u64) -> Option<RequestId> {
        self.attempts
            .iter()
            .rfind(|attempt| attempt.leg == leg)
            .map(|attempt| attempt.id)
    }

    /// A borrow of this flag reserves the validated journal slot across final
    /// admission. Committing the release is then an infallible assignment.
    pub(crate) fn release_flag(&mut self, id: RequestId) -> Result<&mut bool, Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let attempt = self
            .attempts
            .iter_mut()
            .find(|attempt| attempt.id == id)
            .ok_or(Error::Invalid)?;
        if attempt.released || attempt.result.is_some() {
            return Err(Error::State);
        }
        Ok(&mut attempt.released)
    }

    pub fn released(&mut self, id: RequestId) -> Result<(), Error> {
        *self.release_flag(id)? = true;
        Ok(())
    }
    pub fn result(&mut self, id: RequestId, result: WriteResult) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let attempt = self
            .attempts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(Error::Invalid)?;
        if attempt.result.is_some()
            || (!attempt.released && !matches!(result, WriteResult::Failed(_)))
        {
            return Err(Error::State);
        }
        attempt.result = Some(result);
        Ok(())
    }
    /// Seal success only when every terminal logical leg is proven accepted.
    /// Refused/failed/unresolved attempts remain available to the failure join;
    /// a venue's optimistic done callback cannot erase their evidence.
    pub fn succeed(&mut self) -> Result<(), Error> {
        if self.sealed || self.attempts.is_empty() {
            return Err(Error::State);
        }
        let accepted = self.attempts.iter().enumerate().all(|(index, attempt)| {
            self.attempts[index + 1..]
                .iter()
                .any(|later| later.leg == attempt.leg)
                || (attempt.released
                    && matches!(attempt.result, Some(WriteResult::Accepted { .. })))
        });
        if !accepted {
            return Err(Error::State);
        }
        self.sealed = true;
        Ok(())
    }

    /// Seal exactly once. A caller's fallback text never chooses the outcome;
    /// sent-leg errors have priority over pre-send siblings. Earlier release
    /// evidence survives a signer/transport failure on a newer physical retry.
    pub fn fail(&mut self, fallback: VenueError) -> Result<VenueError, Error> {
        if self.sealed {
            return Err(Error::State);
        }
        self.sealed = true;
        let any_released = self.attempts.iter().any(|a| a.released);
        let uncertain = self.attempts.iter().enumerate().any(|(index, a)| {
            a.released
                && (matches!(a.result, None | Some(WriteResult::Failed(_)))
                    || self.attempts[index + 1..]
                        .iter()
                        .any(|later| later.leg == a.leg && !later.released))
        });
        let all_rejected = any_released
            && self
                .attempts
                .iter()
                .enumerate()
                .filter(|(_, a)| a.released)
                .all(|(index, a)| {
                    matches!(&a.result, Some(WriteResult::Rejected(error)) if effects_free(error))
                        && !self.attempts[index + 1..]
                            .iter()
                            .any(|later| later.leg == a.leg && !later.released)
                });
        let error = |a: &WriteAttempt| match &a.result {
            Some(WriteResult::Rejected(error) | WriteResult::Failed(error)) => Some(error.clone()),
            _ => None,
        };
        // Choose the terminal physical attempt of the earliest logical leg,
        // preferring legs that reached the venue. Callback order is irrelevant.
        let terminal = |index: usize, a: &WriteAttempt| {
            !self.attempts[index + 1..]
                .iter()
                .any(|later| later.leg == a.leg)
        };
        let select = |sent_only: bool| {
            self.attempts
                .iter()
                .enumerate()
                .filter(|(index, a)| {
                    terminal(*index, a)
                        && (!sent_only
                            || self
                                .attempts
                                .iter()
                                .any(|other| other.leg == a.leg && other.released))
                })
                .filter_map(|(_, a)| error(a).map(|error| (a.leg, error)))
                .min_by_key(|(leg, _)| *leg)
                .map(|(_, error)| error)
        };
        let mut error = select(true).or_else(|| select(false)).unwrap_or(fallback);
        error.outcome = if !any_released {
            Some(Outcome::NotSent)
        } else if uncertain {
            Some(Outcome::Unknown)
        } else if all_rejected {
            Some(Outcome::Rejected)
        } else {
            // Definitively accepted effects plus known refusals retain the
            // existing partial contract; do not invent execution uncertainty.
            None
        };
        let mut ids = error
            .partial
            .as_ref()
            .map_or_else(Vec::new, |p| p.order_ids.clone());
        for attempt in &self.attempts {
            let order_ids = match &attempt.result {
                Some(WriteResult::Accepted { order_ids }) => Some(order_ids.as_slice()),
                Some(WriteResult::Rejected(error) | WriteResult::Failed(error)) => {
                    error.partial.as_ref().map(|p| p.order_ids.as_slice())
                }
                None => None,
            };
            if let Some(order_ids) = order_ids {
                for id in order_ids {
                    if !ids.contains(id) {
                        ids.push(id.clone());
                    }
                }
            }
        }
        if !ids.is_empty() {
            if let Some(partial) = error.partial.as_mut() {
                partial.order_ids = ids;
            } else {
                error = error.partial_ids(&ids);
            }
        }
        Ok(error)
    }

    /// Retain each terminal logical-leg refusal for the existing reporting path.
    /// A refusal recovered by a later attempt is history, not a rejected leg.
    /// Reading evidence never reopens a sealed command or emits another result.
    pub fn rejections(&self) -> impl Iterator<Item = (RequestId, &VenueError)> {
        self.attempts
            .iter()
            .enumerate()
            .filter(|(index, attempt)| {
                !self.attempts[index + 1..]
                    .iter()
                    .any(|later| later.leg == attempt.leg)
            })
            .filter_map(|(_, attempt)| match &attempt.result {
                Some(WriteResult::Rejected(error)) => Some((attempt.id, error)),
                _ => None,
            })
    }
}

fn effects_free(error: &VenueError) -> bool {
    error
        .partial
        .as_ref()
        .is_none_or(|p| p.order_ids.is_empty())
}

#[cfg(test)]
#[path = "request_context_tests.rs"]
mod tests;
