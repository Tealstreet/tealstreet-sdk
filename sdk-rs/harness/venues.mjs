import { createPrivateKey, createPublicKey, verify } from 'node:crypto';
// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
const root = '../../packages/safe-cex/src/exchanges';

// Public deterministic replay credentials, never a funded account.
const backpackSeed = Buffer.alloc(32, 7);
const backpackPrivate = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), backpackSeed]),
  format: 'der',
  type: 'pkcs8',
});
const backpackPublic = createPublicKey(backpackPrivate);
const backpackKey = backpackPublic.export({ format: 'der', type: 'spki' }).subarray(-32).toString('base64');
const backpackInstructions = {
  'GET /api/v1/account': 'accountQuery',
  'PATCH /api/v1/account': 'accountUpdate',
  'GET /api/v1/position': 'positionQuery',
  'GET /api/v1/capital/collateral': 'collateralQuery',
  'GET /api/v1/orders': 'orderQueryAll',
  'POST /api/v1/order': 'orderExecute',
  'POST /api/v1/orders': 'orderExecute',
  'DELETE /api/v1/order': 'orderCancel',
  'DELETE /api/v1/orders': 'orderCancelAll',
  'GET /wapi/v1/history/fills': 'fillHistoryQueryAll',
  'GET /wapi/v1/history/orders': 'orderHistoryQueryAll',
  'GET /wapi/v1/history/funding': 'fundingHistoryQueryAll',
};
const verifyBackpackSubscribe = (message) => {
  if (!(message.params ?? []).some((p) => p.startsWith('account.')) || message.method !== 'SUBSCRIBE') return true;
  const [key, signature, timestamp, window] = message.signature ?? [];
  return (
    key === backpackKey &&
    window === '5000' &&
    verify(
      null,
      Buffer.from(`instruction=subscribe&timestamp=${timestamp}&window=${window}`),
      backpackPublic,
      Buffer.from(signature ?? '', 'base64'),
    )
  );
};
const verifyBackpack = ({ method, url, headers, body }) => {
  const parsed = new URL(url);
  const instruction = backpackInstructions[`${method} ${parsed.pathname}`];
  if (!instruction) return true;
  const h = Object.fromEntries(
    (Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]),
  );
  if (h['x-api-key'] !== backpackKey || h['x-broker-id'] !== '1200') return false;
  const params = method === 'GET' ? Object.fromEntries(parsed.searchParams) : JSON.parse(body ?? '{}');
  const rows = Array.isArray(params) ? params : [params];
  const pieces = rows.map(
    (row) =>
      `instruction=${instruction}` +
      Object.keys(row)
        .sort()
        .filter((k) => row[k] != null)
        .map((k) => `&${k}=${encodeURIComponent(String(row[k]))}`)
        .join(''),
  );
  const payload = `${pieces.join('&')}&timestamp=${h['x-timestamp']}&window=${h['x-window']}`;
  return verify(null, Buffer.from(payload), backpackPublic, Buffer.from(h['x-signature'] ?? '', 'base64'));
};

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
  backpack: {
    rust: 'backpack',
    ts: { module: `${root}/backpack/backpack.exchange.ts`, className: 'BackpackExchange', name: 'backpack' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depth.')).length >= count,
    isPrivateRecord: (record) => record.type === 'ws' && JSON.parse(record.text).stream?.startsWith('account.'),
    streamOf: (text) => {
      try {
        return JSON.parse(text).stream;
      } catch {
        return undefined;
      }
    },
    marketFilter: (row) => row.symbol.endsWith('_PERP'),
    replayCredentials: { key: backpackKey, secret: backpackSeed.toString('base64') },
    verifyRequest: verifyBackpack,
    verifySubscribe: verifyBackpackSubscribe,
    compareSubscriptions: true,
    compareWriteOrder: true,
    compareClientIds: true,
    compareFillPrices: true,
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
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderbook.')).length >=
      count,
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
