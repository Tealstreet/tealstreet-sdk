# Owned history completion and pagination

Owner: codex-cxxyqr. Planned from landing `556906286997` (2026-10-22).
Reference merged in #1710 at `1910baa65a3`; fan-out remains active.

## Next local phase: owned range paging / Binance reference

Fresh `feat/rust-owned-history-range` starts from actual scheduler landing
`faf799268857` while #1720 owns the source merge window. Commit this plan first,
then locally integrate the reviewed frozen #1719 foundation `35ee0ea84af` to
exercise its contextual finalization. The reviewed local dependency was subsequently merged as actual #1719
`fa89bc0b59c` and real-integrated before publication; this owner API is now landed.
Do not publish/merge this phase before that actual integration;
real-integrate actual #1720/#1719 and other shared-engine dependencies before
final proof. No dispatch/request/signing/host, clock, lib.rs, row/account reducer
or financial helper edits. Engine source stays in history.rs (and paging.rs only
if the shared range primitive demonstrably needs an additive API).

Add engine `RangeHistory<T>` around `HistoryRows` and `RangeWalk`. It owns active
Fetch bounds, pending typed rows with matching PageRow facts, fresh-index
selection and terminal/lifecycle state. Do not let the adapter call RangeWalk.page
or mutate dedupe state before the whole response is valid. Pair row/fact staging
atomically, validate any selection invariant, then retain only selected accepted
rows. Asking for another step with an unadmitted Fetch, or completing success
before Finished, poisons strict completion even if rows were accepted. Shared
contextual finalization receives accepted rows only. Preserve a known omission
inside the active chunk too: RangeWalk only latches truncated/exhausted counts
at rollover, so add a small read-only coverage snapshot if necessary. Test a
full duplicate/no-progress page followed by failure before ChunkDone; count
selected accepted rows, never excluded/staged rows, for recovery eligibility. Reuse existing first/later
failure and strict-rejection policy; do not copy it into this adapter. Range
budget/stall coverage is recorded by the owner, preserving prior range omissions
when a subsequent actual-page failure ends the query.

Binance general histories are the reference: ranged income uses this owner and
its existing successful RangeWalk span splitting, fresh-index order, compound
identity and 2-second chunk cadence. Keep request limit distinct from the range
pager's native page size; income has no new global output cap. Fills retain
oldest-first seven-day requests/per-request limits and existing output order,
without new sort/dedupe/global truncation. Orders remain one request plus final
descending sort; public trades stay unsigned and preserve response order. Use
HistoryRows/Completion for the non-range cases too, check every whole-array parse
result, stage failed prefixes without admitting them, and remove unwrap-or-empty
serialization. Preserve missing-symbol/keyless no-I/O and unsupported profiles.

Before accepted rows, actual page failure rejects typed, never empty complete;
after accepted rows, it finishes those rows plus one command-owned incomplete
marker. The old ranged-income walk.failed continuation is superseded by that
owner policy. Signing refusal/timeout remains STRICT with its supplied typed
VenueError and timer cleanup; do not stringify it into recoverable HTTP-page
failure. Keep client-order lookup separate: primary credential routing, cache
scope, identity checks and definitive -2013/HTTP conditions stay unchanged;
partial histories must never manufacture a definitive lookup miss. The
existing Binance best-effort-empty test encodes the superseded contract: replace
its empty-success/CommandError expectations with typed failure and exactly-once
command delivery, while retaining its background-log isolation assertions. The
account test for later fills-window loss is superseded too: keep prior rows with
owned incompleteness instead of CommandError plus empty success.

Engine tests pin fresh-index selection, duplicate/out-of-range rows, successful
range order, pending/terminal lifecycle failures, first/later page failures,
failed-prefix exclusion, known range omissions and encoder refusal. Binance
regressions pin successful windows/pacing/output and first/later native/HTTP/
transport/malformed failures, accepted-empty behavior, strict typed signing
failure/timer cleanup, unsigned trades and unchanged lookup proof. Prove changed
behavior original RED/core GREEN. Update caller/AUDITS/QUALITY scope without
claiming other histories, background notices, remote signing or B11 closed.

Independent written-plan, correctness and adversarial reviews must answer the
architecture gate: shared engine range admission/completion plus a reference
adapter, not a second venue marker helper. Focused engine/Binance and shared-owner
reference crates, strict Clippy/fmt/bans/links locally; full family/replay/WASM CI,
then a separate fresh guard, separate exact merge and actual tree/parent proof.

Local range-core checkpoint: shared RangeHistory now owns bounds, staged facts,
selection and lifecycle; no Binance adapter code is changed yet. Six new core
tests cover 18 scenario paths including actual 50-page budget exhaustion, active
chunk gaps before rollover, no double count after finish, empty/filtered recovery
eligibility and strict signer/encoder/lifecycle failures. These and the nine
existing history tests pass; strict engine all-target Clippy passes. Independent
correctness/adversarial reviews are clean; the additional budget control follows
the adversarial suggestion. This checkpoint is local only with unmerged #1719
as a reviewed dependency; actual dependency integration and reference regressions
remain required before publication.

## Next cursor fan-out: BloFin general histories

Fresh `feat/rust-owned-history-blofin` starts from actual signer prerequisites
`802e9c8c6e9`. It uses the already-landed `CursorHistory` API from #1719,
not an unmerged range-core API. Develop locally while range reference #1724
runs CI; integrate its actual shared-engine merge before final proof if it
lands first. No engine source/export, dispatch, signing, clock, row/account
reducer, host, fixture, comparator or budget edits in this fan-out.

Replace BloFin general-history accepted vectors, cursor owner and seen-ID set
with one typed `CursorHistory` bound to the real command. The native adapter
keeps wire parsing, filtered native time/cursor selection and mapping; its
contextual finalizer sorts/truncates accepted rows exactly as before. Keep the
existing request-sequence pins: fills/native per-read cap and filter semantics,
income cursor selection as smallest unbounded numeric native ID, raw received
counts driving pagination, one-request orders, unsigned public trades, and all
current symbol/profile/keyless admission. Income has no new early global limit.
Derived PnL and fee rows retain compound identity and final limits.

Streamed row prefixes remain staged until the whole HTTP/native/envelope verdict
is valid. First failure/no accepted mapped rows rejects typed; actual later-page
failure finishes prior accepted rows with one command-owned incomplete marker,
including Error-only logging. Failed-prefix rows cannot enter recovery. Native
paging refusal remains STRICT as before: empty cursor on a full continuing page,
repeated cursor or budget exhaustion rejects through the owner, never partial
success. Header/signing preparation failure is also strict even after accepted
rows; do not reinterpret it as a failed HTTP page or add a resend. Existing
closed-position owner behavior remains untouched.

Before editing source, pin current successful request sequences and review this
plan. Add original-RED/core-GREEN first/later native/HTTP/transport/malformed-page
regressions, accepted-empty/filtered recovery controls, failed-prefix exclusion,
marker ownership/order at Error-only level, unchanged strict stalls and PnL/fee
identity/sort/limits. Explicitly supersede the old later-income-rejection test
only for actual page failure; keep strict pagination-refusal assertions. Preserve
all unrelated financial-reducer tests and account state. Update caller/audit/
quality scope without declaring universal fan-out or B11 closed.

Independent correctness/adversarial reviews must answer the architecture gate:
this adapts a second general-history family to the existing shared completion/
cursor owner, not a venue-local copy. Focused engine/BloFin/Binance and reference
Bybit tests, strict Clippy/fmt/bans/links; final shared dependencies integrated,
full exact-head CI, then separate revert guard, separate merge and actual
parents/tree proof. Coordinate native-history overlap with signer/clock/rows
lanes before source changes. No NX edits or remote-signing closure.

## Contract and sequence

1. Reference PR: engine-owned history completion and cursor paging, exercised by
   BloFin closed positions. Bind a non-cloneable completion context to the real
   command at construction. Shared paging owns cursor/page counts, deduplication,
   budget and stall decisions. Finishing consumes the context and sends any
   command-owned `history.incomplete` before successful retained rows, including
   at Error-only logging. Failure before accepted rows rejects; a later failed
   page returns only previously accepted rows with owned incompleteness. Failed
   response bodies never become accepted rows or complete empty success. Keep
   existing BloFin URLs, ordering, raw rows, short/duplicate-page
   completion and page-cap message. New policy changes need explicit regressions.
2. Fan-out on fresh branches: use the same completion path for remaining marker
   families (Binance, Bitunix, Bybit, Hyperliquid, Kraken, Lighter, OKX, Paradex,
   TxFlow, WEEX) and other paged history consumers. Extend the shared core for
   timestamp, range, offset, multi-stage and composite walks; do not create a
   second venue implementation. Preserve each endpoint's limits, ordering,
   continuation evidence and bounded/unbounded contract, with the shared failure
   policy: first-page/no-accepted-row failure rejects; later failure retains only
   accepted rows with a command-owned incomplete marker.
   Query-associated markers require command ownership; background diagnostics
   remain distinct. Add a CI prohibition on venue-owned completeness emission
   after conversion, and retain B11's downstream NX ingestion/coverage work.
3. Small follow-on after query fan-out: route the six background account-coverage
   diagnostics (Bitunix/Lighter/TxFlow) through one engine-owned account/background
   coverage notice, retaining the code and explicit scope without a command ID.
   These are not query outcomes and must not use fabricated request attribution.
4. Separate follow-up PR after history/background fan-out: capability/feature
   conformance in CI for combined and standalone family builds. Every advertised command must
   survive actual command decoding and reach a supported handler under that
   family's Cargo features. Prove the MEXC standalone order-book missing-feature
   regression red/green; cover aliases and intentional unsupported profiles.
   A grep of manifests or combined-only feature unification is insufficient.

## Reference implementation and verification

- Add `engine/src/history.rs`, export it from `engine/src/lib.rs`, and extend
  shared `paging.rs` only where the reference actually needs reusable policy.
  Completion carries the command, first incompleteness reason/message and
  consumes itself through success/failure. `HistoryRows<T>` stages streamed rows
  separately from accepted pages and owns encoding/recovery; no free-form command
  field at marker sites. A shared cursor wrapper records stalls and produces completion states.
- Convert `venues/blofin/src/closed.rs`; remove its raw marker, cursor/page state
  and dedupe ownership in favor of the shared pager. Keep schema-specific ID
  comparison and URL construction local.
- Tests: engine concurrent command attribution; complete vs incomplete vs failed
  completion; markers before results at Error-only level; repeated cursors,
  budget boundaries, short/duplicate pages and read failure. Pin BloFin request
  sequences before migration. No accidental account/store mutation.
- Focused gates: engine + BloFin + Binance tests, touched-crate fmt and strict
  Clippy, bans. `CARGO_BUILD_JOBS=4`; no local release WASM or replay sweeps.
  Full CI supplies family builds, replay and timing gates. Independent correctness
  and adversarial reviews must explicitly answer the architecture-gate question.
- Update caller/audit/quality docs only for the implemented slice, preserving
  historical evidence and unconverted exceptions. No NX changes.

## Shared-file ownership and landing

Signer lane (codex-uufub4) owns engine dispatch/action/signing sections and host
adapters. History completion/paging is disjoint, and final reference integration
stays outside signer dispatch files. The overseer explicitly permits this disjoint
reference PR to merge on green before signer core; signer integrates it later. Clock lane
(codex-5c57jl) owns time/provider/calibration. Row lane (codex-mxb4mq) currently
owns coverage.rs and Bybit/Bitunix account.rs; later snapshot freshness is its
scope. Coordinate before touching those files or selecting those consumers.

Each PR uses a fresh landing branch, plan-first commit, independent review,
green CI and a separate exact-path revert guard before a separate merge call.
Caller docs stay current against actual merge pins. No source merge during an
exclusive window without overseer release. Signing remains the signing lane's
scope; no request retry/outcome rewrite is mixed into history work.

## Plan audit (before implementation)

Independent read-only audit: reference design is cross-cutting core work, not a
per-venue fix. Preserve duplicate/short/missing/unchanged-cursor termination,
untrimmed dedupe versus trimmed numeric continuation, and caller-limit versus
safety-budget completion. Shared cursor policy will explicitly represent legacy
end-on-duplicate/unchanged behavior. The original plan's strict later-page rejection was superseded by the owner:
first failure before accepted rows rejects; later failure retains only accepted
rows with owned incompleteness. Failed serialization and lifecycle invariant
failures still reject without a marker or successful result. A consuming token prevents its
own reuse, not arbitrary reconstruction or bypass through legacy Cx APIs; final
fan-out must include the venue-emission prohibition. Multi-stage query owners must
survive pager restarts without clearing incompleteness.

## Reference implementation checkpoint

The shared `history::Completion` retains the owner and first missing-coverage
cause until final success/failure. `CursorHistory` owns the cursor walk, dedupe,
stall recording and terminal-state check; construction consumes the same owner
used for no-I/O early returns. BloFin supplies numeric cursor selection and page
facts. All its completion/error paths use that owner, with no raw semantic marker.
Short pages, missing IDs, repeated identities and immediate unchanged cursors
retain their existing endpoint termination. Non-adjacent cursor cycles now stop
early with retained rows and owned incompleteness instead of spending the remaining
budget on a cycle; a dedicated regression pins this intentional improvement.

Independent correctness and adversarial reviews found no blocking issues and
answered the architecture gate: shared engine choke point with BloFin as reference,
not a separate venue implementation. Both asked for the non-adjacent-cycle
regression; it is now included. Remaining emitters are not claimed converted.

Verification so far: the BloFin request-sequence/termination pins passed against
pre-migration source, the new cursor-cycle regression failed at its intended
assertion against original source and passed after restoration, and three core
integration tests plus six closed-position tests pass. Touched engine/BloFin
strict all-target Clippy, fmt, bans and touched relative doc links pass. Full
engine/BloFin/Binance tests and CI remain final merge gates; no local WASM build
or full replay/performance sweep is being run on the shared machine.

## Owner failure-policy revision (before final reference CI)

Overseer direction explicitly supersedes the original later-page rejection:
first failure with no accepted rows is a typed rejection, never empty success;
a later failed page returns prior accepted rows with one owned incomplete marker.
The engine now owns staged/accepted row buffers and encoding, so a failed envelope
cannot leak its streamed rows. An accepted empty page is insufficient for row
recovery. Serialization failure and premature success are strict errors, not
recoverable page failures. The independent implementation reviews caught the
premature-success-after-accepted-page edge; it now has a strict path and regression.

Actual row core `b2f074bf2637` is integrated by porcelain merge `06b9f5cff29`;
its README/quality/source additions are retained. The source overlap remains
outside signer dispatch and clock/row implementation paths.

Revised-policy verification: original BloFin source fails both first-page typed
rejection and later-page accepted-row recovery regressions at their intended
assertions; the corrected seven-test closed-position suite passes. After actual
row-core integration, 695 engine/BloFin/Binance tests pass, as do touched-crate
strict all-target Clippy, fmt, bans and touched doc links. Independent correctness
and adversarial delta reviews are clean after the strict invariant fix. Full CI
must run on the revised final head before merge.

## Fan-out phase 1: windowed / seeded closed-position reads

Fresh `feat/rust-owned-history-windows` from actual `1910baa65a3`.
Plan before source changes; source merges held for row phase 2 #1711 until release.

- Engine `history.rs`: restart the cursor stage without replacing the completion
  owner, accepted rows or first missing-coverage cause. Restart resets per-stage
  cursor/budget history; premature restart is a strict lifecycle error. Preserve
  cross-stage dedupe state (consumers which retain duplicate rows need not opt in).
- Engine `paging.rs`: seed an initial request cursor, needed by OKX's explicit
  upper close-time bound. Preserve decimal strings; do not coerce IDs through f64.
- Bybit `closed.rs`: replace local page/cursor counters and raw rows with the
  shared owner/pager and staged response rows. Keep seven-day windows, category
  selection, URL encoding, per-window budget and raw duplicate rows. Require the
  success response's result/list before page admission. Native response codecs
  remain venue facts, not a separate completion or paging algorithm.
- OKX `closed.rs`: preserve initial after=to, before=from, oldest uTime-1,
  100 ms page delays, current missing/unchanged-cursor exhaustion and raw rows.
  Move row acceptance, counters, completion and failure recovery into the core.
- Both consumers: first-page/no-accepted-row errors reject; later failed responses
  discard staged rows and return accepted rows with owned incompleteness. Non-
  adjacent cursor cycles stop earlier with incomplete coverage. Serialization and
  lifecycle errors stay strict. Do not modify account/row/store/fence/signing code.
- Tests before migration pin existing request sequences; add original-red/core-
  green first/later failure and malformed-success tests. Cross-window tests must
  prove independent budgets/cursor reset, preserved early incomplete cause,
  marker ordering at Error-only level, raw duplicate preservation, and later
  failure recovery. Guard premature reset even after accepted rows.
- Verify engine/Bybit/OKX/BloFin and Binance native tests; strict touched Clippy,
  fmt, bans, docs links. Independent plan/correctness/adversarial review and full
  CI before separate guard/merge. No local WASM release or replay sweeps.
- Caller doc: pin #1710 and document this slice's later-failure recovery, required
  incompleteness handling and unchanged method args. Do not close B11 or claim
  other histories migrated. Follow subsequent fan-out with account/background
  notice core, then standalone/combined capability conformance.

Phase-1 implementation audit: independent correctness and adversarial reviews
are clean. Stage reset poisons a pending/dirty reset before any recoverable page
failure can hide it; accepted rows, identities and first incomplete cause survive
valid resets while cursor/budget history resets. Bybit keeps raw duplicates and
requires result/list before staging. OKX keeps its seeded bound and delay, and
outer HTTP/envelope validation precedes page admission. Bybit repeated adjacent
cursors now stop early too; OKX retains its immediate unchanged-cursor exhaustion.
The pre-migration request suites passed (six Bybit, five OKX tests); new first/later
failure and missing-list regressions are checked against original source before
final focused gates and full CI. Architecture gate: one shared engine extension
with two protocol adapters, not separate per-venue completion implementations.

Phase-1 focused verification: 1,225 engine/Bybit/OKX/BloFin/Binance tests pass;
touched strict all-target Clippy, fmt, bans and doc links pass. Four original-
source regressions fail at their intended assertions (first/malformed + later
failure per Bybit/OKX), then corrected closed-position suites pass. Source merge
still waits for #1711 priority release, full final-head CI and a separate guard.

Final windowed-slice integration includes actual row phase 2 #1711
`bd1a69072c7`, CI bootstrap #1713 `04bec2ac52f` and clock #1709
`6f815755c37`. All seven reviewed history source/test files remain byte-identical;
incoming source and plan changes are retained. The caller refresh also records
#1709's retired hostClock/bit14 and shared clock ingress, superseding historical
venue opt-ins. Both integration reviews are clean; all 1,225 focused native tests,
strict touched-crate Clippy, formatting, bans and touched links pass again.

## Next fan-out: Bybit general histories

Fresh `feat/rust-owned-history-bybit` starts from actual #1712 `0e15f31a874`.
Keep implementation outside engine dispatch/request/signing, coverage, clock,
account reducers and host adapters. Integrate actual queued dependency changes
before final CI; #1715 and install-retry #1716 still precede later merges.

