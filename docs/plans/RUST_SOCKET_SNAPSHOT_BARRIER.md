# Socket login and native snapshot completion are independent

## Ownership and original evidence

Owner: codex-5c57jl, assigned by claude-ddi2il. API/custody reviewer:
codex-uufub4. This is a separate shared socket-core prerequisite, not an
extension of frozen #1943. Branch `feat/rust-socket-snapshot-barrier` starts at
actual15bd7821b52 and plain-merges frozen1943 in a71bca11050. Phemex whole
adoption is a later consumer; its parked/public WIP is not part of this PR.

Original evidence precedes this plan: Phemex plan35cf6d89b57 and dedicated
real-Engine controls81fc6001a6b against an exact native861462871e3 archive.
Ten tests execute:6 assertion RED,4 working GREEN,0ignored. Independent original
positive proves connect+20s expiry despite delayed open, login ACK and pongs;
another proves malformed snapshot retains expiry. Equality snapshot currently
publishes without timely watchdog ingress; duplicate/late open, duplicate ACK,
expired financial ingress and late pong are independently pinned. The new auth
fixture field-order mistake was corrected before the exact recorded receipt.
No production bytes were changed for those controls.

## Native requirement and scope

Phemex login ACK authorizes aop_p.subscribe. It does NOT complete the 20s native
snapshot requirement. That immutable deadline starts at physical CONNECT, not
OPEN, and only the existing accepted AOP snapshot proof can discharge it.
Pongs affect the independent12.5s receive bound. A valid snapshot also begins
native ready/stable supervision at its accepted completion, as before.

Existing shared AuthEvidence::Accepted changes lifecycle readiness and invokes
Supervisor::authenticated together. The opt-in must separate those facts while
leaving existing registration/default behavior byte-equivalent. No financial
predecoder, venue timer/epoch ledger, new scheduler, HTTP sender, host message
or provider schema belongs in this prerequisite.

## Proposed additive API

Names below are the concrete API review target, not a published contract:

```rust
// Same feed policy, bounds and errors as existing registration; explicitly
// retain its auth_timeout_ms as a CONNECT-origin native snapshot barrier.
Cx::register_socket_feed_with_snapshot(FeedPolicy) -> Result<FeedId, SocketError>

// Only inside the exact admitted managed text-message callback, after native
// snapshot validation. No caller-chosen feed, socket, epoch, time or generation.
Cx::complete_socket_snapshot() -> Result<(), SocketError>
```

Registration requires Private, AuthMode::Required, ControlPolicy::Native,
AuthRecovery::Reconnect and a finite positive auth_timeout_ms. Incompatible
policy is InvalidPolicy before registry/resource mutation. This narrow opt-in
changes neither FeedPolicy struct literals nor existing register_socket_feed.
Future venues needing other policy combinations require a reviewed extension.

A completion attempt is valid only if the engine's private active message proof
identifies the current live epoch of an opted-in feed, login has been accepted,
the snapshot barrier is still pending and the immutable bound has not expired.
No proof may be returned to or retained by a venue. Outside native message
scope, wrong feed, before login, duplicate completion, after retirement, replaced
epoch, HTTP/timer/command/signing callback or shared released-interest callback:
Stale, with no deadline/liveness/readiness mutation. Invalid time stays typed
and cannot renew work. Completion grants no HTTP, command or signing authority.

## Exact hooks and custody

1. `engine/src/engine_socket.rs`: additive Cx registration and completion methods;
   the managed text ingress sets private current-message epoch proof only around
   `Venue::on_message` after all existing live/expiry/control gates. Clear it on
   exit, including ordinary failure paths. Shared lifecycle/continuation callbacks
   cannot borrow an outer message proof; extend their existing custody clearing.
   Legacy text, binary frames and control-only frames do not mint this proof.
2. `engine/src/engine.rs`: one cfg-managed-sockets private Io proof field and
   initializer. Retain existing event/HTTP/Plan/fatal/drain sequencing. No
   borrowing/restoring the Plan expiry cleanup barrier. Existing socket-native
   scope clearing remains intact, with the new proof treated as an activation
   loan rather than global transport state.
3. `engine/src/socket_driver.rs`: registration records the opt-in and original
   configured snapshot bound. Begin computes checked CONNECT-origin absolute
   deadline for each physical epoch, before commit. Reset only on a genuinely
   new epoch; never on open, ACK, duplicate ACK, pong, repair or reprepare.
   On accepted open, bound the existing Supervisor authentication watch by the
   original connect deadline. Login acceptance advances existing lifecycle auth
   and subscription readiness, cancels any login retry, but leaves Supervisor
   authenticating while opted-in snapshot proof remains outstanding. Do not
   refresh native receive/stable timestamps on that ACK.
