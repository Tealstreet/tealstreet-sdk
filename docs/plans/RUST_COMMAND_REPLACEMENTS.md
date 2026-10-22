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
