# BingX Spot protocol fixtures

Source: BingX's official [Spot v3 documentation](https://bingx-api.github.io/docs-v3/#/en/Spot/Introduction),
including the published examples in its
[documentation bundle](https://bingx-api.github.io/docs-v3/static/js/app.9149f5bb7dc21a77fc01.js).
Supporting references are linked in `../../../SPOT.md`.

- `markets.json`: the BTC-USDT `common/symbols` response example.
- `tickers.json`: BTC-USDT `spot/v2/quote/ticker`, including the percent suffix.
- `balances.json`: the USDT row from `account/balance`; CHEEMS and VST rows are
  omitted so the fixture tests USD aggregation without an unrelated valuation.
- `orders.json`: the BNB-USDC `trade/openOrders` example. Tests that seed only
  BTC-USDT explicitly substitute its symbol to isolate order mapping.
- `order.json`: the NEMS-USDT `trade/order` response example.
- `fills.json`: the first BTC-USDT `trade/myTrades` example row.
- `candles.json`: the first two rows of `spot/v2/quote/klines`, newest first.
- `depth.json`: published `spot/v1/market/depth` levels. Its descending asks are
  intentionally retained so consumers must sort them.

Native order IDs represented as hexadecimal JavaScript literals in the bundle
are converted directly to exact decimal JSON integers, without executing JS or
rounding through a double. Tests add `9007199254740993` as an exact-ID control.
Envelopes omit unrelated debug and timing fields. Private listen keys and
signing credentials are synthetic fixture values; there are no live keys.

The WS controls in `tests/spot.rs` retain the documented Spot `dataType` and
field shapes for ticker, full depth, kline and execution-report events. Prices,
quantities, symbols and timestamps are adapted where stated by the tests to
exercise duplicate fills, zero-fill orders and causal races with one catalog.
