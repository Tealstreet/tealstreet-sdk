# KuCoin futures Rust port

Branch: `feat/rust-kucoin`, based on `origin/feat/rust-rewrite` (wave 2).
Spec: `RUST_REWRITE.md`; process and gates: `VENUE_PORT_BRIEF.md`.
Full auto: phase plan/audit, implementation, verification, independent sub-agent
review, fixes, focused commits. Never stash/rebase/force push/touch master.

## Scope and design audit

Classic linear futures (`kucoin`). Spot has separate HF endpoints, symbol and
account semantics; sharing HMAC/token primitives alone does not justify a second
venue port. UTA is deliberately inactive, consistent with the TS runtime.
Typed sans-I/O state machines with borrowed serde rows; large lists stream.
Dynamic bullet endpoints supply each connection's URL, token, and ping timings.
Public snapshot subscriptions shard below 400 topics; full-depth books buffer
and recover contiguous sequences against REST snapshots. Native lot quantities
use each market multiplier exactly once at the engine boundary.
API-v2 headers sign the raw query endpoint plus exact body; passphrase is HMAC
signed too. Private tokens authenticate subscriptions without an extra login.
Reads can retry; ambiguous mutations cannot. Reconciliation retains good state
on failures and fences stale snapshots/terminal orders. Caller IDs preflight the
whole batch before writes, replacements mint new IDs.

## Phases

1. Public: crate, signing, markets/tickers, bullet socket pool and watchdogs,
   sequence books, REST/WS candles, documented-payload tests. Review then commit.
2. Private: paginated normal/stop orders, positions/balance/settings/mode,
   stream fills and reconciliation race guards. Review then commit.
3. Trading: typed sequenced placement/brackets/stops, cancel/replace, settings,
   client IDs and rejection/retry contracts. Review then commit.
4. Queries: fills/orders/income/public trades in engine JSON shapes, bounded
   paginated reads and exact conversion. Review then commit.
5. Gates: wasm/TS registration, recorder/private tape and independent signature,
   token/subscription audits; live 20-second two-book fixture; correctness and
   CPU/memory gates; Binance regression gates; CI registration. Review and commit.

## Verification and landing

Per phase: crate fmt/clippy/tests and relevant TS checks after integration.
Final: bans, wasm clippy/build, private replay, public replay (Rust CPU and memory
below TS), Binance public/private, workspace/integration checks. Record actual
numbers here. Merge latest landing into lane, push, PR into feat/rust-rewrite,
adversarial sub-agent reviews posted on PR, address/review, green CI, merge PR,
then fresh follow-up branch from landing. No private credentials required:
private validation uses independently documented synthetic payloads.

## Progress

- Planning: read current remote brief/spec and TS contracts; interfaces agreed
  between public/private/trading modules. Parent owns queries/harness/landing.

### Core phases implemented and reviewed

Public: dynamic bullet endpoints, sharded snapshot pool (380 topics/socket,
100 symbols/frame), native ping deadlines/fresh-token reconnects, pre-seed
subscription reference counts, full-depth sequence recovery and REST/WS candles.
Private: streamed positions and paginated normal/stop books, balance, modes and
settings, per-execution fills, read retries, revision/timestamp/closure guards.
Trading: sequenced jobs, native lot placement, st-orders brackets, standalone
full/partial stops, cancel+replace with fresh IDs, both cancel families, cross
leverage/margin/mode commands, headless metadata reads and per-side leverage.
Queries: fills/orders numbered pages with census, exact ledger offset paging in
one-day windows, seven-day order/fill windows, recent public trades, typed JSON.

Independent phase reviews: trading agent reviewed public and queries; public
agent reviewed private; private agent reviewed trading. Findings fixed and
regressed: generation collisions, partial position risk resets, stale closures
and reopened positions, stale metadata after writes, zero-rounded replacement
prices, isolated per-side leverage, query parameter injection and one-ms ledger
windows. Native tests/clippy pass at the core checkpoint (ongoing harness adds
integration coverage). Optional broker signed headers and signed passphrase have
independent HMAC oracles. Crypto audit found no newly unused first-port facades.

Known TS corrections (also listed in gate.mjs): complete paginated order reads;
failed/delayed account reads retain newer state; correct side-dependent stop
classification and trigger/close flags; execution deduplication; reject silently
dropped conditional fields and incompatible bracket references; cancel-symbol
covers stops too. Missing TS futures history surfaces use independently declared
documented-payload oracles. Live replay found a TS buffered-book suffix loss on
cached snapshot gaps; a production reference fix and deterministic regression
are being verified. Private validation uses synthetic documented payloads.

### Query/risk boundary follow-up

Independent final review passes. Added symbol-required itemized funding through
`/api/v1/funding-history`, 60-day windows, inclusive time overlap/exact ID dedup,
explicit incomplete/same-ms saturation errors, failed-page rollback, and shared
350 ms query pacing (11 funding tests). Pending settlement offset -1 is excluded
before parsing completed ledger cursors. Lot-to-base boundaries quantize to the
contract multiplier so 3 lots at 0.1 remain exactly 0.3, matching TS safe math.
Native crate: 67 tests pass; clippy all-targets with warnings denied passes.

### Harness gates before latest landing merge

- Captured a real 20-second public session, both books; committed compressed
  first-20-second fixture has 16,154 frames, 692 markets/tickers, two full books.
  Trimmer `--http-limit 0` retains all cached snapshots needed for gap recovery.
- Three-run public medians TS/Rust: CPU 907.3/722.1 ms, steady CPU 663.0/487.9 ms,
  retained 6.2/2.4 MB. Correctness/protocol/request gates PASS.
- Private fixture: 76 records, 17 commands including native bracket/stops,
  cancel/replace/both cancel books, leverage/margin, blocked mode switch, candles,
  all history surfaces and dedicated funding. Independent JSON oracles only
  replace the four missing TS query methods; no mutation result overrides.
