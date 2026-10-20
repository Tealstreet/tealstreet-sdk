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
   WebSocket login seconds. Validate nullable shapes against committed payloads
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
