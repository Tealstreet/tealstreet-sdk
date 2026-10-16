# WOO precision-axis formatter audit

Scope: implemented Rust at `7d78eeb8e5`, with origin pinned at
`ccfd79d1c5`. This is a callee/data-flow
inspection, not a passing formatter-mutation claim.

Taxonomy: **VALID NEGATIVE**. Neither current swap coverage nor a historically
never-blind corpus is claimed. No fixture values or corpus bytes were changed
for this audit, and no formatter mutation was run because no such site exists.

There are **zero compiled two-axis formatter sites to swap**. A source sweep
for precision, price/amount step/tick names, adjust and round, followed through
all exported WOO modules, finds one precision producer: `map_woo_v3_markets`
builds `MarketPrecision.amount` from baseTick and `.price` from quoteTick. It
does not round or format orders. The existing mapper-identity target reads both
native output fields independently against live source; this audit does not
claim a newly executed mutation for that producer.

The implemented pricing helpers select fair/touch quotes, apply percentage
slippage, and test post-only crossings. They read no precision step. Limit/algo
mappers copy/compute response amounts and prices without quantization. Timestamp
flooring and stats percentage rounding are not price-versus-amount formatters.

Actual TypeScript placement contains the future sites:

| Owning row | Precision inputs | Current Rust row |
| --- | --- | --- |
| WOOXExchange.placeOrder | price and amount, including IOC price and amount splitting | pending, no rs target |
| WOOXExchange.placeStopMarket | price and amount, including split lots/remainder | pending, no rs target |
| WOOXExchange.placeTrailingStopLossOrder | price for callback distance | pending, no rs target |
| WOOXExchange.placePositionalAlgoOrder | price for stop-loss and take-profit trigger fields | pending, no rs target |

Those source paths have four price-step reads and two amount-step reads; none
has a Rust operation yet. They remain pending. This is **not immunity** for the
future formatters. A precondition of their implementation is fractional values that distinguish
several price/amount step pairs, both sides and limit/stop/bracket fields, with
direct native payload assertions and compiling axis-swap controls.

Coverage must be reported per output field: price, quantity, trigger, stop-loss,
take-profit and callback distance wherever each is present. A passing sibling
field is not evidence for another field. This negative census ran zero field
swaps because none of these Rust formatters exists yet.
