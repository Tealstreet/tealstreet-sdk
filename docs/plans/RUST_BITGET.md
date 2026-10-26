# Bitget Rust port

Lane: `feat/rust-bitget`. Spec: `RUST_REWRITE.md`; process: the venue port
brief on `origin/feat/rust-rewrite`. Full automatic execution, phase review by
sub-agent, commits at stable points. Target is USDT-M first, USDC shared paths;
inverse only where its sizing and collateral behavior can be verified.

## Phases

1. Public: borrowed wire records, streamed markets/tickers/candles, three
   ticker sockets, ref-counted books/candles, snapshot/update sequencing and
   checksum validation, reconnect and ping watchdog. Register crate and wasm.
   Verify bans, fmt, clippy and documented-payload public tests; sub-agent audit.
2. Private: exact REST/WS signing, Classic/UTA detection and subscriptions,
   typed account snapshots, position/settings mapping, failed-read preservation,
   terminal tombstones and fresh-WS guards, polling and event reconciliation.
   Verify signing, snapshot races, reconnect, empty/error responses; audit.
3. Trading: sequenced jobs, whole-batch client-id preflight, Classic batches,
   UTA orders/strategy orders, sized vs position stops, brackets/trailing,
   amend with fresh newClientOid, cancel and account settings. Never retry an
   ambiguous write. Verify payloads, partial failures and lifecycle; audit.
4. Queries: paged executions, order history, income and public trades as rare
   JSON command results. Bound ranges/cursors and dedupe executions. Audit.
5. Harness/gates: generic replay requests including JSON bodies and commands,
   documented private tape, real 20-second two-book public fixture, Bitget
   correctness/CPU/memory and Binance regression gates, wasm clippy/build,
   repository checks. Audit and record actual measurements.

## Risks and decisions

- Classic v2 public streams stay shared across Classic/UTA private accounts.
- A sized TPSL is clamped to the protected side, floored to the lot; zero is
  a full-position sentinel. Cancel uses immutable native plan type.
- Native modify-order sends both price and size plus a fresh client id.
- Successful error envelopes and malformed streamed records must not clear
  last-known-good account state.
- Books must not publish updates before a snapshot or across a sequence gap.
- TS cumulative fill events violate one execution per fill; any intentional
  correction needs an explicit gate entry and commit explanation.
- Keep TS available to the replay oracle until the migration integration decides
  deletion; never weaken parity or performance gates to make a tape pass.

## Progress

- Read the spec, existing engine/Binance pattern, Bitget plan-type and client-id
  guidance, TS Classic/UTA startup, public and trading behavior.
- Fetched origin and read the updated Process section. Existing lane is clean.
- Public implementation: crate + workspace/wasm/SDK/harness registration,
  USDT/USDC product selection, streamed contracts/tickers/candles, three public
  sockets, refcounts, sequence/checksum recovery, 5s ping / 20s watchdog.
- Public sub-agent audit found string sequence decoding, missing snapshot sides,
  pre-open ping, malformed WS candles, and REST mark-price parity issues. Fixed
  all; exact sequence, malformed-data and watchdog regressions are covered.
- Native public checks: bans PASS; fmt PASS; clippy all targets PASS; 11 tests
  PASS. Repository typecheck/lint/tests and wasm clippy are running.
- TS defect corrected: books previously applied gaps without validating modern
  seq/pseq (and legacy checksum). Register this in KNOWN_TS_FIXES at harness
  phase; normal tapes must still compare identically.
- Next: private state/signing and reconciliation.

## Final integration

After all gates: merge `origin/feat/rust-rewrite` into lane (resolve layout conflicts under standing authorization),
re-run affected checks, push, PR to `feat/rust-rewrite`, adversarial local
sub-agent reviews posted to PR, fix/review until clean or diminishing returns,
merge PR, create fresh follow-up branch. Never touch master or force/rebase.

- Workspace relocation: adopted landing members glob and moved Bitget to
  `sdk-rs/venues/bitget`; host uses its direct path dependency. Overseer gave
  standing authorization for landing-branch layout conflict resolution.
- No further stashes: use branch WIP commits. Private-phase stash is restored
  after the relocation merge. Runtime-tree wasm test failure is overseer-owned.