Extend engine `HistoryRows`/`CursorHistory` with consuming `finish_with`, whose
single-use finalizer receives only accepted `Vec<T>` and can capture native query
sorting/dedupe/limit metadata. Existing `finish` delegates through that same path.
Lifecycle poison and premature completion reject before invoking the finalizer;
failed-page staging is discarded; first/no-accepted-row failures reject typed;
actual later-page failure retains accepted rows plus one command-owned marker.
Encoding failure still emits neither marker nor success. Add a shared consuming
strict-rejection path for pagination-policy refusals, distinct from recoverable
actual page failure. Never pass a policy refusal into the recovery branch.

Migrate only `venues/bybit/src/history.rs` in this slice: one owner from initial
admission/no-I/O paths through category/window restarts and final completion.
Use a typed row enum and engine-owned accepted/staged storage; mismatched row
variants reject, never silently filter away records. Preserve mapped-row counts
before final dedupe, per-category limit accounting, global truncation, page/window
budgets, native arguments and unsigned public-trade behavior. Preserve income's
compound identity including source/hedge legs and its empty-list termination.
Make missing/malformed result/list fail rather than become empty complete output;
return serialization failures instead of `unwrap_or_default()`. Bounded cap and
repeated-cursor policy errors stay strict; unbounded order cap retains rows with
owned incompleteness across subsequent windows. Any intentional early cycle
termination difference needs an explicit caller note and regression.

Engine tests cover context-capturing finalization, staged-row exclusion, strict
policy rejection after accepted rows, encoding errors and poisoned lifecycle not
invoking the finalizer. Bybit account/product/variant regressions cover first and
later native/HTTP/transport/malformed failures, Error-only marker-before-result,
empty accepted pages, window/category recovery, duplicate limit accounting,
hedge-income identity and strict cap/cursor policy. Prove original RED/core GREEN
for changed behaviors; preserve existing assertions. Update AUDITS/QUALITY and
caller scope without claiming other histories, background coverage or B11 closed.

Use independent plan/correctness/adversarial review and state the architecture
answer: shared engine completion/paging extension, not duplicated venue policy.
Focused engine/Bybit plus Binance native tests, strict touched-crate Clippy,
format/bans/links locally; full CI owns family/replay/build proof. Then separate
fresh guard, separate exact-head merge and actual parent/tree verification.
Paradex lookup/fallback coverage is a later slice: partial unrelated rows must
never manufacture a definitive lookup miss.


Phase-3 implementation uses a typed individual-row enum; page-local decoding
requires result/list before staging. The adapter's contextual finalizer rebuilds
the exact result kind, rejects mismatched variants, then sorts/dedupes/truncates
accepted rows. No direct completion or history marker emitter remains in this
adapter. Private `Listed.ret_code` is renamed `_ret_code` with identical explicit
serde naming/parser because its last read-and-discard site migrated; account
execution deserialization is unchanged. Transport errors are typed Network;
unclassified native/HTTP/parse failures use the core Internal default and keep
their existing message, without inventing native codes from HTTP fallbacks.

Independent correctness/adversarial reviews are clean and both answer the
architecture gate: shared engine extension plus protocol adapter, not duplicated
venue completion policy. Existing successful request sequences/variants pass.
The new matrix covers first/later native, HTTP, transport and malformed-page
failures, accepted-empty/filtered pages, strict caps/cycles, duplicate limit
accounting, recovered hedge income, cross-window cause retention and category
recovery with sorted global limits. Original adapter source fails five new
account regressions at their intended assertions (typed first failure, retained
later failure, hedge recovery, early cycle refusal and cross-window recovery);
corrected code passes all nine new account/product tests. Final focused gates,
actual signer dependency integration, final independent review and CI remain.


Actual signer #1714 `ad0e6fa1e42` is real-integrated at `dae6e76b666` with no
conflicts. All six history source/test changes and four own docs are preserved;
incoming signer construction/dispatch, tests and plans remain unchanged. This
phase's caller catch-up also pins that actual signer API and the separately
required NX trusted bootstrap migration; it claims neither remote signing nor
NX runtime adoption. Final native checks and integration reviews run from this
shared-engine dependency before publishing the PR.


Final integration audit is clean: all 83 incoming signer paths are byte-identical
to the actual merge, and all six reviewed history source/test files were preserved
through the porcelain merge. Strict touched-crate Clippy, fmt, banned-pattern
checks and 89 relative doc links pass. Two additional original-source controls
prove missing required list data otherwise becomes empty success and a later
category failure otherwise loses prior rows; both are corrected. Together with
the five initial failing regressions this gives seven original-RED/core-GREEN
controls; duplicate-count and empty-page checks preserve existing policy. Root
typecheck/lint pass; root tests and final-head CI complete before merging.


A final narrow contract review caught introduced metadata conflation before
merge: Bybit's existing venue_error helper returns either native code or HTTP
status, so blindly labeling that number venueCode would fabricate native codes
for malformed HTTP-200 and body-less 503 responses. Removed the new metadata
assignment instead of adding a venue-local classifier; the first-page matrix
now asserts no fabricated venueCode. Typed Internal/Network behavior and the
existing message remain. Fresh focused checks and final-head CI are required.


Scheduler #1717 actual `faf799268857` landed before the history guard and is now
real-integrated without conflicts. All five incoming paths and six reviewed
history source/test files are preserved byte-for-byte; no scheduler/history
source interaction is introduced by this merge. Fresh focused checks and CI run
from this shared-engine dependency; earlier `e68e7c0d4b5` CI is not merge proof.


Actual cumulative-fill #1720 `9fc2a7c174ad` is real-integrated at `ff7a9a9c031`;
merge is conflict-free and the effective diff is still the ten owned paths.
Incoming engine/execution/num and Bybit/Bitget account implementations are kept;
history source/tests are unchanged. Caller catch-up records the five demonstrated
reference financial repairs without claiming other cumulative consumers migrated.
Latest overseer order is #1722 clock calibration, then #1719, then #1721 signer
prerequisites. Batch the imminent actual clock dependency before final history
CI; no merge against the prior scheduler-only head despite its green checks.


### Range reference implementation checkpoint

The Binance adapter now uses typed HistoryRows for fills/orders/public trades
and single income, and RangeHistory for ranged income. IncomeHistory is only
transparent native-shape delegation; there is no venue completeness emitter,
range page-admission call, independent accepted vector or walk.failed recovery.
All streamed root-array results are checked before admission. The private
Request.window field is removed because the engine range owner holds the active
bounds. Client-order lookup is unchanged; typed signer failures strictly reject
through each owner and retire pending income pacing.

Four original adapter controls fail at their intended assertions: first-page
failure, later fill-window loss, later ranged-income failure and accepted-empty/
excluded-row recovery eligibility. The corrected adapter passes those and
successful native ordering/per-window limits, income compound identity/chunk
pacing, keyless/public-only behavior and strict signing refusal. Actual dependency
integration, final independent reviews, focused gates and CI still precede
publication/merge; no NX or remote-signing closure is claimed.


Final local reference gates: 1,308 engine/Binance/Bybit/BloFin/OKX native
tests, strict all-target engine/Binance Clippy, fmt, bans and 93 relative links
pass. Correctness and adversarial reviews are clean, explicitly confirming
shared core ownership and unchanged successful lookup/request contracts. The
range owner is boxed in the adapter enum to avoid inflating every single-income
read. The legacy account test for discarded earlier fill windows now asserts
retained rows and marker ordering; no comparator/fixture/budget is weakened.
Both Refused and TimedOut supplied signing errors preserve strict typed notSent
rejection after accepted rows. Actual #1719 is still required before publication.


Actual #1719 `fa89bc0b59ccb36bc36d87428950fddc67217e51` is now
real-integrated after its exact frozen ed757 head passed full CI and a separate
guard/merge. This dependency merge is content-neutral: the reviewed history
foundation and all incoming source/tests remain intact. Actual financial #1720
and protocol-clock #1722 are already ancestors. Signer prerequisites #1721 have
next merge priority; this range reference can run provisional CI, but it must
real-integrate that actual shared-engine merge before final proof/guard. No
unmerged signer API or caller migration is claimed in these docs.


Signer prerequisites #1721 actually merged as
`802e9c8c6e947aa36864aabb7254aa6e50c92b4e` and are real-integrated without
conflicts before final range-reference proof. All 26 incoming paths are retained;
the eight owned core/adapter source/test files remain identical to reviewed
506471f0645. The caller doc pins actual structured provider results/trusted setup
and leaves NX bootstrap and remote signing open. A nearby historical pagination
summary now explicitly supersedes its old Binance error-plus-empty-success
contract rather than continuing to recommend it. Fresh focused native/strict
checks, integration reviews and exact-head CI run from this dependency before
the separate final guard/merge.

### BloFin general-history local checkpoint

The existing cursor owner now holds all general query completions, staged and
accepted typed rows, native-ID dedupe, cursor/page state and contextual final
shaping. Raw received counts/native cursor selection still precede filters;
fills count accepted plus staged executions, while income expands a deduped
native trade into distinct PnL/fee rows and retains its final-only output limit.
Orders and public trades close their one-page cursor stage. No engine source,
lib, header/sign implementation or financial reducer is edited. Header-preparation
failure, missing/repeated cursors and page budget remain strict; actual later
page failure returns only prior accepted rows with owned incompleteness.

Two extra successful request/limit pins were committed and passed before the
adapter edit (12 total existing/pinned tests). New controls cover nine failure
forms across all four queries, staged prefix and native-error-after-prefix,
filtered-only recovery eligibility, PnL/fee identity, concurrent command owners,
strict header entropy refusal and the actual 100-page budget. The original
adapter fails the intended typed-first/retained-later/derived-income controls;
its bytes are restored after the negative run. Corrected 20 history controls
and 768 engine/BloFin/Binance tests pass, with strict BloFin all-target Clippy,
fmt and bans. Independent correctness/adversarial source reviews are clean and
answer the architecture gate: reuse the shared engine owner, no per-venue copy.
The old later-income-rejection assertion is superseded only for actual page
failure; strict pagination refusal remains tested.

This remains a local unpublished checkpoint on actual #1721. Integrate actual
range reference #1724 before final proof, then add the caller/audit/quality catch-up
on that base to preserve incoming doc sections. No cutover/NX/remote closure.


Actual range reference #1724 `22550c4050c33a2f0c0b5bd84b26c027883650e0`
is real-integrated into this BloFin slice. Its nine incoming paths outside the
four owned shared docs, and all four reviewed BloFin source/test blobs, remain
exact; incoming lines in those shared docs are preserved. The only
conflict was independent added sections in this shared plan; the overseer
authorized retaining incoming Range blocks then BloFin blocks without rewriting
or deleting lines, and both original complete documents remain ordered
subsequences of the resolution. No source/test/contract conflict occurred.
Caller/audit/quality catch-up is added after that integration, preserving the
incoming Binance and actual #1721 provider sections. Fresh focused gates and
independent integration/caller reviews precede exact-head CI and separate guard.
The financial #1725 hold is respected: this fan-out touches no Binance, engine
execution/num, WOO or Bitrue source; integrate its actual shared-engine dependency
if it lands before the final proof.


Actual scheduler-policy #1723 `08b9c12f4861237f474578928e8ab606da1e1236`
landed before publication and is real-integrated conflict-free. Its two incoming
paths are exact; the reviewed BloFin implementation is unchanged. This is policy
foundation without runtime adoption, so it adds no caller behavior row. Fresh
focused/strict checks and CI use this shared-engine dependency. The #1725
financial window remains independent and is observed before any final guard.


A bounded caller review caught admission metadata regression before merge:
passing unsupported lookup/invalid input through Completion's unclassified
Internal fallback would mislabel known user errors. Those native admission
facts now set the existing common VenueError kind/outcome fields explicitly:
Unsupported/notSent for lookup, InvalidInput/notSent for range/market. No new
classifier/helper/engine source is added. A four-case admission regression fails
against frozen 1f3 (Internal instead of Unsupported) and passes corrected code.
The generic history_failed callback intentionally retains the common Internal
default because both transport and stream malformed/missing-list callbacks use
it; a blanket Network label would misclassify decode failures. Caller wording
records these metadata changes and includes later filtered-fill recovery.

## Background account coverage notice follow-on

Plan from landing `361a4ccafad` after the BloFin general-history merge. The six
remaining account/background `history.incomplete` diagnostics are not query
results: Bitunix has one unresolved execution-metadata notice; Lighter has
three outage-backfill notices; TxFlow has two reconciliation exhaustion notices.
Never invent a command ID to make these look query-owned.

Add one engine `history::BackgroundCoverage` reporter with native facts
`Incomplete` and `Exhausted`, mapping centrally to the existing Warn and Error
levels. It emits the existing `history.incomplete` code with
`scope: "account/background"` and no `command` field. Replace exactly those six
venue emitters; preserve messages, trigger conditions, retries, account/fill
state, caps, and query completion. No dispatch, signing, clock, host or wire
change is intended. Coordinate account-file overlap with the financial lane.

Verify original-red/corrected-green native cases for each venue and both
exhaustion paths, plus engine scope/severity/Error-only visibility and an
unrelated pending query which must not become incomplete or completed. Run
focused engine/Bitunix/Lighter/TxFlow plus Binance tests, strict touched-crate
Clippy/fmt and full CI. Independent plan/correctness/adversarial reviews must
answer the architecture gate: this is one engine choke point, not three local
policies. Record the caller boundary without claiming full history fan-out or
NX/runtime closure. After this phase, the owner ordered capability/feature
conformance CI next; other unconverted paged histories remain separate work.


The actual stamp #1726 `e09e3448625` and financial #1729 `6e10b0e8a23`
are real-integrated before the Bitunix edit. Their source changes are retained;
this follow-on changes no financial arithmetic, dispatch or request builders.
Original-red controls fail on the missing scope field for Bitunix metadata
repair, Lighter's exhausted reconnect retry and all three TxFlow limit branches
(offset, accumulated rows, deduplication). The shared reporter preserves four
Warn/two Error sites and emits only the scope field. Its real-engine control
keeps an unrelated command pending across repeated notices, then completes that
command without an incomplete marker. Existing health counters are unchanged;
there is no history.incomplete health counter to add or increment.

Focused engine/Bitunix/Lighter/TxFlow/Binance gates pass 1,133 native tests;
strict all-target Clippy and formatting pass. Adversarial source review is clean:
the six replacements are emitter-only, and the existing Cx::notice path already
delegates to Cx::event. The only event contract change is the explicit scope.


## Next fan-out: Bitunix fills, orders and income

Plan from actual background merge `b1dfdb43291d5a6de7dd8808b486adddda4cb118`.
Bybit and Kraken Spot are excluded because their lanes are active. Bitunix's
three history queries still keep local row vectors, a dedupe set, page counters
and direct history.incomplete emission. A later failed page currently rejects
its earlier accepted rows; first-page failures reject without a typed kind.

Use the existing shared `CursorHistory` for all three query kinds, with one
owner and flat serialized native result variants. Stage mapped rows until the
HTTP/envelope/stream verdict succeeds. Native facts select the time cursor for
orders and offset token for income; the engine owns dedupe/progression, the
50-order/100-income page budgets, missing-coverage notice and final completion.
Fills stay single-page. Native invalid/nonadvancing order-time bounds are
projected to a repeated opaque cursor so the existing core reports the same
stall; an unbounded initial token never enters the wire request. Do not add or
extend a pager/completion API or edit shared engine dispatch, clock or signing.

Preserve native request ordering, inclusive boundaries, raw-ID progress before
mapper filters, skip increments, response aliases, income PnL/fee derivation,
units, sorting/truncation, keyless/unknown-fill-market no-I/O, and unsupported
public-trade/client-ID lookup admission. The common first-page failure path
becomes typed; later HTTP/native/stream failure resolves only accepted pages
plus exactly one command-owned Warn before done. Staged failed-page prefixes
never leak. Signing/credential preparation refusal remains strict even after
accepted rows, via one history-specific rejection arm in fail_private; do not
change credentials, signatures or native account reducers.

Original-RED controls: typed first-page failure, accepted-prefix recovery after
later transport/HTTP/native/malformed/missing-list failure, failed-page prefix
exclusion, a full mapper-filtered page followed by failure, Error-only single
owned marker ordering, concurrent query isolation, and strict signing-entropy
refusal with notSent. Existing cursor stalls/page caps/valid-empty/filter/time/
alias/ledger tests remain semantic-preservation controls; update the old strict
later-failure expectation only where owner policy explicitly supersedes it.

Intended source scope: Bitunix history.rs, the single fail_private history arm
in account.rs, and native history tests. Append caller/audit/quality notes as
appropriate. Coordinate that account arm with the other lanes before editing;
no Bybit/Kraken Spot, source outside this venue, shared core or harness/workflow
changes. Run focused engine/Bitunix/Binance tests, strict Clippy/fmt, independent
plan/correctness/adversarial reviews, full CI and the separate guard/merge in the
owner queue. Architecture gate: reuse one engine history choke point rather
than introducing a venue-specific completeness/paging policy. Integrate actual
shared dependencies before final proof; #1737 conformance stays separate.


Plan review is clean with preservation constraints: seed orders via
CursorWalk::starting_at so invalid first-page bounds repeat a registered token;
do not use end_on_unchanged_cursor. Keep raw added counts separate from staged
mapped rows, no early order-limit completion, and income skip 100..9900. A truly
empty page ends natively, so the zero-accepted-rows/later-failure control uses a
full raw page whose rows are all mapper-filtered but retain native ID/time
progress. Separately prove genuine empty-page termination. RequestContext lane
confirmed no Bitunix edits; the one fail_private arm remains disjoint.


Implementation keeps the CursorHistory owner inline: boxing it shrank History
far below the pre-existing Private continuation and triggered large_enum_variant.
Inlining preserves the existing Pending size balance and avoids a new allocation
or unrelated account-continuation boxing. No lib.rs or reducer changes needed.

Original-RED checkpoint e2aae0aa398 failed all six new failure-contract groups
against the old implementation (15 preservation tests passed). The shared-owner
implementation passes those controls plus four boundary controls: genuine empty
termination, invalid unbounded first cursor, unchanged explicit endTime, and
final-only requested order limit. Signing preparation remains strict, now with
typed notSent metadata instead of an unclassified rejection; callbacks
multiplexing transport and decode retain the shared Internal
fallback rather than inventing a Network classification. The owner is the sole
command identity and emits incompleteness only at successful finalization.

Focused engine/Bitunix/Binance gates pass 848 native tests, including all 25
Bitunix history tests; strict all-target Clippy and formatting pass. Correctness
review confirms unchanged raw progress/filter ordering and seeded request bounds.
Full CI and the current owner merge queue still apply; this is not a landing or
deployed-runtime claim.


## Next fan-out: Paradex cursor histories

Plan from actual `2c5c5f6987f35cafc0e272282e54da37c0a66d4f`. Metadata #1747
and conformance #1737 remain separate drafts; keep #1747's CI watch running.
Bybit and Kraken Spot stay outside this slice. Paradex still owns CursorWalk,
query row vectors and its bounded page-cap marker in history.rs.