4. The existing Supervisor watch remains the sole timer owner. Retain an exact
   immutable absolute bound in shared driver custody and ensure the scheduled
   watch plus synchronous ingress/claim/frame/signing release checks enforce it
   at equality. If representing that exact bound needs a narrow Supervisor
   absolute-deadline helper, add it in `engine/src/socket.rs` with default None;
   do not approximate or clamp away overflow. Readiness/expiry checks, including
   #1943 handoff readiness, must see the outstanding snapshot bound.
5. Completion consumes the current message proof only after its driver epoch,
   phase, login and deadline checks succeed, discharges the pending barrier once,
   then invokes existing Supervisor::authenticated. Duplicate completion cannot
   move receive time or reset stable/backoff. Retirement clears the pending epoch
   proof/deadline before callbacks, cancels owned watch and uses shared backoff.
6. `engine/src/socket_lifecycle.rs` changes only if a narrow mode/query is needed;
   avoid adding a second readiness ledger. Existing logical authenticated/readiness
   state remains the source for subscription permission. Snapshot completion is
   separately engine-owned supervision custody, not a second login auth result.

Default registration remains unchanged. Empty/default paths do not allocate or
create snapshot work. Registration, connect and completion keep fatal-first
checks and safe-ID preflight. No drain callback or host reentrancy is added.

## Consumer placement (later Phemex PR)

Phemex registers its private policy through the opt-in, projects id999 success
as login evidence and preserves exact auth/ping/AOP order. Existing account
snapshot validation remains the only financial proof. The exact existing hook
is account.rs accepted `orders_p` snapshot branch (original lines1074-1079),
which currently calls Supervisor::authenticated. Positions/accounts-only
snapshots do not discharge it. An accepted orders snapshot with
`safe_snapshot=false` from a newer order_revision DOES discharge it while
preserving the newer financial state; do not require revision equality. At
that accepted orders-snapshot commit boundary, strictly BEFORE ANY affected
Store publication (all fallible validation/preparation first, followed by the
infallible native commit),
call complete_socket_snapshot and retain the existing financial outcomes.
If it refuses, do not publish or grant stream authority. No independent parser
or inferred success from a snapshot label alone. Existing sequence, revision,
row ownership/malformed/unknown-order and financial assertions remain exact.

The core prerequisite has no Phemex production edits or inventory pruning.
Phemex whole-socket work stacks only after this contract is reviewed and its
controls pass; actual prerequisite landing is required before consumer merge.

## Plan audit and RED-first matrix

Audit the current `Driver::begin`, `finish_ingress`, `finish_control`,
`observe_ingress`, `handoff_ready`, final frame release and signed-send gates.
Auth ACK cannot extend idle timing, and delayed open cannot extend snapshot
expiry. Native working controls must prove positions/accounts-only snapshots
retain expiry and revision-conflicted accepted orders snapshots end the bound
without erasing newer orders; a raw type=snapshot label is never proof. One positive must prove a genuine login permits native AOP frames while
the snapshot barrier remains outstanding. Another proves validated snapshot
ends it without renewing the original deadline first.

Original shared assertion controls use a real Engine synthetic native venue:
delayed-open then ACK with pong traffic through connect+20s; missing snapshot;
malformed snapshot; accepted-looking snapshot exactly at bound without timer
turn; expired snapshot callback count0/publication0; duplicate auth ACK; accepted
snapshot before bound then later current incremental publication; timeout and
fresh reconnect with the new epoch's independent bound. Record pre-opt-in
existing-API failure pins separately from the post-opt-in API wiring change;
missing-method compilation is not an assertion RED.

Custody negatives: call completion from start/open/control/lifecycle/ordinary
command/timer/HTTP/signer/released-interest callbacks; before login; from a public
or ordinary feed; after in-callback retirement/replacement; duplicate completion;
stale old physical message; synthetic borrowed outer message scope. Each must
leave current pending proof/deadline unchanged. Add time overflow/nonfinite and
fractional/equality controls, plus one accepted completion positive so denial
controls are not vacuous. Retain current 1943 early/inline/rearm/expiry-barrier,
HTTP delivery isolation and fatal/caught-callback controls unchanged.

