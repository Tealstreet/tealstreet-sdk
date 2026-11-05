# Keyless public-data reads via the public CORS proxy; BingX chart 100410 hint

**Change.**
- **S1:** a keyless public-data-only engine (a worker mirror's chart source) on an
  opted-in venue sends its reads to that venue's allowlisted hosts through the
  public CORS proxy (`/api/public_proxy`), not the exchange proxy.
  - Opt-in is explicit: `PUBLIC_PROXY_READ_HOSTS` in
    `transport/tealstreet/publicCorsAllowlist.ts` names each venue and the public
    REST hosts it may send there. A request to any other host (BloFin demo, a
    testnet the proxy lacks) keeps the exchange proxy.
  - `/api/public_proxy` is a host allowlist (the edge Worker's `PUBLIC_HOSTS`,
    derived by NX `infra/tofu/proxy-fleet/public-cors.tf` from
    `terraform/exchange-proxy-haproxy-tokyo.cfg`). `PUBLIC_CORS_HOSTS` is a
    checked-in copy; its test equals the config where it is present (NX), and
    requires every opted-in host to be on it and named by the venue's crate.
  - Opted in: apexomni, backpack, bingx, bitget, bitgetinverse, bitgetusdc,
    blofin (mainnet host only), coinbase, coinbasespot, grvt, mexc, phemex.
  - Not opted in: cryptocom (`api.crypto.com` is not allowlisted; keyless
    candles 403), the OKX family (`openapi.okx.com` is not allowlisted),
    binanceinverse (Binance's own public-read route applies).
  - `DIRECT_PUBLIC_READS` venues (Hyperliquid) keep their direct reads.
  - A keyed engine, or a full account, keeps its route.
  - The signed-packet guard (#3089/#3092) still refuses anything credentialed
    toward the public proxy. Selection is by credential, venue and host, never
    by method, so the credential-selection inventory is unchanged.
- **S2:** a BingX 100410 (or 429) on a chart candle read carries `retryAfterMs`
  only when BingX names the unblock time (Retry-After, or an epoch in the
  message). The bare "请稍后重试" no longer becomes a 5-minute floor, so the
  chart's own backoff applies. Discovery, account, identity and metadata reads
  keep the floor.

**Why.** BingX chart history from every user's page egressed through the
exchange proxy, the 9-IP Tokyo trading fleet. All users shared one 100410
budget there, and chart weight landed on the IPs users trade through. Each
refusal then held that user's charts for 5 minutes. BingX klines refuse our
browser origin (403 with `Origin`), so browser-direct is not possible; the
public proxy already admits the kline paths (deaws2).

**Callers.**
- SDK: `engine.publicDataOnlyRoute.test.ts` now expects the public proxy for a
  keyless BingX engine. A refused-key engine still uses the exchange proxy.
- A venue joins the opt-in only with hosts on `PUBLIC_CORS_HOSTS`; a host
  added to the proxy config needs the same line in that copy.
- NX: the chart's rate-limit ladder (N1) applies once no venue hint arrives.

**Obligation.** None. **Deletion prerequisites.** None added.
