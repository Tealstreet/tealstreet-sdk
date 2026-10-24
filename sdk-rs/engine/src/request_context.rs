//! Semantic HTTP ownership and the final controllable transport handoff.
//! Whole-plan and Preflight APIs require the `http-plans` Cargo feature.
//!
//! This module owns no requests, credentials, clocks or network executor. A host
//! prepares its route first; the engine checks a single-use permit immediately
//! before that prepared executor hands the request to the transport API.
use crate::signing::{KeyRef, SignReceipt};
use crate::{CommandId, CredentialSlot, Outcome, RequestId, VenueError};

#[cfg(feature = "http-plans")]
#[path = "request_context_plan.rs"]
pub mod plan;

#[cfg(feature = "http-plans")]
#[path = "request_context_owner.rs"]
mod ownership;
#[cfg(feature = "http-plans")]
pub use ownership::{
    HttpOwnerRetirement, Plan, PlanPolicy, Preflight, PreflightPolicy, PreflightPurpose,
};

#[path = "http_evidence.rs"]
mod evidence;
pub(crate) use evidence::ResponseProof;
pub use evidence::UnprovenEvidence;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Owner {
    Command {
        id: CommandId,
        generation: u64,
    },
    Background {
        generation: u64,
    },
    /// Venue-maintained authentication session (listen key, WS token, login).
    /// Retiring the session revokes every context and attempt it owns.
    Session {
        generation: u64,
    },
    /// One physical execution serving several command observers.
    Execution {
        generation: u64,
    },
}

/// Engine-issued session authority. Holds no key, token or credential value.
#[derive(Clone, Debug, PartialEq)]
pub struct Session {
    issuer: std::sync::Arc<()>,
    generation: u64,
}

impl Session {
    pub(crate) fn new(issuer: &std::sync::Arc<()>, generation: u64) -> Self {
        Self {
            issuer: issuer.clone(),
            generation,
        }
    }
    pub(crate) fn issued_by(&self, issuer: &std::sync::Arc<()>) -> bool {
        std::sync::Arc::ptr_eq(&self.issuer, issuer)
    }
    pub fn owner(&self) -> Owner {
        Owner::Session {
            generation: self.generation,
        }
    }
}

/// A continuation parked behind a session, returned when the venue drains it.
#[derive(Debug)]
pub enum Waiter<P> {
    Ready(Context, P),
    /// Nothing was sent for this owner; the error's outcome is NotSent.
    Failed(Context, P, VenueError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPolicy {
    /// Every context issued for the session must use exactly this credential.
    pub credential: Option<Credential>,
    /// The venue contract makes a new create/keepalive harmless while an
    /// earlier one is unresolved (cite it at the declaration site).
    pub repeat_safe: bool,
    /// Unsigned requests that carry the primary key's login or session token
    /// in their body or headers. Only a keyless session may declare it.
    pub private_egress: Option<PrivateEgress>,
}

/// Granted only by a `SessionAuth` credential selection
/// (`Selection::session_egress`); a venue cannot construct it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateEgress(());

impl PrivateEgress {
    pub(crate) fn granted() -> Self {
        Self(())
    }
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

/// Egress declared at trusted issuance, independent of HTTP method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Egress {
    /// Primary account egress: keyless reads, or confirming a primary credential.
    Primary,
    /// Unsigned public reads through the host's public CORS egress.
    PublicCors,
    /// Unsigned public reads sent directly, bypassing account proxies.
    Direct,
    /// Primary private egress for unsigned requests carrying a session's
    /// login or token. Minted only by the session core; routing only.
    Session(SessionEgress),
}

/// Opaque: no constructor outside the engine, so a venue cannot forge it.
/// It names no key and grants no write, signing or session authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionEgress {
    generation: u64,
}

impl SessionEgress {
    pub(crate) fn new(generation: u64) -> Self {
        Self { generation }
    }
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
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
    /// Legs of an execution; issued only beneath its engine-issued root.
    Execution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credential {
    pub slot: CredentialSlot,
    pub reference: KeyRef,
}

impl Credential {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.reference.0.is_empty() || self.reference.0.len() > 256 {
            Err(Error::Invalid)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pacing {
    pub group: u32,
    pub minimum_gap_ms: f64,
}

/// A flat declaration over primitive pacing groups, resolved at engine construction.
#[derive(Clone, Copy, Debug)]
pub struct PacingSet {
    pub group: u32,
    pub constraints: &'static [Pacing],
}

/// Immutable across physical retries of a logical leg.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    issuer: Option<std::sync::Arc<()>>,
    owner: Owner,
    leg: u64,
    operation: Operation,
    credential: Option<Credential>,
    /// Egress declared at issuance; it may only confirm a credential's intent.
    egress: Option<Egress>,
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

/// A first-command admission failure, distinct from an issued Context failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandEntryError {
    /// The engine already published the exact unissued command's failure.
    Rejected(Error),
    /// No fresh genuine-command loan; this grants no terminal authority.
    Revoked,
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
            || credential.as_ref().is_some_and(|c| c.validate().is_err())
        {
            return Err(Error::Invalid);
        }
        Ok(Self {
            issuer: None,
            owner,
            leg,
            operation,
            credential,
            egress: None,
            deadline,
            pacing,
            retry: RetryPolicy::Never,
        })
    }

    /// Capture egress before issuer binding: unsigned reads declare it, a
    /// credentialed context may only confirm a primary slot. Descendants keep it;
    /// it never grants credential, nonce or identity authority.
    pub(crate) fn with_egress(mut self, egress: Option<Egress>) -> Result<Self, Error> {
        let Some(egress) = egress else {
            return Ok(self);
        };
        let valid = match (self.credential.as_ref(), egress) {
            (Some(_), Egress::Session(_)) => false,
            (Some(credential), egress) => {
                egress == Egress::Primary && credential.slot == CredentialSlot::Primary
            }
            (None, Egress::Session(_)) => true,
            (None, _) => self.operation == Operation::Read,
        };
        if !valid {
            return Err(Error::Invalid);
        }
        self.egress = Some(egress);
        Ok(self)
    }

    pub(crate) fn egress(&self) -> Option<Egress> {
        self.egress
    }

