// Replay-only authentication oracle. Never import either candidate's signer.
// Protocol: hyperliquid-python-sdk/utils/signing.py (action_hash/l1_payload).
import { createRequire } from 'node:module';
import { requestKey } from './requests.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
let ethers, encode;
const crypto = () => {
  if (!ethers) {
    ({ ethers } = require('ethers'));
    ({ encode } = require('@msgpack/msgpack'));
  }
  return ethers;
};
const address = (value) => typeof value === 'string' && /^0x[\da-f]{40}$/i.test(value);
const integer = (value) => Number.isSafeInteger(value) && value >= 0;
const cloid = (value) => typeof value === 'string' && /^0x[\da-f]{32}$/i.test(value);
const fail = (message) => { throw new Error(`Hyperliquid ${message}`); };
const object = (value) => value !== null && typeof value === 'object' && !Array.isArray(value);
const parse = (body) => JSON.parse(body, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? BigInt(context.source) : value);
const unsigned64 = (value) => integer(value) || (typeof value === 'bigint' && value >= 0n && value <= 0xffffffffffffffffn);
// useBigInt64 intentionally encodes large Number integers as float64. Promote
// only the integers outside msgpack's 32-bit forms, retaining their wire type.
const msgpackValue = (value) => {
  if (typeof value === 'number' && Number.isSafeInteger(value) && (value > 0xffffffff || value < -0x80000000)) return BigInt(value);
  if (Array.isArray(value)) return value.map(msgpackValue);
  if (object(value)) return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, msgpackValue(item)]));
  return value;
};
const l1Actions = new Set(['order', 'cancel', 'cancelByCloid', 'modify', 'batchModify',
  'updateLeverage', 'updateIsolatedMargin', 'scheduleCancel']);
const agentInfo = new Set(['userRole']);
const accountInfo = new Set(['userFees', 'clearinghouseState', 'spotClearinghouseState', 'frontendOpenOrders',
  'openOrders', 'userAbstraction', 'maxBuilderFee', 'userFills', 'userFillsByTime',
  'historicalOrders', 'userFunding', 'userNonFundingLedgerUpdates', 'portfolio',
  'userRateLimit', 'activeAssetData', 'subAccounts']);

// Export the primitive separately: official dummy/nonce0 vectors do not widen
// the live replay action allowlist or bypass freshness checks.
export function hyperliquidActionHash(frame) {
  crypto();
  if (!object(frame.action) || !integer(frame.nonce)) fail('malformed action or nonce');
  const vault = frame.vaultAddress;
  if (vault != null && !address(vault)) fail('malformed vault');
  if (frame.expiresAfter != null && !integer(frame.expiresAfter)) fail('malformed expiry');
  const suffix = Buffer.alloc(9 + (vault == null ? 0 : 20) + (frame.expiresAfter == null ? 0 : 9));
  suffix.writeBigUInt64BE(BigInt(frame.nonce));
  if (vault != null) {
    suffix[8] = 1;
    Buffer.from(vault.slice(2), 'hex').copy(suffix, 9);
  }
  if (frame.expiresAfter != null) suffix.writeBigUInt64BE(BigInt(frame.expiresAfter), suffix.length - 8);
  // Preserve insertion order and lossless uint64 action fields. Sorting here
  // would validate a different payload from the one sent to the venue.
  return ethers.keccak256(Buffer.concat([encode(msgpackValue(frame.action), { useBigInt64: true }), suffix]));
}
export function recoverHyperliquidSigner(frame, testnet = false) {
  const connectionId = hyperliquidActionHash(frame);
  const signature = frame.signature;
  if (!object(signature) || ![27, 28].includes(signature.v) ||
      ![signature.r, signature.s].every((part) => typeof part === 'string' && /^0x[\da-f]{1,64}$/i.test(part)))
    fail('malformed signature');
  return ethers.verifyTypedData(
    { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: ethers.ZeroAddress },
    { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] },
    { source: testnet ? 'b' : 'a', connectionId },
    { r: `0x${signature.r.slice(2).padStart(64, '0')}`, s: `0x${signature.s.slice(2).padStart(64, '0')}`, v: signature.v },
  ).toLowerCase();
}

