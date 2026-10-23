# Live whole-plan admission and preflight

## Scope and dependency boundary

Peel the shared, single-command engine foundation from the preserved
`feat/rust-bybit-context-groups` checkpoint `5d30986a65e` into
`feat/rust-plan-admission-core`. Start with actual command-entry and read-root
implementation #1830 (`0001c78eeb2`) and integrate actual #1820 static headers
(`2b1e35e50b2`) before source verification. The checkpoint is implementation work
in progress, not a reviewed API freeze or evidence for the new integrated tree.

This first code PR owns genuine-entry Preflight custody, bounded native calls,
checked whole-plan admission, reserved execution slots, and single-owner cleanup.
It does not migrate ordinary Bybit production code. Bybit adoption follows the
shared source review. BingX's paired-read adapter can consume its single-owner
call windows after its own exact native acceptance tests pass.

Execution/observer joining remains #1854's one shared mechanism. Metadata
coalescing and Hyro fan-out require that core and an explicit composition with
Plan; this PR grants no joined completion authority. Original-Read-root history
Completion remains separate: a Plan leg or Slot cannot be cast to that root.

Aster's exact 1000/2000/3000ms same-call retry waits need a subsequent engine-owned
retry-wakeup extension. The existing checkpoint supports step pacing, not that
retry-wait API. Do not claim this foundation alone unblocks Aster retries.

## Intended public seams

These signatures describe the review target; publish a concrete reviewed source
pin before adapters consume them. Existing Context APIs retain their behavior.

- `begin_preflight(PreflightPolicy) -> Result<Preflight, CommandEntryError>`:
  only in the command's genuine initial callback, with no command-ID argument.
- `preflight_read_context(&Preflight, Policy, CallTerms) -> Result<Context, Error>`;
  exact delivery validation, call settlement and proof-gated retry use that same
  opaque owner and the one shared Call state machine.
- `admit_plan(&Preflight, &[StepBound], PlanPolicy) -> Result<Plan, Error>`:
  atomically changes the same command generation from Preflight to Planned.
- `plan_slots`, `plan_leg_context`, `validate_plan_delivery`,
  `finish_plan_call`, and `plan_retry_context` operate on opaque Plan-bound slots.
- `done_preflight` / `fail_preflight`, `done_plan` / `fail_plan`, and validated
  empty-plan completion seal through existing engine completion and journal
  authority. They do not accept numeric command authority.
- `Venue::on_http_plan_ready` reports declared work eligibility under scoped
  engine custody. A cleanup-only retirement hook clears native continuations;
  it grants no send, publication, Store or completion authority.

`PreflightPolicy` captures the declared read and write credential intents once.
ReadOnly cannot gain write intent. WriteAfterReads may make prerequisite reads,
then admit only the declared write intent. Equal slot/reference values never
establish account or identity equivalence. Credential selection and egress
remain their shared issuance owners' policies; compose them without a local copy.

## Genuine entry and retained authority

Factor one private entry transaction used by `admit_http_command` and
`begin_preflight`. It obtains the command only from the genuine callback loan,
checks the existing resource watermark and delivery restrictions, and transitions
Fresh to Bound before issuance. Keep the current automatic typed rejection and
Revoked/no-new-authority behavior. Do not issue a dummy Context then convert it.
Preserve the group lane's read-only `entry_bound` query when composing that API.

Fold #1830's `entry_owned` retention into
`CommandPhase::Context { root, entry_owned }`. Retained Context, Preflight,
Planned and Retiring owners count against the existing 256-owner bound until
actual typed terminal cleanup. Retention is a lifetime predicate, not permission
to use a different phase's APIs. Generic Context queueing, retry and typed
post-expiry completion must continue to work. Siblings remain within the
immutable original root. Numeric completion cannot seal retained owners.

ReadOnly admission rejects an existing order scope. WriteAfterReads requires
that the existing scope is unsent, and marks it managed only after admission
succeeds. Preserve those begin-time lock guards and failure cleanup exactly.

Cached/no-I/O Preflight or Plan terminal publication consumes initial entry
custody through the shared sealed-command path. Same-callback reminting after
success, rejection or terminal publication is refused without another terminal.

## Native time envelopes

