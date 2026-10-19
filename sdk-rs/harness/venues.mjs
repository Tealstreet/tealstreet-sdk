// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
const root = '../../packages/safe-cex/src/exchanges';

export const VENUES = {
  blofin: {
    rust: 'blofin',
    ts: { module: `${root}/blofin/blofin.exchange.ts`, className: 'BlofinExchange', name: 'blofin' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('books.')).length >= count,
    privateSocket: '/ws/private',
    compareFillPrices: true,
    options: { passphrase: 'replay-passphrase' },
    cacheBustPaths: ['/api/v1/market/instruments'],
    requestBodies: true,
    defaultBodyValues: { tpTriggerPriceType: 'last', slTriggerPriceType: 'last', triggerPriceType: 'last' },
    generatedId: (key, value) => key === 'clientOrderId' && value.startsWith('b3cdedc0a20880ba'),
  },
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >=
      count,
    privateSocket: '/private/ws',
  },
  bybit: {
    rust: 'bybit',
    ts: { module: `${root}/bybit/bybit.exchange.ts`, className: 'BybitExchange', name: 'bybit' },
    /** Book topics get a message handler once each subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderbook.')).length >= count,
    privateSocket: '/v5/private',
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
