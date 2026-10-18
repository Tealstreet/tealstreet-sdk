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
produce identical 624 outputs: 238 Store snapshots, 237 books and 149 candles.
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

The matched release check uses three interleaved control/candidate reports on the
same original 570-frame bundle and the same process CPU/wall overlay from
`e5da2bf716eab49d0b9d7ddacf68df25b549a35c`. Both artifacts use the locked
default optimized release profile with the node feature, Rust 1.99.0 and
CARGO_INCREMENTAL=0. The otherwise-matched control is normal master plus the
checked raw-task receipts; it intentionally lacks the health change.

- control: source `5d99568297c27993eb40b5a2a62b41b7e30789b0`, addon SHA-256 `571a76ef5f0161708e2a77ed1050eab287c94a97a7273d00fa112fba6ee8b945`.
- health: source `110994b364e13dcd0e6b3604d7c389d3acae931d`, addon SHA-256 `b0c17d3825f7bc42a96a248e43abcb7d4e8a4fb1a585eabce66f1421491e1d6a`.

All six reports pass strict parser, physical, Promise-job and raw-task drain
checks. Every candidate channel has complete source parity; the control differs
only in the expected health channels. Complete serialized V8 outputs repeat
byte-for-byte within each native side. Across sides the checker allows exactly
the intended health delta: three own undefined keys in each of 238 notifications
and the final state (717 additions), with the source's 14-key order; unrelated
values, property order and output sequences must remain identical.

Native samples below are process CPU and wall milliseconds around feed-to-drain.
Each report has two fresh native workers. All three pairs are retained without
normalization, warm-up exclusions or selection.

| Pair / worker | Control CPU | Health CPU | Control wall | Health wall |
| --- | ---: | ---: | ---: | ---: |
| 1 / 1 | 15649.361 | 16337.599 | 38710.414749999996 | 56502.175791 |
| 1 / 2 | 14297.31 | 14669.27 | 34892.088040999995 | 51374.355833999995 |
| 2 / 1 | 14735.26 | 14684.634999999998 | 37088.300166 | 38183.13 |
| 2 / 2 | 15951.955 | 14768.777 | 40371.007790999996 | 40053.474792 |
| 3 / 1 | 16844.682 | 16361.956 | 46653.866209 | 54585.818583 |
| 3 / 2 | 17119.631 | 15166.907 | 49152.357542 | 48756.931500000006 |

Median native CPU is 15.80s control / 14.97s health (ranges 14.30–17.12s /
14.67–16.36s). Median wall is 39.54s / 50.07s (ranges 34.89–49.15s /
38.18–56.50s). Source control CPU medians are 2.35s / 2.23s and wall medians
7.44s / 8.81s. No local Cargo runs during measurement. Another lane's existing
OKX release replay and a peer's jobs1 Clippy gate overlap; CPU is the primary
metric and the broad wall spread is qualified as shared-machine load. This
health check makes no isolated speedup or latest-master performance claim.

Reports and raw retained outputs are preserved under
`.cache/health-release-timing/{control,health}-{1,2,3}.json`; `summary.json` records
CPU user/system/total, wall medians, min/max and every source/native sample.
The first release named-fixture attempt hit the unchanged 15s source-worker open
deadline before any timing feed; the retained retry passes all seven checks.
The first measured control report returned the expected DIFF exit code; its
original sample is retained and the sequence resumes at candidate 1.

The comparator's negative controls mutate copies of actual measured retained
outputs and recompute their hashes. It rejects an unrelated final-state latency
change at `$.state.latency`, removal of own undefined privateLastMessage at the
source health-order check, and key-order-only mutation in candidate worker 2 at
the complete retained-byte repeat check. Node deep equality alone accepts that
last reordering. Original measured reports and outputs remain unchanged.

After normal current-master inheritance, the real accepted-defaults test first
rejects the obsolete whole-file source fingerprint (10e9ec73 to f2bfff19). The
reviewed Exchange declaration through line 521, all 53 default APIs/bodies and
16 classifications are byte-identical; refreshing only the pin and its scoped
evidence makes the same real test pass. The health runtime delta is unchanged
from the frozen release pair.
