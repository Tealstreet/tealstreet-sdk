# Binance portfolio-margin best quotes

PM uses USD-M public market data. Its store ticker's bid and ask come from
`<symbol>@bookTicker` on the public socket, for every discovered market. These
subscriptions work without private credentials or an order-book subscription,
replay after reconnects, and expand when discovery adds markets.

Binance documents the all-market `!bookTicker` stream at a five-second cadence
and individual symbol book tickers as real time. Using the all-market stream
can leave ticker quotes behind the independently streamed depth book during a
price move. See [Binance's public stream documentation](https://developers.binance.com/en/docs/catalog/core-trading-derivatives-trading-usd-s-m-futures/api/ws-streams/public).

REST `ticker/bookTicker` still seeds startup prices. Daily ticker and mark-price
frames update their own fields without overwriting PM bid/ask. The fixtures in
`tests/pm_ticker_bidask.rs` model a 0.1% move between all-market samples and
assert that the ticker and depth book agree on the venue's best prices.

The PM replay adapter in `sdk-rs/harness/binance-protocol.mjs` derives the same
real-time subscriptions from the TypeScript reference's own market catalogue.
Every per-symbol topic remains visible to strict replay comparison.

# Native order-plan validation

Spot, USD-M, COIN-M and portfolio-margin placements materialize every native
lot, batch, bracket and algo child before the first mutation. Native LOT_SIZE,
MARKET_LOT_SIZE, PRICE_FILTER and notional filters are checked at the rounded
wire values. Missing or malformed filter metadata refuses the command before
transport. Options retain their own market bounds.

Splits floor the native maximum to its grid and rebalance a short tail while
conserving the total quantity. Notional bounds use checked decimal arithmetic;
a valid decimal boundary cannot be rounded into a larger minimum. Limit rows
use their wire limit price, Spot algo rows use their wire stop price, unpriced
derivative rows use mark, and inverse contracts use their contract value. Spot
market-notional enable flags and explicit reduce-only minimum exemptions are
preserved; quantity bounds still apply to reducing orders. See Binance's
[Spot filters](https://developers.binance.com/en/docs/products/spot/filters) and
[derivative minimum-notional refusal](https://developers.binance.com/en/docs/products/derivatives-trading-portfolio-margin/error-code#-4164-min_notional).

Ordinary placement jobs submit their exact immutable envelopes to the engine's
[write-plan gate](../../engine/write-plan.md). Each native batch row has its own
indexed outcome. Native acknowledgements must contain a positive identity for
the endpoint; a planned client ID alone cannot prove acceptance. Returned IDs
retain the venue adapter's identity namespace, including Spot/PM client IDs.
Definite row refusals and ambiguous replies stop the untouched suffix without
resend, cancel or rollback. Mixed replies retain every acceptance in the already
dispatched batch; unattributable rows remain unknown. The engine keeps ownership
of signing, HTTP release and command completion.

Cancel-replace validates the complete predictable replacement before cancel.
After a confirmed cancel, a newly observed fill can reduce the remaining size;
the replacement must still satisfy every native quantity and price rule.
