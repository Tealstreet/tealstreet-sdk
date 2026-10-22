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
