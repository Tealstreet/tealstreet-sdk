# Command replacement groups and ordinary Bybit reference

Local follow-on to draft #1741, starting from b299d70fe865e634137eed469a229180bc158765
on `feat/rust-command-replacement-bybit`. Owner authorized local work while #1741
waits for #1757. No push or PR until #1741 lands. Watches remain active; return
to the core branch immediately when #1757 lands, committing local checkpoints
before switching. Real-integrate the actual core before final publication.

## Problem and boundary

The shared CommandWrites journal can retry one logical leg after a definitive,
effects-free refusal. It cannot yet express a whole refused batch replaced by N
single-order legs. Bybit handles native 10001 with fallback steps but retains the
batch diagnostic and venue-owned refused/effective/unknown/resent aggregation.
Existing native fallback already returns successful IDs when every single is
accepted; this is a missing shared journal representation, not a second native
success bug. Native
gateway failures can also enter benign already-gone/not-modified branches before
uncertainty is sealed, applying cancellation effects or completing optimistically.
The initial apply_local_write gate already rejects non-2xx retCode0 envelopes;
preserve that guard rather than claiming it is absent.
Extend the engine journal once; venues declare native facts and replacement
intent only. No second executor, callback-order finalizer or error-text retry.

Scope is the shared replacement relation plus ordinary Bybit aliases (linear,
inverse, spot, demo and their product paths), including generation-bound mode
preflight and existing delays. Hyro remains explicitly legacy/inventoried:
physical command0/shared waiters need a separate shared multi-owner observer and
reconciliation-proven retry contract. Do not claim whole-Bybit completion.
KnownPartial is already in #1741; reuse it instead of adding another variant.
Managed history Completion, credential/nonce authority, WS/session supervision,
financial freshness tokens, error-classification fan-out and NX edits stay out.

## Shared journal design

1. Add a bounded, acyclic replacement relation to CommandWrites in
   engine/src/request_context.rs. A released parent attempt can be replaced only
   after a recorded definitive Rejected with no effects/IDs/overflow, while the
   command and parent policy/deadline remain live. Accepted, KnownPartial,
   Failed/unknown, unsent, superseded, sealed or already-replaced parents refuse.
   Plain retry keeps its existing same-leg semantics.
2. Proposed pure seam: replace(parent RequestId, child_legs &[u64]) -> Result.
   Child legs are engine-issued context identities, not caller numbers exposed
   through a public numeric API. Require nonempty, unique, fresh child legs;
   reserve their bounded journal capacity atomically before changing the parent.
   No partial group, context leak or supersession on capacity/validation failure.
   Existing attempt/text/exact-ID budgets bound both reserved and physical work.
   Registration consumes an existing reservation instead of bypassing limits.
3. Supersession changes terminal projection, not history: retain the parent's
   release/result/evidence, but terminal reports and success follow replacement
   leaves in parent planning position and declared child order. All accepted
   leaves recover the parent; rejected leaves stay failures; known effects plus
   refusals use existing partial, and unresolved sent work stays Unknown.
   Missing/unreleased replacement work cannot yield success or NotSent after a
   parent release. Preserve the prior-send/failed-retry conservative Unknown
   rule, including failed replacement preparation. Exact known IDs survive all
   group transitions; overflow remains Unknown and cannot authorize replacement.
   Nested replacements must be bounded, cycle-free and deterministic; journal
   scans remain bounded by the same command limit.
4. ContextRegistry/Cx owns the public bridge: a context-bearing replacement API
   validates issuer, owner generation, parent mutation policy and current native
   delivery/result before allocating the child Contexts and journal reservation
   as one commit. Children remain Mutation, retain credential intent and have
   deadlines no later than the original parent/root bound. No raw-ID fallback,
   new authority on expiry, or parent refusal suppression before admission.
   The implementation audit must settle the exact allocator/rollback shape and
   logical-order representation before coding this bridge.

## Ordinary Bybit adapter

