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
