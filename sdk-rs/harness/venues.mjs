// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
const root = '../../packages/safe-cex/src/exchanges';

const bybit = (name, className) => ({
  rust: name,
  ts: { module: `${root}/bybit/${name}.exchange.ts`, className, name },
  tsBooksSubscribed: (exchange, count) =>
    count === 0 || Object.keys((exchange.publicWebsocket ?? exchange.publicInverseWebsocket)?.messageHandlers ?? {}).filter((key) => key.startsWith('orderbook.')).length >= count,
  privateSocket: '/v5/private',
  matchRequestBody: true,
  volatileBodyFields: true,
});

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
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >= count,
    privateSocket: '/private/ws',
  },
  bybit: bybit('bybit', 'BybitExchange'),
  bybitinverse: bybit('bybitinverse', 'BybitInverseExchange'),
  bybitspot: bybit('bybitspot', 'BybitSpotExchange'),
  bybitdemo: bybit('bybitdemo', 'BybitDemoExchange'),
  hyrotrader: bybit('hyrotrader', 'HyrotraderExchange'),
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

/** Native Hyro callers supply endpoints; the harness uses the same public config as TS. */
export const configuredOptions = (name) => {
  if (name !== 'hyrotrader') return {};
  const env = process.env;
  const mode = env.NEXT_PUBLIC_HYROTRADER_PROD_REST_URL ? 'PROD' : 'SIMULATOR';
  const rest = env[`NEXT_PUBLIC_HYROTRADER_${mode}_REST_URL`]?.trim().replace(/\/+$/, '');
  if (!rest) throw new Error('HyroTrader endpoints are not configured');
  const ws = rest.replace(/^http/i, 'ws');
  return {
    endpoints: {
      rest,
      publicWs: env[`NEXT_PUBLIC_HYROTRADER_${mode}_PUBLIC_WS_URL`] || `${ws}/v5/public/linear`,
      privateWs: env[`NEXT_PUBLIC_HYROTRADER_${mode}_PRIVATE_WS_URL`] || `${ws}/v5/private`,
    },
  };
};