Preflight and write-plan admission are distinct phases in one command generation.
Each prerequisite read has its declared request/retry window. Successful native
mode-change redos retain today's unbounded redo behavior; no new whole-preflight
cap is introduced. Repeated external mode changes are an explicit liveness audit
item, whose future cap would need a caller-visible policy decision.

Mint the immutable whole-plan deadline only after final native steps are known.
Use checked sums of serial steps and pacing, maxima across concurrent calls, and
all declared retry/fallback branches. There is no fixed 120-second product cap.
Count every declared occurrence, including aliased fallback slices, under the
existing capacity bound; cyclic or excessive declarations refuse before work.

`RetryWindow::Shared` keeps the logical call's original window.
`FreshAttempt` permits a newly selected physical attempt's native timeout within
the declared immutable chain and plan bound. Capture that physical deadline once:
preparation, reprepare, binding and final claim cannot renew it. Refusal is not
successful DATA progress. Retry proof and journal budgets still govern whether
another declared attempt is legal.

## Atomic resource admission

Before any plan dispatch, reserve backing capacity for the complete declaration:
logical contexts, schedule/continuation storage, journal evidence, retirement
storage, and the actual safe-ID ranges for the owner timer and every declared
physical request/generation/timer and signing attempt. Preflight both Io and
Dispatch namespaces before committing either. Use actual #1789 `reserve_ids`;
never introduce a temporary allocator or direct scalar arithmetic.

Ordinary issuance must respect capacity credits already held by plans.
Validation/capacity refusal grants no Context, partial reservation, child action
or financial effect. Once committed, unused fallback IDs remain monotonic gaps;
never rewind or alias IDs after later policy, header or recipe refusal.

The initial declaration does not bound repeated host reprepare attempts. Preserve
the checked additional Dispatch reservation for those provider attempts, with the
original raw physical cap checked first. Additional reservation refusal is typed;
prior unresolved release retains Unknown, while proven facts retain the existing
journal projection. No scalar allocation or fatal allocator
fallback may replace this path.

## Live execution and settlement

Preflight-to-Planned mutates the existing CommandOwner in place. Preserve its
pending/delivery counters and the global delivery depth; retire the predecessor
physical request exactly once. Retain its exact scoped delivery witness until
natural callback exit, without clearing markers to fabricate readiness. Initial
slot custody expires at each genuine event and non-event callback boundary.

One common Call implementation backs Preflight and Planned calls. Each selected
slot has an immutable identity, attempt budget and deadline. Shared delivery
validation precedes native/cache/Store publication. Selected writes enroll in the
existing journal before queueing can fail; accepted facts and prior release
uncertainty survive all later errors. Completion requires declared work to be
settled and uses the existing journal projection, not optimistic native success.

An expired or failed Mutation call may settle only its normalized Failed journal
fact through failure-only authority, then continue already-reserved independent
steps if native policy permits. It grants no effects, retry or successful DATA
credit. An unrecovered/exhausted Read failure is terminal for this phase; declared
proof-gated Read retries remain legal under the same owner and immutable root.
Root expiry aborts the whole owner. For a Mutation, an unproven 408/5xx response
aborts the whole owner under the shared write response-proof policy. A retryable
Read response is not Mutation uncertainty and does not abort before its declared
retry is selected. Do not make every physical failure an unconditional whole-Plan
abort. Aster's native GET retry set stays 429 and 500..599 plus transport failures;
408 is not added. Refusal and retry waiting never count as validated DATA progress.

One owner timer multiplexes the plan root and declared step eligibility. Raw
expiry wins at equality. Early, stale and duplicate wakeups cannot replay work.
Mark ready slots notified before invoking the callback. Do not flush readiness
while an outer HTTP delivery scope remains active merely because an inner
callback temporarily cleared its marker.

Typed preparation/transport/Timeout/integrity failures retain their existing
cause. Preserve the owned-read failure wrapper and call incoming
`on_http_failed` inside it at the transport and both UTF-8 sites, using the same
pure failure factory for callback and fallback settlement. A callback that ignores
an unhandled owned failure cannot leak the command; a validated retry or explicit
terminal suppresses duplicate fallback settlement. Preserve all original typed
expiry/integrity assertions.

