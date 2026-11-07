// Offline test-wire construction, separate from the verifier and candidates.
import { createRequire } from 'node:module';
import { HYPERLIQUID_SPOT_REPLAY } from './hyperliquidspot.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
export async function signHyperliquidFrame(action, { nonce = Date.now(), vaultAddress, expiresAfter, testnet = false } = {}) {
  const { ethers } = require('ethers'), { encode } = require('@msgpack/msgpack');
  const timestamp = Buffer.alloc(8); timestamp.writeBigUInt64BE(BigInt(nonce));
  const prepare = (item) => {
    if (Array.isArray(item)) return item.map(prepare);
    if (item && typeof item === 'object') return Object.fromEntries(Object.entries(item).map(([key, value]) => [key, prepare(value)]));
    return typeof item === 'number' && Number.isSafeInteger(item) && (item > 4294967295 || item < -2147483648) ? BigInt(item) : item;
  };
  const parts = [encode(prepare(action), { useBigInt64: true }), timestamp,
    vaultAddress ? Buffer.concat([Buffer.from([1]), Buffer.from(vaultAddress.slice(2), 'hex')]) : Buffer.from([0])];
  if (expiresAfter !== undefined) {
    const expiry = Buffer.alloc(8); expiry.writeBigUInt64BE(BigInt(expiresAfter));
    parts.push(Buffer.from([0]), expiry);
  }
  const signature = await new ethers.Wallet(HYPERLIQUID_SPOT_REPLAY.secret).signTypedData(
    { name: 'Exchange', version: '1', chainId: 1337, verifyingContract: ethers.ZeroAddress },
    { Agent: [{ name: 'source', type: 'string' }, { name: 'connectionId', type: 'bytes32' }] },
    { source: testnet ? 'b' : 'a', connectionId: ethers.keccak256(Buffer.concat(parts)) },
  );
  const { r, s, v } = ethers.Signature.from(signature);
  return { action, nonce, ...(vaultAddress === undefined ? {} : { vaultAddress }),
    ...(expiresAfter === undefined ? {} : { expiresAfter }), signature: { r, s, v } };
}
