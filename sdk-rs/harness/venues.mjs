import { initializeTxFlowCrypto, createTxFlowRequestValidator, validateTxFlowSocketMessage, txFlowRequestBody, createTxFlowSocketAudit } from './txflow-protocol.mjs';
import { createBinanceSpotSocketAudit, prepareBinanceSpot, spotSocketRequest, spotSocketResponse, validateBinanceRequest, binanceSubscriptionMessage, binanceTickerSymbols, binanceRequestOptions } from './binance-protocol.mjs';
import { COINBASE_REPLAY, verifyCoinbaseRequest, verifyCoinbaseSubscribe, coinbaseSubscription, createCoinbaseSocketAudit } from './coinbase-protocol.mjs';
import { polymarket } from './polymarket.mjs';
import { breakoutprop } from './breakoutprop-requests.mjs';
import { uniqueRequestHeaders } from './protocol-headers.mjs';
// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac, createPrivateKey, createPublicKey, verify } from 'node:crypto';
import { APEX_SEED, apexWireBody, createApexSocketAudit, prepareApexReplay, prepareApexTs, validateApexRequest, apexRequestOptions } from './apexomni-protocol.mjs';
import { createKrakenRequestAudit, validateKrakenSocketMessage, validateKrakenSocketFrame, krakenRequestBody, krakenCommandResult } from './kraken-protocol.mjs';
let krakenRequestAudit;
import { paradexProtocol } from './paradex-protocol.mjs';
import { createWooSocketAudit, validateWooRequest } from './woo-protocol.mjs';
import { kucoin } from './kucoin.mjs';
import { createRequire } from 'node:module';
import { asterRequestUrl, asterSocketAudit, createAsterRequestValidator, initializeAsterCrypto } from './asterdex-requests.mjs';
import { parseRequestJson } from './request-json.mjs';
import { subscriptionKeys, requestKey } from './requests.mjs';
import { initializeLighterCrypto, createLighterVerifier, lighterRequestBody } from './lighter-crypto.mjs';



const root = '../../packages/safe-cex/src/exchanges';

const lighter = (name) => {
  let verifier;
  return {
    rust: name,
    ts: { module: `${root}/lighter/lighter.exchange.ts`, className: 'LighterExchange', name },
    replayCredentials: { applicationId: '42', apiKeyIndex: '3', secret: '01' + '00'.repeat(39), key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
    async initializeReplay(impl, options) {
      if (options.publicDataOnly) return;
      const oracle = await initializeLighterCrypto(impl);
      verifier = createLighterVerifier(options, oracle, name);
    },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('order_book:')).length >=
      count,
    isPrivateRecord: (record) => record.type === 'ws' && /account_all/.test(record.text ?? ''),
    isWrite: (method) => method !== 'GET',
    requestBodies: true,
    requestBody: lighterRequestBody,
    requestUrl(url) {
      const parsed = new URL(url);
      parsed.searchParams.delete('auth');
      return parsed.href;
    },
    subscriptionMessage(message) {
      return message.auth === undefined ? message : { ...message, auth: '<verified-auth>' };
    },
    verifyRequest: (request) => verifier?.request(request) ?? false,
    verifySubscribe: (message) => verifier?.subscribe(message) ?? false,
    validateMatchedRequest: (record, request) => verifier?.matched(record, request),
    commandResult: (result, command, index) => verifier?.commandResult(result, command, index) ?? result,
    compareFillPrices: true,
    compareWriteOrder: true,
  };
};

// Existing venues retain typed request matching after Tape extraction.
// Backpack separately checks its documented numeric wire equivalences and write order.
const typedRequest = function (method, url, body, callerIds) {
  return requestKey(method, url, body, callerIds, this);
};

const bybit = (name, className) => ({
  rust: name,
  family: 'bybit',
  subscriptionGenerations: true,
  compareWriteOrder: true,
  ts: { module: `${root}/bybit/${name}.exchange.ts`, className, name },
  tsBooksSubscribed: (exchange, count) =>
    count === 0 ||
    Object.keys((exchange.publicWebsocket ?? exchange.publicInverseWebsocket)?.messageHandlers ?? {}).filter((key) =>
      key.startsWith('orderbook.'),
    ).length >= count,
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
      const category = name === 'bybitinverse' ? 'inverse' : name === 'bybitspot' ? 'spot' : 'linear';
      const recordedCategory = new URL(record.url).searchParams.get('category');
      if (recordedCategory && recordedCategory !== category) continue;
      const body = JSON.parse(record.body);
      if (Number(body.retCode) !== 0) continue;
      for (const row of body.result?.list ?? []) {
        if (
          (name === 'bybitinverse' ? row.quoteCoin === 'USD' : ['USDT', 'USDC'].includes(row.quoteCoin)) &&
          typeof row.symbol === 'string' &&
          row.symbol
        )
          symbols.add(row.symbol);
      }
    }
    const category = name === 'bybitinverse' ? 'inverse' : name === 'bybitspot' ? 'spot' : 'linear';
    const configured = records.find((record) => record.type === 'config')?.options?.endpoints?.publicWs;
    const observed = records.find(
      (record) => record.type === 'ws' && record.url?.includes(`/v5/public/${category}`),
    )?.url;
    const url = new URL(configured ?? observed ?? `wss://stream.bybit.com/v5/public/${category}`);
    return subscriptionKeys(url.toString(), {
      op: 'subscribe',
      args: [...symbols]
        .map((symbol) => `tickers.${symbol}`)
        .concat(books.map((symbol) => `orderbook.${name === 'bybitspot' ? 200 : 1000}.${symbol}`)),
    });
  },
  tsRejectReason: (order) =>
    order.status === 'canceled' && order.info?.rejectReason === 'EC_PostOnlyWillTakeLiquidity' ? 'PostOnly' : undefined,
  privateSocket: '/v5/private',
  bootstrapPrivateFromTape: name === 'hyrotrader',
  requestUrl:
    name === 'bybitspot'
      ? (url) => {
          const parsed = new URL(url);
          // Spot instruments are unpaginated; TS's limit/cursor fields are invalid.
          if (parsed.pathname === '/v5/market/instruments-info' && parsed.searchParams.get('category') === 'spot') {
            parsed.searchParams.delete('limit');
            parsed.searchParams.delete('cursor');
          }
          return parsed.toString();
        }
      : undefined,
  requestOptions: name === 'hyrotrader' ? (method, url) => ({
    generatedBodyId: (key, value) => method === 'POST' && /^\/v5\/order\/create(?:-batch)?$/.test(url.pathname)
      && key === 'orderLinkId' && typeof value === 'string' && value.length > 0 && value.length <= 36 && !/[^A-Za-z0-9_-]/.test(value),
  }) : undefined,
  isWrite: (method) => method === 'POST',
  requestKey: typedRequest,
  matchRequestBody: true,
  volatileBodyFields: true,
});