    /// Explicit host route for the managed Prepare packet; None keeps the
    /// host's intent-based routing.
    pub fn route(&self) -> Option<crate::RequestRoute> {
        match self.egress? {
            Egress::Primary | Egress::Session(_) => None,
            Egress::PublicCors => Some(crate::RequestRoute::PublicCors),
            Egress::Direct => Some(crate::RequestRoute::Direct),
        }
    }

    fn credential_intent(&self) -> RoutingIntent {
        match self.credential.as_ref().map(|credential| credential.slot) {
            None => RoutingIntent::Public,
            Some(CredentialSlot::Primary) => RoutingIntent::Primary,
            Some(CredentialSlot::Secondary) => RoutingIntent::Secondary,
        }
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
    pub(crate) fn same_issuer(&self, other: &Self) -> bool {
        match (&self.issuer, &other.issuer) {
            (Some(left), Some(right)) => std::sync::Arc::ptr_eq(left, right),
            _ => false,
        }
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
        match self.egress {
            Some(Egress::Primary | Egress::Session(_)) => RoutingIntent::Primary,
            Some(Egress::PublicCors | Egress::Direct) => RoutingIntent::Public,
            None => self.credential_intent(),
        }
    }
    pub fn deadline(&self) -> f64 {
        self.deadline
    }
    pub(crate) fn pacing(&self) -> Option<Pacing> {
        self.pacing
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
    pub(crate) fn retry(&self, evidence: RetryEvidence, now: f64) -> Result<Self, Error> {
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
    BindGroup {
        permit: GroupPermit,
        members: Vec<GroupPacket>,
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
        let signature_valid_until = crate::SigningRelease::new(signature, time)?
            .raw_expiry_ms()
            .map(|projected| deadline.min(projected));
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
    if request == 0 || request > crate::resource_ids::MAX_RESOURCE_ID || generation == 0 {
        return Err(Error::Invalid);
    }
    Ok(Attempt {
        request: RequestId(request),
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
    deadline: f64,
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

#[path = "request_context_group_protocol.rs"]
mod group_protocol;
pub use group_protocol::{GroupClaim, GroupPacket, GroupPermit, GroupRelease};

#[path = "request_context_group_lifecycle.rs"]
pub(crate) mod group_lifecycle;

impl Lifecycle {
    pub fn new(context: Context, attempt: Attempt) -> Self {
        Self {
            deadline: context.deadline(),
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

    /// Pure physical-window metadata, never engine issuance or retry authority.
    /// A plan may narrow the logical bound; route/signature renewal cannot
    /// extend this attempt's original deadline.
    pub fn with_deadline(
        context: Context,
        attempt: Attempt,
        created_at: f64,
        deadline: f64,
    ) -> Result<Self, Error> {
        if !created_at.is_finite()
            || !deadline.is_finite()
            || deadline <= created_at
            || deadline > context.deadline()
        {
            return Err(Error::Invalid);
        }
        let mut lifecycle = Self::new(context, attempt);
        lifecycle.deadline = deadline;
        Ok(lifecycle)
    }

    /// Immutable physical cap; it never grants more time than the Context.
    pub fn deadline(&self) -> f64 {
        self.deadline
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

    /// Single requests retain their existing mutable admission hook.
    pub fn claim(
        &mut self,
        permit: Permit,
        time: ReleaseTime,
        pacing: &mut SendPacing,
        admit: &mut dyn FnMut(&HttpRelease) -> Result<Admission, Error>,
    ) -> Result<Claim, Error> {
        let draft = match self.prepare_claim(permit, time, pacing)? {
            group_lifecycle::ClaimPreparation::WaitUntil(at) => return Ok(Claim::WaitUntil(at)),
            group_lifecycle::ClaimPreparation::Ready(draft) => draft,
        };
        let ready = match admit(draft.release()).and_then(|admission| draft.admit(admission)) {
            Ok(ready) => ready,
            Err(error) => {
                self.state = State::Settled(None);
                return Err(error);
            }
        };
        Ok(Claim::Released(self.commit_claim(ready, pacing)))
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
        Ok(now >= self.deadline && self.settle())
    }
}

/// Fixed per-engine group floors. A request may extend the gap after its own
/// handoff, never weaken the group floor or erase the previous release's gap.
#[derive(Default)]
pub struct SendPacing {
    groups: Vec<(Pacing, Option<f64>)>,
    sets: Vec<(u32, Vec<PacingConstraint>)>,
}
#[derive(Clone, Copy)]
struct PacingConstraint {
    index: usize,
    minimum_gap_ms: f64,
}
struct ResolvedPacing<'a> {
    single: Option<PacingConstraint>,
    members: &'a [PacingConstraint],
    outer_gap_ms: f64,
}
impl ResolvedPacing<'_> {
    fn constraints(&self) -> impl Iterator<Item = PacingConstraint> + '_ {
        self.single
            .into_iter()
            .chain(self.members.iter().copied())
            .map(|mut member| {
                member.minimum_gap_ms = member.minimum_gap_ms.max(self.outer_gap_ms);
                member
            })
    }
}
fn resolve_pacing<'a>(
    groups: &[(Pacing, Option<f64>)],
    sets: &'a [(u32, Vec<PacingConstraint>)],
    pacing: Option<Pacing>,
) -> Result<ResolvedPacing<'a>, Error> {
    let mut resolved = ResolvedPacing {
        single: None,
        members: &[],
        outer_gap_ms: 0.,
    };
    let Some(pacing) = pacing else {
        return Ok(resolved);
    };
    if !pacing.minimum_gap_ms.is_finite() || pacing.minimum_gap_ms < 0. {
        return Err(Error::Invalid);
    }
    resolved.outer_gap_ms = pacing.minimum_gap_ms;
    if let Some(index) = groups
        .iter()
        .position(|(group, _)| group.group == pacing.group)
    {
        resolved.single = Some(PacingConstraint {
            index,
            minimum_gap_ms: groups[index].0.minimum_gap_ms,
        });
    } else {
        resolved.members = &sets
            .iter()
            .find(|(id, _)| *id == pacing.group)
            .ok_or(Error::Invalid)?
            .1;
    }
    Ok(resolved)
}
impl SendPacing {
    pub fn new(groups: &[Pacing]) -> Result<Self, Error> {
        Self::with_sets(groups, &[])
    }

    pub fn with_sets(groups: &[Pacing], sets: &[PacingSet]) -> Result<Self, Error> {
        const SET_CAPACITY: usize = 16;
        if sets.len() > SET_CAPACITY || sets.iter().any(|set| set.constraints.len() > SET_CAPACITY)
        {
            return Err(Error::Capacity);
        }
        if groups
            .iter()
            .any(|p| !p.minimum_gap_ms.is_finite() || p.minimum_gap_ms < 0.)
            || sets.iter().any(|set| {
                set.constraints.is_empty()
                    || set
                        .constraints
                        .iter()
                        .any(|p| !p.minimum_gap_ms.is_finite() || p.minimum_gap_ms < 0.)
            })
        {
            return Err(Error::Invalid);
        }
        if groups
            .iter()
            .enumerate()
            .any(|(i, p)| groups[..i].iter().any(|q| q.group == p.group))
            || sets.iter().enumerate().any(|(i, set)| {
                groups.iter().any(|p| p.group == set.group)
                    || sets[..i].iter().any(|other| other.group == set.group)
                    || set
                        .constraints
                        .iter()
                        .enumerate()
                        .any(|(j, p)| set.constraints[..j].iter().any(|q| q.group == p.group))
            })
        {
            return Err(Error::Duplicate);
        }
        let mut registered = Vec::new();
        registered
            .try_reserve_exact(groups.len())
            .map_err(|_| Error::Capacity)?;
        registered.extend(groups.iter().map(|group| (*group, None)));
        let mut resolved_sets = Vec::new();
        resolved_sets
            .try_reserve_exact(sets.len())
            .map_err(|_| Error::Capacity)?;
        for set in sets {
            let mut members = Vec::new();
            members
                .try_reserve_exact(set.constraints.len())
                .map_err(|_| Error::Capacity)?;
            for member in set.constraints {
                let index = groups
                    .iter()
                    .position(|p| p.group == member.group)
                    .ok_or(Error::Invalid)?;
                members.push(PacingConstraint {
                    index,
                    minimum_gap_ms: groups[index].minimum_gap_ms.max(member.minimum_gap_ms),
                });
            }
            resolved_sets.push((set.group, members));
        }
        Ok(Self {
            groups: registered,
            sets: resolved_sets,
        })
    }
    #[cfg(feature = "http-plans")]
    #[cfg(feature = "http-mutation-barriers")]
    pub(crate) fn validate_mutation_group(&self, pacing: Option<Pacing>) -> Result<(), Error> {
        self.validate_zero(pacing)
    }

    /// Canonical expanded-floor validation for admission paths requiring no pacing.
    pub fn validate_zero(&self, pacing: Option<Pacing>) -> Result<(), Error> {
        let resolved = resolve_pacing(&self.groups, &self.sets, pacing)?;
        if resolved
            .constraints()
            .any(|member| member.minimum_gap_ms != 0.)
        {
            return Err(Error::Invalid);
        }
        Ok(())
    }

    pub(crate) fn check(&self, pacing: Option<Pacing>, now: f64) -> Result<Option<f64>, Error> {
        let resolved = resolve_pacing(&self.groups, &self.sets, pacing)?;
        let mut wait: Option<f64> = None;
        for member in resolved.constraints() {
            if let Some(at) = self.groups[member.index].1.filter(|at| now < *at) {
                wait = Some(wait.map_or(at, |previous| previous.max(at)));
            }
        }
        Ok(wait)
    }
    fn released(&mut self, pacing: Option<Pacing>, now: f64) {
        let resolved =
            resolve_pacing(&self.groups, &self.sets, pacing).expect("Claim prevalidated pacing");
        for member in resolved.constraints() {
            self.groups[member.index].1 = Some((now + member.minimum_gap_ms).min(f64::MAX));
        }
    }
}

/// Proof that the engine classified this attempt's refusal as `RefusedResend`.
/// Only `Cx::http_write_fact` creates one, once per logical leg.
#[derive(Debug)]
pub struct ResendGrant {
    pub(crate) attempt: RequestId,
}

/// Native codecs supply definitive facts. HTTP status and exception text are
/// deliberately absent from this contract.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteResult {
    Accepted {
        order_ids: Vec<String>,
    },
    Rejected(VenueError),
    /// Native ACK proves both effects and a definitive refusal, even when it
    /// creates no order IDs (for example, a partially cancelled batch).
    KnownPartial(VenueError),
    /// Native ACK proves a validated subset of effects, while other effects
    /// remain unresolved. Publish only that subset; never retry the whole write.
    PartialUnknown(VenueError),
    Failed(VenueError),
}

impl WriteResult {
    fn order_ids(&self) -> &[String] {
        match self {
            Self::Accepted { order_ids } => order_ids,
            Self::Rejected(error)
            | Self::KnownPartial(error)
            | Self::PartialUnknown(error)
            | Self::Failed(error) => error
                .partial
                .as_ref()
                .map_or(&[], |partial| partial.order_ids.as_slice()),
        }
    }
}

const REPORT_MESSAGES: usize = 256;
const REPORT_MESSAGE_BYTES: usize = 8 * 1024;
const REPORT_BYTES: usize = 64 * 1024;

/// Native presentation facts, separate from execution outcome. Empty explicitly
/// suppresses reports (silent native codes); it never suppresses the write fact.
#[derive(Default, PartialEq)]
pub struct RejectionReports {
    messages: Vec<String>,
    retained_bytes: usize,
}

impl RejectionReports {
    pub fn new(messages: Vec<String>) -> Result<Self, Error> {
        if messages.capacity() > REPORT_MESSAGES {
            return Err(Error::Capacity);
        }
        let mut retained_bytes = messages
            .capacity()
            .checked_mul(std::mem::size_of::<String>())
            .ok_or(Error::Capacity)?;
        for message in &messages {
            if message.capacity() > REPORT_MESSAGE_BYTES {
                return Err(Error::Capacity);
            }
            retained_bytes = retained_bytes
                .checked_add(message.capacity())
                .ok_or(Error::Capacity)?;
        }
        if retained_bytes > REPORT_BYTES {
            return Err(Error::Capacity);
        }
        Ok(Self {
            messages,
            retained_bytes,
        })
    }
}

impl std::fmt::Debug for RejectionReports {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RejectionReports(<redacted>)")
    }
}

struct WriteAttempt {
    id: RequestId,
    // A classified RefusedResend verdict: the only evidence one resend may use.
    resend_granted: bool,
    leg: u64,
    released: bool,
    result: Option<WriteResult>,
    reports: RejectionReports,
    id_overflow: bool,
}

struct WriteLeg {
    id: u64,
    // Stable append-only indexes; ordinal preserves the declared child order.
    parent: Option<(usize, usize)>,
    superseded: bool,
}

/// Bounded evidence retained for the whole command, including prior retries.
/// Registration order is logical planning order, never callback arrival order.
pub struct CommandWrites {
    attempts: Vec<WriteAttempt>,
    legs: Vec<WriteLeg>,
    capacity: usize,
    sealed: bool,
    report_bytes: usize,
    report_messages: usize,
    evidence: evidence::EvidenceBudget,
    // An earlier attempt of this intent may have reached the venue.
    inherited_uncertain: bool,
    stream: Option<Box<StreamedWrites>>,
}

/// A streamed serial Plan's settled calls: accepted IDs exactly as received,
/// refusals counted, and refusal reports kept only while the text budget lasts.
#[derive(Default)]
#[cfg_attr(not(feature = "http-plans"), allow(dead_code))]
struct StreamedWrites {
    refusals_complete: bool,
    ids: Vec<String>,
    settled: usize,
    refused: usize,
    effects: bool,
    retained: Vec<(RequestId, VenueError, RejectionReports)>,
    retained_bytes: usize,
}
// Private synchronous terminal preparation. Never exposed as a venue grant.
#[cfg(feature = "http-plans")]
pub(crate) struct PreparedWriteSeal {
    failure: Option<VenueError>,
    evidence: evidence::EvidenceBudget,
}
#[cfg(feature = "http-plans")]
impl PreparedWriteSeal {
    pub(crate) fn is_failure(&self) -> bool {
        self.failure.is_some()
    }

    pub(crate) fn failure(&self) -> Option<&VenueError> {
        self.failure.as_ref()
    }
}
impl CommandWrites {
    /// Prepare the full declared journal before plan admission. Logical legs
    /// include unselected fallback branches; neither registration nor retry
    /// may grow this backing after the plan's resources have committed.
    #[cfg(any(test, feature = "http-plans"))]
    pub(crate) fn new_reserved(capacity: usize, logical_legs: usize) -> Result<Self, Error> {
        let mut writes = Self::new(capacity)?;
        if logical_legs == 0 || logical_legs > capacity {
            return Err(Error::Capacity);
        }
        writes
            .attempts
            .try_reserve_exact(capacity)
            .map_err(|_| Error::Capacity)?;
        // Existing journal registration/replacement permissions remain intact.
        // Reserve their full shared upper bound too, so even an underdeclared
        // logical shape cannot trigger later growth of this backing.
        writes
            .legs
            .try_reserve_exact(capacity)
            .map_err(|_| Error::Capacity)?;
        Ok(writes)
    }

    pub fn new(capacity: usize) -> Result<Self, Error> {
        if capacity == 0 || capacity > evidence::MAX_ATTEMPTS {
            return Err(Error::Capacity);
        }
        Ok(Self {
            attempts: Vec::new(),
            legs: Vec::new(),
            capacity,
            sealed: false,
            report_bytes: 0,
            report_messages: 0,
            evidence: evidence::EvidenceBudget::default(),
            inherited_uncertain: false,
            stream: None,
        })
    }
    pub fn register(&mut self, id: RequestId, leg: u64) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        if self.attempts.iter().any(|a| a.id == id || a.leg == leg) {
            return Err(Error::Duplicate);
        }
        if let Some(reserved) = self.legs.iter().find(|entry| entry.id == leg) {
            if reserved.superseded {
                return Err(Error::Revoked);
            }
            // The first physical attempt consumes this already-counted slot.
        } else {
            if self.occupied() >= self.capacity {
                return Err(Error::Capacity);
            }
            self.legs.push(WriteLeg {
                id: leg,
                parent: None,
                superseded: false,
            });
        }
        self.attempts.push(WriteAttempt {
            id,
            leg,
            released: false,
            result: None,
            reports: RejectionReports::default(),
            id_overflow: false,
            resend_granted: false,
        });
        Ok(())
    }

    /// Only a typed definitive refusal may authorize another write attempt.
    /// The request context's policy/deadline must also authorize this retry.
    pub fn retry(&mut self, previous: RequestId, id: RequestId) -> Result<(), Error> {
        if self.attempts.iter().any(|a| a.id == id) && !self.sealed {
            return Err(Error::Duplicate);
        }
        let leg = self.retry_admissible(previous)?;
        // Each grant authorizes exactly one resend.
        if let Some(old) = self.attempts.iter_mut().find(|a| a.id == previous) {
            old.resend_granted = false;
        }
        self.attempts.push(WriteAttempt {
            id,
            leg,
            released: false,
            result: None,
            reports: RejectionReports::default(),
            id_overflow: false,
            resend_granted: false,
        });
        Ok(())
    }

    /// Reserve every replacement leaf before superseding an effects-free
    /// refused batch. The registry supplies engine-issued child leg identities.
    /// This preserves physical history; it does not grant transport authority.
    pub fn replace(&mut self, previous: RequestId, children: &[u64]) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let old = self
            .attempts
            .iter()
            .find(|attempt| attempt.id == previous)
            .ok_or(Error::Invalid)?;
        if !old.released
            || old.id_overflow
            || !matches!(&old.result, Some(WriteResult::Rejected(error)) if effects_free(error))
            || self.latest(old.leg) != Some(previous)
        {
            return Err(Error::RetryForbidden);
        }
        if children.is_empty() {
            return Err(Error::Invalid);
        }
        if self
            .occupied()
            .checked_add(children.len())
            .is_none_or(|count| count > self.capacity)
        {
            return Err(Error::Capacity);
        }
        for (index, child) in children.iter().enumerate() {
            if children[..index].contains(child) || self.legs.iter().any(|leg| leg.id == *child) {
                return Err(Error::Duplicate);
            }
        }
        let parent = self
            .legs
            .iter()
            .position(|leg| leg.id == old.leg)
            .expect("physical attempts have logical legs");
        self.legs
            .try_reserve(children.len())
            .map_err(|_| Error::Capacity)?;
        // No fallible operation after this point; failed admission retains the
        // original refusal and leaves its delivery authority unsuperseded.
        self.legs[parent].superseded = true;
        self.legs
            .extend(children.iter().enumerate().map(|(ordinal, id)| WriteLeg {
                id: *id,
                parent: Some((parent, ordinal)),
                superseded: false,
            }));
        Ok(())
    }

    /// Failure-only continuation for a direct replacement that never sent.
    /// The active-parent lookup intentionally cannot authorize this old delivery.
    pub(crate) fn unreleased_replacement_child(
        &self,
        previous: RequestId,
        parent_leg: u64,
        child: u64,
    ) -> bool {
        if self.sealed {
            return false;
        }
        let Some(old) = self.attempts.iter().find(|attempt| attempt.id == previous) else {
            return false;
        };
        if old.leg != parent_leg
            || !old.released
            || old.id_overflow
            || !matches!(&old.result, Some(WriteResult::Rejected(error)) if effects_free(error))
            || self
                .attempts
                .iter()
                .rfind(|attempt| attempt.leg == old.leg)
                .is_none_or(|attempt| attempt.id != previous)
            || self
                .attempts
                .iter()
                .any(|attempt| attempt.leg == child && attempt.released)
        {
            return false;
        }
        let Some(parent) = self
            .legs
            .iter()
            .position(|leg| leg.id == old.leg && leg.superseded)
        else {
            return false;
        };
        self.legs.iter().any(|leg| {
            leg.id == child
                && !leg.superseded
                && leg.parent.is_some_and(|(index, _)| index == parent)
        })
    }

    fn occupied(&self) -> usize {
        self.attempts.len()
            + self
                .legs
                .iter()
                .filter(|leg| !self.attempts.iter().any(|attempt| attempt.leg == leg.id))
                .count()
    }

    fn active(&self, leg: u64) -> bool {
        self.legs
            .iter()
            .any(|entry| entry.id == leg && !entry.superseded)
    }

    pub(crate) fn superseded(&self, leg: u64) -> bool {
        self.legs
            .iter()
            .any(|entry| entry.id == leg && entry.superseded)
    }

    fn terminal(&self, attempt: &WriteAttempt) -> bool {
        self.latest(attempt.leg) == Some(attempt.id)
    }

    fn order(&self, leg: u64) -> Vec<u64> {
        let mut index = self
            .legs
            .iter()
            .position(|entry| entry.id == leg)
            .expect("known leg");
        let mut path = Vec::new();
        while let Some((parent, ordinal)) = self.legs[index].parent {
            path.push(ordinal as u64);
            index = parent;
        }
        path.push(self.legs[index].id);
        path.reverse();
        path
    }

    fn ordered_attempts(&self) -> Vec<&WriteAttempt> {
        let mut attempts: Vec<_> = self.attempts.iter().collect();
        attempts.sort_by_cached_key(|attempt| self.order(attempt.leg));
        attempts
    }

    fn reached(&self, leg: u64) -> bool {
        let mut index = self
            .legs
            .iter()
            .position(|entry| entry.id == leg)
            .expect("known leg");
        loop {
            if self
                .attempts
                .iter()
                .any(|attempt| attempt.leg == self.legs[index].id && attempt.released)
            {
                return true;
            }
            match self.legs[index].parent {
                Some((parent, _)) => index = parent,
                None => return false,
            }
        }
    }

    /// Journal side of `retry`, without allocating the new attempt.
    fn retry_admissible(&self, previous: RequestId) -> Result<u64, Error> {
        if self.sealed {
            return Err(Error::State);
        }
        if self.occupied() >= self.capacity {
            return Err(Error::Capacity);
        }
        let old = self
            .attempts
            .iter()
            .find(|a| a.id == previous)
            .ok_or(Error::Invalid)?;
        if !self.active(old.leg)
            || old.id_overflow
            || !old.resend_granted
            || !matches!(&old.result, Some(WriteResult::Rejected(error)) if effects_free(error))
            || self.attempts.iter().rposition(|a| a.leg == old.leg)
                != self.attempts.iter().position(|a| a.id == previous)
        {
            return Err(Error::RetryForbidden);
        }
        Ok(old.leg)
    }

    /// An owner may park for a re-send only when `retry` would accept it.
    pub(crate) fn resend_blocked(&self, leg: u64) -> bool {
        self.latest(leg)
            .is_some_and(|latest| self.retry_admissible(latest).is_err())
    }

    pub(crate) fn leg_attempts(&self, leg: u64) -> usize {
        self.attempts.iter().filter(|a| a.leg == leg).count()
    }

    pub(crate) fn latest(&self, leg: u64) -> Option<RequestId> {
        if !self.active(leg) {
            return None;
        }
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

    /// Mark a recorded effects-free refusal as resendable once. Only the
    /// engine classifier calls this, for a `RefusedResend` verdict.
    pub(crate) fn grant_resend(&mut self, id: RequestId) -> Result<(), Error> {
        let attempt = self
            .attempts
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(Error::Invalid)?;
        if !matches!(&attempt.result, Some(WriteResult::Rejected(error)) if effects_free(error)) {
            return Err(Error::State);
        }
        attempt.resend_granted = true;
        Ok(())
    }

    pub fn released(&mut self, id: RequestId) -> Result<(), Error> {
        *self.release_flag(id)? = true;
        Ok(())
    }
    pub fn result(&mut self, id: RequestId, result: WriteResult) -> Result<(), Error> {
        let reports = match &result {
            WriteResult::Rejected(error) | WriteResult::KnownPartial(error) => {
                RejectionReports::new(vec![evidence::default_report(&error.message)])?
            }
            _ => RejectionReports::default(),
        };
        // Default presentation must not discard native money facts when its
        // reporting quota is full. Explicit report admission stays atomic below.
        let reports = if self.report_bytes + reports.retained_bytes <= REPORT_BYTES
            && self.report_messages + reports.messages.len() <= REPORT_MESSAGES
        {
            reports
        } else {
            RejectionReports::default()
        };
        self.result_with_reports(id, result, reports)
    }

    /// Record an unproven released response using the same retained-evidence
    /// quota. No native callback runs until the attempt has passed admission.
    pub(crate) fn unproven_result(
        &mut self,
        id: RequestId,
        status: u16,
        capture: impl FnOnce(&mut UnprovenEvidence),
    ) -> Result<(), Error> {
        self.capture_unproven_result(id, |budget| UnprovenEvidence::new(budget, status), capture)
    }

    pub(crate) fn unproven_native_result(
        &mut self,
        id: RequestId,
        capture: impl FnOnce(&mut UnprovenEvidence),
    ) -> Result<(), Error> {
        self.capture_unproven_result(id, |budget| Ok(UnprovenEvidence::native(budget)), capture)
    }

    fn capture_unproven_result(
        &mut self,
        id: RequestId,
        writer: impl FnOnce(evidence::EvidenceBudget) -> Result<UnprovenEvidence, Error>,
        capture: impl FnOnce(&mut UnprovenEvidence),
    ) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let index = self
            .attempts
            .iter()
            .position(|attempt| attempt.id == id)
            .ok_or(Error::Invalid)?;
        let attempt = &self.attempts[index];
        if self.latest(attempt.leg) != Some(id) {
            return Err(Error::Revoked);
        }
        if !attempt.released || attempt.result.is_some() {
            return Err(Error::State);
        }
        let mut evidence = writer(self.evidence)?;
        capture(&mut evidence);
        let (budget, error, overflow) = evidence.finish();
        self.attempts[index].result = Some(WriteResult::Failed(error));
        self.attempts[index].id_overflow = overflow;
        self.evidence = budget;
        Ok(())
    }

    /// Atomic native fact plus bounded presentation metadata. A metadata refusal
    /// leaves the attempt untouched; callers may record the same fact with empty
    /// reports. Provenance/state failures are never a reason to bypass authority.
    pub fn result_with_reports(
        &mut self,
        id: RequestId,
        result: WriteResult,
        reports: RejectionReports,
    ) -> Result<(), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        if matches!(result, WriteResult::Accepted { .. }) && !reports.messages.is_empty() {
            return Err(Error::Invalid);
        }
        let report_bytes = self
            .report_bytes
            .checked_add(reports.retained_bytes)
            .filter(|bytes| *bytes <= REPORT_BYTES)
            .ok_or(Error::Capacity)?;
        let report_messages = self
            .report_messages
            .checked_add(reports.messages.len())
            .filter(|count| *count <= REPORT_MESSAGES)
            .ok_or(Error::Capacity)?;
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
        let (budget, result, id_overflow) = self.evidence.result(result);
        attempt.result = Some(result);
        attempt.id_overflow = id_overflow;
        attempt.reports = reports;
        self.evidence = budget;
        self.report_bytes = report_bytes;
        self.report_messages = report_messages;
        Ok(())
    }
    /// Publication follows this attempt's normalized native facts, not the
    /// aggregate outcome of its siblings. This check consumes no authority.
    pub(crate) fn validate_effects(&self, id: RequestId) -> Result<(), Error> {
        let attempt = self.validate_recorded_attempt(id)?;
        if !matches!(
            attempt.result,
            Some(
                WriteResult::Accepted { .. }
                    | WriteResult::KnownPartial(_)
                    | WriteResult::PartialUnknown(_)
            )
        ) {
            return Err(Error::State);
        }
        Ok(())
    }

