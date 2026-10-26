import type { ExchangeName } from '../../../types';

// Single source of truth for exchange network routing. This module declares, per
// exchange, the complete packet path that was previously scattered across
// getPrivateProxy's URL-regex, the HOW adapter's TOKYO_EGRESS list, and ~18
// per-exchange ws-public/ws-private/ws-business files that hardcoded
// wss://fsjnpy143pbz.* hosts. Consumers resolve their host from here instead of
// hardcoding it; `resolveExchangeRouting` is the one reader.
//
// Scope note: testnet routing is intentionally NOT modelled (testnet is dead).
// Genuinely runtime-stateful WS fallbacks (lighter, bitget) keep their state
// machines in their classes. This table owns their candidate hosts and the small
// recovery policy needed by the engine's generic host state machine.

// Which whitelisted private CORS proxy pool an exchange's REST traffic uses when
// tealstreetProxy is on. Reproduces getPrivateProxy's current URL-regex outcome
// (binance|okx|bitget -> jp, grvt -> tiny, everything else -> asia) as explicit
// per-exchange data rather than a substring match.
export type RestPool = 'jp' | 'asia' | 'tiny';

// HOW (HTTP-over-WebSocket) tradetunnel egress region for REST-over-WS. 'jp'/'sg'
// reproduce the adapter's TOKYO_EGRESS_PATTERN outcome (baseURL-keyed, per-exchange).
// 'regionConditional' is for exchanges that install a region-pinned adapter from
// the account's proxyRegion (bybit when tunnelling) -> the resolver
// returns 'jp' when proxyRegion === 'JA', else 'sg' (their bare-adapter fallback).
export type HowEgress = 'jp' | 'sg' | 'regionConditional';

// A concrete tunnel host — the wss URL currently hardcoded in the exchange's ws class.
export type WsHost = string;

// How a WS channel chooses its host. The data lives in the spec; `resolveExchangeRouting`
// interprets these against the resolved transport options. Each kind names the CURRENT
// observed behavior faithfully so the golden master can prove the cutover is a no-op.
export type WsStrategy =
  // Always the exchange's own WS — never tunnelled (the default strategy).
  (
    | { kind: 'direct' }
    // Always the tunnel host regardless of options (grvt).
    | { kind: 'alwaysProxy'; host: WsHost }
    // tealstreetProxy ? host : direct (okx private/business, blofin, coinbase, apexomni).
    | { kind: 'proxyConditional'; host: WsHost }
    // tealstreetProxy ? (proxyRegion === 'JA' ? ja : other) : direct (no current exchange; was bitmex).
    | { kind: 'regionConditional'; ja: WsHost; other: WsHost }
    // bybitSiteId === 'BRA_BTL' ? host : direct. Browser WS cannot send the site
    // header, so the dedicated proxy path injects it upstream.
    | { kind: 'bybitBrazilConditional'; host: WsHost }
    // Direct primary; the class falls back to `fallback` after repeated reconnect
    // failures (bitget). The fallback HOST is owned here; the trigger state is not.
    | { kind: 'fallbackOnly'; fallback: WsHost }
    // Gated by the separate `tealstreetWsRoute` flag (NOT tealstreetProxy). The class
    // chooses between `proxy`, direct, and an optional `readonly` direct variant at
    // runtime (lighter). The candidate HOSTS are owned here; the fallback state is not.
    | { kind: 'wsRouteCandidates'; proxy: WsHost; readonly?: WsHost }
  ) & {
    // Optional Tokyo passthrough host (athena Caddy, Cloudflare-fronted) that the
    // class's WsDirectFallback dials after consecutive SILENT direct connects
    // (bybit, okx). The resolver attaches it only when the channel resolved DIRECT:
    // a tunnelled route (Bybit Brazil's header-injecting path, OKX's tealstreetProxy
    // path) must not degrade to a plain passthrough that drops what the tunnel adds.
    // For `fallbackOnly` this is the same field the kind already requires.
    fallback?: WsHost;
    // Optional PRIVATE venue socket to reach THROUGH the tradetunnel's rpc.relay
    // (bybit/okx private+business). The resolver turns it into a relay url on the
    // egress region's tunnel host, again only for a DIRECT resolution, and the
    // class offers it ahead of `fallback` when the host app has enabled the relay
    // (`transport/tealstreet/wsRelay`). Never a public stream: the tunnel refuses those.
    relayUpstream?: WsHost;
    // Also attach the relay to a TUNNELLED resolution (bitget): a venue that checks
    // the key's bound IP on login needs the egress users bind for REST, which the
    // proxy host may not be.
    relayWhenTunnelled?: boolean;
    initialSilentConnects?: number;
    ignoredMessages?: string[];
    directUrls?: string[];
  };

export interface ExchangeWsRouting {
  public?: WsStrategy;
  private?: WsStrategy;
  business?: WsStrategy;
  // The private socket of a UTA account, on a different venue path (bitget v3).
  privateUta?: WsStrategy;
}

// The full declarative routing spec for one exchange. `ws` channels default to
// 'direct' when omitted.
export interface ExchangeRoutingSpec {
  restPool: RestPool;
  howEgress: HowEgress;
  ws?: ExchangeWsRouting;
}

// Record over every exchange forces exhaustiveness at compile time — a new
// exchange cannot be added without declaring its routing.
export type RoutingTable = Record<ExchangeName, ExchangeRoutingSpec>;
