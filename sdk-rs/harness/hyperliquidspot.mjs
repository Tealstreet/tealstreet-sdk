// Independent Hyperliquid L1 action signature oracle, using the public SDK key.
import { uniqueRequestHeaders } from './protocol-headers.mjs';
import { spotRequestKey } from './spot-request-key.mjs';
import { createRequire } from 'node:module';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
export const HYPERLIQUID_SPOT_REPLAY = {
  key: '0x14791697260e4c9a71f18484c9f997b308e59325',
  secret: '0x0123456789012345678901234567890123456789012345678901234567890123',
  applicationId: '0x14791697260e4c9a71f18484c9f997b308e59325',
};
let ethers, encode;
export function initializeHyperliquidSpot() {
  if (ethers) return;
  ({ ethers } = require('ethers'));
  ({ encode } = require('@msgpack/msgpack'));
}
export function validateHyperliquidSpotRequest({ method, url, body, headers, now = Date.now() }) {
  const u = new URL(url);
  const h = uniqueRequestHeaders(headers);
  if (Object.keys(h).some((name) => ['authorization','coinbase-authorization','cookie','api-key','api-sign'].includes(name))) throw Error('Hyperliquid Spot unexpected credentials');
  if (u.username || u.password || u.hash || u.search) throw Error('Hyperliquid Spot unexpected URL fields');
  if (!['https://api.hyperliquid.xyz', 'https://api.hyperliquid-testnet.xyz'].includes(u.origin) || method !== 'POST')
    throw Error('Hyperliquid Spot request must use upstream POST');
  const frame = JSON.parse(body);
  if (u.pathname === '/info') {
    if (frame.type === 'spotClearinghouseState' && frame.user?.toLowerCase() !== HYPERLIQUID_SPOT_REPLAY.applicationId)
      throw Error('Hyperliquid Spot account identity mismatch');
    return;
  }
  if (u.pathname !== '/exchange' || !frame.action || !Number.isSafeInteger(frame.nonce) || Math.abs(now - frame.nonce) > 60_000)
    throw Error('Hyperliquid Spot malformed action or nonce');
  initializeHyperliquidSpot();
  const encoded = encode(frame.action), suffix = Buffer.alloc(frame.vaultAddress ? 29 : 9);
  suffix.writeBigUInt64BE(BigInt(frame.nonce));
  if (frame.vaultAddress) { suffix[8] = 1; Buffer.from(frame.vaultAddress.slice(2), 'hex').copy(suffix, 9); }
  const connectionId = ethers.keccak256(Buffer.concat([encoded, suffix]));
  const signer = ethers.verifyTypedData(
    { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: ethers.ZeroAddress },
    { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] },
    { source: u.hostname.includes('testnet') ? 'b' : 'a', connectionId }, frame.signature,
  );
  if (signer.toLowerCase() !== HYPERLIQUID_SPOT_REPLAY.key) throw Error('Hyperliquid Spot invalid action signature');
  if (frame.action.orders?.some((order) => order.a < 10000 || order.r !== false) || frame.action.builder)
    throw Error('Hyperliquid Spot action must use spot indices, no reduce-only or builder');
}
export const hyperliquidspot = {
  rust: 'hyperliquidspot',
  recordAfterReady: true,
  ts: { module: '../../packages/safe-cex/src/exchanges/hyperliquid/hyperliquidspot.exchange.ts', className: 'HyperliquidSpotExchange', name: 'hyperliquidspot' },
  publicOptions: { key: '' },
  recordOptions: { publicDataOnly: true, key: '' },
  replayCredentials: HYPERLIQUID_SPOT_REPLAY,
  initializeReplay: initializeHyperliquidSpot,
  validateRequest: validateHyperliquidSpotRequest,
  requestBodies: true,
  requestKey(method, url, body, callerIds) {
    let generatedOrder = false;
    return spotRequestKey(method, url, body, callerIds, (value) => {
      if (method === 'POST' && new URL(url).pathname === '/exchange') {
        generatedOrder = value.action?.type === 'order';
        const { nonce: _nonce, signature: _signature, ...action } = value; return action;
      }
      return value;
    }, (key, value) => method === 'POST' && new URL(url).pathname === '/exchange'
      && generatedOrder && key === 'c' && typeof value === 'string' && value.length === 34 && /^0x[0-9a-fA-F]+$/.test(value));
  },
  compareWriteOrder: true,
  compareFillPrices: true,
  compareBalanceCoins: true,
  compareBalanceCurrencies: true,
  isPrivateRecord: (record) => record.type === 'ws' && /"(?:userFills|spotClearinghouseState|spotState|orderUpdates)"/.test(record.text ?? ''),
  onSocketSend(socket, text) {
    const frame = JSON.parse(text);
    if (frame.method === 'subscribe' && frame.subscription?.type) {
      socket.hyperSpotChannels ??= new Set();
      socket.hyperSpotChannels.add(frame.subscription.type);
    } else if (frame.method === 'unsubscribe') {
      socket.hyperSpotChannels?.delete(frame.subscription?.type);
    }
  },
  socketMatches(socket, text) {
    const channel = JSON.parse(text).channel;
    return ['pong', 'subscriptionResponse'].includes(channel) || socket.hyperSpotChannels?.has(channel) === true
      || (channel === 'spotState' && socket.hyperSpotChannels?.has('spotClearinghouseState') === true);
  },
  createSocketAudit: createHyperliquidSpotSocketAudit,
  isWrite: (_method, url) => new URL(url).pathname === '/exchange',
  tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
};

