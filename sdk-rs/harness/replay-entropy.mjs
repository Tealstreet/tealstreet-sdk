// Synthetic replay entropy, never imported by production hosts. Wasm getrandom
// uses Web Crypto on Node 22 and node:crypto on hosts without Web Crypto.
import crypto, { createHash } from 'node:crypto';

export const installReplayEntropy = (tapeSha256, { webCrypto = globalThis.crypto, nodeCrypto = crypto } = {}) => {
  if (!/^[a-f0-9]{64}$/.test(tapeSha256 ?? '')) throw new Error('Replay entropy requires a tape SHA-256');
  let counter = 0n, block = Buffer.alloc(0), offset = 0;
  const fill = function (bytes) {
    // These are the exact one-argument Uint8Array bindings used by getrandom.
    // Fail rather than silently emulate another crypto API call shape.
    if (arguments.length !== 1 || !(bytes instanceof Uint8Array)) throw new TypeError('Expected one Uint8Array');
    for (let index = 0; index < bytes.byteLength; index++) {
      if (offset === block.length) {
        const sequence = Buffer.alloc(8);
        sequence.writeBigUInt64BE(counter++);
        block = createHash('sha256').update('tealstreet replay entropy v1\0')
          .update(tapeSha256, 'hex').update(sequence).digest();
        offset = 0;
      }
      bytes[index] = block[offset++];
    }
    return bytes;
  };
  const bindings = [[webCrypto, 'getRandomValues'], [nodeCrypto, 'randomFillSync']]
    .filter(([target, name]) => typeof target?.[name] === 'function');
  if (!bindings.length) throw new Error('No host entropy binding available');
  const originals = bindings.map(([target, name]) => [target, name, target[name]]);
  for (const [target, name] of bindings) target[name] = fill;
  return () => { for (const [target, name, original] of originals) target[name] = original; };
};

export const initializeBitgetReplay = (impl, _options, { isPrivate, tapeSha256 } = {}) => {
  if (impl === 'rust' && isPrivate) return installReplayEntropy(tapeSha256);
};