Adopt the existing `CursorHistory` for fills (including the native orders-history
fallback), historical orders, income, public trades and client-ID lookup. Use
one typed row enum and one completion owner across endpoint stages; schema
mapping remains native. Stage each page until HTTP/envelope/stream validation
succeeds. Actual failure before accepted mapped rows rejects typed; later actual
page failure returns only accepted earlier rows with one command-owned incomplete
Warn before successful completion, including Error-only logging. No failed prefix
may enter the finalizer. A valid empty or fully filtered page does not itself
qualify for partial recovery. Preserve the shared Internal fallback for callbacks
which multiplex transport and decoding rather than inventing Network evidence.

Keep endpoint selection and query parameters, 5/50/200 page budgets, defaults and
clamps, raw-row/cursor progress, existing native ID dedupe/filter order, final
sort/limits and single-page public trades. Preserve empty-page traversal, fills'
empty-last-page fallback and one-page aggregate fallback. Carry the same owner
through the fallback; no reset of accepted rows, dedupe or missing-coverage state.
If needed, expose only CursorHistory's read-only page count delegated to its
existing walker, to retain the native fallback trigger without a second counter
or pagination policy. No new pager/completion/scheduler algorithm.

Client-ID lookup remains strict on actual failures, cursor cycles and incomplete
proof; partial histories must never become a definitive miss. Preserve exhaust-
open-orders-before-history, early found-order completion, bounded nondefinitive
miss and complete-miss proof. Strict unbounded budget/cursor-policy refusals also
remain strict even after accepted rows; bounded page-cap success retains its
existing marker/message. Auth/preparation admission remains strict: do not
reinterpret refusal as an HTTP-page failure or add auth/retry behavior. Keep raw
public-only/keyless/unknown-symbol no-I/O shapes and existing admission messages.

Before implementation, run preservation request-sequence tests and commit new
original-RED controls for typed first failure and accepted-page recovery after
later HTTP/native/transport/malformed/missing-list failures. Cover failed-prefix
exclusion, zero accepted rows, Error-only marker ownership/order, concurrent
owners, fallback stage ownership and strict lookup/cursor/budget behavior. Existing
successful codec, request pin, ledger and lookup tests remain. Update caller/
audit/quality notes for the changed general-history failure contract only; B11,
all-family enforcement and deployed NX proof remain open.

Coordinate history.rs ownership with signer/clock/row lanes before source edits.
No financial reducer, row admission, signing/dispatch/request, host, clock,
metadata, feature/harness/workflow, comparator or budget changes. Focused engine/
Paradex/Binance plus history reference tests, strict Clippy/fmt and independent
plan/correctness/adversarial review; architecture gate must state this reuses the
shared engine owner rather than adding per-venue policy. Integrate actual shared
dependencies before final CI/proof, then normal queue and separate guard/merge.


Paradex plan-audit refinements: permit only narrow `Pending::History` arms in
lib.rs preparation/JWT-queue failure and signing-failure dispatch to invoke the
strict owner rejection path; actual HTTP/stream failures keep recoverable-page
routing. Never classify these by error-message text. Existing `raw_rows` is a
query-accumulated count, not reset per page: preserve its fills-fallback trigger
(empty raw traversal, not a final empty page after earlier raw rows). Early found
lookup and fallback must admit and terminate the validated stage before owner
finish/restart. Preserve open-lookup stage-budget transition as a nondefinitive
proof boundary, without introducing a new warning or a second paging counter.


The plan review also caught the page-50 cycle edge: suppressing a probe-stage
budget with want_more=false would bypass the existing cycle-before-budget check.
Allow a narrow shared `CursorHistory::advance_probe` operation: run the same
walker/admission/lifecycle path but treat only PageBudget as a probe boundary
without declaring query-row incompleteness. Repeated cursors still stall before
the budget; the native lookup still rejects them. Normal advance is unchanged.
Pin both cases in core/native tests; no native cursor set or replacement pager.
Preparation rejection gains typed Internal/notSent through the owner, retaining
its strict no-rows/no-marker behavior. Original source compiles and fails all
three new typed-first/retained-later/zero-accepted behavioral control groups.
The existing 22 history/request-sequence controls pass before source changes.


Paradex implementation checkpoint: one boxed `CursorHistory<Output>` replaces
local row buffers, cursor owner and direct bounded-cap marker. Boxing only this
owner avoids enlarging the existing Pending enum beyond the strict Clippy bound;
no other continuation or account state is boxed/rewritten. Native raw-row counts
remain cumulative, with unchanged endpoint request pins and final shaping.

The shared proof-stage extension uses the same walker and lifecycle admission,
without suppressing repeated-cursor detection. Probe stages cannot stage or
already have admitted result rows in the current stage: misuse poisons completion
and returns one strict Internal failure. A valid restart preserves prior-stage
accepted rows and the first omission cause. Direct core and real-completion
controls cover both staged/admitted misuse, cycle-before-budget and valid prior
stage preservation. Removing the guard makes the real-completion assertion RED.

All 30 native Paradex history controls pass, including the 22 pre-existing
successful request/codec/lookup controls. The three original-RED failure groups
now pass; later-failure controls cover transport, HTTP, native envelope, malformed
row, truncated JSON and missing-list failures. Native controls also cover
strict queued JWT/preparation and delegated-signing failure after acceptance,
page-50 lookup cycle, a matching lookup row inside a failed envelope, capped
nondefinitive lookup without a new marker, empty traversal fallback and the
cumulative raw-row no-fallback boundary. Removing the strict preparation routing
arm produces assertion RED; source is restored before the final checks.

Correctness/adversarial reviews are clean after hardening probe admission. This
is shared engine enforcement plus native adapter facts, not a venue-specific
replacement algorithm. No final replay/WASM/live deployment proof is claimed at
this checkpoint. Other history families and B11 caller coverage remain open.


Final local checks at this checkpoint: 1,007 engine/Paradex/Binance/BloFin native
tests pass, including 30 Paradex history tests; touched-crate strict all-target
Clippy, fmt, banned-pattern scan and changed-doc relative targets pass. Final
independent correctness/adversarial reviews are clean. Full exact-head CI and
shipped-artifact/replay comparison remain required before merge; no local release
WASM, timing or full replay sweep was run.




## Next fan-out: Lighter cursor histories

Plan from owner-pinned actual `2c5c5f6987f35cafc0e272282e54da37c0a66d4f`.
Paradex #1748 is separate and watched; merge it on exact-head green under the
announced separate guard/merge flow, then integrate its actual shared dependency
before final Lighter proof. Bybit, Kraken Spot and Paradex native sources are
out of scope here. #1747 remains a separate draft waiting on #1741.

Lighter still owns a CursorWalk, mutable result vector and per-query command in
history.rs. Replace those with the existing `CursorHistory<Row>` completion and
staged/accepted rows. No new engine API, pager, marker helper or scheduler.
Keep native wire mapping, cross-stage trade/funding identity namespaces,
principal checks and schema validation; a failed response's prefix never enters
accepted rows. First failure before mapped rows rejects typed; later actual
HTTP/native/stream/row/list failure returns only accepted pages with one owned
incomplete Warn before success, including Error-only logging. A wrong-principal
failed page contributes no rows. Empty/fully filtered pages alone cannot permit
partial recovery. Multiplexed transport/decode callbacks retain Internal rather
than inventing Network classification.

All-income keeps one owner through trades then funding. Admit the final valid
trade page and terminal stage before restart, retaining accepted rows, dedupe and
first missing-coverage cause. Record the accepted-length baseline at the native
stage transition so each stage's existing output-limit stopping rule uses shared
accepted/staged counts; do not add another paging/accepted-row counter. Preserve
limit defaults/clamps (1..200), native request limit 100, 1000-page stage budget,
URL/cursor escaping, market/account/order selection, optional no-I/O behavior,
raw row counts, lower-bound past-window termination, financial units and fee/
rebate/PnL mapping, stable final sort and global truncation.

Repeated cursor/budget and saturated full page without a continuation remain
strict endpoint-policy rejections even after accepted rows. Synchronous auth-token
preparation failure remains strict, now typed Internal/notSent through the owner;
never route it through recoverable HTTP-page failure. Do not alter signer/auth
implementation, add resends, weaken principal validation, or touch account/live
reconciliation. Native successful mappings and request sequences must remain.

Before implementation, run existing native history/request-sequence preservation
controls and commit original-RED typed-first/later-page/cross-stage failure tests.
Cover HTTP/native/transport/malformed/truncated/missing-list failures, failed
prefix exclusion, empty/filtered zero-accepted rejection, cross-command ownership
at Error-only logging, funding-stage failure after accepted trade income, strict
cursor/saturation and auth preparation refusal, and unchanged no-I/O/filter/order
contracts. Update caller/audit/quality scope without closing B11 or other-family
fan-out. If layout grows, box only the owner where needed for Pending size.

Expected source scope: Lighter history.rs and native history tests only. Seek a
narrow callback arm only if a real preparation seam needs it; no engine/host/
dispatch/request/signing/clock/metadata/execution/coverage changes. Coordinate
with the other lanes before edits. Plan/correctness/adversarial reviews must
answer the architecture gate: one existing engine owner adopted by a native
adapter, not a per-venue paging/completeness implementation. Focused engine/
Lighter/Binance and shared history references, strict Clippy/fmt/bans/links; final
actual dependencies plus exact-head CI/replay proof, normal owner queue, separate
guard and merge. No local release WASM/full replay/timing sweep or NX edits.


Lighter implementation receipt: original source passed 19 controls and failed
six assertion groups for typed first/zero-accepted failures, accepted-page
recovery, cross-stage income, concurrent owner isolation and strict typed auth
preparation. The preserved cursor/saturation and request-sequence controls stay
strict. The adapter now owns one boxed CursorHistory<Row>; native Row is serialized
untagged to preserve the same output objects. No engine or signing API changes.
Required native list validation rejects missing/null/object lists; streamed
prefixes remain staged until the complete response verdict. Independent plan,
correctness and adversarial review found no blockers. Review-requested controls
also check no incomplete marker on strict refusal, truncated streamed prefixes,
malformed funding after prior trade income and orders/public-trade completion.
One review precision fix preserved original malformed-envelope and principal-row
legacy text instead of adding a new prefix. Caller metadata does change where
previously unclassified errors become Internal, and auth refusal gains NotSent;
those changes are explicitly recorded in the caller section.

Focused engine/Lighter/Binance/BloFin verification passes 1004 native tests and
strict all-target Clippy. Final touched-venue checks, formatting and repository
bans run before committing. Actual shared/build dependencies must be integrated
before final proof/merge: the active owner hold is #1746, then #1728, then the
ready #1748/#1749; this Lighter follow-on has no exception. No CI or deployed
runtime closure is claimed by local controls.





## Coinbase Advanced Trade owner adoption (2026-10-22)

Fresh branch from actual1746 `3f0e3e450b2`: move Advanced Trade fills,
historical orders and public trades onto existing engine `CursorHistory<Row>`.
This is existing history-owner fan-out, not another per-venue pagination policy.
No Starbase integration, account reducer, signing/dispatch, engine source/API,
metadata, scheduler, host runtime or public query argument changes. Keep the
current Spot/INTX/CFM endpoints; one owner spans CFM then Spot family reads.
The queued Bybit/Kraken/Paradex/Lighter/WEEX/Bitunix/BloFin/Binance slices are
excluded. Coordinate Coinbase signer/account ownership before source edits.

Current Request stores CursorWalk, cumulative rows, fingerprint identity proof
and per-family accepted count itself. Streamed rows enter that cumulative set
before final HTTP/envelope validation; later failures discard every accepted
page through cx.fail. Replace cumulative rows/walk with shared owner, keep a
response-local mapped candidate buffer, validate full HTTP/native envelope,
required rows and all row mappings before admitting it. Native fingerprint
comparison remains a mapping-integrity proof for conflicting IDs; shared owner
owns dedupe membership and staged/accepted coverage. Preserve filtered identity
checks and make repeated identical rows non-progress without inventing retries.

First/zero-admitted page transport/decode/row failure rejects typed Internal;
a later failed page returns only prior admitted rows plus exactly one command-
owned history.incomplete Warn before successful completion. Page budget or
repeated native cursor becomes shared-owner incomplete coverage. Invalid native
cursor shape/length, proof_token_required, request construction and signer
preparation remain strict rejection even after acceptance; preparation errors
report notSent without manufacturing a successful ledger. No failed response
prefix or failed family can publish rows or mutate account state. Terminal
paths remove active command ownership; concurrent queries remain isolated at
Error-only logging. Preserve existing admission errors outside started history.

Keep 50-page budget PER family, original family order, product filters, cursor
escaping, immediate continuation, 1..1000 result limit and per-request native
limits. Preserve strict identity conflict checks, first-wins identical repeats,
UTC inclusive filters/RFC3339 rounding, CFM native contract units, public maker-
side inversion, stable descending timestamp sort and final global truncation.
Do not claim Starbase parity, live coverage or B11 NX closure.

Commit original-RED regressions before production: typed first failure; accepted-
only later transport/HTTP/decode/mapping failure with failed-prefix exclusion;
filtered/empty first pages do not authorize partial success; shared repeated-
cursor coverage; CFM-to-Spot accepted recovery; concurrent owner/Error-only
marker binding; strict proof-token/preparation control. Preserve existing native
request/shaping/unit tests, adjust only their old later-failure contract. Run
focused engine/Coinbase/Binance tests, strict Clippy/fmt/bans/link checks and
independent plan/correctness/adversarial architecture reviews, then full exact-
head CI/replay with actual shared/build predecessors before separate guard/merge.
Expected own paths: Coinbase history.rs/history tests (native test support only
if strict preparation needs it), existing OWNED_HISTORY/AUDITS/QUALITY/CALLERS.

Coinbase independent plan review refinement: shared CursorHistory on actual1746
is sufficient. Per-family output is accepted-length baseline plus validated
candidates; family completion precedes restart, and CFM reaching its own limit
still proceeds to Spot. Native fingerprint comparison stays before UTC filters
and shared freshness; a conflict invalidates the entire page and follows the
same actual-mapping-failure policy (prior accepted-only partial, typed rejection
if none). Identical repeats add no output; do not enable a new no-new-rows cursor
termination policy. Preserve the pre-finalization 2*MAX_OUTPUT safety bound.

One narrow preparation routing addition is necessary: Coinbase rest.rs signs
queued requests in dispatch(), and fail_unsent currently sends History through
ordinary response-failure handling. Add only its Pending::History match arm to
strict owner rejection, preserving all signer/queue/dispatch behavior; otherwise
later credential/entropy refusal would incorrectly resolve prior rows. Existing
request() consumes Pending before returning immediate queue/path refusal; keep
that direct active cleanup and typed Internal/notSent rejection rather than
cloning or reconstructing the owner. Include rest.rs in owned scope. No shared
signing/dispatch change is introduced. Native preparation and CFM-family tests
may use existing cfg(test) private access, never a production mutator. Rows lane
confirmed no current Coinbase history/request-builder ownership conflict.

Coinbase implementation receipt: original12 history preservation tests pass;
regression-only ca876aa0aac adds nine assertion-RED integration cases and a
CFM-to-Spot original-RED control. Expanded controls against exact original native
source produce10PASS/12 assertionRED; corrected bytes restored by hash in finally.
The controls cover typed first/zero-row failure, accepted-only failed-prefix
exclusion, command isolation at Error-only logging, filtered fingerprint conflict,
native continuation/proof refusal, queued signing refusal, duplicate-only changing
cursors and the50-page cap. Corrected final22 history controls and native family
control pass; full final Coinbase187 tests pass. Earlier broad engine/Coinbase/
Binance833 controls pass; final adapter strict all-target Clippy/fmt/signing bans,
four touched-plan link checks and diff checks pass. Native owner is boxed to
preserve bounded Pending layout without changing lib.rs or lint allowances.

Plan/correctness/adversarial reviews CLEAN after one caught mismatch: native Page
metadata deserialization must reject strictly, whereas malformed whole-response
stream/decode failure retains ordinary accepted-page recovery. Explicit controls
now prove that split. Shared default CursorWalk does not enable a new no-fresh
termination policy; duplicate-only changing cursors reach the existing family cap.
No engine source/API added. Full exact-head CI/replay/actual build predecessors
remain pending; queue keeps actual1728 then KCS1753 ahead of this source slice.

Coinbase CI scan follow-up: gitleaks8.30.1 found exactly one generic-api-key
false positive in original regression commitca876aa0aac, native family fixture
line10: the same literal organizations/test/apiKeys/test already recorded for
existing Coinbase history/account/trading fixtures. The native seed is fixed
[7;32], not an account credential. Add only that exact commit/path/rule/line
fingerprint to .gitleaksignore; no scanner rule, path pattern or config change.
Full original-range scan reproduces RED1, exact exception makes GREEN0; independent
source/report review CLEAN. This adds a ninth owned path, with no runtime change.






## Next fan-out: WEEX income range and single-page completion

Plan from owner-pinned actual `5743c485d44e348e3d29039c4ebe83f6a78f0186`.
Paradex #1748 and Lighter #1750 remain separate watched PRs. Landing is held for
#1746, then #1728, then #1748/#1749 under the owner queue. This slice has no merge
exception and will integrate actual shared/build/history predecessors before
final proof. Bybit, Kraken Spot, Paradex and Lighter native sources are excluded.

Scope is WEEX income only: replace Read::Income's RangeWalk and accepted-row
vector with the engine RangeHistory owner for bounded ranges, and the existing
CursorHistory owner for one-page/unbounded income. Native Request rows remain
response-local parse staging so retries/preparation and whole-envelope admission
cannot poison the owner. State map keys and request command IDs route responses;
only the retained engine owner may finish the query or emit incompleteness.
WEEX fills/orders/public-trades list reconciliation, its missing-fill-ID reporter,
account/settings/metadata, signing, dispatch and scheduler are separate follow-ons.

The five venue-local income completeness emitters disappear. A failed first page
or any failure before accepted mapped rows rejects typed, never an empty complete
array or an early marker. An actual later HTTP/native/transport/stream/list/row
failure immediately resolves accepted earlier pages with one owner Warn before
success, including Error-only logging, and stops requesting further chunks.
The failed page's prefix is excluded. This intentionally supersedes WEEX's old
skip-failed-chunk-and-continue contract. Multiplexed transport/decode callbacks
retain Internal rather than inventing Network classification. Native signing
preparation refusal stays strict after acceptance and reports notSent; keyless
and admission no-I/O behavior stays unchanged.

Preserve native POST body/headers, 100-day span windows, 100-row ranged requests,
single-page caller-limit clamp, 100 ms pacing and existing host-pause deferral.
RangeHistory owns inclusive-boundary selection/dedupe, including the existing
compound income identity and missing-bill-ID timestamp fallback; native mapping,
units/categories, stable descending sort and no extra final truncation remain.
Opaque nextKey/hasNextPage is a valid-page coverage omission, never proof that a
span walk consumed an advertised cursor. A valid page is admitted and its gap is
retained by the engine; existing span continuation is preserved. A final success
emits one marker, not one per page. Budget exhaustion retains admitted rows with
owned missing coverage, preserving the 10,000 physical-query selection cap.

