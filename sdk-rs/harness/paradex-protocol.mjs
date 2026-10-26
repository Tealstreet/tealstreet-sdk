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
const headerMap = (headers) => Object.fromEntries(Object.entries(headers ?? {}).map(([key, value]) => [key.toLowerCase(), String(value)]));
const isPublic = (path) => /^\/v1\/(?:system\/config|markets(?:\/|$)|trades(?:\/|$)|orderbook(?:\/|$)|tradingview(?:\/|$))/.test(path);
const verifyRequest = ({ method, url, body, headers }) => {
  const parsed = new URL(url);
  if (!['api.prod.paradex.trade', 'api.testnet.paradex.trade'].includes(parsed.hostname)) return false;
  if (isPublic(parsed.pathname)) return method === 'GET';
  const testnet = parsed.hostname.includes('testnet');
  const h = headerMap(headers);
  if (parsed.pathname.startsWith('/v1/auth/')) {
    const timestamp = Number(h['paradex-timestamp']);
    const expiration = Number(h['paradex-signature-expiration']);
    return method === 'POST' && parsed.pathname === `/v1/auth/${credentials.subkeyAddress}` && (body == null || body === '') &&
      h['paradex-starknet-account'] === credentials.paradexAddress && Number.isSafeInteger(timestamp) &&
      Math.abs(timestamp * 1000 - Date.now()) < 60_000 && [300, 86400].includes(expiration - timestamp) &&
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
const requestBody = (value) => Array.isArray(value) ? value.map(requestBody) : value && typeof value === 'object'
  ? Object.fromEntries(Object.entries(value).filter(([key]) => !['signature', 'signature_timestamp'].includes(key)).map(([key, item]) => [key, requestBody(item)])) : value;
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
    if (!sockets.has(key)) sockets.set(key, { channels: new Set(), auth: false, authId: undefined });
    return sockets.get(key);
  };
  const requiredPrivate = ['orders.ALL', 'fills.ALL', 'positions', 'balance_events', 'account'];
  const requiredPublic = [...tickers.flatMap((s) => [`bbo.${s}`, `trades.${s}`]), 'markets_summary', ...books.map((s) => `order_book.${s}.deltas`)];
  const check = () => {
    const all = [...states.values()].flatMap((sockets) => [...sockets.values()]);
    if (!requiredPublic.every((topic) => all.some((s) => s.channels.has(topic)))) return false;
    return !isPrivate || all.some((s) => s.auth && requiredPrivate.every((topic) => s.channels.has(topic)));
  };
  return {
    sent(url, text, socket) {
      const s = state(url, socket);
      const frame = JSON.parse(text);
      if (['ping', 'pong'].includes(frame.type)) { failures.push('Unexpected Paradex application heartbeat'); return; }
      if (frame.method === 'auth') {
        if (frame.params?.bearer !== TOKEN) failures.push('Invalid Paradex socket JWT');
        else s.authId = frame.id;
      } else if (['subscribe', 'unsubscribe'].includes(frame.method)) {
        const topic = frame.params?.channel;
        if (typeof topic !== 'string') failures.push('Invalid Paradex subscription');
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
  // TS retains its 24-hour auth signature; Rust intentionally limits replay
  // authorization to five minutes. Both preimages are independently verified.
  createRequestAudit: (implementation) => ({
    validate({ url, headers }) {
      if (!new URL(url).pathname.startsWith('/v1/auth/')) return;
      const h = headerMap(headers);
      const lifetime = Number(h['paradex-signature-expiration']) - Number(h['paradex-timestamp']);
      if (lifetime !== (implementation === 'rust' ? 300 : 86400)) throw new Error('Unexpected Paradex auth lifetime');
    },
    response() {},
  }),
  compareClientIds: true, compareFillPrices: true, compareSubscriptions: true, compareWriteOrder: true,
  matchRequestBody: true, requestBody, generatedBodyId: (key) => key === 'client_id',
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
  skipFrame: (record) => {
    if (record.type !== 'ws' || typeof record.text !== 'string') return false;
    let frame;
    try { frame = JSON.parse(record.text); } catch { return false; }
    if (frame.id === undefined) return false;
    if (frame.error !== undefined) throw new Error(`Recorded Paradex control rejection: ${JSON.stringify(frame.error)}`);
    // Bind successful recorded ACKs to actual RPC IDs; market/account bytes stay raw.
    return frame.result !== undefined;
  },
  publicTickers: (records) => {
    const seed = records.find((r) => r.type === 'http' && r.method === 'GET' && new URL(r.url).pathname === '/v1/markets');
    return seed ? JSON.parse(seed.body).results.filter((m) => (m.status === undefined || m.status === 'ACTIVE') && String(m.symbol ?? m.market).endsWith('-PERP'))
      .map((m) => m.symbol ?? m.market) : [];
  },
};
