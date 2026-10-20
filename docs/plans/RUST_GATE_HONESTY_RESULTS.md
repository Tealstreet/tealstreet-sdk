# Strict replay results

A complete reviewed **107-case** collection at `67cf7d9b2b` in
[run 39424799340](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39424799340)
reported **27 strict PASS / 80 strict FAIL**. Its newly observed literal paths
were independently classified and added to the manifest afterward; that run's
verifier correctly rejected them until declared. The table records the reviewed
collection, not universal parity. Earlier harness defects are corrected:
Coinbase's seven TS and seven Rust measured receipts match their respective
audits, and TxFlow's paced-startup fixture replays without unmatched requests.
Exact merge-head certification and raw artifacts are attached to
[PR #1346](https://github.com/Tealstreet/tealstreet-sdk/pull/1346).
Harness integrity failures cannot enter the venue exception list.

The sole machine-readable failure list is
[`known-gate-failures.json`](../../sdk-rs/harness/known-gate-failures.json): exact venue,
tape SHA and literal diagnostic paths, grouped only to avoid repeating explanations.
Known failures remain FAIL. Harness integrity, incomplete own-audit performance work
and missing checkpoint evidence cannot be exempted. An earlier Hyro steady-CPU miss
from [run 39417011625](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39417011625)
is retained; a later favorable run does not erase it.

All 49 exercised venue aliases have a private divergence. Some are old TS defects
corrected by Rust; some are missing native fields/capabilities; others are interim
lifecycle/protocol differences requiring investigation. This table routes work and
does not assign every mismatch to Rust. Exact paths and raw reports carry the detail.

| Venue | Public cases | Private cases | Observed divergence / routing |
| --- | --- | --- | --- |
| `apexomni` | 1 PASS | 1 FAIL | Open/terminal/bracket lifecycle and parent IDs; account PnL and leverage/settings readiness. |
| `asterdex` | 1 FAIL | 2 FAIL | Interim open interest; missing TS client IDs/TIF and terminal-order status (V1 and V3). |
| `backpack` | 1 FAIL | 1 FAIL | Book/ticker timing and rounded percentages; hedge flag, IDs/TIF/settings, entry price, execution count/price/timestamps. |
| `binance` | 1 PASS | 2 FAIL | TS drops client IDs/TIF; execution versus average fill prices; interim zero margin placeholder. Includes the frozen lifecycle case. |
| `binanceinverse` | 1 PASS | 1 FAIL | Order client IDs/TIF, terminal state, and protective trigger references. |
| `binancepm` | 1 PASS | 1 FAIL | Order client IDs/TIF and protective-order state. |
| `binancespot` | 1 PASS | 1 FAIL | Order client IDs/TIF and fill price/lifecycle differences. |
| `bingx` | 1 FAIL | 1 FAIL | Raw precision/ticker/book corrections; client IDs, protective flags and execution count/price. |
| `bitget` | 1 PASS | 1 FAIL | TS drops native/client order IDs on live order views. |
| `bitgetinverse` | not separately gated | 1 FAIL | Order ID/client ID, contract amount/remaining and protective order lifecycle. |
| `bitgetusdc` | 1 PASS | 1 FAIL | TS drops client IDs and live execution price; native getAccount is unsupported and omits the TS account-query request. |
| `bitrue` | 1 PASS | 1 FAIL | Client IDs/TIF and interim order/position state. Landing TS reports GTC where native omits TIF; the newly exposed checkpoint already compares different order IDs. |
| `blofin` | 1 PASS | 1 FAIL | Client IDs, terminal order status and execution count/price. |
| `breakoutprop` | 1 FAIL | 1 FAIL | Interim public books; private ticker prices, balances/PnL, position and order/client-ID state. |
| `bybit` | 1 PASS | 1 FAIL | Client IDs, terminal-order retention and execution sequencing. |
| `bybitdemo` | 1 PASS | 1 FAIL | Interim execution count/arrival differs despite terminal agreement. |
| `bybitinverse` | 1 PASS | 1 FAIL | Coin wallet/PnL conversion, order identity and execution sequencing. |
| `bybitspot` | 1 FAIL | 1 FAIL | Legacy instrument precision/limits and protective order/execution mapping. |
| `coinbase` | 1 FAIL | 6 FAIL | Raw market activation and ticker stats; INTX/CFM/spot wallet and currency state, client IDs and executions (ES256/EdDSA). Terminal retention and stale-open snapshots shift sorted rows; native REST TIF is absent from TS live views. |
| `coinbasespot` | not separately gated | 2 FAIL | Spot wallet/currency state, client IDs, settings and executions (ES256/EdDSA); closed-order retention and REST-derived TIF differ, shifting sorted rows. |
| `cryptocom` | 1 FAIL | 1 FAIL | Interim bid/ask, hedge state, IDs/fills; both replays also report missing required private subscription coverage. |
| `grvt` | 1 PASS | 1 FAIL | Client/optimistic order state and execution identity; both replays report missing required GRVT private streams. |
| `hyperliquid` | 1 PASS | 1 FAIL | Interim position/settings/order state; TS execution ID 9001 survives while the Rust live-fill frame has none. |
| `hyperliquidspot` | 1 PASS | 1 FAIL | TS native trade identity survives; the Rust live-fill frame lacks the execution ID. |
| `hyrotrader` | 1 PASS | 1 FAIL | Initial balance and client-ID/TIF state; an earlier seven-pair steady-CPU miss remains declared even after a later pass. |
| `kodiakfi` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |
| `kraken` | 1 FAIL | 1 FAIL | Raw contract precision, ticker/funding and deep-book handling; order/position/settings and duplicate execution state. |
| `krakenspot` | 1 PASS | 1 FAIL | Interim canceled/closed amendment status differs. Harness duplicate subscriptions were corrected, not exempted. |
| `kucoin` | 1 FAIL | 1 FAIL | Initial book readiness; open/stop order lifecycle, client/trigger IDs, racing fills, position and settings state. |
| `kucoinspot` | 1 PASS | 1 FAIL | Live order client-ID retention. |
| `lighter` | 1 FAIL | 2 FAIL | Initial tickers/books/account seed; order/fill count and execution price differences (mainnet/testnet fixtures). |
| `lighter-robinhood` | 1 FAIL | 2 FAIL | Initial ticker/book/account seed and execution/order differences (mainnet/testnet fixtures). |
| `mexc` | 1 FAIL | 1 FAIL | Initial books; order lifecycle and TS live execution price/ID/fee versus missing native metadata. |
| `mode` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |
| `okx` | 2 PASS | 1 FAIL | Client IDs and interim available/currency wallet balances; both linear and inverse public cases pass. Native retains an extra 83.6 reservation after order closure at checkpoints 1–3. |
| `okxdemo` | not separately gated | 1 FAIL | Client IDs and interim available/currency wallet balances. |
| `okxeu` | 2 PASS | 1 FAIL | Client IDs and interim available/currency wallet balances; public regular/X-Perp cases pass after clock correction. |
| `okxeudemo` | not separately gated | 1 FAIL | Client IDs and interim available/currency wallet balances. |
| `orderly` | 1 FAIL | 2 FAIL | Raw percentage, bracket/root terminal lifecycle, leverage/settings and duplicate execution state; query-oracle integrity was corrected. |
| `paradex` | 1 PASS | 1 FAIL | Client IDs and optimistic/open/terminal order lifecycle through cancel/amend/reconciliation. TS retains canceled caller/batch rows that native reconciliation removes. |
| `phemex` | 1 FAIL | 1 FAIL | Raw rounded percentages; settings readiness, order cumulative fills/remaining/IDs and duplicate executions. |
| `polymarket` | 1 PASS | 1 FAIL | Interim available PnL and open-order lifecycle/retention. |
| `salsadex` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |
| `tealstreet-dex` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |
| `txflow` | 1 PASS | 1 FAIL | Per-side leverage and margin settings differ at intermediate command boundaries. |
| `weex` | 1 PASS | 1 FAIL | Order lifecycle and TS execution IDs/fees/timestamps absent from the native live-fill frame. |
| `what` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |
| `woo` | 1 PASS | 1 FAIL | Initial books; orders/positions/settings/executions, plus incomplete auth/heartbeat/private subscriptions in both fixture replays. |
| `woofi` | not separately gated | 2 FAIL | Orderly-family bracket/root terminal lifecycle, leverage/settings and duplicate executions (mainnet/testnet fixtures). |

## Measurement and artifact boundaries

- Public gates use seven fresh interleaved process pairs and require at least 10%
  improvement for main/steady/process CPU and retained memory. Every performance
  output must reproduce its own audited state and exact transport receipt.
- CI performance uses standalone venue artifacts. The combined shipping artifact
  is separately size-budgeted; standalone CPU results do not prove combined-load
  or browser performance. Private metrics are informational only.
- The corrected-head combined web and Node artifacts are each 12,174,800 bytes
  (ceiling 12,582,912), SHA256
  `637e35fe2648c20d8e3783ab430f68b0691eee56406de80525b2e10d2b750195`.
  The standalone ceiling remains 1,258,291 bytes.
- Fixer output reports every overwritten leaf count; raw checkpoints precede
  fixers. Per-venue permissions do not authorize rewrites to Rust output.
- Several TS venues provide real execution IDs/fees/timestamps now preserved by
  capture. Rust live-fill frames lack some of those fields. Null/null equality
  remains absence of evidence; it cannot certify unsupported metadata.
- Private fixtures and the Hyro/Breakout/USDC public fixtures are synthetic.
  The frozen Binance lifecycle fixture is sanitized, scripted and Rust-driven.
  No live credentialed private, frozen-master TS, peak-memory or browser cutover
  proof is claimed by these results.