Extend the shared owner only where its current API cannot express these facts:
a validated incomplete-page admission for RangeHistory, a single-page coverage
fact for CursorHistory, and an engine-owned optional lifetime request budget for
RangeHistory (not a venue counter or replacement pager). Reject invalid lifecycle
use before changing accepted rows/gap state. A pending/unvalidated page must not
be turned into budget or cursor-gap success. The budget must stop before issuing
the first over-budget request, retain the first omission, and keep owner-only
command fields. Exact API is subject to independent plan review before code.
No changes to RangeWalk algorithms or release/admission time ownership.

First commit original-RED native controls: typed first/zero-accepted failure,
accepted-only later failure across pages/chunks, valid-prefix exclusion, matching
command ownership with concurrent reads at Error-only logging, no markers on
rejection, and exactly one marker on cursor-gap/budget success. Preserve existing
body/pacing/pause/dedupe/request-sequence controls; deliberately update tests that
pin continuation after a failed first page. Add small engine controls for budget
edges, validated-gap admission and strict lifecycle misuse. No local large
request/timing sweeps; existing full native cases and CI supply those controls.

Expected paths: engine/src/history.rs and engine history tests, WEEX history.rs
and native history tests, with only a narrow callback seam if a real preparation
path needs it; existing AUDITS/QUALITY/CUTOVER_CALLERS/OWNED_HISTORY sections.
Coordinate shared-engine and WEEX ownership first. Plan, correctness and
adversarial review must answer the architecture gate: reusable engine coverage/
budget policy plus native fact mapping, not another venue-local history core.
Focused engine/WEEX/Binance and shared-history references, strict Clippy/fmt/bans/
links, exact-head CI/replay and actual predecessor proof before the separate
guard/merge. No NX writes, B11 closure or deployed-runtime claim.


Independent WEEX plan review: feasible, with validated atomic coverage-admission
APIs on the shared owners. Validate active/finished/poisoned state and range
row/fact alignment before recording a gap or accepting the page; preserve
RangeWalk.failed() as an admitted native coverage fact. Lifetime cap reuses
RangeWalk.requests, with no second counter: only request N+1 is stopped, while
natural Finished after exactly N remains complete and pause resumption cannot
step/charge twice. Single-page includes equal bounds and preserves its original
rows without range filtering/dedupe. A valid empty page advertising a cursor may
succeed incomplete; a failed page with zero accepted rows must reject. Final
encoding returns Result, replacing the income expect. Terminal completion removes
read state and clears retained delay/deferred continuation.

The existing native-backed fixtures/incomplete-query-event.json producer currently
pins a first-failure marker. Replace that producer with accepted-page/later-failure
command-1 success under the new contract; regenerate only its exact event and
adjust linked host/worker test message assertions if pinned. No host runtime or
protocol edit. Small engine tests cover exact/over budget, lifecycle misuse and
first-cause retention; existing native lifetime cap covers 10,000 selections.


WEEX implementation receipt: baseline23 native history controls pass; original
four added behavior groups fail by assertion (untyped/early-marker first failure,
continuation after empty prior coverage, discarded accepted rows, early repeated
cursor-gap publication). Regression-only commit12e0585321e precedes production.
The corrected final30 history tests pass, including seven failure modes,
concurrent owner isolation, equal-bound/native-row preservation and resumption of
the same selected range after a shared IP pause. Existing10,000-selection native
cap still passes. Three new engine groups cover exact/over/invalid/late budget,
first-cause retention, valid empty gaps, invalid pending/finished admission and
encoder failure. An off-by-one budget mutation failed its assertion and exact
source bytes were restored before corrected GREEN.

Plan/correctness/adversarial source reviews are CLEAN. Focused engine/WEEX/Binance
820 tests and strict all-target Clippy pass; final touched-history controls and
Clippy/fmt/bans pass after test refinements. The native-backed fixture is now
produced by command1's accepted-page/later-failure success, with two focused
host/worker ownership tests passing through corepack. No host runtime/protocol,
financial reducer, scheduler/release or signing edits. All remaining actual
shared/build dependencies, exact-head CI/replay proof and owner hold/queue still
apply before merge; no deployed caller completion claim.


## WEEX follow-on: fills, orders and public-trade list completion

Plan from owner-pinned actual `5743c485d44e348e3d29039c4ebe83f6a78f0186`,
exact-head stacked on reviewed #1751 `f9753aa6bb2ce1a0c6e689f449c1ba833f9c6c26`
under explicit owner authorization. The stack is needed for validated native
omission admission through CursorHistory::advance_with_coverage; this follow-on
adds no engine API. Keep #1748/#1750/#1751 watched and honor the #1746/#1728
hold and owner queue. Integrate actual predecessors before final proof/guard.

Replace WEEX Read::List's accepted Rows/vector and query-local dedupe set with
one boxed engine CursorHistory<ListRow>, shared across every native fill window
or paired normal/algo order read. Keep typed response-local Rows until the whole
HTTP/root-or-orders-list/stream/row verdict succeeds, then use shared identity
dedupe, stage rows and admit the single-page native stage. Advance/restart only
through the owner; a pause resumes the selected native Fetch without advancing
again. Keep Fetch descriptors as native endpoint/parameter facts; replace the
hand-rolled bounded-window calculation with the existing engine paging::windows
helper while retaining the exact current budget/admission and partial-bound
contracts. Do not add venue-local cursor/page/request counters or resends.

First/zero-accepted actual HTTP/native-response/transport/stream/list/row failure
rejects typed without a success marker. A later actual failure returns accepted
earlier windows/endpoints with one owner Warn before success, stopping the native
queue; an invalid response's prefix is excluded. In particular, a good normal
order read followed by failed algo orders now retains good normal rows with an
owned gap instead of discarding the query. Synchronous signing preparation
failure remains strict after accepted rows and gains Internal/notSent. Existing
admission/keyless/no-I/O behavior stays unchanged. Multiplexed callbacks retain
Internal rather than inventing Network classification.

Missing fill IDs remain omitted without invented identities, after the existing
symbol/order/time filters. Stage that coverage fact on Request until a whole
valid response is admitted. Only then emit its once-per-query native Warn
row.skipped(field id) and pass the omission to the shared owner; the engine emits
one history.incomplete at successful finalization. No failed response may mark
coverage or consume the valid-page diagnostic latch. A valid all-omitted page may
succeed incomplete []; a later actual failure with no admitted rows still rejects.
Retain the native empty-fill event text/fixture and translated host/worker owner
proof, updating tests only for intentionally deferred publication/partial success.

Preserve native filters, absolute quantity/fee normalization, timestamp units,
large native IDs, normal/algo order mapping, first-wins symbol|id dedupe across
windows/endpoints, stable descending fill/order sort and original public-trade
order/duplicates. Keep native limit clamps (fills/orders1..100, trades1..1000),
page0, seven-/90-day windows, immediate endpoint/window continuation,
shared-pause timing, partial/absent bounds, and no new final truncation. Preserve
existing handling of endpoint continuation metadata unless an independent plan
review finds a concrete completeness hole needing an explicit shared gap fact;
never invent another pagination algorithm. Income #1751 behavior remains exact.
Terminal completion clears retained list state/timer so late work cannot resume.

Before production edits, commit original-RED controls for typed first failure,
accepted-only later fill-window and normal/algo recovery, zero-accepted rejection,
failed-prefix exclusion, missing-list validation, concurrent owner isolation at
Error-only logging, deferred omission publication, failed-page latch isolation,
strict signing/pause and exact native shaping/request-sequence preservation.
Preserve native-backed empty-fill fixture proof and update the deliberately old
whole-query-rejection test. Review plan, correctness and adversarial gates must
state this is existing engine owner adoption, not a per-venue paging/coverage
implementation. Focused engine/WEEX/Binance plus shared-history reference tests,
strict Clippy/fmt/bans/links, exact-head CI/replay and actual dependencies before
separate guard/merge. No local WASM release/replay/timing sweep or NX writes.

Expected own paths after the exact stack: WEEX history.rs/tests/history.rs and
only native-backed fixture/host-worker TEST adjustments if required; existing
AUDITS/QUALITY/CUTOVER_CALLERS/OWNED_HISTORY notes. No engine, signing, dispatch,
account/financial reducer, metadata, release-admission or scheduler source edits.


Independent WEEX list plan review: feasible with no new core API. List
continuations are immediate; only income uses 100ms pacing. Preserve that exact
timing and pause-only List.delay. Keep the ratio-budget admission alongside
shared window-count bounds (they differ near the limit). Explicit
allAlgoOrders.hasMore:true is a concrete existing omission: admit valid rows
with an owned gap while keeping the endpoint queue unchanged; absent/false is
complete and explicitly malformed metadata rejects whole-page admission. No
new pagination or retries. Cover empty advertised-more responses, malformed
metadata, public duplicates, immediate/pause continuation and budget boundaries.


WEEX list implementation receipt: original31 controls pass and seven new behavior
groups fail by assertion before source edits (regression-only f5d529491f8).
Corrected38 history controls pass, and final39 add the legacy ratio boundary and
complete paused normal/algo resumption. The native private preparation control
revokes test setup between accepted normal rows and preparing algo orders;
original source rejects without kind/outcome, corrected source rejects strictly
Internal/notSent with no marker/partial/second request. The fixture lives in
native test support, not a production setup holder, and adds no signer allowlist
exception. Exact original-source RED/current restored-source GREEN are retained.
Two unchanged native-backed empty-fill host/worker fixture tests pass.

Independent plan/correctness/adversarial reviews are CLEAN. Focused engine/WEEX/
Binance831 tests and strict Clippy pass before final test refinements; final
WEEX tests/strict Clippy/fmt/bans/links run before freeze. Source needs no engine
API and preserves income. The explicit owner authorization now permits exact
preintegration of reviewed Bybit #1728 head31035f3f09a71e5fb29d1083c217512972e5f9ec,
which includes actual #1746 build policy3f0e3e450b2, for parallel CI. Actual
Bybit ancestry, all actual shared predecessors, final exact-head CI/replay and
separate guard/merge remain required; no landing/deployment claim.

## Phemex income range ownership (planned)

Start from actual #1756 `5f6488c7664`. This slice adopts the existing shared
`RangeHistory` owner for `venues/phemex/src/income.rs`; it adds no pager policy or
venue-local marker emitter. Single-page fills/orders/trades and client-ID lookup
remain outside this bounded slice. No signing format, request path, credential
selection, rate-pause behavior or financial Store mutation changes.

The current income adapter keeps separate `RangeWalk`, page, accepted rows and
numeric command fields. Failure of a later page/currency discards accepted
ledger rows, and a saturated range rejects its entire result. Replace these
parallel states with one `RangeHistory<Income>` and `Completion`, retained across
USDT/USDC stages through an additive checked shared range-stage restart. Stage decoded rows
until the full page validates; feed native timestamp/identity coverage into the
shared pager before applying the existing time/type filter. Failed page prefixes
must never enter accepted results. Preserve occurrence identities, running-balance
components, ordering, optional result truncation, limits, probes and both
settlement currencies. No new offset paging or endpoint behavior is invented.

Before implementation, add behavior assertions that fail on current source:
first-page transport/HTTP/malformed failure rejects with a typed error and no
success/marker; later-page or second-currency failure retains only accepted
rows, emits exactly one command-owned incomplete Warn, then completes. Cover
streamed failed-page prefixes, two simultaneous command owners and Error-only
logging. Saturated-timestamp coverage must become a shared-owned incomplete
success with accepted rows rather than an empty complete result or total loss.
Existing successful short/full-page, duplicate identities, type filtering,
window boundaries, both currencies, rate deferral and strict admission controls
remain. Explicit construction/signing/serialization refusals stay terminal and
must not become recoverable pages; classify only facts the failure callback
actually provides, preserving native details where available.

Plan audit must check the current `RangeHistory` stage API, Phemex request/sign
failure call graph and test helpers before production changes. Add or adapt
focused assertion tests first and retain original-red logs. Then implement the
shared-owner adapter, run focused engine/Phemex/Binance native tests and strict
checks with Cargo concurrency four, and obtain correctness/adversarial reviews.
Use Corepack Yarn only; never run the account-policy check. No local release
WASM, full replay or timing matrix. CI and source-bound artifact proof own those
gates. Document the caller-visible retained-partial/typed-first-failure contract
and supersede the historical all-or-nothing test expectation explicitly.

Architecture gate: adoption of one engine-owned range/completion policy by a
new family, not a per-venue fix of a cross-cutting concern. Coordinate request
builder ownership with the signer lane before editing; this slice does not
change engine dispatch, RequestContext, execution, row admission or metadata.
Merge only after #1737 and any then-current owner queue dependencies, with fresh
integration/proof and a separate announced guard and match-head merge.


Phemex plan audit refinement: `RangeHistory` currently lacks the cursor owner's
stage restart and filtered-admission operations. Add these to the engine core
before venue adoption. A checked `restart_stage(RangeWalk)` must require finished
state with no active fetch, staged rows/facts or lifecycle error, fold the old
walk's coverage gap into the same Completion, retain accepted rows and first gap,
and preserve the cumulative request budget across stages. Never construct a new
command owner for the second currency. Add shared lifecycle/coverage/budget tests.

Add shared `accept_page_where` filtering after the pager has observed every
validated native timestamp/identity. Only selected rows satisfying the output
predicate enter accepted rows; `accepted_len` must exclude filtered rows. A
query whose earlier pages produced no eligible output still rejects on failure,
not an empty partial success. The venue must not keep a parallel accepted vector
or use placeholder Option rows to inflate acceptance. Engine tests must pin
filter-before-retention/after-coverage semantics and rejected-page prefix
isolation. The adapter remains responsible only for native decoding and its
existing time/type predicate, final sort/truncate and currency selection.

Both methods are additive shared owner operations, not a Phemex pager fork.
Coordinate `engine/history.rs` with the managed Completion bridge; RequestContext
authority stays outside this slice. Signer API inspection found current Phemex
header construction infallible; do not invent a new sign-failure callback or
change request builders. The signing lane confirmed these regions are clear.


Final plan review: stage replacement must account for retired-stage request
selections as well as the current walk count, and `with_request_budget` cannot
be reapplied after restart to reset authority. Test the exact boundary across
two stages, inability to reopen an exhausted budget, and invalid early restart
without mutating pending rows or an existing gap. Keep the existing more-than-
100,000 retained-output guard a strict policy rejection, never a partial success.


Saturated-stage sequencing is an intentional change: the old USDT truncated
walk rejected before USDC. The shared owner will retain that coverage gap and
continue the requested USDC stage, then return eligible accepted rows with one
owned marker. Test that additional stage explicitly. Healthy request sequences
stay unchanged. Preserve equal-bound padding only inside the walker
(`end.max(start + 1)`), native-request clipping to actual end, and output filtering
against the actual caller end rather than the padded synthetic bound.

Phemex implementation checkpoint: the four original-source behavior assertions
are RED before adoption and GREEN through the shared range owner. New core
controls cover composite ownership/first gaps, early restart, filtered coverage
and cumulative budgets. Independent review found a boundary regression in the
initial adapter: a padded equal-bound walk admitted against the synthetic end
while native HTTP used the actual end. A new assertion reproduced an inverted
continuation. Additive shared `restrict_active_range` now binds narrower native
bounds before dispatch and rejects expansion/inversion or changes after staging;
coverage still sees all decoded row facts. Both direct and streamed edge cases
are corrected, with additional core restriction controls. The range owner is
boxed inside the native request to keep Pending's enum variants bounded, rather
than suppressing the strict Clippy size check. This does not change completion
authority, request wire or the planned physical-release integration boundary.

## Next cursor fan-out: GRVT history ownership

Plan first on fresh `feat/rust-owned-history-grvt` from actual capability landing
`4c4df7c99dc` (2026-10-22). Avoid active OKX/Bitget/BingX signer regions, Bybit
and Kraken Spot. GRVT already uses engine CursorWalk but retains accepted and
staged rows plus numeric completion in its venue continuation. Adopt one boxed
CursorHistory/Completion owner for fills, closed orders, funding/fill-derived
income and public trades. Preserve single-order lookup's separate identity proof
and result shape. No RequestContext/dispatch/signer/host/metadata/financial-store
edits; no managed Completion bridge or unmerged Phemex range dependency.

Keep native row codecs, raw/output identity ledgers, lifecycle ownership maps,
subaccount/symbol/time filters and count facts in the adapter. Move output-row
staging/acceptance, cursor advancement, page-cap/stall coverage and finalization
to the existing shared owner. Stage a full page before admission. Before any
accepted output, terminal HTTP/transport/decode/envelope failure rejects typed;
after accepted output, return only previous accepted pages with one command-owned
incomplete Warn before Done, visible at Error-only logging. Do not publish a
failed page prefix. Keep native no-row lookup proof separate from history rows.

Retain the existing five read retries and once-only session refresh unchanged.
They reset only the current unaccepted page, including staged rows and native
page identity/lifecycle facts; accepted rows and cursor/budget stay unchanged.
If needed add a small shared CursorHistory discard-staged-page API, fail closed
when called after completion/poisoning. Do not copy retry/paging policy into the
adapter or mutate the shared cursor/identity state before page validation.
Funding-to-fill income remains one owner across restart_stage, preserving the
first gap, accepted rows, native stage counts and existing per-stage page caps.
Keep stage-specific shaping/limits and final descending sort/truncation exact.

Order-filtered fills remain a strict proof when lifecycle ownership is missing
or ambiguous, the requested total limit is exceeded, or bounded cursor walking
cannot prove the complete order fill set. Route those explicit policy refusals
through shared reject, preserving no partial totals. General history stalls/caps
use shared command-owned coverage. Signing/session preparation remains strict;
do not turn a pre-send setup refusal into a recoverable failed HTTP page.
Inspect failure origins before assigning kinds; the common callback multiplexes
transport, malformed-stream and session failures, so do not label all Network.

Commit original-source assertion controls before production edits: first and
later terminal failures, failed streamed prefixes, retry/session-refresh page
reset, cross-stage income failure, filtered/no eligible output, concurrent owners
and Error-only delivery. Preserve strict order-fill proof controls and exact
request sequence, large IDs/nanosecond bounds, maker/fee/PnL units, row.skipped
behavior and live Store isolation. Extend engine tests for safe current-page
discard without erasing accepted rows, coverage or page budget. Independent
plan/correctness/adversarial reviews must answer the architecture gate: existing
shared ownership adoption plus reusable admission reset, not a venue-local
completeness/pagination implementation. Focused engine/GRVT/Binance tests and
strict touched Clippy/fmt/bans locally with four Cargo jobs; no local WASM release,
full replay, size or timing sweep. Update caller/AUDITS/QUALITY with exact scope,
then current-head CI/source-bound proof, queue announcement, separate guard and
separate merge. Keep #1758's watch active; no cutover/NX/deployment closure.

Independent GRVT plan review is CLEAN with preservation refinements: every
terminal failure of an order-filtered fill proof stays strict, including actual
later HTTP/decode/transport loss after accepted fills. Keep fail_pending strict
for deferred/bootstrap/session-refresh failure; only explicit final history
HTTP/stream/transport call sites use recoverable owner finish. Retain native
stage_count independently of total accepted rows, especially after funding to
fills. Discard preserves admitted rows, first gap, cursor/pages/dedupe/stage state;
calling it after completion/poisoning poisons strict completion. Add exhaustion,
refresh-failure, filtered later-failure and lifecycle-misuse controls. RequestContext
owner explicitly confirmed these additive lower HistoryRows/CursorHistory regions
clear; Completion authority and its pending follow-on API are untouched.


