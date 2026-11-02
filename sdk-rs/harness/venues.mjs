// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHash, createHmac, createPrivateKey, createPublicKey, verify } from 'node:crypto';
import {
  COINBASE_REPLAY,
  verifyCoinbaseRequest,
  verifyCoinbaseSubscribe,
  coinbaseSubscription,
  createCoinbaseSocketAudit,
  installSynchronousEs256,
} from './coinbase-protocol.mjs';
import { installSynchronousDigest } from './sync-webcrypto.mjs';
import {
  APEX_SEED,
  apexWireBody,
  createApexSocketAudit,
  prepareApexReplay,
  prepareApexTs,
  validateApexRequest,
} from './apexomni-protocol.mjs';
import { createRequire } from 'node:module';
import { createBinanceSpotSocketAudit, prepareBinanceSpot, spotSocketRequest, spotSocketResponse, validateBinanceRequest, binanceSubscriptionMessage, binanceTickerSymbols } from './binance-protocol.mjs';
import { parseRequestJson } from './request-json.mjs';
import { hyperliquidspot } from './hyperliquidspot.mjs';
import { createHyperliquidRequestValidator } from './hyperliquid-protocol.mjs';
import { krakenspot } from './krakenspot.mjs';
import { kucoin } from './kucoin.mjs';
import { kucoinspot } from './kucoinspot.mjs';
import { breakoutprop } from './breakoutprop-requests.mjs';
import { polymarket } from './polymarket.mjs';
import { requestKey, subscriptionKeys } from './requests.mjs';
import {
  asterRequestUrl,
  asterSocketAudit,
  createAsterRequestValidator,
  initializeAsterCrypto,
} from './asterdex-requests.mjs';
import { paradexProtocol } from './paradex-protocol.mjs';
import { createWooSocketAudit, validateWooRequest } from './woo-protocol.mjs';
import { initializeLighterCrypto, createLighterVerifier, lighterRequestBody } from './lighter-crypto.mjs';
import { createLighterReplayRandom } from './lighter-replay-random.mjs';
import { initializeTxFlowCrypto, createTxFlowRequestValidator, validateTxFlowSocketMessage } from './txflow-protocol.mjs';

import {
  createKrakenRequestAudit,
  validateKrakenSocketMessage,
  validateKrakenSocketFrame,
  krakenRequestBody,
  krakenCommandResult,
} from './kraken-protocol.mjs';
let krakenRequestAudit;

// TS oracle adapters: test-only, never reachable from the safe-cex runtime.
const root = '../../packages/safe-cex/src/__tests__/reference';

