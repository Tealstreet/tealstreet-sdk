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
    if (parsed.origin !== 'https://rpc.txflow-testnet.xyz' || method !== 'POST') throw new Error('Invalid TxFlow testnet endpoint');
    const header = Object.entries(headers).find(([key]) => key.toLowerCase() === 'x-api-version')?.[1];
    if (header !== '2026-08-16') throw new Error('Missing TxFlow API version');
    const mime = Object.entries(headers).find(([key]) => key.toLowerCase() === 'content-type')?.[1];
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
  if (endpoint.protocol !== 'wss:' || endpoint.hostname !== 'rpc.txflow-testnet.xyz' || !['/ws', '/api-ws'].includes(endpoint.pathname)) throw new Error('Invalid TxFlow socket endpoint');
  const frame = JSON.parse(text);
  if (frame.method === 'ping') return;
  if (!['subscribe', 'unsubscribe'].includes(frame.method)) throw new Error('Invalid TxFlow websocket method');
  if (!frame.subscription || typeof frame.subscription.type !== 'string' || !frame.subscription.type.length) throw new Error('Missing TxFlow subscription');
  if (new URL(socket.url).pathname === '/ws' && frame.type !== frame.subscription?.type) throw new Error('Missing TxFlow node channel type');
  if (new URL(socket.url).pathname === '/api-ws' && frame.type !== undefined) throw new Error('Unexpected TxFlow API channel type');
}