GRVT local implementation receipt: regression-only `5a3d2f20fbb` has 24 existing
controls GREEN and five new behavior groups assertion RED on original production
source. Shared owner adoption makes them GREEN. Additional controls pin exhausted
streamed retries (all six attempts, byte-identical native body), strict later
order-filtered failure, strict failed session refresh, and a general repeated
cursor's owned partial result. Existing successful retry/refresh, full four-leg
income request sequence, large IDs/nanosecond bounds, lifecycle proof, lookup and
row-skipped controls remain. Two engine controls pin safe discard retaining
accepted output/first gap/cursor/pages/identity and strict lifecycle poisoning
after completion. Lower-core regions were explicitly cleared with RequestContext
owner before editing; managed Completion authority is untouched. Independent
implementation reviews and final focused checks precede publication.


GRVT independent implementation and final tests/docs reviews are CLEAN. All 834
focused engine/GRVT/Binance tests and strict Clippy passed before three additional
adversarial controls; final focused history tests pass with those controls,
including 34 GRVT history tests. The additions pin discard after existing
poisoning, limit-one mixed income with native stage counts and stable newest-row
selection, and the real 50-page general cap with no 51st request. API docs state
that staged-page discard does not roll back direct shared fresh observations;
GRVT keeps retryable identity facts local until admission. First-kind telemetry,
strict filtered/session proofs and B11 limits are recorded in the caller doc.
Full artifact CI/proof and actual queued predecessors remain required.

## Next cursor fan-out: Crypto.com exact-nanosecond histories

Plan first on fresh `feat/rust-owned-history-cryptocom` from actual capability
landing `4c4df7c99dc` (2026-10-22), outside OKX/Bitget/BingX signer work. Replace
Crypto.com's local accepted rows/completion with one boxed CursorHistory owner
for fills, closed orders, income and public trades. Preserve unsupported lookup,
initial credential/range admission and effective-zero-limit/equal-bound no-I/O. No account,
signing, dispatch, RequestContext, metadata, Store or host edits.

Keep native exact u64 nanosecond bounds and oldest+1 exclusive-end translation,
method/key pacing and deferred timer order. Do not convert native cursors through
f64. Feed cursor strings/facts to the shared walker only after a whole valid page.
Use the existing native stopping rules: requested output limit, empty page,
short repeated-identity page and exhausted time range. Missing exact cursor proof
when continuation is required, and a non-decreasing inclusive boundary, remain
strict policy refusals even after accepted rows; they cannot claim complete
pagination. Do not invent a 50-page policy absent from this venue: use the shared
walker's representable page ceiling as a fail-closed limit, with native monotonic
bounds providing ordinary progress. Public trades remain one unsigned GET with
native order/duplicates and count cap; ranged income keeps its existing no-output
cap when both bounds are supplied.

Stage output in the shared owner; retain native page identity facts separately
until admission/retry. On terminal actual HTTP/native/decode/transport loss before
eligible accepted rows, reject typed. After accepted rows, recover only those rows
with one command-owned incomplete Warn before success, including Error-only
logging. A malformed or failed response prefix is never admitted. Keep exact
current-page retries: one 40101 retry, otherwise three transport retries, unchanged
1000ms delay and shared method pacing. They discard staged output and roll back
only native page identities; accepted rows, cursor/budget and first coverage cause
survive. Reuse #1760's small shared discard API after explicit stack/region
coordination rather than adding a second reset implementation. Actual #1760 and
other shared predecessors must be integrated before final proof/merge.

Commit failing-first controls on original source before production edits: typed
first/zero-eligible failure, later HTTP/native/malformed-page failure, exhausted
stream retries excluding all prefixes, retry success preserving exact bounds and
IDs, concurrent owners at Error-only logging, income partial recovery and public
single-page failure. Preserve fee sign, timestamp units, large IDs, fractional
millisecond bounds, overlap dedupe, requested limit/output order, transfer/session
settlement mapping, missing-cursor policy and independent method budgets. Inspect
callback origins before assigning a kind: multiplexed transport/decode errors
must not all become Network. No new signing capability/argument or NX handler.

Independent plan/correctness/adversarial reviews must answer the architecture
gate: existing shared CursorHistory/Completion and reusable discard adoption,
not another venue pager, marker emitter or row-retention policy. Focused
engine/Crypto.com/Binance tests plus strict touched Clippy/fmt/bans locally with
four Cargo jobs; no release WASM/replay/size/timing sweep. Update caller/AUDITS/
QUALITY scope, publish draft CI and watches, integrate actual dependencies under
the owner queue, then exact-head CI/proof, announcement, separate guard and
separate merge. Keep #1758/#1760 watches; no B11/runtime/cutover closure.

Crypto.com plan review is CLEAN with exact preservation constraints: do not
enable generic no-new-row/empty stalls, because a full duplicate page may still
advance its older bound and only a short repeated page proves exhaustion. Run
strict nanosecond/nonadvance checks only when continuation is needed, after that
short-page stop rule. Keep the single mixed retry counter (40101 below one,
other transport below three), and 1000ms continuation/retry timers distinct from
per-method pacing. Public successful queries remain one page with duplicates,
while their existing retries stay intact. The representable page ceiling is a
strict policy refusal, not a new normal 50-page bound. account::signed is a
synchronous validated-credential helper with no fallible signing callback, so
no account/signing edits are required. Owner explicitly authorized exact stack
on reviewed #1760 `ba960f9b7e8`; genuine integration `73b500095be` preserves all
unowned blobs and the original regression blob. The sole independent-section
plan-doc conflict preserves incoming GRVT then Crypto.com verbatim, with block
hashes/base subsequence and no conflict markers. Actual #1760 remains required.


Crypto.com local implementation receipt: original regression-only
`a45c3c6ce5d` has 12 existing controls GREEN and five new assertion-RED groups.
Shared CursorHistory adoption makes those groups GREEN. Native short-duplicate
stop precedence, full-duplicate older-bound progress, strict later cursor proof,
public duplicate/unsigned retry, both mixed retry orders, accepted/new-ID rollback
and explicitly bounded income with more than the requested output limit are
pinned. The effective-limit qualifier matters: explicitly ranged income already
ignores caller output limits, including zero, while other zero-limit queries
return without I/O. The source uses no new core API and preserves the approved
#1760 exact stack; no account/signing/Completion edits. Independent correctness
and adversarial source reviews are CLEAN; final controls/docs review, CI and
actual queued dependency integration remain before final guard/merge.


Crypto.com final local checks: 756 full focused engine/Crypto.com/Binance tests
passed before the final three controls; all 26 final Crypto.com history tests
pass, including accepted/new-ID retry rollback, both mixed retry orders and
unsigned public retry. Final strict all-target Clippy, fmt, banned patterns and
diff checks pass. Independent source and final tests/docs correctness/adversarial
reviews are CLEAN. No new core source is added beyond the approved #1760 stack;
actual predecessor ancestry, final CI/proof and separate guard/merge remain.

## Next cursor fan-out: Hyperliquid inclusive account histories

Plan first on fresh `feat/rust-owned-history-hyperliquid` from actual
`9bf754a01149d373c522728edbebb46abc5ae5b2` (2026-10-23). Adopt the existing
boxed CursorHistory/Completion owner for perp fills, closed orders, income and
public trades. Preserve spot behavior and client-ID lookup as their separate
paths. No signer, account reducer, dispatch, Context, Store, clock, metadata,
remote wire or capability changes; keep OKX/Bitget/BingX work untouched.

Replace accepted-output vectors/maps with staged typed native output in the
shared owner. Fully validate a page before admission; terminal actual
HTTP/stream/decode/transport failure before eligible accepted output rejects typed,
while a later failure returns only accepted pages with one command-owned
history.incomplete Warn before success, including Error-only logging. The shared
Internal fallback is honest for multiplexed callbacks. Filtered native rows and
failed-page prefixes must not authorize partial recovery. No history row may
mutate live Store. Admission and lookup errors retain their separate behavior.

Preserve inclusive userFillsByTime/userFunding request bounds and the additional
empty/duplicate terminal probe even after a short successful page. Native progress
counts include validated deduped rows before output filters; retain native ID
facts separately from accepted output. CursorWalk owns page-budget/repeat policy;
its internal cursor may bind native timestamp plus identity progress, since a
same-timestamp page with new native IDs must not be rejected as a repeated physical
time bound. Keep physical request bytes and existing 200-page per-stage bound,
with caps 2000 fills and 500 funding used only for full-page missing-coverage
facts. Short all-duplicate pages end the stage; full pages with no new identity
carry an owned gap. General cap/stall termination retains admitted output and
owned incompleteness rather than discarding it. Stage reset and first-gap retention
must use the existing shared restart_stage API, never a second local Completion.
For mixed income retain fill-derived PnL/fee rows through the funding stage and
retain the same owner even if a prior stage ends incomplete.

Orders remain one historicalOrders response: fold native transitions by ID in the
final admitted-row encoder, preserving latest status timestamp and closed/canceled
rank tie precedence before creation-time/symbol filtering. Feed rolling-retention
coverage (raw count at least 2000) through shared admission/completion instead of
emitting a venue-local marker. Public trades remain one response with native
dedupe and descending sort. Keep exact timestamp units, large native IDs, gross
historical PnL, rebate/commission signs, builder revenue metadata, vault/master
identity, fee assets, native sort/tie order, output limits (fills default/clamp),
weight accounting, no-I/O admission and endpoint/body shapes. Do not infer a
new retry or signing policy.

Commit original-source failing-first controls before implementation: typed first
and zero-eligible failure, later HTTP/malformed/stream failure with prefix exclusion,
concurrent owners at Error-only logging, mixed-income funding failure retaining
only accepted fill rows, same-time distinct-ID continuation, full inclusive stall
and real 200-page/no-next-request cap. Keep all existing native history controls,
including rolling retention ownership, order transition ties, lookup and spot
behavior. Add no engine API unless a real native fact cannot use current shared
CursorHistory; coordinate any lower-core change with other history/RequestContext
owners first. Independent plan/correctness/adversarial reviews must answer the
architecture gate: shared owner/pager/marker adoption, with native endpoint facts
only. Focused engine/Hyperliquid/Binance tests, strict Clippy/fmt/bans with four Cargo
jobs; no local release-WASM, replay sweep or timing matrix. Update caller/audit/
quality evidence, publish draft CI and keep all existing watches. Final current
landing integration, authenticated full raw/package proof, independent reviews,
announcement, separate guard and separate merge remain required. B11/runtime/
cutover claims stay open.

Hyperliquid plan review is CLEAN on the existing APIs. Bind progress to newest
physical timestamp plus cumulative validated native identities, never page ordinal
or filtered output count; physical inclusive startTime must not gain one ms.
Admit positive page 200 with an owned gap and no request 201, but allow a terminal
probe after 199 positive pages. Full zero-new-ID pages pass an explicit coverage
gap through shared admission; short zero-new-ID pages end normally. Fold order
transitions in encounter order before filtering, keeping the first equal-time,
equal-rank update. Preserve successful-page-only history_weight accounting and
existing queue reservations. Clock/row owners confirm no concrete Hyperliquid
source overlap. Existing full-overlap test's strict rejection is an explicitly
superseded general-query contract: retain its assertion of missing coverage and
add accepted-row/owned-marker assertions rather than silently dropping the test.

Hyperliquid original regression-only `e2255a5140a` records 11 existing tests
GREEN and six new assertion-RED groups before production edits. Existing shared
CursorHistory adoption makes all 17 pass. Accepted output is held only by the
shared owner; native inclusive timestamp/identity facts feed its pager, and order
transitions fold only in the admitted-output encoder. The prior full-overlap
strict-failure assertion is explicitly superseded with retained-row/owned-gap
assertions. No engine API, lib/account reducer, signing or Context edits. Both
independent source reviews are CLEAN and confirm the architecture gate. Additional
controls pin zero-income native progress/typed rejection, order-fold-before-filter,
first cap gap through funding failure, and full-inclusive first-gap retention
through successful or failed funding. A valid coverage-only cap may return owned
incomplete empty output; actual failure still requires eligible accepted output
for recovery. All 873 focused engine/Hyperliquid/Binance tests passed before the
last two review-requested controls; final history/strict gates and tests/docs
review precede publication. Actual #1762 integration and fresh post-sort-baseline
CI/proof remain required before final guard; all prior history CI watches continue.

Hyperliquid final local receipt: all 22 history tests pass, including both final
review suggestions. Strict all-target Clippy, fmt, bans and diff checks pass.
Both independent final tests/docs reviews are CLEAN; source remains the reviewed
shared-owner adoption. No local release-WASM, full replay or timing run. Final
publication integrates actual #1762; fresh CI and post-sort-baseline proof remain
required, alongside the existing queued history merge order.

## Next cursor fan-out: Kraken futures histories

Plan first on fresh `feat/rust-owned-history-kraken-futures` from actual
landing `fe44969d033529c244fc4f33fce5395c0273df04` (2026-10-23), during the
owner-assigned Orderly #1767 exclusive landing window. This checkpoint is a
plan only. Existing Hyperliquid #1765 and Backpack #1766 CI watches remain
active. No landing merge occurs before actual #1767; real-integrate its
shared-engine input before final CI/proof. Signer, clock and financial lanes
confirm Kraken futures history and request-builder regions are clear; later
account reducers remain separate.

Scope: adopt existing boxed `CursorHistory<Row>`/`Completion` in
`sdk-rs/venues/kraken/src/history.rs`, with original-source controls in the
existing `tests/history.rs` and nearby CALLERS/AUDITS/QUALITY/this-plan
updates. Existing `lib.rs` already routes streamed HTTP/transport failure to
`history_failed`; no routing edit is planned. No engine API, paging primitive,
account reducer, signing, credentials, Store, Context, capabilities, harness,
workflow, fixtures, reference output, budget or NX edits. Numeric Completion
remains the explicit legacy authority, not managed Context completion or a
journal-policy bypass; coordinate any future exact legacy-inventory refresh
after actual landing.

One existing engine owner must own accepted/staged rows, command finalization,
identity/cursor history and the fifty-page budget. Remove the venue accepted
rows vector, global seen/cursor sets and page counter. Native raw counts,
fresh/native identity facts, ascending/descending observations, oldest/newest
native millisecond bounds and endpoint cursor derivation remain adapter facts.
Use raw native identities for `fresh`, including filtered records; only
eligible mapped rows are staged. The raw account-ledger identity remains
booking ID plus asset/contract; derived PnL/funding/fee/liquidation rows keep
their existing IDs, cash-leg filtering and units. Filtered rows must not
authorize failure recovery. No new output cap or early stop: fills and orders
still walk their current native coverage despite a small requested limit;
trades retain their current limit stop.

Admit a page only after HTTP/native envelope, required-array, all streamed
rows, order length and cursor-envelope consistency checks succeed. Actual first
or zero-eligible-output failure rejects typed (Internal fallback); actual
later-page HTTP/transport/decode/malformed failure returns only earlier
accepted rows plus one command-owned incomplete Warn before Done, including
Error-only logging. Discard the failed page's prefix, including order updates
to already accepted native IDs. Move order terminal-state folding into the
contextual final encoder across admitted rows only: keep the latest
strictly-greater updated timestamp, first encounter on equal timestamps,
existing filters-before-fold, ignored NotFound/EditRejected/nonterminal events,
and final descending sort/truncation. Do not retain a separate mutable
accepted-order cache outside the history owner. No historical row may enter the
live account Store.

Preserve strict endpoint-policy refusals separately from recoverable actual
page failure: repeated/no-fresh cursor, fifty-page exhaustion, and a saturated
order page with no continuation proof, or contradictory decoded body/header
tokens, still reject via owner.reject even after prior rows. Do not change
their existing INCOMPLETE messages or manufacture a complete order total. Drive
repeat/empty/no-new-row/budget detection through CursorWalk settings and native
CursorPage facts; explicitly enable both stall_on_empty_page() and
stall_on_no_new_rows() so an empty page carrying a token preserves the current
strict refusal. Reject its Stalled verdict instead of adding another loop.
Validate missing/conflicting continuation proof before page admission and
reject it strictly; no request or complete result is authorized by unusable
proof. Generic whole-envelope/row decode failure and a declared order length
mismatch remain actual failed-page errors with accepted-only recovery. Preserve
the existing native parser rather than inventing token authority from a
malformed envelope. Keep body/header opaque token agreement and exact
`lastFillTime`/`lastTime` ISO millisecond overlap, ledger `since`/count and
orders since/before/count/sort/ closed/opened/tradeable request bytes. A
terminal page at request fifty is still complete; a full continuing page fifty
rejects without a request fifty-one. Synchronous consuming signed setup refusal
stays strict through its current error path, never a partial HTTP-page failure
or a reason to resend.

Replace the ledger's direct HISTORY_INCOMPLETE emitter with
advance_with_coverage on the validated terminal since-only coverage gap.
Preserve its message and current suppression when eligible accepted+staged
output already satisfies the requested limit. Preserve native ledger predicate
precedence: crossing from can prove descending coverage; crossing to proves
coverage only in the ascending, distinct-time continuation branch. A descending
page with newest above to alone still has the existing since-only gap unless
from or the eligible output limit proves sufficient coverage. A valid
all-filtered saturated page may explicitly return incomplete empty output; a
failed page with no eligible accepted output must reject instead. Completion
owns command fields, severity, exactly-once marker ordering and serialization
failure suppression. Preserve first coverage cause without a venue marker
latch.

Before production edits, commit original-source assertion-RED controls for
typed first/native/streamed failure, accepted-only later-page recovery,
failed-prefix isolation, zero-eligible recovery refusal, concurrent owners and
Error-only marker-before-Done delivery. Cover fills, ledger, closed orders and
unsigned public trades with full-page continuations where applicable. Pin an
accepted terminal order followed by a newer failed-page update to the same ID,
proving that the failed update cannot replace the prior accepted state.
Preserve and extend existing strict repeated-cursor, full same-time fill
boundary and real fifty-page/no-fifty-first tests; add terminal fiftieth page,
missing/conflicting order token, an accepted order page followed by an empty
page with a token (strict rejection, no partial marker/result), declared length
mismatch, fully filtered
native progress, requested small-limit behavior, descending-above-to ledger gap
precedence, native compound ledger identities and existing exact
large-ID/signing/public-unsigned/request-sequence controls. Retain the explicit
unsupported client-ID lookup refusal and range admission behavior unchanged.

Independent plan/correctness/adversarial reviews must explicitly answer the
architecture gate: this is adoption of the existing shared engine history
owner, not another venue completeness/paging implementation. Focused
engine/Kraken futures/Binance native tests plus touched all-target strict
Clippy/fmt/bans/diff checks use four Cargo jobs. Corepack Yarn only; never
bare/Volta yarn, npx or the account-policy check. No local release WASM/full
replay/size/timing run. After actual shared predecessors, require exact-head
green CI and fresh authenticated post-sort raw/package/ordered-gate evidence,
then announce, run a fresh separate porcelain guard and a separate exact-head
merge. Caller docs record accepted-only partial recovery and typed telemetry
without claiming B11 ingestion/coverage, managed completion, remote-signing or
deployed cutover proof closed.

