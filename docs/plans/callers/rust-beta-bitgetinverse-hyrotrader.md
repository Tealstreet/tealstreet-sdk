# Beta: bitgetinverse and HyroTrader

`bitgetinverse` (bitget family) and `hyrotrader` (bybit family) join
`BETA_VENUES` and `beta-venues.mjs`. The beta bundle still builds the same 15
families, so `engine.beta.web.mjs` is unchanged. Both venues already declare
dispatch signing and enabled remote signing in `construction-rules.json`, so
client wasm, exchange-server with keys, and exchange-server with browser
remote signing all resolve through the existing policy. HyroTrader keeps
refusing caller client order IDs and payload signers.

## Obligations

- Ordinary browser accounts default to `https://sim-bybit.hyro.trade` for
  production/challenge REST and derive the matching public/private WSS paths.
  `NEXT_PUBLIC_HYROTRADER_PROD_*` variables override those build-time defaults.
  Explicit demo accounts require the separate `NEXT_PUBLIC_HYROTRADER_SIMULATOR_*`
  configuration; they never borrow production endpoints. Native registry callers
  must supply the selected production/demo endpoint map or explicit endpoints.
  An incomplete selected map fails with typed `invalidInput` / `notSent` before I/O;
  neither mode falls back to Bybit or the other mode's endpoint map.
- Preview beta admission follows the existing metadata. Production availability
  and mobile remain disabled; this integration does not change those gates or
  certify hosted execution. Credentialed simulator acceptance remains a separate
  approval and must precede any production-availability decision.
