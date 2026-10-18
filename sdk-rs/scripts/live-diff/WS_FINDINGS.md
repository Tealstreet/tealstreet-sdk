# Public WebSocket findings — 2026-10-18

Each venue has a live capture window of at least 60 seconds after subscription
registration, followed by two fresh TS and two fresh native replays. This is
live evidence of remaining differences, **not a full parity PASS**.

| Venue         | Captured frames | Completed barriers TS/native | Finding                                                                                                                                                                                                                                              |
| ------------- | --------------: | ---------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Binance USD-M |             570 | 2/2 and 2/2                  | Observed TS sequences contain 237 books and 149 candles; observed native sequences contain 1 book and 0 candles, repeating within each side. These are provisional windows, not proven frame loss. Store delivery counts/clock fields do not repeat. |
| Bybit         |          61,518 | 2/2 and 0/2                  | Both native barriers time out after 60 seconds. Captured input includes 357 publicTrade frames; partial native outputs cannot establish final data parity.                                                                                           |
| OKX           |         285,713 | 2/2 and 0/2                  | Both native barriers time out after 60 seconds. Public and business sockets replay on one ordered timeline; partial output comparisons remain diagnostic.                                                                                            |
| Hyperliquid   |             496 | 2/2 and 1/2                  | One native barrier times out; native book/candle sequences also differ between repeats. No final channel parity attribution.                                                                                                                         |

The four primary runs use owner-confirmed immutable addon SHA
`1129c766c0d0d31394960064e968245ee25f0480aaf72c85528415f07d0bc796`,
source label `0d9632aca90b99c708178554af614152cacb8151` (#1017 candidate).
All four workers in every primary case receive every captured frame with the
same input hash and zero REST replay misses. This proves terminal delivery;
a command barrier can overtake unread socket frames and does not prove native input consumption. All comparisons therefore remain provisional, even where all four command barriers completed.

The same 570-frame Binance tape was also replayed against local addon SHA
`21f25569e48100cd2b11ca7d63cb9be469d92ce2da8a8c1367a1466cf8310e18`,
source `33c2ae3eed7cc07e39ad7587089e0935dd6da080`. Both native barriers timed
out, so its 1-book/4-candle observations are partial only. That baseline replay
overlapped the Bybit replay; load sensitivity must be considered before source
attribution. The observations do not establish that #1017 introduced the loss.
Native Binance logs include stale-receive/reconnect messages during replay.

Wdtab1 subsequently replayed the 570-frame candidate in isolation: both native
barriers timed out. Its 3-second CPU profile points to `markPriceUpdate` →
`native_store::Graph::update_ticker` → `Graph::notify` →
`project<StoreData>/to_typed_preserving_undefined` → `typed::capture`,
walking the whole Store per ticker update. The isolated report/profile paths
are in structured evidence. This supports a backlog finding; it does not prove
dropped subscriptions. A genuine native input-consumption fence is required
before final ordered output parity can be claimed.

[Structured evidence](ws-public-2026-10-18.json) records every input receipt,
output artifact, barrier, exact channel diff and within-side repeat comparison.
The `.cache/live-diff/<venue>-streams-60s.json` reports retain fuller diagnostics
and subscription bytes. Exact public frame bytes, binary flags, REST bodies and
raw V8 output sequences are preserved in the ignored fixture paths referenced
there. No credential access or order write occurs in this probe.

Reproduce without new exchange data from this worktree:

```sh
node sdk-rs/scripts/live-diff/streams.mjs --venue binance \
  --replay .cache/live-diff/fixtures/binance/df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f.bundle.v8 \
  --addon /absolute/path/to/the/recorded/addon.node \
  --rust-revision ACTUAL_BUILD_SOURCE --output .cache/live-diff/binance-replay.json
```

The other venue bundle paths are in structured evidence. Use the addon with the
recorded SHA and the same Node major version. Wall-clock timestamps and the
production 250ms Store throttle remain uncontrolled inputs: Store sequence
repeat failures are explicit, never normalized away or called final native
mapper bugs. A controlled clock/timer boundary is still needed for full exact
Store determinism. There is no standalone public trade listener in the SDK;
Bybit trade inputs are checked through its production derived-candle path.

Venue/subscription/liveness evidence is routed through Aimux to `codex-wdtab1`;
Store/emitter/clock evidence to `codex-dwfl8p`. Both owners require baseline
validation before attributing a production cause. The #1017 scope remains frozen.

An independent fresh Bitunix read rerun using wdtab1's `e4542a3` / `a7743c55`
addon confirms all 697 markets and all 697 tickers exactly, including string
`limits.amount.max`. Report: `.cache/live-diff/bitunix-max-confirmed.json`;
owner PR #1062 records that live confirmation.
