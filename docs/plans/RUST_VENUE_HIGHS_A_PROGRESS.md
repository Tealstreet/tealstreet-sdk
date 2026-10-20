# MEXC, Bitrue and BloFin audit repairs

Branch: `feat/rust-venue-highs-a`, based on `6fea5de843` from
`origin/feat/rust-rewrite`. Scope comes from
`venue-highs-mexc-bitrue-blofin.md` and the named `RUST_AUDITS.md` findings.
Judge each finding by production impact; lifecycle differences alone are not
bugs. No TS venue deletion, deployment or credentialed trading.

## Phases

1. Revalidate and plan. Read the current venue implementations and TS contracts;
   retain only findings that still reproduce. Coordinate MEXC startup with
   `codex-u1pmrg`, whose cross-venue startup work owns that policy.
2. MEXC public/private/trading repairs. Cover ticker interest beyond the first
   100 markets, row-local unknown-market order reconciliation and snapped native
   full-position stop quantities. Recheck single-order pacing against the
   documented venue throttle policy before changing it. Add reproducing tests.
3. Bitrue account/trading repairs. Preserve confirmed account leverage without
   assuming market maximum, allow fresh Cross accounts to trade, continue cancel
   batches with visible per-ID failures, prevent one unavailable symbol from
   blocking usable orders, expose coin balances, and discover/apply real leverage
   brackets without silently changing caller intent. Add reproducing tests.
4. BloFin account/protocol repairs. Retain the correct per-symbol margin mode
   after position close, tolerate documented nullable row fields without
   replacing good state with malformed snapshots, and serialize/sign integer
   WebSocket login milliseconds. Validate nullable shapes against committed payloads
   and venue contracts, stating any live-evidence limitation.
5. Integration and gates. Review each completed venue phase with an independent
   sub-agent, fix findings, update affected CLAUDE docs and audit strikes. Extend
   deterministic harness evidence where needed; explicitly document genuine TS
   bugs in `KNOWN_TS_FIXES` and commit messages without weakening comparisons.
   Run bans, fmt, clippy, native tests, wasm clippy/build, root typecheck/lint/test,
   private/public replay gates for all three venues and Binance, and CI size
   checks. Public replay uses recorded fixtures with unchanged comparisons.
6. Landing. Merge latest landing branch, rerun affected gates, push and open a PR
   into `feat/rust-rewrite`; post independent code-correctness reviews, fix and
   repeat. Merge only after green CI, then create a fresh follow-up branch.

Implementation is split by venue with disjoint write scopes; shared harness,
audit docs, integration, commits and landing remain with the primary agent.
Each phase records its concrete plan, outcome, regression counts, review and
commit below. Stable points are committed; never stash, rebase or force-push.

## Progress

- Initial worktree clean. Fetched landing and created the requested fresh branch.
- Read the current brief, spec, audit judging rule and quality standards.
- Sent startup ownership coordination to `codex-u1pmrg` via Aimux; no startup
  retry-policy edits planned in this lane pending that coordination.
- Venue workers are revalidating findings and preparing bounded implementation
  plans before source edits. Shared harness changes wait for their findings.

### MEXC phase — implemented and independently reviewed

All four non-startup findings reproduced. The public socket now subscribes to
the documented sparse all-contract `sub.tickers` feed and reconciles detailed
`SetTickerSymbols` interest across selection changes and reconnects. Sparse
fields preserve detailed metadata; known open-interest quantity is revalued at
the new mark. Explicit selections may exceed 100: no official constraint was
found that would justify retaining the TS default cap for explicit interest.

Typed, identifiable unmappable order rows log and preserve an existing copy of
that ID while usable siblings commit. A missing native ID remains a failed
snapshot. Full-position TP/SL snaps native volume to `volUnit`. Singletons use
the regular submit route and 450 ms spacing; multi-order fallback and 510
cooldown retain 1,500 ms pacing. Startup retry policy is untouched.

Validation: 73 native tests and strict all-target clippy pass. Eight added
regression functions cover >100 freshness/interest/reconnect, sparse/invalid
quotes, unmappable rows and malformed identities, stop steps and pacing.
Independent review found the missing-ID snapshot hole; fixed with both initial
and already-loaded regressions, then re-reviewed PASS. A proposed 100-interest
cap was withdrawn because TS's cap alone is not a production constraint.

