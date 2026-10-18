These are unchanged public response bytes and four selected message frames from
the Binance 570-frame live capture. The source bundle SHA-256 is
`df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f`.
`binance-nested-socket.json` retains each frame's original connection, timestamp,
binary flag and base64 bytes, plus response URL keys and SHA-256 hashes. It selects
the first depthUpdate, markPriceUpdate, kline and 24hrTicker messages in order.
The replay ends one second after the last selected frame.

The market and candle response files are shared with the request-stats regression.
The other four files retain the 24-hour tickers, book tickers, premium indexes and
BTC depth snapshot used by the real startup and subscriptions. No response row or
frame body is filtered or synthesized.

`native-binance-nested-socket.test.mjs` runs the maintained TS exchange/worker and
compiled napi account on these inputs using the existing controlled TLS relay.
Both sockets must open, all four frames must finish their handlers without errors,
and the nested candle subscription must deliver. It compares the actual
worker-delivered public handler stats and public-health timestamp. This is a
bounded grouping regression, not a full 570-frame equality or arbitrary-task drain
claim. The separate missing own-undefined health fields are outside its scope.
