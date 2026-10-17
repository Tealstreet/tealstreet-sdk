# Native ticker coercion census

Each row tracks one public socket source path from the 32-path census. Source
paths are relative to `packages/safe-cex/src/exchanges/`. A source classification
alone is not native proof. A venue becomes proven only after constructed native
socket frames exercise malformed values, null, numeric strings and NaN-producing
inputs alongside good rows, with native number bits checked against maintained TS
or the approved strict-field row policy. Single-row protocols use consecutive
frames; array protocols put the bad row between good siblings in one frame.

The source oracle constructs the maintained exchange and its actual DefaultStore;
it never replaces a handler, normalizer, coercer or Store method. Native tests use
the production venue constructor, socket protocol and DefaultStore. Scripted socket
and HTTP boundaries provide input only. NaN bits are canonicalized for comparison;
infinity and signed zero stay exact. Each numeric input field varies independently,
with distinct finite values in the good rows to detect misrouted fields. Every
case resets the real Store tickers to a finite baseline on both sides, so a
previous NaN or unchanged good-row value cannot hide a dropped frame. JSON
frames carry NaN-producing strings; assertions observe native NaN in the Store. Existing narrower tests do not automatically
satisfy this corpus.

| Venue path | TS source | Numeric policy from source census | Native witness |
| --- | --- | --- | --- |
| ApexOmni | apexomni/apexomni.ws-public.ts | Truthy-gated parseFloat; missing mark falls back to index | Native proven: safe-cex-venue-apexomni/tests/binding.rs::production_ticker_coercion_matches_constructed_typescript; 64 three-row frames |
| AsterDEX | asterdex/asterdex.ws-public.ts | Coercion except copied truthy fundingTime; decision-3 row refusal | Pending complete coercion corpus; row policy #807 |
| Backpack | backpack/backpack.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Binance futures | binance/binance.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Binance PM | binance/binancepm.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Binance Spot | binance/binancespot.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| BingX | bingx/bingx.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Bitget | bitget/bitget.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Bitrue | bitrue/bitrue.ws-public.ts | Unary Number conversion; rose multiplied by 100 | Native proven: safe-cex-venue-bitrue/tests/exchange_binding.rs::production_ticker_coercion_matches_constructed_typescript; 32 cases / 96 actual frames |
| Bitunix | bitunix/bitunix.ws-public.ts | parseFloat; depth fallback with `|| 0` | Native proven: safe-cex-venue-bitunix/tests/native_identity.rs::production_ticker_coercion_matches_constructed_typescript; 80 cases / 240 primary and secondary frames |
| BloFin | blofin/blofin.ws-public.ts | Numeric coercion | Pending; prioritize missing ticker binding corpus |
| Bybit primary/demo | bybit/bybit.ws-public.ts | parseFloat/arithmetic; category-dependent volumes | Pending complete coercion corpus |
| Bybit inverse | bybit/bybitinverse.ws-public.ts | parseFloat/arithmetic; dedicated inverse volumes | Pending complete coercion corpus |
| Bybit Spot | bybit/bybitspot.ws-public.ts | parseFloat/arithmetic | Pending complete coercion corpus |
| Coinbase | coinbase/coinbase.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Crypto.com | cryptocom/cryptocom.ws-public.ts | parseFloat | Pending; prioritize missing ticker binding corpus |
| GRVT | grvt/grvt.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Hyperliquid futures | hyperliquid/hyperliquid.ws-public.ts | parseFloat mids; positive finite Number fast contexts | Native proven: safe-cex-venue-hyperliquid/tests/binding.rs::production_ticker_coercion_matches_constructed_typescript; 24 three-row frames, including raw-DEFLATE fast contexts and midPrice |
| Hyperliquid Spot | hyperliquid/hyperliquidspot.ws-public.ts | parseFloat mids | Native proven: safe-cex-venue-hyperliquid/tests/spot_binding.rs::production_ticker_coercion_matches_constructed_typescript; 8 three-row frames including midPrice |
| Kraken futures | kraken/kraken.ws-public.ts | Numeric coercion, including fundingTime helper | Pending complete coercion corpus |
| Kraken Spot | kraken/krakenspot.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| KuCoin futures | kucoin/kucoin.ws-public.ts | Truthy tickerV2 fallback; nullish snapshot/instrument guards with parseFloat | Native proven: safe-cex-venue-kucoin/tests/exchange_trait.rs::production_ticker_coercion_futures_matches_constructed_typescript; 72 cases / 216 frames across tickerV2, snapshot and instrument |
| KuCoin Spot | kucoin/kucoinspot.ws-public.ts | Finite parseFloat after nullish-to-empty conversion; nonfinite values leave prior fields | Native proven: safe-cex-venue-kucoin/tests/exchange_trait.rs::production_ticker_coercion_spot_matches_constructed_typescript; 256 cases / 768 symbol and ticker-all frames, including every nullish numeric alias |
| Lighter | lighter/lighter.ws-public.ts | Finite number/string selection, book-side fallback and funding normalization | Native proven: safe-cex-venue-lighter/tests/exchange_binding.rs::production_ticker_coercion_matches_constructed_typescript; 1,240 cases / 2,280 ticker, ticker-array and market-stat frames, including numeric aliases and nested bid/ask forms |
| MEXC | mexc/mexc.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| OKX | okx/okx.ws-public.ts | Numeric coercion | Pending complete coercion corpus |
| Orderly | orderly/orderly.ws-public.ts | Copied ticker/BBO/mark values stay strict; decision-3 row refusal; funding parsed | Pending complete coercion corpus; row policy #803 |
| Paradex | paradex/paradex.ws-public.ts | parseFloat/Number finite handling | Pending complete coercion corpus |
| Phemex | phemex/phemex.ws-public.ts | Packed parseFloat/arithmetic; truthy bid/ask guards | Native proven: safe-cex-venue-phemex/tests/exchange_binding.rs::production_ticker_coercion_matches_constructed_typescript; 72 three-row packed frames |
| Polymarket | polymarket/polymarket.ws-public.ts | Finite parseFloat bid/ask/last with token-to-symbol lookup; midpoint mark/index | Native proven: safe-cex-venue-polymarket/tests/exchange_binding.rs::production_ticker_coercion_matches_constructed_typescript; 24 cases / 40 price-change and last-trade frames with production token mappings; numeric mapping scope, not throttle timing |
| WEEX | weex/weex.ws-public.ts | toNumber finite fallback; bounded fractional percentage scaling | Native proven: safe-cex-venue-weex/tests/exchange_binding.rs::production_ticker_coercion_matches_constructed_typescript; 96 cases / 288 frames, object and one-element array payloads |
| WOO | woo/woo.ws-public.ts | Copied ticker/BBO/mark values stay strict; decision-3 row refusal | Pending complete coercion corpus; row policy #789/#814 |