Retirement is exactly once before command-ID reuse. Readiness/cleanup callbacks
cannot borrow HTTP delivery, order-response or another command's entry custody.
Caught callback panics poison the appropriate owner and fence subsequent queries,
held signing resources and transport drain under the existing fatal policy.

## Committable implementation sequence

1. Commit and independently audit this plan before production edits. Preserve the
   old combined checkpoint unchanged. Compose the authorized #1830 retention,
   field/query guards and typed-failure resolutions locally, preserving incoming
   authority and tests. Any new source conflict goes to the maintainer.
2. Add genuine-entry Preflight and one shared Call owner. Adapt fixtures through
   real Engine command callbacks; compile/fixture corrections are not product RED.
   Verify mutual exclusion with initial Context admission and cached terminal use.
3. Peel checked declarations, full safe-ID/capacity reservation, live Plan slots,
   readiness/timer ownership and journal settlement. Keep ordinary Bybit source,
   observer groups, history Completion and retry waits outside this code diff.
4. Run the preserved native regressions through Bybit and BingX consumer
   checkpoints. Record exact native RED/GREEN receipts separately from synthetic
   engine controls; no adapter readiness claim based only on API inspection.
5. Audit the full integrated source, publish the reviewed Plan/CallWindow pin and
   open the shared core PR with an explicit prerequisite matrix and CALLERS
   behavior. Final gate is green integrated-head CI (including replay gates),
   independent reviews, and a separately executed fresh guard and head-matched
   merge in the assigned exclusive engine window. A second full artifact
   provenance collection is not a merge requirement.

## Regression and preservation matrix

- Keep all #1830 entry/retention, #1792 read-root/delivery, #1789 safe-ID,
  #1816 typed-expiry, #1838 integrity, and #1820 static-header assertions unchanged.
  When #1856 lands, genuinely integrate and preserve its Background-only exact
  delivery helper/tests rather than overwriting that additive registry seam.
- Both initial APIs in either order, cached terminal followed by remint, issuance
  refusal, repeated IDs, expiry followed by unrelated admission, and all 256 owners.
- Retained ordinary Context queue/retry and typed completion after expiry;
  non-Context phase cannot borrow original-root completion authority.
- Bybit: eight serial valid ACKs at 19 seconds (152 seconds total); eight fresh
  refusal/retry chains within the declared 320-second envelope; successful mode
  redos beyond 120 seconds; a fresh retry receives its declared native 20 seconds.
- BingX: two serial reads with one fresh 20-second signature retry each require
  80 seconds, versus 40 seconds for the no-retry variant. Scheduler progress comes
  only from validated DATA. Preserve native step plans, payloads and result bytes.
- Refusal before either ID commit; exhaustion at each safe-ID edge; no partial
  action/journal exposure; unrelated issuance cannot steal reserved capacity.
- In-place phase transition while predecessor delivery is active: unchanged
  counters/depth and exact scoped witness; initial-slot loans expire at callback
  boundaries. Additional reprepare exhaustion refuses typed without losing prior
  release uncertainty or touching an expired physical cap.
- Real timeout/malformed/preparation delivery with ignored callback, selected
  retry and explicit terminal controls; accepted facts and Unknown remain intact.
- Step pacing, exact deadline equality, duplicate/early/stale timers, genuine
  non-event readiness, nested callbacks, retirement reentry and held-resource panic.
- Aster follow-on must separately prove exact 1/2/3-second waits, four physical
  attempts, fresh signatures and failed-prefix reset through an engine-owned
  same-call wakeup. No new venue timer and no claim that step pacing supplies it.

## Caller contract and follow-ons

Before source publication, append final-behavior CALLERS text for whole-plan
admission/refusal and immutable native envelopes. NX must retain structured error
kind/outcome and reconcile unresolved writes; reservation refusal grants no
optimistic financial success. Do not put merge/proof status in CALLERS.

