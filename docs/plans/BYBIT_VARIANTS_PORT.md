# Bybit variants port

Branch: `feat/rust-bybit-variants`, based on `origin/feat/rust-rewrite`.
Landing target: `feat/rust-rewrite`; merge only after all PR CI is green.

## Baseline

PR [#1298](https://github.com/Tealstreet/tealstreet-sdk/pull/1298) was watched
to completion before implementation. All six Rust checks, both TS CI runs
(5m49s and 8m56s), and both secret scans passed. No repair PR was necessary.

## Phases

1. Public/config: one `Variant` option and paths table in the existing Bybit
   crate; inverse USD contracts, spot precision/depth, demo routing, explicit
   Hyro endpoints. Preserve socket watchdogs, pagination and startup guards.
2. Private: category filtering, inverse coin/base/quote normalization, spot
   balances without derivative positions, and Hyro's correlated handshake,
   atomic reconciliation, readiness and session/snapshot guards.
3. Trading/queries: native inverse quantities, spot single writes and supported
   conditional shapes, demo routing, Hyro identity-preserving recovery; retain
   client-ID and reads-only retry contracts.
4. Integration/gates: register variants in wasm, TS shell and harness; documented
   private tapes and live public fixtures; run every venue-brief gate, Binance
   regressions, root typecheck/lint/tests and adversarial phase/PR reviews.
5. Push/open PR into `feat/rust-rewrite`, post findings, fix/review until passing
   or diminishing returns, wait for green CI, merge and cut a fresh branch.

## Review and verification

Each implementation phase and the opened PR received local sub-agent audits.
Final integration gates and CI are recorded below before merge.

Public tests and private tapes derive from current Bybit V5 documentation and
the repository's documented contracts. Hyro uses the October 6 simulator subset
documented in the Bybit CLAUDE.md; endpoints are explicit configuration and
credentials never come from simulator defaults.

## Implementation checkpoint

Public/config, private, trading/queries and native-unit frame transport are
implemented and cross-reviewed locally. Public/config has 6 native cases,
private variants 18, trading/recovery 31, plus the original 43 Bybit cases.
The shell has 33 wasm integration tests and 4 independent binary/copier tests.
Root typecheck (44 tasks) and lint (28 tasks) pass. Final integrated gates are
in progress; the landing branch's new OKX port will be merged before final checks.

Review fixes include strict Hyro URL/row validation; correlated handshake and
atomic stale-safe snapshots; lost/duplicate create recovery; snapshot/WS fences;
Demo REST acknowledgement publication; exact inverse native units across frames;
and Hyro order-only incremental fills with reconnect snapshot seeding.

TS defects fixed: inverse quantity clamp/truncation before splitting; inverse
coin PnL summed into USD balance; spot instrument precision/limits and invalid
pagination; market-buy quote/base ambiguity; derivative-only spot controls and
position-stop endpoints; spot cancellation families and `tpslOrder` mapping;
`PartiallyFilledCanceled` events ignored; Hyro historical partial fills replayed
when an unseen partial order resumes after a snapshot. Each is listed in `gate.mjs`'s `KNOWN_TS_FIXES` comments.

Live 20-second captures for inverse/spot/demo are committed as trimmed fixtures.
Hyro origins are not configured in this environment: its public fixture is
explicitly documented/synthetic, and live Hyro connectivity remains unverified.
All private fixtures use documented payloads without credentials. Classic
CONTRACT-wallet valuation without aggregate USD fields retains the pre-existing
TS limitation; these account gates cover Unified accounts. Ambiguous Hyro
position-stop writes are reported/fenced because that endpoint has no client ID
with which to authorize a safe automatic replacement.

The harness now includes module allocations symmetrically in retained memory
(the prior baseline excluded TS imports but counted all wasm memory), compares
market flags, isolates concurrent output files and fails bounded I/O/readiness
waits. No correctness or performance threshold was relaxed.

## First integrated gates (superseded measurements)

Before final review fixes and the Crypto.com integration, integrated with the green OKX landing branch at `693ad4f139`; all gates passed:

- Banned patterns; workspace fmt; strict Bybit and wasm-target Clippy.
- Bybit 89 native tests, engine 15, Binance 29; OKX trading 21.
- Release wasm 1,568,090 bytes, below the 1.6 MiB aggregate budget.
- Wasm shell 22 tests; Workers host 2; independent inverse frame/copier 4.
- Root typecheck 44 tasks, lint 28, licenses 31 workspaces. Final root test
  run: 27 tasks passed, safe-cex 454 files / 6,498 tests passed in each run.
  Local tests used two workers and a 30-second default timeout under shared
  machine load; no repository assertion or timeout was changed.
- All 17 replays passed: four variant public and private pairs, Binance and
  Bybit linear public/private, OKX public plus all four OKX private variants.
  Each replay run checks every requested book, seeded market/ticker state,
  command outputs, and consumes every taped write.

Median of three public replays (main-thread CPU, retained memory):

| Venue | Frames | TS CPU ms | Rust CPU ms | TS MB | Rust MB |
| --- | ---: | ---: | ---: | ---: | ---: |
| Bybit inverse | 707 | 103.2 | 27.7 | 11.8 | 1.3 |
| Bybit spot | 4,338 | 100.0 | 77.5 | 11.9 | 1.6 |
| Bybit demo | 5,832 | 137.0 | 99.8 | 14.3 | 2.6 |
| Hyro documented public contract | 160 | 18.6 | 16.0 | 11.8 | 0.8 |
| Bybit linear regression | 8,742 | 166.9 | 131.3 | 14.8 | 2.6 |
| Binance regression | 871 | 175.7 | 73.1 | 12.7 | 2.1 |
| OKX regression | 8,902 | 615.3 | 538.0 | 12.9 | 2.7 |

Private correctness replay frame counts: inverse 14, spot 11, demo 14, Hyro 13,
Bybit linear 18, Binance 14 and each OKX variant 29. Private tapes are paced
and do not gate performance. Hyro numbers validate the documented fixture;
they are not a claim of live simulator connectivity.

## Final adversarial review

PR [#1307](https://github.com/Tealstreet/tealstreet-sdk/pull/1307).
[Findings and resolutions](https://github.com/Tealstreet/tealstreet-sdk/pull/1307#issuecomment-6273094487)
are posted on the PR. All six confirmed blockers were fixed:

- Spot cancel-all covers all three native families and retains failed families.
- Demo acknowledgement publication respects newer snapshots/private sessions.
- Hyro retries key validated original caller intent before dynamic formatting;
  original lots, payload and identity survive price/position changes. This also
  fixes a reproduced TS bug: its inner recovery key uses re-rendered payloads.
- Healthy Hyro snapshots preserve unreported fill deltas. Absent orders retain
  bounded recent cursors; recovery snapshots seed terminal duplicate protection.
- Non-finite tick slippage protection fails the whole batch before JSON/I/O.

Independent follow-up reviews pass: 31 trading cases, 20 state reproducers and
additional cases, and all three non-finite wasm preflight cases with zero writes.
Native suite now has 98 Bybit cases plus 15 engine cases before Crypto.com merge.

The first push scan found Binance's public documentation signing vector in an
inherited commit. One exact historical fingerprint is exempted in
`.gitleaksignore`; the complete `origin/master..HEAD` scan reports zero findings.
No secret rule or production-source path was broadly exempted.

Crypto.com landed concurrently at `c63c3e5670` and is integrated before the final
push. Its typed protocol/request audits, stable heap + ArrayBuffer + wasm memory
measurement, production TS store transport, and CPU sampling before forced GC
are preserved. Both implementations still start memory measurement before module
imports. These corrected measurements supersede the earlier table.
All four venues and all Bybit/OKX variants are revalidated together. Integration
also restores Crypto.com shell caller-ID helpers and rejects Bybit-only execution
controls before any Crypto.com writes; both regressions were proved failing first.
A final adjacent JSON check also rejects non-finite entry and protective numbers
before transport, preventing a requested bracket from becoming an omitted field.

## Four-venue corrected public gates (superseded)

All eight public gates PASS (median of three). These measurements supersede all
prior tables and include both startup and steady CPU gates:

| Venue | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| bybitinverse | 707 | 178.9 / 61.4 | 126.5 / 32.3 | 16.1 / 1.4 |
| bybitspot | 4,338 | 262.3 / 178.0 | 165.3 / 100.2 | 16.8 / 1.8 |
| bybitdemo | 5,832 | 367.9 / 250.8 | 240.9 / 147.0 | 20.9 / 3.0 |
| hyrotrader | 160 | 33.6 / 24.0 | 10.0 / 6.7 | 15.7 / 0.9 |
| binance | 871 | 253.5 / 104.2 | 142.6 / 47.3 | 18.4 / 2.6 |
| bybit | 8,742 | 478.8 / 325.7 | 300.4 / 194.4 | 21.5 / 2.9 |
| cryptocom | 2,177 | 197.9 / 140.6 | 125.2 / 80.3 | 14.5 / 1.6 |
| okx | 8,902 | 1136.5 / 1055.2 | 865.9 / 743.0 | 18.2 / 2.9 |

The integrated four-venue wasm is 1,821,884 bytes under the landing branch's
1.8 MiB budget. No size or performance threshold was relaxed for this integration.

## Final local validation

- Workspace bans, fmt, strict all-target Clippy, native tests and wasm-target
  Clippy PASS. Bybit 98, engine 17, Binance 29, OKX 65, Crypto.com 43 native
  cases pass; Crypto.com includes the new vendor-control guard.
- Wasm shell 31 tests, Workers 2, independent inverse frame/copier 4, and
  harness helpers 10 pass. Non-finite numeric preflight was independently
  re-reviewed, including all five new wasm cases. Per-implementation fixture
  selection was independently reviewed; exact bodies, single-use mutations,
  unknown-tag rejection and unused selected-write failures remain enforced.
- Root typecheck 44 tasks, lint 28, licenses 31 workspaces PASS. Final root
  tests: 27/27 tasks; safe-cex 454 files / 6,498 tests in each run. Package
  typecheck/lint also pass after the final shell preflight change.
- Spot private tape documents the TS single unfiltered cancellation and Rust's
  three native family writes with explicit implementation tags. Both consume
  every selected mutation. Frames and state comparisons remain shared.

All 19 final replay gates PASS: eight public performance/correctness replays and
11 private correctness replays. Private frame counts: inverse14, spot11, demo14,
Hyro13, Binance14, Bybit linear18, Crypto.com21 and each OKX variant29. No selected
write is unmatched, reused or left unused. Final independent numeric-preflight
and fixture-selection review passes with no remaining confirmed blocker.

Final CI must be green before merge.

## Five-venue landing integration

WEEX landed concurrently at `6cd219ce4c`; merge `18568fec88` preserves its
header-capable sockets, active ticker scope, streamed response metadata,
independent conditional oracle and stricter validation of every memory component.
The Bybit endpoints, readiness, inverse frame units and exact scoped write plans
remain intact. WEEX shell caller-ID helpers are registered and its planner rejects
Bybit-only BBO/TickSize controls before any write. Independent integration reviews
pass without remaining confirmed findings.

Workspace native tests, strict Clippy, bans, fmt and wasm Clippy pass: Bybit 98,
engine 22, Binance 29, OKX 66, Crypto.com 43, WEEX 77 cases. Shell wasm 33, host/Workers 7,
and harness 15 pass. Root typecheck 44, lint 28 and licenses 31 pass after integration;
the prior complete root test run passed 27 tasks / 6,498 safe-cex tests in each run.
The combined artifact is 2,285,097 bytes under the landing branch's existing
2.25 MiB budget. No performance threshold was relaxed.

All nine public correctness/performance gates have passing complete three-run
results. Hyro's small documented 160-frame fixture had one steady-CPU miss
(TS 9.1 / Rust 11.5 ms); two complete unchanged three-run confirmations passed
(TS 5.5 / Rust 3.9 ms, then 9.9 / 6.4 ms). Every sample and the initial failure were retained;
no source, fixture, threshold or outlier selection changed. The table uses the
last complete run. Live Hyro connectivity remains unverified.

| Venue | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| bybitinverse | 707 | 183.2 / 66.8 | 122.8 / 32.2 | 16.0 / 1.4 |
| bybitspot | 4,338 | 291.1 / 223.6 | 190.5 / 144.7 | 16.8 / 1.8 |
| bybitdemo | 5,832 | 360.3 / 228.8 | 214.6 / 131.0 | 20.9 / 3.0 |
| hyrotrader | 160 | 31.3 / 20.7 | 9.9 / 6.4 | 15.7 / 0.9 |
| binance | 871 | 203.9 / 74.5 | 113.8 / 26.8 | 18.4 / 2.6 |
| bybit | 8,742 | 199.6 / 135.2 | 123.8 / 65.6 | 21.5 / 2.9 |
| cryptocom | 2,177 | 81.5 / 64.0 | 46.1 / 25.1 | 14.5 / 1.6 |
| okx | 8,902 | 583.8 / 521.4 | 435.6 / 387.8 | 18.2 / 2.8 |
| weex | 2,028 | 125.2 / 77.0 | 59.5 / 31.3 | 18.3 / 2.8 |

All 21 replay gates PASS: nine public performance/correctness gates and twelve
private correctness gates, including WEEX 20 frames. Final-head CI is the remaining
landing check.

## CI performance follow-up

Head `633d5eb8d7` passed both complete root CI runs, secret scans, native
fmt/Clippy/tests, wasm32 and bans. The replay job failed only the documented
Hyro public fixture's steady CPU gate: TS 2.0 ms / Rust 2.2 ms. Total main CPU
was TS 17.4 ms / Rust 13.2 ms; memory was TS 15.5 MB / Rust 0.9 MB.
This supersedes the earlier local passing result for merge readiness.

The follow-up removes generator allocation from `Frame.first` and makes
`wsMessage` return `undefined` when no actions exist, bypassing empty UTF-8
decoding/freeing in generated bindings. Nonempty actions retain their order and
errors. Both declarations match; an actual wasm ticker/error regression passed
against the rebuilt artifact. All 34 wasm tests, 7 host/Workers tests, 4 inverse
frame/copier checks, root typecheck (44 tasks), lint (28 tasks), bans, fmt and
wasm Clippy pass. Independent shared-boundary review found no blocker.

Borrowing public decimal strings and avoiding temporary numeric book copies
also reduced parser allocations, with escaped-string fallback and malformed
payload behavior independently checked. These changes alone still missed the
small-fixture steady CPU gate locally; all failed results are retained. Further
optimization and integration with the concurrent BingX/MEXC landing remain in
progress. No fixture, performance threshold or sample selection was changed.

## Seven-venue integration checkpoint

Merge `63d521a860` adds the landed BingX and MEXC ports. Preserve their binary
socket support, timed replay clock, connection generations, request/subscription
audits, full position scopes, leverage rails and existing 2.5 MiB wasm budget.
The shared replay baseline remains before implementation imports, with bounded
backing-buffer quiescence and every memory component validated on private tapes
too. Exact spot write-plan selection and Hyro's taped readiness/ACK correlation
remain explicit. All 42 harness helper tests and the TS Hyro private replay pass.

The final Bybit decoder reuses two typed numeric side buffers and validates the
entire frame before changing a book. All 105 native Bybit tests pass; independent
review passed 266 object-frame comparisons. Undocumented array-shaped struct
representations now reject; documented object frames preserve validation and
numeric behavior. The public fixture and CPU/memory thresholds remain unchanged.

Integration review fixes: BingX/MEXC reject Bybit-only controls for an entire
batch before writes (both reproduced red; 93/52 native tests pass); MEXC consumes
an exact native-leaves column when present and retains legacy reconstruction as
fallback; keyless MEXC metadata reads respect inferred public-only mode; optional
caller identity is omitted on serialization so Rust's own option round-trip
continues to work with the landed strict-null rejection. Full integrated build,
replays and final-head CI remain required before merge.

## Final seven-venue local gates

All 25 replay gates PASS: eleven public correctness/performance medians of three,
and fourteen private correctness replays, including every Bybit profile, Binance,
BingX, MEXC, Crypto.com, WEEX and all four OKX aliases. Current frame counts also
include the landing harness's recorded connection events. Public fixtures and
performance thresholds are unchanged.

| Venue | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| bybitinverse | 708 | 109.6 / 31.5 | 71.7 / 13.0 | 15.8 / 1.3 |
| bybitspot | 4339 | 128.3 / 93.3 | 74.4 / 40.7 | 16.7 / 1.9 |
| bybitdemo | 5833 | 196.2 / 139.7 | 98.2 / 57.3 | 21.0 / 2.9 |
| hyrotrader | 160 | 35.3 / 24.0 | 11.0 / 9.1 | 15.4 / 0.9 |
| binance | 873 | 168.9 / 76.4 | 90.8 / 30.0 | 18.1 / 2.5 |
| bybit | 8743 | 236.1 / 216.5 | 128.6 / 105.8 | 21.5 / 2.9 |
| cryptocom | 2178 | 245.5 / 158.8 | 150.2 / 93.8 | 14.4 / 1.7 |
| okx | 8904 | 1353.6 / 1174.0 | 1020.6 / 861.9 | 19.1 / 3.8 |
| weex | 2029 | 254.0 / 176.0 | 152.6 / 107.7 | 18.1 / 2.5 |
| bingx | 189 | 134.5 / 91.7 | 59.5 / 44.6 | 17.6 / 2.3 |
| mexc | 14701 | 994.4 / 523.5 | 644.4 / 307.8 | 23.7 / 3.9 |

Native workspace tests, strict all-target Clippy, bans, fmt, wasm-target Clippy
and the release wasm build PASS. Native counts: Bybit 105, engine 39, Binance 29,
BingX 93, MEXC 52, Crypto.com 43, OKX 66 and WEEX 77. Rebuilt wasm shell 37,
host/socket/Workers 13, inverse/MEXC frame/copier 8 and harness 46 tests pass.
Root typecheck 44 tasks, lint 28 and licenses 31 workspaces pass. The artifact is
2,539,883 bytes, within the landing branch's unchanged 2.5 MiB budget.

The landed subscription audit initially reported inverse/spot generic TS topics
versus category-qualified Rust topics. Bybit documents both alternatives. A
TS-only oracle mapping at the taped private origin now recognizes those exact
alternatives; Rust topics, origins, operations and generations remain exact.
Independent adversarial review passed, including negative comparisons for each
wrong-scope case and malformed collection validation. This is an API-equivalence
check, not an additional TS bug.

Independent reviews pass for the complete decoder, shared wasm boundary, venue
registries, inverse/MEXC units, leverage rails, native execution-control guards,
client-identity serialization and keyless metadata guard. Findings and resolutions
are posted on PR #1307. Final-head CI remains required before merge. Live Hyro
connectivity is still unverified; no credentials or endpoints are configured here.

## BloFin landing integration checkpoint

All checks on `5ba51ec854` passed: both full repository CI jobs (8m59s and
5m52s), replay CI (7m22s), native fmt/Clippy/tests, wasm32, bans and secret scans.
Hyro's unchanged CI fixture passed total CPU TS 14.6 / Rust 10.6 ms and a narrow
steady-CPU comparison (rounded 2.8 / 2.8 ms, ratio 1.01x). No failing run was
retried to obtain this result; the source allocation fixes were tested as a new
candidate.

BloFin landed at `d9db55921c` while CI ran, creating conflicts. Integrate its
typed request hooks, authenticated socket audit, native settings and fixed
MEXC parity tape. All 53 merged harness helper tests pass; native workspace
tests pass (Bybit 105, engine 45, BloFin 68, MEXC 54 plus the prior venues).
BloFin's planner now rejects an entire batch with Bybit-only controls before
writes, with a reproduced regression. Its native order extras also need both
worker and main frame appliers enabled; the real wasm boundary regression
reproduced missing extras before that fix.

The combined artifact at the inherited `s` optimization was 2,689,402 bytes,
above the unchanged 2.5 MiB budget. The build script now selects LLVM `z`
optimization for wasm only, using the existing pinned toolchain and no new
dependency. The resulting artifact is 2,582,022 bytes. Full functional and
27 replay gates must pass on that artifact before pushing the integrated head,
followed by green final-head CI again. Fixtures and thresholds remain unchanged.

## Backpack landing integration checkpoint

The BloFin integration passed all 12 public replays and 11 completed private
replays before Backpack landed at `b265a0bab0`; the remaining OKX private runs
were interrupted by the next merge and are not claimed as passing. The corrected
BloFin private script uses the root `rust:private-tape:blofin` alias.

Backpack's landing extracts Tape from replay. Preserve strict implementation
filtering for both direct and typed response stores, typed matching for existing
venues, Backpack's signed requests and ordered mutations, and Hyro's two-phase
handshake. All 84 harness regressions pass, including extraction-specific cases.
The shared planner rejects unsupported Bybit controls and non-finite numeric
protections before JSON serialization. Backpack's native planner now also rejects
an entire batch with Bybit-only controls; its 50 native tests pass. The shell
client-ID policy rejects close orders consistently with the native planner.

The rebuilt nine-venue wasm is 2,844,905 bytes at LLVM `z`. The 3 MiB budget comes
from the Backpack landing unchanged. The complete matrix is now 29 replay gates
(13 public, 16 private); final validation and green final-head CI remain required
before merge. Hyro live connectivity remains unverified.

Independent extraction review caught a missing `stable` import at the gate's
market comparator. The first replay reproduced the ReferenceError; restore the
import and rerun the complete matrix. A new integration test executes the actual
gate CLI with controlled replay outputs across every venue's public/private
oracle, including populated comparisons, independent mismatches and invalid
memory components on a later private run. Removing the import in a temporary
copy proves this test fails. All 87 harness tests and the undefined-symbol audit
pass; a second adversarial extraction review found no further defects.

Nine-venue functional checks pass: 659 native workspace cases (Bybit 105,
engine 46, Backpack 50), 39 wasm boundary cases, 25 host/socket/Workers cases and
eight frame/copier cases. Workspace bans, fmt, strict all-target Clippy,
wasm Clippy, typecheck (44 tasks), lint (28 tasks), licenses (31 workspaces)
and the lane commit secret scan pass. Final replay results and CI remain pending.

The global `z` build then failed Hyro CPU: total TS/Rust 24.6/26.7 ms and
steady 6.4/8.6 ms. An explicit engine O3 override improved total to 18.7/18.0 ms
but still failed steady at 5.4/6.1 ms. Both failed runs are retained. The next
source candidate compiles the engine and Bybit feeds at O3, reuses engine
book/fill vector capacities after encoding, removes per-delta ticker arrays,
and decodes book headers without a nested iterator or temporary typed view.
Review and native/host regressions pass; the new wasm is 2,998,620 bytes under
the inherited 3 MiB budget. Hyro's first full gate on that candidate passes:
total 30.6/25.1 ms, steady 10.4/10.3 ms (a narrow margin), memory 15.5/1.0 MB.
No fixture, CPU window, threshold or median-selection rule changed. The full
matrix and final-head CI are still required on this candidate.

That candidate completed 28/29 gates: all 16 private and 12 public gates pass,
but BingX steady CPU failed at TS/Rust 32.0/35.2 ms. Restoring the landing
branch's `s` dependency build, while retaining explicit engine/Bybit O3 and
the allocation fixes, produces 3,117,529 bytes within the inherited 3 MiB
budget. The full matrix is being rerun with that artifact, beginning with Hyro
and BingX. All earlier failures remain recorded; fixtures and gates are exact.

## Nine-venue passing checkpoint before Bitget

All 29 gates PASS (13 public medians of three; 16 private), on the final
3,117,529-byte artifact and `0d3ba72e58` runtime source. All 39 wasm boundary
cases and 28 host/Workers cases pass again on that artifact; 87 harness and
eight frame/copier regressions pass. The final independent adversarial review
of buffer lifetimes, transferred ownership, book ordering, ticker masks and
profile precedence passes. No further defects remain from that review.

| Venue | Frames | Main CPU TS / Rust ms | Steady CPU TS / Rust ms | Retained TS / Rust MB |
| --- | ---: | ---: | ---: | ---: |
| bybitinverse | 708 | 130.2 / 35.1 | 82.3 / 15.9 | 15.9 / 1.5 |
| bybitspot | 4339 | 126.4 / 90.1 | 73.0 / 41.9 | 16.9 / 2.1 |
| bybitdemo | 5833 | 175.2 / 111.4 | 98.5 / 43.8 | 21.1 / 3.0 |
| hyrotrader | 160 | 29.1 / 21.4 | 11.6 / 5.0 | 15.5 / 1.0 |
| binance | 873 | 176.5 / 73.2 | 91.6 / 25.4 | 18.2 / 2.7 |
| bybit | 8743 | 214.9 / 141.2 | 120.9 / 64.0 | 21.7 / 3.0 |
| backpack | 1087 | 122.4 / 48.8 | 59.8 / 20.0 | 16.4 / 1.8 |
| cryptocom | 2178 | 178.0 / 148.5 | 118.3 / 75.3 | 14.5 / 1.9 |
| blofin | 2765 | 116.7 / 89.1 | 67.9 / 42.3 | 17.1 / 2.3 |
| okx | 8904 | 670.4 / 617.5 | 498.1 / 455.1 | 19.2 / 3.9 |
| weex | 2029 | 120.6 / 84.2 | 57.7 / 37.2 | 18.2 / 2.6 |
| bingx | 189 | 151.0 / 87.1 | 68.9 / 38.7 | 17.7 / 2.5 |
| mexc | 14701 | 464.1 / 205.2 | 271.1 / 106.7 | 23.6 / 3.3 |

Bitget landed at `3a7b46407d` before the final push. Integrate its shared
harness/host changes and rerun required gates and final-head CI before merge.

## Bitget integration checkpoint

Merged the tenth venue at `3a7b46407d`, preserving its `createTape` factory,
pre-consumption write/signature audits, generated-ID aliases, topic routing and
stronger symmetric constructor CPU window. Keep before-import settled memory,
CPU capture before final GC, all four finite/nonnegative components on every
run, strict implementation filtering, typed request hooks and Hyro readiness.
All 127 harness tests (including every prior test name) pass; undefined-symbol
and independent extraction audits pass.

The new Bitget planner rejects Bybit-only controls before any batch write and
its amendment options include the new fields. All 78 native Bitget cases pass,
including a proved red/green Classic/UTA regression. The shared shell now honors
Bitget/BitgetUSDC caller IDs; both aliases reproduced the missing-rule failure
before the fix and pass inherited-helper regressions.

Independent review also found named array paths accepting an array element as
an object field in the new shared splitter. Bybit rejects that malformed root
envelope later; no account corruption was demonstrated. Track seeking container
kinds so named paths require object parents while explicit root arrays remain
supported. The regression is proved red/green, all 50 engine tests and strict
Clippy pass, and a second independent review passes valid Bitget/Bybit paths and
chunk boundaries.

The all-O3 hot-path build was 3,724,964 bytes, over the inherited 3,670,016-byte
budget. O2 dispatch only reduced that to 3,721,305 bytes. Keep incoming dispatch,
engine and Bitget O3, and use Bybit's `s` profile with the source allocation fixes.
The resulting artifact is 3,658,339 bytes, within the unchanged inherited 3.5 MiB
budget. The full matrix is now 31 gates (14 public, 17 private) and is running;
final-head CI must be green before merge. No fixture or gate threshold changed.

The first ten-venue replay stopped at TS memory settlement before producing
metrics. The exact cause of that untraced failure is unproven. Cold-transform
diagnostics found compiler-owned 8-byte shared buffers with esbuild's worker
enabled (at least 100 allocations, 31 retained), and none with it disabled.
Both warm controls settled; these diagnostic runs were not accepted gate retries.
Bitget's checkpoint intended `ESBUILD_WORKER_THREADS=0` but its actual runner
omitted it. Set that supported option before imports for both gate subprocesses
and direct replay. Implementation modules remain included symmetrically. The
100 ms quiet window, one-second deadline, four validity checks and performance
comparisons are unchanged. Timeout errors now expose sample/transition counts,
last buffer bytes and elapsed time for diagnosis. All 127 harness tests pass,
including child-env preservation and deterministic timeout diagnostics. Restart
the full matrix on this configuration; all 45 wasm and 28 host cases pass.

With memory sampling valid, Hyro then failed steady CPU narrowly (TS/Rust
6.5/6.6 ms), while total CPU 30.8/27.3 ms and memory 15.5/1.0 MB passed.
The next source candidate retains book-symbol strings behind an active prefix
and returns drained ticker-delta storage after frame serialization. It removes
repeated symbol and delta-vector allocations while preserving symbol-based
deduplication, publication order, snapshots, market replacement and field masks.
All 52 engine and 105 Bybit tests, strict Clippy and independent adversarial
review pass. The artifact is 3,658,442 bytes within the same inherited budget.
The 31-gate matrix must pass before pushing; all earlier failures are retained.

## Resumed integration after restored quota

All 31 ten-venue replay gates passed on `95ea95c633` (14 public medians of
three; 17 private), with the 3,658,442-byte artifact. The subsequent combined
wasm boundary/host run also passed. Quota restoration resumed the final PR
process; PR #1307 still requires its new head pushed and green CI before merge.

First fetched and merged `c5bc2c4adf`, the Hyperliquid landing. Keep its venue
feature flags, per-venue builds/CI matrix and fixed 1,258,291-byte single-bundle
budget. Bybit aliases share the Bybit feature and run only in its matrix job.
The new build leaves the engine and dispatch at O3; Bybit can return to O3
within its per-venue budget. All pre-existing allocation and measurement fixes
remain. Preserve new typed query/identity controls and all exact replay audits.
Hyperliquid's native Bybit-control guard is proved red/green; all 58 native
cases pass. Shared Hyperliquid wrapper regressions and client-ID contracts pass.
Combined functional checks, per-venue size/performance gates and final-head CI
are pending on this merge. Hyro live connectivity remains unverified.

### Exact request audit follow-up

The first per-venue build is 918,426 bytes, below the new fixed 1,258,291-byte
budget. The stronger incoming cross-implementation HTTP audit exposed three
request differences: Hyro skipped the shared optional PreLaunch instrument
read; spot skipped account metadata; Hyro used 200-row position pages instead
of TS's simulator-specific 50. Inverse/demo/linear private gates pass unchanged.
Spot and Hyro fail only those request comparisons; no mutation exception was
added. Native fixes restore the exact probes/page size and Hyro's symbol
selection (USDT only, last duplicate value at first insertion position).
Review also found that optional PreLaunch reads could delay startup until the
host HTTP timeout. Add the shared TS helper's 1.5-second grace after required
Trading pagination, with late-response fences, before re-running the matrix.

Post-merge combined functional gates pass: wasm 45, host 28, frame/copier 8.
All 811 native cases, 166 harness helpers, bans/fmt/Clippy, root typecheck/lint
and licenses pass. The first full root test run hit cold import timeouts in
Hyro configuration tests while running two safe-cex suites simultaneously;
25/27 tasks completed and 6,634 safe-cex unit cases passed with one timeout.
A serial root-task rerun is pending; no timeout or assertion was relaxed.

The request-parity phase is frozen: all 114 Bybit native cases and strict
all-target Clippy pass. The thirteen public tests cover every profile,
PreLaunch pagination/nonempty results, optional error/nonzero responses,
Hyro symbol deduplication, slow/retried Trading, timeout with partial optional
pages, stale timers and late callbacks while fee metadata is still pending.
The two new account request regressions were proved red/green, including the
second Hyro position page. Independent public and account reviews pass with
no remaining findings. The optional grace starts only after the final required
Trading page, cancels its exact timer on completion, and ignores late optional
results. No request audit, performance threshold or timeout assertion changed.

The final combined artifact passes 45 wasm, 28 host and 8 frame/copier cases.
Its spot wasm assertion was stale after restoring account metadata: it now
positively requires exactly one GET account/info and still prohibits position,
fee-rate and PreLaunch reads. Independent review passes.

A serial root rerun and a two-worker rerun reproduced cold-import timeouts in
the existing Hyro configuration test. A scoped baseline reproduced both failing
imports. The test now preloads the real constructor graph before timed cases
and mocks only the unused all-venue factory dependency of the pure account
options mapper (the mock throws if called). Every env reset and assertion is
retained; an extra two-instance test proves freshly reloaded real constructors
capture new REST/public/private origins while the existing instance keeps its
old configuration. All eleven scoped cases pass at the unchanged five-second
limit; the formerly failing cases took 25/143 ms. Independent review passes;
production code and performance measurement are unchanged. A fresh complete
root test run is pending. The serial run was stopped after confirming its
known import failures; its partial results are not a passing gate.

### Landing advanced during final gates

On `0b30b016fc`, all ten Bybit profile gates and Binance public/private pass;
Hyperliquid public also passes. Per-venue bundles observed: Bybit 920,675,
Binance 757,632, Hyperliquid 994,423 bytes. Final workspace 820 native tests,
Clippy/bans/fmt/wasm Clippy, typecheck44/lint28 pass. Both full safe-cex suites
pass all eleven repaired configuration cases; the unit suite passes all6,636.
The concurrent CI-shaped suite hit one unrelated capability-branding cold
registry-import timeout. A bounded-worker rerun was interrupted for integration,
so that partial run is not claimed passing. No capability assertion was altered.

The landing then advanced through transport PR#1313 and Bitrue PR#1314 to
`fc18c55874`. Integrate the new host routing roles, transport boundary/golden
matrix tests and twelfth venue without removing the profile guards or gates.
Completed eleven-venue runs are historical; final validation must include the
new landing and Bitrue. Preserve the fixed per-venue size budget.

### Twelve-venue transport integration

Resolve the two shared TS conflicts as the union of routed host transports and
all five Bybit profiles. Preserve raw account/factory routing, secondary-key
rejection, explicit Hyro origins, native quantities, protection/client-ID guards
and Backpack clock/disposal binding. All 265 focused routing/HOW/native-matrix/
equivalence/adapter/wrapper cases pass; independent shell review passes. The
172 harness cases pass; independent whole-diff and Bybit-specific reviews pass
with exact request/write/subscription/memory audits unchanged.

The new Bitrue crate requires the three optional placement fields in amendment
reconstruction and an early whole-batch Bybit-control guard. Both invalid-later
batch regressions were proved red (one write escaped) and green. The incoming
Bitrue account test's socket pattern needed `..` for the new action fields.
Its two socket sites also lacked required explicit roles; public/private tags
now preserve the real URLs and reconnect paths. Bybit's central role selection
is tested across all five profiles and both networks, including identical Hyro
public/private URLs, initial connections and reconnects. Native/Clippy and
independent review are being finalized after these minimal integration fixes.

The root-owned union audit passes: twelve features match twelve CI jobs, all
Bybit profile steps stay in the Bybit job, the fixed 1,258,291-byte budget is
unchanged, and optional wsMessage output, buffer reuse and role/context
serialization survive. Typecheck44, lint28 and licenses31 pass. The corrected
combined release build passed; combined functional, 35 replay gates and final
head CI are pending. Do not use the earlier partial matrix as final evidence.

### Reviewed source ready for final CI

On `f77c744f99`, all 910 workspace native tests, 172 harness helpers, 49 combined
wasm, 37 host and 8 frame/copier cases pass. All focused routing265 and extended
native-role/control176 cases pass. Independent audits pass for every resolved
phase; bans/fmt/workspace Clippy/wasm Clippy, typecheck44/lint28/licenses31 pass.
The complete root command passes 27/27 tasks, including both 6,856-case safe-cex
suites. The run uses two test workers passed through Turbo's environment and
serial root tasks on this shared host; no timeout/assertion was relaxed.

All twelve required Bybit-profile/Binance public and private gates pass on this
final source. Hyperliquid public/private also pass (14 of the 35 broader gates
complete). Bundle sizes so far: Bybit 921,841; Binance 758,842; Hyperliquid
995,381 bytes, each below 1,258,291. Push this reviewed source to run final-head
CI in parallel with the remaining regression bundles. Do not merge until the
full regression matrix and exact final-head CI pass. Final counters/performance
and CI evidence will be posted on PR #1307; these checkpoint entries retain the
observed state at each commit. Hyro live connectivity remains unverified.

### Adversarial stop-trigger blocker and Bitget landing

Post-push review found a P1 Rust placement defect: Spot StopLoss/TakeProfit
with neither price nor trigger_price reached an ordinary Market payload without
orderFilter/triggerPrice. A lightweight native probe reproduced both outbound
writes. Fix before merge: require a finite positive effective trigger before
batch dispatch, preserving valid trigger/price fallback and position-stop paths;
prove missing-trigger later members cannot leak earlier writes. This is a Rust
port defect, not an additional TS KNOWN_TS_FIXES claim. The separate final
harness/TS boundary review passed with lightweight negative probes.

All root27 tasks (both6,856-case suites) and native910/wasm49/host37/frame8 pass
on the pushed source; fourteen core/profile/Hyperliquid gates pass, and Bitrue
public/private subsequently pass too. These are historical after the new fix.
The landing advanced to `d172a666b1` (Bitget inverse/UTA/trailing parity), making
the PR conflict. Stop the incomplete broader matrix, integrate the new position
extensions/harness/aliases, then validate the corrected final source. Do not
merge until the blocker is fixed, independent review passes and CI is green.

The stop-trigger fix is frozen: all 121 Bybit native cases, strict Clippy and
formatting pass; the four mixed-batch regressions were proved red/green.
Whole-batch validation precedes mode/Hyro reads and all writes. The shared
helper validates the rounded finite positive effective trigger and is reused
for standalone and position-stop planning; attached normal bracket parents are
unchanged. Independent review and seven focused stop-path cases pass. No
additional native blocker was found; no TS bug correction was added for this
Rust-only defect.

The Bitget extension is specifically an optional BALANCE collateral tail, not
new POSITION columns: absent legacy data leaves coins unspecified, an appended
zero count clears holdings. Preserve legacy four-scalar balance and nine-column
positions, both inverse families, exact native order tails and existing buffer
reuse. Bitget inverse adds one private gate (36 total: 16 public/20 private),
plus four independent documented UTA/Classic trailing integration cases. The
172 harness helpers and native Bitget trading26 cases pass; request/body/signature
and mutation consumption checks remain strict and its incoming fixture is
unchanged. Final-source combined build/checks and gates must run after merge.

The reviewed parity merge is `8a79b6d5b0`, pushed after both implementation and
actual-WASM P1 reviews passed. The first local full replay attempt failed before
imports at the strict TS baseline settling deadline (four samples, one buffer
transition, ~1.1MB backing buffers, ~1.1s elapsed). Preserve those logs; retry
only after heavy local checks, without changing deadline/window/thresholds.
Typecheck44/lint28/licenses31 pass on this source; full workspace native tests
also completed successfully (see the command log), with Clippy finishing.

WOO #1316 then landed as `6945ee83b4`, so GitHub cannot create a merge commit
for the PR until that integration is resolved. Preserve its thirteenth venue,
protocol controls and generic percentage slippage field, deduplicating the
already-present Bybit option. Current-source P1/host/frame/harness gates remain
mandatory. Final PR CI will run the complete 13-job venue matrix on isolated
runners, alongside local required profile gates when the shared host is quiet;
use those complete CI results as the authoritative full regression evidence.
Do not claim interrupted or baseline-timeout local matrices as passing.

### Thirteen-venue coherent source

Resolve WOO as a strict union. The generic percentage slippage option has one
serde-defaulted field; remove sixteen duplicate initializer lines introduced by
its different field position, with no existing-venue behavior change. WOO's
batch preflight now rejects Bybit-only BBO/TickSize controls before reads/writes;
both mixed-batch regressions were proved red/green. WOO112 + Bybit121 native
cases and strict Clippy/fmt pass; independent native review passes and the P1
stop guard is unchanged. WOO's original explicit socket roles remain intact.

All 178 harness helpers and all-module symbol checks pass without weakening
prior exact transport/body/signature/mutation/metric controls. Independent
schema/feature and TS wrapper reviews pass. Thirteen features match thirteen
CI jobs, with the same 1,258,291-byte per-venue budget; Bybit aliases and endpoints
remain feature-gated. The twelve-venue previous full native run passed 987 cases;
that historical result is not a current thirteen-venue full test claim. Current
combined build and targeted boundary checks precede pushing for complete CI.

### Fourteen-venue landing integration

The thirteen-venue head `1b8b1f128d` completed all 38 replay gates in CI
(17 public/21 private), all 1,099 native tests and the remaining Rust checks.
Its two root CI jobs were still running when Lighter #1319 advanced the landing
branch to `31ec401f60`; those results are historical, not final-source evidence.
The local required replay was interrupted for the merge after Hyro/inverse/spot
public gates passed. No settling window, memory metric or performance threshold
was relaxed.

Resolve the Lighter conflicts as a union: retain the five Bybit profiles,
configured Hyro origins, strict request consumption, subscription generation and
category checks, generic known-fix dispatch and symmetric pre-import metrics.
Add Lighter form-signature identity verification and protocol initialization;
omit incoming stale duplicate known-fix calls. All 194 harness tests and helper
lint pass. Auto-merged feature/schema/build/CI audit passes with fourteen venue
features and the unchanged individual bundle budget.

The TS wrapper retains all finite/control preflight while always checking
caller IDs, including venues such as Lighter with no supported caller-ID rule.
Focused Lighter/WOO/Hyperliquid wrapper47 tests, safe-cex typecheck and lint pass.
Lighter must reject Bybit-only BBO and TickSize controls across the entire batch
before reads/writes/store mutation: both new mixed-batch tests were proved red
and green, including nonfinite ticks, unchanged snapshots and no dirty frame.
All 87 Lighter native tests pass. Independent TS/native guard audit passes;
final independent harness audit passes. Final-source build/artifact checks and
CI remain required before merge.

### Fifteen-venue landing integration

The fourteen-venue source `66b27530c1` passed all 24 CI checks, 40 replay gates
(18 public/22 private), 1,185 native tests, 194 harness helpers and the local
12 required Bybit/Binance gates. Combined WASM59/host38/frame10 and root
licenses31/typecheck44/lint28/test27 passed; both safe-cex suites passed 6,924
cases. After final PR evidence was posted, the authorized match-head merge
was rejected atomically: Orderly #1318 had advanced the landing branch to
`f66bdc26f9`. Preserve those successful historical results; integrate Orderly
and wait for new exact-head CI instead of bypassing conflicts/checks.

Keep all five Bybit profiles and strict gates while adding Orderly broker ×
network coverage. Position-ID string references append after the same nine
position numeric columns, opt-in only; preserve the independent native order
tail and optional balance collateral tail. Frame10 and host/wire45 tests pass.
Independent WASM/wire review passes. CI has fifteen base crates and retains the
same individual bundle budget and every Bybit public/private gate.

Orderly's whole-batch Bybit BBO/TickSize rejection precedes readiness, active
queue, planning and signing. Idle and busy mixed batches were proved red/green,
with immediate exact error and unchanged actions/store/frame. All 84 Orderly
native cases, strict Clippy/fmt and independent guard review pass. TS wrapper88
cases pass after updating stale WOO negative-registration assertions for newly
registered separate Orderly aliases. Production guards are preserved. Typecheck,
lint and workspace/WASM Clippy pass. Final harness union224 tests and scoped
lint pass, including negative extra/missing Orderly command-result oracles. The
combined release build and all 74 actual WASM tests pass. Independent harness
and WASM/wire audits pass. New-source replay gates/CI remain required before
merge.
