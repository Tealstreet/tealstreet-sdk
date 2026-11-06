# Bybit keyless chart reads via the public CORS proxy

**Change.**
- `bybit`, `bybitspot` and `bybitinverse` opt into `PUBLIC_PROXY_READ_HOSTS`
  with `api.bybit.com`, the one REST host all three read (`sdk-rs/venues/bybit/src/paths.rs`).
  Demo (`api-demo.bybit.com`) and testnet stay on the exchange proxy.
- A keyless public-data engine qualifies when its REST leaves through the
  Tealstreet proxy **or** the tunnel (`usesTunnel`). Bybit keeps its tunnel under
  `publicDataOnly`, so a tunnel account's chart source read klines over the
  tunnel, from the trading fleet's IPs.
- The public-proxy transport never takes the tunnel, and the signed-packet guard
  still refuses a credentialed request before any I/O.

**Why.** Bybit "Too many visits" (10006) on the chart candle path reached 10
users on one release: every proxied or tunnelled Bybit account's chart history
shared the fleet's public budget.

**Callers.**
- Any other opted-in venue on a tunnel-only account (Tealstreet proxy off,
  tunnel on) now also reads its chart history through the public proxy.

**Obligation.** None. **Deletion prerequisites.** None added.