const lighter = (name) => {
  let verifier;
  return {
    virtualPrivateClock: true,
    rust: name,
    ts: { module: `${root}/lighter/lighter.exchange.ts`, className: 'LighterExchange', name },
    replayCredentials: { applicationId: '42', apiKeyIndex: '3', secret: '01' + '00'.repeat(39), key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
    async initializeReplay(impl, options, { tapeSha256 } = {}) {
      if (options.publicDataOnly) return;
      const oracle = await initializeLighterCrypto(impl);
      // Install after crypto initialization, only in this private TS child.
      // The production generator and independent duplicate-ID verifier stay intact.
      if (impl === 'ts') Math.random = createLighterReplayRandom(tapeSha256);
      verifier = createLighterVerifier(options, oracle, name);
    },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('order_book:')).length >=
      count,
    isPrivateRecord: (record) => record.type === 'ws' && /account_all/.test(record.text ?? ''),
    isWrite: (method) => method !== 'GET',
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
    frameText: (text) => verifier?.echoCreated(text) ?? text,
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
  virtualPrivateClock: true,
  compareInstrumentClassification: true,
  compareOptionTerms: true,
  rust: name,
  subscriptionGenerations: true,
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
  generatedId: name === 'hyrotrader' ? (key) => key === 'orderLinkId' : undefined,
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
const orderlyNative = (symbol) =>
  symbol.replace(/^(.+)USDC(_[A-Za-z0-9_-]+)?$/, (_, base, suffix = '') => `PERP_${base}_USDC${suffix}`);
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
    virtualPrivateClock: true,
    waitForAllStageWrites: true,
    orderlyProtocol: true,
    subscriptionGenerations: true,
    publicTickers: (records) => {
      const seed = records.find((r) => r.type === 'http' && new URL(r.url).pathname === '/v1/public/info');
      return seed ? JSON.parse(seed.body).data.rows.map((row) => row.symbol) : [];
    },
    rust: alias,
    ts: {
      module: `${root}/orderly/exchanges/${name}/${name}.exchange.ts`,
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
            } else state.channels.delete(message.topic);
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
  virtualPrivateClock: true,
  requestKey: typedRequest,
  ts: {
    module: `${root}/okx/${name.startsWith('okxeu') ? 'okxeu' : 'okx'}.exchange.ts`,
    className: name.startsWith('okxeu') ? 'OKXEUExchange' : 'OKXExchange',
    name,
  },
  options: { testnet: name.endsWith('demo') },
  compareBalanceCurrencies: true,
  validateRequest({ method, url, body, headers }) {
    // Independent V5 HTTP proof before matching can erase clocks/aliases.
    const parsed = new URL(url), h = Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers ?? {}))
      .map(([key, value]) => [key.toLowerCase(), String(value)]));
    const host = name.startsWith('okxeu') ? 'eea.okx.com' : name.endsWith('demo') ? 'openapi.okx.com' : 'www.okx.com';
    if (parsed.protocol !== 'https:' || parsed.hostname !== host) throw new Error('Invalid OKX native HTTP host');
    const aliases = new Set(['c', 'clientOrderId', 'clientAlgoId', 'newClientOrderId', 'requestId', 'category', 'symbol']);
    for (const key of parsed.searchParams.keys()) if (aliases.has(key)) throw new Error(`Foreign OKX query alias ${key}`);
    if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
    const raw = body == null ? '' : typeof body === 'string' ? body : JSON.stringify(body);
    if (raw) {
      const rows = parseRequestJson(raw);
      const visit = (value) => {
        if (Array.isArray(value)) return value.forEach(visit);
        if (!value || typeof value !== 'object') return;
        for (const [key, child] of Object.entries(value)) {
          if (aliases.has(key)) throw new Error(`Foreign OKX body alias ${key}`);
          visit(child);
        }
      };
      visit(rows);
    }
    if (name.endsWith('demo') ? h['x-simulated-trading'] !== '1' : h['x-simulated-trading'] !== undefined)
      throw new Error('Invalid OKX simulated-trading header');
    const privatePath = /^\/api\/v5\/(account|trade)\//.test(parsed.pathname);
    if (!privatePath && !h['ok-access-sign']) return;
    const timestamp = h['ok-access-timestamp'], time = Date.parse(timestamp);
    if (!/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(timestamp ?? '') || !Number.isFinite(time)
      || Math.abs(time - Date.now()) > 30_000 || new Date(time).toISOString() !== timestamp
      || h['ok-access-key'] !== 'replay-key' || h['ok-access-passphrase'] !== 'replay-passphrase')
      throw new Error('Invalid OKX signed HTTP credentials or timestamp');
    const expected = createHmac('sha256', 'replay-secret').update(`${timestamp}${method.toUpperCase()}${parsed.pathname}${parsed.search}${raw}`).digest('base64');
    if (h['ok-access-sign'] !== expected) throw new Error('Invalid OKX signed HTTP signature');
  },
  prepareTs(exchange) {
    const fetchMarkets = exchange.fetchMarkets.bind(exchange);
    exchange.fetchMarkets = async () => (await fetchMarkets()).map((market) => {
      if (Number.isFinite(market.limits.amount.max)) return market;
      // KNOWN_TS_FIXES: preopen instruments omit unavailable order-type caps;
      // Math.min(parseFloat('')) made their maximum NaN in TS. Use only raw
      // positive native caps; malformed supplied values remain an error/gap.
      const caps = ['maxIcebergSz', 'maxLmtSz', 'maxMktSz', 'maxStopSz', 'maxTriggerSz', 'maxTwapSz']
        .map((key) => market.info?.[key]).filter((value) => value !== undefined && value !== '');
      if (!caps.length || caps.some((value) => typeof value !== 'string' || value.trim() !== value || !Number.isFinite(Number(value)) || Number(value) <= 0)) return market;
      return { ...market, limits: { ...market.limits, amount: { ...market.limits.amount, max: Math.min(...caps.map(Number)) } } };
    });
    if (exchange.parseTickers) {
      const parseTickers = exchange.parseTickers.bind(exchange);
      exchange.parseTickers = (raw) => parseTickers(raw).map((ticker) => {
        const row = raw.find((row) => row.instId === ticker.id);
        if (!row || (row.last !== '' && row.open24h !== '')
          || [row.last, row.open24h].some((value) => value !== '' && !Number.isFinite(Number(value)))) return ticker;
        // KNOWN_TS_FIXES: an untraded X-Perp has no last/open quote yet.
        const last = Number(row.last), open = Number(row.open24h);
        return { ...ticker, ...(row.last === '' ? { last: 0, mark: 0, index: 0 } : {}),
          percentage: open === 0 ? 0 : Math.round((last - open) / open * 10000) / 100 };
      });
    }
    if (exchange.privateWebsocket) {
      // KNOWN_TS_FIXES: pushes are partial position updates. Preserve full-account
      // initial margin, coin valuation and live order reserve across push/tick paths.
      const round = (value) => Math.round(value * 100) / 100;
      const finite = (value) => Number.isFinite(Number(value)) ? Number(value) : 0;
      const nativeRisk = (raw) => {
        const absent = (value) => value === undefined || value === '';
        const read = (field) => {
          const value = raw[field];
          if (typeof value !== 'string' || !/^[+-]?(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(value)
            || !Number.isFinite(Number(value)) || (field !== 'upl' && Number(value) < 0))
            throw new Error(`Invalid OKX reference native risk ${raw.instId}.${field}`);
          return Number(value);
        };
        // The production converter coerces nonfinite amounts to zero. Refuse
        // unproven raw risk before it or any real handler can hide that failure.
        if (!absent(raw.imr)) read('imr');
        if (!absent(raw.upl)) read('upl');
        if (Number(raw.pos) === 0) return { imr: 0, upl: 0 };
        return { imr: read(absent(raw.imr) && raw.mgnMode === 'isolated' ? 'margin' : 'imr'), upl: read('upl') };
      };
      const usdRisk = (raw) => {
        const risk = nativeRisk(raw), market = exchange.store.marketsById[raw.instId];
        const ticker = exchange.store.tickersBySymbol[market?.symbol];
        const reference = finite(ticker?.mark) || finite(raw.markPx) || finite(raw.avgPx);
        const imr = exchange.convertMarginValueToUsd(risk.imr, raw.ccy || market?.info?.settleCcy, market, reference);
        const upl = exchange.convertMarginValueToUsd(risk.upl, raw.ccy || market?.info?.settleCcy, market, reference);
        if (!Number.isFinite(imr) || !Number.isFinite(upl)) throw new Error(`Invalid OKX reference converted risk ${raw.instId}`);
        return { imr, upl };
      };
      const validatePositions = (positions) => {
        for (const position of positions ?? []) if (position.info) usdRisk(position.info);
      };
      let authoritativeEquity, cachedInfo;
      const reserve = () => round(exchange.store.orders.filter((order) => order.info && order.status === 'open' && order.type === 'limit' && !order.reduceOnly)
        .reduce((sum, order) => sum + order.remaining * order.price / Math.max(finite(order.info.lever || '1'), 1), 0) * 1.1);
      const balanceFromRows = (equity) => {
        let used = 0, upnl = 0;
        for (const position of exchange.store.positions.filter((row) => row.contracts > 0 && row.info)) {
          const risk = usdRisk(position.info);
          used += risk.imr;
          upnl += risk.upl;
        }
        used = round(used); upnl = round(upnl);
        return { used, upnl, free: round(equity - used - reserve()), total: round(equity - upnl) };
      };
      const positionTopic = exchange.privateWebsocket.handlePositionTopic.bind(exchange.privateWebsocket);
      exchange.privateWebsocket.handlePositionTopic = (message) => {
        for (const row of message.data ?? []) usdRisk(row);
        validatePositions(exchange.store.positions);
        const equity = authoritativeEquity ?? exchange.store.balance.total + exchange.store.balance.upnl;
        positionTopic(message);
        exchange.store.update({ balance: { ...exchange.store.balance, ...balanceFromRows(equity) } });
      };
      const accountTopic = exchange.privateWebsocket.handleAccountTopic.bind(exchange.privateWebsocket);
      exchange.privateWebsocket.handleAccountTopic = (message) => {
        validatePositions(exchange.store.positions);
        const revision = exchange.accountEquityRevision;
        accountTopic(message);
        // A rejected production account message cannot acquire authority in the reference.
        if (revision !== undefined && revision === exchange.accountEquityRevision) return;
        const values = (message.data ?? []).map((row) => Number.parseFloat(String(row.totalEq ?? ''))).filter(Number.isFinite);
        if (!values.length) return;
        authoritativeEquity = round(values.reduce((sum, value) => sum + value, 0));
        cachedInfo = exchange.store.balance.info;
        exchange.store.update({ balance: { ...exchange.store.balance, ...balanceFromRows(authoritativeEquity) } });
      };
      const formatter = exchange.formatBalanceUsedAvail.bind(exchange);
      exchange.formatBalanceUsedAvail = async (...args) => {
        validatePositions(args[1]);
        const revision = exchange.accountEquityRevision;
        const balance = await formatter(...args);
        // Preserve the production fence when an account push wins during the await.
        if (revision !== exchange.accountEquityRevision) return balance;
        if (authoritativeEquity === undefined) return balance;
        // Only the cached raw snapshot superseded by that push is stale. A new
        // REST object has independent provenance; never freeze its fresh equity.
        if (args[0] === cachedInfo) return { ...balance, ...balanceFromRows(authoritativeEquity) };
        authoritativeEquity = round(balance.total + balance.upnl);
        cachedInfo = args[0];
        return balance;
      };
    }
    // KNOWN_TS_FIXES: preserve production paging and mappers, route each native
    // product explicitly instead of the generic histories' hardcoded SWAP.
    let source;
    const get = exchange.xhr.get.bind(exchange.xhr);
    exchange.xhr.get = (url, config = {}) => {
      if (/\/(orders-history|bills(?:-archive)?)$/.test(url)) {
        const market = exchange.store.marketsById[config.params?.instId];
        config = { ...config, params: { ...config.params,
          instType: market ? exchange.getMarketInstrumentType(market) : source ?? 'SWAP',
        } };
      }
      return get(url, config);
    };
    for (const method of ['fetchHistoricalOrders', 'fetchIncomeHistory']) {
      const original = exchange[method].bind(exchange);
      exchange[method] = async (options = {}) => {
        if (!name.startsWith('okxeu') || options.symbol) return original(options);
        const rows = [];
        try {
          for (source of ['SWAP', 'FUTURES']) rows.push(...await original(options));
        } finally { source = undefined; }
        const income = method === 'fetchIncomeHistory';
        const unique = [...new Map(rows.map((row) => [income ? row.tranId : row.id, row])).values()]
          .sort((a, b) => (income ? b.timestamp - a.timestamp : b.createdTime - a.createdTime));
        if (!(options.limit > 0)) return unique;
        if (!income) return unique.slice(0, options.limit);
        // One native bill decomposes to fee/PnL siblings; limit whole bills.
        const bills = new Set();
        return unique.filter((row) => {
          const bill = row.tranId.split(':')[0];
          if (!bills.has(bill) && bills.size >= options.limit) return false;
          bills.add(bill); return true;
        });
      };
    }
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
  currencyBalances: true,
  rust: name,
  ts: {
    module: `${root}/coinbase/${name}.exchange.ts`,
    className: name === 'coinbasespot' ? 'CoinbaseSpotExchange' : 'CoinbaseExchange',
    name,
  },
  publicOptions: { spotOnly: true, extra: { coinbasePublicSpot: true } },
  options: { testnet: false, ...(name === 'coinbasespot' ? { spotOnly: true } : {}) },
  replayCredentials: COINBASE_REPLAY,
  privateSocket: 'advanced-trade-ws-user.coinbase.com',
  // Public startup schedules ticker polling before replay begins. Registering
  // books before socket open adds a 100 ms retry that otherwise shifts that
  // poll's phase depending on audit cost. Use the same completed seed/open
  // boundary in both modes; private startup keeps its existing readiness.
  tsBooksReady: (exchange, store) => !exchange.isPublicDataOnly
    || Boolean(store.loaded.markets && store.loaded.tickers && exchange.publicWebsocket?.isConnected),
  tsBooksSubscribed: (exchange, count) =>
    Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('level2.')).length >= count,
  verifyRequest: verifyCoinbaseRequest,
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
  // Both order censuses run on timers (TS write reconcile and tick, Rust tick and
  // REST pacing). Fire them at tape time, or a checkpoint races a census in flight.
  virtualPrivateClock: true,
  // Native REST pacing stores an absolute next-send time; startup's paced reads
  // must not leave it seconds ahead of a rewound clock.
  monotonicReplayClock: true,
  loadTs: ({ isPrivate }) => { if (isPrivate) installSynchronousEs256(); },
  isWrite: (method) => method !== 'GET',
  generatedBodyId: (key, value) =>
    key === 'client_order_id' && typeof value === 'string' && value.startsWith('tealstreet-'),
});

