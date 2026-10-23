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
- `plan_slots`, `plan_leg_context`, `plan_leg_context_with_intent`,
  `validate_plan_delivery`, `finish_plan_call`, `plan_retry_context` and
  `plan_resend_context` operate on opaque Plan-bound slots. The resend operation
  accepts only the existing classified `ResendGrant`; generic retry rejects a
  definitive write refusal.
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
   authority and tests. Under the standing Plan composition authorization, keep
   both mechanisms, forward incoming data unchanged and retain the owned wrapper;
   semantic choices still go to the maintainer.
2. Add genuine-entry Preflight and one shared Call owner. Adapt fixtures through
   real Engine command callbacks; compile/fixture corrections are not product RED.
   Verify mutual exclusion with initial Context admission and cached terminal use.
3. Peel checked declarations, full safe-ID/capacity reservation, live Plan slots,
   readiness/timer ownership and journal settlement. Keep ordinary Bybit source,
   observer groups, history Completion and retry waits outside this code diff.
4. Audit the full integrated source, publish the reviewed Plan/CallWindow pin and
   open a draft shared core PR with an explicit prerequisite matrix and CALLERS
   behavior. The maintainer authorizes core CI and native consumer ports to run
   in parallel; publication does not waive either consumer acceptance gate.
5. Run the preserved native regressions through Bybit and BingX consumer
   checkpoints before readiness/merge. Record exact native RED/GREEN receipts
   separately from synthetic engine controls; no adapter readiness claim based
   only on API inspection. Final gate is green integrated-head CI (including replay gates),
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

## Classified Plan resend and intent gate

Generic `plan_retry_context` rejects `DefinitiveRefusal`, including when the
journal already holds a classified grant. A separate `plan_resend_context`
accepts only the opaque `ResendGrant` and delegates unchanged to
`http_resend_context` before the shared declared-call selection. Validate exact
Plan, slot, physical delivery, remaining declared attempt and original window
before consuming journal retry credit. The existing journal owns one-resend
consumption; the Plan adds no resend counter, code table or status precedence.

Prove the generic bypass refusal with a real released Plan write and a grant
issued by `http_write_failure` from a const CodeTable. After refusal, DATA
identity, actions and reserved IDs remain unchanged; the opaque-grant path can
select its declared retry. Retain wrong-slot/wrong-grant/reuse/expired-window
controls and existing selected-retry queue-refusal uncertainty.

Per-leg intent must be narrowed before the first journal registration, using
one shared engine validation table. Admission reserves the intent vector along
with journal, Context and ID backing. Do not relax the incoming registered-leg
refusal or move journal enrollment after queue/preparation failures.

## Deferred boundary audit corrections

Plan readiness is engine-owned work. During `on_http_plan_ready`, mask the exact
outer `order_response_token` as well as excluding HTTP/entry custody, then
restore the token unchanged after normal return. A caught callback panic keeps
the existing poison fence. A due readiness callback after an unrelated real
order read must never reconcile that read's uncertainty fence; the original read
callback retains its own authority.

Root-timer expiry and readiness queue refusal can seal a placement and release
its lock. Run the existing deferred-command bookkeeping at that boundary without
requiring another external event. Extract one shared deferred-work routine and
invoke it after the readiness loop has returned from every callback; do not call
`finish_event_scope` recursively inside readiness or duplicate its order-lock
policy. Deferred callbacks receive no outer HTTP delivery, command entry or
order-read token, and the saved outer values are restored unchanged.

Add genuine response/timer regressions before the fix: an accepted order read
makes a delayed Plan ready but leaves uncertainty fenced; a valid read callback
can still reconcile; a Place Plan with a queued cancel completes that cancel at
root expiry and at readiness queue refusal without another event. Preserve all
prior readiness, panic, observer and deferred-command assertions.

## Native Read early success

BingX's paired metadata walk can return the current cached settings after the
first validated leverage response when publication becomes ineligible. It then
issues no margin-type read. Keep the original declared root; do not add a call,
re-admit a plan, or infer fresh-cache publication from structural completion.

