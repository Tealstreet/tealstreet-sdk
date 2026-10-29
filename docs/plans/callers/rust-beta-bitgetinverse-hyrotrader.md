# Beta: bitgetinverse and HyroTrader

`bitgetinverse` (bitget family) and `hyrotrader` (bybit family) join
`BETA_VENUES` and `beta-venues.mjs`. The beta bundle still builds the same 15
families, so `engine.beta.web.mjs` is unchanged. Both venues already declare
dispatch signing and enabled remote signing in `construction-rules.json`, so
client wasm, exchange-server with keys, and exchange-server with browser
remote signing all resolve through the existing policy. HyroTrader keeps
refusing caller client order IDs and payload signers.

## Obligations

- NX web and exchange-server: a HyroTrader account needs
  `NEXT_PUBLIC_HYROTRADER_PROD_REST_URL` (and `..._SIMULATOR_REST_URL` for
  demo) at build time, exactly as the TS venue did. Without it, construction
  fails with "HyroTrader REST/public/private URLs are not configured".
