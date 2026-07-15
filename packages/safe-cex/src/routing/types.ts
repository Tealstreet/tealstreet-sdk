import type { ExchangeName } from '../types';

// Single source of truth for exchange network routing. This module declares, per
// exchange, the complete packet path that was previously scattered across
// getPrivateProxy's URL-regex, the HOW adapter's TOKYO_EGRESS list, and ~18
// per-exchange ws-public/ws-private/ws-business files that hardcoded
// wss://fsjnpy143pbz.* hosts. Consumers resolve their host from here instead of
// hardcoding it; `resolveExchangeRouting` is the one reader.
//
// Scope note: testnet routing is intentionally NOT modelled (testnet is dead).
// Genuinely runtime-stateful WS fallbacks (lighter, bitget) keep their state
// machines in their classes — this table only owns the candidate HOST DATA those
// machines choose between, which is the part that was hardcoded.

// Which whitelisted private CORS proxy pool an exchange's REST traffic uses when
// tealstreetProxy is on. Reproduces getPrivateProxy's current URL-regex outcome
// (binance|okx|bitget -> jp, grvt -> tiny, everything else -> asia) as explicit
// per-exchange data rather than a substring match.
export type RestPool = 'jp' | 'asia' | 'tiny';

// HOW (HTTP-over-WebSocket) tradetunnel egress region for REST-over-WS. 'jp'/'sg'
// reproduce the adapter's TOKYO_EGRESS_PATTERN outcome (baseURL-keyed, per-exchange).
// 'regionConditional' is for exchanges that install a region-pinned adapter from
// the account's proxyRegion (bitmex always, bybit when tunnelling) -> the resolver
// returns 'jp' when proxyRegion === 'JA', else 'sg' (their bare-adapter fallback).
export type HowEgress = 'jp' | 'sg' | 'regionConditional';

// A concrete tunnel host — the wss URL currently hardcoded in the exchange's ws class.
export type WsHost = string;

// How a WS channel chooses its host. The data lives in the spec; `resolveExchangeRouting`
// interprets these against the resolved transport options. Each kind names the CURRENT
// observed behavior faithfully so the golden master can prove the cutover is a no-op.
export type WsStrategy =
  // Always the exchange's own WS — never tunnelled (the default strategy).
  | { kind: 'direct' }
  // Always the tunnel host regardless of options (grvt).
  | { kind: 'alwaysProxy'; host: WsHost }
  // tealstreetProxy ? host : direct (okx private/business, blofin, coinbase, apexomni).
  | { kind: 'proxyConditional'; host: WsHost }
  // tealstreetProxy ? (proxyRegion === 'JA' ? ja : other) : direct (bitmex).
  | { kind: 'regionConditional'; ja: WsHost; other: WsHost }
  // Direct primary; the class falls back to `fallback` after repeated reconnect
  // failures (bitget). The fallback HOST is owned here; the trigger state is not.
  | { kind: 'fallbackOnly'; fallback: WsHost }
  // Gated by the separate `tealstreetWsRoute` flag (NOT tealstreetProxy). The class
  // chooses between `proxy`, direct, and an optional `readonly` direct variant at
  // runtime (lighter). The candidate HOSTS are owned here; the fallback state is not.
  | { kind: 'wsRouteCandidates'; proxy: WsHost; readonly?: WsHost };

export interface ExchangeWsRouting {
  public?: WsStrategy;
  private?: WsStrategy;
  business?: WsStrategy;
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
