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
