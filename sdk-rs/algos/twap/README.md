# Shared TWAP validation

Before admitting a schedule, TWAP materializes all remaining quantities on the
native amount grid, carries each slice's dust forward, and checks every resulting
write. A quantity plan that loses total size or violates a native minimum,
maximum or grid is refused before any slice or chaser starts. Restore and resume
use the same gate. Typed starts return `VenueError` with `invalidInput/notSent`;
the existing hosted operations continue emitting their refusal as an error log.

`plan::validate_write(&PlannedOrder)` is pure and matches the shared engine
`WritePlan` validator signature. Limit prices are normalized before validation;
the dispatched request uses that same prepared payload. Price/tick and quantity
checks apply independently of notional status. Native marketable limits also
carry their normalized execution price. Each instance exposes all admitted
children through `planned_writes()`, including the first released child and its
client ID.

Native `MarketOrderFacts` travel directly from the engine Store through the
shared Rust projection to AlgoStore and CxView. They never enter serialized
markets, binary frames, TS bindings or generated types. Native metadata comes
from the account store, never the caller's `activeMarket` object. Missing markets
refuse. Each child's notional status is explicit: `Unknown` means "notional
unknown", `Exempt` records a native exemption, and `Required` retains the minimum
and quote value per placement unit. A known minimum with no usable reference
refuses before any write. Reduce-only alone never grants an exemption.

This PR populates facts already held by Binance, Bitget, BingX, AsterDEX, Lighter
and Hyperliquid. Binance COIN-M and Bitget inverse keep their native quote units;
Lighter uses its existing side-dependent market price offset and rounding.
Binance and AsterDEX expose market-lot rules separately from limit-lot rules.
Binance Spot's explicit market exemption does not exempt limit orders.

OKX, Blofin, Bybit, Paradex, Backpack, Bitunix, Crypto.com and KuCoin Spot still
need their native notional ingestion/policy exposed; their absent facts remain
explicitly `Unknown` in every child plan. This phase still awaits the shared
`WritePlan` outcome journal to complete the
[split-plan invariant](../../../docs/plans/SPLIT_PLAN_INVARIANT.md).

Prices and fills can change after admission. Each scheduled release rechecks
the remaining plan with the current view and stops on predictable invalidity.
Existing uncertain placements are never replayed on restore. Runtime outcome
reporting will use the shared journal; TWAP has no local replacement journal.