Independent correctness and adversarial plan reviews are CLEAN after the
explicit strict token-proof, empty-with-token and native ledger-precedence
refinements. Existing CursorHistory/Completion APIs suffice; no production
source or tests are edited in this plan checkpoint. Both reviews confirm the
shared-engine architecture gate. Original-RED controls precede implementation.

Kraken futures regression-only checkpoint `f2389ff658b` leaves production
unchanged: nineteen controls pass (including the new strict-proof compatibility
control), and four new assertion groups fail on untyped first/filtered failures,
accepted-page loss and failed newer-order-prefix isolation. Existing engine
owner adoption makes all four groups green. Added compatibility controls pin
terminal page fifty versus continuing cap fifty/no page fifty-one, strict empty
or missing/conflicting order continuation after accepted rows, declared length
mismatch, first native/transport/decode typing, ledger descending-above-to gap
precedence and filtered-empty coverage, concurrent owners, first order state on
equal timestamp, and no-I/O strict consuming setup. Terminal helpers inspect
physical HTTP actions before filtering notices, preventing vacuous no-resend
assertions. Independent source correctness/adversarial reviews are CLEAN and
confirm existing-engine choke-point adoption; no engine API or managed completion
policy is added. Final focused/strict checks and tests/caller review precede
draft publication; actual shared dependencies and full CI/evidence remain.

## KuCoin futures and spot owner adoption (planned 2026-10-23)

Base: actual landing `90dc6637d6e12eafecc283d0dfb9a78e7fc967d6`.
Scope: `venues/kucoin/src/history.rs`, `funding.rs`, spot query-only regions in
`spot/mod.rs`, focused existing/new history tests, and appended caller/audit/
quality notes. No signing helper, credential selection, account reducer,
capability, HTTP recipe or request-context authority changes. Keep all watches;
no landing merge during the #1741 exclusive window. Actual shared predecessors
must be integrated before final CI/proof.

1. Pin numbered-page/ledger and funding request sequences, admission errors,
   retry delays/counts, 350ms pacing, exact integer offsets/nanosecond fill
   bounds, newest-first tiled windows, census/cursor proofs and final shaping.
   Spot fills remains one request, limit/defaults/output schema unchanged;
   disabled order-history/income stubs and lookup remain unchanged.
2. Regression-only first: reject Internal with no Done/marker on initial failed
   HTTP/envelope/row reads; after an accepted futures/funding page resolve only
   admitted rows with one command-owned Warn on terminal page failure. Prove
   failed-page prefixes and retries cannot leak rows/poison dedupe, concurrent
   owners remain isolated, successful empty is complete, and spot malformed/
   transport failures become typed failures without partial success.
3. Adopt existing engine `CursorHistory`/`Completion` for futures history and
   funding, retaining one owner across every native window. Native page/offset/
   timestamp tokens describe wire continuation; the shared walk owns accepted
   row identity, continuation lifecycle and missing-coverage reporting. Keep
   native census, strictly decreasing ledger/funding cursor and bounded-attempt
   refusals strict via owner.reject; those native completeness proofs are not
   recoverable page failures. Keep the existing total HTTP-attempt safety guard
   (counts retries), independent of accepted-page tracking. Stage normalized
   page rows locally until the whole response validates, then admit atomically.
   Spot fills uses `HistoryRows` for its single validated response, without
   inventing pagination or enabling unsupported endpoints.
4. Preserve final descending time/identity ordering and limit exceptions for
   explicitly ranged income/funding. Preserve newest-window allocation bounds,
   cursor inclusivity and padding, output units, filters and successful paths.
   HTTP/decode callbacks are multiplexed, so the shared Internal default is
   honest; do not classify all failures as Network. No numeric/managed fallback.
5. Independent correctness/adversarial reviews must answer the architecture
   gate: these are two family adapters to the existing engine history choke
   point, not a second marker emitter/pager. Focused engine/KuCoin (spot feature)/
   Binance tests, strict touched all-target Clippy/fmt/bans/diff locally with
   four Cargo jobs; no local release WASM/full replay/size/timing. Update caller
   impact and open a draft with CI watch, no cutover/B11/runtime closure claim.

KuCoin plan-audit clarification: malformed metadata decoding follows failed-page
owner policy; well-formed census values and missing/nondecreasing native cursors
remain strict proof refusals. Funding cursor proof still precedes the output cap,
while ledger cursor proof is checked only when continuation is actually needed.
Raw census/cursor facts include filtered/duplicate rows; output admission does not.

KuCoin local adoption checkpoint: exact final 17 controls on original source
`90dc6637d6e` produce 14 assertion RED / 3 compatibility GREEN; the implementation
makes all 17 GREEN. Original comparison saves/restores only this slice's three
source files byte-for-byte, with test/source hashes recorded locally. The full
focused engine/KuCoin (spot)/Binance native suite passes 957 tests; strict touched
all-target Clippy/fmt/bans/diff checks pass. Independent plan, correctness,
adversarial source/delta and final tests/docs reviews are CLEAN. Actual #1741
must be genuinely integrated before draft/final CI; none of these local receipts
claims new shared-core compatibility or deployed runtime coverage.

Actual #1741 integration was conflict-free and preserved all four reviewed
source/test blobs and the entire incoming unowned tree; integrated native suite
passes 1,100 tests. Its exact lexical inventory needs the owner-authorized
follow-on: prune KuCoin history 20->13 and funding 15->9 legacy sites, and spot
22->20 with the same GET call re-expressed only to carry HistoryRows. Record the
full exact spot old/new maps in existing HISTORY_BOOTSTRAPS and hash-pin the
transition in checker controls; preserve every other map and all checker logic.
The funding cursor formatting is factored through a tuple to avoid two false
macro matches on the local identifier `request`, without changing cursor bytes.
No new HTTP site, recipe, headers, signing or broad exemption is authorized.

The precise ratchet update passes all 12 checker controls and the full bans
script. Exact spot map hashes (sorted JSON) are
`cff9fea5f76ae51654a83c7953df010ef41e3941614a6fdd67b9f1001bd212ff`
(old) and `fca911a5d114175c7374dfef35111bf3498f871eead93ef398d926ccf6929da8`
(new); existing negative controls also exercise this transition against modified
baselines, foreign paths, inflated counts/new digests and newer pruning.
Final tuple-cursor source passes all 257 KuCoin/spot tests (including the same
17 owned-history controls), strict all-target Clippy/fmt and diff checks. No
production request-context adoption or signing changes are implied by pruning.

## Next single-page fan-out: Bitrue execution histories

Plan first on fresh `feat/rust-owned-history-bitrue` from actual landing
`bf362312d3edfb4dfa00b726c172f944053b9198` (2026-10-23). Plan only: keep queued
Hyperliquid #1765, Backpack #1766 and Kraken futures #1768 CI/proof watches active
and honor the owner queue, announcing before any separate guard/merge. Check
native history/request-builder overlap with signer/clock/financial lanes before
production edits. Future shared-engine/harness/build inputs require genuine
integration and fresh source-bound evidence before landing.

Bitrue fills read one bounded `/fapi/v2/myTrades` page; the venue's forward fromId
is not a documented backward recent-history cursor. Do not invent continuation
or claim general complete order history/PnL ledger. Scope existing
`venues/bitrue/src/history.rs`, its `tests/history.rs` and nearby caller/audit/
quality/this-plan docs. Reuse engine `HistoryRows<Execution>`/`Completion` rather
than constructing a fake cursor walk for a one-page endpoint. One owner binds
the real command and owns staging, admission, error finalization and encoding.
Remove the venue row-output vector and duplicate numeric completion field. No
new engine API, per-venue completeness emitter, paging/retry policy, Store,
account reducer, signing/key selection, Context, capabilities, harness/fixture/
budget/workflow or NX changes. Public WS trades, native client-ID lookup and
explicit unsupported order-history/income admission stay separate and unchanged.

Rows stream into owner staging only. Native HTTP/envelope check, required-list
streaming decoder, all row validation, hard 1000-row guard and endpoint coverage
proof must succeed before accept_page and final serialization. Keep the native
received count before filters and the exact strict refusal when a full requested
native page leaves fewer eligible unique rows than the requested limit. That
coverage refusal remains owner.reject even if a valid prefix was staged; it
never manufactures a successful empty/partial answer or a completeness marker.
Actual native/HTTP/malformed/transport failure before page admission rejects
typed with the existing shared Internal fallback, never empty complete. Since
this endpoint admits exactly one terminal page, there is no artificial
accepted-page/later-failure recovery case to add. Preserve valid empty success,
exact existing messages and any supplied typed error information.

Request::reset remains compatible with the existing library reset_job call.
Discard only unvalidated owner staging and native per-attempt received/failed/
seen-ID facts, retaining the original Completion and command across retries.
The existing per-attempt identity set is native page deduplication, not an
accepted/global row owner; clear it with the failed attempt so the same native
ID can reappear in a valid retried page. No command Done/failure/marker may
escape from a failed attempt that will retry. Preserve the one explicit -1022
signature resend (both envelope and expected-list error paths), three eligible
GET retries, existing rejection/UTF-8/no-list exclusions, retry counters and
strict timestamp increment/reset behavior. Do not copy or repair the generic
issue/retry/signing implementation in this slice; all those callsites stay as-is.

Preserve byte-exact URL/contractName/limit/startTime/endTime and flooring/clamps,
primary versus read credentials, native huge IDs and same-side bid/ask order ID,
first eligible duplicate identity, side/type filtering, exact millisecond range
checks before second timestamps, base/contract/quote quantity disambiguation,
absolute commission/quote asset, maker optionality, final descending sort/limit
and no live Store writes. The contextual finalizer sorts/truncates only the
admitted page; serialization error rejects through the same command owner.
Numeric Completion remains explicit legacy authority, not managed Context
completion; notify the request-core lane of any eventual exact legacy-inventory
net shrink after actual landing.

Before production edits, commit original-source assertion-RED controls for typed
terminal native/malformed/exhausted-transport failures and the strict full-page
coverage refusal. Preserve existing success, lookup/public/unsupported, time/
identity/quantity, signature-resend and interrupted-stream compatibility tests.
Add valid-prefix plus malformed-row/envelope/UTF-8/no-list refusal, failed-prefix
reset followed by successful same-ID changed-economics retry, mixed signature
and transport retry accounting, filtered-full/all-duplicate pages, received-row
1000/1001 boundary, concurrent query owners and exactly one terminal delivery.
Inspect raw actions for no early Done/failure/marker or unexpected extra HTTP;
no history output may mutate live orders/fills. Pin the distinction between
legitimate terminal empty data and missing/invalid arrays. Avoid an invented
partial-success regression for a query with no accepted earlier page.

Independent plan, correctness and adversarial reviews must explicitly answer
the architecture gate: adoption of the existing engine row-admission/completion
owner, not another venue completion/paging/coverage implementation. Focused
engine/Bitrue/Binance tests and touched all-target strict Clippy/fmt/bans/diff
checks use four Cargo jobs; Corepack Yarn only, never bare/Volta yarn, npx or
the account-policy check. No local release-WASM/full replay/size/timing sweep.
Caller docs explain Internal fallback on formerly unclassified owner-completed
failures and strict coverage refusals while preserving admission/lookup/setup
paths; B11 ingestion/coverage, other adapters and deployed runtime proof stay
open. Final exact-head green CI and fresh post-sort raw/package/ordered-gate
evidence precede an announced fresh separate guard and separate exact-head merge.

Bitrue implementation checkpoint: regression-only `51b6193c21e` leaves production
unchanged, with 19 controls PASS and four typed-terminal assertion groups RED.
Existing HistoryRows/Completion adoption makes those groups GREEN. All 28 final
history controls pass, including mixed resend/GET orders with strict timestamp
increments, no-list signature refusal after exhausted GET retries, signature
resend not replenishing GET budget, staged same-ID economic replacement,
1000/1001 raw-row admission, valid empty success versus malformed list,
UTF-8/no-list nonretry, independent concurrent owners and first eligible native
identity. No new core, library request/retry, public WS or lookup edits.
Independent correctness source review is CLEAN; final adversarial/tests/docs
review and focused native/strict checks remain before the draft PR. Current
shared registry/SDK predecessor #1763 must be genuinely integrated before final
CI/proof. No historical evidence is a certificate for that new head.

Bitrue final local checks: all 842 focused engine/Bitrue/Binance tests pass;
all 28 final history controls pass again after tightening the mixed-retry raw
terminal drain. Touched all-target strict Clippy, fmt, bans and diff checks pass.
Independent correctness, adversarial source and final tests/docs reviews are
CLEAN; architecture gate is existing shared owner adoption. No local release
WASM/full replay/size/timing run. Actual #1763 integration and fresh CI/proof
remain required before landing.

## Next offset-history fan-out: Backpack

Plan first on fresh `feat/rust-owned-history-backpack` from actual landing
`4a17998a4d9825dc7de303a23be793b5a7d9cabe` (2026-10-23). Scope is Backpack
fills, closed orders, funding income and single-page public trades. Avoid active
OKX/Bitget/BingX signing work, KuCoin's signer hold and Orderly financial reducers.
Keep the #1760/#1761/#1765 CI watches and respect #1764's exclusive landing window.
This plan adds no new plan file or index row.

The adapter currently owns accepted output in `Request.rows`, emits numeric
completion, and rejects all accepted pages on a later HTTP/stream failure or
repeated/capped offset walk. Adopt a boxed engine `CursorHistory<Row>` with one
`Completion` for output staging/admission, fifty-page budget, progress checks and
terminal coverage. No new history-owner type, engine API, managed Context bridge,
retry loop or venue-local completeness emitter is planned. Touch only Backpack
`src/history.rs`, the narrow `Pending::History` failure arm in `src/lib.rs`,
`tests/history.rs`, and the existing caller/AUDITS/QUALITY/OWNED_HISTORY sections.
Do not change `account.rs`, `sign.rs`, request signing, credentials or Store.

Use the existing raw-page fingerprint as the opaque progress token while the
native page requires continuation; do not use a monotonically increasing offset
as evidence that a repeated page progressed. Remove the venue's separate page
fingerprint set: `CursorWalk` owns repeat detection and the page cap. Preserve
the existing engine `OffsetWalk` solely for native next-offset arithmetic, invoked
only after `CursorHistory` admits a valid page and returns More. It cannot
authorize another request or a terminal outcome; the shared owner makes those
decisions first. Keep exact raw-row displacement rather than selected output
count or requested page size. Review this narrow protocol-helper composition
before implementation; do not introduce a second active pagination authority or
copy offset arithmetic/stall policy into the venue.

Keep native row decoding, compound fill/order/funding identities, local symbol/
order/time filters, oldest-time facts, page sizes, native order and output limits.
The More predicate uses admitted plus staged retained output, preserving current
limit behavior without admitting a page prefix. Decode into staged rows; accept
only after HTTP/envelope/stream validation. Actual failure before eligible output
was accepted rejects typed; after accepted output, finish with only earlier
accepted pages and one command-owned incomplete Warn before Done, visible at
Error-only logging. A later failure after only filtered pages still rejects.
Repeated/capped valid coverage becomes owned incompleteness, including an empty
result when valid pages contained no eligible output. No result row enters Store.

Preserve malformed-fill isolation and the existing row.skipped diagnostic;
malformed non-fill rows still invalidate their whole page. A validated page with
skipped fills supplies a coverage gap to `advance_with_coverage`, rather than
claiming complete coverage. Stage that gap and skipped-row diagnostic facts with
the page: failed-page prefixes must not contribute output or an earlier coverage
cause. Retain only the first admitted gap across subsequent pages. Public trades
remain unsigned and single-page. Client-ID lookup, unsupported ledger/time/market
admission, and synchronous signed-setup refusal remain strict and separate;
preserve the existing consuming signed helper rather than adding a generic
credential fallback or misclassifying setup refusal as recoverable HTTP loss.

Before production edits, commit original-source assertion controls for typed
first failure, retained later failure, streamed failed-prefix isolation, empty
filtered-prefix failure, owned repeat/cap coverage and skipped-fill coverage.
Pin two concurrent owners, exactly one marker before Done, Error-only delivery,
49 full pages plus terminal probe versus a full fiftieth page with no fifty-first
request, and non-adjacent repeated raw pages. Preserve the existing exact offset
request-sequence control, fractional fill bounds, spot-row filtering, native
units/IDs/quantity fallback, row-skipped diagnostic and unsigned public trades.
Document the intentional replacement of the old repeat-page strict rejection.

Independent plan/correctness/adversarial reviews must explicitly answer the
architecture gate: shared owner adoption with an existing engine offset codec,
not a per-venue cross-cutting pagination/completion fix. Run focused Backpack,
engine and Binance native tests, touched all-target Clippy/fmt/bans/diff checks
with four Cargo jobs. Corepack Yarn only; never bare/Volta yarn, npx or the
account-policy check. No local release WASM/full replay/size/timing sweep. Before
landing, integrate actual shared predecessors, obtain exact-head green CI and
authenticated post-#1762 full raw/package/gate proof, announce the guard window,
then run a fresh separate porcelain guard and a separate match-head merge. B11
NX ingestion/coverage and deployed cutover proof remain open.

Backpack plan review refinement: owner More can occur at most forty-nine times
before the fifty-page cap, so the existing offset helper cannot reach its own
budget. Treat any unexpected non-More helper result as a strict invariant
failure; never let it independently finish or authorize a request. Insert page
fingerprints only when continuation is required, matching the old repeat set,
and do not stall on zero eligible identities. Preserve the current validation/
filter order and eager non-fill command_error diagnostic. Keep separate staged
skipped-fill facts and the admitted cumulative row.skipped message, emitted once
on successful completion, including retained partial success. Add controls for
a failed skipped prefix, an admitted skip followed by actual failure, all-skipped
valid coverage, malformed non-fill after an accepted page, public dedupe/limit,
and raw offset displacement across spot and malformed-fill neighbors. Strict
synchronous setup failure must suppress both partial output and owned markers.

Backpack regression-only checkpoint `b33295f3591` leaves production unchanged:
all twelve existing history controls pass and seven new assertion groups fail on
the original implementation. Shared owner adoption makes those groups pass. The
old repeated-full-page strict assertion is explicitly superseded with an owned
marker/empty-result assertion, preserving its no-silent-truncation purpose.
Additional controls pin failed-page skip isolation, admitted first-gap retention,
eager non-fill CommandError with prior accepted rows, raw displacement across
spot/skipped neighbors, unchanged unsigned public dedupe/limit and strict missing-
credential setup. Static credentials cannot become invalid midway through a
healthy native fixture without introducing a test mutation seam; the unchanged
consuming helper's error arm remains strict after any earlier accepted pages.
Independent source correctness/adversarial reviews are CLEAN: no engine API,
account/signing, Context, Store or parallel pagination-policy edits. Final focused
checks and final tests/caller review precede draft publication.

