# BingX price/amount precision swap audit

Audited production tree: `831c0dc80a7ee25cc1399e54025a82eeadc1940c`.

Historical result at that SHA: **0 price-step reads, 0 amount-step reads, 0 compiled two-axis
formatter sites; 0 fields swapped**. No production precision swap can be constructed at this
scope. No mutation target was run and no swap-proof claim is made.

Method: followed all four production module declarations from src/lib.rs,
inspected api.rs and errors.rs functions, and checked constants.rs and the
Serialize/Deserialize/numeric-output implementations in types.rs. The API module
contains only key collation, errors contains only message construction, constants
contains declaration tables, and types contains wire records and ingress/output
adapters. Trigger/stop names in constants are mapping entries, not formatter
consumers. Precision/step/round/adjust searches were cross-checked against this
module inventory and the actual parsed TOML mappings rather than used alone.

The TypeScript source has future sites that this zero does not cover:

- formatCreateOrder reads price and amount precision separately; it formats the
  limit price and stop-loss/take-profit stopPrice using pPrice, and quantity,
  remainder and lot size using pAmount.
- replaceTriggerOrderPrice and cancelReplaceLimitOrderPrice each adjust both
  quantity and price.
- formatCreateTrailingStopLossOrder adjusts price distance using price precision.
- updateOrder consumes amount precision; deriveTickerBidAsk consumes price
  precision; fetchMarkets derives both market precision axes.

All seven corresponding manifest rows remain pending with empty Rust mappings.
Before claiming any of these future formatter rows, preserve the original corpus
byte-for-byte with its case count, byte length and SHA256. Audit each payload
field separately (price, quantity, trigger, stop-loss and take-profit), not the
venue as a whole. Use actual TypeScript
fractional cases across several distinct price/amount step pairs, both sides,
limit and stop-market, including stop/trigger/bracket fields. Check decoded
native payload values. For each production precision substitution, prove it
compiles, fails the named assertion at exit 101, and restores/passes. Test inputs
must yield different rounded outputs for the swapped steps, not merely different
step declarations. Existing declaration tests do not establish this property.


## Synthetic ticker utility (subsequent slice)

The historical zero above no longer describes the whole implementation.
derive_ticker_bid_ask now consumes price precision to derive two fields, bid and
ask. It does not round an order quantity or format orders. Each field was audited
independently by replacing its production price_step operand with amount precision.

- bid: compiled; original 168-case target failed exit101 at
  BINGX-PRECISION native bid; restored/passed.
- ask: compiled; original 168-case target failed exit101 at
  BINGX-PRECISION native ask; restored/passed.

Classification: ALREADY COVERED by this helper's introduction corpus, per field.
No claim is made about earlier unimplemented code or pending order formatters.
Zero fixture-value repairs were needed. The original actual-source capture is
preserved as tests/fixtures/ticker-precision-original.json: 168 cases, 24043 bytes,
SHA256 de6b6cf9e523e1fe9164d86ddb8021a5b20c00b6460eecddd1feb267d7d5f037.
It is historical evidence, not an expected-value substitute for live TypeScript.
The actual-source oracle stayed byte-identical through both controls (SHA256
7349288ee7f534e4fd60fc38921fa2f38bad667825c978addd70085b57bc64fb).

The corpus includes four finite distinct step pairs, fractional last prices,
missing precision, nonpositive/nonfinite last prices and steps, signed zero,
subnormals and overflow. Native bid/ask binary64 comparisons normalize only NaN.
Node v26.7.0 and darwin arm64 are recorded in the capture. The six remaining
previously inventoried rows remain pending; future order-field controls are still
required independently for price, quantity, trigger, stop-loss and take-profit.
