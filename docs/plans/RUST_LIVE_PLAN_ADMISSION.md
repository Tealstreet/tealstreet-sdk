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
proof-gated Read retries remain legal. Root expiry and an unproven
408/5xx response abort the whole owner under the shared response-proof policy.
Do not make every physical failure an unconditional whole-Plan abort.

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
