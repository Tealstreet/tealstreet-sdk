# Binance variants port

Branch: `feat/rust-binance-variants`, based on `origin/feat/rust-rewrite`.
Follow `VENUE_PORT_BRIEF.md`; all PRs target `feat/rust-rewrite`.

## Phases

1. Public profiles: extend the existing Binance paths table and options for
   Spot and COIN-M. Preserve USD-M/PM. Verify markets, ticker seeds/streams,
   spot versus futures depth continuity, candles and mainnet/testnet routes.
2. Private profiles: typed Spot signed WebSocket API subscriptions and account
   balances; COIN-M contract sizes, coin collateral valuation and private
   reconciliation. Preserve stale response and reconnect fences.
3. Trading and queries: profile-specific order types, identifiers, endpoints,
   split sizes, bracket handling, cancel/amend, unsupported command preflight,
   and history. Keep whole-batch client-ID validation before writes.
4. Integration and evidence: WASM/worker registration, strict independent replay
   tapes, public captures, CI coverage, roster/docs. Reproduce Hyro CPU misses,
   profile any genuine Rust loss and fix its cause without relaxing gates.
5. Final adversarial reviews: post findings on the PR, fix and repeat, rerun all
   required checks including USD-M/PM/Hyro. Merge only with green CI, then cut
   a fresh branch from the landing branch.

Every phase receives a sub-agent audit and commits at stable points. No stash,
rebase, force push or master changes. Official documented payloads and runtime
contracts define regression expectations. TS defects fixed deliberately require
`KNOWN_TS_FIXES`, commit-message and PR disclosure.

## Progress

- Planning: inspected the four TS Binance profiles and current Rust engine.
  Public/account implementation and Hyro investigation have bounded owners;
  trading/query/integration remains with the lane owner.