- Preserve current command root through ResolveMode redo and delayed steps;
  create bounded child Contexts when a planned read/write is actually queued.
  Declare a nonempty command root Mutation before its ResolveMode Read child;
  empty read-only work must never escalate. Explicit preflight is Read and
  writes are Mutation; never infer from
  method. Retain original root/leg bounds through timers, signature retries,
  asynchronous providers and returned mode-version changes.
- Ordinary Request/Rest carries managed authority into actual release, retry,
  native callback and preparation failure. Use existing engine final-claim,
  route/provider and release metadata, not a local sent timestamp or callback.
  Keep Hyro's explicit legacy route separate. Background/account/history work
  outside this reference remains legacy; do not extend the allowlist.
- Record native10002/10004 as effects-free Rejected before one same-leg retry;
  use typed engine policy, never `network lost; error sign` matching to replay
  a managed write. Lost/unknown writes do not automatically replay. A released
  earlier attempt followed by provider/transport failure remains Unknown.
- For native10001 batch fallback, submit definitive refusal then the shared
  replacement declaration before staging singles. A full successful fallback
  returns success and child IDs; its superseded parent does not become a fatal
  or terminal rejection report. Preserve the existing one native batch-refusal
  commandError diagnostic before done using validated command_error_context;
  historical reporting is separate from terminal projection, with no duplicate
  journal report. Preserve request bytes, client IDs and existing
  native single/batch semantics; partial/unknown children cannot hide behind a
  recovered sibling. Preserve exact IDs and native refusal text as evidence.
- Native codec classification precedes any local Store/ownership patch. A 500
  with retCode0, malformed/ambiguous/mismatched ACK or unproven item never grants
  successful mutation authority. Known mixed cancel effects use KnownPartial,
  not whole Rejected or invented Unknown; unresolved siblings still dominate.
- Replace ordinary Job's refused/effective/unknown/network/signing_error outcome
  selection with shared facts, deterministic reports and done_context/fail_context.
  Native planning/joins/counters remain; legacy Hyro retains its existing rails.
  Review any mixed branch so managed authority never falls back numerically.

## Committable phases and acceptance

1. Engine tests first: original inability to recover a replaced refusal must be
   RED. Implement pure replacement/reservation/projection with ordered children,
   nested groups, capacity rollback, malformed/duplicate/foreign relationships,
   repeated replacement and stale/sealed authority controls. Keep prior journal
   retry, KnownPartial, bounded money evidence and rejection-order tests intact.
2. Context-bearing atomic bridge with real Engine preparation/claim tests:
   original deadline, wrong issuer/generation, revoked command, denied parent,
   expired group and refused provider; no authority or journal residue on failure.
3. Ordinary Bybit native RED controls, then reference adoption:10001 -> all
   accepted singles; accepted+rejected known partial with exact PartialIDs;
   accepted+pre-send refusal known partial; transport loss Unknown; prior native
   refusal then failed retry Unknown; mixed cancel effects without IDs; native
   500/retCode0 no Store/ownership patch; both callback orders; mode redo and
   delayed timer after reused command IDs. Tests drive real preparation and final
   claim. Preserve every current Hyro/protocol-clock/signature/ACK assertion.
4. Shrink only the exact Bybit legacy inventory (tests/maps committed first),
   independently verify no unaffected map moved, and keep all remaining legacy
   entries/hard cutover gate explicit. Re-run engine+Bybit+Binance focused native
   tests, strict Clippy/fmt, bans, ratchet adversarial controls and secret scan.
   Independent correctness/adversarial reviews before each phase commit.
5. After actual #1741 lands, integrate it and any required actual dependencies;
   open the follow-on PR, run fresh CI and complete authenticated raw/gate/package
   proof. Declare every intentional outcome/request change without exclusions;
   preserve CPU/size thresholds. Separate fresh guard and exact-head merge only
   when authorized by the normal queue.

Architecture gate: No. This is an engine journal/lifecycle extension with native
codec facts and one reference adoption, not a Bybit-specific cross-cutting fix.

## Independent plan audit refinements (before source)

