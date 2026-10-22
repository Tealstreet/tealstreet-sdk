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
