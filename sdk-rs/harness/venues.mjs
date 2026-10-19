// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac } from 'node:crypto';

const root = '../../packages/safe-cex/src/exchanges';

export const VENUES = {
  weex: {
    rust: 'weex',
    ts: { module: `${root}/weex/weex.exchange.ts`, className: 'WeexExchange', name: 'weex' },
    privateSocket: '/v3/ws/private',
    options: { passphrase: 'replay-passphrase', privateWsHeaders: true },
    tsBooksSubscribed: (exchange, count) => [...exchange.publicWebsocket.topics].filter((t) => t.endsWith('@depth200')).length >= count,
    prepareTs(exchange, WebSocket) {
      for (const socket of [exchange.publicWebsocket, exchange.privateWebsocket]) {
        socket.createWebsocket = (url, protocols, options = {}) => {
          const headers = typeof options.headers === 'function' ? options.headers() : options.headers;
          const ws = new WebSocket(url, protocols, { headers });
          ws.addEventListener('open', socket.trackConnected);
          return ws;
        };
      }
    },
    validateRequest({ method, url, body = '', headers = {} }) {
      const parsed = new URL(url);
      if (parsed.pathname.startsWith('/capi/v3/market/')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}${method}${parsed.pathname}${parsed.search}${body || ''}`).digest('base64');
      if (lookup('ACCESS-KEY') !== 'replay-key' || lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' || lookup('ACCESS-SIGN') !== signature) throw new Error(`invalid WEEX signed request: ${method} ${parsed.pathname}`);
    },
    validateSocket(url, headers = {}) {
      if (!url.includes('/v3/ws/private')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}/v3/ws/private`).digest('base64');
      if (lookup('ACCESS-KEY') !== 'replay-key' || lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' || lookup('ACCESS-SIGN') !== signature) throw new Error('invalid WEEX upgrade authentication');
    },
  },
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >= count,
    privateSocket: '/private/ws',
  },
};

export const venueOf = (name) => {
  const venue = VENUES[name];
  if (!venue) throw new Error(`unknown venue ${name}; known: ${Object.keys(VENUES).join(', ')}`);
  return venue;
};

/** Every axios instance the exchange owns, so all of its REST goes to the tape. */
export const axiosClients = (exchange) =>
  [...new Set(Object.values(exchange))].filter((value) => value?.defaults && value?.interceptors);