- Supersession revokes the parent's mutation-delivery/result/retry/replacement/
  release authority, not just its terminal error. Require latest same-leg parent
  and change the shared authority validator so an old callback cannot seal the
  command after replacement. Explicit root completion remains generation-bound.
- Reserved children participate in terminal projection immediately, before a
  RequestId exists, including expiry/cancellation/queue refusal. They cannot be
  ignored by succeed or all_rejected; parent release plus failed child
  preparation is conservative Unknown. Keep reservations until owner sealing.
- Prepare all child immutable Contexts after checking the entire policy/count/
  context/journal/relation budget, then one infallible registry+journal commit.
  Do not loop issue_bound with rollback. Monotonic allocation may leave checked
  gaps on refusal but is never rewound. A refused replacement leaves the parent's
  recorded native rejection and existing authority unchanged.
- Use bounded explicit root planning rank plus child ordinals/path, not packed
  numeric leg arithmetic. Shared terminal traversal drives errors, reports and
  partial metadata. Test siblingB issued before children ofA, nestedA groups and
  opposite callback order, under the same 256-work budget.
- Preserve the existing fallback success/progress/client-ID/one nonterminal
  commandError/done assertions. The first native RED is unsafe gateway ACK
  publication; missing shared supersession gets pure-engine regression and
  explicit relation tests. No invented existing native fallback failure claim.

Independent architecture audit: No. These refinements belong in the one shared
registry/journal bridge; no per-venue queue/authority/classification workaround.

Re-verification for native RED: venue_error already rejects non-2xx retCode0
before apply_local_write. The concrete unguarded path is later native benign
message matching: Cancel with a gateway408/500 envelope whose message says
order-not-exists can call note_order_terminal/mark_canceled and complete, because
http_unknown is only propagated when errors is nonempty. Add a demo cancellation
regression that keeps its original Store row and seals Unknown for those gateway
responses, including retCode0 and110001; retain a valid200 already-gone control.
This tests the approved native certainty boundary without inventing a missing
initial guard. Source remains unchanged until shared-journal plan/audit gates.

Local original-native RED checkpoint (no production edits): the new real engine
Demo cancellation matrix confirms all four gateway408/500 plus retCode0/110001
responses remove the existing row and report success. Expected: unchanged row
and Unknown. The identical native200/110001 already-gone control passes. Exact
assertion failure is retained in /tmp/codex-command-replacement-gateway-original-red.log;
the existing fallback/client-identity diagnostic assertions remain untouched.
This branch is intentionally RED until the reviewed shared lifecycle/native-fact
adoption phase; do not push or open a PR before actual #1741.

Pure journal representation selected before implementation: retain the existing
flat physical-attempt array and add a bounded logical-leg array. Each leg has its
opaque id, optional (parent index, declared child ordinal) and superseded flag.
No retained path vectors or packed integers. Count attempts plus unissued
reserved children against the existing capacity; first registration consumes a
reservation, retries consume new physical capacity. Atomic replace validates
latest released effects-free parent, all fresh child ids and total capacity,
reserves storage, then marks parent and appends children without further failure.
A bounded parent-index walk defines root-rank/child ordering and detects prior
ancestor release; latest() excludes superseded parents. Terminal projection must
include active reserved leaves, preserve historical attempt uncertainty, and
keep child-provider failure after ancestor release Unknown. Root completion/
registry wiring remains the separately audited next phase. No venue may call
the pure numeric journal directly; ordinary Bybit uses the context bridge.

Pure implementation audit correction: selected native/fallback PartialIDs keep
their exact first-occurrence prefix. Append additional facts in replacement
planning order; do not reorder or recharge retained prefix IDs. Two unchanged
evidence-budget assertions caught the original ordering change and now pass;
no assertion was weakened. Oversized replacement child slices are rejected
before quadratic duplicate validation. Spare-capacity controls distinguish
revoked-parent retry from an earlier capacity error and cover retried ancestors
plus replacement-child parked/provider/rejected/accepted retries.

