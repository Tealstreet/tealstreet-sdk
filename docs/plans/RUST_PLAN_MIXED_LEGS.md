# Immutable mixed-operation and credential Plan legs

Plan-first G2 checkpoint, 2026-10-24. Owner codex-hcmq3f. First consumer
claude-4yjlw0's BloFin custody cutover; native review codex-9czdjh; Plan API fit
codex-uufub4. Base actual a828ede7ebb54bc9c032ea61365213abac5e3dca. This phase
adds a declaration plan and isolated original-source tests only. No runtime
binding API, signer migration or whole-family readiness is claimed.

## Native geometry and caller contract

BloFin S3-S6 remains one production custody cutover. Its smallest G2 proof is
hedged isolated leverage: one mutation command, two concurrent Primary POSTs
(long and short), one journal/barrier and exactly one terminal. Native leverage,
position leverage and revisions commit only after BOTH replies succeed. The
cross/one-way form remains one POST without positionSide. Both nonce/header
preparations precede the native pair's enqueue; failure of the second native
preparation sends neither. Independent managed send-time preparation does NOT
provide that group barrier: one sibling can be accepted before the other's
provider refusal. Consumer ruling requires preserving this preparation barrier. A separately
reviewed shared parallel-Mutation group prepare/release barrier is an OPEN
prerequisite to BloFin cutover; #1939 only governs reply independence. G2's
immutable binding REDs can proceed, but no native parity claim until that barrier
exists. The overseer assigned G2b to codex-hcmq3f as a separate plan/RED slice;
G2 runtime stacks on it, reviewed by codex-uufub4 and claude-dazzy3.
No local eager-sign fallback. Reverse, duplicate and stale deliveries must not
change the aggregate result.

The mixed proof is Primary cancel -> Secondary Read Verify -> Primary place,
strictly serial under the same mutation owner. No observer execution group is
needed. Source-bound replacement contract:

- Cancel normal batches <=20, TPSL <=10, algo <=1, in family order 0 -> 1 -> 2.
  Successful cancels with further work wait 350 ms (normal/algo) or 1000 ms
  (TPSL), through declared shared pacing.
- Only family-0 replacement targets require order-detail Verify, after all
  cancels and before any placement. Plain cancel, other families and place-only
  work do not gain a new Verify. Reads use Secondary when configured, otherwise
  Primary, with the existing passphrase inheritance rules.
- Verify proves a successful envelope, exactly one object or singleton array,
  matching instId/orderId, state=canceled, and finite nonnegative native
  filledSize equal to the captured NATIVE fill. No retry or skip on failure.
  Multiple rows, NaN/negative values and mismatched identity fail.
- Before every Verify/place, newer execution, Closed or increased base fill
  aborts replacement, including fills during pacing or pending Verify. Known
  canceled/tombstoned/closed rows stay so on later failure. No generic pre-POST
  settings-revision fence exists natively; any signer-release race fence must
  be separately declared and opt-in, not described as preservation.
