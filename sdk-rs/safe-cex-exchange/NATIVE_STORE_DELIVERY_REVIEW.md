# Native Store notification delivery

The internal BaseExchange update listener and native account observer previously
subscribed to typed Store snapshots. Every ticker write therefore walked the
whole Store before the emitter or throttle could run. The native subscription
now borrows the canonical state referent from the same ordered listener registry.
The account throttle retains that referent and projects it only when its actual
leading or trailing callback invokes. Explicit typed subscriptions still project
at their reached listener slot. There is no typed cache or fallback graph.

`core_notification` constructs the production DefaultStore and BaseExchange:
an unrelated throwing getter is never visited, repeated events retain the live
state referent, and the first listener throw preserves its original cause and
committed mutation while preventing later subscribers. The getter case compiled
RED on parent `4abd3fe9c6` and GREEN with this change. The maintained TypeScript
subscription statement runs against the actual Store and TinyEmitter in
`tests/native_store_emitter_source.mjs`. `store_delivery_native` sends a
100-update burst through the shared throttle and observes only the leading and
latest trailing projection, referent identity, and cancellation. Native account
tests exercise the real registry delivery path.

## Recorded production addon receipts

The addon was built from committed source `bb07e4424f`; subsequent changes add
proof tooling, metadata, and master integration. Its immutable SHA-256 is
`6c522f1b1fadb997ac0745ad0c7f7cb710501448e851393f43380bcdf68abe93`.
The compact receipt artifact is
[`native-store-delivery-2026-10-19.json`](../scripts/live-diff/native-store-delivery-2026-10-19.json).
It pins report hashes, exact remaining differences, and native parsed/processed
and physical transport receipts, with zero pending source jobs at final drain.

| Input | Before | Candidate receipt and business outputs |
| --- | --- | --- |
| Binance 570 frames | Master and canonical-Store parent both exceeded the 60-second final barrier; samples walked typed Store snapshots per ticker. | Both native runs fully drain; 237 book events, 149 candles and 238 ticker Store deliveries exactly match TS. Independent final barriers are about 360 ms. |
| Lighter recorded startup | Both native final barriers exceeded 60 seconds. | Both native runs fully drain the single recorded greeting; 213 markets, 213 tickers and 197 live tickers match TS. Ticker Store payloads match exactly; final barriers are about 352–354 ms. |
| Apex recorded startup | Native processing exceeded the 60-second receipt deadline. | Both native runs fully drain all three frames; 89 markets and 89 live tickers match TS. Ticker Store payloads match exactly; final barriers are about 355–362 ms. |
| Bybit 61,518 frames, independent replay | Not used as a separate before claim. | Both native runs fully drain; 312 book events, 408 candles and 241 ticker Store deliveries exactly match TS. Both combined 961-delivery sequences repeat exactly. |

Lighter/Apex use `legacy-startup-bundle.mjs` to retain the exact recorded HTTP
bytes, validated frame hashes and relative frame times. The old recorder omitted
the stable Apex `v=2` query; the converter restores it by evaluating the actual
maintained `BASE_WS_URL` initializer and verifies the host/path. These are startup
receipts, not a 60-second market-data coverage claim. The reports retain raw V8
output artifacts, rather than replacing values with counts.

These are qualified liveness and business-output results, not a full Store parity
claim. Remaining repeatable TS/native differences are requestStats counts,
socketHealth own-undefined versus absent fields and a timestamp, native-only
Binance message-handler statistics, and Apex pong `hadHandlers`. They are recorded
exactly and assigned separately. One local Binance replay also swapped only
combined-stream indices 1/2 (Store versus orderbook), while every individual
channel repeated exactly; the independent replay's combined streams repeated
exactly. The two independent NAPI queues are a separately assigned FFI ordering
repair. No payload normalization or tolerance hides these differences.

Final-barrier duration is **not** total tape wall time. Repeated facade allocation
remains a measured hotspot and is the immediate throughput follow-up; that PR
must compare complete 570-frame wall time against master.

## Bybit options: NOT GREEN

The exact V2 options bundle is
`34e3c9b4935ae3f6c10dff0ba68bdf1c99125658beacbd7bb6aa02709eaac3ca`.
Both native controlled-clock runs still time out at 60 seconds after 19 successful
HTTP replies, with later reads explicitly NOT_RUN. A real-time-clock control also
times out, so the remaining failure is not specific to advancing the replay clock.

An instrumented clock driver records an attempted 17-second advance without a
reply. A later three-second sample at 40 seconds after launch has 2,079 samples
under native account refresh, 1,699 under BaseExchange refresh, and 1,537 under
Bybit `invoke<Vec<Market>> -> to_typed -> decode_native/strict_codecs` while
decoding the expected 8,110-market result. This active typed-result phase supersedes an
earlier parked sample as the current diagnosis; it is not evidence of a limiter
deadlock. The remaining typed fetchMarkets cost is assigned to a separate lane.
This PR does not claim options completion or widen that return boundary.