Registry phase authority refinement, independently reviewed before coding:
retain exact first-issued command-root identity/metadata for generation-bearing
logical no-delivery timers, including cleanup of expired logical handles. A
superseded mutation delivery cannot publish diagnostic/result/seal even if its
caller passes that root authority. Sending/retrying/result/claim authority of a
root used as the physical parent is revoked while its original no-delivery
logical timer authority remains. Never clear delivery scope to bypass this.
Historical native batch diagnostics publish once before replacement commits,
with explicit empty terminal report metadata, preserving the original error/done
assertions without duplicate terminal errors. Shared API tests must exercise
the same Context as root and sent parent, foreign root clones and old callbacks.

Local pure-journal checkpoint: eight new controls plus all464 engine order-lock
tests and284 Binance tests pass. Strict engine/Binance/Bybit all-target Clippy,
formatting, bans and11 ratchet adversarial controls pass; independent pure-slice
review CLEAN, architecture No. A compiled control disabling reserved-leaf
uncertainty fails exactly Some(Rejected) versus Some(Unknown); exact restored
source SHA256 3f1fa5d8ee78129a83d67a3f7d9e1c1b260fcc69a6343eef803f5b220461ff41
and the restored control are GREEN. No old evidence assertion was changed.
No registry/root-completion/public bridge or native Bybit production migration
is claimed: the committed native gateway regression intentionally remains RED
until that adapter phase. Local-only; no PR/push or complete raw/CI proof.

Next registry bridge API checkpoint before source: propose
Cx::http_replacement_contexts(parent:&Context, children:&[Policy])->Result<Vec<Context>,Error>.
Require the exact current parent Delivery, command Mutation, timed live handle,
DefinitiveRefusal retry eligibility and recorded effects-free latest native fact.
Reject empty/over256 input before allocation; child operation/credential/pacing
match parent, retry policy cannot broaden parent, and every child deadline is
min(parent.deadline,now+timeout). Reserve a checked monotonic io.next_id range
(never rewind on refusal), stage every immutable issuer-bound Context, validate
active-context+journal capacity and pre-reserve context storage. Call the journal's
atomic replace last; on success only infallible expired-handle cleanup and staged
context insertion remain. Failure retains original native refusal/authority and
leaves no visible child/group. No loop over issue_bound and undo.

CommandOwner retains the exact original completion-root Context, independent of
logical-handle cleanup. Shared validate_handle rejects superseded send/retry
handles; completion separately allows exact original root only for logical
no-delivery timers. ANY superseded/historical mutation Delivery rejects completion
even with that root argument; superseded nonroot authority also rejects without
a Delivery. Root-used-as-sent-parent, expired-handle root timer, wrong issuer/
generation/modified-root metadata and late parent callbacks are explicit tests.
No new deadline/send permission; report original batch diagnostic before atomic
supersession, with empty terminal journal report metadata. This bridge remains
engine-owned; no direct numeric journal calls from Bybit.

Registry plan-audit correction before source: keep validate_handle as cleanup/
provenance only. Put superseded send denial in the timed validate path; otherwise
finish_http_context could strand old pending/delivery holds. Superseded/historical
Delivery is checked before any exact-root completion exception. Reserve returned
Vec and HashMap storage for the pre-cleanup peak before journal.replace, and
never increment pending until physical registration. Existing shared io.next
wraps; this bridge must explicitly checked_add its entire bounded range, return
Capacity on exhaustion and never invoke wrapping per-child allocation.

Add error-atomicity snapshots for invalid nth child, wrong/sibling/foreign parent,
expired/denied delivery, context/journal capacity and near-u32::MAX allocator;
no visible children/group, no parent authority change or ID reuse on refusal.
Then prove a valid smaller group still admits. Real engine tests cover old
preparation/permit/retry/fact/diagnostic callbacks, no-delivery root after cleanup
and exact child credential/pacing/deadline bounds. Cleanup remains once-only.
Independent plan review CLEAN with these constraints, architecture No.

