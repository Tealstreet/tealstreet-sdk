# Shared Selection-managed signed reads

## Pinned scope and ownership

Plan baseline: landing `a828ede7ebb54bc9c032ea61365213abac5e3dca`.
Branch: `feat/rust-selected-managed-read-helper`. Owner: codex-9czdjh.
Required external reviewers: codex-uufub4 (authority/core) and claude-4yjlw0
(first following consumer, Bitget). This is a proposal, not a frozen API or
permission for private Bitunix production. No runtime change is in this phase.

The missing seam is the composition of trusted credential Selection, opaque
Identity, initial Read admission, managed signed queueing and exact typed
refusal. `Selection::credential(KeyRef)` exists, as do `admit_http_command`,
`http_context` and `request_signed_managed`; their composition currently lives
in venue copies. No venue should introduce a third ownership implementation.

The helper lives in NEW `sdk-rs/engine/src/selected_read.rs`. The public module
export needs one line in engine `lib.rs`; no registry/lifecycle/Selection policy
rewrite is proposed. The overseer permits this wiring after claimants land or
as a docs-free composition in the next combo. Until that coordinated window,
claimed source files remain untouched. This plan and its index are shared docs.

## Current source inventory

| Source | Existing ownership/policy | Intended disposition |
| --- | --- | --- |
| `engine/src/credential_selection.rs` | `Slots<T>::select(role)` returns immutable Selection plus selected material; Selection fixes slot, route and optional account-evidence scope | Reuse existing role/slot policy. Couple Selection and Identity inside a private-field engine object; no caller-supplied mismatch |
| `engine/src/request_context_entry.rs` | Genuine callback-only admission, no command ID argument, Rejected already terminal, Revoked grants no owner | Call unchanged for initial command reads only |
| `engine/src/request_context_registry.rs` | Background issuance, typed completion, retry validation and retirement | Consume unchanged; no numeric-ID reconstruction or renewed root |
| `engine/src/http_lifecycle.rs` | Validate before allocation, queue Prepare, deferred provider, Bind/Claim, exact physical delivery and expiry | Queue unchanged; preserve exact Error and continuation |
| `venues/bingx/src/managed.rs` | Read/queue/retry/finish copy; method chooses identity and slot; generic Internal/NotSent on issuance/queue errors | Replace ownership and selection composition with shared helper; retain native recipe/codec adapter only |
| `venues/weex/src/managed.rs` | Command-only Read admission/queue/complete/reject copy; generic Internal/NotSent refusals; native Never retry | Replace ownership composition with shared helper; native task/diagnostics stay in venue |
| `venues/bingx/src/sign.rs`, `venues/weex/src/sign.rs` | Method-based accessors used beyond these reads | Replace only selected-read call sites after ownership windows; do not remove accessors still used by writes/session paths |

Fresh open-PR map checked before this plan: 50 open PRs. Engine `lib.rs`,
`credential_selection.rs`, `engine.rs`, `http_lifecycle.rs` have multiple active
claims. BingX `managed.rs` belongs to #1889; BingX `lib.rs` to #1884/#1747;
WEEX `lib.rs` to #1934. uufub4 also holds unpublished BingX trading/runtime WIP
(`feat/rust-bingx-managed-trading`). Do not edit those without a coordinated
window. BingX/WEEX family dispatch recipes are 4yjlw0-owned and stay unchanged.
No helper source or new test path is presently claimed. Refresh this map before
source, then record exact owner clearances and the wiring/composition pin.

## Consumer contracts

- Bitget caller branch `f86a9a241f1`, stacked on #1915: ordinary private reads
  select Secondary when configured, else Primary. Complete secondary setup and
  passphrase inheritance/refusal stay in trusted construction. Degraded
  BALANCE/UTA_ASSETS declare PrimaryRead(DegradedRead) at admission, pinning both
  primary identity and Primary egress. FetchAccount uses the normal read role;
  invalid/public-only refusal keeps its native pre-I/O diagnostic. Background
  settings/positions/balance/order reads use the same immutable selection.
