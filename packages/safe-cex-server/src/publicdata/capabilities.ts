import type { ExchangeName } from '../../../safe-cex/src/types';

import type {
  PublicDataCapability,
  PublicdataAllowedExchange,
  PublicdataExchangeHealth,
  PublicdataExchangeName,
  PublicdataSubscribeOptions,
  PublicdataTickerSubscribeMessage,
} from './types';

// Keep this allowlist intentionally smaller than publicdata-rust's ExchangeName
// enum. That enum is protocol vocabulary; these entries are the safe-cex
// exchange mappings we are ready to consume as account tickers.
export const DEFAULT_PUBLICDATA_TICKER_CAPABILITIES: readonly PublicDataCapability[] = [
  {
    exchange: 'bybit',
    publicdataExchange: 'bybit',
    tickerSnapshot: true,
    tickerDeltas: true,
    tps: true,
    usdAggregates: true,
    safeCexMarketShape: false,
    orderbook: false,
    liveOhlcv: false,
    notes: 'First publicdata ticker integration target.',
  },
  {
    exchange: 'binance',
    publicdataExchange: 'binance',
    tickerSnapshot: true,
    tickerDeltas: true,
    tps: true,
    usdAggregates: true,
    safeCexMarketShape: false,
    orderbook: false,
    liveOhlcv: false,
    notes: 'USDT-M futures mapping only; binancespot/binancepm variants need separate verification.',
  },
] as const;

export type FilterPublicdataExchangesOptions = {
  readonly requestedExchanges: readonly ExchangeName[] | null | undefined;
  readonly exchangeHealth: readonly PublicdataExchangeHealth[];
  readonly now: number;
  readonly maxStalenessMs: number;
  readonly capabilities?: readonly PublicDataCapability[];
};

export const filterPublicdataTickerExchanges = ({
  requestedExchanges,
  exchangeHealth,
  now,
  maxStalenessMs,
  capabilities = DEFAULT_PUBLICDATA_TICKER_CAPABILITIES,
}: FilterPublicdataExchangesOptions): PublicdataAllowedExchange[] => {
  if (!requestedExchanges?.length) {
    return [];
  }

  const healthByExchange = new Map<string, PublicdataExchangeHealth>();
  for (const health of exchangeHealth) {
    healthByExchange.set(health.exchange, health);
  }

  const capabilityByExchange = new Map<ExchangeName, PublicDataCapability>();
  for (const capability of capabilities) {
    if (!capability.tickerSnapshot || !capability.tickerDeltas) continue;
    capabilityByExchange.set(capability.exchange, capability);
  }

  const seen = new Set<ExchangeName>();
  const allowed: PublicdataAllowedExchange[] = [];
  for (const exchange of requestedExchanges) {
    if (seen.has(exchange)) continue;
    seen.add(exchange);

    const capability = capabilityByExchange.get(exchange);
    if (!capability) continue;

    const health = healthByExchange.get(capability.publicdataExchange);
    if (!health) continue;
    if (health.status !== 'connected') continue;
    if (health.tickerCount <= 0) continue;
    if (!Number.isFinite(health.lastUpdate) || now - health.lastUpdate > maxStalenessMs) {
      continue;
    }

    allowed.push({
      exchange,
      publicdataExchange: capability.publicdataExchange,
      capability,
      health,
    });
  }

  return allowed;
};

export const toPublicdataExchangeNames = (allowed: readonly PublicdataAllowedExchange[]): PublicdataExchangeName[] => {
  const seen = new Set<PublicdataExchangeName>();
  const names: PublicdataExchangeName[] = [];
  for (const item of allowed) {
    if (seen.has(item.publicdataExchange)) continue;
    seen.add(item.publicdataExchange);
    names.push(item.publicdataExchange);
  }
  return names;
};

export const buildTickerSubscribeMessage = (
  allowed: readonly PublicdataAllowedExchange[],
  options: PublicdataSubscribeOptions = {},
): PublicdataTickerSubscribeMessage | null => {
  const exchanges = toPublicdataExchangeNames(allowed);
  if (exchanges.length === 0) {
    // publicdata-rust treats omitted, null, empty, or invalid filters as
    // "all enabled exchanges". Returning null forces callers to no-op instead.
    return null;
  }

  return {
    op: 'subscribe',
    channel: 'tickers',
    rate: options.rate ?? 1,
    exchanges,
    includeIndex: options.includeIndex ?? false,
    includeTps30s: options.includeTps30s ?? false,
    includeTps60s: options.includeTps60s ?? false,
    includeOiAgg: options.includeOiAgg ?? false,
  };
};
