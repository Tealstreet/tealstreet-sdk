# Binance request counter witness

Exact public HTTP 200 bodies from the 570-frame Binance capture on 2026-10-18:

| File | GET | SHA-256 |
| --- | --- | --- |
| `binance-request-markets.json` | `https://fapi.binance.com/fapi/v1/exchangeInfo` | `5d69aec1ac94824fbdff4b445a581c73ea5d229995c23711cc2bca8eb4a758c2` |
| `binance-request-candles.json` | `https://fapi.binance.com/fapi/v1/klines?endTime=1792334872923&interval=1m&limit=1&startTime=1792334812923&symbol=BTCUSDT` | `13be26c10c14098d96af591b7b712c5908b97979dc2a687e999ae9c94aabb131` |

The source capture is `binance-streams-60s.json`, REST/tape bundle
`df9a504272da029f0c871cb9c44bb2777ff092c34ea966a68d3c126269218d5f.bundle.v8`.
The later `snapshot-eager-full.json` replay retains the observed TS `ok: 5/6`
versus native `ok: 0` differences while all 624 data outputs repeat.

`native-binance-request-stats.test.mjs` isolates that counter boundary: four or
five real candle calls followed by a market refresh serve these unchanged
bodies over replay-only HTTPS. It compares the maintained TS ExchangeWorker
callback with the compiled napi Store callback, including the no-notify rule.
It does not claim to replay the full socket tape or fence arbitrary raw tasks.