- Client-ID receipt slices precede each placement. Preserve acknowledged native
  routing and ordered success IDs. Later failure carries structured Partial with
  accepted IDs (BloFin P0 #12), replacing the legacy message-only aggregate.

Bitunix's second consumer is Secondary Positions -> Orders -> TPSL preflight,
then Primary write, and Primary cancel -> Secondary order-detail -> Primary
replacement with native conditional/fill/pacing fences. Its exact committed
inventory pin is pending codex-9czdjh; no consumer source changes here.

## One admission and immutable bindings

Today's PlanPolicy applies one Operation and Credential to every call. A GET
inside a Mutation/Primary Plan is therefore still Mutation/Primary authority.
Method, callback state and local credential switches cannot repair that gap.
Extend the ONE canonical admission transaction to normalize a resolved binding
for every logical call, including fallback calls, before any resource IDs.
Existing uniform admission lowers into this same representation; do not retain
a second policy implementation or add a venue override after selection.

Proposed declaration vocabulary (spelling is pending API fit):

```rust
enum LegBinding {
    Read(BoundSelectedRead),
    Mutation(IdentityBinding), // Primary mutation role only
}
struct LegDeclaration {
    binding: LegBinding, // Operation derives from this checked variant
    retry: RetryPolicy, // resolved per leg, bounded by its CallBound credits
    pacing: Option<Pacing>, // resolved per leg; no callback override
}
// Same internal admission used by uniform, conditional and physical Plans.
// Each recursive call declaration contains its bound and LegDeclaration.
cx.admit_bound_plan(&preflight, declared_steps, root_policy, terminals, barriers)
```

These are proposed types, not an available runtime API. The former
SelectedIdentity placeholder is removed. codex-mxb4mq owns G2c #1961; its API
proposal `59ab097cf561c616a0a7ecc0a39f088633e0dba3` defines the opaque
IdentityBinding and BoundSelectedRead inputs. codex-9czdjh's existing
Selection-managed helper alone does not supply their provenance. G2 retains the
real installed binding, including the entire bound read selection, rather than
reconstructing it from refs, Selection or Arc. The same retained selection builds
the family recipe for request_signed_managed. No later Identity lookup reselects.

Subject/config-generation provenance is an OPEN shared admission prerequisite,
not supplied by today's Selection (slot/route/evidence) or Credential
(slot+KeyRef). The reviewed helper must bind a trusted engine-issued configured
identity snapshot to the exact issuer/subject/config generation, prove each ref
belongs to it, and retain that immutable snapshot through queued dispatch. The
setup is immutable per engine: account configuration changes create a new engine;
old snapshot retirement happens through teardown, not live replacement. The
final snapshot seal freezes all dependent setters even before start. A private
wrapper or a venue integer is not proof.
Coordinate this with codex-9czdjh's non-Clone admitted wrapper; external
queue_existing is deferred there. Until its exact snapshot/retirement contract
is reviewed, G2 has no source clearance and cannot promise that provenance.

Selection resolves ONCE from configured identities plus an explicit operation
policy, never an HTTP method. Read uses configured Secondary else Primary;
absent Secondary passphrase inherits Primary, explicit empty refuses. Mutation
always uses Primary: Secondary-only configuration cannot authorize a write.
Primary-only degraded reads are an explicit declared policy variant. An absent
identity, cross-slot/ref/subject mismatch or stale generation refuses before
IDs or I/O; no alias, raw-key fallback or credential substitution is allowed.

The root retains Mutation intent and journal authority. Preflight declares its
read/write credential domains before any preflight I/O. Every resolved leg must
belong to those admitted domains and the exact subject/generation snapshot;
G2 cannot introduce a new credential domain from a later DATA callback. Read
and Mutation bindings are immutable for retries, refusal fallback and reprepare.
A configuration change tears down the old engine, never reselecting its remaining
legs. Private failure-only cleanup remains available after public authority is
retired; it grants no signing, send, retry or successful publication rights.

Choose a recursive declaration attaching each binding directly to its call,
including each fallback occurrence. Normalize it through the exact Schedule
traversal once at admission; do not accept a parallel root-first binding slice.
Current fallback sequences are processed LIFO, and equal cardinality cannot
detect a same-count identity permutation. Structural attachment removes that
class of mismatch rather than trusting an ordinal convention.
Reject duplicate/missing/extra/wrong bindings before reservation; failure leaves
Preflight, activation custody and IDs unchanged. All fallible validation and
storage preparation precede the same admission commit as uniform declarations.
Conditional terminal anchors must resolve to Mutation calls: a mixed root's
Operation::Mutation cannot authorize a terminal anchored on Read Verify. Reject
that declaration before IDs, and pin both admission refusal and Read terminal
rights denial. Conditional terminals and future ReconcileBound remain fields of that single
internal declaration. No alternate constructor bypasses readiness or custody.

Each leg also fixes RetryPolicy. Today's root PlanPolicy.retry is insufficient:
CallBound attempts/window/fallback do not grant retry authority, and a Mutation
Context cannot use ReadFailure. A Read leg's bounded ReadFailure permission is
independent of a Mutation leg's Never/DefinitiveRefusal permission. Retry/fallback
selection cannot upgrade operation, credential or journal authority. Per-leg
Pacing is also immutable; CallBound currently has neither retry permission nor
pacing. Uniform APIs normalize their root policy into each attached binding.
No callback infers ReadFailure/DefinitiveRefusal or a different pacing domain.

Journal enrollment and replacement must be operation-aware. Today's
replace_plan_call enrolls every fallback logical ID through CommandWrites::replace;
that would incorrectly enroll a Read child as money work. G2 registers only
Mutation descendants and preserves the original write's evidence/ancestry while
Read descendants remain transport-only. An unresolved write never disappears
because its fallback is Read or because a Read completed. Mixed fallback controls
pin success, refusal and Unknown projection without imaginary Read money facts;
proof that resolves a prior write still needs the reconciliation contract.

## Read Verify has no mutation rights

A selected Verify Context carries Operation::Read and the selected credential
and route, despite the root's mutation journal. It may validate exact current
Plan/slot/DATA and finish that call. It cannot record Accepted/refused money
facts, acquire a mutation intent, independently seal the command, mint a root,
renew a deadline, or publish a fact through the earlier write Context. Foreign
owner/slot/generation/retry handles refuse before signing and dispatch.

Successful native proof advances the declared next slot. A failed proof fails
the SAME Plan through the shared prepare/commit projection, preserving all prior
write effects and IDs. classify::read supplies read kind/auth/retry evidence,
not a financial outcome. Provider refusal is NotSent for that unreleased leg
and never auth evidence; it cannot erase an earlier accepted or uncertain write.
No local status ladder, outcome demotion, effective flag or fabricated IDs.

All writes use the landed classifier CodeTable/http_write_* facts; accepted IDs
and Unknown remain sticky under the central journal. Successful Verify alone
cannot prove a previous uncertain mutation resolved; any reconciliation needs
the separate reviewed continuation contract. This slice grants no TxFlow delayed
placement-ID settlement or old-write-context publication.

## Bounds, parallel barrier and composition

Keep checked sum-of-serial/max-of-parallel geometry, structural attempt credits,
immutable root and exact timer custody. The first consumer declares 20 s per
physical attempt as an explicit new bounded policy; native timeout_ms was None.
The cap begins at selection/admission before Prepare/Bind, not release. Signing,
pacing, retries and verification never renew it. No arbitrary 120 s root.
Read retries consume only their declared credits; they do not reset write or
resend budgets. A Verify that natively has no retry declares one attempt.

For a parallel all-Mutation step, the required shared barrier prepares and
provider-signs every leg before releasing any. Failure/refusal/expiry before
release cancels all prepared work: local NotSent for this step, no partial from
this step. Earlier released steps still govern the whole command's projection.
After the barrier opens, failures use ordinary journal Partial/Unknown. Serial
steps and Read legs retain per-call release. Delayed-provider and prepare-deadline
REDs must prove zero HTTP before all succeed and zero HTTP if one refuses. This
is an OPEN shared prerequisite, not a venue-local staging workaround.

BloFin draws a 21-byte nonce from injected signing entropy at prepare. Draws
must follow declaration order (long then short; cancel, Verify, place), independent
of provider response order. Reprepare redraws for its exact leg deterministically
through the signing core. Preserve the existing nonce-order native control; no
venue random source or new entropy loop.

Two concurrent Primary writes settle independently and order-independently;
exactly one root terminal follows the barrier. #1939 independent_calls is an
explicit dependency for faithful continuation after delivered 408/5xx. Until
its actual integration, core's stop/Unknown behavior is not described as native
parallel completion; do not add a venue fallback. Controls cover accepted plus
401/429/unlisted/transport/timeout, both refused, accepted sibling then a post-release transport failure, and provider refusal before any release. Accepted IDs become structured Partial
on failure; no accepted sibling permits whole-command NotSent/Rejected.

G1 separately grants finite Read-only background admission and MUST reject any
Mutation leg. G2 grants no background issuance or Session mutation cast. Planned
executions (#1919), conditional completion (#1894), retained readiness (#1896),
Session retirement and #1939 share the same admission and prepare/commit funnel.
Observer expiry must sweep before projection; owner-specific terminal delivery
and retirement remain in the common commit hook. No source hook is edited while
those frozen PRs hold it. The runtime work integrates their actual landings and
coordinates exact changes with their owners first.

## Plan, REDs and implementation gates

Own proposed runtime module: request_context_plan_bindings.rs. Own test file:
engine/tests/plan_mixed_legs.rs. Shared integration touches admission,
CallTerms/Entry/Context credential projection, owned admission and release
validation. These are held by #1919/#1939/#1896 and the session work; no runtime
edits in this phase and no legacy inventory prune.

1. Commit this native/API plan and README row; obtain consumer/core API fit.
2. Original RED fixture uses today's genuine command Preflight and uniform
   Mutation/Primary Plan, with real Prepare/Bind/Claim and signed requests.
   Its cancel -> GET Verify -> place transport oracle asserts the Verify Context
   is Read/Secondary with the exact selected identity. Today it is not. Keep a
   working two-Primary parallel control. Missing methods/compile errors, unsafe
   casts, fake roots or per-venue overrides are not RED evidence.
3. On the reviewed API, change only fixture admission/selection to the immutable
   declaration; preserve native transport and assertion oracles. Add adversarial
   wrong issuer/subject/generation/credential/slot tests and read-rights denial.
4. Implement shared bindings and run the native BloFin proof consumer, then the
   single production custody cutover. Venue inventory only shrinks in that PR.

Controls also cover cardinality/fallback enumeration, rejected admission consumes
no IDs, Secondary-only write refusal, Read fallback journal exclusion, late credential replacement, exact route
and recipe Identity binding, delayed provider before/after accepted sibling,
reverse/duplicate/stale pair replies, no terminal before both settle, original
root unchanged, native malformed/multiple/negative/NaN Verify rows, intervening
fill and >256 retirement/reuse cycles. No read success may erase an Unknown.

Gates: focused engine http-plans/test-support tests, feature-off compile and
unchanged default-off storage, scoped strict Clippy/fmt, both request ratchets
and bans, then BloFin+Binance native checks. Full WASM/size/replay/CPU gates run
in exact-head CI. Independent source/adversarial, caller and full-map reviews.
No merge without an explicit overseer window.

Architecture gate: this is one shared immutable binding/admission choke point
for multiple venues, not a per-venue credential switch, journal or signing path.

## Original-source checkpoint

On a828 engine source, plan_mixed_legs compiles and runs through genuine command
admission and Prepare/Bind/Claim. Both original assertions fail: Verify is
Mutation instead of Read and carries primary-ref instead of secondary-ref. The
parallel two-Primary control passes, including reverse replies, exactly one
terminal and replay inertness. No Accepted fact is fabricated for the GET.
The two REDs remain ignored, owned by codex-hcmq3f's G2 runtime adoption; only
fixture admission/selection changes when that reviewed API exists. This is
original assertion evidence, not a passing mixed-leg implementation claim.

## G2b integration and provenance API gate

G2b reviewed implementation `a01dbc4caeeda778723aa964caf6739e9420fb1c` is
genuinely merged at `057202251f1`. The only manual merge path is README: incoming
rows plus this plan's row once. Every other incoming path is preserved; the
remaining own paths are this plan and the original mixed-leg fixture. The old
fixture receives only `independent_calls: false` to retain its original policy.
Its two original REDs remain owned/ignored, with no changed transport oracle.
G2b exact-head CI is still running; this is a labelled dependency, not READY or
actual landing.

On this integration the unchanged parallel compatibility assertion passes. Both
owned REDs were explicitly executed and still fail at their original assertions:
Verify has Mutation instead of Read, and Primary instead of Secondary refs.
The ordinary target therefore reports one PASS and two owned ignores; no mixed
runtime or provenance implementation is claimed. Formatting passes. The full
merge diff also contains two inherited whitespace-only findings in incoming
credential/signer plans; the own three-path delta has no whitespace finding.

G2c draft `f39d62ed0995227923fc82d05dba253d6df12634` carries the proposed
API above. Its owner explicitly confirms no runtime binding/source pin exists.
The proposed Cx selection methods are select_installed_identity(role),
bind_selected_read(selected), and validate_identity_binding(binding, context).
G2 must retain and validate the real opaque witness at admission, Prepare,
Bind, release, DATA and receipt custody, alongside the genuine Context. A Read
binding cannot become Mutation; Mutation binds Primary only. Secondary must have
the same resolved canonical trading-account domain, environment and value as
Primary; missing subjects never establish equality.

Remaining source gates belong to G2c: the final constructor seal after actual
provider/entropy injection; atomic install and immediate dependent-setter freeze;
native-reviewed closed subject codecs; exact issuer, installed allocation and
liveness checks; and irreversible dispose/failEngine teardown with private
failure-only cleanup. G2 declaration/provenance integration remains source-gated
until its owners publish the reviewed runtime pin. No local subject, counter,
plain compatibility witness or reconstructed binding bypasses that gate.