    pub(crate) fn validate_housekeeping(&self, id: RequestId) -> Result<(), Error> {
        self.validate_recorded_attempt(id).map(|_| ())
    }

    fn validate_recorded_attempt(&self, id: RequestId) -> Result<&WriteAttempt, Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let attempt = self
            .attempts
            .iter()
            .find(|a| a.id == id)
            .ok_or(Error::Invalid)?;
        if self.superseded(attempt.leg) || self.latest(attempt.leg) != Some(id) {
            return Err(Error::Revoked);
        }
        if !attempt.released || attempt.id_overflow || attempt.result.is_none() {
            return Err(Error::State);
        }
        Ok(attempt)
    }

    /// Failure-only structural progress. This grants no publication or retry
    /// authority, including for evidence-only IDs on a failed native reply.
    #[cfg(feature = "http-plans")]
    pub(crate) fn validate_failed_attempt(&self, id: RequestId) -> Result<(), Error> {
        if self.failed_attempt(id)?.id_overflow {
            return Err(Error::State);
        }
        Ok(())
    }

    #[cfg(feature = "http-plans")]
    fn failed_attempt(&self, id: RequestId) -> Result<&WriteAttempt, Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let attempt = self
            .attempts
            .iter()
            .find(|attempt| attempt.id == id)
            .ok_or(Error::Invalid)?;
        if self.superseded(attempt.leg) || self.latest(attempt.leg) != Some(id) {
            return Err(Error::Revoked);
        }
        if !matches!(attempt.result, Some(WriteResult::Failed(_))) {
            return Err(Error::State);
        }
        Ok(attempt)
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn failed_attempt_error(&self, id: RequestId) -> Result<VenueError, Error> {
        let attempt = self.failed_attempt(id)?;
        let Some(WriteResult::Failed(error)) = &attempt.result else {
            return Err(Error::State);
        };
        let mut error = error.clone();
        if attempt.id_overflow {
            evidence::mark_id_overflow(&mut error);
        }
        Ok(error)
    }

