import { createHmac, createPrivateKey, createPublicKey, verify } from 'node:crypto';
// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { parseRequestJson } from './request-json.mjs';
import { requestKey } from './requests.mjs';

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
  isWrite: (method) => method !== 'GET',
  // Caller IDs remain exact, even when the caller chooses a broker-prefixed ID.
  generatedBodyId: (key, value) =>
    ['clOrdId', 'algoClOrdId'].includes(key) && typeof value === 'string' && value.startsWith('e847386590ce4dBC'),
  respondToPing: true,
  validateLogin: true,
  compareFillPrices: true,
});

export const VENUES = {
  bitrue: {
    rust: 'bitrue',
    ts: { module: `${root}/bitrue/bitrue.exchange.ts`, className: 'BitrueExchange', name: 'bitrue' },
    privateSocket: '/stream?listenKey=',
    compareFillPrices: true,
    volatileBodyFields: true,
    generatedBodyId: (key) => key === 'clientOrderId',
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/listenKey'),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('_simple_depth_step0')).length >= count,
    validateRequest: ({ method, url, body, headers }) => {
      const parsed = new URL(url);
      if (!['fapi.bitrue.com', 'fapiws-auth.bitrue.com'].includes(parsed.hostname))
        throw new Error('Bitrue must emit a real upstream URL');
      const fields = Object.fromEntries(Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]));
      if (!fields['x-ch-apikey']) return;
      if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
      const timestamp = fields['x-ch-ts'];
      const payload = timestamp + method + parsed.pathname + parsed.search + (body ?? '');
      if (fields['x-ch-apikey'] !== 'replay-key' || !/^\d+$/.test(timestamp) ||
          Math.abs(Date.now() - Number(timestamp)) > 60_000 ||
          fields['x-ch-sign'] !== createHmac('sha256', 'replay-secret').update(payload).digest('hex'))
        throw new Error('Invalid Bitrue REST signature');
    },
    createSocketAudit: (_impl, { isPrivate, books }) => {
      const states = new Map();
      const failures = [];
      const state = (url) => {
        if (!states.has(url)) states.set(url, { channels: new Set(), pings: new Set() });
        return states.get(url);
      };
      return {
        received(url, text) {
          const frame = JSON.parse(text);
          const current = state(url);
          if (frame.ping !== undefined) current.pings.add(JSON.stringify({ pong: frame.ping }));
          else if (frame.event === 'ping') current.pings.add(JSON.stringify(frame.ts === undefined ? { event: 'pong' } : { event: 'pong', ts: frame.ts }));
          if (frame.channel && (frame.tick || frame.data) && frame.event_rep !== 'rep' && !current.channels.has(frame.channel))
            failures.push('data arrived without subscription: ' + frame.channel);
        },
        sent(url, text) {
          const current = state(url);
          if (text === 'pong') return;
          const frame = JSON.parse(text);
          if (frame.event === 'sub') current.channels.add(frame.params.channel);
          if (frame.event === 'unsub') current.channels.delete(frame.params.channel);
          if (frame.event === 'pong' || frame.pong !== undefined) {
            const expected = [...current.pings].find((raw) => {
              const ping = JSON.parse(raw);
              return Object.entries(ping).every(([key, value]) => frame[key] === value);
            });
            if (!expected) failures.push('unexpected Bitrue heartbeat reply');
            else current.pings.delete(expected);
          }
        },
        ready() {
          return !isPrivate || [...states].some(([url, s]) => url.startsWith('wss://fapiws.bitrue.com/stream?listenKey=') && s.channels.has('user_account_update'));
        },
        errors() {
          for (const [url, s] of states) {
            if (s.pings.size) failures.push('missing Bitrue heartbeat reply');
            if (url.includes('fmarket-ws')) for (const symbol of books) {
              const native = 'e_' + symbol.replace(/^E-/, '').replaceAll('-', '').toLowerCase();
              if (!s.channels.has(`market_${native}_simple_depth_step0`)) failures.push('missing Bitrue depth subscription: ' + symbol);
            }
          }
          return failures;
        },
      };
    },
  },
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
    isWrite: (method) => method !== 'GET',
  },
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
    volatileBodyFields: true,
    isWrite: (method) => method !== 'GET',
    defaultBodyValues: { tpTriggerPriceType: 'last', slTriggerPriceType: 'last', triggerPriceType: 'last' },
    generatedId: (key, value) => key === 'clientOrderId' && value.startsWith('b3cdedc0a20880ba'),
  },
  mexc: {
    rust: 'mexc',
    matchRequestBody: true,
    canonicalIntegerStrings: true,
    ignoredBodyFields: ['externalOid', 'requestId'],
    rollingListWindows: true,
    ts: { module: `${root}/mexc/mexc.exchange.ts`, className: 'MexcExchange', name: 'mexc' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depth.')).length >= count,
    // Public/private sockets share this URL; tapes identify their ordinal.
    privateSocket: undefined,
    independentLeverage: true,
    tsSettings: (exchange, store) =>
      store.markets.map((market) => {
        const config = exchange.symbolSettings.get(market.symbol) ?? exchange.deriveSymbolConfig(market.symbol);
        return {
          symbol: market.symbol,
          marginType: config.marginType,
          buyLeverage: config.buyLeverage,
          sellLeverage: config.sellLeverage,
        };
      }),
  },
  bingx: {
    rust: 'bingx',
    ts: { module: `${root}/bingx/bingx.exchange.ts`, className: 'BingxExchange', name: 'bingx' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('market.depth.')).length >=
      count,
    privateSocket: '?listenKey=',
    matchRequestBody: true,
    requestKey: (method, url, body, callerIds) => requestKey(method, url, body, callerIds),
  },
  weex: {
    rust: 'weex',
    ts: { module: `${root}/weex/weex.exchange.ts`, className: 'WeexExchange', name: 'weex' },
    privateSocket: '/v3/ws/private',
    matchRequestBody: true,
    volatileBodyFields: true,
    // WEEX's income history is a read despite using POST.
    isWrite: (method, url) =>
      method !== 'GET' && !(method === 'POST' && new URL(url).pathname === '/capi/v3/account/income'),
    options: { passphrase: 'replay-passphrase', privateWsHeaders: true },
    tsBooksSubscribed: (exchange, count) =>
      [...exchange.publicWebsocket.topics].filter((t) => t.endsWith('@depth200')).length >= count,
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
      const signature = createHmac('sha256', 'replay-secret')
        .update(`${time}${method}${parsed.pathname}${parsed.search}${body || ''}`)
        .digest('base64');
      if (
        lookup('ACCESS-KEY') !== 'replay-key' ||
        lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' ||
        lookup('ACCESS-SIGN') !== signature
      )
        throw new Error(`invalid WEEX signed request: ${method} ${parsed.pathname}`);
    },
    validateSocket(url, headers = {}) {
      if (!url.includes('/v3/ws/private')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}/v3/ws/private`).digest('base64');
      if (
        lookup('ACCESS-KEY') !== 'replay-key' ||
        lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' ||
        lookup('ACCESS-SIGN') !== signature
      )
        throw new Error('invalid WEEX upgrade authentication');
    },
  },
  cryptocom: {
    rust: 'cryptocom',
    ts: { module: root + '/cryptocom/cryptocom.exchange.ts', className: 'CryptocomExchange', name: 'cryptocom' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('book.')).length >= count,
    privateSocket: '/exchange/v1/user',
    matchRequestBody: true,
    requestBodyParams: true,
    requestBody: (body) => body?.params ?? body,
    publicTickers: (records) => {
      const seed = records.find((r) => r.type === 'http' && r.url.endsWith('/public/get-instruments'));
      return seed
        ? JSON.parse(seed.body)
            .result.data.filter((r) => r.inst_type === 'PERPETUAL_SWAP' && r.tradable)
            .map((r) => r.symbol)
        : [];
    },
    // KNOWN_TS_FIXES: public-only startup returns before market subscriptions.
    prepareTsReady: (exchange, { isPrivate }) => {
      if (!isPrivate)
        exchange.publicWebsocket.subscribeToMarketData(
          exchange.store.markets.map((m) => ({ instrumentName: m.id, indexInstrumentName: m.info?.underlyingSymbol })),
        );
    },
    // KNOWN_TS_FIXES: the global ticker/book throttles drop updates across
    // symbols. Reset after every frame so each next update gets a leading call,
    // without retaining trailing timers during accelerated replay.
    afterTsFrame: (exchange) => {
      exchange.publicWebsocket.handleTickerUpdate.cancel();
      exchange.publicWebsocket.handleBookUpdate.cancel();
    },
    isWrite: (method, url) =>
      method === 'POST' && /private\/(advanced\/)?(create|amend|cancel|change|close)-/.test(new URL(url).pathname),
    createSocketAudit: (impl, { isPrivate, books, tickers = [] }) => {
      const connections = new Map();
      const failures = [];
      const socket = (url) => {
        if (!connections.has(url))
          connections.set(url, { auth: false, authAck: false, channels: new Set(), heartbeats: new Set() });
        return connections.get(url);
      };
      return {
        received(url, text) {
          // Market data has no auth/heartbeat state to audit.
          if (!text.includes('public/heartbeat') && !text.includes('public/auth')) return;
          const frame = JSON.parse(text);
          if (frame.method === 'public/heartbeat') socket(url).heartbeats.add(frame.id);
          if (frame.method === 'public/auth' && frame.code === 0) {
            if (!socket(url).auth) failures.push('auth acknowledgement preceded valid auth request');
            socket(url).authAck = socket(url).auth;
          }
        },
        sent(url, text) {
          const frame = JSON.parse(text);
          const state = socket(url);
          if (frame.method === 'public/auth') {
            const payload = frame.method + frame.id + frame.api_key + frame.nonce;
            state.auth =
              Number.isSafeInteger(frame.id) &&
              Number.isSafeInteger(frame.nonce) &&
              Math.abs(Date.now() - frame.nonce) < 60_000 &&
              frame.api_key === 'replay-key' &&
              frame.sig === createHmac('sha256', 'replay-secret').update(payload).digest('hex');
            if (!state.auth) failures.push('invalid private WS auth signature');
          } else if (frame.method === 'subscribe') {
            if (url.endsWith('/user') && !state.authAck)
              failures.push('private subscriptions sent before authentication');
            for (const channel of frame.params.channels) state.channels.add(channel);
          } else if (frame.method === 'unsubscribe') {
            for (const channel of frame.params.channels) state.channels.delete(channel);
          } else if (frame.method === 'public/respond-heartbeat') {
            if (!state.heartbeats.delete(frame.id)) failures.push('unexpected heartbeat response id');
          }
        },
        ready() {
          const entries = [...connections.entries()];
          if (isPrivate && !entries.some(([url, state]) => url.endsWith('/user') && state.auth)) return false;
          if (
            tickers.length &&
            !entries.some(
              ([url, state]) =>
                url.endsWith('/market') &&
                tickers.slice(0, 350 - books.length).every((symbol) => state.channels.has('ticker.' + symbol)),
            )
          )
            return false;
          if (
            books.length &&
            !entries.some(
              ([url, state]) =>
                url.endsWith('/market') && books.every((symbol) => state.channels.has('book.' + symbol + '.50')),
            )
          )
            return false;
          return true;
        },
        errors() {
          for (const [url, state] of connections) {
            if (state.heartbeats.size) failures.push('heartbeat response missing');
            if (url.endsWith('/user') && isPrivate) {
              if (!state.auth) failures.push('private WS never authenticated');
              const required = [
                'user.order',
                'user.advance.order',
                'user.positions',
                'user.balance',
                ...(impl === 'rust' ? ['user.trade'] : []),
              ];
              for (const channel of required)
                if (!state.channels.has(channel)) failures.push('missing private subscription ' + channel);
            }
            if (url.endsWith('/market'))
              for (const symbol of tickers.slice(0, 350 - books.length)) {
                if (!state.channels.has('ticker.' + symbol)) failures.push('missing ticker subscription ' + symbol);
              }
            if (url.endsWith('/market'))
              for (const symbol of books) {
                if (!state.channels.has('book.' + symbol + '.50')) failures.push('missing book subscription ' + symbol);
              }
          }
          return failures;
        },
      };
    },
    checkRequest: (method, url, body) => {
      const path = new URL(url).pathname.replace('/exchange/v1/', '');
      if (!path.startsWith('private/')) return true;
      try {
        const envelope = parseRequestJson(body);
        const params = Object.keys(envelope.params)
          .sort()
          .map((key) => {
            const value = envelope.params[key];
            return key + (Array.isArray(value) ? value.join('') : String(value));
          })
          .join('');
        const payload = envelope.method + envelope.id + envelope.api_key + params + envelope.nonce;
        return (
          method === 'POST' &&
          envelope.method === path &&
          Number.isSafeInteger(envelope.id) &&
          Number.isSafeInteger(envelope.nonce) &&
          Math.abs(Date.now() - envelope.nonce) < 60_000 &&
          envelope.api_key === 'replay-key' &&
          createHmac('sha256', 'replay-secret').update(payload).digest('hex') === envelope.sig
        );
      } catch {
        return false;
      }
    },
  },
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >=
      count,
    privateSocket: '/private/ws',
    volatileBodyFields: true,
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
  },
  bybit: {
    rust: 'bybit',
    ts: { module: `${root}/bybit/bybit.exchange.ts`, className: 'BybitExchange', name: 'bybit' },
    /** Book topics get a message handler once each subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderbook.')).length >=
      count,
    startupSubscriptions: (records, books) => {
      const symbols = new Set();
      for (const record of records) {
        if (
          record.type !== 'http' ||
          record.method !== 'GET' ||
          !(record.status >= 200 && record.status < 300) ||
          new URL(record.url).pathname !== '/v5/market/instruments-info'
        )
          continue;
        const body = JSON.parse(record.body);
        if (Number(body.retCode) !== 0) continue;
        for (const row of body.result?.list ?? []) {
          if (['USDT', 'USDC'].includes(row.quoteCoin) && typeof row.symbol === 'string' && row.symbol)
            symbols.add(row.symbol);
        }
      }
      const socket = 'wss://stream.bybit.com/v5/public/linear subscribe ';
      return [...symbols]
        .map((symbol) => `${socket}tickers.${symbol}`)
        .concat(books.map((symbol) => `${socket}orderbook.1000.${symbol}`));
    },
    privateSocket: '/v5/private',
    tsRejectReason: (order) =>
      order.status === 'canceled' && order.info?.rejectReason === 'EC_PostOnlyWillTakeLiquidity'
        ? 'PostOnly'
        : undefined,
    isWrite: (method) => method === 'POST',
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