const base58 = (bytes) => {
  const alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
  let value = BigInt(`0x${Buffer.from(bytes).toString('hex')}`);
  let result = '';
  while (value) {
    result = alphabet[Number(value % 58n)] + result;
    value /= 58n;
  }
  for (const byte of bytes) {
    if (byte !== 0) break;
    result = '1' + result;
  }
  return result;
};
const orderlySeed = Buffer.alloc(32, 11);
const orderlyPrivate = createPrivateKey({
  key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), orderlySeed]),
  format: 'der',
  type: 'pkcs8',
});
const orderlyPublic = createPublicKey(orderlyPrivate);
export const ORDERLY_REPLAY = {
  key: `ed25519:${base58(orderlyPublic.export({ format: 'der', type: 'spki' }).subarray(-32))}`,
  secret: `ed25519:${base58(orderlySeed)}`,
  applicationId: `0x${'11'.repeat(32)}`,
  // OrderlyNetwork/js-sdk packages/net/src/ws/ws.ts COMMON_ID.
  publicApplicationId: 'OqdphuyCtYWxwzhxyLLjOWNdFP7sQt8RPWzmb5xY',
};
export const ORDERLY_BROKERS = {
  'tealstreet-dex': 'TEALSTREET',
  kodiakfi: 'TEALKODIAK',
  mode: 'TEALMODE',
  salsadex: 'TEALSALSA',
  what: 'TEALWHAT',
  woofi: 'TEALSTREET',
};
const orderlyClasses = {
  'tealstreet-dex': 'TealstreetDEXExchange',
  kodiakfi: 'KodiakFIExchange',
  mode: 'MODEExchange',
  salsadex: 'SalsaDEXExchange',
  what: 'WhatExchange',
  woofi: 'WoofiExchange',
};
const orderlyNative = (symbol) => symbol.replace(/^(.+)USDC(_[A-Za-z0-9_-]+)?$/, (_, base, suffix = '') => `PERP_${base}_USDC${suffix}`);
const orderlyPrivateUrl = (url) => new URL(url).pathname.startsWith('/v2/ws/private/stream/');
const orderlySocketRequest = (message) => {
  if (message.event !== 'request') return undefined;
  if (
    message.params?.type !== 'orderbook' ||
    typeof message.params.symbol !== 'string' ||
    !message.params.symbol.startsWith('PERP_') ||
    typeof message.id !== 'string' ||
    !message.id
  )
    throw new Error('invalid Orderly orderbook request');
  return { key: JSON.stringify(['orderbook', message.params.symbol]), id: message.id };
};
const orderlySocketResponse = (message) => {
  if (message.event !== 'request') return undefined;
  if (
    typeof message.id !== 'string' ||
    !message.id ||
    (message.success === true && typeof message.data?.symbol !== 'string')
  )
    throw new Error('invalid Orderly orderbook response');
  return {
    id: message.id,
    key: message.data?.symbol ? JSON.stringify(['orderbook', message.data.symbol]) : undefined,
  };
};
const orderlySignature = (payload, signature) => {
  try {
    return (
      typeof signature === 'string' &&
      /^[A-Za-z0-9_-]+={0,2}$/.test(signature) &&
      verify(null, Buffer.from(payload), orderlyPublic, Buffer.from(signature, 'base64url'))
    );
  } catch {
    return false;
  }
};
const orderlyAuth = (message) => {
  if (message.event !== 'auth') return true;
  const params = message.params;
  return (
    params?.orderly_key === ORDERLY_REPLAY.key &&
    Number.isSafeInteger(params.timestamp) &&
    Math.abs(Date.now() - params.timestamp) < 60000 &&
    orderlySignature(String(params.timestamp), params.sign)
  );
};
const orderly = (alias) => {
  const name = alias === 'orderly' ? 'tealstreet-dex' : alias;
  return {
    family: 'orderly',
    orderlyProtocol: true,
    subscriptionGenerations: true,
    publicTickers: (records) => {
      const seed = records.find((r) => r.type === 'http' && new URL(r.url).pathname === '/v1/public/info');
      return seed ? JSON.parse(seed.body).data.rows.map((row) => row.symbol) : [];
    },
    rust: alias,
    ts: {
      module: `${root}/../__tests__/reference/orderly/exchanges/${name}/${name}.exchange.ts`,
      className: orderlyClasses[name],
      name,
    },
    options: { applicationId: ORDERLY_REPLAY.applicationId, testnet: false },
    publicOptions: { applicationId: ORDERLY_REPLAY.publicApplicationId },
    configureTs(exchange) {
      exchange.publicWebsocket.replaySnapshots = true;
    },
    replayCredentials: { key: ORDERLY_REPLAY.key, secret: ORDERLY_REPLAY.secret },
    privateSocket: '/v2/ws/private/stream/',
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('@orderbookupdate')).length >=
      count,
    streamOf: (text) => {
      try {
        const message = JSON.parse(text);
        return message.data && message.topic;
      } catch {
        return undefined;
      }
    },
    socketRequest: orderlySocketRequest,
    socketResponse: orderlySocketResponse,
    requireSocketRequestRecords: true,
    matchRequestBody: true,
    requestBodies: true,
    compareWriteOrder: true,
    compareClientIds: true,
    compareFillPrices: true,
    isWrite: (method) => method !== 'GET',
    verifySubscribe: orderlyAuth,
    verifyRequest: ({ method, url, headers, body = '' }) => {
      const parsed = new URL(url);
      if (parsed.pathname.includes('/public/') || parsed.pathname === '/v1/kline') return true;
      const h = Object.fromEntries(
        (Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([key, value]) => [
          key.toLowerCase(),
          String(value),
        ]),
      );
      if (
        h['orderly-key'] !== ORDERLY_REPLAY.key ||
        h['orderly-account-id'] !== ORDERLY_REPLAY.applicationId ||
        !/^\d+$/.test(h['orderly-timestamp'] ?? '') ||
        !Number.isSafeInteger(Number(h['orderly-timestamp'])) ||
        Math.abs(Date.now() - Number(h['orderly-timestamp'])) >= 60000
      )
        return false;
      const contentType = h['content-type']?.split(';')[0];
      if (
        contentType !== (['GET', 'DELETE'].includes(method) ? 'application/x-www-form-urlencoded' : 'application/json')
      )
        return false;
      const text = `${h['orderly-timestamp']}${method}${parsed.pathname}${parsed.search}${body || ''}`;
      return (
        (h['orderly-textsign'] === undefined || h['orderly-textsign'] === text) &&
        orderlySignature(text, h['orderly-signature'])
      );
    },
    createSocketAudit: (_impl, { isPrivate, books, tickers = [] }) => {
      const connections = new Map();
      const failures = [];
      const socket = (url, generation = 1) => {
        generation = generation?.generation ?? generation;
        const key = `${url} ${generation}`;
        if (!connections.has(key))
          connections.set(key, {
            url,
            auth: false,
            authAck: false,
            channels: new Set(),
            serverPings: 0,
            closed: false,
          });
        return connections.get(key);
      };
      const active = () => [...connections.values()].filter((s) => !s.closed);
      const publicTopics = [
        'tickers',
        'bbos',
        'markprices',
        ...tickers.map((native) => `${native}@estfundingrate`),
        ...books.map((symbol) => `${orderlyNative(symbol)}@orderbookupdate`),
      ];
      const privateTopics = ['executionreport', 'algoexecutionreportv2', 'position'];
      return {
        connected(url, generation) {
          socket(url, generation);
        },
        closed(url, generation) {
          socket(url, generation).closed = true;
        },
        sent(url, text, generation) {
          const message = JSON.parse(text),
            state = socket(url, generation);
          if (message.event === 'auth') {
            state.auth = orderlyPrivateUrl(url) && orderlyAuth(message);
            if (!state.auth) failures.push('invalid Orderly private auth signature');
          } else if (['subscribe', 'unsubscribe'].includes(message.event)) {
            if (orderlyPrivateUrl(url) && !state.authAck)
              failures.push('Orderly private subscription preceded authentication');
            if (typeof message.topic !== 'string' || !message.topic)
              failures.push('invalid Orderly subscription topic');
            if (message.event === 'subscribe') {
              state.channels.add(message.topic);
              if (state.channels.size > 100) failures.push('Orderly socket exceeds its 100-topic limit');
            }
            else state.channels.delete(message.topic);
          } else if (message.event === 'pong') {
            if (!state.serverPings) failures.push('unexpected Orderly heartbeat response');
            else state.serverPings--;
          } else if (message.event === 'request' && orderlyPrivateUrl(url)) {
            failures.push('Orderly public snapshot requested on private socket');
          }
        },
        received(url, text, generation) {
          const message = JSON.parse(text),
            state = socket(url, generation);
          if (['subscribe', 'unsubscribe'].includes(message.event) && message.success === false) {
            if (message.event === 'subscribe') {
              if (typeof message.topic === 'string') state.channels.delete(message.topic);
              else state.channels.clear();
            }
            failures.push(`rejected Orderly ${message.event}: ${message.topic ?? 'unknown topic'}`);
          }
          if (message.event === 'ping') state.serverPings++;
          if (message.event === 'auth' && message.success === true) {
            if (!state.auth) failures.push('Orderly auth success preceded a valid request');
            state.authAck = state.auth;
          }
          if (message.data && message.topic) {
            if (!state.channels.has(message.topic)) failures.push(`unsubscribed Orderly data ${message.topic}`);
            if (orderlyPrivateUrl(url) && !state.authAck) failures.push('Orderly private data preceded authentication');
          }
        },
        ready() {
          return (
            publicTopics.every((t) => active().some((s) => !orderlyPrivateUrl(s.url) && s.channels.has(t))) &&
            (!isPrivate || active().some((s) => orderlyPrivateUrl(s.url) && s.auth))
          );
        },
        errors() {
          const errors = [...failures];
          if (!publicTopics.every((t) => active().some((s) => !orderlyPrivateUrl(s.url) && s.channels.has(t))))
            errors.push('missing Orderly public subscriptions');
          if (
            isPrivate &&
            !active().some(
              (s) => orderlyPrivateUrl(s.url) && s.authAck && privateTopics.every((t) => s.channels.has(t)),
            )
          )
            errors.push('missing Orderly authenticated private subscriptions');
          if (active().some((s) => s.serverPings)) errors.push('missing Orderly heartbeat response');
          return errors;
        },
      };
    },
  };
};