Ordinary Bybit's code tables, single/batch ACK proof, PartialUnknown and ratchet
pruning remain its adoption PR. Shared groups (#1854), Plan-aware history
completion, native retry-wait wakeups, and genuine entry with immutable egress
are separate coordinated extensions. Preserve the reviewed shared credential,
header, session and classification seams when those land; never duplicate them
inside a venue adapter.


## Actual classifier composition (#1837)

Compose actual `e0322577e35` before source freeze. Capture the trusted dispatch
intent when the same owner is first admitted in the genuine entry callback, and
retain intent, generation and per-leg declarations through the in-place phase
transition. Never infer intent from a venue-supplied error or default an owner
issued outside trusted dispatch into financial write authority.

Production writes use the shared `CodeTable`, `classify_write`,
`http_write_fact`/batch/failure APIs. `http_write_result` remains engine-test only.
Generic Plan retry rejects `DefinitiveRefusal`. A dedicated Plan resend consumes
`ResendGrant` through the unchanged shared `http_resend_context`, then uses the
same private reserved-slot selection body and existing journal retry. It adds no
new resend counter, grant implementation or code table. Read native resends use
shared `classify::read` evidence under the declared Read budget, never write grants.

Per-leg intent narrowing must precede the first journal registration, including
for reserved Plan calls. Reserve backing before IDs and reuse shared narrowing
validation; do not weaken the incoming registered-attempt refusal or duplicate
its allowed-intent policy in a Plan adapter. Retain all incoming grant/intent
negative controls while adapting old test fixtures to the new evidence contract.

## Actual Read execution-group composition (#1854)

Actual `e2b03d5592a` supplies command-rooted Read executions. Preserve this one
mechanism and its genuine-entry begin/join, execution-root-only legs, exact
observer seal authority, member-scope refusal and event-entry expiry sweep.
Mutation journals, background roots and Plan-aware group admission remain their
separate slices. Opaque Preflight/Plan/Slot authority cannot become a command
Context root for this API.

Compose incoming `finish_command_as(observer)` by passing the exact result of
`seal_as(..., observer)` into the existing shared terminal helper. Select
`delivery=None` only as the observer seal's local argument; never clear the
active delivery marker. Keep phase restriction inside `validate_completion_as`
for both modes, retain exact membership checks, and forget observers only on
successful Context cleanup, after any Plan retirement path that can refuse.
Execution expiry must run before a Plan timer's early return.

The approved phase representation needs the incoming execution registry's root
and entry-owned field accesses projected through Context-phase accessors. Keep
`observer_member` rejecting every other phase so `observer_authority` always has
a Context root for a genuine member. No execution-Cx API implementation is copied.

Execution-root insertion must honor promised Plan Context credits and reserve
backing before any group insertion: 250 unissued credits plus six live Contexts
already fill the shared 256 limit. Refuse a seventh root without changing group,
observer or root maps, actions or promised credits; prove a one-free-credit
positive. Preserve incoming begin's existing one-ID monotonic gap on refusal.
The execution-root helper is the common seam for future command, background and
successor roots when those consumers land; do not add a second capacity ledger.

Before source freeze, add controls for direct numeric/typed observer seal denial,
observer completion while an unrelated delivery marker remains held, observer
expiry before Plan-timer return, non-Context membership refusal, and full-credit
refusal/one-credit admission. Existing incoming group assertions stay unchanged.

## Genuine-entry Preflight transaction

`begin_preflight(policy)` consumes the same genuine callback loan as
`admit_http_command(policy)`. It has no command-ID parameter. Extract the existing
entry boundary once and keep its fresh-resource watermark, delivery exclusion,
owner pruning, bound-before-issue ordering, typed rejection and lock cleanup
unchanged. Its private issuer may retain a Preflight owner directly; do not mint
an intermediate Context or copy entry state into the registry.

Read rejection emits one failure without a write outcome; write rejection proves
NotSent. Revoked means the loan was absent, previously used or burned, and emits
no second terminal. Cached completion spends the loan. A Context and a Preflight
cannot both be initial owners in one callback, including after terminal or
rejection. Preserve every existing Context entry and retained-owner test.

Before the API change, add genuine Engine callback regressions for start, timer,
public feed and socket callbacks, opaque side effects, Context-first/Preflight-first
ordering, cached terminal/remint and invalid-policy/capacity rejection. The
baseline-only test adapter calls the old numeric API with command 7 and converts
its error type; it creates no authority or notices. Its assertion failures must
precede the signature change. Pure registry/state fixtures can call the private
issuer explicitly; native flow fixtures and new consumer evidence must use the
public genuine-entry operation.
