# Public REST evidence — 2026-10-19

All 49 maintained registry names are accounted for: 45 captured response bundles, 58 qualified non-bootstrap reads across 26 venues. No credentials or orders were used. This is partial public-data coverage, not a whole-venue parity claim.

Most rows pin `7e855d1` / addon `abe32699`, before #1017. Priority-venue follow-ups and corrected Coinbase/Orderly profiles pin #1111 source `bb07e4424f` / addon `6c522f1b`. Each entry in [the evidence](evidence/public-rest-2026-10-19.json) retains its exact source, addon, harness hashes, report hash, bundle and typed output/request-tape paths. Raw public response bytes and complete V8 outputs remain in the local ignored cache.

P=qualified nonempty exact match; D=qualified difference; C=clock/completion timing differs; N=within-side nondeterminism; T=watchdog timeout; I=replay input mismatch; B=HTTP/SDK refusal; U=unqualified bootstrap dependency; NR=not executed after a stopped worker; E=empty; —=unavailable or authenticated. Bootstrap-only successes are excluded from coverage. Candle columns are 1m/1h/1d.

| Venue             | Native source | Markets | Candles  | REST book | Public trades | Tickers |
| ----------------- | ------------- | ------- | -------- | --------- | ------------- | ------- |
| binance           | bb07e4424f    | C       | P/P/P    | —         | P             | C       |
| bybit             | bb07e4424f    | NR      | NR/NR/NR | —         | —             | NR      |
| okx               | bb07e4424f    | C       | P/P/P    | —         | —             | C       |
| hyperliquid       | bb07e4424f    | U       | C/P/P    | —         | —             | U       |
| apexomni          | 7e855d1df9    | P       | C/C/C    | —         | —             | —       |
| asterdex          | 7e855d1df9    | N       | C/C/C    | C         | C             | C       |
| backpack          | 7e855d1df9    | P       | C/C/C    | C         | —             | C       |
| bingx             | 7e855d1df9    | P       | C/C/C    | —         | —             | P       |
| bitget            | 7e855d1df9    | P       | C/C/C    | —         | —             | T       |
| bitrue            | 7e855d1df9    | C       | —/—/—    | —         | —             | —       |
| bitunix           | 7e855d1df9    | C       | C/C/C    | —         | —             | C       |
| blofin            | 7e855d1df9    | C       | P/P/P    | —         | —             | C       |
| breakoutprop      | 7e855d1df9    | —       | —/—/—    | —         | —             | —       |
| coinbase          | bb07e4424f    | C       | —/—/—    | —         | —             | C       |
| cryptocom         | 7e855d1df9    | P       | C/C/C    | C         | —             | C       |
| grvt              | 7e855d1df9    | —       | —/—/—    | —         | —             | —       |
| kraken            | 7e855d1df9    | P       | C/C/C    | —         | —             | P       |
| kucoin            | 7e855d1df9    | P       | P/P/P    | —         | —             | P       |
| lighter           | 7e855d1df9    | C       | C/C/C    | —         | —             | —       |
| mexc              | 7e855d1df9    | NR      | NR/NR/NR | NR        | —             | NR      |
| woofi             | bb07e4424f    | P       | —/—/—    | —         | —             | C       |
| paradex           | 7e855d1df9    | D       | C/C/C    | C         | —             | C       |
| phemex            | 7e855d1df9    | P       | P/P/P    | —         | —             | T       |
| polymarket        | 7e855d1df9    | C       | P/B/B    | D         | —             | P       |
| txflow            | 7e855d1df9    | —       | —/—/—    | —         | —             | —       |
| weex              | 7e855d1df9    | C       | P/P/P    | D         | P             | C       |
| woo               | 7e855d1df9    | P       | C/C/C    | —         | —             | T       |
| binanceinverse    | 7e855d1df9    | P       | P/P/P    | —         | P             | P       |
| binancepm         | 7e855d1df9    | C       | P/P/P    | —         | —             | C       |
| binancespot       | 7e855d1df9    | P       | C/C/C    | —         | —             | T       |
| bitgetinverse     | 7e855d1df9    | C       | —/—/—    | —         | —             | C       |
| bitgetusdc        | 7e855d1df9    | C       | C/C/C    | —         | —             | P       |
| bybitdemo         | 7e855d1df9    | NR      | NR/NR/NR | —         | —             | NR      |
| hyrotrader        | 7e855d1df9    | —       | —/—/—    | —         | —             | —       |
| bybitinverse      | 7e855d1df9    | C       | C/C/C    | —         | —             | C       |
| bybitspot         | 7e855d1df9    | P       | C/C/C    | —         | —             | P       |
| coinbasespot      | bb07e4424f    | C       | —/—/—    | —         | —             | C       |
| hyperliquidspot   | 7e855d1df9    | U       | C/P/P    | —         | —             | U       |
| krakenspot        | 7e855d1df9    | P       | C/C/C    | —         | —             | P       |
| kucoinspot        | 7e855d1df9    | P       | C/C/C    | —         | —             | T       |
| lighter-robinhood | 7e855d1df9    | —       | —/—/—    | —         | —             | —       |
| okxdemo           | 7e855d1df9    | C       | C/C/C    | —         | —             | T       |
| okxeu             | 7e855d1df9    | C       | C/C/C    | —         | —             | T       |
| okxeudemo         | 7e855d1df9    | C       | C/C/C    | —         | —             | T       |
| kodiakfi          | bb07e4424f    | N       | —/—/—    | —         | —             | C       |
| mode              | bb07e4424f    | C       | —/—/—    | —         | —             | C       |
| salsadex          | bb07e4424f    | P       | —/—/—    | —         | —             | C       |
| tealstreet-dex    | bb07e4424f    | P       | —/—/—    | —         | —             | C       |
| what              | bb07e4424f    | C       | —/—/—    | —         | —             | C       |

Qualified dispatch differences were routed to codex-wdtab1: OKX family/Blofin `fetchTickersRaw`, and Polymarket/Weex `fetchOrderBook`, all reject an existing TS method. HyperliquidSpot `start` returns `undefined` in TS and `true` in native. Paradex missing market fees is the known frozen-baseline issue already fixed and live-confirmed on the [#1085](https://github.com/Tealstreet/tealstreet-sdk/pull/1085) candidate.

Qualified populated funding-rate outputs occur on BingX, Kraken, Binance inverse and Bitget USDC. Only Kraken establishes populated open-interest parity here (all 207 rows); matching zero placeholders or NaN do not establish populated OI coverage. No REST orderbook snapshot is qualified PASS in this sweep.

Coinbase brokerage candles/products and Orderly `/v1/kline` require signing and are excluded from the keyless plan. BreakoutProp requires a session; HyroTrader needs an operator-provided origin; TXFlow has no TS public REST implementation. GRVT native open and Lighter-Robinhood TS bootstrap hit the watchdog on their pinned baselines; neither establishes a mapper difference.

Bybit options still exceed 60 seconds on the checked #1111 addon with the exact V2 bundle. Later reads remain NR. A sample at 40 seconds places the hosting thread in synchronous return decoding of 8,110 markets, so the early parked-runtime sample is not evidence of a lost wakeup. Full request tapes, clock traces, receipts and samples remain in `.cache/live-diff/bybit-options-emitter-v2-rest.json` and its referenced artifacts.

Every timestamp-only difference under an advancing coarse REST clock stays C: HTTP parsing/completion is not fenced relative to each advance. Exact timestamps remain in the artifacts. This qualification does not replace the separate per-frame WS drain proof.
