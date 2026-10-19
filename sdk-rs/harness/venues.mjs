// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac } from 'node:crypto';

const root = '../../packages/safe-cex/src/exchanges';

const okx = (name) => ({
  rust: name,
  ts: {
    module: `${root}/okx/${name.startsWith('okxeu') ? 'okxeu' : 'okx'}.exchange.ts`,
    className: name.startsWith('okxeu') ? 'OKXEUExchange' : 'OKXExchange',
    name,
  },
  options: { testnet: name.endsWith('demo') },
  configureTs: (exchange) => {
    // The Rust lane covers linear stablecoin swaps, including on the EEA class.
    exchange.getInstrumentTypes = () => ['SWAP'];
    exchange.isSupportedMarketInstrument = (m) =>
      m.instType === 'SWAP' && m.ctType === 'linear' && ['USDT', 'USDC'].includes(m.settleCcy);
  },
  tsBooksSubscribed: (exchange, count) =>
    Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('books.')).length >= count,
  isPrivateRecord: (record) => {
    if (record.type !== 'ws') return false;
    const path = new URL(record.url).pathname;
    if (path.endsWith('/private')) return true;
    if (!path.endsWith('/business')) return false;
    try {
      const message = JSON.parse(record.text);
      return message.event === 'login' || ['orders', 'orders-algo', 'algo-advance'].includes(message.arg?.channel);
    } catch {
      return false;
    }
  },
  matchRequestBody: true,
  // Caller IDs remain exact, even when the caller chooses a broker-prefixed ID.
  generatedBodyId: (key, value) =>
    ['clOrdId', 'algoClOrdId'].includes(key) && typeof value === 'string' && value.startsWith('e847386590ce4dBC'),
  respondToPing: true,
  validateLogin: true,
  compareFillPrices: true,
});

export const VENUES = {
  weex: {
    rust: 'weex',
    ts: { module: `${root}/weex/weex.exchange.ts`, className: 'WeexExchange', name: 'weex' },
    privateSocket: '/v3/ws/private',
    matchRequestBody: true,
    volatileBodyFields: true,
    // WEEX's income history is a read despite using POST.
    isReadRequest: (method, url) => method === 'POST' && new URL(url).pathname === '/capi/v3/account/income',
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
    volatileBodyFields: true,
  },
  bybit: {
    rust: 'bybit',
    ts: { module: `${root}/bybit/bybit.exchange.ts`, className: 'BybitExchange', name: 'bybit' },
    /** Book topics get a message handler once each subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderbook.')).length >= count,
    privateSocket: '/v5/private',
    matchRequestBody: true,
    volatileBodyFields: true,
  },
  ...Object.fromEntries(['okx', 'okxeu', 'okxdemo', 'okxeudemo'].map((name) => [name, okx(name)])),
};

export const venueOf = (name) => {
  const venue = VENUES[name];
  if (!venue) throw new Error(`unknown venue ${name}; known: ${Object.keys(VENUES).join(', ')}`);
  return venue;
};

/** Every axios instance the exchange owns, so all of its REST goes to the tape. */
export const axiosClients = (exchange) =>
  [...new Set(Object.values(exchange))].filter((value) => value?.defaults && value?.interceptors);