Local registry WIP checkpoint before returning to the core dependency: the
context-bearing atomic bridge and exact-root retention compile, and all13
replacement tests pass (five registry controls plus eight pure journal tests).
Coverage includes invalid nth policy, foreign/sibling/expired delivery, active
context and journal capacity rollback, unchanged native parent fact on refusal,
shortened child bounds, superseded delivery versus logical root completion and
checked allocator exhaustion. This is not the completed registry phase: real
Engine prepare/bind/final-claim controls and final independent audit remain
pending. No ordinary Bybit production edit, push or PR; its original gateway
regression remains intentionally RED. Priority switches to #1741 as #1757 lands.

Independent checkpoint audit found no concrete production blocker; architecture
No (shared registry/journal). Remaining tests must combine the same root and
physical parent with expired-handle cleanup, modified/foreign root authority and
reused command generations, absent/wrong current Delivery and Never-policy
parent. Add full atomicity snapshots and real Engine old-preparation/permit/
callback fences before claiming the registry phase complete. Private unit
Delivery construction is not a substitute for engine-scoped attempt generation.

Local follow-on checkpoint after real-integrating core746817: added an actual
Engine prepare/bind/final-claim/refusal/replacement callback test, using the same
Context as logical root and physical parent. It rejects old completion, facts,
retry, diagnostic, preparation and duplicate physical callback; after expired
handle cleanup only exact root logical completion remains, reservations seal
Unknown, and reused command generations reject the old root. Focused test is
GREEN. Other previously listed authority/atomicity controls and final phase audit
remain pending; no ordinary Bybit production migration or publication claim.
Priority returns to #1741 after its fresh Binance CI failure; this branch stays
local and unpublished.

Local continuation after core502caee: genuine conflict-free integration retains
the actual Phemex history core and prepared-wrapper fixes. Add public-bridge
acceptance controls for absent callback Delivery, another physical request's
Delivery and Never-policy parents. Each refusal must preserve the exact live
parent context, journal latest attempt, pending count and queued actions, with
no child authority. The absent-delivery refusal must not burn IDs; validated
attempts may burn monotonic IDs but may never reuse them. A correct definitive
refusal must still admit children after a wrong callback was denied. These are
coverage controls, not a claimed new production bug. Signed parked-provider,
stale permit and full atomicity controls remain pending. Keep core CI watching
and return for actual #1764 integration; no publication before actual #1741.

The added callback-authority control and20 existing replacement-filtered engine
controls pass; strict engine Clippy and formatting pass. Independent coverage
review CLEAN: distinct refusal reasons, exact parent/latest/pending preservation,
no queued resource/action, monotonic burned identities and subsequent valid
admission are asserted without changing an old assertion. Architecture No.
Signed-child/provider/permit and complete atomicity acceptance remain pending;
ordinary Bybit production is still unchanged and its original RED test remains.

Next local acceptance slice, after genuine corea0c528 integration: extend only
the existing PhysicalReplacement test venue with an opt-in signed mode, opaque
test SignStep and child-callback completion count. Production journal/registry
stays unchanged. Drive actual managed preparation, parked Sign actions, validated
provider results, bind permits and final physical claims for parent and children.
Cover all accepted replacement children; one accepted child followed by provider
refusal (Unknown with exact accepted ID); and provider refusal plus a result at
the original raw deadline (Unknown, no child release). The fixture deliberately
calls optimistic done_context only after both child callbacks; engine facts must
override it. Preserve the existing plain-handoff assertions verbatim. Confirm no
bind/send before provider completion, no deadline extension, single command seal,
late/duplicate parent/provider/permit rejection and retained original diagnostic.
Review the test plan before edits. Use focused --lib replacement controls and
strict engine Clippy; no local WASM/replay. No PR/push until actual #1741, whose
exact-head CI/archive watches remain active. Return for any core failure/dependency.

Signed-test plan review: native facts must use each emitted child Context while
root completion runs within that real child's Delivery; never clear callback
scope. Provider/deadline failures are engine-owned, with no invented fixture fact.
Count only successful physical claims (parent+2, parent+1, parent-only) and retain
exact IDs from accepted native responses. Repeated parent/provider/permit events
must not change callback, diagnostic or terminal counts. Reuse the original100ms
raw cap; allow normal cancellation/tombstone retention. This drives the engine's
emitted Sign/action protocol, not an executing native-host SignFuture. Independent
plan review CLEAN, architecture No; no production or phase-completion claim.