- OKX #1938 `2a58ec8d414`: leverage/config reads select secondary else primary;
  family recipe owns demo headers through `unsigned_http_headers` unchanged.
- BingX metadata (#1884) needs the same secondary selection without calling
  `credentials.identity(Method::Get)`. Metadata constructor/eligibility edits
  stay with cxxyqr. This follows copy removal; it is not silently changed here.
- Bitunix private REST remains plan/RED only, separately gated on G1/G2, caller
  timeout/history #1945, trusted trim #1868 and this helper's reviewed/actual
  pin. Raw WS-only material cannot construct selected REST authority.
- Writes, WS/login, unsigned public requests, Plan/Slot admission, execution
  groups, background Plan issuance and mixed-operation declaration are outside
  this first seam. No managed Read grants those rights. G1/G2 may consume the
  eventual immutable selection object, but must use their own opaque issuers.

## Proposed shared boundary for review

Exact Rust signatures freeze only after the two required reviews. Proposed
names below describe the intended ownership, not a currently exported API.

1. `ReadRole::{Read{scope}, Primary{reason,scope}}` expresses only read roles;
   no method argument, Mutation or SessionAuth variant. An engine constructor
   `SelectedRead::select(&Slots<Identity>, ReadRole)` calls existing trusted
   selection and captures **Selection plus selected Identity together** in
   private fields. It returns CredentialUnavailable unchanged. Callers cannot
   construct arbitrary pairs or replace identity/slot after admission. No raw
   keys, fallback accessor, trimming, passphrase logic or credential parsing.
2. `ReadPolicy { timeout_ms, pacing, retry }` explicitly declares native bounds;
   helper fixes operation to Read and credential to the captured selection.
   No helper default deadline, attempt count, retry policy or rate floor.
   Initial physical attempt is one; permitted retries use existing evidence
   and root deadline, never a new initial owner. BingX background retries and
   Never identity reads retain their existing distinction; WEEX remains Never.
3. Distinct entry points: `command` calls genuine `admit_http_command` before
   opaque I/O and takes NO command ID; `background` creates an ordinary bounded
   Background Read. External-Context `queue_existing` is DEFERRED entirely:
   Context does not retain complete Identity/signing/passphrase/evidence scope,
   so operation/credential equality cannot prove the original selection. It
   never infers command entry from Scope or turns a Plan/Slot into plain Read.
4. Queue descriptor contains native method/URL/body/optional ArrayAt. A pure
   family builder receives the selected opaque Identity and original Context,
   constructs the unchanged SignStep and native pending continuation. The
   builder performs no provider call/entropy/clock read or other opaque work.
   Helper queues `request_signed_managed` and stream selection together, then
   applies the selected account-evidence scope with its exact slot. There is
   no route-after-send override: managed intent derives from the same selected
   credential before Prepare. Keep native ordered static/demo headers intact.
5. Result preserves failure phase and custody: initial entry refusal returns
   `CommandEntryError::{Rejected,Revoked}` unchanged with **no second terminal**;
   background admission returns the exact request-context Error; queue refusal
   returns the original pending, exact Error and genuine issued Context so its
   owner can reject/retire through existing typed APIs. Never collapse errors to
   generic Internal/NotSent. Initial Read entry rejection has outcome None;
   queue/preparation/provider failures preserve their exact existing metadata,
   including NotSent where the engine supplies it. Prepare, provider and
   physical failures remain existing engine callbacks with their
   exact typed error and continuation. No synthetic Context on admission error.
6. Caller owns native publication, completion and diagnostics. Shared helper
   must not complete an unrelated owner, hide an already-issued terminal, or
   adopt a numeric fallback. It may offer common typed retirement plumbing,
   but cannot choose native body semantics or accepted-row recovery. Queue
   failure must not leak an admitted root: consumer paths prove exactly one
   typed settlement/retirement and original pending delivery.
7. A mandatory non-Clone admitted wrapper retains original Context plus the
   exact selected Identity/slot/egress/evidence through every queue/retry. Retry
   consumes that wrapper plus native continuation and explicit RetryEvidence,
   validates via `http_retry_context`, retains account generation and original
   egress, and builds a new physical attempt only. Validation failure returns
   the wrapper and continuation unchanged with the exact Error; queue failure
   also preserves both. No separately supplied replacement Selection is accepted.
   No deadline renewal, runtime fallback or newly selected key on retry. Fresh
   reconciliation reads have their own declared owner, never another read's
   in-flight reservation. Plan-owned retries remain with Plan APIs.

For missing selection in a command, the shared helper admits a genuine local
Read with credential None using the same explicit native policy, then calls
command_error_context for an optional caller-supplied native diagnostic, then
fail_context with the caller-supplied exact native typed credential error.
Diagnostic precedes the sole terminal; an absent diagnostic emits none.
Rejected/Revoked return unchanged without a second terminal. This branch issues
no I/O, Identity or selection and is shared rather than a venue admission copy.
Background selection refusal remains typed and creates no command.

## Signature proposal after core API-fit

These signatures are the next review target, not an exported/frozen API. Types
Context/Error/CommandEntryError/RetryEvidence/Pacing/RetryPolicy are existing
request_context types; Identity/SignStep/Slots/Selection remain their existing
engine types. SelectedRead and AdmittedRead have private fields and no Clone.

```rust
pub enum ReadRole {
    Read { scope: Option<AccountReadScope> },
    Primary { reason: PrimaryReason, scope: Option<AccountReadScope> },
}
pub struct ReadPolicy {
    pub timeout_ms: u32,
    pub pacing: Option<Pacing>,
    pub retry: RetryPolicy,
}
pub struct ReadRequest {
    pub method: Method,
    pub url: String,
    pub body: Option<String>,
    pub stream: Option<ArrayAt>,
}
pub struct SelectedRead { /* private Selection + Identity */ }
pub struct AdmittedRead { /* private SelectedRead + original Context */ }
pub struct LocalRefusal {
    pub diagnostic: Option<String>,
    pub error: VenueError,
}
pub enum CommandReadError {
    Entry(CommandEntryError),
    LocalRefusal { error: VenueError, terminal: Result<(), Error> },
}
pub enum BackgroundReadError {
    Selection(CredentialUnavailable),
    Admission(Error),
}
pub struct RetryRefusal<T> {
    pub read: AdmittedRead,
    pub pending: T,
    pub error: Error,
}
impl SelectedRead {
    pub fn select(slots: &Slots<Identity>, role: ReadRole)
        -> Result<Self, CredentialUnavailable>;
    pub fn command<P>(cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>, policy: ReadPolicy,
        missing: LocalRefusal) -> Result<AdmittedRead, CommandReadError>;
    pub fn background<P>(cx: &mut Cx<'_, P>,
        selected: Result<Self, CredentialUnavailable>, policy: ReadPolicy)
        -> Result<AdmittedRead, BackgroundReadError>;
}
impl AdmittedRead {
    pub fn context(&self) -> &Context;
    pub fn queue<P>(self, cx: &mut Cx<'_, P>, request: ReadRequest,
        recipe: impl FnOnce(&Identity) -> Box<dyn SignStep>,
        pending: impl FnOnce(Self) -> P) -> Result<RequestId, (P, Error)>;
    pub fn retry<P, T>(self, cx: &Cx<'_, P>, evidence: RetryEvidence, pending: T)
        -> Result<(Self, T), Box<RetryRefusal<T>>>;
}
```

Queue copies only the genuine Context for the engine request, and moves the
non-Clone wrapper into native pending through its pure constructor. A queue
refusal returns that same pending containing its original wrapper; no wrapper
or continuation is destroyed by the helper. Retry takes native continuation T
separately so validation refusal returns it and the wrapper intact. Consumers
then reconstruct the same native pending, preserving all fields. Provider and
prepare callbacks already receive that pending. The family recipe closure gets
only the wrapper's immutable Identity. The caller can borrow Context for exact
delivery/publication/typed terminal but cannot swap a newly selected role into
this wrapper. LocalRefusal reports the exact typed error and terminal result;
its caller must return, never numeric-fail or publish an additional terminal.
The optional LocalRefusal diagnostic preserves Bitget's command-error-before-
failure order; OKX can supply None. Slots::new(primary, secondary, SlotPolicy)
cheaply moves/clones already validated opaque Options identities and adds no
setup logic. Existing http_retry_context takes &self, so retry uses &Cx.
No helper-owned owner registry, timer or scheduler. No external-Context queueing.

Ordered headers are part of byte equality, not just header sets. Keep keyless
remote/hosted dispatch working: one positive native control in each first
adopter must prepare a selected read and receive provider signing without local
key material. Existing family recipes/provider adapters remain unchanged.

## Provenance boundary for following G1/G2 composition

G2 owner codex-hcmq3f identified a separate OPEN prerequisite at `224810a4c03`:
trusted issuer/subject/config-generation snapshot provenance and retirement.
Current Identity refs plus Selection and Credential do not prove that broader
property. This helper preserves the exact selected snapshot already admitted
under existing Context/Engine lifetime; its private wrapper is not proof of
trusted subject or configuration-generation retirement. No G1/G2 readiness or
new snapshot invalidation API is claimed. Core/owner review must determine the
separately owned shared prerequisite before those Plan consumers compose it;
do not add a local generation counter, synthetic credential or runtime swap.

## Original failing controls before source

Write separate compiling tests against frozen production and commit them before
implementation. Missing API symbols or compile failures do not count as RED.
Use genuine Engine callback/Prepare/provider/Bind/Claim paths; do not fabricate
Context, Selection, Cx or HTTP delivery. Existing engine fixtures may prove shared
behavior, but do not label those a native consumer receipt.

First native RED candidates (verify the actual first failure before claiming):
- BingX and WEEX duplicate FetchAccount while the first managed read is pending
  must not stage another Prepare/provider or retire the first owner. Current
  copies issue `http_context(Scope::Command(id))` rather than genuine entry.
- Admission capacity refusal in a genuine command must retain Capacity's typed
  kind/message and emit exactly one terminal without provider/HTTP/Store change;
  current copies replace issuance Error with generic Internal/NotSent.
- Exact typed queue failure must return its native pending and genuine root;
  original errors cannot be erased. Construct only a genuinely reachable
  failure; do not claim managed queue Capacity after full root admission if
  the equal capacity limits make it unreachable.

If one candidate already passes, record GREEN and keep it as compatibility;
choose a demonstrable failure from the copies rather than altering assertions
or writing a local helper that fabricates a bug. No third venue ownership copy.

After the seam exists, focused engine controls cover ordinary/degraded selection
for GET and POST reads, both slots/single slot/missing credentials, distinct
account evidence scopes, wrong credential/operation/pre-issued authority,
duplicate/foreign callback, exact entry/queue errors, provider refusal, delayed
preparation, fractional original expiry, retry evidence/unchanged egress,
pending preservation (including retry-validation refusal and exactly one command
terminal or Background retirement), and no mutation/session/public privilege.
Compare native
BingX/WEEX request bytes, successful body semantics, streaming and provider
operation refs before/after. A selected identity must match the recipe and
context at first queue and every retry; no caller-supplied replacement pair.

## Implementation and integration sequence

1. Commit this plan/index, send to uufub4/4yjlw0, resolve review findings, freeze
   proposal and collision disposition. No source while core contract is open.
2. Commit executed original assertion REDs and compatibility receipt separately.
3. Add new engine module and new tests; wire its one-line export only in the
   approved composition/window. Adopt BingX/WEEX in the same final PR when their
   owner windows allow it, removing duplicated selection/admission/queue/error
   policy. Retain only family bytes and native continuation adapters. If the
   owners cannot supply a window, report the concrete held paths; no third copy
   or false consumer readiness. Bitget/OKX then adopt in their owner follow-ups.
4. Preserve the request inventory: the seam itself is delta zero; any actual
   BingX/WEEX legacy removals require exact per-venue prune and owner's review.
   Both checker commands must pass; never rename bindings to hide scanner hits.
5. Focused engine/BingX/WEEX/Binance native tests, feature-off plus relevant
   feature-on controls, scoped strict Clippy/fmt/bans, independent source/caller/
   full-map reviews, then full exact-head CI with all WASM/replay/size gates.
   Local CARGO_BUILD_JOBS=2, no local replay/profile/release WASM work.
6. CALLERS records actual typed refusal/duplicate behavior and native retry/
   deadline/credential policy, with source anchors for callers; update AUDITS,
   QUALITY and index only for established behavior. Caller-visible changes to
   private auth retries or history remain separate owner decisions, not implied
   by this helper. Merge only in an engine window with actual integration,
   separate full-incoming/own-file guard and head-matched merge.

Architecture gate: shared concern, implemented once in engine. The two existing
venue ownership copies are removed through coordinated adoption; this seam
introduces no per-venue cross-cutting policy or signing recipe.

## Executed original controls

On unchanged `a828ede7ebb` production, separate new native test files compile
and run four controls per venue: **two semantic assertion REDs total and six
compatibility GREENs**. Both capacity controls observe the real 256-root bound
and fail on the venue copy replacing exact Capacity/None outcome with generic
Internal/NotSent. Duplicate pending-command controls already pass in both
venues; they remain unchanged compatibility controls, not RED evidence. The
remote fixtures use only opaque refs and real Prepare/provider-result/Bind/Claim;
secondary credential/signing refs, ordered headers and primary fallback pass.
They prove native delegated dispatch, not execution of an external signing
service. No Context, Selection or Cx is fabricated. An initial fixture compile
attempt named the Sign operation variant incorrectly; it was corrected before
this receipt and is not counted as a RED. No production or inventory changed.

Overseer provenance disposition: hcmq3f owns trusted issuer/subject/config-
generation snapshot retirement inside G2 or a separate G2c slice. This helper
continues to consume existing Context generation only.

## Maintainer standalone-helper split

The overseer subsequently directed the helper to land independently: new
selected_read.rs, engine controls and minimal module export, with existing
native compatibility evidence. This explicitly authorizes the one-line lib.rs
export despite concurrent claims. No claimant runtime hunk is edited. BingX
adopts after #1889 (claude-55ao7t); WEEX after #1934 (uufub4, combo-4a). Native
original capacity REDs remain unchanged at `95d6b16389d`, held on branch
`feat/rust-selected-read-consumer-reds`. The standalone helper excludes those
still-failing native assertions from its passing test targets; it retains the
six native compatibility controls and proves exact capacity behavior through
genuine engine-helper controls. This is not native adoption or a claim that
those two original venue failures have gone GREEN. Consumer PRs must re-enable
and execute the unchanged original assertions through actual adoption.

Source verification found RetryRefusal is at least 233 bytes. Its error return
is boxed to satisfy strict Clippy without adding a success-path allocation or
weakening custody: `Result<(Self,T), Box<RetryRefusal<T>>>`. The boxed error still
owns the identical wrapper and continuation. This signature refinement requires
source/API re-review; no lint exemption is added.

Before command admission, consumers of the reviewed command-timeout seam call
command_wait: Expired goes through reject_expired_command without admission;
Remaining(ms) supplies the explicit root ReadPolicy timeout. Do not guess a
fixed 120s root. This standalone helper does not itself consume that unlanded
seam. Multi-page history additionally needs a reviewed child-leg method retaining
the root's exact selection and immutable caller bound, with each page minted
from the original root at dequeue and a native physical timeout. Re-selection
per page and misuse of retry are forbidden. This is a separate follow-up gate
for OKX/Bitget/BloFin/Bitunix history, not supplied by the current single-leg API.
