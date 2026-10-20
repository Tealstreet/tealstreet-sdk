// Independent wire checks from the official Paradex Python message schemas.
import { createRequire } from 'node:module';
import { parseRequestJson } from './request-json.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
const credentials = {
  key: `0x${'1'.padStart(64, '0')}`, secret: '', paradexAddress: '0x0123', subkeyAddress: '0x456',
};
const TOKEN = 'replay-paradex-jwt';
const chain = (testnet) => testnet ? 'PRIVATE_SN_POTC_SEPOLIA' : 'PRIVATE_SN_PARACLEAR_MAINNET';
const DOMAIN = [{ name: 'name', type: 'felt' }, { name: 'chainId', type: 'felt' }, { name: 'version', type: 'felt' }];
const fields = (names) => names.map((name) => ({ name, type: 'felt' }));
const quantum = (value) => {
  const match = /^(\d+)(?:\.(\d*))?(?:e([+-]?\d+))?$/i.exec(String(value));
  if (!match || Math.abs(Number(match[3] ?? 0)) > 100) throw new Error('Invalid Paradex signing decimal');
  const digits = match[1] + (match[2] ?? '');
  const places = 8 + Number(match[3] ?? 0) - (match[2]?.length ?? 0);
  return String(places >= 0 ? BigInt(digits) * 10n ** BigInt(places) : BigInt(digits) / 10n ** BigInt(-places));
};
const typed = (type, names, message, testnet) => ({
  domain: { name: 'Paradex', chainId: chain(testnet), version: '1' }, primaryType: type,
  types: { StarkNetDomain: DOMAIN, [type]: fields(names) }, message,
});
const verify = (signature, data) => {
  const { ec, typedData } = require('starknet');
  const pair = JSON.parse(signature);
  if (!Array.isArray(pair) || pair.length !== 2 || pair.some((v) => typeof v !== 'string' || !/^\d+$/.test(v))) return false;
  return ec.starkCurve.verify(new ec.starkCurve.Signature(BigInt(pair[0]), BigInt(pair[1])),
    typedData.getMessageHash(data, credentials.paradexAddress), ec.starkCurve.getPublicKey(credentials.key));
};
const headerMap = (headers) => {
  const entries = (Array.isArray(headers) ? headers : headers instanceof Headers ? [...headers.entries()] : Object.entries(headers ?? {}))
    .map(([key, value]) => [key.toLowerCase(), String(value)]);
  // HTTP combines duplicate field names. A last-wins map must not turn an
  // ambiguous Authorization/signing header into a uniquely authenticated one.
  if (new Set(entries.map(([key]) => key)).size !== entries.length) throw new Error('Duplicate Paradex request header');
  return Object.fromEntries(entries);
};
const isPublic = (path) => /^\/v1\/(?:system\/config|markets(?:\/|$)|trades(?:\/|$)|orderbook(?:\/|$)|tradingview(?:\/|$))/.test(path);
const generatedClientId = (value) => typeof value === 'string' && value.startsWith('ts_') && value.length > 3 && value.length <= 24 && !/[^A-Za-z0-9]/.test(value.slice(3));
const verifyRequest = ({ method, url, body, headers, callerIds = new Set() }) => {
  const parsed = new URL(url);
  if (parsed.protocol !== 'https:' || !['api.prod.paradex.trade', 'api.testnet.paradex.trade'].includes(parsed.hostname)) return false;
  if (isPublic(parsed.pathname)) return method === 'GET';
  const testnet = parsed.hostname.includes('testnet');
  const h = headerMap(headers);
  if (parsed.pathname.startsWith('/v1/auth/')) {
    const timestamp = Number(h['paradex-timestamp']);
    const expiration = Number(h['paradex-signature-expiration']);
    return method === 'POST' && parsed.pathname === `/v1/auth/${credentials.subkeyAddress}` && (body == null || body === '') &&
      h['paradex-starknet-account'] === credentials.paradexAddress && Number.isSafeInteger(timestamp) &&
      Math.abs(timestamp * 1000 - Date.now()) < 60_000 && expiration === timestamp + 86400 &&
      verify(h['paradex-starknet-signature'], typed('Request', ['method', 'path', 'body', 'timestamp', 'expiration'],
        { method: 'POST', path: '/v1/auth', body: '', timestamp, expiration }, testnet));
  }
  if (h.authorization !== `Bearer ${TOKEN}`) return false;
  if (method === 'POST' && parsed.pathname.startsWith('/v1/orders') && h['paradex-partner'] !== 'TEALSTREET') return false;
  if (!['POST', 'PUT'].includes(method) || !/^\/v1\/orders(?:\/|$)/.test(parsed.pathname)) return true;
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  const payload = typeof body === 'string' ? parseRequestJson(body) : body;
  const rows = Array.isArray(payload) ? payload : Array.isArray(payload?.orders) ? payload.orders : [payload];
  return rows.length > 0 && rows.every((row) => {
    if (!row || typeof row !== 'object') return false;
    if (Object.hasOwn(row, 'client_id') && (typeof row.client_id !== 'string' || !row.client_id || row.client_id.length > 64 || /[\r\n\u2028\u2029]/.test(row.client_id)
      || method === 'POST' && !callerIds.has(row.client_id) && !generatedClientId(row.client_id))) return false;
    const timestamp = Number(row.signature_timestamp);
    if (!Number.isSafeInteger(timestamp) || Math.abs(timestamp - Date.now()) > 60_000 || !['BUY', 'SELL'].includes(row.side)) return false;
    const amend = method === 'PUT';
    const message = { timestamp: String(timestamp), market: row.market, side: row.side === 'BUY' ? '1' : '2',
      orderType: row.type, size: quantum(row.size), price: row.type === 'MARKET' || !row.price ? '0' : quantum(row.price),
      ...(amend ? { id: parsed.pathname.split('/').at(-1) } : {}) };
    return verify(row.signature, typed(amend ? 'ModifyOrder' : 'Order',
      ['timestamp', 'market', 'side', 'orderType', 'size', 'price', ...(amend ? ['id'] : [])], message, testnet));
  });
};
const requestBody = (value, { method, url }) => {
  const path = new URL(url).pathname;
  if (!['POST', 'PUT'].includes(method) || !/^\/v1\/orders(?:\/|$)/.test(path)) return value;
  const strip = (row) => row && typeof row === 'object' && !Array.isArray(row)
    ? Object.fromEntries(Object.entries(row).filter(([key]) => !['signature', 'signature_timestamp'].includes(key))) : row;
  if (Array.isArray(value)) return value.map(strip);
  if (Array.isArray(value?.orders)) return { ...value, orders: value.orders.map(strip) };
  return strip(value);
};
const requestOptions = (method, url) => ({ preserveBodyFields: ['signature', 'signature_timestamp'], generatedBodyId: (key, value) =>
  method === 'POST' && ['/v1/orders', '/v1/orders/batch'].includes(url.pathname) && key === 'client_id'
  && (generatedClientId(value) || value === 'generated') });
