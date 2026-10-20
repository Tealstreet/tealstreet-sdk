# KuCoin Spot parity

`kucoinspot` uses `safe-cex-venue-kucoin`'s optional `spot` feature and the
independent `KucoinSpot` state machine. Classic futures remains `Kucoin`.
The split keeps spot-only code outside futures-only builds.

The oracle is the current TS HF adapter in
`packages/safe-cex/src/exchanges/kucoin/kucoinspot.*`.

- Public symbols and all-ticker seeds stream through typed decoders. Symbols
  preserve their native `BTC-USDT` identity; quantities are base units.
- Both sockets acquire fresh bullet tokens on recovery, keep explicit public
  and private roles, subscribe once per welcome, and enforce ping deadlines.
- Public books match TS's supplied `/spotMarket/level2Depth50` snapshots;
  each update replaces both sides, with refcounted subscriptions. This is the
  adapter's documented fifty-level feed, not the futures incremental book.
- Trade accounts publish currency balances, coin holdings, and USD values from
  the same stablecoin/ticker precedence as TS. The fixed
  isolated/one-way metadata of TS zero-size positions lives in SETTINGS; the
  open-position table remains empty and leverage remains one.
- Optional secondary credentials sign private GETs; unsigned public GETs use the
  secondary client's route too. Token requests, order writes, and sockets retain
  primary identity/routing. Complete secondary credentials and passphrase are
  validated before startup I/O; an omitted read passphrase inherits the primary.
- Open-order reconciliation discovers active symbols first and walks numbered
  pages of fifty. Failed or malformed pages preserve the good snapshot.
  Revision fences prevent an older response from overwriting socket updates;
  bounded terminal identities prevent resurrection after terminal events.
- HF placement honors caller `clientOid` verbatim, validates the whole batch,
  and rejects unsupported trigger/bracket shapes. Alteration is the native
  `/hf/orders/alter` operation, preserving identity; cancel-by-ID retains caller
  symbol context even before the order is observed. Symbol and account-wide
  cancellations use their native endpoints. Writes never retry after an
  ambiguous failure.
- Historical executions use TS's symbol-required `/hf/fills` request, its
  single-page limit of one hundred, base quantities, and millisecond timestamps.
- Client-ID lookup matches TS's cache-first, open-only lookup: misses are
  explicitly non-definitive. Account summary returns main, sub-account and
  referral identities from `/api/v2/user-info`.
- Setting hedge/margin mode remains the TS no-op; setting leverage publishes
  one. The engine never sends derivative settings writes for spot.

TS does not implement spot order history, income/PnL/funding history, or public
trades. Order/income capabilities retain the branded base fallback; public
trades rejects as TS does. No invented native endpoints fill those gaps.

The Rust path deliberately fixes malformed-data safety bugs in TS: duplicate
execution replay is deduplicated; older reconciliation cannot roll back fills;
failed active-symbol discovery and malformed order pages preserve good state.
The shared order geometry check covers partial private rows as well as REST;
execution price and size must be positive. Required REST account amounts reject
missing, null, empty or negative values instead of silently becoming zero.
Balance socket notifications are partial by design and trigger a fresh REST
trade-account read. Zero-size terminal order notifications remain valid.
Current open-order matches validate financial geometry and the known execution
floor before emitting or consuming a trade identity. Execution bounds use only
explicit native totals and cumulative fills, with decimal tolerance; delayed
unique executions survive newer-order and terminal fences when optional native
quantities are absent. A rejected packet leaves its identity available for a
corrected execution.

The engine names real `https://api.kucoin.com` and bullet-supplied WebSocket
URLs. The existing TS-resolved Tealstreet host transport applies account routing.
Signing reuses the futures API-key-v2 signer and optional broker headers.

## Evidence

The native suite covers public mapping, subject-based all-ticker updates,
book replacement/refcounts, candle ordering/units, holdings valuation,
native placement/alter/cancel payloads, batch preflight (including rejection of
IV-priced orders before any write), ambiguous writes,
late execution delivery, reconciliation failure/staleness, lookup scope,
fixed settings, account identity, and fresh-token watchdog recovery.

`sdk-rs/harness/private-tape-kucoinspot.mjs` emits a deterministic forty-six-record,
thirteen-command private tape. `sdk-rs/harness/kucoinspot.mjs` independently
checks signing and bullet/subscription protocol; it performs no financial
output rewriting.

Both public gates pass three runs with standard egress on fresh twenty-second
captures at `f920bcf5e5`, using the landed `finalizeReplay` measurement path. The [gate evidence](KUCOIN_GATES.json) records exact source, artifact,
tape hashes, audit results, and median measurements:

| Venue | Main CPU TS / Rust (ms) | Steady CPU TS / Rust (ms) | Retained TS / Rust (bytes) | WASM bytes |
| --- | --- | --- | --- | --- |
| Spot | 472.816 / 420.256 | 302.067 / 270.743 | 18,509,121 / 4,249,156 | 687,701 |
| Futures | 381.541 / 193.144 | 231.219 / 108.682 | 21,400,489 / 4,043,004 | 782,377 |

Spot private correctness passes twenty-two frames and thirteen commands; futures
passes thirty-two frames and seventeen commands. Every request, write,
signature and protocol audit is clean. Both actual Spot SDK/WASM boundary cases
pass, and the native suite passes 149 tests, including 46 Spot tests,
with Clippy warnings denied at the same frozen source as the artifact gates.
The fixed 1,258,291-byte budget is unchanged.
