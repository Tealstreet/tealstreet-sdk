import type { ResolvedRouting, TransportProvider } from './types';
import type { ExchangeName, ExchangeOptions } from '../types';

import { directTransport } from './direct';

let defaultTransport: TransportProvider | undefined;

/** Registers the transport every exchange in this runtime uses unless its options override it. */
export const setDefaultTransport = (provider: TransportProvider): void => {
  defaultTransport = provider;
};

// Resolved lazily so module load order cannot leave the default unset.
export const getTransport = (options: Pick<ExchangeOptions, 'transport'>): TransportProvider =>
  options.transport ?? defaultTransport ?? directTransport;

/** `url` through the transport's public CORS proxy, or `url` itself when it has none. */
export const withPublicCorsProxy = (url: string, options: Pick<ExchangeOptions, 'transport'> = {}): string => {
  const proxy = getTransport(options).publicCorsProxy();
  return proxy ? `${proxy}/${url}` : url;
};

export const resolveTransportRouting = (
  exchange: ExchangeName,
  options: Partial<ExchangeOptions>,
): ResolvedRouting => getTransport(options).wsRouting(exchange, options);
