// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac, createPrivateKey, createPublicKey, verify } from 'node:crypto';
import { parseRequestJson } from './request-json.mjs';
import { subscriptionKeys, requestKey } from './requests.mjs';

const root = '../../packages/safe-cex/src/exchanges';

// Replay transport and signing clocks both default to wall-clock milliseconds.
const validWeexTimestamp = (time, now) => typeof time === 'string' && /^\d+$/.test(time)
  && Number.isSafeInteger(Number(time)) && Number(time) > 0
  && Number.isFinite(now) && Math.abs(Number(time) - now) <= 30_000;

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
const validBackpackTime = (timestamp, window, now) => /^\d+$/.test(String(timestamp ?? ''))
  && /^\d+$/.test(String(window ?? '')) && Number.isSafeInteger(Number(timestamp)) && Number(timestamp) > 0
  && Number.isSafeInteger(Number(window)) && Number(window) > 0 && Number(window) <= 60000
  && Math.abs(Number(timestamp) - now) <= Number(window);
const verifyBackpackSubscribe = (message, now = Date.now()) => {
  if (!(message.params ?? []).some((p) => p.startsWith('account.')) || message.method !== 'SUBSCRIBE') return true;
  const [key, signature, timestamp, window] = message.signature ?? [];
  return (
    key === backpackKey &&
    window === '5000' && validBackpackTime(timestamp, window, now) &&
    verify(
      null,
      Buffer.from(`instruction=subscribe&timestamp=${timestamp}&window=${window}`),
      backpackPublic,
      Buffer.from(signature ?? '', 'base64'),
    )
  );
};
const verifyBackpack = ({ method, url, headers, body, now = Date.now() }) => {
  const parsed = new URL(url);
  const instruction = backpackInstructions[`${method} ${parsed.pathname}`];
  if (!instruction) return true;
  const h = Object.fromEntries(
    (Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]),
  );
  if (h['x-api-key'] !== backpackKey || h['x-broker-id'] !== '1200'
    || !validBackpackTime(h['x-timestamp'], h['x-window'], now)) return false;
  const params = method === 'GET' ? Object.fromEntries(parsed.searchParams) : parseRequestJson(body ?? '{}');
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

// MEXC accepts snowflake IDs as either JSON numbers or strings. Keep every
// economic field's JSON type and every caller-provided client ID exact.
export const mexcRequestBody = (body, { url = '' } = {}) => {
  const ids = new Set(['id', 'orderId', 'positionId']);
  const visit = (value, key = '') => {
    if (ids.has(key) || (key === '<root-id>' && /\/private\/order\/cancel$/.test(new URL(url).pathname))) {
      if (typeof value === 'bigint') return String(value);
      if (JSON.isRawJSON(value)) return value.rawJSON;
      if (typeof value === 'number' && Number.isSafeInteger(value)) return String(value);
    }
    if (JSON.isRawJSON(value)) return value;
    if (Array.isArray(value)) return value.map((item) => visit(item, key === '' ? '<root-id>' : key));
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, visit(v, k)]));
    return value;
  };
  return visit(body);
};
export const mexcRequestUrl = (url) => {
  const parsed = new URL(url);
  const params = parsed.searchParams;
  if (parsed.hostname === 'api.mexc.com' && /^\/api\/v1\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname)
    && (params.get('states') === '1' || params.get('is_finished') === '0')
    && /^\d+$/.test(params.get('start_time') ?? '') && /^\d+$/.test(params.get('end_time') ?? '')) {
    params.set('start_time', String(BigInt(params.get('start_time')) - BigInt(params.get('end_time'))));
    params.set('end_time', 'now');
  }
  return parsed.href;
};

