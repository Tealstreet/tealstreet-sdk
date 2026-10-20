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