export function createHyperliquidSpotSocketAudit(_impl, { isPrivate } = {}) {
  const sockets = new Map(), failures = [];
  const state = (socket) => {
    if (!sockets.has(socket)) sockets.set(socket, { closed: false, topics: new Map() });
    return sockets.get(socket);
  };
  // The server echoes explicit defaults omitted by either client.
  const key = (topic) => JSON.stringify(Object.fromEntries(Object.entries(topic)
    .filter(([name,value]) => !(topic.type === 'l2Book' && (name === 'mantissa' && value === null || name === 'fast' && value === false || name === 'nSigFigs' && value === null)))
    .sort(([a],[b]) => a.localeCompare(b))));
  const checkUrl = (url) => {
    const u = new URL(url);
    if (!['wss://api.hyperliquid.xyz','wss://api.hyperliquid-testnet.xyz'].includes(u.origin) || u.pathname !== '/ws' || u.search || u.hash || u.username || u.password) throw Error('Hyperliquid Spot invalid socket URL');
  };
  return {
    connected(url, socket) { checkUrl(url); state(socket); },
    closed(_url, socket) { state(socket).closed = true; },
    sent(url, text, socket) {
      checkUrl(url);
      const own = state(socket), frame = JSON.parse(text);
      if (own.closed) throw Error('Hyperliquid Spot send on closed socket');
      if (frame.method === 'ping') return;
      const topic = frame.subscription;
      if (!['subscribe','unsubscribe'].includes(frame.method) || !topic || !['allMids','l2Book','candle','userFills','orderUpdates','spotClearinghouseState','spotState','notification','userNonFundingLedgerUpdates'].includes(topic.type)) throw Error('Hyperliquid Spot unsupported subscription');
      if (topic.type === 'l2Book' && topic.nSigFigs !== undefined && ![null,2,3,4,5].includes(topic.nSigFigs)) throw Error('Hyperliquid Spot unsupported book precision');
      if (topic.user !== undefined && (typeof topic.user !== 'string' || topic.user.toLowerCase() !== HYPERLIQUID_SPOT_REPLAY.applicationId)) throw Error('Hyperliquid Spot wrong socket account');
      if (['userFills','orderUpdates','spotClearinghouseState','spotState','notification','userNonFundingLedgerUpdates'].includes(topic.type) && topic.user === undefined) throw Error('Hyperliquid Spot missing socket account');
      if (frame.method === 'subscribe') own.topics.set(key(topic), topic); else own.topics.delete(key(topic));
    },
    received(_url, text, socket) {
      const own = state(socket), frame = JSON.parse(text);
      if (own.closed) { failures.push('Hyperliquid Spot data on closed socket'); return; }
      if (frame.channel === 'pong') return;
      if (frame.channel === 'subscriptionResponse') {
        if (frame.data?.method === 'subscribe' && !own.topics.has(key(frame.data.subscription ?? {}))) failures.push('Hyperliquid Spot unrequested subscription acknowledgement');
        return;
      }
      const data = frame.data, channel = frame.channel === 'spotState' ? 'spotClearinghouseState' : frame.channel;
      if (data?.user !== undefined && (typeof data.user !== 'string' || data.user.toLowerCase() !== HYPERLIQUID_SPOT_REPLAY.applicationId)) failures.push('Hyperliquid Spot data changed account');
      const subscribed = [...own.topics.values()].some((topic) => (topic.type === channel || (channel === 'spotClearinghouseState' && topic.type === 'spotState'))
        && (channel !== 'l2Book' || topic.coin === data?.coin)
        && (channel !== 'candle' || topic.coin === data?.s && topic.interval === data?.i));
      if (!subscribed) failures.push('Hyperliquid Spot data without own subscription');
    },
    ready: () => !isPrivate || [...sockets.values()].some((s) => !s.closed && [...s.topics.values()].some((t) => t.user)),
    errors: () => [...new Set(failures)],
  };
}
