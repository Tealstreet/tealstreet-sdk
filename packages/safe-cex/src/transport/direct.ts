import type { TransportProvider } from './types';

const warnedWsRoute = new Set<string>();

/**
 * Every request and socket goes straight to the venue. The default when the host
 * registers nothing, so the SDK reaches no one else's infrastructure on its own.
 */
export const directTransport: TransportProvider = {
  name: 'direct',

  rest: ({ exchange, options, config }) => {
    // An account asking for Tealstreet routing would silently go direct here and
    // fail on the venue's IP allowlist with an error that blames the key.
    if (options.tealstreetProxy || options.tealstreetTunnel) {
      throw new Error(
        `[${exchange}] tealstreetProxy/tealstreetTunnel requested but the direct transport is active. ` +
          'Register a transport with setDefaultTransport() or pass ExchangeOptions.transport.',
      );
    }
    return {
      baseURL: options.overrideProxyUrl ? `${options.overrideProxyUrl}/${config?.baseURL}` : config?.baseURL,
      proxied: false,
    };
  },

  restAdapter: () => undefined,
  proxyPrefix: () => '',
  publicCorsProxy: () => undefined,
  privateProxy: () => undefined,
  serverProxyUrl: () => undefined,
  requireProxy: (options) => options,

  wsRouting: (exchange, options) => {
    // REST refuses Tealstreet routing outright; a ws route request only degrades to
    // direct, so say so once rather than let a venue's own warning blame the user.
    if (options.tealstreetWsRoute && !warnedWsRoute.has(exchange)) {
      warnedWsRoute.add(exchange);
      console.warn(
        `[${exchange}] tealstreetWsRoute requested but the direct transport is active; connecting directly. ` +
          'Register a transport with setDefaultTransport() or pass ExchangeOptions.transport.',
      );
    }
    return {
      restProxyHost: '',
      howEgress: 'sg',
      wsPublic: { tunnelled: false },
      wsPrivate: { tunnelled: false },
      wsBusiness: { tunnelled: false },
    };
  },

  wsConnection: () => undefined,
};
