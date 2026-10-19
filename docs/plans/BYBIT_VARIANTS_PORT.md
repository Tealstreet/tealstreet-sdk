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
private variants 15, trading/recovery 25, plus the original 43 Bybit cases.
The shell has 12 wasm integration tests and 4 independent binary/copier tests.
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
All four venues and all Bybit/OKX variants will be revalidated together.

Final CI must be green before merge.
