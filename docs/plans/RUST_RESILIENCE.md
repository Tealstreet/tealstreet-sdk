# Resilience lane: backoff, private rows, startup remainder

Scope: `RUST_QUALITY.md` shared helpers 3 and 5; `RUST_AUDITS.md` cross-venue
"rows and subscriptions" and "per-coin balances and snapshot starvation"; the
private-row, app stop payload, BreakoutProp authenticated startup and MEXC
bootstrap remainder handed over in codex-u1pmrg's public-startup handoff
(`RUST_PRIVATE_ROWS_TICKERS.md`, `RUST_APP_STOP_PAYLOADS.md` on that branch).
Public discovery retries land separately through the public-startup PR.

## PRs, in order

1. `engine::backoff::Backoff`: one formula for the remaining 42 inline
   exponential retry delays and the socket supervisor, constants unchanged
   (identical for every reachable attempt). Unsupervised socket reconnects use
   the supervisor's jitter (75-100% of the delay, seeded from host time) so
   clients dropped together do not reconnect together; REST retries stay exact.
2. Private-row isolation (after the public-startup PR merges): an engine
   snapshot builder where a rejected row keeps the store's copy of its id and
   an unidentified rejection blocks absence-based removal in its scope.
   Venues: Bitget, Phemex, MEXC, KuCoin, Kraken, Orderly, Coinbase,
   BreakoutProp, Hyperliquid, Lighter, Paradex, TxFlow, Bitrue, BloFin.
   WOO, Crypto.com, AsterDEX, BingX, WEEX, ApeX, GRVT, Polymarket and Backpack
   row tolerance belongs to the venues-b lane; OKX to venues-a.
3. App stop payload request tests from tealstreet-next's order constructors;
   Kraken futures and spot market-entry TP/SL rejected with a clear error;
   Lighter trailing never falls through to a plain market order.
4. BreakoutProp authenticated discovery startup; MEXC position-mode and
   plan-list bootstrap recovery without a false invalid-credentials result.
5. Per-coin balances (Crypto.com, Bitrue) and revision-discard snapshot
   starvation (WEEX, Bitrue). Bybit per-coin balances belong to venues-a.

Each fix gets a reproducing regression; gates run for every touched venue plus
Binance; each PR gets adversarial sub-agent reviews posted on it.

## Progress

- PR 1: `feat/rust-backoff-schedule`.