    /// Seal success only when every terminal logical leg is proven accepted.
    /// Refused/failed/unresolved attempts remain available to the failure join;
    /// a venue's optimistic done callback cannot erase their evidence.
    pub fn succeed(&mut self) -> Result<(), Error> {
        self.validate_success()?;
        self.sealed = true;
        Ok(())
    }

    fn validate_success(&self) -> Result<(), Error> {
        if let Some(stream) = &self.stream {
            return self.validate_streamed_success(stream);
        }
        if self.sealed || self.attempts.is_empty() || self.attempts.iter().any(|a| a.id_overflow) {
            return Err(Error::State);
        }
        let accepted = self.legs.iter().filter(|leg| !leg.superseded).all(|leg| {
            self.latest(leg.id)
                .and_then(|id| self.attempts.iter().find(|attempt| attempt.id == id))
                .is_some_and(|attempt| {
                    attempt.released && matches!(attempt.result, Some(WriteResult::Accepted { .. }))
                })
        });
        if !accepted {
            return Err(Error::State);
        }
        Ok(())
    }

    /// Carry an owner's inherited maybe-sent fact; it can only raise a failure
    /// projection to Unknown and never clears.
    pub(crate) fn inheriting(mut self, uncertain: bool) -> Self {
        self.inherited_uncertain |= uncertain;
        self
    }

