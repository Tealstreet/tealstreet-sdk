# Coinbase Spot cash balance and US retail readiness

Coinbase Spot now publishes a USD root balance for the NX margin panel. The
total comes from `breakdown.portfolio_balances.total_cash_equivalent_balance`
when its currency is USD. Free is the sum of USD/USDC
`available_to_trade_fiat`, falling back to each row's available crypto amount
when that optional field is absent. Used is total minus free; upnl is zero.
Without a cash-equivalent summary, total is the USD/USDC row fiat sum, falling
back to native cash quantities. Non-cash crypto and futures equity do not enter
this root. Negative, inconsistent or non-USD summaries retain the last snapshot.

Currency slices retain their actual crypto total, free and used quantities.
Multiple wallet/vault rows for the same asset are summed, with used recomputed
as total minus free. This fixes the live DEFAULT-portfolio readiness failure:
the old Rust path rejected duplicate assets and never loaded balance. Legacy
TS overwrote earlier rows; summation deliberately corrects that behavior too.
The 1,000 physical-row limit stays in place. Invalid rows retain the previous
asset slice rather than publishing a partial sum, and non-finite sums refuse
the snapshot. Malformed amount errors name their field without echoing values.
No synthetic USD currency replaces an actual USD holding, and no USD/USDC
order-sizing alias is introduced. CFM and INTX roots retain their own sources.
An empty complete Spot snapshot clears the root and currencies.

The fixture models a US retail account with one DEFAULT portfolio, duplicate
USDC wallet/vault rows, null or absent derivatives sections and no INTX
or CFM account. Real engine startup sets empty positions locally, completes all
five working-order status reads, and reports markets/tickers/balance/orders/
positions loaded. The same startup and 2,600-USDC balance pass through the actual
JS EngineHost, WASM frame decoder and SDK store. No live account evidence is
claimed by those fixture tests.

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
  claiming that legacy had a nonzero aggregate root.
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

Coinbase's documented breakdown separates cash-equivalent portfolio totals and
native/fiat amounts per holding:
https://docs.cdp.coinbase.com/api-reference/advanced-trade-api/rest-api/portfolios/get-portfolio-breakdown

No new caller setup or ordering prerequisite. Hosted signing, credential
custody, request routing and write behavior are unchanged.