const headerValue = (headers, name) => (Array.isArray(headers) ? headers : Object.entries(headers ?? {}))
  .find(([key]) => key.toLowerCase() === name)?.[1];
function validBitunixSignature(key, nonce, timestamp, sign, query, body) {
  const hash = (value) => createHash('sha256').update(value).digest('hex');
  return key === 'replay-key' && typeof nonce === 'string' && nonce.length > 0
    && /^\d+$/.test(String(timestamp)) && Number.isSafeInteger(Number(timestamp))
    && sign === hash(hash(`${nonce}${timestamp}${key}${query}${body ?? ''}`) + 'replay-secret');
}

export const VENUES = {
  txflow: {
    rust: 'txflow',
    independentLeverage: true,
    options: { testnet: true },
    virtualPrivateClock: true,
    recordPublicCorsDirect: true,
    recordAfterReady: true,
    ts: { module: `${root}/txflow/txflow.exchange.ts`, className: 'TxFlowExchange', name: 'txflow' },
    requestKey: typedRequest,
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
  breakoutprop,
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
    const attempts = new Map();
    return {
      virtualPrivateClock: true,
      rust: 'grvt',
      initializeReplay(_impl, options) {
        if (!options.publicDataOnly) initializeCrypto();
      },
      ts: { module: `${root}/grvt/grvt.exchange.ts`, className: 'GrvtExchange', name: 'grvt' },
      tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((k) => k.startsWith('orderbook.')).length >= count,
      options: { grvtSessionProxy: true, grvtTradingAccountId: '777' },
      replayCredentials: { key: 'grvt-replay-key', secret },
      requestBodies: true,
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
        const h = Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers ?? {})).map(([k, v]) => [k.toLowerCase(), String(v)]));
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
          || !/^\d+$/.test(sig.e) || !/^\d+$/.test(o.m.ct)
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
        const serialized = typeof body === 'string' ? body : JSON.stringify(body);
        const previous = attempts.get(o.m.co);
        if (previous && previous.body !== serialized) throw new Error('GRVT auth resend changed the signed order');
        attempts.set(o.m.co, { body: serialized });
      },
      createRequestAudit() {
        // Credentials become valid only after the transport actually returns
        // a successful login. Requests alone cannot create or rotate a session.
        let current;
        const fields = (headers) => Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers ?? {}))
          .map(([key, value]) => [key.toLowerCase(), String(value)]));
        return {
          current: () => current && { ...current },
          validate({ url, headers }) {
            if (new URL(url).hostname !== 'trades.grvt.io') return;
            const h = fields(headers);
            if (!current || h['x-ts-grvt-session'] !== current.cookie || h['x-grvt-account-id'] !== current.account
              || (h.cookie !== undefined && !h.cookie.split(';').map((v) => v.trim()).includes(`gravity=${current.cookie}`)))
              throw new Error('GRVT request did not use the current authenticated session');
          },
          response({ url, response }) {
            const parsed = new URL(url);
            if (!['edge.grvt.io', 'trades.grvt.io'].includes(parsed.hostname)) return;
            let body;
            try { body = parseRequestJson(response.body); } catch { return; }
            const code = body?.c ?? body?.code ?? body?.e?.c ?? body?.e?.code ?? body?.error?.c ?? body?.error?.code;
            if (parsed.hostname === 'trades.grvt.io') {
              if (String(code) === '1000') current = undefined;
              return;
            }
            if (parsed.pathname !== '/auth/api_key/login' || response.status < 200 || response.status >= 300
              || (code !== undefined && String(code) !== '0')) return;
            const pairs = String(response.headers ?? '').split('\n').filter(Boolean).map((line) => {
              const colon = line.indexOf(':');
              return [line.slice(0, colon).trim(), line.slice(colon + 1).trim()];
            });
            const h = fields(pairs);
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
          const key = `${url}#${generation ?? 1}`;
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
  phemex: {
    virtualPrivateClock: true,
    rust: 'phemex',
    phemexProtocol: true,
    tsReplaySettled: (exchange, count) =>
      Object.values(exchange.publicWebsocket.orderBookInnerState).filter((s) => s.isSnapshotLoaded).length >= count,
    ts: { module: `${root}/phemex/phemex.exchange.ts`, className: 'PhemexExchange', name: 'phemex' },
    matchRequestBody: true,
    compareFillPrices: true,
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.topics).filter((t) => t.startsWith('orderbook_p.subscribe')).length >= count,
    isPrivateRecord: (r) => r.type === 'ws' && /"(accounts_p|positions_p|orders_p)"/.test(r.text),
    generatedQueryId: (name, value) => name === 'clOrdID' && value.startsWith('Tealstreet_'),
    bootstrapFrame: (r) => r.bootstrap === true,
    onSocketSend(socket, text) {
      if (JSON.parse(text).method === 'user.auth') socket.private = true;
    },
    socketMatches: (socket, text) =>
      Boolean(socket.private) === (JSON.parse(text).id === 999 || /"(accounts_p|positions_p|orders_p)"/.test(text)),
    pingReply(text) {
      const m = JSON.parse(text);
      return m.method === 'server.ping' ? JSON.stringify({ id: m.id, result: 'pong', error: null }) : null;
    },
    validateRequest({ url, body = '', headers = {} }) {
      const parsed = new URL(url);
      const get = (name) => Object.entries(headers).find(([key]) => key.toLowerCase() === name)?.[1];
      const expiry = get('x-phemex-request-expiry');
      if (!expiry && !parsed.pathname.startsWith('/g-') && !parsed.pathname.startsWith('/api-data/')) return;
      const query = decodeURIComponent(parsed.search.slice(1).replace(/\+/g, '%20'));
      const expected = createHmac('sha256', 'replay-secret')
        .update(parsed.pathname + query + expiry + (body || ''))
        .digest('hex');
      if (
        get('x-phemex-access-token') !== 'replay-key' ||
        get('x-phemex-request-signature') !== expected ||
        !Number.isSafeInteger(Number(expiry)) ||
        Math.abs(Number(expiry) - Date.now() / 1000 - 60) > 5
      )
        throw new Error(`invalid Phemex request: ${parsed.pathname}`);
    },
    createSocketAudit(_impl, { isPrivate, books }) {
      const failures = [],
        topics = new Set(),
        sockets = new Map();
      const socket = (url, generation = 1) => {
        const key = `${url} ${generation}`;
        if (!sockets.has(key))
          sockets.set(key, { private: false, auth: false, acknowledged: false, subscribed: false, closed: false });
        return sockets.get(key);
      };
      return {
        connected(url, generation) {
          socket(url, generation);
        },
        closed(url, generation) {
          socket(url, generation).closed = true;
        },
        sent(url, text, generation) {
          const m = JSON.parse(text),
            state = socket(url, generation);
          if (m.method === 'user.auth') {
            state.private = true;
            state.acknowledged = false;
            state.subscribed = false;
            const [kind, key, sig, expiry] = m.params ?? [];
            state.auth =
              !state.closed &&
              m.id === 999 &&
              kind === 'API' &&
              key === 'replay-key' &&
              Number.isSafeInteger(expiry) &&
              Math.abs(expiry - Date.now() / 1000 - 60) < 5 &&
              sig ===
                createHmac('sha256', 'replay-secret')
                  .update(key + expiry)
                  .digest('hex');
            if (!state.auth) failures.push('invalid Phemex WS auth');
          }
          if (
            m.method === 'orderbook_p.subscribe' &&
            (m.params?.length !== 3 || m.params[1] !== false || m.params[2] !== 0)
          )
            failures.push('Phemex books require full depth0 at20ms');
          if (m.method === 'aop_p.subscribe') {
            state.private = true;
            if (state.closed || !state.acknowledged)
              failures.push('Phemex private subscribe preceded auth acknowledgement');
            else state.subscribed = true;
          }
          if (m.method?.endsWith('.subscribe')) topics.add(m.method + ':' + (m.params?.[0] ?? ''));
        },
        received(url, text, generation) {
          const m = JSON.parse(text),
            state = socket(url, generation);
          if (m.id !== 999 || state.closed) return;
          state.acknowledged = false;
          if (!state.auth) failures.push('Phemex auth ACK without request');
          else if (m.error != null || m.result?.status !== 'success')
            failures.push('Phemex auth ACK did not confirm success');
          else state.acknowledged = true;
        },
        ready() {
          const privateSockets = [...sockets.values()].filter((state) => state.private && !state.closed);
          return (
            topics.has('perp_market24h_pack_p.subscribe:') &&
            books.every((s) => topics.has('orderbook_p.subscribe:' + s)) &&
            (!isPrivate ||
              (privateSockets.length > 0 && privateSockets.every((state) => state.acknowledged && state.subscribed)))
          );
        },
        errors() {
          return failures;
        },
      };
    },
  },

  paradex: {
    ...paradexProtocol,
    rust: 'paradex',
    // Order refresh debounces and REST reconciliation run on timers; at wall time a
    // checkpoint races them (canceled-row retention at checkpoints 5-7).
    virtualPrivateClock: true,
    ts: { module: `${root}/paradex/paradex.exchange.ts`, className: 'ParadexExchange', name: 'paradex' },
    // Effective position leverage differs from configured cross-margin leverage.
    tsSettings: (exchange, store) =>
      store.markets.map(({ symbol }) => ({
        symbol,
        marginType: 'cross',
        leverage: exchange.leverageSettings.get(symbol) ?? 10,
      })),
    tsBooksSubscribed: (exchange, count) => exchange.publicWebsocket.orderbookCallbacks.size >= count,
  },
  apexomni: {
    rust: 'apexomni',
    virtualPrivateClock: true,
    ts: { module: `${root}/apexomni/apexomni.exchange.ts`, className: 'ApexOmniExchange', name: 'apexomni' },
    privateSocket: '/realtime_private',
    replayCredentials: { omniKeySeed: APEX_SEED },
    publicOptions: { passphrase: '' },
    recordOptions: { publicDataOnly: true, key: '', secret: '' },
    prepareReplay: prepareApexReplay,
    prepareTs: prepareApexTs,
    wireBody: apexWireBody,
    matchRequestBody: true,
    generatedId: () => false,
    generatedBodyId: (key) =>
      [
        'clientId',
        'clientOrderId',
        'tpClientOrderId',
        'slClientOrderId',
        'expiration',
        'tpExpiration',
        'slExpiration',
        'tpSignature',
        'slSignature',
      ].includes(key),
    compareWriteOrder: true,
    compareFillPrices: true,
    compareClientIds: true,
    subscriptionGenerations: true,
    createSocketAudit: createApexSocketAudit,
    validateRequest: validateApexRequest,
    independentLeverage: true,
    tsSettings: (_exchange, store) =>
      store.markets.map((market) => ({
        symbol: market.symbol,
        marginType: 'cross',
        buyLeverage:
          store.positions.find((position) => position.symbol === market.symbol && position.side === 'long')?.leverage ??
          0,
        sellLeverage:
          store.positions.find((position) => position.symbol === market.symbol && position.side === 'short')
            ?.leverage ?? 0,
      })),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('orderBook200.H.')).length >=
      count,
    isWrite: (method) => method !== 'GET',
  },
  kraken: {
    rust: 'kraken',
    virtualPrivateClock: true,
    compareBalanceCoins: true,
    productSubscriptions: true,
    tsBooksReady: (exchange, store) => Boolean(store.loaded.tickers && exchange.publicWebsocket),
    tsSettings: (exchange, store) =>
      store.markets.map((m) => {
        const position = store.positions.find((p) => p.symbol === m.symbol && p.contracts > 0);
        return {
          symbol: m.symbol,
          leverage: exchange.leverageSettings.get(m.symbol) ?? position?.leverage ?? m.limits.leverage.max,
          marginType: exchange.marginTypePreferences.get(m.symbol) ?? position?.marginType ?? 'cross',
        };
      }),
    ts: { module: `${root}/kraken/kraken.exchange.ts`, className: 'KrakenExchange', name: 'kraken' },
    replayCredentials: { key: 'replay-key', secret: 'cmVwbGF5LXNlY3JldA==' },
    tsBooksSubscribed: (exchange, count) =>
      Object.values(exchange.publicWebsocket.orderBookHandlers).filter((v) => v.length).length >= count,
    isPrivateRecord: (record) =>
      record.type === 'ws' && /"(?:fills|open_orders|open_positions|challenge)"/.test(record.text ?? ''),
    initializeReplay(impl) {
      krakenRequestAudit = createKrakenRequestAudit({ strict: impl === 'rust' });
    },
    validateRequest: (request) => krakenRequestAudit(request),
    validateSocketMessage: validateKrakenSocketMessage,
    validateSocketFrame: validateKrakenSocketFrame,
    requestBody: krakenRequestBody,
    commandResult: krakenCommandResult,
    requestBodies: true,
    compareFillPrices: true,
    compareWriteOrder: true,
    isWrite: (method) => method !== 'GET',
  },
  asterdex: {
    initializeReplay: initializeAsterCrypto,
    rust: 'asterdex',
    virtualPrivateClock: true,
    ts: { module: `${root}/asterdex/asterdex.exchange.ts`, className: 'AsterDEXExchange', name: 'asterdex' },
    privateSocket: '/ws/',
    compareFillPrices: true,
    compareBalanceCurrencies: true,
    compareWriteOrder: true,
    requestUrl: asterRequestUrl,
    createRequestValidator: createAsterRequestValidator,
    createSocketAudit: asterSocketAudit,
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.topics).filter((topic) => topic.endsWith('@depth20@100ms')).length >= count,
    startupSubscriptions: (records, books) => {
      const url =
        records.find((record) => record.type === 'ws' && new URL(record.url).pathname === '/stream')?.url ??
        'wss://fstream.asterdex.com/stream';
      return subscriptionKeys(url, {
        method: 'SUBSCRIBE',
        params: ['!ticker@arr', '!markPrice@arr@1s', ...books.map((symbol) => `${symbol.toLowerCase()}@depth20@100ms`)],
      });
    },
    prepareTsTransport: async (_exchange, adapter) => {
      const { default: axios } = await import('axios');
      axios.defaults.adapter = adapter;
      createRequire(import.meta.url)('axios').defaults.adapter = adapter;
    },
  },
  ...Object.fromEntries([...Object.keys(ORDERLY_BROKERS), 'orderly'].map((name) => [name, orderly(name)])),
  ...Object.fromEntries(['lighter', 'lighter-robinhood'].map((name) => [name, lighter(name)])),
  woo: {
    rust: 'woo',
    virtualPrivateClock: true,
    wooProtocol: true,
    subscriptionGenerations: true,
    ts: { module: `${root}/woo/woo.exchange.ts`, className: 'WOOXExchange', name: 'woo' },
    privateSocket: '/v2/ws/private/',
    matchRequestBody: true,
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('@orderbookupdate')).length >=
      count,
    respondToPing: true,
    createSocketAudit: createWooSocketAudit,
    validateRequest: validateWooRequest,
    compareFillPrices: true,
  },
  bitrue: {
    virtualPrivateClock: true,
    rust: 'bitrue',
    bitrueProtocol: true,
    compareBalanceCurrencies: true,
    ts: { module: `${root}/bitrue/bitrue.exchange.ts`, className: 'BitrueExchange', name: 'bitrue' },
    privateSocket: '/stream?listenKey=',
    compareFillPrices: true,
    requestBodies: true,
    volatileBodyFields: true,
    // Creation mints IDs; cancellation must retain the exact native target.
    generatedId: () => false,
    requestOptions: (method, url) => ({
      generatedBodyId:
        method === 'POST' && url.pathname === '/fapi/v2/order' ? (key) => key === 'clientOrderId' : undefined,
    }),
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.includes('/listenKey'),
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.endsWith('_simple_depth_step0'))
        .length >= count,
    validateRequest: ({ method, url, body, headers }) => {
      const parsed = new URL(url);
      if (!['fapi.bitrue.com', 'fapiws-auth.bitrue.com'].includes(parsed.hostname))
        throw new Error('Bitrue must emit a real upstream URL');
      const fields = Object.fromEntries(
        Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]),
      );
      if (
        !fields['x-ch-apikey'] &&
        method === 'GET' &&
        parsed.hostname === 'fapi.bitrue.com' &&
        parsed.pathname === '/fapi/v1/contracts'
      )
        return;
      if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
      const timestamp = fields['x-ch-ts'];
      const payload = timestamp + method + parsed.pathname + parsed.search + (body ?? '');
      if (
        fields['x-ch-apikey'] !== 'replay-key' ||
        !/^\d+$/.test(timestamp) ||
        Math.abs(Date.now() - Number(timestamp)) > 60_000 ||
        fields['x-ch-sign'] !== createHmac('sha256', 'replay-secret').update(payload).digest('hex')
      )
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
          else if (frame.event === 'ping')
            current.pings.add(
              JSON.stringify(frame.ts === undefined ? { event: 'pong' } : { event: 'pong', ts: frame.ts }),
            );
          if (
            frame.channel &&
            (frame.tick || frame.data) &&
            frame.event_rep !== 'rep' &&
            !current.channels.has(frame.channel)
          )
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
          return (
            !isPrivate ||
            [...states].some(
              ([url, s]) =>
                url.startsWith('wss://fapiws.bitrue.com/stream?listenKey=') && s.channels.has('user_account_update'),
            )
          );
        },
        errors() {
          for (const [url, s] of states) {
            if (s.pings.size) failures.push('missing Bitrue heartbeat reply');
            if (url.includes('fmarket-ws'))
              for (const symbol of books) {
                const native = 'e_' + symbol.replace(/^E-/, '').replaceAll('-', '').toLowerCase();
                if (!s.channels.has(`market_${native}_simple_depth_step0`))
                  failures.push('missing Bitrue depth subscription: ' + symbol);
              }
          }
          return failures;
        },
      };
    },
  },
  bitunix: {
    rust: 'bitunix',
    // The private tape times account snapshots after each command; replay them
    // on the recorded clock so a later snapshot cannot answer an earlier read.
    virtualPrivateClock: true,
    ts: { module: `${root}/bitunix/bitunix.exchange.ts`, className: 'BitunixExchange', name: 'bitunix' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.futuresWebsocket.messageHandlers).filter((key) => key.includes('_depthop_')).length >= count,
    privateSocket: '/private/',
    isWrite: (method) => method !== 'GET',
    matchRequestBody: true,
    compareFillPrices: true,
    // `{"ping": <ms>}` is a timestamped heartbeat, not a feed subscription.
    socketMessage: (message) =>
      message && typeof message === 'object' && Object.keys(message).length === 1 && message.ping !== undefined
        ? undefined : message,
    // Documented double-SHA256 signature over nonce, timestamp, key, sorted
    // query (key+value pairs) and body; public /market/ reads are unsigned.
    validateRequest({ url, body = '', headers = {} }) {
      const parsed = new URL(url);
      if (parsed.pathname.includes('/market/')) return;
      const query = [...parsed.searchParams].sort(([a], [b]) => a.localeCompare(b))
        .map(([key, value]) => `${key}${value}`).join('');
      if (!validBitunixSignature(headerValue(headers, 'api-key'), headerValue(headers, 'nonce'),
        headerValue(headers, 'timestamp'), headerValue(headers, 'sign'), query, body))
        throw new Error(`invalid Bitunix request signature: ${parsed.pathname}`);
    },
    validateSocketMessage(socket, text) {
      const frame = JSON.parse(text);
      if (frame.op !== 'login') return;
      const arg = Array.isArray(frame.args) && frame.args.length === 1 ? frame.args[0] : {};
      if (!validBitunixSignature(arg?.apiKey, arg?.nonce, arg?.timestamp, arg?.sign, '', ''))
        throw new Error('invalid Bitunix socket login');
      socket.loginVerified = true;
    },
  },
  bitget: {
    rust: 'bitget',
    virtualPrivateClock: true,
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
    virtualPrivateClock: true,
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
    virtualPrivateClock: true,
    ts: {
      module: `${root}/bitget/bitgetinverse.exchange.ts`,
      className: 'BitgetInverseExchange',
      name: 'bitgetinverse',
    },
    tsBooksSubscribed: (exchange, count) =>
      Object.entries(exchange.publicWebsocket.messageHandlers).filter(
        ([key, handlers]) => key.startsWith('orderBook_') && handlers.length,
      ).length >= count,
    privateSocket: '/ws/private',
    options: { passphrase: 'replay-passphrase' },
  },
  backpack: {
    rust: 'backpack',
    virtualPrivateClock: true,
    // TS ed25519 signing hashes through WebCrypto, which settles on wall time.
    loadTs: ({ isPrivate }) => { if (isPrivate) installSynchronousDigest(); },
    ignoreGeneratedBodyId: (key, value, callerIds, context) =>
      key === 'clientId' &&
      !callerIds.has(String(value)) &&
      context?.method === 'POST' &&
      /^\/api\/v1\/orders?$/.test(new URL(context.url).pathname) &&
      typeof value === 'number' &&
      Number.isSafeInteger(value) &&
      value > 0 &&
      value <= 0xffff_ffff,
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

  hyperliquidspot,
  krakenspot,
  hyperliquid: {
    virtualPrivateClock: true,
    createRequestValidator: createHyperliquidRequestValidator,
    compareWriteOrder: true,
    rust: 'hyperliquid',
    ts: {
      module: `${root}/hyperliquid/hyperliquid.exchange.ts`,
      className: 'HyperliquidExchange',
      name: 'hyperliquid',
    },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
    privateSocket: undefined,
    publicOptions: { key: '' },
    recordOptions: { publicDataOnly: true, key: '' },
  },
  blofin: {
    rust: 'blofin',
    virtualPrivateClock: true,
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
        !/^\d+$/.test(String(timestamp)) ||
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
    virtualPrivateClock: true,
    mexcProtocol: true,
    requestBody: mexcRequestBody,
    requestUrl: mexcRequestUrl,
    generatedBodyId: (key) => key === 'externalOid',
    onSocketSend(socket, text) {
      const { method } = JSON.parse(text);
      if (method === 'login') socket.mexcRole = 'private';
      else if (method?.startsWith('sub.') || method?.startsWith('unsub.')) socket.mexcRole = 'public';
    },
    socketOrdinalMatches(socket, ordinal) {
      if (ordinal === 0) return socket.mexcRole === 'public';
      if (ordinal === 1) return socket.mexcRole === 'private';
      return socket.ordinal === ordinal;
    },
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
    // Shared /edge tapes label public=0/private=1. Match authenticated role,
    // independently of whether catalogue-first private startup connects first.
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
    virtualPrivateClock: true,
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
    virtualPrivateClock: true,
    requestKey: typedRequest,
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
    virtualPrivateClock: true,
    rust: 'cryptocom',
    requestKey: typedRequest,
    ts: { module: root + '/cryptocom/cryptocom.exchange.ts', className: 'CryptocomExchange', name: 'cryptocom' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('book.')).length >= count,
    privateSocket: '/exchange/v1/user',
    matchRequestBody: true,
    cryptoProtocol: true,
    // Authentication is validated separately; economic params stay exact.
    requestBody: (body) => (body?.method && body?.params ? body.params : body),
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
    virtualPrivateClock: true,
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
  binanceoptions: {
    rust: 'binanceoptions', virtualPrivateClock: true, polling: true,
    tsBooksSubscribed: (_exchange, count) => count === 0,
    requestKey: typedRequest, validateRequest: validateBinanceRequest,
    ts: { module: `${root}/binance/binanceoptions.exchange.ts`, className: 'BinanceOptionsExchange', name: 'binanceoptions' },
    volatileBodyFields: true,
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
  },
  polymarket,
  kucoin: {
    ...kucoin,
    // TS keeps canceled rows until a REST order poll replaces them; at wall time a
    // checkpoint races that poll (checkpoints 6-7). Run the polls on tape time.
    virtualPrivateClock: true,
    requestKey: (method, url, body, callerIds) => requestKey(method, url, body, callerIds, kucoin),
  },
  kucoinspot,
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
  [...new Set(Object.values(exchange).flatMap((value) => [
    value,
    value?.axiosInstance,
    ...Object.values(value && typeof value === 'object' ? value : {}),
  ]))].filter((value) => value?.defaults && value?.interceptors);

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
