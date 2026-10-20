import { uniqueRequestHeaders } from './protocol-headers.mjs';
// Verifies the native documented preimage independently of either adapter.
import { createRequire } from 'node:module';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
let ethers, encode;
export async function initializeTxFlowCrypto() {
  ethers ??= require('ethers');
  encode ??= require('@msgpack/msgpack').encode;
}
const ordered = (value, keys) => {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('TxFlow action must be an object');
  if (Object.keys(value).some((key) => !keys.includes(key))) throw new Error('Unexpected TxFlow action field');
  return Object.fromEntries(keys.filter((key) => Object.hasOwn(value, key)).map((key) => {
    if (value[key] === null || value[key] === undefined) throw new Error('TxFlow optional fields must be omitted');
    return [key, value[key]];
  }));
};
const integer = (n) => { if (!Number.isSafeInteger(n) || n < 0) throw new Error('Unsafe TxFlow native integer'); return n; };
const nativeBody = (action) => {
  const { type, ...body } = action;
  if (type === 'order') {
    const result = ordered(body, ['grouping', 'orders', 'builder']);
    if (!Array.isArray(result.orders) || !result.orders.length || result.orders.length > 50) throw new Error('Invalid TxFlow order batch');
    result.orders = result.orders.map((o) => {
      const row = ordered(o, ['a', 'b', 'p', 's', 'r', 't', 'c', 'm', 'expiresAt']);
      integer(row.a);
      if (typeof row.b !== 'boolean' || typeof row.p !== 'string' || typeof row.s !== 'string') throw new Error('Invalid TxFlow order fields');
      if (row.c !== undefined && (typeof row.c !== 'string' || !row.c.length || row.c.startsWith('0x') || Buffer.byteLength(row.c) > 64)) throw new Error('Invalid TxFlow cloid');
      row.t = ordered(row.t, ['limit', 'trigger']);
      if (Object.keys(row.t).length !== 1) throw new Error('Invalid TxFlow order type');
      if (row.t.limit) row.t.limit = ordered(row.t.limit, ['tif']);
      else row.t.trigger = ordered(row.t.trigger, ['isMarket', 'triggerPx', 'tpsl']);
      if (row.r !== undefined && typeof row.r !== 'boolean') throw new Error('Invalid TxFlow reduce-only field');
      return row;
    });
    return result;
  }
  if (type === 'cancel') {
    const result = ordered(body, ['cancels']);
    if (!Array.isArray(result.cancels) || !result.cancels.length || result.cancels.length > 200) throw new Error('Invalid TxFlow cancel batch');
    result.cancels = result.cancels.map((o) => {
      const row = ordered(o, ['a', 'o', 'grouping']); integer(row.a); integer(row.o); return row;
    });
    return result;
  }
  if (type === 'modify2') {
    const result = ordered(body, ['order', 'tpsl']);
    result.order = ordered(result.order, ['o', 'p', 's', 't']); integer(result.order.o);
    return result;
  }
  const fields = { updateLeverage: ['asset', 'leverage'], updateMarginMode: ['asset', 'isCross'], updatePositionMode: ['isOneway'] }[type];
  if (!fields) throw new Error('Unsupported TxFlow signing action');
  const result = ordered(body, fields);
  if ('asset' in result) integer(result.asset);
  return result;
};
export function createTxFlowRequestValidator(options, { callerIds, requireCreationIds = callerIds !== undefined } = {}) {
  const knownCallerIds = callerIds ?? new Set();
  let lastNonce = -1;
  const generated = new Set();
  return ({ method, url, headers = {}, body }) => {
    const parsed = new URL(url);
    if (parsed.origin !== 'https://rpc.txflow-testnet.xyz' || parsed.username || parsed.password || parsed.hash || parsed.search || method !== 'POST') throw new Error('Invalid TxFlow testnet endpoint');
    const fields = uniqueRequestHeaders(headers);
    const header = fields['x-api-version'];
    if (header !== '2026-08-16') throw new Error('Missing TxFlow API version');
    const mime = fields['content-type'];
    if (typeof mime !== 'string' || mime.split(';')[0].trim().toLowerCase() !== 'application/json') throw new Error('Missing TxFlow JSON content type');
    if (parsed.pathname === '/info') {
      const read = typeof body === 'string' ? JSON.parse(body) : body;
      if (!['perpMeta', 'activeAssetCtx', 'candleSnapshot', 'l2Book'].includes(read?.type)) {
        const expected = options.vault || options.applicationId;
        if (!expected || typeof read?.user !== 'string' || read.user.toLowerCase() !== expected.toLowerCase()) throw new Error('Invalid TxFlow account read subject');
      }
      return;
    }
    if (parsed.pathname !== '/exchange' || !ethers || !encode) throw new Error('Uninitialized TxFlow verifier');
    const envelope = typeof body === 'string' ? JSON.parse(body) : body;
    const nonce = integer(envelope.nonce);
    if (nonce <= lastNonce || Math.abs(nonce - Date.now()) > 60_000) throw new Error('Invalid TxFlow replay nonce');
    if (envelope.confirm !== 'executed') throw new Error('TxFlow execution confirmation required');
    if ((envelope.vaultAddress ?? '').toLowerCase() !== (options.vault ?? '').toLowerCase()) throw new Error('Unexpected TxFlow vault');
    const packed = encode(nativeBody(envelope.action));
    const creationIds = envelope.action.type === 'order' ? envelope.action.orders.map((o) => o.c) : [];
    if (requireCreationIds && creationIds.some((id) => id === undefined)) throw new Error('Missing connector-generated TxFlow creation cloid');
    const presentIds = creationIds.filter((id) => id !== undefined);
    if (new Set(presentIds).size !== presentIds.length) throw new Error('Duplicate TxFlow creation cloid');
    if (presentIds.some((id) => !knownCallerIds.has(id) && generated.has(id))) throw new Error('Reused generated TxFlow cloid');
    const suffix = Buffer.alloc(envelope.vaultAddress == null ? 9 : 29);
    suffix.writeBigUInt64BE(BigInt(nonce));
    if (envelope.vaultAddress != null) {
      suffix[8] = 1; suffix.set(ethers.getBytes(ethers.getAddress(envelope.vaultAddress)), 9);
      if (envelope.vaultAddress.toLowerCase() !== options.vault?.toLowerCase()) throw new Error('Unexpected TxFlow vault');
    }
    const connectionId = ethers.keccak256(Buffer.concat([packed, suffix]));
    const digest = ethers.TypedDataEncoder.hash(
      { name: 'TxFlow-Testnet', version: '1', chainId: 2511, verifyingContract: ethers.ZeroAddress },
      { Agent: [{ name: 'txflowNetwork', type: 'string' }, { name: 'chainId', type: 'uint32' }, { name: 'apiVersion', type: 'uint32' }, { name: 'connectionId', type: 'bytes32' }] },
      { txflowNetwork: 'TxFlow-Testnet', chainId: 2511, apiVersion: 1, connectionId },
    );
    const signer = ethers.recoverAddress(digest, envelope.signature);
    if (signer.toLowerCase() !== new ethers.Wallet(options.secret).address.toLowerCase()) throw new Error('Invalid TxFlow signature');
    lastNonce = nonce;
    for (const id of presentIds) if (!knownCallerIds.has(id)) generated.add(id);
  };
}
export function validateTxFlowSocketMessage(socket, text) {
  const endpoint = new URL(socket.url);
  if (endpoint.origin !== 'wss://rpc.txflow-testnet.xyz' || endpoint.username || endpoint.password || endpoint.hash || endpoint.search || !['/ws', '/api-ws'].includes(endpoint.pathname)) throw new Error('Invalid TxFlow socket endpoint');
  const frame = JSON.parse(text);
  if (frame.method === 'ping') return;
  if (!['subscribe', 'unsubscribe'].includes(frame.method)) throw new Error('Invalid TxFlow websocket method');
  if (!frame.subscription || typeof frame.subscription.type !== 'string' || !frame.subscription.type.length) throw new Error('Missing TxFlow subscription');
  if (new URL(socket.url).pathname === '/ws' && frame.type !== frame.subscription?.type) throw new Error('Missing TxFlow node channel type');
  if (new URL(socket.url).pathname === '/api-ws' && frame.type !== undefined) throw new Error('Unexpected TxFlow API channel type');
}