Local signed-fixture scaffold checkpoint only: opt-in test recipe/child callback
paths compile; all21 existing replacement-filtered engine controls, strict engine
Clippy and formatting pass. Independent scaffold review CLEAN, architecture No:
default plain behavior and every original plain-handoff assertion remain intact,
child facts retain child Delivery and optimistic root completion stays inside it.
The actual signed acceptance tests have not yet been added, so this is not new
signed coverage or a completed phase. Return to #1741 as its rebuilt-WASM Binance
job/lifecycle replay passes; maina0 stays frozen for final CI and evidence.

Local signed acceptance checkpoint: the real Engine matrix now passes for all
accepted children (exact IDs and success), one accepted child plus refused sibling
(Unknown retaining its exact ID), and refused child plus provider result at the
original100ms deadline (Unknown, no child release). All22 replacement-filtered
engine controls pass. Independent review found two coverage gaps; the test now
also asserts physical pending/managed maps and signer work retire, and rejects
a duplicate child claim while its command is still live. Late duplicate results
cannot emit another seal or diagnostic. This exercises emitted Sign actions,
not native-host SignFuture execution. Protocol/route reprepare fencing and full
registry atomicity acceptance remain pending, as does ordinary Bybit adoption;
its original gateway regression intentionally remains RED. No push/PR until
actual1741; its maina0 CI and authenticated evidence watches remain active.

Next local test-only slice: drive one signed replacement child through a bound
but unreleased permit, then separately invalidate it by protocol calibration or
route Stale before100ms. Require an emitted Cancel plus fresh Prepare retaining
that child's exact Context/deadline and reduced raw timeout. Old live permit and
provider result must never bind/send or seal; prepare/sign/claim the new attempt
and accept its native ID, then accept the unaffected sibling and require success
with both exact IDs and one original diagnostic. New provider attempts are
distinct, physical release remains parent+2, duplicate old cleanup cannot retire
the current resource, and all physical work retires. Use existing production
Engine APIs and test-only protocol observation, no allocator/source change.
This composes existing core reprepare fences with the replacement relation; it
is not a new bug claim or native-host provider execution proof. Strict scoped
checks and independent review precede checkpoint; core CI remains priority.

Reprepare plan audit: retain the SAME HTTP Attempt/RequestId and Context; change
only preparation sequence, permit and provider signing ID. Protocol calibration
is global, so leave the sibling unrouted until after observing it. Stale route
callbacks use the existing preparation_failed API. Old cleanup must not emit a
new Prepare or Bind, and the fresh accepted child must prove no stale failure
fact was recorded. Extract only the existing signed-test helpers into one local
test module so both acceptance tests drive identical actual Engine APIs.
Independent plan review CLEAN; architecture No.

Local reprepare acceptance checkpoint: both protocol-revision and route-Stale
cases pass, along with all23 replacement-filtered controls and strict engine
Clippy. Helper extraction preserves the prior three signed scenarios. Each fresh
child keeps its exact Context/HTTP attempt/raw100ms deadline, receives distinct
preparation/provider authority with90ms remaining timeout, and rejects live old
permits/results/cleanup without another diagnostic. Fresh children still return
exact success/IDs and retire physical work. The initial newly written expiry
expectation was corrected: this recipe declares no absolute signature expiry,
so None is correct; the Context bound is separately asserted. Independent final
review CLEAN, architecture No. Full registry atomicity and ordinary Bybit
adoption remain open, with its original gateway RED retained. Local/unpublished.

Local integration checkpoint953ffd1: genuinely merge current frozen core8368,
including actual1767/1760/1761 and the native fixture correction. All seven owned
and22 incoming paths are disjoint; every5303 combined tree entry is preserved.
No PR/push;1741 exact-head/full-data/package watches remain active.