## Private phase checkpoint

- Exact REST/WS HMAC signing; Classic/UTA detection and late 40085 promotion;
  separate private socket authentication, subscriptions and watchdog.
- Streaming nested arrays retain small validated envelopes for cursors and
  business errors. Binance's original streaming form remains unchanged.
- Typed snapshots, settings, position mode, liquidation fallback, full-snapshot
  and per-position freshness guards, terminal-order tombstones, open-order
  grace, virtual-stop polling, account reconciliation and bounded fill caches.
- UTA fill/strategy topics, product filtering, dual strategy leg identity and
  status-less strategy snapshots reflect current documented native payloads.
- Sub-agent review findings fixed and covered by regressions: late promotion,
  old HTTP generations, socket replacement, UTA scope, execution semantics,
  missing strategy status, TP/SL ID collision, hedge-close direction, virtual
  refresh strategy preservation, funding fee alias, map pruning.
- Removed unused first-port `crypto/wallet_utils.rs` and its export; no native
  venue, host or engine caller exists. Retained the signing primitives.
- Bans/fmt/clippy pass; engine 12 tests, Bitget public 11 + private 15 tests pass.
- TS fixes are explicit in `harness/gate.mjs`'s KNOWN_TS_FIXES registry; normal
  Bitget tapes receive no mismatch normalization. Next: trading commands.

## Trading phase checkpoint

- Sequenced Classic/UTA write jobs: preflight caller IDs across the entire
  batch, native bracket and stop payloads, max-lot splitting, batch limits,
  immutable plan types, sibling-aware cancel, leverage/margin/mode settings.
- Native modify uses fresh newClientOid and both size/price. Resolve asynchronous
  identity through the new client ID, respecting terminal detail and fresh WS.
  Failed batch cancellations block replacement; acknowledged partial successes
  remain terminal. Only explicit known rejection permits cancel-and-replace.
- Unknown transport/server write outcomes are never retried or rolled back.
  Reads retry boundedly; signature rejection is re-signed once. UTA strategy
  replacement preserves both legs and rolls back only a known failed placement.
- Post-write private grace triggers Classic fallback and full REST polling;
  literal pongs do not count as authenticated payloads. Old settings snapshots
  cannot overwrite acknowledged leverage/margin/mode changes.
- Sub-agent review found and fixed partial-cancel handling, async modify identity,
  terminal/snapshot races, sibling cancellation, UTA client-only identity and
  full-position amend, lot-induced caller-ID splits, and partial-write health.
- Bans/fmt/clippy pass; public 11, private 15, trading 23 tests pass. TS defects
  recorded in KNOWN_TS_FIXES: lot splitting lost quantity, Classic cross wire
  spelling, stale modify identity, unsafe fallback after unknown write outcomes.

## Query phase plan

Stream typed fills/history/bills/trades lists into rare JSON results. Follow
native cursors even on short pages, dedupe identities, split accepted time
windows, bound page/window counts, and fail loudly on incomplete/error reads.
Classic hedge fills derive execution direction from position side + tradeSide;
UTA uses execution direction and the documented positive charged-fee convention.
Bill rows emit gross PnL and signed commission separately with distinct IDs.
UTA bills remain explicitly unsupported rather than calling incompatible v2.
Verify documented samples, huge IDs, fee signs, hedge closes, cursor cycles,
range boundaries, malformed pages and retry failure; sub-agent review afterward.

## Query phase checkpoint

- Streamed Classic/UTA fill and order history, Classic account bills, public
  trades, typed Binance-compatible JSON results. Native cursors continue across
  short pages; empty terminal pages end cleanly. Cycles/page caps fail loudly.
- Inclusive 30-day windows, identity dedupe, execution timestamps in seconds,
  order/income/public-trade timestamps in milliseconds. Classic hedge-close
  direction, signed rebates, and separate gross PnL / commission bill rows.
- Explicit UTA income unsupported error; no incompatible Classic fallback.
- Query sub-agent review fixed filtered malformed rows, invalid financial
  numbers, missing required Classic profit, unbounded detection wait, and
  query-triggered late UTA promotion. Shared promotion also re-plans explicitly
  rejected trading jobs before any accepted placements; its wait is bounded.
