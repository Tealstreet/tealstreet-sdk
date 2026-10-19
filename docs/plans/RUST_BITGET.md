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