// Replay transport and signing clocks both default to wall-clock milliseconds.
const validWeexTimestamp = (time, now) =>
  typeof time === 'string' &&
  /^\d+$/.test(time) &&
  Number.isSafeInteger(Number(time)) &&
  Number(time) > 0 &&
  Number.isFinite(now) &&
  Math.abs(Number(time) - now) <= 30_000;

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
const validBackpackTime = (timestamp, window, now) =>
  /^\d+$/.test(String(timestamp ?? '')) &&
  /^\d+$/.test(String(window ?? '')) &&
  Number.isSafeInteger(Number(timestamp)) &&
  Number(timestamp) > 0 &&
  Number.isSafeInteger(Number(window)) &&
  Number(window) > 0 &&
  Number(window) <= 60000 &&
  Math.abs(Number(timestamp) - now) <= Number(window);
const verifyBackpackSubscribe = (message, now = Date.now()) => {
  if (!(message.params ?? []).some((p) => p.startsWith('account.')) || message.method !== 'SUBSCRIBE') return true;
  const [key, signature, timestamp, window] = message.signature ?? [];
  return (
    key === backpackKey &&
    window === '5000' &&
    validBackpackTime(timestamp, window, now) &&
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
  if (
    h['x-api-key'] !== backpackKey ||
    h['x-broker-id'] !== '1200' ||
    !validBackpackTime(h['x-timestamp'], h['x-window'], now)
  )
    return false;
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
    if (value && typeof value === 'object')
      return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, visit(v, k)]));
    return value;
  };
  return visit(body);
};
export const mexcRequestUrl = (url) => {
  const parsed = new URL(url);
  const params = parsed.searchParams;
  if (
    parsed.hostname === 'api.mexc.com' &&
    /^\/api\/v1\/private\/(planorder|stoporder)\/list\/orders$/.test(parsed.pathname) &&
    (params.get('states') === '1' || params.get('is_finished') === '0') &&
    /^\d+$/.test(params.get('start_time') ?? '') &&
    /^\d+$/.test(params.get('end_time') ?? '')
  ) {
    params.set('start_time', String(BigInt(params.get('start_time')) - BigInt(params.get('end_time'))));
    params.set('end_time', 'now');
  }
  return parsed.href;
};

const coinbase = (name) => ({
  family: 'coinbase',
  preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  generatedId: () => false,
  currencyBalances: true,
  rust: name,
  ts: {
    module: `${root}/../__tests__/reference/coinbase/${name}.exchange.ts`,
    className: name === 'coinbasespot' ? 'CoinbaseSpotExchange' : 'CoinbaseExchange',
    name,
  },
  publicOptions: { spotOnly: true, extra: { coinbasePublicSpot: true } },
  options: { testnet: false, ...(name === 'coinbasespot' ? { spotOnly: true } : {}) },
  replayCredentials: COINBASE_REPLAY,
  privateSocket: 'advanced-trade-ws-user.coinbase.com',
  tsBooksSubscribed: (exchange, count) =>
    Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('level2.')).length >= count,
  verifyRequest: verifyCoinbaseRequest,
  verifyPublicRequest: true,
  verifySubscribe: verifyCoinbaseSubscribe,
  subscriptionMessage: coinbaseSubscription,
  createSocketAudit: createCoinbaseSocketAudit,
  streamOf: (text) => {
    try {
      const message = JSON.parse(text);
      const channel = message.channel === 'l2_data' ? 'level2' : message.channel;
      if (['heartbeats', 'user'].includes(channel)) return channel;
      const product = message.events?.[0]?.product_id;
      return product ? `${channel}.${product}` : undefined;
    } catch {
      return undefined;
    }
  },
  subscriptionGenerations: true,
  matchRequestBody: true,
  requestBodies: true,
  compareWriteOrder: true,
  compareClientIds: true,
  compareFillPrices: true,
  isWrite: (method) => method !== 'GET',
  requestKey: typedRequest,
  requestOptions: (method, url) => ({ generatedBodyId: (key, value) => method === 'POST' && url.pathname === '/api/v3/brokerage/orders'
    && key === 'client_order_id' && typeof value === 'string' && value.startsWith('tealstreet-')
    && value.length > 11 && value.length <= 32 && !/[^A-Za-z0-9]/.test(value.slice(11)) }),
});

