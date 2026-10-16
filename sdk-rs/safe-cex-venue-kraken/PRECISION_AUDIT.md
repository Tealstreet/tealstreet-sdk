# Kraken precision-swap audit

Audited the eight active Rust source files and their module/import graph against
pushed implementation `0bbfaca058aa514284efe750dd072b78cd9dd0a6`. That census counted **0 amount-step reads,
0 price-step reads, and 0 compiled two-axis formatter sites**. No precision swap
can be applied to implemented Kraken Rust yet. No mutation was run or claimed as
passing/failing for this class. This is not evidence that future formatters are
swap-proof.

The implemented paths are constants/types, fill and ledger utilities, four module
helpers, account-log response handling, REST token extraction, and synthetic Spot
positions. The subsequent leverage-settings read adds no precision consumer.
Numeric coercion and timestamp flooring do not consume Market precision.
`parse_trigger_signal` selects an enum only. The Store reads consume symbol and
active fields; the synthetic positions use source-defined numeric constants.
There is no delegated order formatter hidden in these paths.

The actual TypeScript has **12 precision reads across four pending writers**:

| Source method | Amount reads | Price reads | Price payload paths |
| --- | ---: | ---: | --- |
| Futures `placeOrder` | 1 | 3 | `limitPrice`, `trailingStopMaxDeviation`, `stopPrice` (stop-loss/take-profit) |
| Futures `updateOrder` | 1 | 1 | `limitPrice`; other order types also route through placement |
| Spot `placeOrder` | 1 | 3 | `price` for limit, stop-loss/take-profit, and relative trailing deviation |
| Spot `updateOrder` | 1 | 1 | `limit_price` or `trigger_price` after adjustment |

The two `fetchMarkets` methods are also pending. Futures derives amount step from
`contractValueTradePrecision` and price step from `tickSize`; Spot derives them
separately from `lot_decimals` and `pair_decimals`. The read counts above refer to
formatter reads, not these derivations or comments.

When these writers are ported, run the existing target with each production
price/amount substitution before adding a repair. Record any surviving mutation.
Use live fractional cases with several distinct step pairs, both sides, limit,
stop-market, take-profit, trailing deviation, and trigger amendment paths. Verify
that the actual derived outputs differ for the chosen inputs; unequal step values
alone do not prove that. Assert native request fields before JSON/roundtrip
projection and require each mutation to compile, fail with exit 101 at its named
assertion, restore original bytes, and pass. Keep one Node process per corpus.


Spot candle read follow-on: `spot_reads.rs` reads market `info.pairName`, with zero
price-step/amount-step reads and zero step-adjust calls. Candle price parseFloat
is scalar parsing, not a two-axis formatter. This census does not claim future
order fields are swap-proof; the preconditions above still apply to each writer.
