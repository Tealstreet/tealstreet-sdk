import type { ExchangeName, ExchangeOptions } from '../../types';

export const BYBIT_BRAZIL_SITE_ID = 'BRA_BTL' as const;

export type BybitSiteId = typeof BYBIT_BRAZIL_SITE_ID;

const BYBIT_SITE_ID_EXCHANGES = new Set<ExchangeName>(['bybit', 'bybitinverse', 'bybitspot']);

export const isBybitSiteIdExchange = (exchange: ExchangeName): boolean => BYBIT_SITE_ID_EXCHANGES.has(exchange);

export const normalizeBybitSiteId = (exchange: ExchangeName, siteId: unknown): BybitSiteId | undefined => {
  if (!isBybitSiteIdExchange(exchange)) return undefined;
  return siteId === BYBIT_BRAZIL_SITE_ID ? BYBIT_BRAZIL_SITE_ID : undefined;
};

export const getBybitSiteHeaders = (
  options: Pick<ExchangeOptions, 'bybitSiteId'>,
): Record<string, string> => {
  return options.bybitSiteId === BYBIT_BRAZIL_SITE_ID ? { 'x-site-id': BYBIT_BRAZIL_SITE_ID } : {};
};

export const getBybitWebSocketOptions = (
  options: Pick<ExchangeOptions, 'bybitSiteId'>,
): { headers: Record<string, string> } | undefined => {
  const headers = getBybitSiteHeaders(options);
  return Object.keys(headers).length > 0 ? { headers } : undefined;
};
