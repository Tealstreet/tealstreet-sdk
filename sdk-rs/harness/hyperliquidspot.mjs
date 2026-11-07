// Independent Hyperliquid L1 action signature oracle, using the public SDK key.
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
export function validateHyperliquidSpotRequest({ method, url, body, now = Date.now() }) {
  const u = new URL(url);
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
  virtualPrivateClock: true,
  recordAfterReady: true,
  ts: { module: '../../packages/safe-cex/src/__tests__/reference/hyperliquid/hyperliquidspot.exchange.ts', className: 'HyperliquidSpotExchange', name: 'hyperliquidspot' },
  publicOptions: { key: '' },
  recordOptions: { publicDataOnly: true, key: '' },
  replayCredentials: HYPERLIQUID_SPOT_REPLAY,
  initializeReplay: initializeHyperliquidSpot,
  validateRequest: validateHyperliquidSpotRequest,
  requestBodies: true,
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
    }
  },
  socketMatches(socket, text) {
    const channel = JSON.parse(text).channel;
    return ['pong', 'subscriptionResponse'].includes(channel) || socket.hyperSpotChannels?.has(channel) === true
      || (channel === 'spotState' && socket.hyperSpotChannels?.has('spotClearinghouseState') === true);
  },
  isWrite: (_method, url) => new URL(url).pathname === '/exchange',
  tsBooksSubscribed: (exchange, count) => Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('l2Book.')).length >= count,
};