- Native checks: 62 tests (public 11, account 15, trading 24, queries 12),
  fmt/clippy/bans PASS. WASM clippy and release build PASS.
- Harness work started: strict JSON body comparison preserving caller IDs,
  subscription routing for same-URL sockets, passphrase options, documented
  Classic private tape. Initial Bitget private and both Binance gates PASS.
  Public 20-second two-symbol recording captured; fixture gate running.

## Harness checkpoint (gate execution continues)

- Captured the required 20-second BTCUSDT/ETHUSDT public stream; committed
  fixture candidate is its first six seconds, 36,631 replay frames, 846 markets
  and tickers, two books. Tick precision drift fixed by decimal contract-step
  decoding. All public outputs now compare identically; latest completed run
  still misses both CPU thresholds by about 6%, memory passes (2.1 vs 2.7 MB).
- Single-pass typed public router preserves reordered-envelope support and
  malformed-book recovery. Numeric visitor avoids untagged buffering. Exact
  fixed-decimal fast path (53-bit mantissa / exact power-of-ten divisor) falls
  back to standard float parsing for longer inputs/exponents; numeric tests and
  sub-agent arithmetic audit pass. Release build / public gate rerun pending.
- Strict shared replay transport: compare request JSON, preserve caller IDs,
  generated-ID presence/type/freshness, consume mutation replies exactly once,
  report omitted writes, route same-URL sockets by product/topic/symbol. Only
  documented Classic batch margin spelling normalizes. Five harness tests pass.
- Phase sub-agent audit found and fixed missing generated-ID checks, reusable
  write responses, omitted writes not reaching final JSON, missing product in
  routing, overly broad margin normalization, and malformed book data fallback.
- Stronger Bitget private gate PASS (latest 53.7/39.9 ms TS/Rust CPU); Binance
  private PASS and public PASS (218.8/115.6 ms, 2.4/2.2 MB) before the latest
  strict transport changes. Re-run all gates on the final integrated branch.
- Native 66 tests and fmt/clippy PASS. Repository checks running. Latest landing
  Process adds green CI before PR merge; enforce it. No push/PR until every gate.

## Landing integration checkpoint

- Public gate now PASS: 36,631 frames / 846 markets / 846 tickers / two books,
  TS/Rust CPU 562.6/486.1 ms, steady CPU 382.2/329.7 ms, retained 2.7/2.1 MB.
  Outputs identical. These are pre-integration measurements; final reruns follow.
- Merged landing 20dd66dddb, preserving nested workspace and Bybit registration,
  scripts, public/private fixtures, requestId canonicalization, and known TS
  fixes. Shared splitter combines Path + alternative Envelope paths, bounded
  metadata retention, whole-envelope validation and malformed-list rejection.
- Combined engine / Bitget / Binance / Bybit native tests PASS; harness 5 PASS.
  Integration sub-agent audit and full final gates still running. No PR yet.

## Final local gates

All venue gates now PASS on the integrated branch. Public numbers are medians
of three runs (Bybit regression: one run); private tapes gate correctness only.

| Gate | TS CPU ms | Rust CPU ms | TS steady ms | Rust steady ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bitget public | 592.2 | 556.8 | 401.4 | 373.1 | 2.7 / 2.0 |
| Binance public | 207.6 | 107.7 | 122.4 | 52.7 | 2.3 / 2.0 |
| Bybit public | 305.5 | 133.1 | 185.8 | 73.8 | 3.1 / 2.1 |

- Bitget private 11 frames, Binance private 14, Bybit private 18: PASS,
  exact account/command output, zero untaped or omitted writes.
- Bitget native 67 tests; workspace native tests and clippy; workspace fmt;
  bans; WASM clippy/build; harness 5; TS WASM integration 6; Worker host 2: PASS.
- Repository typecheck 44 tasks and lint 28 tasks PASS. Default full tests hit
  existing HyroTrader 5s cold-import timeouts (also with serial Turbo tasks).
  Equivalent safe-cex run with one worker / 30s timeout passes all 451 files,
  6,489 assertions; the other 25 repository test tasks passed normally.
