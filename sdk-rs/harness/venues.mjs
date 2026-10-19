// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
const root = '../../packages/safe-cex/src/exchanges';

export const VENUES = {
  hyperliquid: {
    rust: 'hyperliquid',
    ts: { module: `${root}/hyperliquid/hyperliquid.exchange.ts`, className: 'HyperliquidExchange', name: 'hyperliquid' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
    privateSocket: undefined,
    publicOptions: { key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
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