Backpack final local checks: all 814 focused engine/Backpack/Binance tests pass,
including 24 Backpack history controls. Touched all-target strict Clippy, fmt,
banned-pattern checks and diff checks pass. Both final source and tests/docs
reviews are CLEAN and confirm the shared-owner architecture gate. CI/full
artifact proof and actual queued dependency integration remain mandatory; no
local release-WASM, full replay, timing, deployed NX or cutover proof is claimed.

## Next bounded-read fan-out: ApeX Omni executions

Plan first from actual landing `d05c5ede4b86409e1a8d9798e64b79b1c1c888f4`
on `feat/rust-owned-history-apexomni`. Hyperliquid Spot and Breakout have only
empty/unsupported history stubs, not a request pager to migrate. Keep all queued
history CI/archive/proof watches active and honor the bounded #1769 merge hold.
Scope ApeX `src/history.rs`, `tests/history.rs` and existing caller/audit/quality/
this-plan docs. No signing/request-library/account/Store/engine source change.

ApeX executions try native/display symbols on `/v3/fills`, then the same bounded
alternatives on `/v3/history-orders`. They do not implement a cursor walk. Use
existing engine HistoryRows/Completion, bound once to the original command, for
terminal admission/encoding/error delivery, without inventing a pager. Remove
the duplicate command field. Preserve the native per-attempt bounded Entry
buffer: its replacement-by-key, stable raw-time sort and top-limit eviction are
endpoint normalization facts (including last-duplicate economics and later
reappearance after eviction), not accepted query output or a second finalizer.
Only the selected validated terminal result enters the engine owner; no failed
attempt prefix or background cache can independently authorize error recovery.

Owner decision (2026-10-23): when every REST alias/route actually fails, reject
typed Internal even if the background WS cache contains matching executions.
That cache was not fetched by this command and must not be presented as complete
history. Keep the same alias/fallback attempts and bounded upsert. On validated
successful empty/nonempty paths, preserve the existing cache/authoritative
selection, including nonempty primary data stopping further REST attempts even if every
row filters out. Existing finish_history still selects matching WS cache when
that validated nonempty response has no eligible normalized rows; preserve that
distinction. Only eligible nonempty REST output excludes cache merging. Callers may still read the live cache separately after rejection.
Track whether any actual REST response validated, across candidate resets, so
mixed valid-empty/failure alternatives retain the expressly preserved successful
path while all-actual-failure exhaustion rejects. Initial consuming signed() refusal remains the existing separate strict
String-to-cx.fail path, without claiming unavailable typed details. The wrapper
already flattens retry/resend preparation failures to error.message before
history_failed; preserve that alias-exploration behavior and leave typed
preparation provenance to the shared request/signing lane. Preserve admission's
empty/unsupported behavior. Numeric Completion remains explicit legacy authority,
never managed Context fallback. No first-page actual failure may become complete
empty/cache output when all alternatives failed; no invented later-page partial
or history.incomplete marker is needed for this bounded selection.

The wrapper already validates HTTP/status/native error metadata and supports
both signature formats and eligible transport retries. Preserve its code and
policy exactly. Request::reset clears only native attempt count/current rows
(and any engine pending rows defensively), retaining owner, candidate/fallback
and the successful-response fact. Moving between aliases/routes likewise keeps
one owner. At final success retain existing cache filter/identity/sort/limit
selection, stage its normalized executions, accept exactly once and finish with
the existing JSON shape. At all-failure exhaustion finalize Err through the
engine owner, preserving the last terminal message and adding shared Internal
classification where the history adapter previously returned success.

Preserve exact signed query bytes, symbol alternative ordering, no synthetic
page/token, clamped limit1..100, floored inclusive/exclusive time bounds, symbol/
order/millisecond filters, native and anonymous compound identity, original raw
milliseconds and timestamp seconds, quantity/price/fee optionality, authoritative
REST never merging WS rows, equal-time ordering and native cumulative history-
order fallback. No new capability, native history implementation for unsupported
axes, remote signing policy, live state mutation or NX edit.

Before production edits commit original-RED controls superseding the old
all-failed-REST cache-success assertion: all-native failures with and without
cache reject typed, exhausted transport/decode/missing-list alternatives reject,
and failed streamed prefixes never enter successful or error output. Pin exact
four candidate requests when native/display symbols differ (two when identical) and no early terminal/marker/extra HTTP. Compatibility
controls cover valid empty aliases and cache selection, mixed valid/failed
alternatives, first nonempty all-filtered response with cache and no further REST attempts,
bounded duplicate
replacement/eviction, signature reset and transport prefix rollback, concurrent
query owners, large identities/raw-millisecond sorting and Store isolation.
The cache remains independently available in the live account path after an
all-failure history rejection.

Independent plan/source/final tests-docs reviews answer the architecture gate:
existing shared owner adoption, with native bounded alternative/upsert facts
retained and no copied pager/completion policy. Run focused engine/ApeX/Binance
native tests, touched strict all-target Clippy/fmt/bans/diff with four Cargo jobs;
Corepack Yarn only, never bare/Volta yarn, npx or account-policy. No local release
WASM/full replay/size/timing sweep. Open a draft after reviews, update the caller
contract for all-failure rejection/cache access, and require actual queued shared
inputs plus exact-head green CI and fresh raw/package/ordered-gate proof before
an announced separate guard and separate merge. B11 and runtime coverage stay open.

ApeX local implementation receipt: refined plan `bc700e3ccf6` was independently
reviewed CLEAN twice. Regression-only `3a023040ec3` leaves production unchanged:
18 controls PASS and four assertion groups RED. Existing HistoryRows adoption
turns those groups GREEN. All 25 final history controls pass, including mixed
valid/failed alternatives, filtered nonempty primary cache selection, two-route
same-symbol alternatives, native bounded eviction-before-duplicate replacement,
signature-prefix reset, sixteen failed transport prefixes, concurrent owners and
cache availability after rejection. Independent source review is CLEAN; broader
focused/strict checks and final adversarial/tests-docs review follow before the
draft PR. The #1769 bounded hold and required actual dependency integration remain.

ApeX final local checks: all 849 focused engine/ApeX/Binance tests PASS. All 25
history controls PASS again after adding an evicted native identity's later
reappearance case. Strict all-target Clippy/fmt, banned-pattern and diff checks
PASS. Both independent source reviews and final adversarial/tests/caller-doc
reviews are CLEAN, including the unchanged native bounded upsert and legacy
signing-provenance seam. No local release WASM/full replay/size/timing run. Draft
CI and actual queued shared dependencies still require fresh source-bound proof.

## TxFlow indexed shared history owner (2026-10-23)

Plan-first from actual #1784 `ab1ab29ec3d9babb684b4e9b8c35d7abcd538577`.
The row-reporting owner confirmed history.rs/history tests clear; its #1785
changes account.rs/account tests only. Scope is those two history files, exact
legacy-site pruning, and OWNED_HISTORY/AUDITS/QUALITY/CALLERS additions. No lib,
account, signing, native recipe, engine, host, SDK, workflow or dependency edits.

Adopt one existing CursorHistory<Row> for native identity observations, staged
rows, admission, accepted output and completion. Replace OffsetWalk with the
same fixed native offset fact (0,2000,...,200000) expressed as opaque CursorWalk
tokens and a 101-page cap per native stage. Keep stall-on-empty and no-new-row
checks; a terminal empty page and terminal offset200000 complete, while a required
next offset or repeated/no-progress page stays a strict rejection. Do not stop
native pagination when the caller's output limit is reached: the existing query
selects the newest sorted result after all native pages. No new pager policy/API.

Keep one owner across fill-derived income, funding and ledger stages; restart
only a completed stage, retaining native identities and accepted output while
resetting offset/page history. Stage only normalized eligible output; native
freshness still counts every new validated native identity before output filters.
Validate HTTP/native envelope, required page and every row before page admission.
An actual HTTP/transport/decode/row failure before accepted output rejects with
Internal; after accepted output it returns only accepted prior pages/stages plus
one command-scoped Warn incomplete marker. Failed-page prefixes and filtered-only
pages never authorize recovery. Keep native stalled/offset safety and lost
account-identity refusals strict. Pre-request unsupported-query/testnet/identity,
range/limit/ambiguous-symbol and empty selected-ledger admission stay unchanged.
Numeric Completion is explicit legacy authority, not managed Context fallback.

Preserve exact info request kind/accountType/coin/time/deltaTypes/offset/limit
encoding, testnet scope, archived/delisted/relisted symbol resolution, raw native
identity namespaces, financial signs/units and millisecond-to-seconds rules,
complete income stage selection, time and numeric-fill-ID tie sorts, global final
truncation, and zero history Store writes. No REST public trades or lookup support
is invented. Polymarket also has unsupported historical query axes rather than a
native history implementation; leave its book/account methods outside this slice.

Commit original-RED controls first for typed zero/filtered-only failure, later
page-prefix rollback and mixed-income stage failure. Compatibility cases pin the
2000-step offset sequence, no-new/empty strictness, terminal offset200000 versus
required202000 refusal, stage reset with identities retained, full pagination
before final top-limit sorting, native mappings and independent owner markers.
Preserve all existing native contracts. Self-audit native builder/mapping and all map hashes, then run focused
engine/TxFlow tests, strict Clippy/fmt/bans/checker (four Cargo jobs), independent
review and a draft with same-PR caller behavior. Corepack Yarn only, never bare/
Volta yarn, npx or account-policy; no local release WASM/full replay timing sweep.
Respect #1792 then #1789 priority and integrate actual shared/build inputs before
final CI/proof and an announced separate guard followed by separate merge.


TxFlow local receipt: plan `f05f7057b8c` precedes regression-only `5c4ed45188a`.
Original ten native controls pass; the four new original-source controls produce
three assertion RED groups and one cap compatibility pass. Corrected seventeen
history controls and 710 focused engine/TxFlow native tests pass. Strict
Clippy/fmt/bans and thirteen request-context checker controls pass. Separate
source/full-map and adversarial/test/caller self-audits are CLEAN. Native
admission and mapping are unchanged apart from owner dedupe, and the info recipe
is unchanged apart from the native offset accessor. Exact pure legacy prune is
eleven to six, every other map/checker unchanged. Independent exact-head review,
current CI and final shared-input artifact proof are tracked in the draft PR.


## Orderly shared history owner (2026-10-23)

Plan-first from actual `fa13c9030ed51e6e0ee0ce0bfaf898cfea55f0a5`.
Scope: Orderly history.rs and existing history integration tests, the exact
legacy-site inventory if sites disappear, and OWNED_HISTORY/AUDITS/QUALITY/CALLERS
appendices. No signing, account, lib, request recipe, shared engine, host or SDK
source changes. The Orderly account/fill owner confirmed these regions are clear.

Replace Request's output rows, output deduplication, global page counter and
numeric finalization with one boxed engine CursorHistory<Row>. One continuous
CursorWalk(MAX_PAGES=50), initially regular:1, uses native-stage-qualified page
tokens, including algo:1 for the second orders endpoint. Do not restart its walk:
regular and algo share the existing total fifty-page budget. Retain native
algo/page encoding, per-stage result limit, metadata census and page counts as
venue facts. Every fully validated page is admitted once; only eligible rows
stage. Final normalization sorts descending native time then ID and truncates
once, preserving millisecond order times and second execution/income/trade times.

An actual HTTP, transport, decode, missing-list or row-validation failure before
accepted output rejects with the shared Internal default. After accepted output
it returns only accepted pages and one command-scoped Warn incomplete marker;
failed-page prefixes stay staged and never leak. Regular orders survive an algo
read failure. Filtered native rows cannot authorize partial recovery. Native
metadata inconsistencies, impossible/no-progress continuation and the shared
page-budget stall stay strict rejects even with accepted rows, retaining their
messages. Signing/setup refusal also stays strict, now through owner rejection
with Internal classification; no managed Context or secret-signing bridge.
Admission checks before constructing the owner retain existing behavior.

Preserve exact paths/query/signature bytes, canonical uint64 IDs, nested algo
classification and depth/node bounds, regular/algo identity spaces, settled-only
funding identities, funding account-wide 3001ms start spacing and delayed timers,
public trade occurrence keys, per-stage limit and final sorting. No cache/Store
publication or new capability. Numeric Completion is explicitly legacy authority.

Commit failing-first controls on original production: typed first failure,
accepted-page HTTP loss, malformed continuation prefix rollback, regular-to-algo
failure and filtered-page refusal. Add compatibility controls for total budget
across stages, terminal fiftieth page, native metadata/no-progress strictness,
concurrent command marker attribution, empty success and funding timing. Existing
whole-read continuation/algo-failure assertions are superseded explicitly.

Plan audit: all imports and shared methods exist on this base; continuous cursor
stages avoid a budget reset, native census is independent of filtered output,
and owner reject suppresses an incomplete marker on strict refusals. No shared
API extension or source conflict is needed. Run original 17 controls first, then
regression REDs, implementation GREENs, full Orderly/engine focused native tests,
strict Clippy/fmt/bans and request-context inventory checks with four Cargo jobs.
Corepack Yarn only; never bare/Volta yarn, npx or account-policy. No local release
WASM/full replay/size/timing sweep. Self-audit source and adversarial cases before
draft publication. Caller-visible changes carry their CALLERS section in this
PR, with behavior wording only. #1783 and #1784 are final-proof dependencies;
current queue and separate announced guard/merge rules remain in force.


Orderly local implementation receipt: plan `0615cec06d9` precedes regression-only
`b0f7a0fe62d`. Original production passes all seventeen existing controls; the
new five controls give three assertion failures and two compatibility passes.
The existing shared owner turns those three groups green. Eight owned-history
controls now cover first/later HTTP, transport, malformed and row failures, strict
proof refusals, stage failure/identity/limits, aggregate budget, concurrent owner
attribution and signing refusal. Together with the existing cases all twenty-five
history controls pass. Two former whole-read-failure assertions now explicitly
require the accepted-only partial and warning contract.

The source audit compares admission, funding scheduling, native request/signing
builder and row mapping blocks byte-for-byte with the original base: all equal.
The separate adversarial audit checks failed-page exclusion, filtered-only
refusal, native proof strictness, no page-budget reset, terminal page fifty,
independent concurrent commands, native stage identity and signing refusal.
The exact inventory is a pure thirteen-to-nine prune, every other map and the
checker remain identical. Strict Clippy/fmt, bans and inventory controls pass.
Final draft evidence and later dependency integrations belong in the PR body;
no local release WASM or replay performance run is used.


## Known non-pageable inherited history boundaries

At actual `9a48b645d49c9fa7b940a5cf361a38242cae6b41`, Kraken Spot,
BreakoutProp and Hyperliquid Spot have no native pageable history implementation.
Their fills/orders/income paths inherit empty arrays; lookup returns an existing
non-definitive miss (Hyperliquid can first find a stored live order), and public
trades reject as unsupported/unimplemented. These empty legacy results do not
prove complete native history. There is no native pager to adopt and this
fan-out adds no REST history capability or invented completeness marker.
Kraken Spot's current inherited-history control passes unchanged. The owner
explicitly directed documenting and skipping these stubs.

## MEXC shared history owner (2026-10-23)

Plan-first from actual `9a48b645d49c9fa7b940a5cf361a38242cae6b41`.
Scope: MEXC history.rs and its existing history integration tests, pure legacy
inventory pruning, and OWNED_HISTORY/AUDITS/QUALITY/CALLERS additions. No account,
lib, signing, native recipe, shared engine, host, SDK, workflow or dependency
source changes. The account reporter owner confirmed these history regions clear.

Replace Request's numeric completion, result vector and output identity set with
one boxed CursorHistory<Row>. Native page numbers become opaque continuation
tokens for CursorWalk(MAX_PAGES=1000). Keep the shared paging::windows-generated
newest-first 90-day windows and optional native time parameters; a fully admitted
window end advances Done, then restart_stage keeps the same owner/accepted rows/
identities and resets the page budget exactly as the current native per-window
cap does. No second pager/completion implementation, global cap change, sorting,
range overlap/gap change or new engine API.

Stage eligible rows only and admit them after HTTP/native envelope, row validity
and funding metadata validation. Limits count accepted plus staged output.
Actual HTTP/transport/decode/malformed-row/funding-page failures reject Internal
before eligible accepted output; later failures return accepted prior pages with
one owned command warning and discard the failed prefix. Filtered native rows
cannot authorize recovery. Pagination budget and impossible owner lifecycle
remain strict even after output. Existing unsupported-ledger, identity, market,
range, window-count and limit admission errors stay unchanged. Numeric
Completion stays explicitly legacy, never managed Context authority.

Preserve exact request/signing builder, page-size selection, funding resultList/
currentPage/totalPage rules, order-ID single-request fills, public-trade single
request and ordinal identities, units/contract-size conversions, timestamps,
order mapping, stable dedupe, encounter order, global output limit and Store
isolation. Native repeat/no-progress behavior is unchanged (no added stall-on-
empty/no-new flags). A terminal page 1000 succeeds; one requiring 1001 rejects.

Original controls run first; commit regression-only controls before production:
typed first-failure and filtered-only refusal, accepted-page HTTP/transport loss,
malformed continuation-prefix exclusion, funding continuation failure, older
window failure, identity retention across windows and exact terminal/budget
behavior. Explicitly supersede the existing whole-read continuation rejection.
Compatibility controls retain exact window requests, native formats, limit/
single-page behavior, empty success and concurrent command attribution.

Plan self-audit: all CursorHistory methods exist on this actual base; restart
after a completed window retains output identities but resets native per-window
budget; accepted+staged limits preserve prefix selection; error finish drops
pending rows. Native builder/mapping blocks can be byte-compared independently.
Run focused engine/MEXC native tests, strict all-target Clippy/fmt/bans and full
request-context map/checker controls (four Cargo jobs). Corepack Yarn only; never
bare/Volta yarn, npx or account-policy. No local release WASM/full replay/timing
sweeps. Source and adversarial/caller review precede a draft, caller changes stay
in the same PR, actual #1784 build input is a final-proof prerequisite, and the
existing landing queue/announced separate guard and merge rules remain.


MEXC local implementation receipt: plan `3a6257c19df` precedes regression-only
`94bceed7a93`. Original ten controls pass; the five added original-source tests
produce four assertion RED groups and one page-cap compatibility pass. Corrected
source passes all eighteen history controls, including the additional window
identity/order, concurrent marker and single-request/global-limit cases. The
focused engine/MEXC run passes 746 native tests. Strict all-target Clippy/fmt,
bans and the request-context checker plus thirteen checker controls pass.

Source/native-preservation and separate adversarial/test/caller self-audits are
CLEAN: admission/window tiling, entire request/header builder and native mapping
blocks are byte-identical to the requested actual base. Full-map comparison
proves a pure nine-to-six MEXC history prune, every other map/checker unchanged.
This is local evidence, with independent exact-head review and source-bound
current CI/final artifact proof tracked in the draft PR. No new native capability
is implied for the documented Spot/Breakout stubs.

## Phemex bounded non-income history owner (2026-10-23)

Plan-first from actual #1784 `ab1ab29ec3d9babb684b4e9b8c35d7abcd538577`.
Scope: phemex/src/history.rs and existing history integration tests, exact legacy
inventory pruning if sites disappear, and OWNED_HISTORY/AUDITS/QUALITY/CALLERS
appendices. Reporter #1786 owns account.rs/account controls and confirmed these
history files clear. No account, lib, signing, income, request recipe, shared
engine, SDK, host, workflow or dependency source changes. Polymarket's historical
query axes are unsupported; its book/account code is not an implemented history
pager to migrate and remains outside this slice.

