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


### Native implementation checkpoint

Public/private/trading phases share option selection and socket callbacks, so
these implementations are committed together after independent phase reviews.
USD-M/PM regression tests remain in the Binance crate. Native checks reached
73 passing cases before the final depth-generation regression; crate Clippy,
WASM-target Clippy, bans, root typecheck (44 tasks), root lint (28 tasks), the
first two actual WASM boundary cases and six independent frame tests passed.
Final rebuilt-artifact checks and replay evidence remain pending.

Phase reviews fixed protective lot splitting, ignored trigger intent, post-only
amendment semantics (including REST seeds), duplicated/reordered executions,
subscription ACK deadlines, Spot collateral valuation, native COIN-M sizing in
both worker/main frame appliers, and old depth snapshots crossing resubscription.
A real live Spot failure was isolated to large compressed subscription messages:
17,392-byte requests received close1008 `Payload too long` with Node's `ws`;
smaller messages worked. Public control messages now have bounded topics/bytes
and 400ms pacing; recapture will verify the correction.

Known TS defects deliberately fixed: retired aggregate Spot feeds; signature-only
Spot subscription liveness; split quantities rounded away; invalid/oversized Spot
protective legs; Spot fill price and replay/cancel identity; lost maker semantics
on amendment; COIN-M caller-ID preflight and duplicate/terminal execution state;
malformed inverse order snapshots. The harness checkpoint lists any further
verified differences. Defensive rejection and Rust-only bridge/depth fixes are
not presented as new TS defects.

Hyro unchanged baseline: three-run public PASS, TS/Rust main50.8/34.4ms,
steady17.6/13.2ms, retained16.1/0.9MB. Profiling found no confirmed production
regression. Machine-wide concurrent work prevents claiming strict isolation;
final fresh-artifact Hyro gates remain required with unchanged thresholds.
