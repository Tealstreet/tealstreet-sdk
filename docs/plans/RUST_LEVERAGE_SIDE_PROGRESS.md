# Side-aware leverage and Bitget production audit fixes

Branch: `feat/rust-leverage-side`, from landing `73cf0e8269`.
Scope: Bitget family audit in `RUST_AUDITS.md`, plus cross-venue leverage side
propagation and reachable leverage clamp panics. No startup resilience or
“Confirm” changes. Judge differences by production impact, not TS lifecycle
identity. Keep TypeScript venue code and existing default selection intact.

## Phases

1. Inventory TS side-aware leverage and audit production impact. Independent
   plan review; commit this plan with the command-boundary phase.
2. Add optional typed `side` to the engine/JSON command and forward it from
   `RustEngineExchange`. Update callers mechanically; test omitted/long/short
   and invalid JSON side. Preserve legacy coupled behavior for other venues.
3. Implement selected-leg writes/settings for Bitget, BingX, MEXC, Phemex and
   WEEX. Preserve opposite-leg state and failed-write state. Guard reachable
   invalid leverage bounds rather than panicking. Test each venue's payload,
   defaults, mode rules and successful/failed effects; independent phase review.
4. Fix Bitget history, row reconciliation, zero-lot handling, pre-probe command
   handling, sized-stop warning, busy refresh rescheduling, zero leverage limit
   defaults and UTA one-way fill direction. Regression tests per production
   impact; independent phase review. Exclude startup retry/broker lane work.
5. Workspace/SDK/host checks, affected private/public replay gates and Binance
   regressions. Record real counts and performance/size budgets; review/fix.
   Merge latest landing, push, PR into `feat/rust-rewrite`, post independent
   code-correctness reviews, resolve findings, merge on green CI and cut a fresh
   branch. Strike audit items when their fixes merge.

## Venue inventory

The table covers every registered venue/profile. “Flag” is independent hedged
isolated leverage metadata; ignored side parameters do not imply independence.

| Venue/profile | Flag | TS side behavior / implementation scope |
| --- | --- | --- |
| bitget, bitgetinverse, bitgetusdc | yes | Classic `holdSide`; isolated hedge UTA `posSide`; omitted defaults long. Preserve opposite leg. |
| bingx | yes | Hedge requires side `LONG`/`SHORT`; one-way `BOTH`. |
| mexc | yes | Side selects native position type/ID; omitted writes both rails. |
| phemex | yes | Hedge long/short fields preserve unselected leverage; omitted changes both. |
| weex | yes | Isolated long/short fields preserve unselected leverage; cross is coupled. |
| breakoutprop | yes | TS setter unimplemented, cross/one-way only. Do not invent side support. |
| asterdex, backpack, bybit, bybitdemo, hyrotrader, bybitinverse, blofin, cryptocom, grvt, apexomni, hyperliquid, hyperliquidspot, lighter, lighter-robinhood, kucoin, kucoinspot, kraken, woo | no | TS accepts but ignores side; preserve coupled/account/fixed or unsupported behavior. |
| binance, binanceinverse, binancepm, binancespot, bitunix, bitrue, okx, okxdemo, okxeu, okxeudemo, coinbase, coinbasespot, woofi, kodiakfi, mode, salsadex, tealstreet-dex, what, txflow, krakenspot, paradex, polymarket, bybitspot | no | TS has no side parameter; preserve existing behavior. Bitrue's third parameter is a silent flag. |

Reference-only TS for deleted families is inspected in
`src/__tests__/reference`; it is not restored as production code.

## Production-impact decisions

- Wrong-leg leverage changes trading risk and displayed state: fix end-to-end.
- Missing liquidation/ADL/reduce history, dropped valid snapshot rows and stale
  Classic refreshes silently omit or retain incorrect account data: fix.
- Rejecting an entire batch for one unplaceable zero-lot row drops otherwise
  valid orders. Preserve caller-ID preflight guarantees while skipping the row.
- Commands arriving during account detection must not fail against a guessed
  routing mode. Wait for the existing probe; no new startup retry policy.
- Sized stops downgraded to full-position need the existing user-facing warning.
- Zero/invalid leverage limits must not panic the engine or strand commands.
- UTA one-way fill legs must preserve directional trade history.
- Different internal startup/disposal/retry order alone is not a regression;
  startup resilience and audit “Confirm” items stay outside this lane.

## Progress and evidence

- Independent phase-1 inventory/plan audit: PASS; five side-aware families,
  BreakoutProp metadata-only exception, several reachable dynamic clamp risks.
- Implementation and acceptance gates pending.