export const VENUES = {
  txflow: {
    rust: 'txflow',
    independentLeverage: true,
    options: { testnet: true },
    virtualPrivateClock: true,
    recordPublicCorsDirect: true,
    recordAfterReady: true,
    ts: { module: `${root}/../__tests__/reference/txflow/txflow.exchange.ts`, className: 'TxFlowExchange', name: 'txflow' },
    requestKey: typedRequest,
    generatedId: () => false,
    preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
    preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
    requestBody: txFlowRequestBody,
    createSocketAudit: createTxFlowSocketAudit,
    requestBodies: true,
    matchRequestBody: true,
    volatileBodyFields: true,
    compareWriteOrder: true,
    compareFillPrices: true,
    compareSubscriptions: true,
    compareClientIds: true,
    replayCredentials: { applicationId: '0x0000000000000000000000000000000000000001', secret: '0x' + 'ab'.repeat(32), key: '' },
    initializeReplay(_impl, options) {
      if (!options.publicDataOnly) return initializeTxFlowCrypto();
    },
    createRequestValidator: createTxFlowRequestValidator,
    validateSocketMessage: validateTxFlowSocketMessage,
    isWrite: (_method, url) => new URL(url).pathname === '/exchange',
    isPrivateRecord: (r) => r.type === 'ws' && /"channel":"(?:orderUpdates|clearinghouseState|userAssetData|activeAssetData)"/.test(r.text ?? ''),
    tsBooksSubscribed: (exchange, count) => exchange.nodeSocket?.bookListeners.size >= count,
    tsSettings: (_exchange, store) => Object.entries(store.fetchedLeverage).map(([symbol, setting]) => ({ symbol, ...setting })),
  },
  grvt: (() => {
    // Public deterministic replay key. Never use this well-known key for funds.
    const secret = '0x' + '01'.repeat(32);
    const signer = '0x1a642f0E3c3aF545E7AcBD38b07251B3990914F1';
    let crypto;
    const initializeCrypto = () => crypto ??= createRequire(import.meta.url)('ethers');
    const account = 'fixture-account';
    const builder = '0xee8f3559f298fffa79b8253437969d15b5dccff3';
    const instruments = {
      BTC_USDT_Perp: { hash: '0x030501', decimals: 9 },
      ETH_USDT_Perp: { hash: '0x030502', decimals: 9 },
    };
    // This schema is independent of the venue encoder and the GRVT SDK signer.
    const types = {
      OrderWithBuilderFee: [
        { name: 'subAccountID', type: 'uint64' }, { name: 'isMarket', type: 'bool' },
        { name: 'timeInForce', type: 'uint8' }, { name: 'postOnly', type: 'bool' },
        { name: 'reduceOnly', type: 'bool' }, { name: 'legs', type: 'OrderLeg[]' },
        { name: 'builder', type: 'address' }, { name: 'builderFee', type: 'uint32' },
        { name: 'nonce', type: 'uint32' }, { name: 'expiration', type: 'int64' },
      ],
      OrderLeg: [
        { name: 'assetID', type: 'uint256' }, { name: 'contractSize', type: 'uint64' },
        { name: 'limitPrice', type: 'uint64' }, { name: 'isBuyingContract', type: 'bool' },
      ],
    };
    return {
      rust: 'grvt',
      preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
      preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
      initializeReplay(_impl, options) {
        if (!options.publicDataOnly) initializeCrypto();
      },
      ts: { module: `${root}/grvt/grvt.exchange.ts`, className: 'GrvtExchange', name: 'grvt' },
      tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((k) => k.startsWith('orderbook.')).length >= count,
      options: { grvtSessionProxy: true, grvtTradingAccountId: '777' },
      replayCredentials: { key: 'grvt-replay-key', secret },
      marketFilter: (row) => row.symbol.endsWith('_PERP'),
      requestBodies: true,
      requestKey: typedRequest,
      generatedId: () => false,
      compareClientIds: true,
      compareFillPrices: true,
      compareSubscriptions: true,
      compareWriteOrder: true,
      isPrivateRecord: (record) => record.type === 'ws' && new URL(record.url).hostname.startsWith('trades.'),
      privateSocket: 'wss://trades.grvt.io/ws',
      isWrite: (_method, url) => /\/(create_order|cancel_order|cancel_all_orders|set_initial_leverage|set_position_config)$/.test(new URL(url).pathname),
      prepareTs(exchange) {
        // The production private stream requires a proxy-selected host. Supply
        // only that host boundary, using the recorded real upstream identity;
        // leave its first-frame auth, subscriptions and handlers untouched.
        exchange.options = { ...exchange.options, transport: {
          name: 'grvt-tape-routing',
          rest: ({ config }) => ({ baseURL: config?.baseURL, proxied: false }),
          restAdapter: () => undefined, proxyPrefix: () => '', publicCorsProxy: () => undefined,
          privateProxy: () => undefined, serverProxyUrl: () => undefined,
          requireProxy: (options) => options, wsConnection: () => undefined,
          wsRouting: () => ({ restProxyHost: '', howEgress: 'sg',
            wsPublic: { tunnelled: false }, wsBusiness: { tunnelled: false },
            wsPrivate: { tunnelled: false, host: 'wss://trades.grvt.io/ws' },
          }),
        } };
      },
      requestBody(body, { url }) {
        if (!new URL(url).pathname.endsWith('/create_order') || !body?.o) return body;
        const order = body.o;
        // Only independently verified signature entropy and clock values vary.
        // Preserve field presence, signer, chain ID, caller/native IDs and fees.
        return { ...body, o: { ...order,
          s: Object.fromEntries(Object.entries(order.s ?? {}).map(([key, value]) => [key,
            ['r', 's1', 'v', 'e', 'n'].includes(key) ? `<${key}>` : value])),
          m: Object.fromEntries(Object.entries(order.m ?? {}).map(([key, value]) => [key, key === 'ct' ? '<ct>' : value])),
        } };
      },
      validateSocket(url) {
        const parsed = new URL(url);
        if (parsed.protocol !== 'wss:' || !['market-data.grvt.io', 'trades.grvt.io'].includes(parsed.hostname))
          throw new Error('GRVT must emit a real upstream socket URL');
      },
      validateRequest({ method, url, body, headers }) {
        const parsed = new URL(url);
        if (method !== 'POST' || parsed.protocol !== 'https:' || !['market-data.grvt.io', 'trades.grvt.io', 'edge.grvt.io'].includes(parsed.hostname))
          throw new Error('GRVT must emit a real upstream REST URL');
        const h = uniqueRequestHeaders(headers);
        if (parsed.hostname === 'market-data.grvt.io') {
          if (['cookie', 'x-ts-grvt-session', 'x-grvt-account-id', 'x-api-key'].some((key) => h[key]))
            throw new Error('GRVT public market data must not carry private credentials');
          return;
        }
        if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
        const data = typeof body === 'string' ? parseRequestJson(body) : body;
        if (parsed.hostname === 'edge.grvt.io') {
          if (parsed.pathname !== '/auth/api_key/login' || data?.api_key !== 'grvt-replay-key' || h['x-grvt-remember-me'] !== 'true')
            throw new Error('Invalid GRVT login credentials');
          return;
        }
        const session = h['x-ts-grvt-session'];
        if (!['fixture-session-1', 'fixture-session-2'].includes(session) || h['x-grvt-account-id'] !== account
          || (h.cookie !== undefined && !h.cookie.split(';').map((v) => v.trim()).includes(`gravity=${session}`)))
          throw new Error('Invalid GRVT private session credentials');
        if (!parsed.pathname.endsWith('/create_order')) return;
        const o = data?.o, sig = o?.s;
        if (!o || !sig || !Array.isArray(o.l) || o.l.length !== 1 || !o.m || o.sa !== '777'
          || typeof o.im !== 'boolean' || typeof o.po !== 'boolean' || typeof o.ro !== 'boolean'
          || !['GOOD_TILL_TIME', 'ALL_OR_NONE', 'IMMEDIATE_OR_CANCEL', 'FILL_OR_KILL'].includes(o.ti)
          || sig.ci !== '325' || sig.s?.toLowerCase() !== signer.toLowerCase()
          || ![27, 28].includes(sig.v) || !Number.isInteger(sig.n) || sig.n < 0 || sig.n > 0xffff_ffff
          || typeof sig.e !== 'string' || typeof o.m.ct !== 'string' || typeof o.m.co !== 'string'
          || (!sig.e || /[^0-9]/.test(sig.e)) || (!o.m.ct || /[^0-9]/.test(o.m.ct))
          || !/^[1-9]\d*$/.test(o.m.co) || BigInt(o.m.co) <= 0n || BigInt(o.m.co) > 0x7fff_ffff_ffff_ffffn
          || typeof o.b !== 'string' || o.b.toLowerCase() !== builder || o.bf !== '0.03')
          throw new Error('Invalid GRVT signed order fields');
        const created = BigInt(o.m.ct), expires = BigInt(sig.e), now = BigInt(Date.now()) * 1_000_000n;
        if (created < now - 30_000_000_000n || created > now + 30_000_000_000n
          || expires < now + 3_570_000_000_000n || expires > now + 3_630_000_000_000n)
          throw new Error('Invalid GRVT signature clock');
        const { Signature, parseUnits, verifyTypedData } = initializeCrypto();
        const message = {
          subAccountID: o.sa, isMarket: o.im,
          timeInForce: { GOOD_TILL_TIME: 1, ALL_OR_NONE: 2, IMMEDIATE_OR_CANCEL: 3, FILL_OR_KILL: 4 }[o.ti],
          postOnly: o.po, reduceOnly: o.ro,
          legs: o.l.map((leg) => {
            const instrument = instruments[leg.i];
            if (!instrument || typeof leg.s !== 'string' || typeof leg.ib !== 'boolean'
              || (o.im ? leg.lp !== undefined : typeof leg.lp !== 'string')) throw new Error('Invalid GRVT signed leg');
            const size = parseUnits(leg.s, instrument.decimals), price = o.im ? 0n : parseUnits(leg.lp, 9);
            if (size <= 0n || size > 0xffff_ffff_ffff_ffffn || (o.im ? price !== 0n : price <= 0n) || price > 0xffff_ffff_ffff_ffffn)
              throw new Error('Invalid GRVT native integer units');
            return { assetID: instrument.hash, contractSize: size, limitPrice: price, isBuyingContract: leg.ib };
          }),
          builder: o.b, builderFee: parseUnits(o.bf, 4), nonce: sig.n, expiration: sig.e,
        };
        const recovered = verifyTypedData({ name: 'GRVT Exchange', version: '0', chainId: 325 }, types, message,
          Signature.from({ r: sig.r, s: sig.s1, v: sig.v }));
        if (recovered.toLowerCase() !== signer.toLowerCase()) throw new Error('Invalid GRVT EIP-712 signature');
      },
      createRequestAudit() {
        // Credentials become valid only after the transport actually returns
        // a successful login. Requests alone cannot create or rotate a session.
        let current;
        const attempts = new Map();
        const fields = uniqueRequestHeaders;
        return {
          current: () => current && { ...current },
          validate({ url, headers, body }) {
            if (new URL(url).hostname !== 'trades.grvt.io') return;
            const h = fields(headers);
            if (!current || h['x-ts-grvt-session'] !== current.cookie || h['x-grvt-account-id'] !== current.account
              || (h.cookie !== undefined && !h.cookie.split(';').map((v) => v.trim()).includes(`gravity=${current.cookie}`)))
              throw new Error('GRVT request did not use the current authenticated session');
            if (new URL(url).pathname.endsWith('/create_order')) {
              const serialized = typeof body === 'string' ? body : JSON.stringify(body);
              const data = typeof body === 'string' ? parseRequestJson(body) : body;
              const previous = attempts.get(data?.o?.m?.co);
              if (previous && previous !== serialized) throw new Error('GRVT auth resend changed the signed order');
            }
          },
          response({ url, response, body: requestBody }) {
            const parsed = new URL(url);
            if (!['edge.grvt.io', 'trades.grvt.io'].includes(parsed.hostname)) return;
            let body;
            try { body = parseRequestJson(response.body); } catch { return; }
            const code = body?.c ?? body?.code ?? body?.e?.c ?? body?.e?.code ?? body?.error?.c ?? body?.error?.code;
            if (parsed.hostname === 'trades.grvt.io') {
              if (String(code) === '1000') {
                current = undefined;
                if (parsed.pathname.endsWith('/create_order')) {
                  const serialized = typeof requestBody === 'string' ? requestBody : JSON.stringify(requestBody);
                  const data = typeof requestBody === 'string' ? parseRequestJson(requestBody) : requestBody;
                  attempts.set(data?.o?.m?.co, serialized);
                }
              }
              return;
            }
            if (parsed.pathname !== '/auth/api_key/login' || response.status < 200 || response.status >= 300
              || (code !== undefined && String(code) !== '0')) return;
            const pairs = String(response.headers ?? '').split('\n').filter(Boolean).map((line) => {
              const colon = line.indexOf(':');
              return [line.slice(0, colon).trim(), line.slice(colon + 1).trim()];
            });
            const h = uniqueRequestHeaders(pairs.filter(([key]) => key.toLowerCase() !== 'set-cookie'));
            const cookieHeader = pairs.filter(([key]) => key.toLowerCase() === 'set-cookie')
              .map(([, value]) => value).find((value) => /(?:^|[,;]\s*)gravity=/.test(value));
            const cookie = h['x-ts-grvt-session'] ?? cookieHeader?.match(/(?:^|[,;]\s*)gravity=([^;,\s]+)/)?.[1];
            if (!cookie || !h['x-grvt-account-id']) {
              current = undefined;
              throw new Error('GRVT login response omitted authenticated session credentials');
            }
            current = { cookie, account: h['x-grvt-account-id'] };
          },
        };
      },
      createSocketAudit: (_impl, { isPrivate, books, requestAudit }) => {
        const states = new Map(), failures = [];
        const state = (url, generation) => {
          const key = generation && typeof generation === 'object' ? generation : `${url}#${generation ?? 1}`;
          if (!states.has(key)) states.set(key, { url, closed: false, authSent: false, ack: false, streams: new Set(), prefixCounts: new Map(), pings: 0, messages: 0 });
          return states.get(key);
        };
        const hostname = (current, url) => {
          if (typeof url !== 'string') return new URL(url).hostname;
          if (current.hostnameUrl !== url) {
            const value = new URL(url).hostname;
            current.hostnameUrl = url;
            current.hostname = value;
          }
          return current.hostname;
        };
        const subscription = (current, key, add) => {
          if (add ? current.streams.has(key) : !current.streams.delete(key)) return;
          if (add) current.streams.add(key);
          // Every @ boundary is a possible startsWith query, even inside an ID.
          for (let at = key.indexOf('@'); at !== -1; at = key.indexOf('@', at + 1)) {
            const prefix = key.slice(0, at + 1);
            const count = (current.prefixCounts.get(prefix) ?? 0) + (add ? 1 : -1);
            if (count) current.prefixCounts.set(prefix, count);
            else current.prefixCounts.delete(prefix);
          }
        };
        const fail = (message) => failures.push(message);
        return {
          connected(url, generation) { state(url, generation); },
          closed(url, generation) { state(url, generation).closed = true; },
          sent(url, text, generation) {
            const current = state(url, generation), privateSocket = hostname(current, url) === 'trades.grvt.io';
            const first = current.messages++ === 0;
            if (current.closed) fail('GRVT sent a frame on a closed socket generation');
            if (text === 'pong') {
              if (privateSocket && !current.authSent) fail('GRVT private first frame was not authentication');
              if (current.pings <= 0) fail('unexpected GRVT pong'); else current.pings--;
              return;
            }
            if (text === 'ping') {
              if (privateSocket && !current.authSent) fail('GRVT private first frame was not authentication');
              return;
            }
            const frame = JSON.parse(text);
            if (privateSocket && frame.type === 'auth') {
              const credentials = requestAudit?.current();
              const validCookie = credentials
                ? frame.session_cookie === `rm=true; gravity=${credentials.cookie}`
                : !requestAudit && ['rm=true; gravity=fixture-session-1', 'rm=true; gravity=fixture-session-2'].includes(frame.session_cookie);
              if (!first || current.authSent || !validCookie
                || frame.account_id !== (credentials?.account ?? account) || frame.testnet !== false)
                fail('invalid GRVT first-frame auth for socket generation');
              else current.authSent = true;
            } else if (privateSocket) {
              if (!current.ack || frame.method !== 'subscribe' || !['v1.order', 'v1.position', 'v1.fill'].includes(frame.stream)
                || JSON.stringify(frame.feed) !== '["777"]' || frame.is_full !== false)
                fail('GRVT private subscription preceded auth acknowledgement or changed selector');
              else current.streams.add(frame.stream);
            } else if (frame.method === 'subscribe') {
              for (const selector of frame.params?.selectors ?? []) subscription(current, `${frame.params.stream}:${selector}`, true);
            } else if (frame.method === 'unsubscribe') {
              for (const selector of frame.params?.selectors ?? []) subscription(current, `${frame.params.stream}:${selector}`, false);
            }
          },
          received(url, text, generation) {
            const current = state(url, generation);
            if (current.closed) fail('GRVT received a frame on a closed socket generation');
            if (text === 'ping') { current.pings++; return; }
            if (text === 'pong') return;
            const frame = JSON.parse(text), stream = frame.s ?? frame.stream;
            if (frame.type === 'auth_success') {
              if (!current.authSent || current.ack) fail('GRVT auth ACK preceded credentials or repeated on the same socket generation');
              else current.ack = true;
            }
            if (hostname(current, url) === 'trades.grvt.io' && stream && (frame.f ?? frame.feed)) {
              if (!current.ack || !current.streams.has(stream)) fail('unsubscribed GRVT private data: ' + stream);
              const feed = frame.f ?? frame.feed, selector = frame.s1 ?? frame.selector;
              if ((feed.sa !== undefined && feed.sa !== '777') || (selector && !selector.startsWith('777-')))
                fail('GRVT private data changed sub-account selector');
            }
            if (hostname(current, url) === 'market-data.grvt.io' && stream && (frame.f ?? frame.feed)) {
              const instrument = (frame.f ?? frame.feed).i ?? (frame.f ?? frame.feed).instrument;
              if (!current.prefixCounts.has(`${stream}:${instrument}@`))
                fail('unsubscribed GRVT public data: ' + stream + ':' + instrument);
            }
          },
          ready() { return !isPrivate || [...states.values()].some((s) => !s.closed && s.authSent); },
          errors() {
            for (const current of states.values()) {
              const { url } = current;
              if (current.pings) fail('missing GRVT pong');
              if (hostname(current, url) === 'trades.grvt.io' && isPrivate && !current.closed)
                for (const stream of ['v1.order', 'v1.position', 'v1.fill']) if (!current.streams.has(stream)) fail('missing GRVT private stream: ' + stream);
              if (hostname(current, url) === 'market-data.grvt.io' && !current.closed) for (const symbol of books) {
                const native = symbol.replace(/_PERP$/, '_Perp');
                if (!current.streams.has(`v1.book.s:${native}@500-50`)) fail('missing GRVT book subscription: ' + symbol);
              }
            }
            return failures;
          },
        };
      },
    };
  })(),
  coinbase: coinbase('coinbase'),
  coinbasespot: coinbase('coinbasespot'),
  polymarket,
  breakoutprop,
  phemex: {
    rust: 'phemex',
    requestKey: typedRequest,
    phemexProtocol: true,
    compareWriteOrder: true,
    tsReplaySettled: (exchange, count) => Object.values(exchange.publicWebsocket.orderBookInnerState).filter((s) => s.isSnapshotLoaded).length >= count,
    ts: { module: `${root}/phemex/phemex.exchange.ts`, className: 'PhemexExchange', name: 'phemex' },
    matchRequestBody: true,
    compareFillPrices: true,
    tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.topics).filter((t) => t.startsWith('orderbook_p.subscribe')).length >= count,
    isPrivateRecord: (r) => r.type === 'ws' && /"(accounts_p|positions_p|orders_p)"/.test(r.text),
    requestOptions: (method, url) => ({ generatedQueryId: (name, value) => method === 'PUT' && url.pathname === '/g-orders/create'
      && name === 'clOrdID' && typeof value === 'string' && value.startsWith('Tealstreet_') && value.length > 11 && value.length <= 32
      && !/[^A-Za-z0-9]/.test(value.slice(11)) }),
    bootstrapFrame: (r) => r.bootstrap === true,
    onSocketSend(socket, text) { if (JSON.parse(text).method === 'user.auth') socket.private = true; },
    socketMatches: (socket, text) => { const m = JSON.parse(text); return Boolean(socket.private) === (m.id === 999 || ['accounts_p','positions_p','orders_p'].some((field) => Object.hasOwn(m, field))); },
    pingReply(text) { const m = JSON.parse(text); return m.method === 'server.ping' ? JSON.stringify({ id: m.id, result: 'pong', error: null }) : null; },
    validateRequest({ url, body = '', headers = {} }) {
      const parsed = new URL(url);
      if (parsed.protocol !== 'https:' || !['api.phemex.com','testnet-api.phemex.com'].includes(parsed.hostname)) throw new Error('invalid Phemex upstream URL');
      const entries = (Array.isArray(headers) ? headers : headers instanceof Headers ? [...headers.entries()] : Object.entries(headers))
        .map(([key,value]) => [key.toLowerCase(), String(value)]);
      if (new Set(entries.map(([key]) => key)).size !== entries.length) throw new Error('duplicate Phemex request header');
      const h = Object.fromEntries(entries), expiry = h['x-phemex-request-expiry'];
      if (!expiry && !parsed.pathname.startsWith('/g-') && !parsed.pathname.startsWith('/api-data/g-futures/')) return;
      if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
      if (typeof body !== 'string') throw new Error('invalid Phemex request body');
      const query = decodeURIComponent(parsed.search.slice(1).replace(/\+/g, '%20'));
      const expected = createHmac('sha256', 'replay-secret').update(parsed.pathname + query + expiry + body).digest('hex');
      if (h['x-phemex-access-token'] !== 'replay-key' || h['x-phemex-request-signature'] !== expected || !/^[0-9]+$/.test(expiry ?? '')
        || !Number.isSafeInteger(Number(expiry)) || Math.abs(Number(expiry) - Date.now() / 1000 - 60) > 5) throw new Error(`invalid Phemex request: ${parsed.pathname}`);
    },
    createSocketAudit(_impl, { isPrivate, books }) {
      const failures = [], sockets = new Map();
      const socket = (url, generation = 1) => {
        const key = generation && typeof generation === 'object' ? generation : `${url} ${generation}`;
        if (!sockets.has(key)) sockets.set(key, { topics: new Set(), private: false, auth: false, acknowledged: false, subscribed: false, closed: false });
        return sockets.get(key);
      };
      return {
        connected(url, generation) { socket(url, generation); },
        closed(url, generation) { socket(url, generation).closed = true; },
        sent(url, text, generation) {
          const m = JSON.parse(text), state = socket(url, generation);
          if (m.method === 'user.auth') {
            state.private = true;
            state.acknowledged = false;
            state.subscribed = false;
            const [kind, key, sig, expiry] = m.params ?? [];
            state.auth = !state.closed && m.id === 999 && kind === 'API' && key === 'replay-key' && Number.isSafeInteger(expiry) && Math.abs(expiry - Date.now() / 1000 - 60) < 5 && sig === createHmac('sha256', 'replay-secret').update(key + expiry).digest('hex');
            if (!state.auth) failures.push('invalid Phemex WS auth');
          }
          if (m.method === 'orderbook_p.subscribe' && (m.params?.length !== 3 || m.params[1] !== false || m.params[2] !== 0)) failures.push('Phemex books require full depth0 at20ms');
          if (m.method === 'aop_p.subscribe') {
            state.private = true;
            if (state.closed || !state.acknowledged) failures.push('Phemex private subscribe preceded auth acknowledgement');
            else state.subscribed = true;
          }
          if (m.method?.endsWith('.subscribe')) state.topics.add(m.method + ':' + (m.params?.[0] ?? ''));
          if (m.method?.endsWith('.unsubscribe')) { state.topics.delete(m.method.replace(/\.unsubscribe$/, '.subscribe') + ':' + (m.params?.[0] ?? '')); if (m.method === 'aop_p.unsubscribe') state.subscribed = false; }
        },
        received(url, text, generation) {
          const m = JSON.parse(text), state = socket(url, generation);
          if (['accounts_p','positions_p','orders_p'].some((field) => Object.hasOwn(m, field)) && (state.closed || !state.acknowledged || !state.subscribed)) failures.push('Phemex private data preceded authenticated subscription');
          if (m.orderbook_p && !state.topics.has('orderbook_p.subscribe:' + m.symbol)) failures.push('Phemex book data arrived without subscription');
          if (m.method?.startsWith('perp_market24h_pack_p') && !state.topics.has('perp_market24h_pack_p.subscribe:')) failures.push('Phemex ticker data arrived without subscription');
          if (m.id !== 999 || state.closed) return;
          state.acknowledged = false;
          if (!state.auth) failures.push('Phemex auth ACK without request');
          else if (m.error != null || m.result?.status !== 'success') failures.push('Phemex auth ACK did not confirm success');
          else state.acknowledged = true;
        },
        ready() {
          const privateSockets = [...sockets.values()].filter((state) => state.private && !state.closed);
          const publicSockets = [...sockets.values()].filter((state) => !state.private && !state.closed);
          const hasTopic = (topic) => publicSockets.some((state) => state.topics.has(topic));
          return hasTopic('perp_market24h_pack_p.subscribe:') && books.every((s) => hasTopic('orderbook_p.subscribe:' + s)) && (!isPrivate || privateSockets.length > 0 && privateSockets.every((state) => state.acknowledged && state.subscribed));
        },
        errors() { return failures; },
      };
    },
  },


  paradex: {
    ...paradexProtocol,
    rust: 'paradex',
    requestKey: typedRequest,
    ts: { module: `${root}/paradex/paradex.exchange.ts`, className: 'ParadexExchange', name: 'paradex' },
    // Effective position leverage differs from configured cross-margin leverage.
    tsSettings: (exchange, store) => store.markets.map(({ symbol }) => ({
      symbol, marginType: 'cross', leverage: exchange.leverageSettings.get(symbol) ?? 10,
    })),
    tsBooksSubscribed: (exchange, count) => exchange.publicWebsocket.orderbookCallbacks.size >= count,
  },

  apexomni: {
    rust: 'apexomni',
    ts: { module: `${root}/apexomni/apexomni.exchange.ts`, className: 'ApexOmniExchange', name: 'apexomni' },
    privateSocket: '/realtime_private',
    replayCredentials: { omniKeySeed: APEX_SEED },
    publicOptions: { passphrase: '' },
    recordOptions: { publicDataOnly: true, key: '', secret: '' },
    prepareReplay: prepareApexReplay,
    prepareTs: prepareApexTs,
    wireBody: apexWireBody,
    requestKey: typedRequest,
    requestOptions: apexRequestOptions,
    matchRequestBody: true,
    generatedId: () => false,
    compareWriteOrder: true,
    compareFillPrices: true,
    compareClientIds: true,
    subscriptionGenerations: true,
    createSocketAudit: createApexSocketAudit,
    validateRequest: validateApexRequest,
    independentLeverage: true,
    tsSettings: (_exchange, store) => store.markets.map((market) => ({
      symbol: market.symbol,
      marginType: 'cross',
      buyLeverage: store.positions.find((position) => position.symbol === market.symbol && position.side === 'long')?.leverage ?? 0,
      sellLeverage: store.positions.find((position) => position.symbol === market.symbol && position.side === 'short')?.leverage ?? 0,
    })),
    tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderBook200.H.')).length >= count,
    isWrite: (method) => method !== 'GET',
  },
  kraken: {
    rust: 'kraken',
    compareBalanceCoins:true,
    productSubscriptions:true,
    tsBooksReady:(exchange,store)=>Boolean(store.loaded.tickers&&exchange.publicWebsocket),
    tsSettings:(exchange,store)=>store.markets.map(m=>{
      const position=store.positions.find(p=>p.symbol===m.symbol&&p.contracts>0);
      return {symbol:m.symbol,leverage:exchange.leverageSettings.get(m.symbol)??position?.leverage??m.limits.leverage.max,marginType:exchange.marginTypePreferences.get(m.symbol)??position?.marginType??'cross'};
    }),
    ts: {module: `${root}/kraken/kraken.exchange.ts`, className: 'KrakenExchange', name: 'kraken'},
    replayCredentials: {key:'replay-key',secret:'cmVwbGF5LXNlY3JldA=='},
    tsBooksSubscribed: (exchange,count) => Object.values(exchange.publicWebsocket.orderBookHandlers).filter(v=>v.length).length>=count,
    isPrivateRecord: record=>record.type==='ws'&&/"(?:fills|open_orders|open_positions|challenge)"/.test(record.text??''),
    initializeReplay(impl){krakenRequestAudit=createKrakenRequestAudit({strict:impl==='rust'});},
    validateRequest: request=>krakenRequestAudit(request),
    validateSocketMessage:validateKrakenSocketMessage,
    validateSocketFrame:validateKrakenSocketFrame,
    requestBody:krakenRequestBody,
    requestKey:typedRequest,
    commandResult:krakenCommandResult,
    requestBodies:true,
    compareFillPrices:true,
    compareWriteOrder:true,
    isWrite:method=>method!=='GET',
  },

  asterdex: {
    initializeReplay: initializeAsterCrypto,
    rust: 'asterdex',
    ts: { module: `${root}/asterdex/asterdex.exchange.ts`, className: 'AsterDEXExchange', name: 'asterdex' },
    privateSocket: '/ws/',
    compareFillPrices: true,
    compareBalanceCurrencies: true,
    compareWriteOrder: true,
    requestUrl: asterRequestUrl,
    // Aster does not mint client IDs: injected IDs remain part of exact identity.
    generatedId: () => false,
    requestKey: typedRequest,
    createRequestValidator: createAsterRequestValidator,
    createSocketAudit: asterSocketAudit,
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
    tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.topics).filter((topic) => topic.endsWith('@depth20@100ms')).length >= count,
    startupSubscriptions: (records, books) => {
      const url = records.find((record) => record.type === 'ws' && new URL(record.url).pathname === '/stream')?.url ?? 'wss://fstream.asterdex.com/stream';
      return subscriptionKeys(url, { method: 'SUBSCRIBE', params: ['!ticker@arr', '!markPrice@arr@1s', ...books.map((symbol) => `${symbol.toLowerCase()}@depth20@100ms`)] });
    },
    prepareTsTransport: async (_exchange, adapter) => {
      const { default: axios } = await import('axios');
      axios.defaults.adapter = adapter;
      createRequire(import.meta.url)('axios').defaults.adapter = adapter;
    },
  },

  kucoin: {
    ...kucoin,
    requestKey: (method, url, body, callerIds) => requestKey(method, url, body, callerIds, kucoin),
  },
  ...Object.fromEntries([...Object.keys(ORDERLY_BROKERS), 'orderly'].map((name) => [name, orderly(name)])),
  ...Object.fromEntries(['lighter', 'lighter-robinhood'].map((name) => [name, lighter(name)])),
  woo: {
    rust: 'woo',
    initialBookSnapshots: true,
    wooProtocol: true,
    subscriptionGenerations: true,
    ts: { module: `${root}/woo/woo.exchange.ts`, className: 'WOOXExchange', name: 'woo' },
    privateSocket: '/v2/ws/private/',
    matchRequestBody: true,
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('@orderbookupdate')).length >= count,
    respondToPing: true,
    createSocketAudit: createWooSocketAudit,
    validateRequest: validateWooRequest,
    compareFillPrices: true,
  },
  bitrue: {
    rust: 'bitrue',
    bitrueProtocol: true,
    ts: { module: `${root}/bitrue/bitrue.exchange.ts`, className: 'BitrueExchange', name: 'bitrue' },
    privateSocket: '/stream?listenKey=',
    compareFillPrices: true,
    requestBodies: true,
    volatileBodyFields: true,
    // Creation mints IDs; cancellation must retain the exact native target.
    generatedId: () => false,
    requestOptions: (method, url) => ({
      // Match Bitrue's 1..31-character ID contract; malformed values must retain wire types.
      generatedBodyId: method === 'POST' && url.pathname === '/fapi/v2/order'
        ? (key, value) => key === 'clientOrderId' && typeof value === 'string' && value.length >= 1 && value.length <= 31 && !/[\r\n\u2028\u2029]/.test(value)
        : undefined,
    }),
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/listenKey'),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('_simple_depth_step0')).length >= count,
    validateRequest: ({ method, url, body, headers }) => {
      const parsed = new URL(url);
      if (!['fapi.bitrue.com', 'fapiws-auth.bitrue.com'].includes(parsed.hostname))
        throw new Error('Bitrue must emit a real upstream URL');
      const fields = Object.fromEntries(Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]));
      if (!fields['x-ch-apikey'] && method === 'GET' && parsed.hostname === 'fapi.bitrue.com' && parsed.pathname === '/fapi/v1/contracts') return;
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
      const state = (url, socket) => {
        const identity = socket ?? url;
        if (!states.has(identity)) states.set(identity, { url, socket, channels: new Set(), pings: new Set() });
        return states.get(identity);
      };
      return {
        received(url, text, socket) {
          const frame = JSON.parse(text);
          const current = state(url, socket);
          if (frame.ping !== undefined) current.pings.add(JSON.stringify({ pong: frame.ping }));
          else if (frame.event === 'ping') current.pings.add(JSON.stringify(frame.ts === undefined ? { event: 'pong' } : { event: 'pong', ts: frame.ts }));
          if (frame.channel && (frame.tick || frame.data) && frame.event_rep !== 'rep' && !current.channels.has(frame.channel))
            failures.push('data arrived without subscription: ' + frame.channel);
        },
        sent(url, text, socket) {
          const current = state(url, socket);
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
          return !isPrivate || [...states.values()].some((s) => !s.socket?.closed && s.url.startsWith('wss://fapiws.bitrue.com/stream?listenKey=') && s.channels.has('user_account_update'));
        },
        errors() {
          const publicChannels = new Map();
          for (const s of states.values()) {
            if (s.pings.size) failures.push('missing Bitrue heartbeat reply');
            if (s.url.includes('fmarket-ws')) {
              if (!publicChannels.has(s.url)) publicChannels.set(s.url, new Set());
              for (const channel of s.channels) publicChannels.get(s.url).add(channel);
            }
          }
          for (const channels of publicChannels.values()) for (const symbol of books) {
            const native = 'e_' + symbol.replace(/^E-/, '').replaceAll('-', '').toLowerCase();
            if (!channels.has(`market_${native}_simple_depth_step0`)) failures.push('missing Bitrue depth subscription: ' + symbol);
          }
          return failures;
        },
      };
    },
  },

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
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(
        ([key, handlers]) => key.startsWith('orderBook_') && handlers.length,
      ).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  bitgetusdc: {
    rust: 'bitgetusdc',
    ts: { module: `${root}/bitget/bitgetusdc.exchange.ts`, className: 'BitgetUSDCExchange', name: 'bitgetusdc' },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(
        ([key, handlers]) => key.startsWith('orderBook_') && handlers.length,
      ).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  bitgetinverse: {
    inverseNativeAmounts: true,
    rust: 'bitgetinverse',
    ts: { module: `${root}/bitget/bitgetinverse.exchange.ts`, className: 'BitgetInverseExchange', name: 'bitgetinverse' },
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
      if (
        !validWeexTimestamp(String(timestamp), now) ||
        apiKey !== 'replay-key' ||
        passphrase !== 'replay-passphrase' ||
        typeof nonce !== 'string' ||
        !nonce.length
      )
        throw new Error('invalid BloFin socket login');
      const expected = Buffer.from(
        createHmac('sha256', 'replay-secret').update(`/users/self/verifyGET${timestamp}${nonce}`).digest('hex'),
      ).toString('base64');
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
    // Handler registration precedes socket open and REST depth completion.
    // Both implementations publish the initial REST seeds before legacy replay.
    initialBookSnapshots: true,
    mexcProtocol: true,
    requestBody: mexcRequestBody,
    requestUrl: mexcRequestUrl,
    generatedBodyId: (key) => key === 'externalOid',
    validateSocketMessage(socket, text, now = Date.now()) {
      const frame = JSON.parse(text);
      if (frame.method !== 'login') return;
      const { apiKey, reqTime, signature } = frame.param ?? {};
      if (
        !validWeexTimestamp(reqTime, now) ||
        apiKey !== 'replay-key' ||
        signature !== createHmac('sha256', 'replay-secret').update(`${apiKey}${reqTime}`).digest('hex')
      ) {
        throw new Error('invalid MEXC socket login');
      }
      socket.loginVerified = true;
    },
    validateSocketFrame(socket, text) {
      const frame = JSON.parse(text);
      if (frame.channel === 'rs.login' && frame.data === 'success' && !socket.loginVerified)
        throw new Error('MEXC login acknowledgement preceded valid login');
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
  ...Object.fromEntries([
    ['binance', 'BinanceExchange'], ['binancepm', 'BinancePMExchange'],
    ['binanceinverse', 'BinanceInverseExchange'], ['binancespot', 'BinanceSpotExchange'],
  ].map(([name, className]) => [name, {
    rust: name,
    family: 'binance',
    generatedId: () => false,
    preserveQueryFields: ['requestId'],
    requestOptions: binanceRequestOptions,
    compareWriteOrder: true,
    requestKey: typedRequest,
    validateRequest: validateBinanceRequest,
    subscriptionMessage: binanceSubscriptionMessage,
    recordTickerSymbols: binanceTickerSymbols,
    ts: { module: `${root}/binance/${name}.exchange.ts`, className, name },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >= count,
    privateSocket: name === 'binancespot' ? '/ws-api/v3' : name === 'binancepm' ? '/pm/ws' : name === 'binanceinverse' ? '/ws/' : '/private/ws',
    volatileBodyFields: true,
    ...(name === 'binanceinverse' ? { coinNativeAmounts: true } : {}),
    ...(name === 'binancespot' ? {
      compareFillPrices: true,
      compareCoins: true,
      compareBalanceCurrencies: true,
      // Wire currencies carry native numeric balances, without TS's opaque info.
      tsBalanceCurrencies: (currencies) => Object.fromEntries(Object.entries(currencies).map(([asset, row]) =>
        [asset, { free: row.free, used: row.used, total: row.total, upnl: row.upnl }])),
    } : {}),
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
    ...(name === 'binancespot' ? {
      bootstrapPrivateFromTape: true,
      socketRequest: spotSocketRequest,
      socketResponse: spotSocketResponse,
      createSocketAudit: createBinanceSpotSocketAudit,
      prepareTs: prepareBinanceSpot,
    } : {}),
  }])),
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
  [...new Set(Object.values(exchange).flatMap((value) => [value, value?.axiosInstance, ...Object.values(value && typeof value === 'object' ? value : {})]))]
    .filter((value) => value?.defaults && value?.interceptors);

/** Native Hyro callers supply endpoints; the harness uses the same public config as TS. */
export const configuredOptions = (name) => {
  if (name !== 'hyrotrader') return { ...venueOf(name).options };
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
