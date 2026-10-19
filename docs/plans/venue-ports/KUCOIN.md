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