Add `done_read_plan_after_data(plan, slot, context, result)` as a narrow terminal
transaction. Require a live Read Plan, the exact current released DATA attempt
and slot, and no other selected/queued/in-flight call. Validate every refusal
before settling the current call. The operation settles that call and retires
remaining unissued declarations through the existing owner/journal cleanup;
it grants no new send, retry, publication, group or financial authority.
Mutation, absent/foreign/stale/expired delivery, a settled replay and another
issued call all refuse without changing Plan state, IDs, actions or credits.

Prove the native two-step early-success shape with an original assertion RED
before this operation, and keep a normal two-call positive. Add wrong slot,
Mutation, absent delivery, another active call and post-settlement/replay controls.
The native adapter still owns validated-success decoding and the exact captured
publication/fallback policy; the terminal operation does not warm its cache.

## Shared binary size containment

The first integrated CI build exceeds Aster's fixed WASM budget by 20,415 bytes.
Keep that failure as evidence and preserve the budget. First contain cold owner
retirement and failure paths: move Plan seal validation/journal projection into
one non-generic registry function and keep cold shared finalizers out of line.
Do not add a venue-specific policy, allocation workaround or new permission.
Keep the existing validation, callback and cleanup order byte-for-byte in the
extracted bodies. Run the full feature-on/off engine controls and strict checks,
review the source delta, then measure the unchanged Aster budget in normal CI.
A size reduction is not claimed until that CI measurement exists.

## Zero Plan scans on public events without a Plan

The first integrated Bybit verifier failure is Hyro steady CPU. Fresh landing
logs classify the same 5,260 Spot catalogue diagnostics as known failures; those
are not a new catalogue regression. The new Plan flushes currently scan every
command owner multiple times per public event even when all owners are ordinary
Contexts or execution observers and no Plan exists.

Maintain one bounded index of admitted Plan command IDs, reserved fallibly before
both ID namespaces commit, populated only at the Preflight-to-Plan transition,
and removed at Plan retirement. All Plan scans iterate this index; an O(1)
empty-index guard skips the Plan flush entirely. It is an execution index, not a
second authority or deadline owner. Retained Preflight/Context/execution owners
must never enter it. Keep exact retirement, panic, timer and readiness ordering.

Before the fix, instrument actual visited owner cells only in tests/test-support
and prove a native Hyro engine with a live RefreshMarkets observer pays zero Plan
visits across public ticker events, dispatch preparation and drains. Retain the
assertion RED, then run it in native release mode and preserve all active-Plan,
root-expiry, reservation-refusal, command-ID reuse and observer-isolation controls.
Only normal CI establishes the WASM steady gate; no local WASM build is used.

The first non-generic seal extraction recovers 2,957 bytes in normal Aster CI,
leaving 17,458 above the same fixed budget. Next keep the idle Plan guard inline
but move its active scheduling body out of line. Likewise keep owner-boundary
custody clearing/fatal checks inline, and place only the nonempty retirement
queue body out of line. Share the extracted deferred-command body rather than
inlining a copy into both terminal boundaries. These are code-generation changes
only; preserve every callback, clear/restore, panic and timer operation in order.
Measure size and idle CPU together in normal CI; do not infer either from source.

## Public classified partial-unknown evidence

The internal journal already distinguishes a validated subset with unresolved
remainder (`PartialUnknown`) from a wholly unproven reply (`Failed`). Preserve
that distinction at the public classifier boundary for ordinary Bybit adoption.
A classified Unknown fact with `known_effects=true` maps to PartialUnknown. In a
full-count, nonempty-ID batch, at least one proven Accepted or Satisfied item plus
an Unknown item maps to PartialUnknown, retaining IDs/total/first-failed index.
All-unknown items remain Failed. A count mismatch, empty accepted ID, or unproven
HTTP status remains Failed with evidence-only IDs and no effect authority.

Change no table/attempt/response binding, retry grant, effect budget or journal
projection. Both forms remain Unknown/non-resendable, and neither seals success.
Only the validated subset receives effect authority. Add original assertion REDs
for the public fact and batch mappings, plus all-unknown/count/empty/5xx negatives,
exact-attempt binding and final Unknown-with-proven-IDs journal projection.
Classifier ownership reviews this exact mapping before its source pin is shared.