- Fixed stale local workspace symlink to load `sdk-rs/hosts/wasm`, rather than
  the old ignored build location. Fresh CI installs use the new workspace path.
- Profile-driven optimizations remove envelope-string and singleton ticker-vector
  allocations, redundant contract mapping and non-native ticker aliases. Simple
  decimals use a bounded exact native division; broader grammar uses std parsing.
- Added strict harness tests and Bitget public/private CI steps. Combined wasm
  measures 1,892,956 bytes; fixed CI budget is 2 MiB (performance gates unchanged).
- Final phase sub-agent reviews are clear. Next: fetch/merge latest landing,
  push PR into feat/rust-rewrite, adversarial PR reviews/comments, green CI, merge,
  and create a fresh follow-up branch. Inverse sizing and pre-existing unsupported
  UTA bills/trailing remain outside this port's verified scope.

## OKX landing merge

- Integrated landing 693ad4f139 (OKX), retaining its staged HTTP snapshots,
  authentication/subscription validation, all-run comparisons, missing seed/book
  checks, fill-price comparison and replay deadlines. Shared tape transport keeps
  Bitget exact write bodies, consumed mutations, generated-ID validation and
  same-URL socket routing. All four venue registrations remain available.
- Sub-agent integration review caught misplaced parser test/helper and dropped
  in-item whitespace. Fixed both; added regression coverage for `[1 2]` so it
  cannot become `12`. Native workspace tests/clippy and WASM clippy pass.
- Four-venue WASM is 1,918,946 bytes, within unchanged 2 MiB CI budget.
- Post-merge venue replay regressions running before push; earlier measurements
  remain above for comparison. No gate threshold was relaxed.

## Post-OKX gates

Every replay gate passes on the final four-venue integration. Public medians use
three runs, except the single-run Bybit regression. CPU contention with other
lanes changed absolute timings; all comparisons retain the original thresholds.

| Gate | TS CPU ms | Rust CPU ms | TS steady ms | Rust steady ms | TS / Rust retained MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bitget public | 1752.0 | 1698.5 | 1264.7 | 1220.0 | 2.9 / 2.1 |
| Binance public | 160.8 | 71.0 | 88.4 | 27.1 | 2.3 / 2.0 |
| Bybit public | 164.5 | 146.1 | 113.7 | 86.7 | 3.1 / 2.3 |
| OKX public | 740.2 | 650.3 | 600.4 | 492.8 | 2.5 / 2.5 |

- Bitget 11, Binance 14, Bybit 18 private frames; OKX/OKXEU/OKXDemo/OKXEUDemo
  each 29 private frames: PASS exact account/commands and all taped writes.
- Public counts: Bitget 36,631 frames / 846 markets / 846 tickers / 2 books;
  Binance 871 / 917 / 783 / 3; Bybit 8,742 / 912 / 912 / 2;
  OKX 8,902 / 494 / 494 / 2. OKX retained memory ratio is 1.01x before rounding.
- Native workspace tests/clippy, engine 18, Bitget 67, harness 5, bans, fmt and
  WASM workspace clippy: PASS. Repository typecheck 44 and lint 28 tasks PASS.
- Landing is still 693ad4f139 after the final fetch. No extra merge is needed.

## PR checkpoint

- PR #1308 targets feat/rust-rewrite. Adversarial reviews are running across
  public/parser, account/trading and queries/harness, with findings to be posted.
- Post-merge safe-cex suite: 453 files / 6,494 tests PASS with one worker and
  30s cold-import timeout; WASM adapter 16 and Worker host 2 PASS.
- Remaining: review findings, green PR CI, merge into landing and fresh branch.


## Adversarial PR review, round 1

Eight findings posted on PR #1308; seven fixed and one rejected after venue-contract review:

- Hedge closes retain reduce-only intent through cancel-replace, including native
  close aliases and absent/NO reduceOnly (24 payload combinations).
- Rejected the proposed absent-algo removal: Bitget CLAUDE.md explicitly requires
  retaining WS TPSL because orders-plan-pending can omit live place-tpsl-order
  stops. Restored preservation and added regression coverage for this behavior.