TS divergences are explicit: the gate adds only the exact documented expected
TS mini subscription, never prices or Rust output. The private tape separately
requires TS's rejected singleton market-maker batch attempt and the shared
accepted submit. Request/signature/write-consumption checks remain active.

### Bitrue phase — implemented and independently reviewed

Fresh/flat symbols can establish leverage with a successful explicit write;
unknown margin defaults Cross without guessing market-max leverage. Empty or
zero-leverage flat account rows retain that confirmed setting and valid balance
data. Cancels continue after per-ID preflight, transport, venue and identity
errors, report confirmed IDs and each failure, and preserve failed live orders.
Children remain fallback cancel steps until their parent actually succeeds.

Initial order readiness follows a complete first sweep of attempted symbols;
failed symbols remain visible errors with retained state and scheduled retries.
USDT/USDC currency balances cross frames alongside the primary wallet.
Authenticated, identity-checked leverage brackets update only the ceiling;
leverage writes preserve exact requested integers and reject known limits
visibly rather than silently clamping. `FetchMaxLeverage` is separate from
current account leverage; the SDK uses it for Bitrue max/metadata reads, flushes
native market changes and keeps public-only reads unauthenticated.

Validation: 83 Bitrue native tests, strict all-target clippy and seven new SDK
boundary tests pass. Engine JSON conversion and actual SDK/WASM bracket/frame
assertions are included for integration gates. Private replay now exercises a
flat symbol's 125x bracket and both collateral currencies; comparisons include
all currency balances. Independent review found premature parent/child cancel
deduplication; missing/unmappable/failed parent and successful dedup regressions
resolve it, re-reviewed PASS. Other account/trading/bracket and shared boundary
review PASS. Intentional TS corrections (flat leverage preservation and complete
cancel continuation) are listed in the gate without result normalization.

### BloFin phase — implemented and independently reviewed

Full-position closes (empty or explicit-zero REST/WS rows) restore the global
account margin default only for flat symbols, preserving live overrides and
confirmed leverage. Login serializes integer milliseconds and signs that exact
decimal representation, including when the host clock has a fractional part.

Official algo/TPSL examples contain null inactive protection prices and trigger
types. These optional fields decode as absent; known leverage/liquidation
metadata survive nullable position updates. Required quantities and identities
remain strict. WS order/position/ticker rows decode independently with visible
errors and retained state for malformed rows. A malformed initial ticker can
recover on its next valid event. Authoritative private REST corruption retains
the prior complete snapshot.

Validation: 84 BloFin native tests pass (12 added regressions). Strict clippy
passed before the final two-line aggregation correction; the complete rerun is
queued behind workspace validation. Independent review caught balance sums
dropping unmappable native instruments and rounding each mapped position's PnL;
fixed by summing raw typed native risk/PnL before mapping and final USD rounding,
with two regressions, re-reviewed PASS. H3/null/M2 review otherwise PASS.

Evidence limitation: the captured public tape contains no null values; nullable
protection comes from official documented examples and is now in the private
tape. Required-field nulls in regression tests are labeled defensive corruption
cases, not live traffic. No TS state normalization is added for these fixes.

### Integration checkpoint

- Venue commits: MEXC `769872ed18`, Bitrue/shared boundary `6811eb049b`,
  BloFin `e15200f4d8`. All phase findings were fixed and independently re-reviewed.
- Merged landing `5f84b3e21d` (Binance/Bybit audit repairs) as `31f418fc0b`.
  Final workspace and root checks run against that combined source.
- Initial root checks passed: 44 typecheck tasks, 28 lint tasks, 27 test tasks;
  safe-cex 7,779 tests per runner. Harness: 593 tests pass. Root checks are being
  repeated for the newly merged landing code; these initial counts are not final.
- The new real SDK/WASM bracket regression detected a stale local baseline
  artifact (`fetchMaxLeverage` missing). Its dependency info referenced the old
  archived performance baseline. That failing run is excluded; all local
  workspace release WASM artifacts were cleaned and are rebuilding from current
  source before any replay or compiled SDK evidence is accepted.
- Full workspace clippy passes. Final native tests, WASM clippy/build, compiled
  SDK/host/consumer checks and public/private/size gates remain in progress.
- MEXC startup policy remains assigned to the cross-venue startup lane. Sent
  two coordination messages and durable Aimux handoff `task-dmbixqhc1yzx` to
  `codex-u1pmrg`; no duplicate startup retry implementation in this branch.

