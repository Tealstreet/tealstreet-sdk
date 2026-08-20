import type { ExchangeName } from '../types';
import type { RestPool, WsStrategy } from './types';

import { PRIVATE_PROXY_ASIA, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from '../utils/transportResolution';
import { routingTable } from './routingTable';

// The single reader of the routing table. Given an exchange and the resolved
// transport options, returns the complete network packet path. This is what the
// golden master snapshots and what (in later epics) every consumer calls instead
// of hardcoding its own host. Pure and side-effect-free.

export interface RoutingOptions {
  tealstreetProxy?: boolean;
  proxyRegion?: string;
  // lighter's WS proxy is gated by this separate flag, NOT tealstreetProxy.
  tealstreetWsRoute?: boolean;
}

// A WS channel's resolved routing. `direct` = the exchange's own endpoint (no
// tunnel). `tunnelled` carries the host. `fallback` is a candidate the channel's
// runtime fallback machine may switch to (bitget); `readonly` likewise (lighter).
export interface ResolvedWs {
  tunnelled: boolean;
  host?: string;
  fallback?: string;
  readonly?: string;
}

export interface ResolvedRouting {
  restProxyHost: string; // '' when REST is direct (no tealstreetProxy)
  howEgress: 'jp' | 'sg';
  wsPublic: ResolvedWs;
  wsPrivate: ResolvedWs;
  wsBusiness: ResolvedWs;
}

const poolHost = (pool: RestPool): string =>
  pool === 'jp' ? PRIVATE_PROXY_JP : pool === 'tiny' ? PRIVATE_PROXY_GRVT_TINY : PRIVATE_PROXY_ASIA;

// Reproduces getPrivateProxy: grvt's 'tiny' pool always wins; otherwise
// proxyRegion 'JA' forces the jp pool (the forceJa branch), else the declared pool.
const resolveRestPool = (declared: RestPool, proxyRegion?: string): RestPool =>
  declared === 'tiny' ? 'tiny' : proxyRegion === 'JA' ? 'jp' : declared;

const resolveWs = (strategy: WsStrategy | undefined, o: RoutingOptions): ResolvedWs => {
  if (!strategy) return { tunnelled: false };
  switch (strategy.kind) {
    case 'direct':
      return { tunnelled: false };
    case 'alwaysProxy':
      return { tunnelled: true, host: strategy.host };
    case 'proxyConditional':
      return o.tealstreetProxy ? { tunnelled: true, host: strategy.host } : { tunnelled: false };
    case 'regionConditional':
      return o.tealstreetProxy
        ? { tunnelled: true, host: o.proxyRegion === 'JA' ? strategy.ja : strategy.other }
        : { tunnelled: false };
    case 'fallbackOnly':
      // Direct primary; the channel's runtime machine may switch to `fallback`.
      return { tunnelled: false, fallback: strategy.fallback };
    case 'wsRouteCandidates':
      return o.tealstreetWsRoute
        ? { tunnelled: true, host: strategy.proxy, ...(strategy.readonly ? { readonly: strategy.readonly } : {}) }
        : { tunnelled: false, ...(strategy.readonly ? { readonly: strategy.readonly } : {}) };
  }
};

export const resolveExchangeRouting = (exchange: ExchangeName, o: RoutingOptions = {}): ResolvedRouting => {
  const spec = routingTable[exchange];
  const howEgress = spec.howEgress === 'regionConditional' ? (o.proxyRegion === 'JA' ? 'jp' : 'sg') : spec.howEgress;
  return {
    restProxyHost: o.tealstreetProxy ? poolHost(resolveRestPool(spec.restPool, o.proxyRegion)) : '',
    howEgress,
    wsPublic: resolveWs(spec.ws?.public, o),
    wsPrivate: resolveWs(spec.ws?.private, o),
    wsBusiness: resolveWs(spec.ws?.business, o),
  };
};