Next test-only atomicity slice: exercise the public Context replacement bridge
with a second-child identity collision after its first prospective child was
constructed, while an unrelated expired context is still retained. Denial must
preserve every original context/owner bound, pending count and parent journal
fact, create neither child authority nor HTTP/action, and leave expiry cleanup
until successful admission. Monotonic checked allocation may burn the denied
range; a subsequent valid group must use fresh IDs and preserve the original
raw deadline. Add registry-level controls for accepted/KnownPartial/failed and
unresolved parent facts reaching the final journal rejection, plus an old
command generation after finish/reissue. No source change or new bug claim is
planned: these cover rollback/composition boundaries before ordinary Bybit
adoption. Retain all existing23 replacement controls and native/parser assertions.
Independent plan review precedes edits, then focused engine tests/strict checks
and review before the local checkpoint. Return to1741 for any landed dependency.

Atomicity plan audit CLEAN: target the otherwise-valid final journal stage with
exact RetryForbidden, not an earlier policy/deadline/capacity error. Stale-owner
controls run before100ms. Snapshot semantic owner/context/journal/pending/action
state only; allocator reservation capacities are outside the contract. Permit
only monotonic identity burn, then prove fresh successful admission performs the
previously deferred cleanup. Architecture No; no production edits/new bug claim.

Local atomicity checkpoint: all26 replacement-filtered engine controls and strict
all-target engine Clippy/fmt pass. All original23 controls remain unchanged.
Second-child collision preserves the entire semantic registry/owner/journal and
expired handle before a fresh checked range commits; denied facts preserve exact
accepted/known/failed IDs through sealing (including accepted plus presentation
failure). Old/current generation cross-pairs refuse before expiry, then the live
owner still admits a child. Independent final correctness/adversarial review
CLEAN, architecture No. Test-only; ordinary Bybit remains unchanged and its
original gateway regression is still intentionally RED. No publication/PR or
whole-phase completion claim. Return to1741 for actual1763 integration/freshCI.


## Shared response-proof guard before the ordinary Bybit reference

Owner decision (2026-10-22): generic HTTP408/5xx uncertainty is engine policy,
not a native venue code table. Extend this local follow-on before ordinary Bybit
adoption; migrate Binance too, removing its local generic status predicate.
This is the first slice of error classification, not the full classification
fan-out. No change to draft #1741's source/scope; no publication before it lands.
Local integration78fa7b5558310efff4b1544acaf2bcbf16f8421c preserves all5305
entries from core57582 and the local replacement work, including the README
rows proved by reversing each exact insertion independently.

Verified seam: engine/http_lifecycle.rs currently completes bounded managed
mutation custody, then calls Venue::on_response with mutable Cx/Store. A shared
predicate called inside that callback would be too late. Place the decision
before that callback and before account-auth observation. Apply it to both
complete-response and streamed custody, and to retained late responses; ordinary
managed reads and unmanaged work remain explicitly separate. Define unproven as
HTTP408 or status>=500, exactly preserving both existing predicates, including
600 and65535. Collector.start currently accepts anyu16; this slice does not
claim general HTTP validity or proof for lower invalid statuses. Defer that
policy explicitly; do not broaden this slice to429 or native codes.

Proposed authority split, to audit before implementing:

- One engine response-proof type/decision owns whether native mutation decoding
  receives normal Cx authority. Unproven replies never enter on_response or a
  Store-capable reconciliation callback. Transport loss/malformed collection
  already enter shared Failed evidence; preserve those rails.
- A read-only native evidence hook receives &self, pending correlation and the
  bounded response, plus an engine-owned bounded evidence writer. It may retain
  exact validated native IDs and native diagnostics, never assign caller legs,
  update Store/ownership/auth, return a definitive Rejected, or grant replay.
  The hook defaults to no evidence; oversize/over-cap IDs seal conservative
  Unknown without truncating IDs. It must not allocate an unbounded Vec first.
- Keep native planning cleanup distinct from decoding: a notification without
  Cx/Store/response may discard the affected command's remaining native plan.
  The engine records unresolved sent evidence, revokes further command work and
  projects the shared journal, retaining already-known IDs/facts from siblings.
  Audit this termination contract against Binance amendment and ordinary Bybit
  mode/fallback plans before freezing the hook: no leaked jobs, unresolved
  owner retention, fabricated successful siblings or duplicate terminal events.
  If per-leg continuation is needed, extend shared lifecycle authority instead
  of handing the unproven codec a mutable Store or adding a venue finalizer.