- Default long-side isolated hedge leverage acknowledgement leaves short alone,
  including when an older REST positions snapshot completes afterward.
- Parentless client-only IDs containing `__` remain verbatim; virtual children
  resolve through explicit parent_id.
- Known UTA survives transient settings failures, retries at most three times,
  then reaches the existing detection deadline without falling back to Classic.
- Repeated/cyclic nonempty account cursors fail without committing partial state.
- Harness matches existing clientOid identities exactly, normalizes only minted
  fields, and binds generated aliases for later cancel/modify identities.
- Bitget/USDC private key+secret without a passphrase fails construction early;
  public-only and keyless construction retain their prior behavior.

Added native regression files for independently authored trading/account cases;
all focused tests pass. Final rebuild, replay and repeat audit in progress.

- Second adversarial review is clear after extending side-specific leverage
  through the stale REST guard and preserving the documented TPSL omission rule.
  Independently authored account 4 and trading 3 regression cases pass, including
  Classic/UTA delayed REST and 24 close-order payload combinations.
- Rebuilt WASM adapter 20 tests, harness 7, private replay and repository
  typecheck/lint PASS. Pre-fix PR CI is entirely green; the corrected commit
  requires its own green CI. Final public rerun remains in progress.

## Crypto.com landing integration

- Merged landing e6e410146e after PR became conflicting. Retained incoming
  Crypto.com crate/registration, crypto cleanup, request-signature/socket audits,
  exact large request IDs, POST-read versus mutation semantics, stable retained
  memory accounting and the real TS worker packer/receiver path.
- Strict Bitget factory now delegates to shared Responses with a matched-record
  hook; minted-ID aliases/freshness and multi-socket subscriptions remain intact.
  All 19 harness tests pass, including independent factory integration cases.
- Shared splitter retains both incoming malformed-item/envelope validation and
  Bitget alternative paths; engine 20 tests PASS. All private fixes remain.
- Previous public rerun was invalidated by the merge while child replay imports
  were in progress; rerun on the frozen integration before final reporting.

- Five-venue release WASM measures 2,164,645 bytes. The fixed bundle budget is
  2.25 MiB (2,359,296 bytes), accounting for the newly merged Crypto.com crate;
  CPU/memory replay thresholds remain unchanged. Workspace tests, native clippy
  and WASM clippy pass on this integration.

- Independent final integration review is clear: request checking precedes
  consumption, socket audits run once, write and ID contracts remain strict,
  real worker transfer and memory accounting survive extraction. PR mergeability
  restored; five-venue local replay regressions and current-head CI running.

## Memory baseline settling

- Incoming stable-memory guard rejected cold first TS runs: an already-loaded
  module's transient 240-byte buffer was released after two immediate turns,
  producing negative buffer retention despite identical output and lower Rust
  CPU/memory. Negative components still fail the gate.
- Baselines now require five stable samples across 10ms timer turns (bounded to
  50 attempts), equally for TS/Rust. Startup baseline is outside measured CPU;
  end CPU counters are captured before settling. No clamping/sample selection.
- Independent sub-agent review passes. Fresh cold TS sample retains +32 buffer
  bytes, rather than -208. Complete replay reruns are in progress.

## WEEX landing integration

- Integrated landing 6cd219ce4c when its merge made PR #1308 conflicting.
  Preserved incoming authenticated WS headers, timed reusable read templates,
  request-header checks, generated response placeholders and independent TS
  corrections. All six venue registrations/gates remain available.
- Preserved incoming all-mode complete-document validation, bounded 1MiB
  response metadata and collapsed-null validation envelope, with Bitget's
  alternative paths and business-error promotion hook. Engine23, Binance29,
  WEEX76 and Bitget75 native tests pass; harness29 tests pass.
- Kept the reviewed timer-turn baseline stabilization. Before this merge,
  Bitget's fresh three-run gate passed: TS/Rust CPU2557.2/2425.4ms,
  steady2068.5/1544.4ms, retained5.2/2.3MB. Regression reruns interrupted by
  changing imports during the merge are invalidated, not counted as passes.
