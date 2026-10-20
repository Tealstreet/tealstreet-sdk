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

| Venue/profile                 | Independent flag | Leverage side contract                                      | Position mode scope                        |
| ----------------------------- | ---------------- | ----------------------------------------------------------- | ------------------------------------------ |
| apexomni                      | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| asterdex                      | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| backpack                      | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| binance                       | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| binanceinverse                | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| binancepm                     | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| binancespot                   | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| bingx                         | yes              | Hedged LONG/SHORT required; one-way BOTH                    | Global or unsupported; no new remote scope |
| bitget                        | yes              | Classic holdSide; isolated hedge UTA posSide; omitted long  | Global or unsupported; no new remote scope |
| bitgetinverse                 | yes              | Classic holdSide; isolated hedge leg; omitted long          | Global or unsupported; no new remote scope |
| bitgetusdc                    | yes              | Classic holdSide; isolated hedge UTA posSide; omitted long  | Global or unsupported; no new remote scope |
| bitrue                        | no               | Coupled; TS third argument is silent boolean                | Global or unsupported; no new remote scope |
| blofin                        | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| breakoutprop                  | yes              | TS setter unimplemented; no invented side support           | Global or unsupported; no new remote scope |
| bybit                         | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| bybitdemo                     | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| bybitinverse                  | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| bybitspot                     | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| coinbase                      | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| coinbasespot                  | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| cryptocom                     | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| grvt                          | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| hyperliquid                   | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| hyperliquidspot               | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| hyrotrader                    | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| kodiakfi                      | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| kraken                        | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| krakenspot                    | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| kucoin                        | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| kucoinspot                    | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| lighter                       | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| lighter-robinhood             | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| mexc                          | yes              | Native position type/ID; omitted both rails                 | Global or unsupported; no new remote scope |
| mode                          | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| okx                           | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| okxdemo                       | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| okxeu                         | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| okxeudemo                     | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| paradex                       | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| phemex                        | yes              | Long/short signed leverage, preserve opposite; omitted both | Required market                            |
| polymarket                    | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| salsadex                      | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| tealstreet-dex                | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| txflow                        | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| weex                          | yes              | Isolated long/short preserve opposite; cross coupled        | Optional market; omitted existing scope    |
| what                          | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| woo                           | no               | TS ignores side; preserve existing behavior                 | Global or unsupported; no new remote scope |
| woofi                         | no               | Coupled/fixed/unsupported; no TS side argument              | Global or unsupported; no new remote scope |
| bitunix (not Rust registered) | no               | TS coupled; no Rust venue added                             | Existing TS behavior                       |

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
- Implementation source phases complete; final workspace/replay/CI gates pending.

### Accepted addenda

- Optional `SetPositionMode.symbol`, forwarded by the SDK. Phemex requires one
  market and sends one PUT; WEEX honors an optional target. Binance's symbol is
  only local refresh context; Bybit explicitly clears it for coin-wide mode.
  Their global native mutations stay global. Other venues do not use a remote
  symbol-scoped setter.
- Lighter position-only margin-fraction normalization: ratio/percentage/ticks,
  including `"20.00"` as 5x and a margin-mode write retaining 5x. Market risk
  fractions are unchanged. Price-only amendments re-place remaining quantity;
  explicit amounts remain caller replacement quantities. The shared TS sizing
  bug is documented in `KNOWN_TS_FIXES`, without output normalization.

### Implementation reviews

- Bitget reconciliation/history reviewed independently by trading reviewer:
  PASS, including mixed/all-unmappable snapshots, stale rail reads and timers.
- Bitget trading and Lighter source reviewed independently by reconciliation
  reviewer: PASS for production semantics; compiler/test gates still required.
- MEXC/Phemex/WEEX side and symbol scope reviewed by trading reviewer: PASS after
  removing an invalid `.copied()` on an owned optional setting.
- Engine/JSON/SDK and BingX reviewed independently: PASS after preserving empty
  runtime side for typed rejection and updating BingX legacy long scalar.
- Shared clamp review found accepted Orderly global writes could fail midway
  through per-market cache updates on bad metadata. Fixed: retain the confirmed
  global value with a warning for invalid per-market limits; three-market
  regression passes.
- `setAllLeverage` forwards each open position's side, preserving both hedge
  legs and avoiding BingX's now-correct required-side rejection. Independent
  source review PASS; SDK regression passes.
- Bridge follow-up: when the engine has already supplied both leg settings,
  `fetchLeverageForSymbol` flushes and returns them for all seven side-aware
  profiles instead of overwriting them with the market maximum. New regression
  per profile; fresh uncached discovery remains a separate audit item.

### Implementation verification checkpoint

- Twelve affected engine/venue crates: 1,223 tests in the initial complete run;
  one existing Bitget fixture initially failed because it reset positions and
  scalar leverage without resetting the newly exposed leg settings. Fixture
  setup now seeds both rails; all three unchanged regression assertions pass.
- Final BingX suite: 99 tests pass, including two additional account tests for
  REST row ordering and retaining a flat opposite rail; partial config events
  without short leverage preserve that rail. Final affected coverage is 1,225
  tests after the fixture correction and the two added BingX tests.
- Final Orderly trading suite: 28 pass, including confirmed global leverage
  across valid/invalid/valid market metadata.
- Focused Bitget trading 8/8, reconciliation/history 11/11, Lighter 4/4 pass.
- SDK boundary 7/7 pass; SDK typecheck and lint pass. Full root checks and
  WASM/host/replay gates follow this checkpoint.