Non-income Phemex histories are bounded one-response reads: private fills/orders
request offset0 and clamp limit1..200; public trades retain the native snapshot
and 1000-row defensive cap. Adopt existing HistoryRows<T>/Completion for the
three typed result variants. Keep lookup's distinct native Vec/legacy finalizer
and its is_lookup routing/primary credential selection unchanged. Income
continues delegating to the already-shared RangeHistory owner unchanged.

Eligible decoded rows stage until the entire HTTP/body/required-list/native row
validation succeeds. Only then accept one page and finish through the owner,
with the original stable time sort/truncate and exact JSON shape. HTTP, transport,
decode, malformed/oversized response and native request preparation failure
reject Internal rather than the previous unclassified error, with the same
message and no added write outcome. No partial result or incomplete marker is
possible: this bounded adapter has no accepted earlier page. A failed streamed
prefix cannot escape. No new pager, REST capability, request or synthetic gap.

Preserve all pre-owner admission errors, unknown-market nondefinitive lookup,
lookup exact client ID/version/one-row validation and definitive-absence rules,
16-active-command admission/duplicate refusal, shared rate-pause deferral/timers,
request/header/signature bytes and identity slots, textual enum/stop/trailing
mapping, exact nanosecond conversion, time/order-ID/symbol filtering, native
identity dedupe, equal public-tuple occurrence IDs, fee and quantity units, zero
Store publication, private/public-only gating and cleanup/re-admission after
success/error. Keep numeric Completion explicitly legacy, never managed fallback.

Commit original-RED controls before source edits: typed first HTTP/transport/
malformed and oversized failures on fills/orders/public trades, valid streamed
prefix followed by malformed/non-2xx failure with no output/marker, and slot
cleanup/re-admission. Compatibility controls pin valid empty one-shot completion,
stable equal-time ordering, exact limits/no continuation, concurrent command
isolation and unchanged lookup/income behavior. Existing controls run first.

Plan self-audit: the shared row owner has stage/accept/finish_with methods and
supports native generic encoding. Request retains numeric command solely for
active-set bookkeeping and lookup's existing legacy completion, while each
history variant owns its single finalizer. Native builder and codecs should be
byte-identical except stage instead of Vec push; no shared API extension needed.
Run focused engine/Phemex tests, strict all-target Clippy/fmt/bans/checker (four
Cargo jobs), source and adversarial/caller reviews, then a draft. Corepack Yarn
only, never bare/Volta yarn, npx or account-policy. No local release WASM/full
replay/timing sweep. Respect #1792 then #1789 priority and real-integrate actual
shared/build inputs before final CI/proof/announced separate guard and merge.

Phemex checkpoint: original 28 controls PASS. Regression-only commit
`c3c98778f79` adds four assertion-RED groups (missing Internal classification)
and three compatibility GREEN controls. The wrapper intercepts non-2xx before
the history response handler; the HTTP-message fixture was corrected to its
existing `HTTP 503: unavailable` text, without changing production behavior.
A fresh original-source rerun with that corrected fixture still reports exactly
31 PASS/4 expected assertion failures. Implementation uses existing boxed
HistoryRows variants, native seen/count/filter facts and a shared native shaping
helper; lookup keeps its original numeric finalizers. Whole response validation
precedes the only accept_page. Failed prefixes have no admitted earlier page.

The request-context inventory change is a single exact prune: Phemex history
17->16, removing only the direct malformed/oversized cx.fail site now delegated
to history_failed. Lookup still uses the other finalizer sites, so they remain
in inventory. No added fingerprints, bootstrap, checker change or other map
edit. No account/lib/signing/income/shared-engine production change.

Corrected local validation: 770 engine/Phemex native tests PASS, including
all 35 history controls. Strict all-target/all-feature Clippy, formatting, bans,
request-context checker plus all 13 checker controls and diff checks PASS.
Source self-audit confirms the native builder, income, lookup codec, filtering,
units and row mapping are unchanged; only the three normal row sinks stage into
the shared owner and final stable shaping moves behind page admission. No local
release WASM or full replay run. Independent source/caller/full-map review and
fresh CI remain draft/final-proof gates; actual #1792/#1789 inputs are required
before landing.

## WOO unified owned-history fan-out (2026-10-23)

Plan-first from current actual landing #1792
`8e53b1bd39e0fd03534ffa4453349390251e4585` (including actual WOO build
restore #1784). Scope: woo/src/history.rs, existing tests/history.rs, exact
legacy inventory pruning and OWNED_HISTORY/AUDITS/QUALITY/CALLERS appendices.
#1788's author confirmed its WOO files are account.rs, trading.rs and
 tests/account.rs only (plus shared append-only docs); there is no source/test
or lib/request-builder overlap. Account snapshot tokens and trading invalidation
stay untouched. No signing/helper, lib, engine, SDK/host, dependency, workflow,
public socket or financial account changes. AsterDEX/Polymarket histories wait
for #1785/#1786; no guard/merge before actual #1789 SafeIDs.

Cover all implemented query axes: fills (v3 range and unbounded reads, symbol
alternatives and paged v1 fallback), closed orders, income, public trades.
Unsupported client-ID lookup and admission refusals remain unchanged. One
existing CursorHistory<Entry>/Completion owns output, validated-page admission,
physical-attempt paging budget and terminal delivery for the whole command.
It replaces the venue's accepted rows/seen/terminal/page-cap policy; keep only
native current-page rows/facts until full validation. Existing RangeWalk remains
the subordinate engine geometry selector for v3 fills, with its exact span,
direction/duplicate/skipped-row facts and range-incomplete refusal. It does not
own accepted output or completion. There is no new engine API or second owner.

Use CursorWalk's existing global 1000-page budget for the command, counting
completed native alternatives/failed attempts as well as successful pages.
Advance an empty validated transition for each alternative that actually yields
to another attempt; do not stage a failed page. Delayed income timers resume an
already-selected continuation without another advance. Tokens identify native
range selection (RangeWalk.requests), endpoint, symbol candidate and page, so
legitimate repeated inclusive ranges remain possible under the native range
walk. Terminal response 1000 succeeds without attempting 1001; More/Stalled at
that boundary is a strict owner.reject with the existing pagination message.
The native numeric page remains request vocabulary, subordinate to the owner.

Keep one owner across symbol alternatives, v3->v1 fallback, v1 pages and later
v3 windows. Preserve the exact five candidate spellings/account-wide omission,
per-range reset, last-fill-error tracking and all-empty versus failed-alternative
semantics. Whole-page HTTP/envelope/list validation precedes admission. The
native current-page vector supplies range facts and lets invalid/failed bodies
be discarded without poisoning global dedupe or returning a prefix. Native
selected/filtered eligible rows enter shared staging, then advance/accept once.
Filtering never authorizes recovery; a validated page with no retained rows
cannot turn a later failure into an empty successful partial.

Terminal HTTP/decode/transport/malformed-page failure rejects Internal when no
eligible rows were accepted; otherwise it returns only accepted rows plus the
one command-scoped incomplete warning emitted by the shared owner. Keep fallback
attempts and error messages, including sticky errors after successful-empty
alternatives. Never use live Store/cache rows as history. Entirely malformed
pages remain failures (without introducing extra fallback attempts). Invalid
native page metadata, complete-income row budget, no-progress native refusal,
1000-attempt cap and native range truncation remain strict owner.reject even
with earlier rows. Consuming signed() setup errors keep their existing strict
legacy cx.fail path and unclassified provenance; do not reconstruct a finalizer
after the helper consumed Pending. No raw-secret managed bridge or Context claim.

Preserve native mixed-page row isolation: retain immediate per-page ROW_SKIPPED
Warn text/timing, but a validated mixed page also records an owned coverage gap
via advance_with_coverage. It therefore cannot look like complete history merely
because later pages succeeded. Keep the first gap and emit one owner marker at
terminal delivery; failure before any accepted rows still rejects without a
marker. Entirely malformed pages never admit rows. CALLERS will explicitly state
these typed-first-failure, accepted-only-later-failure and skipped-row coverage
changes, the strict native refusals, and the bounded public trade behavior; write
only behavior, no merge/proof status. Caller handling of unavailable history and
command-scoped coverage is required; no NX adoption/closure is implied.

Keep all native routes/query order/header/signature bytes and credential slot
selection, ten ledger requests/minute shared 6000-ms gate across commands,
size/limit defaults, 100000 complete-income guard, time and symbol normalization,
ID fallback/preference and high integer/string precision, side/position/maker and
optional-field mapping, JSON shape, final descending time/id sort/truncate,
public identical-print identity behavior, dedupe/filter order and zero Store
publication. Keep native RANGE NaN placeholders for skipped rows so a full mixed
v3 page cannot be mistaken for a terminal short page. The public endpoint stays
one response; no manufactured continuation or additional REST capability.

Original-RED controls before source edits: typed first failures after all fill
alternatives; later order/income/ranged/legacy fill failure retaining only accepted
rows; failed streamed prefix not leaking; mixed skipped-row owned coverage at
success; and filtered-only pages never authorizing an empty partial. Pin native
strict metadata/no-progress/budget and all-malformed rejection, zero Store writes,
concurrent command ownership, global 1000-attempt terminal/cap boundary, public
single-page empty/nonempty results and stable shapes, shared ledger timer pacing,
exact candidate/fallback sequence and later-window reset. Run the existing WOO
history suite first; seal RED logs/production hash before implementation. Add only
contained native fixture facts, not source-mirroring tests or new timeouts.

Plan self-audit: existing CursorHistory exposes fresh/stage/advance_with_coverage,
discard_staged_page/accepted_len/reject/finish_with and supports a continuous
walk without restart. Its global budget can count every attempted native leg;
RangeWalk remains the already-shared geometry/direction component, not a copied
pager. Selection of native next action must happen before owner.advance; final
finish is permitted only after owner Done, while terminal Err/reject may stop an
active walk. Retain existing consuming signed() failure boundary explicitly.
Independent plan review must check this composition, fallback counting, ledger
resume and native error/gap policy before production edits.

Then commit regression-only REDs, implement, run focused engine/WOO native tests
with CARGO_BUILD_JOBS=4/test-threads=4, strict all-target/all-feature Clippy,
fmt/bans/request-context checker plus negatives and scope/diff checks; source,
adversarial, CALLERS and full-map reviews before the draft. Pure removed legacy
sites only; any same-site hash re-expression requires the standing exact bounded
transition/full-map review, no broad exemption. Corepack Yarn only: never
bare/Volta yarn, npx or the account-policy check. No local release WASM/full replay
or timing matrix. Keep #1795/#1747 watches and owner queue; actual SafeIDs and
later shared inputs require real integration/fresh final source-bound proof.

WOO plan review: claude-8x5tm1 independently reviewed `68286b13cab` against the
native adapter and marked it CLEAN before source edits. Conditions adopted:
CursorWalk has neither empty nor no-new-row stall flag; shared fresh/stage run
only after complete HTTP/envelope/row/census validation; CALLERS names both the
immediate ROW_SKIPPED warning and terminal owned gap. Original history 29/29 PASS;
regression-only `813b48e98e5` yields 32 PASS/8 expected assertion-RED groups on
unchanged production (three new compatibility GREEN). Logs and original source
SHA are sealed in /tmp/codex-woo-owned-history-original-red-proof.json locally.

The previous ledger-pacing test's `result.is_err()` expectation is explicitly
superseded: its accepted first ledger row now survives page-two HTTP 429 with
one owned coverage marker. The exact 6000-ms timer assertions remain; the test
now checks retained nonempty output and owned warning instead of discarded rows.
Additional source-audit controls cover simultaneous owners with log level Error,
first-gap retention across mixed pages, valid empty public single-response
completion, and unchanged consuming signed() setup classification. No timeout,
production request recipe or account fixture changes.

WOO corrected local checks: all 44 history controls PASS, including the expanded
first-failure HTTP/transport/all-malformed/truncated-prefix matrix in direct and
streamed modes. The broader engine/WOO native suite passed 796 tests before that
last test-only matrix expansion; final history rerun and strict all-target/
all-feature Clippy/fmt/bans plus checker/13 negative controls PASS. The exact
legacy map is pure prune 16->5 (11 removed occurrences, zero additions); every
other map and checker script remains identical. A final-controls rerun against
the original production source independently records 33 PASS/11 expected REDs
(the original eight plus the explicit ledger-policy supersession and two added
coverage/peer assertions). No production source was changed during that run.

Source self-audit confirms matches_entry/history_entry and streamed history_item
are byte-identical. Native parameter construction and signing/send calls are
byte-identical except reading command from owner.command() before consuming
Pending. No engine, lib, account/trading, signing or dependency edits. One owner
continues across every stage and reports only validated eligible rows; no empty
transition can set shared dedupe or manufacture recovery authority. No local
release WASM/full replay/timing run. Independent source/caller/full-map review,
CI and actual SafeIDs source-bound final proof remain distinct gates.

## Explicit owned coverage for non-pageable history stubs (2026-10-23)

Plan-first from requested actual landing #1792
`8e53b1bd39e0fd03534ffa4453349390251e4585`. One small PR for Kraken Spot,
BreakoutProp and Hyperliquid Spot. #1774 TS-freeze plan remains untouched. No
new REST capability, endpoint, fallback, cache-backed history, native pager,
account/signing change or managed Context claim; no guard/merge before #1789.

Read-only overlap inventory checked all 24 open PRs via GitHub and their complete
file lists (including pagination where needed). No target source/test overlap.
#1788 owns only WOO/Crypto account/trading/account-test regions; #1785/#1786
reporters and #1787 tokens likewise have no target files. Its author explicitly
confirmed all three stub scopes clear; old Kraken reporter #1782 is landed.
Shared AUDITS/QUALITY/CALLERS appendices overlap only under the standing
incoming-first/verbatim-once rule. Retain later actual shared dependencies for
final CI/proof; initial green artifacts are not final-proof relabeling inputs.

Source scope: Kraken src/spot/trading.rs's existing Command::Query arm only;
BreakoutProp src/history.rs's query function; Hyperliquid src/history.rs's
self.spot branch only. Keep all surrounding trading/account/native request logic
and Hyperliquid perpetual history byte-identical. Tests stay in existing
kraken/tests/spot_trading.rs, breakoutprop/tests/history.rs and
hyperliquid/tests/spot.rs. Plus pure legacy inventory prune and the four shared
OWNED_HISTORY/AUDITS/QUALITY/CALLERS appendices. No engine API, catalog,
capability schema, SDK/host, dependency or workflow change.

Existing native facts: these three adapters perform no history read for fills,
order history or income and immediately return []. Therefore they cannot prove
complete absence, even if live Store rows exist. Use existing engine
Completion/HistoryRows explicitly: create one Completion for the query, record a
venue-specific reason with Completion.incomplete(), and finish an empty
HistoryRows<()> through its owner. No row is staged or admitted; serialization is
the existing array shape. The emitted Warn history.incomplete event has exactly
fields.command and precedes CommandDone([]), including at log level Error.
One local construction of existing primitives per adapter; no copied marker
emitter, local completeness boolean, custom result envelope or new engine helper.

This is an explicit incomplete empty result, not a validated empty native read.
Keep [] for payload compatibility while making absence of coverage observable.
Do not manufacture a complete state from missing credentials, public-only mode,
unknown symbols, live WS/account caches or an empty Store. No HTTP/socket/timer,
signing work, Store publication or native lookup is introduced by the query.
Existing argument/admission behavior and all unrelated commands are unchanged.

PublicTrades retains its exact existing refusal message but completes through
Completion with ErrorKind::Unsupported and Outcome::NotSent, matching the
existing pre-I/O refusal convention (no request was issued). It emits no
result or incomplete marker. OrderByClientId retains the exact nondefinitive
miss JSON (found:false, definitive:false), delivered through Completion in the
stub branches with no unrelated history marker. Hyperliquid's existing earlier
local cache hit and invalid-client/missing-user shortcuts remain unchanged;
spot misses must never become definitive and perpetual lookup/history remains
separate. No new numeric completion fallback or legacy HTTP site.

CALLERS in the same PR must state: historical list payload remains [], but each
unsupported fills/orders/income read now has one owned incomplete warning before
completion; callers must correlate fields.command, gate coverage and avoid
treating [] as authoritative absence. Public-trade errors now have Unsupported
kind plus NotSent with unchanged text, denoting a pre-I/O refusal rather than
a native rejection. Lookup shape and live Store/cache
access stay separate. State behavior only (no draft/proof/merge status), and do
not claim NX integration or closure. Native unsupported capabilities remain
unsupported; no production-request success is inferred from a stub response.

Verification plan: run existing three test binaries before source edits. Commit
original-source assertion-RED tests for one command-owned Warn per history list,
marker-before-Done and log-level Error retention for each adapter, typed public
trade refusal and no marker on failure. Add compatibility controls for [] shape,
no HTTP/socket/timer/sign work, Store/cache unchanged, nondefinitive lookup and
consecutive command IDs never sharing warnings. Preserve existing broader
history/lookup/settings tests; no timeout/dependency changes or new network use.

Then implement only the three narrow branches, run focused engine/three-family
native tests (four Cargo jobs/test threads), strict all-target/all-feature Clippy,
fmt/bans/request-context checker and all negative controls, source-preservation
and map checks. Legacy inventory may only prune the exact removed done/fail
sites in the three files; every other full map/checker remains identical. Any
fingerprint growth stops rather than adding a bootstrap. Independent plan review
must be CLEAN before implementation; independent source/caller/map review before
the draft. Corepack Yarn only, never bare/Volta yarn, npx or account-policy. No
local release WASM/full replay/timing matrix. Keep #1796 CI watch active.

Independent plan review of `7fa7280571c` is CLEAN before implementation. Adopt
its two notes: match the existing Unsupported/NotSent convention for public
read refusal; CALLERS must advise capability-gating unsupported polling and
deduplicating user-facing alerts while preserving every command's coverage
fact. Do not rate-limit or suppress coverage inside the engine owner.

Non-pageable checkpoint: regression-only `96451b00201` records 113 selected PASS
and six expected assertion-RED groups on original source (110 prior controls and
three new lookup compatibility controls GREEN). The implementation changes only
the three specified query branches/imports. Narrow byte-preservation checks
restore each old branch and recover the entire original Kraken trading and
Hyperliquid history file, proving native trading/perpetual history and earlier
cache/identity shortcuts unchanged. Breakout's query-only file adds no I/O.

Corrected full engine/three-family native suite PASS: 1156 tests before a final
additional positive Hyperliquid cache-hit control. The final selected five test
binaries PASS 120/120, including that control. Strict all-target/all-feature
Clippy/fmt/bans, request-context checker/all 13 controls and diff checks PASS.
The legacy map is pure prune in exactly three files: Kraken Spot trading12->9,
Breakout history3->0 (entry removed), Hyperliquid history15->13. No additions,
other-map/checker edits, bootstrap or exemption. No local release WASM/full
replay/timing run; independent source/caller/full-map review and source-bound CI
remain distinct gates before landing after actual SafeIDs dependencies.