### Fresh public capture follow-up

Fresh anonymous 20-second captures succeeded on all three venues (two books
each): MEXC 19,111 WS frames / 6 HTTP responses, Bitrue 3,412 / 1, BloFin
20,985 / 2. MEXC delivered eight `push.tickers` arrays (5,254 rows); untraded
contracts legitimately carry `lastPrice: 0` beside valid fair/index prices.
The initial strict positive-price check incorrectly skipped the other fields
and warned for inactive instruments. Corrected by resolving catalog identity
first, treating zero quotes as unavailable per field, accepting optional live
index prices, and retaining strict negative/non-finite rejection for known
instruments. MEXC now passes 75 native tests and strict clippy; bounded
independent re-review PASS. The captured input remains in ignored harness tapes.

Canonical recorded fixtures remain unchanged so the benchmark baseline does not
move with these repairs. Live capture is public protocol evidence, not private
authentication/trading evidence or a claim that TS handles its missing mini
feed. The standard paired public gates still compare the committed recordings.

Final merged root checks pass: 44 typecheck, 28 lint, 27 test tasks; safe-cex
7,798 tests / 528 files per runner. Native workspace: 2,939 tests / 228 suites,
zero failures (before the final two MEXC mini regressions; those pass separately).
Bans, fmt, workspace all-target clippy and WASM clippy pass. Clean combined
WASM has the new command and dependency paths exclusively in this worktree;
all 444 compiled SDK tests and 76 host tests pass before the final mini change.
Compiled replay/consumer and per-venue gates continue below.

### Landing wire integration (gates being repeated)

Before the wire integration, all seven canonical public gates passed (three
runs each): MEXC, Bitrue, BloFin, Binance and its Spot/COIN-M/PM profiles. All
seven unique private gates passed, plus the four primary combined-build private
gates. Four isolated sizes passed the unchanged 1,258,291-byte budget: MEXC
879,508; Bitrue 889,649; BloFin 871,085; Binance 1,005,059. All compiled
SDK/host/wire/server/consumer/harness/scripted lifecycle checks passed. These
remain pre-wire evidence, not final numbers for the merged head.

Informational process CPU did not beat TS on Bitrue (216.9 / 217.4 ms TS/Rust)
or BloFin (345.2 / 374.4 ms). The required main-thread CPU, steady CPU and
retained-memory gates passed for every venue; no claim that every process CPU
metric wins. Full metrics/logs were archived under `/tmp/highs-pre-wire-evidence`.