- Six-venue rebuild/remeasurement and final regressions running; no thresholds
  on CPU/memory were relaxed. Current PR head must pass CI before merge.

- Six-venue WASM is 2,623,379 bytes; fixed aggregate budget2.75MiB
  (2,883,584bytes). Workspace native tests/clippy and WASM clippy pass.
- Final integration audit clear. Bitget tape routing now indexes the same
  product/channel/symbol tuple once at subscription time, avoiding a linear
  scan/rebuilt strings across846topics for every frame. Both implementations
  use the same transport and retain exact routing/unsubscribe checks.
- All29harness tests pass; six-venue replays running against frozen code.

## Final performance and measurement checkpoint

- Current PR revision passed every Linux Rust CI gate, including all six public
  and private venue gates. Local steady CPU margin was narrow, so profiled the
  actual fixture: typed ticker decoding was the largest venue cost.
- Compact typed ticker storage uses eleven floats and a presence mask, with
  an independent seen mask for duplicate rejection. Borrowed text, invalid/null
  values, signed zero, live/REST percentage precedence and seed mark stay exact.
  Native 77 tests and clippy pass. Profiling-only script/files were removed.
- Fresh three-run Bitget gate PASS: TS/Rust CPU 550.2/501.1 ms, steady CPU
  364.8/337.3 ms, retained 5.5/2.3 MB; same 36,631 frames, 846 markets/tickers
  and two books. Binance PASS: CPU 205.9/80.5 ms, steady 107.8/29.6 ms,
  retained 4.4/2.3 MB. Other regression reruns in progress.
- Measurement review found constructors were excluded unevenly. Both baselines
  now precede actual runtime/host/main-receiver construction, after imports and
  initial Tape state. Both heap and backing buffers must stabilize over timer
  turns; negative deltas still fail. Reproduced Crypto.com and WEEX private
  negative-memory cases pass with the consistent boundary. No thresholds relaxed.
- New BingX landing 9ddec5d0a4 requires another merge before PR can land.

## BingX landing integration

- Integrated landing 9ddec5d0a4 after it made the PR conflicting. Preserved
  incoming gzip/worker support, typed BingX requests, staged snapshot visibility,
  close/reconnect generations, seed subscription barriers and subscription oracles.
  Bitget retains exact JSON bodies, generated aliases and per-topic socket routing.
- Incoming private tapes gate correctness only; their unscored memory can shed
  imported code. Public memory still requires finite, nonnegative components and
  lower retained bytes. No public CPU/memory threshold changed.
- Engine 35, Bitget 77 and BingX 92 native tests PASS with --locked; WASM target
  check PASS. Harness 48 tests PASS. One earlier run exposed the incoming live-GC
  memory unit test's existing sensitivity; final rerun passed without changing it.
- Final measurement review now samples Wasm linear bytes after end settling,
  via a callback, alongside heap/backing buffers. This prevents timer-driven
  growth during settling from being missed.
- Seven-venue release rebuild, replay verification and bundle remeasurement are
  running before the next push. Earlier interrupted import runs are not passes.

- Seven-venue WASM is 3,049,383 bytes, under a fixed 3.25 MiB aggregate
  budget (3,407,872 bytes). Native workspace tests/clippy and WASM clippy pass.
- Fresh three-run Bitget public gate PASS: 36,631 frames, 846 markets and
  tickers, two books; TS/Rust CPU 816.7/788.2 ms, steady 569.4/548.7 ms,
  retained 5.9/2.5 MB; Rust linear memory 1.2 MB.
- Independent integration review clear: timed responses, single-use writes,
  exact routing/subscriptions, reconnect callbacks, audit delivery and settled
  memory boundaries checked. Final cross-venue regressions and current-head
  CI remain required before landing.

- Final Bybit private replay exposed active reconciliation timers preventing
  heap stabilization. Private tapes now report one GC/timer-turn sample rather
  than failing their correctness gate on unscored memory settling. Both engines
  use the same boundary; public stabilization and thresholds remain unchanged.

## MEXC landing integration

- Seven-venue revision 85b3ab2ef8 finished entirely green in GitHub CI:
  replay gate 6m56s, repository CI 8m52s/9m10s; all local public/private
  regressions passed. WEEX public TS/Rust CPU 144.7/83.5 ms, steady
  67.7/34.6 ms, retained 5.2/2.2 MB.