export const VENUES = {
  bitunix: {
    rust: 'bitunix',
    ts: { module: `${root}/bitunix/bitunix.exchange.ts`, className: 'BitunixExchange', name: 'bitunix' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.futuresWebsocket.messageHandlers).filter((key) => key.includes('_depthop_')).length >= count,
    privateSocket: '/private/',
    isWrite: (method) => method !== 'GET',
    matchRequestBody: true,
    compareFillPrices: true,
  },
  bitget: {
    rust: 'bitget',
    ts: { module: `${root}/bitget/bitget.exchange.ts`, className: 'BitgetExchange', name: 'bitget' },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(([key, handlers]) => key.startsWith('orderBook_') && handlers.length).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  bitgetusdc: {
    rust: 'bitgetusdc',
    ts: { module: `${root}/bitget/bitgetusdc.exchange.ts`, className: 'BitgetUSDCExchange', name: 'bitgetusdc' },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(([key, handlers]) => key.startsWith('orderBook_') && handlers.length).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  backpack: {
    rust: 'backpack',
    initialBookSnapshots: true,
    ignoreGeneratedBodyId: (key, value, callerIds, context) => key === 'clientId' && !callerIds.has(String(value))
      && context?.method === 'POST' && /^\/api\/v1\/orders?$/.test(new URL(context.url).pathname)
      && typeof value === 'number' && Number.isSafeInteger(value) && value > 0 && value <= 0xffff_ffff,
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

  hyperliquid: {
    rust: 'hyperliquid',
    matchRequestBody: true,
    fullSocketIdentity: true,
    requestKey: (method, url, body, callerIds) => requestKey(method, url, body, callerIds),
    ts: { module: `${root}/hyperliquid/hyperliquid.exchange.ts`, className: 'HyperliquidExchange', name: 'hyperliquid' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
    privateSocket: undefined,
    publicOptions: { key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
  },
  blofin: {
    rust: 'blofin',
    blofinProtocol: true,
    validateSocketMessage(socket, text, now = Date.now()) {
      if (text === 'ping' || text === 'pong') return;
      const frame = JSON.parse(text);
      if (frame.op === 'subscribe' && new URL(socket.url).pathname === '/ws/private' && !socket.loginAcknowledged)
        throw new Error('BloFin subscriptions preceded valid login acknowledgement');
      if (frame.op !== 'login') return;
      const arg = frame.args?.length === 1 ? frame.args[0] : {};
      const { apiKey, passphrase, timestamp, nonce, sign } = arg ?? {};
      if (!validWeexTimestamp(String(timestamp), now) || apiKey !== 'replay-key' || passphrase !== 'replay-passphrase'
        || typeof nonce !== 'string' || !nonce.length) throw new Error('invalid BloFin socket login');
      const expected = Buffer.from(createHmac('sha256', 'replay-secret')
        .update(`/users/self/verifyGET${timestamp}${nonce}`).digest('hex')).toString('base64');
      if (sign !== expected) throw new Error('invalid BloFin socket signature');
      socket.loginVerified = true;
    },
    validateSocketFrame(socket, text) {
      if (text === 'pong') return;
      const frame = JSON.parse(text);
      if (frame.event === 'login' && String(frame.code) === '0') {
        if (!socket.loginVerified) throw new Error('BloFin login acknowledgement preceded valid login');
        socket.loginAcknowledged = true;
      }
    },
    requestUrl(url) {
      const parsed = new URL(url);
      if (parsed.pathname === '/api/v1/market/instruments') {
        for (const [key, value] of parsed.searchParams) if (value === '') parsed.searchParams.delete(key);
      }
      return parsed.href;
    },
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
    mexcProtocol: true,
    requestBody: mexcRequestBody,
    requestUrl: mexcRequestUrl,
    generatedBodyId: (key) => key === 'externalOid',
    validateSocketMessage(socket, text, now = Date.now()) {
      const frame = JSON.parse(text);
      if (frame.method !== 'login') return;
      const { apiKey, reqTime, signature } = frame.param ?? {};
      if (!validWeexTimestamp(reqTime, now) || apiKey !== 'replay-key'
        || signature !== createHmac('sha256', 'replay-secret').update(`${apiKey}${reqTime}`).digest('hex')) {
        throw new Error('invalid MEXC socket login');
      }
      socket.loginVerified = true;
    },
    validateSocketFrame(socket, text) {
      const frame = JSON.parse(text);
      if (frame.channel === 'rs.login' && frame.data === 'success' && !socket.loginVerified) throw new Error('MEXC login acknowledgement preceded valid login');
    },
    matchRequestBody: true,
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
    bingxProtocol: true,
    // BingX owns one listener per URL; resubscriptions must recur on reconnect.
    subscriptionGenerations: true,
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
    validateRequest({ method, url, body = '', headers = {}, now = Date.now() }) {
      const parsed = new URL(url);
      if (parsed.pathname.startsWith('/capi/v3/market/')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      if (!validWeexTimestamp(time, now)) throw new Error('invalid WEEX authentication timestamp');
      const signature = createHmac('sha256', 'replay-secret').update(`${time}${method}${parsed.pathname}${parsed.search}${body || ''}`).digest('base64');
      if (lookup('ACCESS-KEY') !== 'replay-key' || lookup('ACCESS-PASSPHRASE') !== 'replay-passphrase' || lookup('ACCESS-SIGN') !== signature) throw new Error(`invalid WEEX signed request: ${method} ${parsed.pathname}`);
    },
    validateSocket(url, headers = {}, now = Date.now()) {
      if (!url.includes('/v3/ws/private')) return;
      const lookup = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name.toLowerCase())?.[1];
      const time = lookup('ACCESS-TIMESTAMP');
      if (!validWeexTimestamp(time, now)) throw new Error('invalid WEEX authentication timestamp');
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
    cryptoProtocol: true,
    // Authentication is validated separately; economic params stay exact.
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
    prepareTsReady: async (exchange, { isPrivate }) => {
      if (isPrivate) return;
      // Public-only start resolves while the socket is still CONNECTING.
      // Sending then makes BaseWebSocket reconnect before its first open.
      while (!exchange.publicWebsocket.isConnected) await new Promise((resolve) => setTimeout(resolve, 1));
      exchange.publicWebsocket.subscribeToMarketData(exchange.store.markets.map((m) => ({ instrumentName: m.id, indexInstrumentName: m.info?.underlyingSymbol })));
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
          } else {
            failures.push('unexpected outbound WS method ' + frame.method);
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
        if (record.type !== 'http' || record.method !== 'GET' || !(record.status >= 200 && record.status < 300)) continue;
        const url = new URL(record.url);
        if (url.pathname !== '/v5/market/instruments-info' || (url.searchParams.has('category') && url.searchParams.get('category') !== 'linear')) continue;
        const body = JSON.parse(record.body);
        if (Number(body.retCode) !== 0) continue;
        for (const row of body.result?.list ?? []) {
          if (['USDT', 'USDC'].includes(row.quoteCoin) && typeof row.symbol === 'string' && row.symbol)
            symbols.add(row.symbol);
        }
      }
      const socket = records.find((record) => record.type === 'ws' && new URL(record.url).pathname === '/v5/public/linear')?.url
        ?? 'wss://stream.bybit.com/v5/public/linear';
      return subscriptionKeys(socket, { op: 'subscribe', args: [...symbols].map((symbol) => `tickers.${symbol}`)
        .concat(books.map((symbol) => `orderbook.1000.${symbol}`)) });
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
