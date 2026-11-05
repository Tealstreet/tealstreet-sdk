# Keyless public-data reads via the public CORS proxy; BingX chart 100410 hint

**Change.**
- **S1:** a keyless public-data-only engine on a venue whose REST is proxied
  sends its reads through the public CORS proxy (`/api/public_proxy`), not the
  exchange proxy. That engine is a worker mirror's chart source; the venues are
  `requiresCors` venues such as BingX.
  - The managed path applies this to reads with `intent: 'public'`; the unmanaged
    path applies it to any request from such an engine.
  - `DIRECT_PUBLIC_READS` venues (Hyperliquid) keep their direct reads.
  - A keyed engine, or a full account, keeps its route.
  - The signed-packet guard (#3089/#3092) still refuses anything credentialed
    toward the public proxy. There is no method-based selection, so the
    credential-selection inventory is unchanged.
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
- NX: the chart's rate-limit ladder (N1) applies once no venue hint arrives.

**Obligation.** None. **Deletion prerequisites.** None added.
