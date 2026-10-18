# Native public listener query review

Reviewed against mirror master `2ebd94220f` and the live TypeScript public socket
bodies. The constructed Futures, PM and Spot regression first failed on
`/fapi/v1/depth?params%5Bsymbol%5D=BTCUSDT&params%5Blimit%5D=1000`.
TypeScript calls `publicXhr.get(ENDPOINTS.ORDERBOOK, { params: { symbol, limit: 1000 } })`.
The native request helper already installs its raw GET argument as config.params.
Removing the extra wrapper in `NativeListenerOperations::get_snapshot` produces
exactly `symbol=BTCUSDT&limit=1000`; the native witness checks that entire query
and `/fapi/v1/depth` for Futures/PM, `/api/v3/depth` for Spot.

## Caller sweep

Searches covered every tracked `safe-cex-venue-*/src/**/*.rs` source:

```sh
rg -n 'run_request\(' sdk-rs/safe-cex-venue-*/src --glob '*.rs'
rg -n '"params"|\["params"\]' sdk-rs/safe-cex-venue-*/src --glob '*.rs'
rg -n 'fn (request|get_snapshot|snapshot|get|send_request|execute_request)|params:' sdk-rs/safe-cex-venue-*/src --glob '*.rs'
```

`run_request` exists only in Binance. All three call sites were reviewed:

| Caller | Argument contract | Result |
| --- | --- | --- |
| recursive primary/secondary routing | forwards the same raw args | unchanged |
| `NativeImports::request` | forwards `ExchangeState` raw GET fields / POST body | unchanged |
| `NativeListenerOperations::get_snapshot` | incorrectly constructed a second params envelope | fixed; three real wire assertions |

The sibling query boundaries reviewed below do not add an identical second
wrapper. Config-based callers construct one params envelope; their adapters
consume config.params rather than wrapping the config itself. Raw-field callers
pass the field record directly. Websocket and wallet/JSON-RPC params are separate
protocol envelopes, not HTTP query arguments.

| Boundary | Review result |
| --- | --- |
| Backpack `native_http::{Client,RoutedClient}::request`, including public snapshot | separate raw params/data arguments; snapshot passes symbol directly |
| Bitunix `native_http::Client::{get,request}` plus history/orderbook/OHLCV config builders | `get` forwards config unchanged; `request` extracts config.params |
| Coinbase `binding::rt::Host::request` and reads/orders/lifecycle callers | wraps raw payload once; GET callers pass flat records or no payload |
| Aster `binding::members::request`, `AccountFetch::request`, `OrderExecution::request` | raw query record becomes config.params once; typed adapter has a separate params field |
| Bitrue leverage/collection configs and binding callbacks | config forwarded unchanged to real xhr.get |
| Phemex `binding::rt::Host::http` / language read and write bridges | forwards the existing config unchanged to JS client |
| Kraken binding host read + member query bridges | raw fill fields wrapped once; other calls pass one config; socket params are RPC |
| Lighter SDK account/order/market request adapters and account config bridge | config.params is unwrapped into the transport request; typed AccountGet wraps its raw params once |
| Binance inverse reads/orders/snapshot via actual JS client | passes one Axios config directly; no native run_request adapter |
| Apex, BingX, Bitget, Bybit, KuCoin, MEXC, WOO, WEEX, Breakout, Orderly, Paradex, Polymarket HTTP bindings | existing config readers/forwarders or explicit RestRequest.params fields; no second params envelope found |
| GRVT, Hyperliquid, Crypto.com | explicit request fields / source body or RPC envelopes; no matching GET args-to-params wrapper found |

This is a source caller/argument-shape sweep, not a runtime equivalence claim for
other venues. The only matching production defect found is the shared Binance
Futures/PM/Spot public-depth adapter, fixed here with its actual native wire test.