Fetched and merged landing `c44f22bd8e` (PR #1345 wire/consumer fixes), preserving
the new maximum-leverage command and SDK/native routing. The pending final gate
pipeline was canceled before accepting its evidence because its source base
was superseded. Full quality checks, combined WASM/SDK/consumer checks and all
seven paired public/private gates are running again on the merged code. Three
independent source reviews also audit this integration; no PR opened yet.

### Final wire-integrated local gates — PASS

Final local checks pass on landing `c44f22bd8e` plus this PR:

- Banned-pattern checks, workspace formatting, strict workspace all-target clippy and WASM clippy.
- Native: **2,950 tests / 228 suites**. Touched venue totals: MEXC 75, Bitrue 83, BloFin 84.
- Root: **44 typecheck tasks, 28 lint tasks, 27 test tasks**; safe-cex **7,847 tests / 530 files per runner**.
- Compiled SDK **445**, host JS/Workers **89**, frame-wire **7**, native session adapter **1**, product semantics **16**, harness **593**, scripted lifecycle **9** tests.
- Compiled consumer checks: safe-cex-server **11**, exchange-mcp **5**, CLI **2**, React Native **2**; MCP build and packaged smoke pass.
- **7 public profiles** (three runs each), **7 private profiles**, plus **4 primary combined-build private replays**, all PASS. Binance Spot/COIN-M/PM included.
- Isolated WASM sizes: MEXC **881,318 B**, Bitrue **890,914 B**, BloFin **872,350 B**, Binance **1,007,175 B**; each below unchanged **1,258,291 B** budget.
- Fresh MEXC public capture also replays on the final combined Rust build. No live private/trading evidence.

| Venue | Main CPU TS / Rust (ms) | Steady CPU TS / Rust (ms) | Retained TS / Rust (MB) | Process CPU TS / Rust (ms) |
| --- | ---: | ---: | ---: | ---: |
| mexc | 973.3 / 664.0 | 639.2 / 423.1 | 27.7 / 5.2 | 1209.0 / 898.7 |
| bitrue | 260.9 / 198.6 | 180.3 / 116.5 | 17.1 / 3.4 | 367.9 / 355.4 |
| blofin | 249.7 / 221.0 | 161.2 / 126.1 | 16.7 / 3.1 | 342.5 / 364.9 |
| binance | 277.7 / 130.4 | 155.0 / 57.0 | 18.2 / 4.1 | 316.7 / 213.6 |
| binancespot | 703.8 / 440.3 | 282.5 / 146.7 | 27.7 / 11.2 | 1035.8 / 655.1 |
| binanceinverse | 170.8 / 64.7 | 112.1 / 34.9 | 14.0 / 1.9 | 203.8 / 94.5 |
| binancepm | 191.0 / 90.8 | 106.0 / 37.2 | 18.0 / 4.1 | 242.8 / 155.7 |

Main-thread CPU, steady CPU and retained-memory gates pass for every profile. Process CPU is informational: BloFin is slower there (364.9 vs 342.5 ms); no claim of a win on every metric. Frozen public fixtures remain unchanged.

Final combined WASM SHA-256: `a6f1ac97d38ee6809753571a5925984a3a9b9b0ee1b2f5fad9372c1a65f65668`. Its dependency info points at current sources and includes the new command; stale archived-baseline evidence was discarded before these checks.

Three fresh integration source reviews PASS with no new findings. All scoped phase findings are fixed and independently re-reviewed. The final source is the merge at `8110defe6a` (venue changes through `9dd1e171a3`); later changes are progress documentation only. Landing remains `c44f22bd8e` after fetch. PR correctness reviews and green CI are the remaining landing gates.

Wind-down instruction received: finish this PR, write the local handoff, then stop without starting a follow-up assignment.

### PR #1355 and final landing integration

Opened https://github.com/Tealstreet/tealstreet-sdk/pull/1355 into
`feat/rust-rewrite`. Three independent PR code-correctness reviews passed at
`c431a0410c` and were posted as PR comments. MEXC/Bitrue/BloFin CI replay jobs
passed, along with native clippy, fmt, WASM and secret scanning.

While CI ran, landing advanced to `f895a98f26` (Paradex repairs and TS listener
isolation, PR #1352). Merged cleanly as `3f7b8b0900`; our scoped source diff is
unchanged, and the harness/audit auto-merges retain both lanes. Integration
reviews, affected checks and final CI now run against this merged source.
The preceding table is explicitly the complete wire-integrated checkpoint;
final post-listener numbers and merge status are recorded in PR #1355. No
follow-up assignment will start after this PR lands, per wind-down.

### Socket-supervisor landing integration

PR #1355 reached 40/40 green checks at `42810f6bd2`. One synthetic HyroTrader
steady-CPU timing failure (TS/Rust 3.8/4.1 ms) passed a single bounded rerun,
after comparison with both passing prior-head and landing results. The evidence
and unchanged hot-path comparison are posted on the PR; no thresholds changed.
The redundant local replay repeat was stopped after compiled SDK, host, wire,
server, harness, lifecycle and all four consumer tests passed; CI had completed
the same full venue gates. Its canceled MCP smoke is not counted as a pass.

GitHub then rejected the merge because landing advanced to `d17518bc5f`
(PR #1350 socket supervision). Integrated it locally. The only conflicts were
adjacent MEXC gate-handler edits (kept both exact mini-subscription correction
and auth-recovery documentation) and appended Bitrue tests (kept both suites).
Production auto-merges now receive independent integration reviews and affected
checks; final CI must pass again before merge. The current final root checkpoint
is 44/28/27 tasks, safe-cex 7,854 tests per runner; pre-supervisor native is 2,959.

CI on the supervisor merge caught one test-only lifecycle assumption: the
unmappable-order regression sent a second `rs.login` to force another full read.
The supervisor correctly ignores duplicate successful logins. Updated the test
to close/retry/open a distinct socket, authenticate it, assert all five reads,
and supply empty responses at the new monotonic clock. Independent review PASS;
production code is unchanged. Native verification continues on this correction.
GRVT's install-stage HTTP 502 and the recurring tiny Hyro CPU measurement are
recorded separately from correctness failures; no thresholds are weakened.
