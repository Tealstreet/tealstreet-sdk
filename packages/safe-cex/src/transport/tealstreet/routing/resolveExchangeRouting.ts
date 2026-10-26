import type { ExchangeName } from '../../../types';
import type { ResolvedRouting, ResolvedWs } from '../../types';
import type { RestPool, WsStrategy } from './types';

import { buildRelayUrl } from '@tealstreet/http-over-websocket/src/tunnelEndpoints';
import { PRIVATE_PROXY_ASIA, PRIVATE_PROXY_GRVT_TINY, PRIVATE_PROXY_JP } from '../transportResolution';
import { routingTable } from './routingTable';

// The single reader of the routing table. Given an exchange and the resolved
// transport options, returns the complete network packet path. This is what the
// golden master snapshots and what (in later epics) every consumer calls instead
// of hardcoding its own host. Pure and side-effect-free.

export interface RoutingOptions {
  tealstreetProxy?: boolean;
  proxyRegion?: string;
  bybitSiteId?: 'BRA_BTL';
  // lighter's WS proxy is gated by this separate flag, NOT tealstreetProxy.
  tealstreetWsRoute?: boolean;
}

const poolHost = (pool: RestPool): string =>
  pool === 'jp' ? PRIVATE_PROXY_JP : pool === 'tiny' ? PRIVATE_PROXY_GRVT_TINY : PRIVATE_PROXY_ASIA;

// Reproduces getPrivateProxy: grvt's 'tiny' pool always wins; otherwise
// proxyRegion 'JA' forces the jp pool (the forceJa branch), else the declared pool.
const resolveRestPool = (declared: RestPool, proxyRegion?: string): RestPool =>
  declared === 'tiny' ? 'tiny' : proxyRegion === 'JA' ? 'jp' : declared;

const resolveWs = (strategy: WsStrategy | undefined, o: RoutingOptions, egress: 'jp' | 'sg'): ResolvedWs => {
  if (!strategy) return { tunnelled: false };
  const resolved = resolveWsKind(strategy, o);
  // Declared fallback candidates ride along only on a DIRECT resolution (see WsStrategy),
  // except a relay the strategy explicitly offers to tunnelled sockets too.
  if (resolved.tunnelled) {
    // Declared direct URLs bypass this channel's route whichever way it resolves.
    const tunnelled = strategy.directUrls ? { ...resolved, directUrls: strategy.directUrls } : resolved;
    return strategy.relayWhenTunnelled && strategy.relayUpstream
      ? { ...tunnelled, relay: buildRelayUrl(egress, strategy.relayUpstream) }
      : tunnelled;
  }
  const out = { ...resolved };
  if (strategy.initialSilentConnects !== undefined) out.initialSilentConnects = strategy.initialSilentConnects;
  if (strategy.ignoredMessages) out.ignoredMessages = strategy.ignoredMessages;
  if (strategy.directUrls) out.directUrls = strategy.directUrls;
  if (strategy.fallback && !out.fallback) out.fallback = strategy.fallback;
  if (strategy.relayUpstream) out.relay = buildRelayUrl(egress, strategy.relayUpstream);
  return out;
};

const resolveWsKind = (strategy: WsStrategy, o: RoutingOptions): ResolvedWs => {
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
    case 'bybitBrazilConditional':
      return o.bybitSiteId === 'BRA_BTL' ? { tunnelled: true, host: strategy.host } : { tunnelled: false };
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
    wsPublic: resolveWs(spec.ws?.public, o, howEgress),
    wsPrivate: resolveWs(spec.ws?.private, o, howEgress),
    wsBusiness: resolveWs(spec.ws?.business, o, howEgress),
    ...(spec.ws?.privateUta ? { wsPrivateUta: resolveWs(spec.ws.privateUta, o, howEgress) } : {}),
  };
};