- Independent phase-5 review passes. HMAC/passphrase, token URLs/private topics,
  subscription caps and detail channels are independently audited. Node 24
  harness 13/13; wasm integration 17/17. Root typecheck (44 tasks), lint (28),
  tests (27 tasks, safe-cex 6,495 tests per run) pass. Bans/wasm clippy pass.
- Binance public/private, Bybit public/private, Crypto.com public/private and
  OKX public regression gates PASS before merging the newly landed WEEX lane.
- Fixed a flaky existing memory unit oracle: a retained ArrayBuffer can coexist
  with a negative heap delta after GC. Deterministic negative-heap coverage keeps
  the runtime Wasm counting test; production metric and gate remain unchanged.

Next: merge latest landing (WEEX), rebuild and run all landing replay gates,
then PR, adversarial reviews posted on PR, green CI, merge and follow-up branch.

### WEEX landing integration checkpoint

Merged landing 6cd219ce4c with both venue registrations, broker/upgrade options,
header validation and timed replay templates intact. Merge review found the new
unconditional `SetTickerSymbols` startup hint reached KuCoin's trading fallback;
added a no-op (its complete snapshot pool is already subscribed) and native
before/after-start regression. Crate 68 tests and wasm integration 18 tests pass;
workspace tests/clippy and package typecheck/lint pass. Combined six-venue wasm
was measured at 2,457,294 bytes before the tiny hint handler; budget 2.5 MiB.

Funding tape wiring was corrected from `kind` to the wasm command's `incomeType`
field; the dedicated funding command now matches its independent expected row.
KuCoin private gate PASS at 32 frames/17 commands. The public replay exposed a
remaining TS startup/book race after the landing harness change; investigation is
active and that gate is not counted passing at this checkpoint. The landing
branch has since advanced through BingX and a startup-subscription harness fix;
merge that next, then rerun the complete gate set before PR publication.

### BingX landing integration checkpoint

Integrated landing 9ddec5d0a4, including the stronger typed request matcher,
subscription generations/readiness, and timed response visibility. Both response
time (argument 5) and signature-audit headers (argument 6) are preserved and
independently regression-tested. KuCoin topics normalize individual symbols
across its batched colon topics. Independent merge/harness review passes (34
harness tests). Seven-venue wasm: 2,882,955 bytes; budget 3 MiB.

The remaining TS book mismatch was harness completion timing: accelerated frames
could finish while KuCoin's 3-RPS queue still held recovery snapshot reads.
A bounded generic settle hook now waits for tracked public read promises before
final copy/metrics. No traffic or comparison is omitted. The queued-read oracle
regresses this directly. Three-run public gate PASS: 16,154 frames, 692 markets,
CPU TS/Rust 1115.8/791.8 ms, steady 782.3/561.3 ms, retained 6.5/2.8 MB, full
books identical. Private gate PASS: 32 frames, 17 commands, all five history
command paths including funding independently validated, zero unserved/unused
mutations or protocol failures. Run the full seven-venue gate set next.


### MEXC landing integration checkpoint