- Late unproven responses remain evidence-only even when their freshness fence
  is current. They never acquire patch authority or reopen a sealed command.
  Bound custody and all raw deadline/generation/attempt checks stay unchanged.
  Financial-token late patching remains its separately planned follow-on.

Acceptance starts with a hostile engine fixture whose normal response callback
would mutate Store: both408 and500/503 must bypass it for direct and split
responses. Verify native-looking success, already-gone/refused and malformed
bodies, accepted earlier sibling plus unproven later sibling, callback order,
wrong/stale generation, deadline boundary, late ACK, exact evidence IDs and
bounded overflow. Journal result must be Unknown, no Store/ownership patch,
no native auth success, no effects-free retry, and original raw deadline and
release count unchanged. Positive200 accepted/refused/known-partial controls
must retain their original assertions.

Then run the same boundary through real Binance and ordinary Bybit native
ACK paths (including Bybit benign cancellation and10002/10004 retry cases).
Remove duplicated408/5xx predicates only after shared-path regressions are
assertion-RED on the original core and GREEN on the guard. Preserve legacy
Hyro semantics explicitly; no all-family closure. Run focused engine/Binance/
Bybit tests, strict Clippy/fmt, ratchet/bans and independent correctness plus
adversarial review at each checkpoint. Architecture gate: No; one engine
proof/authority branch, with native evidence codecs in two different families.


Response-proof plan audit refinements (before production source):

- Pin the command-wide abort: first record the current RELEASED attempt Failed,
  then centrally seal once using all prior journal facts/exact IDs and every
  sibling's release history. Retire outstanding sibling route/preparation/
  provider/transport work through existing cancellation and late custody.
  Never manufacture a successful or definitively rejected sibling fact. A
  later repeated guard cannot emit another terminal result. Cover accepted and
  KnownPartial siblings, plus unresolved released siblings in both callback
  orders; overflow still yields Unknown and cannot drop earlier native facts.
- The discard-only native notification is bound to the exact owner generation,
  never a numeric command alone. It removes only native Job/ResolveMode/delay/
  fallback planning, not account/order ownership reducers. Preserve actual
  dynamic amendment and mode/fallback behavior on proven responses. Test that
  stale cleanup cannot remove a newly reused command's plan. Existing timer
  events may remain physically scheduled but must be inert after that exact
  plan is removed; no arbitrary venue-supplied timer cancellation list.
- Active evidence decoding borrows Pending; late evidence has only bounded
  ReconciliationData, never retained Pending/Rest/LocalWrite/recipes. A writer
  consumes the existing command-wide ID/text quotas row by row (no unbounded
  temporary list or reset per response). A sealed result cannot be amended:
  this slice makes NO late-ID retention/publication promise. Late unproven
  custody may retire without native decoding; never silently lose IDs collected
  before sealing or reopen a result just to attach later evidence.
- Guard only after issuer/generation/attempt/deadline admission, before BOTH
  auth observation and Store-capable native callbacks. For late responses it
  runs before any Store-capable reconciliation callback. Keep status counting,
  begin/finish event scope and deferred order-lock drain exactly once; neither
  stale late cleanup nor native decoding grants completion/send authority.
- Engine controls include408,500,503,600,65535, wrong/stale/reused owners,
  queued and released sibling cancellation/late ACK, repeated guard, exact known
  partial IDs, order locks and deferred failure. Real family controls include
  Binance dynamic amendments plus ordinary Bybit mode/fallback/supersession
  plans. Existing positive200 assertions stay unchanged. Lower invalid-status
  policy is explicitly outside this first classification slice.

Independent correctness/adversarial plan review CLEAN after these refinements;
architecture No. The implementation must establish the owner-bound native plan
cleanup shape before source adoption; no mutable-Cx compatibility escape.
