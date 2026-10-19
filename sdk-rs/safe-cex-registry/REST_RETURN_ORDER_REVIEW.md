# Native REST return construction order

The native account command boundary must retain the value the maintained venue
mapper constructs. A typed SDK result is an explicit projection; its schema order
is not evidence of JavaScript construction order. No production ordering table
or output sort is used.

The source census at master `cbc1c1309a17` found 19 model-returning command methods
across 48 functional registered names in 26 venue crates. TxFlow is a 49th
registered name in a 27th crate, with an inherited empty candle fallback. Fourteen
methods return model vectors; five return singleton or nested model records.
`fetchHistoricalPositions` separately returns ordered `serde_json::Value` records.
Store delivery has its own direct graph projection (#1248).

## Candle hook

`Exchange::fetch_ohlcv_native` supplies the account's canonical `Js` result.
KuCoin Futures/Spot and Bitrue override it with their assembled receiver result.
Their typed `fetch_ohlcv` methods explicitly decode that same result for Rust SDK
callers. The account captures it with `NativeValue::from_js`, preserving authored
keys, native scalars and retained nested data.

KuCoin Spot constructs `timestamp, open, close, high, low, volume`; the shared DTO
uses `timestamp, open, high, low, close, volume`. The registry/real HTTP regression
compares both methods with maintained TS, including key sequences, signed zero,
row order and the actual shared JavaScript JSON stringifier. Futures is the
positive control whose order already matches.

Bitrue's maintained `mapKline` appends the original raw candle as `info`. Its
constructed binding regression delivers a response to the real public socket,
checks the native result and source stringifier output, and observes the cache's
array/raw-object aliases. The typed SDK view remains a projection, and does not
claim to retain that extra property.

## Default proof ledger

Every default family is recorded below. These are **structural construction-order
proofs**, not full venue HTTP executions. The native test executes the actual
`Candle` serializer, checks number bits, and sends its key sequence to the source
checker. That checker discovers each maintained candle literal, refuses spreads
or computed keys, and compares its authored property sequence exactly. Thus the
key sequence is independent of the field values. The fixture does not substitute
field initializers or pretend to execute the complete venue method.

All discovered REST candle constructors have six properties and no `lastSide`;
that property belongs to separate Bybit live-trade callbacks. TxFlow's maintained
base fallback is executed and returns an empty vector. KuCoin and Bitrue are not
covered by this default proof: they use the receiver overrides above.

| Default family | Reviewed source constructors | Source files within its exchange directory |
|---|---:|---|
| ApexOmni | 1 | apexomni.exchange.ts |
| AsterDEX | 1 | asterdex.exchange.ts |
| Backpack | 1 | backpack.exchange.ts |
| Binance, inverse, PM, Spot | 4 | binance.exchange.ts, binanceinverse.exchange.ts, binancepm.exchange.ts, binancespot.exchange.ts |
| BingX | 1 | bingx.exchange.ts |
| Bitget, USDC, inverse | 1 | bitget.exchange.ts (inherited candle method) |
| Bitunix | 1 | bitunix.exchange.ts |
| BloFin | 1 | blofin.exchange.ts |
| BreakoutProp | 1 | breakoutprop.exchange.ts |
| Bybit, demo, inverse, Spot, HyroTrader | 3 | bybit.exchange.ts, bybitinverse.exchange.ts, bybitspot.exchange.ts |
| Coinbase, Spot | 1 | coinbase.exchange.ts |
| Crypto.com | 1 | cryptocom.exchange.ts |
| GRVT | 1 | grvt.exchange.ts |
| Hyperliquid, Spot | 2 | hyperliquid.exchange.ts, hyperliquidspot.exchange.ts |
| Kraken, Spot | 2 | kraken.exchange.ts, krakenspot.exchange.ts |
| Lighter, Robinhood | 1 | lighter.exchange.ts |
| MEXC | 1 | mexc.utils.ts |
| OKX, demo, EU, EU demo | 1 | okx.exchange.ts |
| Orderly's six brands | 1 | orderly.exchange.ts |
| Paradex | 2 | paradex.exchange.ts |
| Phemex | 1 | phemex.exchange.ts |
| Polymarket | 1 | polymarket.exchange.ts |
| WEEX | 2 | weex.utils.ts |
| WOO | 1 | woo.exchange.ts (live legacy body) |
| TxFlow | 0 | inherited base.ts empty fallback, executed |

Reproduce with the registry's `native_host_kucoin_mexc` target, including
`candle_default_projection_is_proven_for_every_other_family`. The maintained
source checker is `tests/rest_candle_projection_source.cjs`; it contains proof
locations, not a production field-order map. Other REST model families remain
separate work, starting with Bybit/OKX/Bitget/Kraken income records. No default
proof for candles is a claim about any other method or model.
