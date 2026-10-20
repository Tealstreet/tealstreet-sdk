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
