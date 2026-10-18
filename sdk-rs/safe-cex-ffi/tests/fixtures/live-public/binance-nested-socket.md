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

The health candidate compiles GREEN from `4573a92bae` (native addon SHA-256
`aa09ab469195c89b674d19f166818c77b7ab5f0a2c5ffb511cb95ff2779e998f`):
all seven reported tests pass, including the three individual presence checks and
whole-record values/key order. The existing 5/6-request production Store callback
regression also passes both cases on that same immutable addon.

That source normally inherits the strict raw-task receipt API from #1154. The
compiled replay API regression passes on the same addon, preserving parser
histories, raw task classification and the source clock controls.

The independent live-diff lane also replays the original 570-frame bundle. Its
strict pre-fix replay (`health-strict570-red.json`, source f262, addon ad989)
drains all 570 physical frames and 238+331 parser messages, with no pending jobs,
unknown/finite/ready/blocking raw tasks, or parser errors. Both native repeats
produce identical 624 outputs: 238 ticker snapshots, 237 books and 149 candles.
The only cross-language discrepancy is 714 missing health properties across
238 Store snapshots. This separates the property loss from processing backlog.

On the health candidate above, `health-strict-prefix-green.json` independently
checks the receipt API and the same 12-frame/6-HTTP prefix. Both native repeats
drain strictly and produce identical ten outputs; complete TS/native outputs
and all four Store health records match, including own undefined values and key
order. This short prefix is a smoke check, not a 60-second coverage claim.

The subsequent `health-strict570-green.json` and
`health-strict570-assertions.log` confirm the complete original 570-frame replay
on the same candidate. Both strict native drains finish with the same physical
and parser counts as RED and no pending jobs or blocking raw work. Both TS and
native repeats agree exactly; the complete 624-output cross-language sequence
also agrees, including all 238 health records and their property order. All three
previously missing properties remain own undefined. Request stats, handler stats,
tickers, books, candles and the other Store fields also match without normalization.