Landing advanced through MEXC (#1304, 63d521a860) before PR publication.
Integrated its socket ordinals, deterministic public replay clock, settings/frame
extensions and size-optimized wasm build. Kept typed request/response matching
outside MEXC, signature header validation, staged/time visibility and query-only
KuCoin read oracles. Shared socket routing accepts the venue URL key so generated
connectId varies while bullet token identity remains exact. Trimming keeps all
in-window HTTP anchors by default (optional explicit cap remains available).

Review found a virtual-clock drain could starve real deadlines on a stalled
read. Fixed with a real event-loop yield per virtual advance; independent review
passes and all 49 harness regressions pass, including stalled reads, queued timer
recovery and token URL routing. Workspace fmt/bans/native+wasm clippy/tests pass.
Eight-venue wasm is 2,662,854 bytes against the existing 3 MiB budget. Node 22
full eight-venue replay and SDK integration verification are running; results
below supersede the earlier seven-venue performance measurements.


### Final local gates (eight venues, Node 22)

All 19 replays PASS: eight public correctness/CPU/memory gates and eleven
account/trading correctness gates (including all four OKX variants). KuCoin
public: 16,157 replay lifecycle/data frames, 692 markets/tickers, two identical
full books. Three-run medians:

| Public venue | TS / Rust CPU (ms) | TS / Rust retained (MB) |
| --- | --- | --- |
| Binance | 294.1 / 130.4 | 4.2 / 2.2 |
| MEXC | 1003.4 / 448.0 | 10.0 / 3.4 |
| Bybit | 1070.4 / 752.9 | 6.3 / 2.4 |
| BingX | 151.7 / 95.1 | 3.6 / 2.0 |
| Crypto.com | 196.0 / 151.3 | 2.0 / 1.4 |
| OKX | 1117.1 / 957.6 | 5.2 / 3.4 |
| KuCoin | 1050.2 / 678.8 | 6.6 / 2.7 |
| WEEX | 132.3 / 86.6 | 5.0 / 2.1 |

KuCoin steady CPU 720.2 / 430.7 ms; private 32 frames / 17 commands PASS,
with zero unserved/unused mutation or socket protocol failures. Under concurrent
root tests, two Node 22 attempts hit the existing one-second backing-buffer
settling deadline before comparison; reruns after root tests PASS unchanged.
Root typecheck 44 tasks, lint 28 tasks, tests 27 tasks (6,563 safe-cex tests per
suite) PASS. Workspace 531 native tests, fmt, bans and native/wasm clippy PASS;
wasm integration 20, host 13 and harness 49 tests PASS. Wasm 2,662,854 bytes.

Final shell audit found the incoming MEXC native-unit bridge must also apply to
KuCoin. Extended its real wasm test to 7 lots at multiplier 0.1, filled 3 and
remaining 4: proved failure from missing placement extras, then enabled the
bridge and decimal-safe division (base 0.7/0.3/0.4; native 7/3/4). Independent
review and final TS verification are recorded in the next checkpoint.

Next: publish PR into feat/rust-rewrite, fresh adversarial local sub-agent
reviews posted as comments, fix/review loop, green CI, merge and fresh branch.

Final lot-bridge review PASS, no P1/P2; existing MEXC compatibility 3/3 PASS.
Fresh Node 22 package typecheck/lint, all 6,563 unit tests (458 files) and all
20 wasm integration tests PASS after the bridge fix. Local gates are complete.

### PR #1312: adversarial round 1

Opened https://github.com/Tealstreet/tealstreet-sdk/pull/1312 into
feat/rust-rewrite at 68e576b1c4. Its initial CI (root, secrets, Rust native,
wasm and all replay gates) is green, but independent reviews found nine accepted
issues. Findings and sources are posted in PR comments 6274462654 and the
subsequent addendum; the initial head is not considered ready to merge.

- R1 P1: partial WS rows erase known safety flags; preserve absent fields,
  hydrate own placements across ACK/stream races, refuse unknown external order
  amendments until reconciliation. Keep submitted bracket protections through
  partial events and REST omission before replacement.
- R2 P2: consume native margin-mode/cross-leverage maps for flat symbols and
  fence delayed metadata reads.
- R3 P2: cancel obsolete depth retries to prevent post-recovery rollback;
  cancel them across disconnects and require a ready detail socket.
- R4 P2: wait for welcome before subscriptions/private startup/ping, retaining
  a bounded handshake deadline. Replay bootstraps only actual recorded welcomes
  without redelivery, invented messages, lost frame counts or reordered business
  traffic. TS's early sends are explicitly documented in KNOWN_TS_FIXES.
- R5 P2: retain exact integer fill times for inclusive range filtering.
- R6 P2: floor clock-derived default query bounds from fractional native clocks;
  keep explicitly supplied fractional bounds invalid.
- R7 P1: restore position contractsNative in the worker bridge; real wasm test
  runs the actual copier-close path and asserts native size 10 for base size 1.
- R8 P1: after cancel acceptance, prove terminal state/final fills through a
  bounded signed read before creating a reduced remainder replacement.
- R9 P2: use revision-scoped snapshot absence fences so triggered stops may
  migrate into the normal book; reserve permanent tombstones for terminal proof.

Public 18 and history 10/funding 13 focused tests pass. Independent follow-up
review of recorded welcome routing and position bridge passes; harness 52,
wasm 20 and fresh TS typecheck/lint/6,563 unit tests pass. The private/trading
repair and final cancellation-race audit are finishing before aggregate native,
wasm rebuild/replays, fix commit, push, and round 2 review.

### Review repair checkpoint before BloFin integration

All repairs pass the aggregate KuCoin crate: 95 tests, fmt/bans and all-target
clippy with warnings denied. Private 23/trading 28 cover the final fill watermark,
contradictory active proof, terminal removal and pre-send race. R10 P1 was found
in the final safety audit: own brackets could lose protections before first REST
reconciliation. The private placement cache now preserves submitted levels and
trigger reference across immediate or delayed-REST amendments; two regressions
pass. Direct unsupported FetchLeverage commands now return an error instead of
reaching an unreachable trading fallback (before/after-start regression).

Independent round-2 public/replay/bridge review passes. Query and private repairs
are ready for cross-review of this committed checkpoint. Parent TS typecheck,
lint and all 6,563 tests pass after the bridge change, as do wasm 20 and harness
52. Landing advanced through BloFin (#1300) and MEXC parity (#1311); integrate
that next, preserving typed mutation matching, signed headers, welcome bootstrap,
queued read draining and native extras, then rebuild/rerun integrated gates.

### BloFin/MEXC parity integration checkpoint

Merged landing d9db55921c, keeping all nine venue registrations, KuCoin broker
and native-unit bridge, typed mutation matching, signed-header argument order,
welcome bootstrap, queued-read settling and query-only independent oracles.
Accepted incoming websocket request accounting/getter, typed matcher tests,
settled-memory helper/regressions, settings side-leverage NaN fallback, and the
BloFin/MEXC parity cases. Independent shared merge review PASS; harness 57 and
wire 2 tests, workspace cargo check and diff checks PASS. Budget remains 3 MiB;
measure the rebuilt artifact before final gates.

Independent private/trading and public/bridge follow-up reviews pass. Round-2
query review found R11 P2: per-window ns filtering leaves sub-ms fill gaps at
integer-ms window transitions. Preserve original caller bounds for fill filters
across every page; regression/repair is in progress, posted on PR #1312 along
with R10. Rebuild and run integrated gates once that final patch is ready.

### Final query review repair

R11 is fixed and independently cross-reviewed: fill rows retain immutable caller
bounds for exact native nanosecond filtering across integer-ms REST windows.
The new regression proves sub-ms boundary rows survive either adjacent page and
rejects rows one ns outside both global bounds. All other query kinds preserve
their existing window filters. No new P1/P2 findings remain in the final public,
private/trading, query, shared replay and native-unit bridge reviews.

Nine-venue fmt/bans, native and wasm clippy with warnings denied, and all 634
workspace tests PASS (KuCoin 96). Rebuilt wasm integration 21, host 15 and
harness 57 tests PASS. KuCoin public/private and Binance public replays PASS;
remaining integrated replays are running before the final gate checkpoint.

### Backpack landing integration

Before integrating Backpack, all required brief gates passed on the nine-venue
artifact (2,846,590 bytes): KuCoin public CPU 734.6/498.4 ms, steady CPU
516.0/344.4 ms, retained 6.6/2.7 MB (TS/Rust); 16,157 frames, 692 markets, two
books. Private 32 frames/17 commands PASS. Binance public/private, BloFin
public/private and MEXC public/private passed too, plus Bybit private. Remaining
extra replay jobs were stopped because the target had advanced to Backpack.

Integrating landing b265a0bab0 now. Adopt its centralized Tape implementation,
carrying KuCoin welcome bootstrap, dynamic socket URL fields, signature headers,
JSON pongs, query-only independent oracles and queued-read settling. Keep both
venues' TS/Wasm options and tests, native quantities, host disposal guards and
strict request matching. Independent audits own separate harness/TS/CI scopes;
rebuild the ten-venue artifact, run required brief and Backpack compatibility
gates, then commit/push and require green final-head CI before merging.

Backpack merge resolution and independent cross-reviews PASS: shared harness
88 tests, wire 4, actual wasm integration 22 and host 25 PASS. Native workspace
684 tests, fmt/bans, native and wasm all-workspace clippy PASS. TS bridge package
typecheck/lint PASS; root typecheck 44/lint 28 tasks PASS and root tests continue.
Ten-venue wasm is 3,131,044 bytes, below the unchanged 3,145,728-byte budget.
No new P1/P2 findings in the independently reviewed shared harness, TS bridge
or gate/CI resolution. Required replay gates run next against this artifact.

### Final ten-venue brief gates (before final-head CI)

All required brief gates PASS after the Backpack integration: KuCoin and
Binance public/private, plus Backpack public/private compatibility. Three-run
public medians (TS / Rust), all with exact output and protocol/request audits:

| Venue | CPU ms | Steady CPU ms | Retained MB |
| --- | ---: | ---: | ---: |
| KuCoin | 1265.4 / 1005.2 | 860.2 / 639.9 | 6.6 / 2.8 |
| Binance | 314.0 / 163.1 | 188.6 / 79.3 | 4.2 / 2.3 |
| Backpack | 219.3 / 127.8 | 122.6 / 68.8 | 3.2 / 1.6 |

KuCoin private: 32 frames / 17 commands, PASS, zero unserved requests or
protocol/signature errors. Public: 16,157 frames, 692 markets/tickers, two books.
Native workspace 684 tests (KuCoin 96), fmt/bans and both clippy targets,
harness 88, wire 4, wasm integration 22, host 25 PASS; wasm 3,131,044 bytes.
Root typecheck 44 and lint 28 tasks PASS. Root test command under Node22 failed
only six dynamic-import timeouts in two unchanged Hyro/metadata files (6,559
other tests pass). Node24 and isolated diagnostics reproduce the Hyro timeout;
a serial full-root retry and module-loader investigation continue. Assertions
and timeouts remain unchanged. Final-head CI must be green before merge.

Adversarial findings R1–R11 repaired and independently cross-reviewed; no new
P1/P2 after final query fix and shared merge review. Publish this checkpoint so
the full ten-venue CI matrix runs while the local import failure is diagnosed.

### Bitget landing integration

Merged Bitget (#1308), landing 4e468ed2e5, in 5a2f2211a4. Independent shared
harness, host/SDK, engine/parser and gate/CI reviews passed with no new P1/P2.
Retained strict typed matching, generated-ID validation, signed headers, recorded
welcomes, query-only oracles, bounded settling and native units. Restored
5300f4960f's ESBUILD_WORKER_THREADS=0 for both replay implementations; that
setting had been lost during an earlier incoming merge. Runtime construction is
measured symmetrically and all public performance assertions remain strict.

Workspace 763 native tests (96 KuCoin), fmt/bans and native/Wasm clippy passed.
Harness 129, wire 4, actual Wasm 26, host 25 and root typecheck/lint passed.
Eleven-venue Wasm was 3,775,268 bytes against the fixed 4 MiB aggregate budget.

### Eleven-venue brief gates

All eight local required/compatibility replays passed: KuCoin, Binance, Bitget
and Backpack public/private. Three-run public medians (TS / Rust):

| Venue | CPU ms | Steady CPU ms | Retained MB |
| --- | ---: | ---: | ---: |
| KuCoin | 509.0 / 231.1 | 295.9 / 134.7 | 6.8 / 2.7 |
| Binance | 181.5 / 80.0 | 94.3 / 29.6 | 4.4 / 2.3 |
| Bitget | 554.6 / 533.6 | 367.6 / 365.1 | 6.0 / 2.5 |
| Backpack | 172.8 / 100.9 | 95.0 / 67.2 | 3.4 / 1.8 |

KuCoin public covered 16,157 frames, 692 markets and two books; private covered
32 frames and 17 commands. Exact output, request, signature, mutation and socket
checks passed. Checkpoint b8fbe9d07f was pushed and now has full green CI. One
BingX steady-CPU inversion (37.9 versus 37.4 ms) passed an unchanged local replay
and one unchanged CI retry; no assertion or threshold was relaxed. The unchanged
local cold-import diagnostic also closed with full CI passing both root suites.
Before that head could merge, Hyperliquid landed and required integration.

### Hyperliquid landing and per-venue gate integration

Integrating Hyperliquid (#1306), landing 5c47683969. Adopt its feature-selected
Wasm builds and 12-venue CI matrix, with a fixed 1,258,291-byte per-venue budget.
Keep KuCoin in the host's default/optional features, build whitelist and SDK
registration, preserving broker/passphrase options and native lot metadata.
The new OrderByClientId query explicitly errors without I/O for KuCoin, matching
peer ports; all 97 KuCoin tests and clippy pass.

Workspace 831 native tests, fmt/bans/native+Wasm clippy, full Wasm 26/host 25,
root typecheck 44/lint 28 tasks, and 315 focused wrapper/client-ID tests pass.
Independent shared merge reviews pass. The full 12-venue artifact is 4,752,442
bytes and is exercised by integration tests; delivery-size enforcement now
follows the landing's individual venue bundles.

The new exact request-set comparison exposed only documented TS read gaps.
Independent fixture declarations require exactly seven native GETs: five missing
query reads, final cancellation proof and authoritative position mode. Each key
must actually be sent by Rust. Query declarations bind command, range, symbol,
pagination, stage and independent expected output; cancellation binds the order,
market, successful DELETE evidence and stage. Active-book aliases require exact
first-page keys, identical responses at the same stage, one page and at most 50
items. Every other read, every write, signatures, timing and protocol checks
remain strict. No expected key is derived from observed Rust output. Harness
169 and wire 4 tests pass. Independent final read-contract review passes with
no P1/P2 findings; raw child replay JSON remains unchanged.

Selective KuCoin Wasm is 742,907 bytes, below the fixed 1,258,291-byte budget.
Public medians TS/Rust: CPU 633.9/393.3 ms, steady CPU 452.7/238.7 ms and retained
memory 7.0/3.2 MB, across 16,157 frames, 692 markets and two books. Private
32 frames/17 commands pass. Exact output/request/signature/protocol checks pass.
One earlier attempt hit the existing memory-settling deadline before comparison
during concurrent checks; an unchanged quiet rerun passed. Binance and incoming
Hyperliquid selective compatibility gates run next before commit/push and green
final-head CI.

### Twelve-venue integration checkpoint

All six required/compatibility selective replays pass: KuCoin, Binance and
Hyperliquid public/private. Exact request and write comparisons remain enabled.
Three-run public medians (TS / Rust):

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 633.9 / 393.3 | 452.7 / 238.7 | 7.0 / 3.2 | 742,907 |
| Binance | 257.9 / 128.3 | 149.4 / 62.6 | 4.2 / 2.2 | 750,118 |
| Hyperliquid | 297.6 / 41.2 | 22.1 / 11.3 | 4.9 / 1.9 | 986,799 |

Each bundle is below the fixed 1,258,291-byte budget. KuCoin private remains
32 frames/17 commands. Independent phase, adversarial repair, shared-merge and
read-contract reviews pass. The pending merge is ready to commit; then integrate
the latest roster-only landing update, push, post the final review evidence and
require green CI at the new head before merging PR #1312 into feat/rust-rewrite.

### Final CI performance repair

Head 1dc54928a3 passed KuCoin and every other venue replay job except BingX.
BingX preserved exact correctness but missed the strict steady CPU comparison
on the CI runner (TS/Rust 30.0/33.1 ms). Change only its package release
optimization from size (`z`) to throughput (`3`), matching the existing Bitget
profile approach. The fixed byte budget and all gate assertions are unchanged.

Controlled selective builds on the same Node 22.23.3 toolchain, three public
runs each, measured Rust steady CPU 61.666 -> 46.303 ms (-24.9%), with the TS
control 73.613 -> 74.056 ms (+0.6%). Rust total CPU was 120.726 -> 96.789 ms
and retained memory 2,045,440 -> 2,045,480 bytes. The local baseline already
passed; this comparison demonstrates improved margin rather than reproducing
the cloud failure. Every candidate Rust steady run beat every baseline run.
Both public gates and the candidate private gate passed their exact output,
request, mutation and protocol checks. Wasm grows 768,786 -> 832,618 bytes,
still 425,673 bytes below the fixed 1,258,291-byte limit.

Independent final profile review passes with no P1/P2 findings. Fresh final-head
CI is required before merging. Previous native, KuCoin, Binance and Hyperliquid
validations remain applicable because this override affects only the BingX
release artifact.

### Transport and Bitrue landing integration plan

Landing fc18c55874 adds account transport routing (#1313) and Bitrue (#1314)
while final CI runs. Merge it without dropping either venue, the landing
structure, per-venue budgets, strict KuCoin read contracts or signature checks.
Independent workers resolve disjoint host/SDK, gate/request, and build/roster
conflicts; a separate cross-review audits their merged result.

The new brief requires explicit socket roles and independent stable feed IDs.
KuCoin bullet sockets can share their endpoint while tokens rotate, and hosts
strip query parameters when tracking fallback. Emit Public detail/snapshot roles
and Private account role, with stable detail/snapshot-{index}/private IDs.
Generalize the engine's static feed name to an owned String to support runtime
shard counts without leaks or arbitrary caps. Preserve the recovery policy.
Extend existing shard, reconnect and private tests to verify role separation,
unique shard names and identity stability across fresh bullet tokens. Review
this compatibility adaptation independently.

Verify fmt/bans/native+Wasm workspace clippy and tests, harness/wire/host, full
Wasm SDK integration and root typecheck/lint. Repeat selective KuCoin and
Binance brief gates, plus incoming Bitrue public/private gates. Keep all exact
comparisons, CPU/memory thresholds and the fixed per-venue budget unchanged.
Commit the integration and reviews, push, require green final-head CI across
all 13 venues before merge, then cut the follow-up branch.

### Thirteen-venue integration checkpoint

All six selective gates pass: KuCoin, Binance and incoming Bitrue public/private.
Three-run public medians TS/Rust, with exact output/request/write/signature and
socket audits unchanged:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 1177.0 / 828.1 | 795.4 / 511.2 | 7.0 / 3.2 | 744,387 |
| Binance | 289.0 / 118.7 | 167.2 / 50.2 | 4.2 / 2.2 | 751,369 |
| Bitrue | 279.9 / 270.2 | 187.0 / 171.6 | 3.8 / 2.4 | 711,181 |

All bundles remain below 1,258,291 bytes. KuCoin public retains 16,157 frames,
692 markets/tickers and two books; private retains 32 frames/17 commands.
Bitrue's first public attempt timed out settling memory before comparison; an
unchanged retry reached comparison, revealing that the local command used
BTCUSDT/ETHUSDT instead of Bitrue's E-BTC-USDT/E-ETH-USDT. Correcting only those
CLI inputs to match the incoming CI configuration passes the complete gate.
No assertion, deadline or source behavior was changed for these attempts.

Workspace 917 tests (97 KuCoin), fmt/bans/native+Wasm clippy, harness 175,
actual Wasm SDK integration 29, host/wire 34 (including four wire tests), and
root typecheck44/lint28 pass. Incoming Bitrue's account test closed socket
pattern needed `..` for new channel/context fields; that compatibility fix
is included. Independent cross-reviews of merged host/SDK/features, shared
harness/request contracts and transport/feed identity all pass with no P1/P2.
The engine accepts owned feed names without changing their serialized shape.

The isolated BingX optimization already passed all 12 replay CI jobs at
55df2945fc. Publish this integrated head and require green native/Wasm/root CI
and all 29 public/private gates across 13 venues before merging PR #1312.

### Bitget parity landing integration plan

While head 24edca0460's 13-venue/29-replay CI matrix is green and its root
tests are finishing, landing d172a666b1 adds Bitget inverse, collateral balance
frames, UTA ledger and trailing parity (#1317), conflicting in the SDK wrapper
and frame applier. Preserve incoming inverse/native conversions alongside
KuCoin's native-lot extras, broker options and registration. Merge all new
Bitget replay and native-contract integration checks without changing budgets
or comparisons. Existing owned feed identities remain intact.

One worker resolves the two TS conflicts; independent reviewers audit the
shared harness/host/CI auto-merges and the resolved frame boundary. Verify
workspace tests/fmt/bans/native+Wasm clippy, harness/combined-Wasm/host-wire and
root typecheck/lint. Repeat KuCoin/Binance public/private plus Bitget public,
Classic private, inverse private and four dedicated UTA/Classic native contract
integrations. Commit results and reviews, push, then require all final-head CI
checks (13 venue jobs, now 30 public/private replay gates plus four dedicated
Bitget integrations) before merge and the fresh branch.

### Adversarial R12: main-thread native hydration

Independent merge cross-review found the main BaseExchange.applyWorkerFrame
constructor still enabled native extras only for MEXC, although the Rust worker
correctly enabled KuCoin. Worker-to-main binary frame hydration therefore lost
KuCoin order remainingNative/filledInPlacementUnits and position contractsNative;
the main copier could send base quantity as a native lot count. Add KuCoin to
the main receiver's native-placement flag while retaining the incoming separate
Bitget-inverse flag. The constructor audit finds exactly two production paths,
both now correct.

Extend the real KuCoin Wasm regression to deliver onFrame buffers to a main
BaseExchange, assert decimal-safe order extras and ten position lots, then run
the actual CrossTradeManager against that hydrated main store and assert a
ten-lot close. Independent cross-review passes. Temporarily removing only the
KuCoin main-receiver flag proves the regression fails on missing contractsNative;
restoring it passes the complete 28-test Wasm file. Inverse-frame unit1 passes,
full combined Wasm integration29 and host/wire35 pass, workspace988 tests and
fmt/bans/native+Wasm clippy pass. Post-restoration root typecheck44/lint28 pass.
No temporary source change remains; selective replay gates run next.

### Bitget parity and R12 gate checkpoint

All seven selective gates pass: KuCoin/Binance public+private, Bitget public,
Classic private and inverse private. All four dedicated UTA USDT/USDC ledger
and Classic/UTA trailing native-contract integrations pass. Public three-run
medians TS/Rust:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 522.9 / 297.7 | 341.1 / 174.7 | 7.0 / 3.2 | 745,027 |
| Binance | 316.2 / 141.3 | 193.5 / 72.8 | 4.2 / 2.2 | 752,009 |
| Bitget | 611.5 / 590.9 | 411.3 / 393.2 | 5.8 / 2.5 | 928,097 |

Exact output, reads/writes, signatures and socket audits remain enabled. Every
bundle is within the unchanged 1,258,291-byte budget. KuCoin public remains
16,157 frames/692 markets/two books and private32frames/17commands.

Independent shared-harness/host and CI/native transport merge reviews pass.
Independent TS constructor/frame-boundary review found R12, now repaired and
cross-reviewed, with a failing negative control and restored full positive
Wasm test. Final workspace988 (97 KuCoin), harness175, actualWasm29, host/wire35,
inverse-frame unit1, fmt/bans/native+Wasm clippy and root typecheck44/lint28 pass.
The prior published head24edca0460 reached all23 green checks, but Bitget parity
then conflicted. Publish this head and require complete new-head CI, including
all30 replay gates and four dedicated integrations across13 venue jobs.

### WOO landing integration plan

Landing6945ee83b4 adds WOO (#1316) while fb4563a52b CI starts. Resolve
shared harness/host conflicts through independent disjoint workers, preserving
all strict KuCoin contracts and the 14-venue feature union. Keep WOO's audit
connection hook, exact authentication and fallback/history behavior. Retain
R12 in both frame receiver paths and Bitget inverse/collateral flags. Incoming
crypto cleanup deletes the unused first-port HMAC-SHA512 facade.

The common PlaceOrderOpts now includes protected-market maxSlippagePct. Set
None for ordinary KuCoin replacements and reject explicitly requested protection
for the whole batch before any I/O, since this port cannot honor that contract;
add a focused Market/Limit batch regression with a specific slippage error.
Independently review this adaptation and all conflict resolutions.

Verify workspace fmt/bans/native+Wasm clippy/tests, harness, combined actual
Wasm+host/wire, root typecheck/lint, and selective KuCoin/Binance/WOO public and
private gates. Preserve all incoming Bitget parity checks in CI. Commit results
and posted independent reviews; require full green final-head CI (14 venue jobs,
32 public/private gates plus four dedicated Bitget integrations), merge and cut
the fresh branch.

### Fourteen-venue WOO integration checkpoint

All six required/compatibility replays pass: KuCoin, Binance and incoming WOO
public/private. Public three-run medians TS/Rust:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 1184.6 / 822.6 | 793.3 / 511.5 | 7.0 / 3.2 | 745,857 |
| Binance | 261.7 / 134.1 | 150.9 / 60.8 | 4.2 / 2.2 | 752,597 |
| WOO | 49.8 / 45.4 | 21.1 / 18.1 | 1.6 / 1.0 | 835,807 |

All exact output/request/write/signature/socket audits remain enabled. Every
bundle is below the unchanged1,258,291-byte limit. KuCoin public16,157frames/
692markets/two books and private32frames/17commands pass. WOO private60frames/
20commands pass. KuCoin and WOO each hit one initial TS memory-settling deadline
before comparison; one unchanged retry each passed. No deadline, threshold or
assertion was changed.

Workspace1,099 tests (98 KuCoin), fmt/bans/native+Wasm clippy, harness181,
combined actualWasm31, host/wire35 and root typecheck44/lint28 pass. Independent
host/SDK, harness/audit and command-compatibility reviews pass, no outstanding
P1/P2. Incoming unused HMAC-SHA512 facade cleanup is retained. Unsupported
slippage protection is explicitly documented in KNOWN_TS_FIXES and regression
tested before any batch I/O. R12 remains repaired in both hydration paths.

Prior headfb4563a52b reached complete green CI before WOO landed. Publish this
reviewed head and require all new-head checks green, including all32 replay
gates and four Bitget ledger/trailing integrations across14venue jobs, before
merging PR1312 into feat/rust-rewrite and cutting the follow-up branch.

### Lighter landing integration plan

Landing31ec401f60 adds Lighter (#1319) while WOO-integrated36623d933c CI
starts. Resolve feature/CI/build/venue and harness conflicts through disjoint
workers, then independent cross-review. Preserve Lighter native signing and
replay initialization/body/matched-response validation, WOO connected audits,
KuCoin signed headers in both response call signatures, exact token/ordinal
welcomes, seven independent read contracts and all prior native hydration fixes.
Cargo.lock takes the incoming lock plus KuCoin's exact package/host dependency,
without dependency version changes. No new common engine option requires a
KuCoin adaptation.

Verify workspace fmt/bans/native+Wasm clippy/tests, full harness, combined actual
Wasm/host-wire and root typecheck/lint. Repeat required KuCoin/Binance gates plus
Lighter public/private using its native ETH/BTC book names. Keep the unchanged
1,258,291-byte per-venue budget and all performance/correctness assertions.
Commit reviews/results and push, then require all final-commit CI checks across
15 venue jobs (34 public/private gates plus four Bitget integrations), merge
PR1312 into feat/rust-rewrite and create the follow-up branch.

### Fifteen-venue Lighter integration checkpoint

All six required/compatibility replays pass: KuCoin, Binance and Lighter
public/private. Public three-run medians TS/Rust:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 933.3 / 689.7 | 696.8 / 423.0 | 7.0 / 3.2 | 745,857 |
| Binance | 211.9 / 105.2 | 118.5 / 40.5 | 4.2 / 2.2 | 752,597 |
| Lighter | 694.8 / 448.4 | 428.7 / 271.9 | 3.0 / 1.8 | 964,467 |

All exact output/request/write/signature/socket audits and the fixed
1,258,291-byte limit remain enabled. KuCoin covers16,157publicframes/692markets/
two books and32privateframes/17commands. No retry was needed in this run.

Final workspace1,183 tests (98 KuCoin), fmt/bans/native+Wasm clippy, harness197,
complete combined actualWasm38 (including Lighter/BloFin/Bitrue boundaries),
host/wire35 and root typecheck44/lint28 pass. Independent harness/signing and
host/SDK/lockfile cross-reviews pass, no outstanding P1/P2. Cargo.lock adds only
the expected Lighter package/dependency relative to our previous parent; versions
and sources remain unchanged. All R1–R12 repairs, main-native hydration, WOO
audits and KuCoin slippage preflight remain intact.

Publish this checkpoint with reviews/results on PR1312, require all final-commit
CI checks green across15venue jobs (34replay gates plus4Bitget native-contract
integrations), then merge into feat/rust-rewrite and create the follow-up branch.

### Orderly family landing integration plan

Landingf66bdc26f9 adds Orderly and six broker aliases (#1318) while the
Lighter-integratedc59bbb77a9 replay matrix is fully green and root CI finishes.
Resolve harness/host conflicts preserving generation-aware correlated request
audits, signatures and protocol fallbacks. Retain original KuCoin headers in
both response call signatures, seven independent GETs, typed token/topic identity
and the R12 main/worker native lot bridge. Incoming explicit position IDs remain
optional; KuCoin keeps its legacy frame identity. New Deactivate notices are
handled by shared hosts; no KuCoin constructor/API adaptation is needed.

Move only KuCoin entries among stable existing entries in CI/build/venue/host/
known-fix lists and format the default feature union on separate lines. Keep
all other entry order and behavior, thresholds and byte budget unchanged.
Independent cross-review covers these relocations and both merge parents.

Run workspace fmt/bans/native+Wasm clippy/tests, full harness/combined actual
Wasm/host-wire, root typecheck/lint, KuCoin+Binance public/private and incoming
Orderly public/mainnet private plus a broker testnet private representative.
Preserve CI's complete fourteen Orderly broker/network private combinations.
Commit results/reviews and require green final-commit CI:16venue jobs,49 public/
private replay gates plus4Bitget integrations. Merge PR1312 into feat/rust-rewrite
and cut the fresh follow-up branch.

### Sixteen-venue Orderly integration checkpoint

All seven selective gates pass: KuCoin/Binance public+private, Orderly public
and mainnet private, Tealstreet DEX testnet private. Public three-run medians
TS/Rust:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 1438.5 / 962.7 | 957.6 / 599.8 | 7.0 / 3.3 | 746,265 |
| Binance | 184.7 / 80.7 | 102.3 / 28.5 | 4.2 / 2.2 | 753,005 |
| Orderly | 194.9 / 132.8 | 148.1 / 92.8 | 2.0 / 1.2 | 783,885 |

Every bundle is within the unchanged1,258,291-byte limit; exact output, request,
mutation, signature and protocol checks remain enabled. KuCoin remains
16,157publicframes/692markets/two books and32privateframes/17commands. Orderly
public initially stopped at TS memory settling before comparison; an unchanged
retry passes. Broker testnet33frames/14commands also pass.

Workspace1,265 tests (98 KuCoin), fmt/bans/native+Wasm clippy, harness226,
complete actualWasm53, host/wire42 and root typecheck44/lint28 pass. Independent
harness/correlated-request/signing and host/SDK/default-loader/native/wire reviews
pass, no outstanding P1/P2. KuCoin notes and registration relocations preserve
contents; all other entry order remains unchanged. The feature union is16 and
its default list is now multiline. Optional position IDs preserve the legacy
KuCoin layout, original role/feed/token URLs and both R12 hydration predicates.

The precedingc59bbb77a9 reached full green CI while this landing was integrated.
Publish this reviewed checkpoint and require all final-commit checks green,
including49 public/private replays and4Bitget native-contract integrations across
16venue jobs, before merging PR1312 and cutting the follow-up branch.

### Bybit variants landing integration plan

Landing922b5fdc89 adds Bybit inverse/spot/demo/Hyro (#1307) while Orderly-
integrateda5e9bc447a CI starts. Resolve SDK/frame/store docs and large shared
harness conflicts through disjoint workers, then independent cross-review.
Preserve native-column precedence, inverse extras and both R12 KuCoin hydration
paths, Orderly IDs/generations, Lighter/WOO audits, typed raw signed headers and
seven independent KuCoin reads. Adopt incoming stricter before-import memory
baseline/nonnegative component accounting and diagnostics without weakening
any assertion. KuCoin request-contract adapters must compose with the new gate.

New common Bybit BBO/tick-slippage fields require None on ordinary KuCoin
replacements and explicit whole-batch rejection before any I/O, matching peers;
add focused tests for each control and document the deliberate TS correction.
Review the shared engine buffer reuse and native quantity columns as compatible
changes, then run workspace fmt/bans/native+Wasm clippy/tests, full harness,
combined actualWasm+host/wire and root typecheck/lint. Repeat KuCoin/Binance
public/private plus Bybit inverse/spot/demo/Hyro public/private gates using the
shared Bybit feature bundle. Preserve all49 prior replay gates and four Bitget
integrations; final CI now covers57 replays across16venue jobs. Commit reviews/
results and require green final-commit CI before PR1312 merge/follow-up branch.

### Bybit variants and measurement integration checkpoint

All14 local replays pass without retry: KuCoin/Binance public+private and all
five Bybit-family public+private cases (linear,inverse,spot,demo,Hyro). Hyro
public is the incoming documented contract fixture, not live endpoint evidence.
Three-run medians TS/Rust using incoming pre-import memory baselines:

| Venue | CPU ms | Steady CPU ms | Retained MB | Bundle bytes |
| --- | ---: | ---: | ---: | ---: |
| KuCoin | 1327.2 / 842.3 | 861.4 / 559.2 | 21.3 / 3.9 | 753,048 |
| Binance | 278.4 / 139.5 | 160.9 / 59.2 | 19.0 / 2.7 | 759,949 |
| Bybit linear | 544.4 / 346.9 | 313.6 / 158.9 | 22.5 / 3.1 | 923,798 |
| Bybit inverse | 176.6 / 56.9 | 112.7 / 27.4 | 16.5 / 1.3 | same Bybit bundle |
| Bybit spot | 355.7 / 250.9 | 226.7 / 134.3 | 17.7 / 2.1 | same Bybit bundle |
| Bybit demo | 485.7 / 318.2 | 268.2 / 160.4 | 21.9 / 3.1 | same Bybit bundle |
| Hyro documented | 38.9 / 31.0 | 13.1 / 8.1 | 16.2 / 0.9 | same Bybit bundle |

All bundles stay below1,258,291bytes. KuCoin public16,157frames/692markets/two
books and private32frames/17commands pass all exact audits. New measurement
model includes runtime/import allocations and rejects every negative component
on public and private runs; diagnostics, arithmetic and limits remain incoming,
so these retained-memory figures are not the earlier post-construction figures.

Workspace1,368 tests (99 KuCoin), fmt/bans/native+Wasm clippy, harness246,
complete actualWasm75, host/wire45 and root typecheck44/lint28 pass. Independent
harness/signature/memory/impl-plan and SDK/frame/native-column cross-reviews
pass with no outstanding P1/P2. KuCoin fixture has zero impl tags, so tagged
response-plan exclusions omit none of its requests or writes. All seven native
GETs and query-only oracle guards remain strict; R12's4remaining/3filled/
10position/10copier-close assertions remain.

Both new Bybit controls reject the full KuCoin batch before I/O, replacements
initialize None, and focused regression + KNOWN_TS_FIXES document the correction.
Publish this reviewed head and require all final-commit checks green across
16venue jobs,57 replay gates and4Bitget integrations, then merge PR1312 into
feat/rust-rewrite and cut the fresh branch.
