# Price/amount precision swap audit

Audited production tree: `26310d938b7ade30d490286ef1537fb2d07e2914`.
Scope: all three venue production files and their compiled module declarations,
plus the actual TypeScript order-placement/signing paths and pending manifests.
PR 7258 remains pinned; this audit is a local follow-up.

## Measured negative result

- Price-precision or price-step reads in venue Rust: **0**.
- Amount-precision or amount-step reads in venue Rust: **0**.
- Compiled formatters consuming both axes: **0**.
- Applicable production precision-swap mutations: **0**.

`src/lib.rs` exports only constants and types. `src/constants.rs` contains
configuration, endpoint strings, lookup declarations and EIP-712 field names.
`makerAmount` and `takerAmount` there are schema names, not arithmetic.
`src/types.rs` decodes seven wire/auth records; price and book size remain
strings, and its Number fields are auth nonce/timestamps or price timestamp.
There is no order payload builder, rounding, precision derivation or step lookup.
Shared numeric-ingress registration is not a price/amount formatter.

Accordingly there is no existing Rust formatter target against which an
applicable production precision swap can compile and survive or fail. No
precision-swap mutation was invented, and no test rerun is claimed for this
source-only audit. Existing wire-field swap controls address field identity,
not the distinct-step precision class. **No sites to swap yet is not swap-proof.**

## Deferred source paths

Actual `polymarket.exchange.ts` placeOrder reads
`market?.precision?.amount ?? 2` for `Number(opts.amount.toFixed(pAmount))`.
It separately normalizes `market.info.tickSize` and passes it to
`adjust(opts.price, tickSize)`, bounds checks and createSignedOrder. The signing
helper passes price, size and tickSize to the official V2 order builder.
`precisionFromTickSize`, `normalizeTickSize`, `PolymarketExchange.placeOrder`
and `createSignedOrder` all remain pending in this venue's manifests.

The actual safe-cex source accepts limit orders only and rejects triggerPrice,
stopLoss, takeProfit, reduceOnly and close options before formatting. A future
port must witness those rejections, not add unsupported stop-market or bracket
formatters merely to populate the fleet test matrix.

Before marking the formatter rows implemented, execute the actual TypeScript
with fractional inputs across multiple distinct price-tick/amount-step pairs
and both Buy/Sell sides. Confirm each pair produces different results when the
wrong axis is used; merely assigning unequal precision values is insufficient.
Assert native price, size and signed/request payload fields independently of
serialized roundtrips. For each derived-axis use, compile a production swap,
require exit 101 at a named native assertion, restore and pass. Include the
unsupported stop/trigger/bracket rejection paths and the official builder seam.
Capture the entire corpus in one Node subprocess. Probability normalization
and payoff remain owned by prediction-core.
