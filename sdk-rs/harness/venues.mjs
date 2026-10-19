// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
const root = '../../packages/safe-cex/src/exchanges';

export const VENUES = {
  bitunix: {
    rust: 'bitunix',
    ts: { module: `${root}/bitunix/bitunix.exchange.ts`, className: 'BitunixExchange', name: 'bitunix' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.futuresWebsocket.messageHandlers).filter((key) => key.includes('_depthop_')).length >= count,
    privateSocket: '/private/',
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
