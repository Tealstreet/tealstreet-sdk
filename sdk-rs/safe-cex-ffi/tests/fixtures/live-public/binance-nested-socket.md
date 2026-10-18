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
worker-delivered public handler stats and the complete socket-health record,
including own undefined privateLastMessage/publicCloseCode/privateCloseCode and
property order. It is bounded to these four frames; it makes no full 570-frame
equality or arbitrary-task drain claim.

The own-undefined assertions compile RED against the prior production addon
`b0b1c0009fddc9f3fa555c6d0894d6ef48464a31d7578b8ed3ac6ead017af4d5`
(source ae2ddb633c). Each of the three source-owned keys is absent in that addon.
The canonical health carrier and source partial merge preserve them through the
production notification and worker delivery path.

The health candidate compiles GREEN from `d05c4cf090` (native addon SHA-256
`829a899fd6ec16f104c09d48127eb39003625713c46e706e777a0e998d3a5b87`):
all seven reported tests pass, including the three individual presence checks and
whole-record values/key order. The existing 5/6-request production Store callback
regression also passes both cases on that same immutable addon.