- MEXC landing 0018bba71a made the PR conflicting before merge. Integrated
  incoming crate, independent leverage rails, request/response frame additions,
  public replay clock and exact socket ordinals. Kept the incoming workspace
  structure and WASM size optimization. Both Bitget product registrations stay.
- Strict shared Tape factory retains generated-ID/write contracts, Bitget topic
  routing, timed responses and generation-tagged subscriptions. Incoming tape
  lifecycle/clock tests are preserved with the shared factory.
- Independent replay/gate and registration reviews clear. Eight-venue rebuild,
  native verification, replay reruns and current-head CI are required next.

## Reconciliation horizon and compiler-worker accounting

- Eight-venue WASM with incoming size optimization measures 2,821,624 bytes,
  below the unchanged 3.25 MiB budget. Workspace native tests/clippy, WASM
  clippy, repository typecheck/lint, WASM adapter and Worker tests pass.
- CI exposed a timing-only private mismatch: native amend tombstones its old
  ID immediately; TS retains it until its 3-RPS REST queue catches up. Both
  full snapshots remove absent terminal rows. Extended the private fixture to
  allow queued reconciliation, then establish an external live order through
  matching REST/WS state. Terminal behavior remains covered by native tests.
- TSX/esbuild sync transforms allocate 8-byte shared buffers in their compiler
  worker. Main-thread GC cannot establish that worker has released them. Gate
  subprocesses now set supported ESBUILD_WORKER_THREADS=0 for both engines
  before imports. Compilation remains outside measurement; no negative clamp,
  sample selection or public threshold relaxation. Independent review clear.
- Local public steady CPU margin narrowed under the incoming global WASM
  size optimization. Keep that aggregate build choice and optimize only the
  Bitget crate's streamed decoder and shared engine at level 3; measure the new artifact next.

- Targeted build measures 2,916,235 bytes, below unchanged 3,407,872 bytes.
  Fresh three-run Bitget public PASS: 36,634 frames (including three recorded
  socket opens), 846 markets/tickers and two books; TS/Rust CPU617.9/576.5ms,
  steady411.0/395.8ms, retained5.9/2.4MB. BingX, Binance and MEXC public PASS;
  remaining public regressions running on frozen source/artifact.
- All private variants PASS, including the final matching REST/WS live-order
  checkpoint for Bitget. Harness62, WASM adapter23 and Worker13 tests PASS.
  Final sub-agent reviews clear. Current pushed head must pass CI before merge.

## Final eight-venue local gate checkpoint

- All eight public gates PASS on the targeted artifact, with exact output and
  lower Rust main/steady CPU and retained memory. All private variants PASS.
- Final independent audit of 5300f4960f is clear; no threshold, output-oracle,
  identity or negative-memory relaxation. Fixed bundle budget stays unchanged.
- GitHub CI is required on the final pushed head before merge.

```text
bitget: main-thread cpu  ts    617.9 ms   rust    576.5 ms   1.07x; steady cpu       ts    411.0 ms   rust    395.8 ms   1.04x; retained memory  ts      5.9 MB   rust      2.4 MB   2.48x
binance: main-thread cpu  ts    180.2 ms   rust     84.0 ms   2.14x; steady cpu       ts     94.2 ms   rust     34.3 ms   2.75x; retained memory  ts      4.4 MB   rust      2.2 MB   2.03x
mexc: main-thread cpu  ts    490.6 ms   rust    200.8 ms   2.44x; steady cpu       ts    277.1 ms   rust    107.0 ms   2.59x; retained memory  ts     10.3 MB   rust      3.4 MB   3.06x
bybit: main-thread cpu  ts    214.3 ms   rust    191.2 ms   1.12x; steady cpu       ts    116.6 ms   rust    100.3 ms   1.16x; retained memory  ts      6.5 MB   rust      2.3 MB   2.80x
bingx: main-thread cpu  ts    113.0 ms   rust     85.8 ms   1.32x; steady cpu       ts     44.8 ms   rust     40.9 ms   1.10x; retained memory  ts      3.8 MB   rust      2.0 MB   1.90x
cryptocom: main-thread cpu  ts    108.6 ms   rust     72.4 ms   1.50x; steady cpu       ts     47.6 ms   rust     31.0 ms   1.54x; retained memory  ts      2.2 MB   rust      1.3 MB   1.62x
okx: main-thread cpu  ts    572.1 ms   rust    495.8 ms   1.15x; steady cpu       ts    421.1 ms   rust    358.1 ms   1.18x; retained memory  ts      5.4 MB   rust      3.4 MB   1.58x
weex: main-thread cpu  ts    120.1 ms   rust     79.9 ms   1.50x; steady cpu       ts     56.1 ms   rust     33.3 ms   1.68x; retained memory  ts      5.3 MB   rust      2.1 MB   2.53x
```

