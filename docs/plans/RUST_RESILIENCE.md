# Resilience lane: backoff, private rows, startup remainder

Scope: `RUST_QUALITY.md` shared helpers 3 and 5; `RUST_AUDITS.md` cross-venue
"rows and subscriptions" and "per-coin balances and snapshot starvation"; the
private-row, app stop payload, BreakoutProp authenticated startup and MEXC
bootstrap remainder handed over from the public-startup work
(`RUST_PRIVATE_ROWS_TICKERS.md`, `RUST_APP_STOP_PAYLOADS.md`).

## PRs

1. `engine::backoff::Backoff` (#1391): one formula for the inline exponential
   retry delays (REST reads, snapshot retries, startup discovery) and the socket
   supervisor, constants unchanged for every reachable attempt. Socket
   reconnects all moved onto `engine::socket::Supervisor`, which owns their
   jitter (75-100% of the delay) through `backoff::jitter`.
2. App stop payloads (#1396, merged): protective orders accept the app's
   `triggerPrice == price`; Kraken Spot refuses a mismatched trigger; Lighter
   refuses trailing stops instead of placing a market order.
3. Per-currency balances on Crypto.com, GRVT, MEXC and Phemex (#1399, merged).
4. Snapshot starvation (#1419, merged): `engine::fence::StaleGuard` on Bitrue,
   Backpack, BingX, Kraken Spot, KuCoin Spot, Orderly, Polymarket and WOO;
   WEEX balance reads no longer fence.
5. MEXC refused key fails startup instead of polling forever (#1431, merged;
   test helper fix #1440).
6. Private-row isolation (#1459): `engine::coverage::SnapshotCoverage` on
   Bitget (replacing #1356's whole-snapshot freeze), Binance USD-M/COIN-M,
   Hyperliquid, Lighter, Orderly and Paradex. MEXC, Crypto.com, BingX, BloFin,
   Bitrue, KuCoin, OKX, Kraken futures, Backpack and GRVT catalogue-filtered
   rows belong to claude-cbzmw6; WOO/AsterDEX/WEEX/ApeX/Polymarket rows to the
   venues-b lane.
7. Startup discovery retries honour 418/429 and `Retry-After` (next).

Not done, by decision: BreakoutProp authenticated discovery startup. The venue
is disabled in production and preview (`enabled: false`,
`enableInPreview: false`; the legacy DXtrade pool is frozen), so by the
production-impact rule the work has no user impact (overseer, 2026-10-21).

Each fix has a reproducing regression; each PR gets adversarial sub-agent
reviews posted on it.