## Verification and train

Scoped shared engine tests for default/managed-only/http-plans-only/combined,
Phemex native timing controls and existing OKX supervision tests, strict Clippy,
formatting and bans/ratchets. Full workspace/CPU/memory/size/replay gates belong
in fresh CI; unchanged budgets/profile/reference/harness. Independent uuf API
and source/custody clearance, fresh CI and train window precede landing.
Original controls stay visible; no package-provenance claim is required by the
current train rule. No merge, deploy, flag change or master cutover implied.

## Executed shared original assertion controls

On unchanged production45ab36cdc9e, the real Engine synthetic-native target
`engine/tests/socket_snapshot_barrier.rs` uses the EXISTING registration API.
Command: `cargo test -p safe-cex-engine --features managed-sockets,test-support
--test socket_snapshot_barrier`. Six tests compile and execute:3 assertion RED,
3 working GREEN,0ignored. Original missing trait methods/wrong Book import were
fixture compilation errors, corrected before this receipt; no compilation error
is counted as RED. The old registration does not declare the new contract, so
the implementation will explicitly opt this synthetic consumer into the new
registration API and add exact-message completion at its native commit point.
That adoption delta will be separately identifiable, not a changed assertion.

REDs: after delayed open, login acceptance and fresh pongs, connect+20s snapshot
enters the native commit callback (1 instead of0); ACK/pongs without snapshot
fail to close at20s; malformed native snapshot also leaves the bound discharged.
Positives: login releases one AOP frame without snapshot; current valid native
snapshot reaches commit; retired epoch cannot enter the decoder. These shared
controls complement the native Phemex12-control6RED/6GREEN receiptc8812e4e85a.

## API-review pins before runtime

Uufub4 bounded API/custody direction CLEAR, with these required pins: native
completion occurs strictly after the one reducer prepared/validated acceptance
and before ANY affected Store publication, never merely where practical.
Login auth permission must still release the native AOP subscription/handoff
needed to obtain that snapshot; the outstanding barrier constrains expiry and
financial commit, not permission to request the snapshot. Every release path
still checks the same original absolute bound. Default registration semantics
remain equivalent; private message proof is cleared across HTTP, Plan expiry
and continuation nesting. Preflight deadline representation and all required
timer/resource IDs before state mutation; preserve existing fatal-first rules.
No reuse after in-callback retirement. Runtime/source review remains separate.

## Runtime checkpoint before source review

The initial implementation uses the existing Supervisor watch with an exact
connect-origin absolute snapshot deadline; accepted login advances subscription
permission while retaining Supervisor authentication expiry until native snapshot
completion. Private active message custody binds BOTH epoch and engine receipt
time. Non-message, stale, duplicate, retired and caller-modified-time attempts
cannot complete it. Exact new socket/watch resource counts are preflighted,
including the extra early timer only when the snapshot bound is shorter than
the original initial watch. Existing default registration remains unchanged.

Fifteen real-Engine controls pass after actual1943 integration, including
original assertions, default registration, login/AOP permission, actual HTTP,
timer, command and released-interest callback denial, retirement/reconnect,
stale epochs, duplicate completion and receipt-time binding. Two new internal
controls additionally cover exact last-safe resource capacity and atomic invalid
policy refusal. Four scoped socket feature modes and strict Clippy passed the
initial implementation; repeat them after subsequent source changes/integration
before certification. No source-review/full-CI/performance claim at this checkpoint.

Signer-lane integration gate: moving synchronous native user.auth to an async
provider can release the ordinary opening ping first. Preserve exact native
auth-before-immediate-ping using reviewed shared signing-release custody; no
venue queue/timer and no silent delay-until-ACK substitution. The Phemex signer
lane is plan/RED-only pending shared raw-signature expiry. This snapshot
prerequisite does not claim to resolve that separate ordering/expiry contract.

## Independent source review

Uufub4 engine/API/custody source review CLEAR at2238e7c5691, with independent
exact git-archive execution of15 socket_snapshot_barrier controls PASS/0ignored.
Committed15d2b3f4cc3 validate-before-capacity delta is source CLEAR; the reviewer
does not claim separate execution of that added unit control. Its local snapshot
unit filter is13PASS. The initial changing-worktree syntax error is excluded
from the immutable review receipt. Actual combo4a2fade3 is genuinely integrated.
Full feature-mode/strict/replay/CPU/memory/size CI and train readiness are separate.