## BloFin landing integration

- Eight-venue head278883674f finished its entire GitHub replay job green.
  Actual landing ref advanced to d9db55921c (BloFin + MEXC parity follow-up)
  while CI ran. GitHub's PR baseRefOid remained stale; use the actual branch
  ref when diagnosing a reported conflict. Merged the new landing normally.
- Preserved incoming BloFin registrations, request defaults/cache-busters and
  native numeric/string IDs, independent leverage and margin-only frame fields,
  WebSocket request oracles, MEXC fixture/history/trailing/income regressions,
  and its size-oriented crate profiles. Bitget/engine stay optimized at level3.
- Shared strict factory adds recursive caller-ID collection, BloFin typed keys
  and normalized opt-in WebSocket request logs; exact Bitget alias/path checks
  and subscription routing remain. All70harness tests PASS.
- Incoming retainedMemory formula is unchanged; kept our stronger public heap
  plus buffer stabilization, private informational sampling, post-settle WASM
  callback and compiler-worker exclusion. No public threshold relaxation.
- Registration, parent replay/gate/Cargo and factory reviews are all clear.
  Nine-venue WASM3,083,413bytes < unchanged3,407,872byte budget. Native and
  replay regressions plus current-head CI remain required before merge.

## Nine-venue throughput checkpoint

- Native workspace, native/WASM clippy/fmt/bans, repository typecheck/lint,
  WASM adapter24 and Worker/wire15 tests PASS. Harness70 tests PASS.
  All private variants PASS, including BloFin and MEXC's updated fixture.
- Local final-dispatch size profiles had unstable CPU margins: level z failed
  Crypto.com; balanced s failed Bitget. Natural machine contention prevents
  assigning isolated causation. Chose throughput optimization for the final
  WASM host while keeping incoming venue z overrides and engine/Bitget3.
- Resulting artifact3,202,529bytes remains below fixed3,407,872bytes. Fresh
  Bitget, Crypto.com and Binance three-run public gates PASS. Bitget TS/Rust
  CPU713.4/632.4ms, steady458.7/440.8ms, retained5.9/2.5MB; remaining
  public regressions and final-head CI required before merge.

## Backpack landing integration

- All nine public and private gates passed before merging Backpack landing
  b265a0bab0. Preserve incoming host abort handling, subscription/signature
  verification, typed candle/time units and independent execution checks.
- Kept one strict shared transport. Added credential injection, pre-consumption
  signature and global write-order checks, Backpack stream routing, exact
  decimal/ID matching and monotonic virtual timer pumping. All113harness
  tests PASS; an earlier run hit the known unrelated live-GC memory unit test
  sensitivity before the final run passed without changing memory checks.
- Sub-agent merge audit found missing Backpack client-ID/time-in-force and
  perpetual-market-filter comparisons. Restored both exactly as incoming;
  follow-up parent and frozen cross-file reviews are clear. Registration and
  factory reviews are clear. Bitget path-scoped generated aliases, identities,
  same-URL subscriptions, stronger constructor baselines and compiler-worker
  exclusion remain. No CPU or memory threshold change.
- Ten-venue WASM3,497,553bytes (Backpack adds295,024bytes); fixed aggregate
  budget becomes3.5MiB(3,670,016bytes) to cover the added crate. Ten-venue
  native/replay checks and final-head GitHub CI are required before merge.