const socketMessage = (message) => {
  if (['ping', 'pong'].includes(message?.type)) return;
  const { id: _id, ...rest } = message;
  return rest;
};
export const paradexStream = (data) => {
  if (typeof data === 'string') {
    try { return JSON.parse(data)?.params?.channel; } catch { return; }
  }
  const b = data instanceof Uint8Array ? data : new Uint8Array(data);
  if (b.length < 8) throw new Error('Truncated Paradex SBE tape frame');
  const v = new DataView(b.buffer, b.byteOffset, b.byteLength);
  const template = v.getUint16(2, true);
  const privateChannel = { 20: 'orders.ALL', 21: 'fills.ALL', 22: 'positions', 23: 'account' }[template];
  if (privateChannel) return privateChannel;
  if (![1, 2, 3, 4].includes(template)) return;
  let offset = 8 + v.getUint16(0, true);
  if (template === 3) for (let group = 0; group < 2; group++) {
    if (offset + 4 > b.length) throw new Error('Truncated Paradex SBE group');
    const block = v.getUint16(offset, true), count = v.getUint16(offset + 2, true);
    if (block < 16) throw new Error('Invalid Paradex SBE group');
    offset += 4 + block * count;
  }
  if (offset >= b.length || offset + 1 + b[offset] > b.length) throw new Error('Truncated Paradex SBE market');
  const market = new TextDecoder('utf-8', { fatal: true }).decode(b.subarray(offset + 1, offset + 1 + b[offset]));
  return template === 4 ? 'markets_summary' : template === 3 ? `order_book.${market}.deltas` : `${template === 1 ? 'trades' : 'bbo'}.${market}`;
};
const createSocketAudit = (_impl, { isPrivate, books = [], tickers = [] }) => {
  const states = new Map();
  const failures = [];
  const state = (url, key = url) => {
    if (!states.has(url)) states.set(url, new Map());
    const sockets = states.get(url);
    if (!sockets.has(key)) sockets.set(key, { closed: false, channels: new Set(), auth: false, authId: undefined });
    return sockets.get(key);
  };
  const requiredPrivate = ['orders.ALL', 'fills.ALL', 'positions', 'balance_events', 'account'];
  const requiredPublic = [...tickers.flatMap((s) => [`bbo.${s}`, `trades.${s}`]), 'markets_summary', ...books.map((s) => `order_book.${s}.deltas`)];
  const check = () => {
    const all = [...states.values()].flatMap((sockets) => [...sockets.values()]).filter((s) => !s.closed);
    if (!requiredPublic.every((topic) => all.some((s) => s.channels.has(topic)))) return false;
    return !isPrivate || all.some((s) => s.auth && requiredPrivate.every((topic) => s.channels.has(topic)));
  };
  return {
    connected(url, socket) { state(url, socket); },
    closed(url, socket) { state(url, socket).closed = true; },
    sent(url, text, socket) {
      const s = state(url, socket);
      const frame = JSON.parse(text);
      if (frame.jsonrpc !== '2.0' || !Number.isSafeInteger(frame.id) || frame.id < 0) failures.push('Invalid Paradex JSON-RPC request identity');
      if (['ping', 'pong'].includes(frame.type)) { failures.push('Unexpected Paradex application heartbeat'); return; }
      if (frame.method === 'auth') {
        if (frame.params?.bearer !== TOKEN) failures.push('Invalid Paradex socket JWT');
        else s.authId = frame.id;
      } else if (['subscribe', 'unsubscribe'].includes(frame.method)) {
        const topic = frame.params?.channel;
        if (typeof topic !== 'string' || ![...requiredPublic, ...requiredPrivate].includes(topic)) failures.push('Invalid Paradex subscription');
        if (requiredPrivate.includes(topic) && !s.auth) failures.push('Paradex private subscription before authentication');
        if (frame.method === 'subscribe') s.channels.add(topic); else s.channels.delete(topic);
      } else failures.push('Unexpected Paradex socket write');
    },
    received(url, data, socket) {
      const s = state(url, socket);
      if (typeof data === 'string') {
        const frame = JSON.parse(data);
        if (s.authId !== undefined && frame.id === s.authId && frame.result && !frame.error) s.auth = true;
      }
      const topic = paradexStream(data);
      if (topic && !s.channels.has(topic)) failures.push(`Unsubscribed Paradex data: ${topic}`);
    },
    ready: check,
    errors() { if (!check()) failures.push('Missing Paradex public/private subscription or authentication'); return failures; },
  };
};
export const paradexProtocol = {
  replayCredentials: credentials,
  // Match loadEngine: load code before baseline, keeping account I/O and signing timed.
  loadTs: async ({ isPrivate }) => { if (isPrivate) await import(require.resolve('@tealstreet/paradex-sdk')); },
  compareClientIds: true, compareFillPrices: true, compareSubscriptions: true, compareWriteOrder: true,
  matchRequestBody: true, requestBody, requestOptions, generatedId: () => false,
  subscriptionRouting: true, streamOf: paradexStream, socketMessage, createSocketAudit, verifyRequest,
  isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.startsWith('/v1/auth/'),
  socketReplies: (_url, message) => {
    if (['ping', 'pong'].includes(message?.type)) throw new Error('Unexpected Paradex application heartbeat');
    if (message?.method === 'auth') {
      if (message.params?.bearer !== TOKEN) throw new Error('Invalid Paradex socket auth');
      return [JSON.stringify({ jsonrpc: '2.0', id: message.id, result: {} })];
    }
    if (['subscribe', 'unsubscribe'].includes(message?.method)) return [JSON.stringify({ jsonrpc: '2.0', id: message.id, result: { channel: message.params.channel } })];
    return [];
  },
  recordedFrame: (record, { tickers, books, isPrivate }) => {
    if (record.type !== 'ws' || typeof record.text !== 'string') return record;
    let frame;
    try { frame = JSON.parse(record.text); } catch { return record; }
    if (frame.id === undefined) return record;
    if (frame.error !== undefined) throw new Error(`Recorded Paradex control rejection: ${JSON.stringify(frame.error)}`);
    if (frame.result === undefined) return record;
    if (frame.jsonrpc !== '2.0' || !Number.isSafeInteger(frame.id) || frame.id < 0 || !frame.result || typeof frame.result !== 'object' || Array.isArray(frame.result))
      throw new Error('Invalid recorded Paradex acknowledgement');
    const channel = frame.result.channel;
    const allowed = [...tickers.flatMap((s) => [`bbo.${s}`, `trades.${s}`]), 'markets_summary', ...books.map((s) => `order_book.${s}.deltas`),
      ...(isPrivate ? ['orders.ALL', 'fills.ALL', 'positions', 'balance_events', 'account'] : [])];
    if (Object.keys(frame.result).length !== 1 || !allowed.includes(channel)) throw new Error('Unmatched recorded Paradex acknowledgement');
    return { ...record, type: 'wait', controlChannel: channel };
  },
  validateWait: (frame, tape) => {
    if (frame.controlChannel && !tape.sockets.some((socket) => socket.opened && !socket.closed && socket.subscriptions.has(frame.controlChannel)))
      throw new Error(`Recorded Paradex acknowledgement has no actual subscription: ${frame.controlChannel}`);
  },
  publicTickers: (records) => {
    const seed = records.find((r) => r.type === 'http' && r.method === 'GET' && new URL(r.url).pathname === '/v1/markets');
    return seed ? JSON.parse(seed.body).results.filter((m) => (m.status === undefined || m.status === 'ACTIVE') && String(m.symbol ?? m.market).endsWith('-PERP'))
      .map((m) => m.symbol ?? m.market) : [];
  },
};