export function createHyperliquidRequestValidator(options, { callerIds = new Set(), getCommandIndex = () => 0, getExpectedWrite } = {}) {
  const testnet = options.testnet === true;
  const origin = testnet ? 'https://api.hyperliquid-testnet.xyz' : 'https://api.hyperliquid.xyz';
  const signer = options.key?.toLowerCase();
  const vault = options.vault?.toLowerCase() || null;
  const account = vault || options.applicationId?.toLowerCase();
  const creations = new Map();
  const aliases = new Map();
  // Fresh offline signer history; retain the highest 100 acknowledged nonces.
  // Different vault/account subjects still share this configured signer's set.
  const nonces = new Set();
  return ({ method, url, body, now = Date.now() }) => {
    const endpoint = new URL(url);
    if (method !== 'POST' || endpoint.origin !== origin || endpoint.search || endpoint.hash ||
        endpoint.username || endpoint.password || !['/info', '/exchange'].includes(endpoint.pathname))
      fail('request endpoint/network mismatch');
    const frame = parse(body);
    if (!object(frame)) fail('malformed request');
    if (endpoint.pathname === '/info') {
      if (typeof frame.type !== 'string' || !frame.type) fail('missing info type');
      // The live public recording includes TS probing userFees with no wallet.
      // Preserve its recorded 422 fallback, never accept it as an account read
      // or silently grant a successful fee response to an invalid subject.
      if (options.publicDataOnly === true && options.key === '' && !account &&
          frame.type === 'userFees' && frame.user === '')
        return (record) => { if (record.status !== 422) fail('anonymous fee probe must retain recorded rejection'); };
      if (agentInfo.has(frame.type) || accountInfo.has(frame.type) || frame.user !== undefined) {
        const expected = agentInfo.has(frame.type) ? signer : account;
        if (!address(expected) || !address(frame.user) || frame.user.toLowerCase() !== expected)
          fail('info account/agent identity mismatch');
      }
      return;
    }
    if (!address(signer) || (vault !== null && !address(vault))) fail('invalid configured signing identity');
    if (Object.keys(frame).some((key) => !['action', 'nonce', 'signature', 'vaultAddress', 'expiresAfter'].includes(key)))
      fail('unsupported signing envelope');
    if (!object(frame.action) || !l1Actions.has(frame.action.type)) fail('unsupported L1 action');
    // Actual venue bounds, rather than an invented strictly monotonic sequence:
    // nonces are signer-scoped and may legitimately arrive out of order.
    if (!integer(now) || !integer(frame.nonce) || frame.nonce <= now - 2 * 86400000 || frame.nonce >= now + 86400000)
      fail('stale or malformed nonce');
    if (frame.expiresAfter != null && (!integer(frame.expiresAfter) || frame.expiresAfter <= now))
      fail('expired or malformed expiry');
    if (frame.vaultAddress != null && !address(frame.vaultAddress)) fail('malformed vault');
    if ((frame.vaultAddress?.toLowerCase() ?? null) !== vault) fail('vault identity mismatch');
    const created = [];
    const reference = (value) => { if (!cloid(value)) fail('malformed cloid reference'); };
    const order = (row, creation) => {
      if (!object(row) || !integer(row.a) || typeof row.b !== 'boolean' || typeof row.r !== 'boolean' ||
          !['p', 's'].every((key) => typeof row[key] === 'string' && /^\d+(?:\.\d+)?$/.test(row[key])) || !object(row.t))
        fail('malformed order');
      const { limit, trigger } = row.t;
      if (Boolean(limit) === Boolean(trigger) || (limit && !['Gtc', 'Ioc', 'Alo'].includes(limit.tif)) ||
          (trigger && (typeof trigger.isMarket !== 'boolean' || !['tp', 'sl'].includes(trigger.tpsl) ||
            typeof trigger.triggerPx !== 'string' || !/^\d+(?:\.\d+)?$/.test(trigger.triggerPx)))) fail('malformed order type');
      if (creation && row.c === undefined) fail('missing creation cloid');
      if (row.c !== undefined) { reference(row.c); if (creation) created.push(row.c.toLowerCase()); }
    };
    const action = frame.action;
    if (action.type === 'order') {
      if (!Array.isArray(action.orders) || !action.orders.length || !['na', 'normalTpsl', 'positionTpsl'].includes(action.grouping))
        fail('malformed order batch');
      action.orders.forEach((row) => order(row, true));
      if (action.builder && (!address(action.builder.b) || !integer(action.builder.f))) fail('malformed builder');
    } else if (action.type === 'cancel' || action.type === 'cancelByCloid') {
      if (!Array.isArray(action.cancels) || !action.cancels.length) fail('malformed cancel batch');
      for (const row of action.cancels) {
        const byCloid = action.type === 'cancelByCloid';
        if (!object(row) || !integer(byCloid ? row.asset : row.a)) fail('malformed cancel asset');
        if (byCloid) reference(row.cloid);
        else if (!unsigned64(row.o)) fail('malformed cancel order ID');
      }
    } else if (action.type === 'modify' || action.type === 'batchModify') {
      const modifications = action.type === 'modify' ? [action] : action.modifies;
      if (!Array.isArray(modifications) || !modifications.length) fail('malformed modifications');
      for (const row of modifications) {
        if (!object(row)) fail('malformed modification');
        if (typeof row.oid === 'string') reference(row.oid);
        else if (!unsigned64(row.oid)) fail('malformed amend order ID');
        order(row.order, false);
      }
    } else if (action.type === 'updateLeverage') {
      if (!integer(action.asset) || typeof action.isCross !== 'boolean' || !integer(action.leverage) || !action.leverage)
        fail('malformed leverage action');
    } else if (action.type === 'updateIsolatedMargin') {
      if (!integer(action.asset) || typeof action.isBuy !== 'boolean' || !Number.isSafeInteger(action.ntli))
        fail('malformed margin action');
    } else if (action.time !== undefined && !integer(action.time)) fail('malformed scheduled cancel');
    if (new Set(created).size !== created.length) fail('duplicate creation cloid');
    if (recoverHyperliquidSigner(frame, testnet) !== signer) fail('invalid action signature');
    const command = getCommandIndex();
    const bindings = [];
    if (getExpectedWrite) {
      const expected = getExpectedWrite();
      const record = expected?.record;
      if (!record || (record.afterCommand ?? 0) > command ||
          requestKey(method, url, body, callerIds) !== expected.key)
        fail('unexpected recorded write intent');
      const recorded = parse(record.requestBody).action;
      const boundReference = (wanted, actual) => {
        if (wanted === undefined && actual === undefined) return;
        if (!cloid(wanted) || !cloid(actual)) fail('cloid reference presence mismatch');
        const target = aliases.get(wanted.toLowerCase()) ?? wanted.toLowerCase();
        if (actual.toLowerCase() !== target || (callerIds.has(wanted) && actual !== wanted))
          fail('cloid reference intent mismatch');
      };
      if (action.type === 'order') {
        const expectedIds = recorded.orders.map((row) => row.c);
        if (!expectedIds.every(cloid) || new Set(expectedIds.map((id) => id.toLowerCase())).size !== expectedIds.length)
          fail('invalid recorded creation cloids');
        recorded.orders.forEach((wanted, index) => {
          const actual = action.orders[index].c;
          if (callerIds.has(wanted.c) && actual !== wanted.c) fail('caller cloid ownership mismatch');
          const prior = aliases.get(wanted.c.toLowerCase());
          if (prior && prior !== actual.toLowerCase()) fail('creation cloid alias changed');
          bindings.push([wanted.c.toLowerCase(), actual.toLowerCase()]);
        });
      } else if (action.type === 'cancelByCloid') {
        // The documented {asset, cloid} fields remain exact in requestKey.
        // Do not invent a generated-reference normalization for this path.
        recorded.cancels.forEach((wanted, index) => {
          if (wanted.cloid !== action.cancels[index].cloid) fail('cancel cloid intent mismatch');
        });
      } else if (action.type === 'modify' || action.type === 'batchModify') {
        const recordedRows = action.type === 'modify' ? [recorded] : recorded.modifies;
        const actualRows = action.type === 'modify' ? [action] : action.modifies;
        recordedRows.forEach((wanted, index) => boundReference(wanted.order.c, actualRows[index].order.c));
      }
    } else if (['cancelByCloid', 'modify', 'batchModify'].includes(action.type)) {
      fail('cloid reference lacks recorded intent');
    }
    const intent = `${hyperliquidActionHash({ action, nonce: 0, vaultAddress: vault })}`;
    for (const id of created) {
      const prior = creations.get(id);
      if (prior && (prior.command !== command || prior.intent !== intent)) fail('reused creation cloid');
    }
    if (nonces.has(frame.nonce) || (nonces.size === 100 && frame.nonce <= Math.min(...nonces)))
      fail('reused or evicted signer nonce');
    // Tape commits only when Responses has selected an eligible ACK. A signed
    // but premature attempt must not poison the later creation/alias bindings.
    const commit = () => {
      nonces.add(frame.nonce);
      if (nonces.size > 100) nonces.delete(Math.min(...nonces));
      for (const id of created) creations.set(id, { command, intent });
      for (const [expected, actual] of bindings) aliases.set(expected, actual);
    };
    if (getExpectedWrite) return commit;
    commit();
  };
}
