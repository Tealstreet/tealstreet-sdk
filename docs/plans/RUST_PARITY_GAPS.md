# Rust venue parity gaps

Branch: `feat/rust-parity-gaps`, based on `origin/feat/rust-rewrite`
(`1f9e9a5206`). Follow `VENUE_PORT_BRIEF.md` and `RUST_REWRITE.md`;
full automatic execution, independent review at each phase, stable commits,
PR into `feat/rust-rewrite`, review findings posted, merge only on green CI.

## Phases and verification

1. **Inventory and plan.** Compare current TS runtime capabilities with the
   merged Rust implementations. Audit the roster for explicitly unfinished
   capabilities; distinguish future ports and live-connectivity evidence from
   missing adapter features. Record skips where TS itself lacks the feature.
2. **Paradex delegated signing.** Trace both TS `remoteSigner` and
   `payloadSigner`, including JWT acquisition, exact signing material, headers,
   private-socket policy, failures, disposal and request correlation. Preserve
   the sans-I/O engine: host delegates signatures and returns typed results;
   venues name real URLs and retain existing account transport routing. Verify
   local/delegated equivalence, missing/failed/malformed/late signer replies,
   concurrent auth and writes, and absence of automatic write retries.
3. **Bitrue queries and settings.** Implement TS-supported order/PnL/funding
   history, complete pagination/time filters, client-order-ID lookup and its
   definitive-miss contract, hedge mode and margin mode changes. Check native
   request shapes, successful settings commits and stale-read fences; reject
   failures without clearing last-known-good state. Test paging boundaries,
   signed amounts, closed-order lookup and rejected/ambiguous writes.
4. **KuCoin spot.** TS has a separate HF spot adapter. Add a typed spot venue
   in the existing crate, reuse signing and host routing, register the exchange
   in WASM/SDK/harness. Cover public markets/tickers/full books/candles, private
   balances/orders/executions and recovery, HF trading/amend/cancel and caller
   IDs, and precisely the queries/settings TS supports. Test spot units and
   market buys, token recovery, snapshot races and page traversal.
5. **Integration and gates.** Review the inventory for small additional gaps,
   update roster/docs and add deterministic regressions. Run bans, fmt, Clippy,
   native tests, wasm32 Clippy, SDK typecheck/lint/tests, host/harness tests;
   build and replay public/private gates for Paradex, Bitrue, KuCoin futures
   and spot, plus Binance public/private. Record fresh public traffic for the
   touched venues. Standard egress comparison remains enabled. Keep the
   1,258,291-byte budget unchanged; use incoming per-venue build support and
   record exact artifact sizes and provenance.
6. **Landing.** Push, create PR into `feat/rust-rewrite`, post independent
   adversarial correctness reviews, fix/re-review, incorporate landing changes
   without rebase/stash, wait for exact-head green CI, merge, fresh branch.

Each implementation phase follows plan audit, implementation, verification,
independent implementation review and fixes before committing. Venue work may
proceed in disjoint scopes; shared host/engine/SDK/harness changes are coordinated.

## Inventory

- Paradex: merged local signing explicitly rejects delegated modes; compare
  current TS modes before designing the signature boundary.
- Bitrue: requested histories, lookup and settings require a TS/Rust audit.
- KuCoin spot: TS supports spot. Its HF adapter does not override order/PnL/
  funding history or public trades; do not invent capabilities TS lacks.
- Roster: explicitly unverified HyroTrader live connectivity is evidence work,
  not a missing adapter implementation. Binance spot/inverse and the named
  future venues are separate unported lanes. Several already merged venue rows
  still say lane running/CI pending; verify PR status before correcting them.

## Progress

- Created branch from the fetched landing head; working tree was clean.
- Read the current brief and rewrite spec. Parallel bounded discovery and
  implementation owners established for each venue, with shared files owned
  by the integration lane.
