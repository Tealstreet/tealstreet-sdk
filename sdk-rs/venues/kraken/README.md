# Kraken venue adapters

## Position liquidation prices

Futures [`open_positions`](https://docs.kraken.com/exchange/api-reference/futures-websocket/open_position)
publishes `liquidation_threshold` in the instrument's quoted price units.
Only finite, positive numbers and numeric strings are usable prices.
Missing, null, empty, malformed or nonpositive thresholds are unknown. Some
Futures accounts publish `0.0` when no per-position threshold exists; a zero
or negative price cannot be used as a position's liquidation price. Normalize
these sentinels to unknown without preventing the position from updating.
This deliberately corrects the TS reference's preservation of explicit zero.

REST [`openpositions`](https://docs.kraken.com/api-reference/account-information/get-open-positions)
does not publish that threshold. It preserves a previously received WS value;
without one, the price is unknown. [`accounts`](https://docs.kraken.com/api-reference/account-information/get-wallets)
has different wallet shapes: `flex` / `multiCollateralMarginAccount` publishes
portfolio and margin totals, while single-collateral `fi_*` margin accounts can
include account-level `triggerEstimates.lt`. The TS reference uses accounts for
balances, not liquidation-price derivation; this adapter likewise does not
invent a per-position estimate from those totals.

Unknown prices use the packed frame's NaN representation and serialize to JSON
`null`. The SDK frame applier preserves that as `Position.liquidationPrice: null`;
finite positive prices remain numbers. Spot currently exposes wallet
balances and an empty positions table, without margin-position REST/WS mapping.

Kraken Spot treats a dispatched write's HTTP 408, 429, or 5xx response as
`Outcome::Unknown`, even when the body contains a native signature, credential,
or rate-limit error. HTTP 429 retains `ErrorKind::RateLimited` for diagnostics.
These responses cannot authorize a resend or terminal credential rejection;
uncertain placements trigger an `OpenOrders` read before another placement.

Definitive signature refusals retain the single re-sign attempt. Private reads
also retain their existing bounded signature and transport retry policies.

Futures writes likewise keep HTTP 408, 429, 5xx, and native `apiLimitExceeded`
responses unknown, including a bracket leg after an accepted entry. Their user
text says the outcome is unknown and directs the caller to Check Open Orders.
These failures never resend or place a replacement stop. Definitive refusals and
private reads retain their existing behavior.

All Futures Unknown trading failures include reconciliation guidance, including
HTTP 200 native server errors and unproven cancellation statuses. Batch errors
retain an unknown leg's message and native diagnostics together when a later
leg is definitively refused.