    /// Seal exactly once. A caller's fallback text never chooses the outcome;
    /// sent-leg errors have priority over pre-send siblings. Earlier release
    /// evidence survives a signer/transport failure on a newer physical retry.
    pub fn fail(&mut self, fallback: VenueError) -> Result<VenueError, Error> {
        let (error, budget) = self.failure_projection(fallback)?;
        self.evidence = budget;
        self.sealed = true;
        Ok(error)
    }

    /// Project the current physical facts without sealing or charging fallback
    /// evidence. This grants no completion, retry or observer authority. The
    /// execution owner supplies presentation-only observer termination errors.
    pub fn project_failure(&self, fallback: VenueError) -> Result<VenueError, Error> {
        self.failure_projection(fallback).map(|(error, _)| error)
    }

    fn failure_projection(
        &self,
        fallback: VenueError,
    ) -> Result<(VenueError, evidence::EvidenceBudget), Error> {
        if self.sealed {
            return Err(Error::State);
        }
        let mut projected_budget = self.evidence;
        let settled = self.streamed().map_or(0, |stream| stream.settled);
        let any_released = settled > 0 || self.attempts.iter().any(|a| a.released);
        let uncertain = self.attempts.iter().enumerate().any(|(index, a)| {
            a.released
                && (matches!(
                    a.result,
                    None | Some(WriteResult::Failed(_) | WriteResult::PartialUnknown(_))
                ) || self.attempts[index + 1..]
                    .iter()
                    .any(|later| later.leg == a.leg && !later.released))
        }) || self.legs.iter().any(|leg| {
            !leg.superseded
                && leg.parent.is_some()
                && self.latest(leg.id).is_none_or(|id| {
                    self.attempts
                        .iter()
                        .find(|attempt| attempt.id == id)
                        .is_none_or(|attempt| !attempt.released)
                })
        });
        let all_rejected = any_released
            && !self.streamed().is_some_and(|stream| stream.effects)
            && self
                .attempts
                .iter()
                .filter(|attempt| attempt.released && self.terminal(attempt))
                .all(|a| {
                    matches!(&a.result, Some(WriteResult::Rejected(error)) if effects_free(error))
                });
        let error = |a: &WriteAttempt| match &a.result {
            Some(
                WriteResult::Rejected(error)
                | WriteResult::KnownPartial(error)
                | WriteResult::PartialUnknown(error)
                | WriteResult::Failed(error),
            ) => Some(error.clone()),
            _ => None,
        };
        // Choose the terminal physical attempt of the earliest logical leg,
        // preferring legs that reached the venue. Callback order is irrelevant.
        let select = |sent_only: bool| {
            self.attempts
                .iter()
                .filter(|a| self.terminal(a) && (!sent_only || self.reached(a.leg)))
                .filter_map(|a| error(a).map(|error| (self.order(a.leg), error)))
                .min_by(|(a, _), (b, _)| a.cmp(b))
                .map(|(_, error)| error)
        };
        // Settled serial refusals precede every live call in planning order.
        let selected = self
            .streamed()
            .and_then(|stream| stream.retained.first())
            .map(|(_, error, _)| error.clone())
            .or_else(|| select(true))
            .or_else(|| select(false));
        let mut overflow = self.attempts.iter().any(|attempt| attempt.id_overflow);
        let mut error = match selected {
            Some(error) => error, // Already-budgeted facts are never charged again.
            None => {
                let known = self
                    .attempts
                    .iter()
                    .filter_map(|attempt| attempt.result.as_ref())
                    .flat_map(WriteResult::order_ids)
                    .map(String::as_str)
                    .collect();
                let (budget, error, fallback_overflow) = projected_budget.error(fallback, &known);
                projected_budget = budget;
                overflow |= fallback_overflow;
                error
            }
        };
        error.outcome = if self.inherited_uncertain {
            Some(Outcome::Unknown)
        } else if !any_released {
            Some(Outcome::NotSent)
        } else if uncertain || overflow {
            Some(Outcome::Unknown)
        } else if all_rejected {
            Some(Outcome::Rejected)
        } else {
            // Definitively accepted effects plus known refusals retain the
            // existing partial contract; do not invent execution uncertainty.
            None
        };
        // Preserve the selected native/fallback evidence prefix exactly. Append
        // additional facts in logical replacement order, without charging known
        // IDs again or reordering a bounded retained prefix.
        let mut ids = if let Some(stream) = self.streamed() {
            // Native appends every identity exactly as received.
            let mut ids = stream.ids.clone();
            for attempt in self.ordered_attempts() {
                if let Some(result) = &attempt.result {
                    ids.extend(result.order_ids().iter().cloned());
                }
            }
            ids
        } else {
            error
                .partial
                .as_ref()
                .map_or_else(Vec::new, |partial| partial.order_ids.clone())
        };
        if self.streamed().is_none() {
            for attempt in self.ordered_attempts() {
                let order_ids = attempt.result.as_ref().map(WriteResult::order_ids);
                if let Some(order_ids) = order_ids {
                    for id in order_ids {
                        if !ids.contains(id) {
                            ids.push(id.clone());
                        }
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
        if overflow {
            evidence::mark_id_overflow(&mut error);
        }
        if let Some(dropped) = self
            .streamed()
            .map(|stream| stream.refused - stream.retained.len())
            .filter(|dropped| *dropped > 0)
        {
            error.message = format!("{} [truncated: {dropped} more refusals]", error.message);
        }
        Ok((error, projected_budget))
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn prepare_seal(
        &self,
        error: Option<VenueError>,
    ) -> Result<PreparedWriteSeal, Error> {
        if error.is_none() && self.validate_success().is_ok() {
            return Ok(PreparedWriteSeal {
                failure: None,
                evidence: self.evidence,
            });
        }
        let (error, evidence) =
            self.failure_projection(error.unwrap_or_else(|| {
                VenueError::new("Command writes are not all confirmed accepted")
            }))?;
        Ok(PreparedWriteSeal {
            failure: Some(error),
            evidence,
        })
    }

    #[cfg(feature = "http-plans")]
    pub(crate) fn commit_seal(&mut self, prepared: PreparedWriteSeal) -> Option<VenueError> {
        assert!(!self.sealed, "prepared journal seals exactly once");
        self.sealed = true;
        self.evidence = prepared.evidence;
        prepared.failure
    }

    #[cfg(all(test, feature = "http-plans"))]
    pub(crate) fn holds_attempt(&self, id: RequestId) -> bool {
        self.attempts.iter().any(|attempt| attempt.id == id)
    }

    /// Retain each terminal logical-leg refusal for the existing reporting path.
    /// A refusal recovered by a later attempt is history, not a rejected leg.
    /// Reading evidence never reopens a sealed command or emits another result.
    pub fn rejections(&self) -> impl Iterator<Item = (RequestId, &VenueError)> {
        self.streamed_retained()
            .map(|(id, error, _)| (*id, error))
            .chain(
                self.ordered_attempts()
                    .into_iter()
                    .filter(|attempt| self.terminal(attempt))
                    .filter_map(|attempt| match &attempt.result {
                        Some(WriteResult::Rejected(error) | WriteResult::KnownPartial(error)) => {
                            Some((attempt.id, error))
                        }
                        _ => None,
                    }),
            )
    }

    /// Every definitive refusal, including streamed ones past the retained text.
    pub fn refusal_count(&self) -> usize {
        self.streamed()
            .map_or(0, |stream| stream.refused - stream.retained.len())
            + self.rejections().count()
    }

    /// Engine-issued logical leg ids carry planning order, independently of
    /// physical registration/callback order and the position of a later retry.
    pub fn reports(&self) -> impl Iterator<Item = &str> {
        self.streamed_retained()
            .flat_map(|(_, _, reports)| reports.messages.iter().map(String::as_str))
            .chain(
                self.ordered_attempts()
                    .into_iter()
                    .filter(|attempt| self.terminal(attempt))
                    .flat_map(|last| last.reports.messages.iter().map(String::as_str)),
            )
    }

    fn streamed(&self) -> Option<&StreamedWrites> {
        self.stream.as_deref()
    }

    fn streamed_retained(
        &self,
    ) -> impl Iterator<Item = &(RequestId, VenueError, RejectionReports)> {
        self.stream.iter().flat_map(|stream| stream.retained.iter())
    }

    fn serial_uncertain(&self) -> bool {
        self.inherited_uncertain
            || self.attempts.iter().any(|attempt| {
                attempt.id_overflow
                    || (attempt.released
                        && matches!(
                            attempt.result,
                            None | Some(WriteResult::Failed(_) | WriteResult::PartialUnknown(_))
                        ))
            })
    }

    fn validate_streamed_success(&self, stream: &StreamedWrites) -> Result<(), Error> {
        if self.sealed || self.serial_uncertain() {
            return Err(Error::State);
        }
        let mut refused = stream.refused > 0;
        let mut sent = stream.settled > 0;
        for attempt in self
            .attempts
            .iter()
            .filter(|attempt| self.terminal(attempt))
        {
            match (&attempt.result, attempt.released) {
                (Some(WriteResult::Accepted { .. }), true) => {}
                (Some(WriteResult::Rejected(_) | WriteResult::KnownPartial(_)), true) => {
                    refused = true
                }
                _ => return Err(Error::State),
            }
            sent = true;
        }
        if !sent || (refused && !stream.refusals_complete) {
            return Err(Error::State);
        }
        Ok(())
    }
}

#[cfg(feature = "http-plans")]
impl CommandWrites {
    /// Journal one serial Plan call at a time: each settled call is compacted
    /// into the streamed history before the next one registers.
    pub(crate) fn streamed_with(mut self, refusals_complete: bool) -> Self {
        self.stream = Some(Box::new(StreamedWrites {
            refusals_complete,
            ..StreamedWrites::default()
        }));
        self
    }

    /// Continuation custody: nothing uncertain, and (for a write) the leg's
    /// latest attempt released with a definitive native answer.
    pub(crate) fn validate_serial_continuation(&self, leg: Option<u64>) -> Result<(), Error> {
        if self.sealed || self.serial_uncertain() {
            return Err(Error::State);
        }
        let Some(leg) = leg else {
            return Ok(());
        };
        let attempt = self
            .latest(leg)
            .and_then(|id| self.attempts.iter().find(|attempt| attempt.id == id))
            .ok_or(Error::State)?;
        if !attempt.released
            || !matches!(
                attempt.result,
                Some(
                    WriteResult::Accepted { .. }
                        | WriteResult::Rejected(_)
                        | WriteResult::KnownPartial(_)
                )
            )
        {
            return Err(Error::State);
        }
        Ok(())
    }

    /// Move a settled leg into the streamed history. Its identities leave the
    /// per-attempt budget; refusals beyond the retained text are only counted.
    pub(crate) fn compact_serial(&mut self, leg: u64) {
        let latest = self.latest(leg);
        let mut index = 0;
        while index < self.attempts.len() {
            if self.attempts[index].leg != leg {
                index += 1;
                continue;
            }
            let attempt = self.attempts.remove(index);
            let terminal = Some(attempt.id) == latest;
            let stream = self.stream.as_mut().expect("streamed serial journal");
            let mut reports = attempt.reports;
            if let Some(result) = attempt.result {
                self.evidence.release_identities(result.order_ids());
                if terminal {
                    match result {
                        WriteResult::Accepted { order_ids } => {
                            stream.effects |= !order_ids.is_empty();
                            stream.ids.extend(order_ids);
                        }
                        WriteResult::Rejected(mut error) | WriteResult::KnownPartial(mut error) => {
                            if let Some(partial) = error.partial.take() {
                                stream.effects |= !partial.order_ids.is_empty();
                                stream.ids.extend(partial.order_ids);
                            }
                            stream.refused += 1;
                            let bytes = error.message.len()
                                + error.venue_code.as_ref().map_or(0, String::len)
                                + error.venue_message.as_ref().map_or(0, String::len);
                            if stream.retained_bytes + bytes <= evidence::RETAINED_TEXT_BYTES {
                                stream.retained_bytes += bytes;
                                stream.retained.push((
                                    attempt.id,
                                    error,
                                    std::mem::take(&mut reports),
                                ));
                            }
                        }
                        // Continuation never settles an uncertain fact.
                        WriteResult::PartialUnknown(_) | WriteResult::Failed(_) => {
                            unreachable!("validated definitive serial fact")
                        }
                    }
                }
            }
            self.report_bytes -= reports.retained_bytes;
            self.report_messages -= reports.messages.len();
        }
        self.legs.retain(|entry| entry.id != leg);
        self.stream
            .as_mut()
            .expect("streamed serial journal")
            .settled += 1;
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

#[cfg(test)]
#[path = "request_context_reports_tests.rs"]
mod report_tests;

#[cfg(test)]
#[path = "request_context_evidence_tests.rs"]
mod evidence_tests;

#[cfg(test)]
#[path = "request_context_replacement_tests.rs"]
mod replacement_tests;
