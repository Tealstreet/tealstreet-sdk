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

The engine names real `https://api.kucoin.com` and bullet-supplied WebSocket
URLs. The existing TS-resolved Tealstreet host transport applies account routing.
Signing reuses the futures API-key-v2 signer and optional broker headers.

## Evidence

The native suite covers public mapping, subject-based all-ticker updates,
book replacement/refcounts, candle ordering/units, holdings valuation,
native placement/alter/cancel payloads, batch preflight, ambiguous writes,
late execution delivery, reconciliation failure/staleness, lookup scope,
fixed settings, account identity, and fresh-token watchdog recovery.

`sdk-rs/harness/private-tape-kucoinspot.mjs` emits a deterministic forty-six-record,
thirteen-command private tape. `sdk-rs/harness/kucoinspot.mjs` independently
checks signing and bullet/subscription protocol; it performs no financial
output rewriting. Final gate evidence is recorded in the parity-gaps plan/PR.
