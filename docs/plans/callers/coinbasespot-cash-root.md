# Coinbase Spot cash balance and US retail readiness

Coinbase Spot publishes a USD root balance for the NX margin panel using only
spot wallet rows. The portfolio breakdown's `account_type` allowlist is
`ACCOUNT_TYPE_FIAT` and `ACCOUNT_TYPE_WALLET` (the endpoint's crypto-wallet name).
Prediction-markets, derivatives, vault, staking, missing and unknown types are
excluded before decoding their amounts. Each excluded type emits a bounded,
once-per-account-type note containing only its type; malformed labels become
UNKNOWN. The note uses the existing row-report suppression, not a new cache.

Root total is the sum of the filtered USD/USDC rows' `total_balance_fiat`, with
native cash quantities as the fallback when absent. Free similarly sums
`available_to_trade_fiat` with a native-quantity fallback. Used is total minus
free; upnl is zero. `portfolio_balances.total_cash_equivalent_balance` is ignored:
it may include prediction or derivatives cash. Non-cash crypto equity is excluded.
CFM and INTX roots retain their own sources.

Currency slices keep actual crypto total and free quantities. Multiple retained
spot wallets for one asset sum their quantities, with used recomputed as total
minus free. This deliberately corrects legacy TS's last-row-wins behavior.
Invalid rows preserve the old asset slice rather than publishing a partial sum;
invalid cash rows or non-finite aggregates refuse the snapshot. The physical
1,000-row limit applies before filtering or aggregation. Malformed amount errors
name their field without echoing values. An empty complete spot snapshot clears
the root and currencies. No synthetic USD currency replaces a real USD holding.

The live read-only probe established why balance never loaded: a DEFAULT
portfolio returned three USD rows of FIAT, PREDICTION_MARKETS_CFM and
DERIVATIVES_CASH types. The old Rust duplicate-asset rejection aborted every
poll while markets, tickers, orders and positions loaded normally. The new
production fixture includes those three USD rows and one 2,600-USDC wallet;
only FIAT USD 100 plus WALLET USDC 2,600 count, yielding root 2,700/free 2,490.
The actual JS EngineHost/WASM/SDK fixture asserts all loaded flags and these
balances with null derivatives sections and no INTX/CFM dependency. A separate
fixture proves remaining genuine duplicate spot wallets sum correctly.

## Legacy comparison

Compared with NX `origin/legacy-master` at
`cb651f89d079914e88a10f24a775f7808241d715`:

- `CoinbaseSpotExchange` also selects the first accessible non-INTX portfolio,
  including DEFAULT, and never needs a derivatives portfolio. Legacy spot
  positions are empty native rows plus zero-size market placeholders; Rust
  publishes an empty position dataset. Both mark positions loaded.
- Legacy `fetchBalance` also starts with a zero root. It reads currency total
  and free from `available_to_trade_crypto`; Rust uses `total_balance_crypto`
  for total and available crypto for free. Neither used the portfolio cash
  summary before this fix. This change fixes that shared limitation rather than
  claiming that legacy had a nonzero aggregate root. Non-spot account types
  are now deliberately excluded.
- NX Margin's Spot summary sums `balance.coins[].notional`, otherwise uses the
  root. Its quote rows use the exact quote currency slice, then quote coin,
  otherwise zero. The separate Spot holdings table values `balance.currencies`
  through a ticker or the USD/USDC/USDT one-to-one fallback. That path can show
  USDC holdings even with a zero root; it does not repair the margin summary.
- The private-module loading hook is byte-identical between those NX refs:
  an authenticated user with active accounts sees the spinner only while
  **no** exchange is marked loaded. Both controllers require the supported
  balance/orders/positions flags plus markets, and optional tickers. This is
  not an all-accounts gate. The live probe confirmed markets, tickers, orders
  and positions loaded but balance stayed false because duplicate assets were
  rejected. The duplicate-wallet fixture reproduces that readiness failure.
- Products polling is a ticker refresh, not a markets reload. After seeding,
  successful responses update tickers without replacing markets or resetting
  readiness (pinned over repeated production timer turns). The public socket
  subscribes heartbeats and requested book/candle/trade channels, not tickers.
  This PR does not change the REST poll cadence.

Coinbase's documented breakdown separates account types and native/fiat amounts
per holding; its embedded account_type enum names the crypto wallet
ACCOUNT_TYPE_WALLET:
https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/portfolios/get-portfolio-breakdown

No new caller setup or ordering prerequisite. Hosted signing, credential
custody, request routing and write behavior are unchanged.
