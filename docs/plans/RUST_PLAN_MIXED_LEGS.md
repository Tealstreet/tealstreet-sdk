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
preparations precede the native pair's enqueue; a delayed provider must not cause
a local eager-sign fallback. Reverse, duplicate and stale deliveries must not
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
struct LegDeclaration {
    operation: Operation,
    selected: SelectedIdentity, // opaque shared Selection + Identity + routing
    // Retry and pacing remain declared call bounds, never callback overrides.
}
// Same internal admission used by uniform, conditional and physical Plans.
cx.admit_plan_with_legs(&preflight, steps, root_policy, legs, terminals)
```

SelectedIdentity is a typed PLACEHOLDER, not a new key carrier or an available
API. codex-9czdjh owns the shared Selection-managed signed-read helper;
claude-4yjlw0 reviews its consumer/signing fit. Consume its reviewed signature
before runtime. The intended result binds slot, exact Identity refs, subject,
credential generation and RoutingIntent; Selection::credential(KeyRef) supplies
the managed Context's Credential. The same selection builds the family recipe
for request_signed_managed. There is no separate runtime Identity lookup that
can reselect after admission.

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
A configuration change retires the owner, never reselects its remaining legs.

The final Rust shape must prevent a binding count/order mismatch. Either a
recursive declaration attaches bindings directly to each CallBound, or a checked
root-first ordinal slice exactly covers Schedule's complete enumeration,
including every fallback occurrence. API review chooses one before source.
Reject duplicate/missing/extra/wrong bindings before reservation; failure leaves
Preflight, activation custody and IDs unchanged. All fallible validation and
storage preparation precede the same admission commit as uniform declarations.
Conditional terminals and future ReconcileBound remain fields of that single
internal declaration. No alternate constructor bypasses readiness or custody.

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

Two concurrent Primary writes settle independently and order-independently;
exactly one root terminal follows the barrier. #1939 independent_calls is an
explicit dependency for faithful continuation after delivered 408/5xx. Until
its actual integration, core's stop/Unknown behavior is not described as native
parallel completion; do not add a venue fallback. Controls cover accepted plus
401/429/unlisted/transport/timeout, both refused, accepted sibling then provider
refusal, and refusal before any release. Accepted IDs become structured Partial
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
no IDs, Secondary-only write refusal, late credential replacement, exact route
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