## Remaining shared cold-path containment

The next normal CI measurement at `0f06f289fe4` is 1,362,187 bytes for
Aster, 18,699 above the unchanged 1,343,488-byte budget. Bybit and Bitunix
replay jobs both pass. That measurement includes the public partial-unknown
mapping and actual capability/history integrations; it establishes no saving
from the preceding outlining.

Try three bounded shared registry boundaries: keep an inline empty Plan-index
guard in `end_plan_admission`, outline only its nonempty custody-clearing loop,
and prevent reinlining `record_owned_read_failure` and `ContextRegistry::finish`
into generic engine failure/finalizer paths. Keep all validation and mutation
order exact. These bodies are non-generic already; LLVM may already share them,
so this is a size experiment, not an established duplication diagnosis or byte
forecast. No new policy, rights, storage, thresholds or callback behavior.

Preserve the native zero-Plan-visit regression and every active Plan/retirement
control; run feature-on/off engine tests and strict checks, then independently
review the extraction and measure normal CI again. Do not claim a byte floor
or savings without the resulting artifact measurement.

## Measured reachability and family feature isolation

The owner stopped further outlining experiments before source was committed.
Paired name-preserving diagnostic CI at `39955649184` reproduces the exact
production lengths, but type/generated-adapter ordering prevents byte equality
after simply stripping names. The owner explicitly accepted this same-pipeline
comparison for function-size attribution; these are not replay substitutes.
The temporary diagnostic branch is deleted after downloading both artifacts.

Encoded function-body deltas account for all 25,156 bytes of code growth.
Examples include `failure_projection` +6,191, `retire_http_command` +4,759,
`http_end` +2,973, `finish_owned_read_failure` +2,431, `plan_seal` +1,972,
`record_owned_read_failure` +1,650 and CommandPhase cleanup +1,304 bytes.
Extraction offsets include `seal_as` -6,666 and `finish_command` -5,832;
positive rows alone are not additive net savings. Twiggy retention paths show
Aster linking Plan seals, readiness, failures and cleanup through generic event,
HTTP and owner-phase boundaries despite never issuing a Plan.

Add a default-off engine `http-plans` Cargo feature. Bybit and BingX opt in
through their actual family dependencies for the native adoption gates. Other
families compile without Preflight/Plan constructors, owner phases, schedules,
slot reservations, readiness/retirement storage or callbacks. Compile those
bodies out, rather than checking a runtime boolean or retaining dummy APIs.
Keep genuine Context entry/read validation, execution groups, safe IDs, the
existing managed Context CommandWrites journal and classified PartialUnknown
behavior. The common journal is not Plan-only and must remain.

Feature-on preserves the reviewed API and semantics. Feature-off shared hooks
are empty/zero where a common boundary needs them, with no Plan storage, scan,
allocation or timer. Save and compare complete Cargo feature closures for all
28 family builds, allowing the new feature only through Bybit/BingX. Run the
full engine feature-on and feature-off controls, native Bybit/BingX gates and
strict checks, independently review the cfg boundaries, then measure normal CI
Aster size and Bybit/Bitunix replay. No threshold, fixture or reference changes.

Workspace test/clippy commands unify venue dependency features and therefore
always enable `http-plans` once Bybit/BingX opt in. Add a dedicated native CI
matrix entry that resolves each standalone family's actual engine feature list,
deduplicates all combinations and runs engine clippy plus tests for each,
including standalone Plan-enabled families without order locks. Include the
explicit `order-locks,test-support`
configuration and the empty feature set. Resolve enabled features with Cargo's
`{f}` output, not only rendered feature edges, which omit root-enabled flags.
Give the mode job a 90-minute timeout and report its measured wall time; other
native jobs retain their 30-minute timeout. Keep the normal workspace gates.
Document that venues
overriding the Plan readiness/retirement hooks must opt in explicitly, even if
a combined workspace build already enables the feature through another venue.
