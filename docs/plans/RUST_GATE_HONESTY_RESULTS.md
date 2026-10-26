# Strict replay results

A complete reviewed **107-case** collection at `24802ce5a5` in
[run 39439275804](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/39439275804)
reported **27 strict PASS / 80 strict FAIL**. Its newly observed literal paths
were independently classified and added to the manifest afterward; that run's
verifier correctly rejected them until declared. The table records the reviewed
collection, not universal parity. Woo public passed in this collection after a
checkpoint-visibility failure in the previous run; both observations remain recorded.
Earlier harness defects are corrected:
Coinbase's seven TS and seven Rust measured receipts match their respective
audits, and TxFlow's paced-startup fixture replays without unmatched requests.
Exact merge-head certification status and raw artifacts are recorded on
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
- Collection `39439275804` combined web and Node artifacts are each 12,262,898 bytes
  (ceiling 12,582,912), SHA256
  `e0fbbd617e4f7df444db8da269a433cee6fe5ff7ac089325437cbac953fe29e7`.
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

## Audited native-correct divergences (2026-10-24)

These classifications supplement the historical collection above. Keep the
matching declarations: strict FAIL records a TS/Rust difference, not necessarily
a Rust defect. Do not change correct native economics to imitate TS. Reopen a
classification only when its wire facts, producer behavior or projection change;
it does not certify unrelated paths or historical tape versions.

| Declaration family and audited cases | Classification and verified native behavior |
| --- | --- |
| Binance USD-M: `710a58aa511ed26bfe55bad1fef6b67a2544e5a119178560763261675d8862bf` (11 keys), `8066c3b4de3742be5345977c9f66d270898b1b8ebbf220f1cffc384f433f6ba8` (43) | TS-side missing client IDs, average-price substitution and zero-margin placeholder. Rust retains reported IDs, emits execution `L=2999` rather than cumulative `ap=2999.2`, and carries isolated wallet `iw=301.72`. |
| Binance Spot: `17f14a131f0240857e7a0866432587f8929a9aea084d1f619c65a88bb4309c67` (28 economic/identity/lifecycle keys) | TS-side missing client IDs/TIF and startup order visibility; TS uses limit `p=2800` instead of execution `L=2799` and changes a filled order to canceled. Rust retains reported metadata/open-order rows and preserves Closed after the later cancel. The four separate protocol keys are outside this classification. |
| Binance COIN-M: `a0a35640cbdb5fdd8f43a14188cc4113290683b1e625e131f6da0e9b8b38f94b` (34 native-correct keys, 14 harness keys) | TS-side missing client IDs/TIF/native contract count and filled-to-canceled transition. Rust retains seven native contracts and Closed. The fourteen `flatPositionPnl` paths are a shared harness defect: unavailable native NaN becomes a null-valued row while TS absence produces none. They are not native PnL defects; pruning requires fresh proof from the projection fix (#2015). |
| Binance PM: `aef98183e725ca464ac67b55e40ee27f826d785a705a6dbb728e4717d5b853b1` (26) | TS-side missing client IDs/TIF. Rust preserves the reported fields; no native economic correction is justified by these paths. |
| Hyperliquid perps: `7b13a769157f3523faf2f64109c8594ffaca9e681561e43b59ae7f426d1aeb2e` (6); perps with spot enabled: `7b2e2323e0e367eaf716b9a80dc458cf6d3f2e00643cc021504dd3e8e7590f46` (8) | TS-side initial HIP-3 position/settings omission or default, transient resting-order visibility and missing client ID. Rust publishes wire FOO size 4 at 50, leverage 10 and margin 20; positive resting ACKs justify orders 3001/3005, and the reported cloid survives. Preserve these values rather than copying TS absence/default 20. |
| TxFlow: `0dc0d15f920189c2c708efb537aca6df5f5cb3342decb03df03306587a22976b` (3) | TS-side stale directional leverage/margin at accepted-command boundaries. Executed `updateLeverage=7`, `updateMarginMode(isCross=false)` and post-command readback support Rust's leverage 7 and isolated mode, including common one-way rails. |

Evidence scope: the Binance audit checked all 156 full failures in the five
private reports from [run 40040892521](https://github.com/Tealstreet/tealstreet-sdk/actions/runs/40040892521),
with relevant native/TS/projection sources unchanged through `2371f5439a3`.
The Hyperliquid/TxFlow wire/source review is pinned to `f8163b7b852` and checks all
six-plus-eight Hyperliquid and three current TxFlow paths. Scoped native controls
pass 22 Binance account-variant, 25 Hyperliquid account and six TxFlow settings
tests. These are bounded economic classifications, not fresh whole-matrix parity
or automatic declaration-prune receipts.

The additional 78-key historical USD-M tape and TxFlow hash
`ea153d7daf90578ce6aaaf66ae426a32d4e6dec464d0f0764364afa377415d0f`
were not reauthenticated by this audit; retain them without extending the
native-correct finding to their whole raw outputs. The separate Hyperliquid
Spot alias's 17-record PASS is a different workload from the 62-record
spot-enabled perps case above and cannot justify pruning its eight keys.

The durable source checks are the native account/account-variant tests and
`private-tape.mjs`, `private-tape-binance-variants.mjs`,
`private-tape-hyperliquid.mjs` and `private-tape-txflow.mjs`. The manifest remains
the sole machine-readable exact-key list; this docs-only classification neither
changes those keys nor weakens comparison.
