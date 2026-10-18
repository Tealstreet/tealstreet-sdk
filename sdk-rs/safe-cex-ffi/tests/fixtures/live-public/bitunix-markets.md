# Bitunix public market recording

Captured 2026-10-18 from GET https://fapi.bitunix.com/api/v1/futures/market/trading_pairs
without credentials. SHA-256: `e2107ba408fa9745b18209c3ff3fbe7c3c981ebb39326f78d43376593f2e8a72`.

`native-bitunix-market-limit.test.mjs` serves these original bytes through a local
HTTP/CONNECT proxy to the maintained TypeScript exchange and the compiled addon.
It compares all 697 eligible markets, including the source string `"1200"`.