// Only the authenticated exchange envelope carries these transport fields.
export const txFlowRequestBody = (body, { method, url, callerIds }) => {
  if (method !== 'POST' || new URL(url).pathname !== '/exchange' || !body?.action) return body;
  const { nonce: _nonce, signature: _signature, ...result } = body;
  if (result.action.type === 'order' && Array.isArray(result.action.orders)) result.action = {
    ...result.action, orders: result.action.orders.map((order) => {
      if (typeof order.c !== 'string' || callerIds.has(order.c) || !(order.c.startsWith('ts_') && order.c.length > 3 && order.c.length <= 24 && !/[^A-Za-z0-9]/.test(order.c.slice(3)))) return order;
      const { c: _c, ...economic } = order;
      return economic;
    }),
  };
  return result;
};

export const createTxFlowSocketAudit = (_impl, { isPrivate, records = [] } = {}) => {
  const options = records.find((r) => r.type === 'config')?.options ?? {};
  const subject = String(options.vault ?? options.applicationId ?? '0x0000000000000000000000000000000000000001').toLowerCase();
  const identities = new Map();
  for (const record of records) if (record.type === 'http') {
    let body; try { body = JSON.parse(record.body); } catch { continue; }
    for (const row of body?.data?.universe ?? []) if (Number.isSafeInteger(row.index) && typeof row.name === 'string') {
      for (const name of [String(row.index), row.name, row.baseCurrency]) if (typeof name === 'string') identities.set(name.toLowerCase(), String(row.index));
    }
  }
  const coin = (value) => identities.get(String(value).toLowerCase()) ?? String(value).toLowerCase();
  const states = new Map(), failures = [];
  const state = (url, socket) => {
    const key = socket && typeof socket === 'object' ? socket : `${url}#${socket ?? 1}`;
    if (!states.has(key)) states.set(key, { closed: false, topics: new Map() });
    return states.get(key);
  };
  const key = (topic) => JSON.stringify(Object.fromEntries(Object.entries(topic).map(([name,value]) => [name,name === 'coin' ? coin(value) : value]).sort(([a], [b]) => a.localeCompare(b))));
  const fail = (message) => failures.push(message);
  return {
    connected(url, socket) { state(url, socket); },
    closed(url, socket) { state(url, socket).closed = true; },
    sent(url, text, socket) {
      const current = state(url, socket);
      if (current.closed) { fail('TxFlow sent on closed socket'); return; }
      validateTxFlowSocketMessage({ url }, text);
      const frame = JSON.parse(text);
      if (frame.method === 'ping') return;
      const topic = frame.subscription;
      if (topic.user !== undefined && (typeof topic.user !== 'string' || topic.user.toLowerCase() !== subject)) {
        fail('TxFlow subscription changed account'); return;
      }
      if (frame.method === 'subscribe') current.topics.set(key(topic), topic);
      else current.topics.delete(key(topic));
    },
    received(url, text, socket) {
      const current = state(url, socket);
      if (current.closed) { fail('TxFlow received on closed socket'); return; }
      const frame = JSON.parse(text), channel = frame.channel;
      if (channel === 'pong' || String(frame.method).toLowerCase() === 'pong') return;
      if (channel === 'subscriptionResponse') {
        const topic = frame.data?.subscription;
        if (frame.data?.method === 'subscribe' && (!topic || !current.topics.has(key(topic)))) fail('TxFlow acknowledgement without own subscription');
        return;
      }
      const data = frame.data;
      if (data?.user !== undefined && (typeof data.user !== 'string' || data.user.toLowerCase() !== subject)) fail('TxFlow data changed account');
      // The API historical fill snapshot is ignored by both adapters.
      if (channel === 'userFills' && data?.isSnapshot === true) return;
      const wanted = channel === 'activeAssetData' ? 'userAssetData' : channel;
      const own = [...current.topics.values()].some((topic) => topic.type === wanted
        && (channel !== 'ticker' || String(topic.instrumentId) === String(data?.instrumentId))
        && (channel !== 'l2Book' || coin(topic.coin) === coin(data?.coin))
        && (channel !== 'candle' || coin(topic.coin) === coin(data?.s ?? data?.coin)));
      if (!own) fail('TxFlow data without own subscription: ' + channel);
    },
    ready() { return !isPrivate || [...states.values()].some((s) => !s.closed && [...s.topics.values()].some((t) => typeof t.user === 'string' && t.user.toLowerCase() === subject)); },
    errors() { return [...new Set(failures)]; },
  };
};
