import { createHash } from 'node:crypto';

// WebCrypto digests resolve from Node's thread pool, so on a virtual replay
// clock the turn they land in follows wall time. Hash on the calling thread
// (same bytes) so each digest settles in that turn's microtasks.
const DIGESTS = { 'SHA-1': 'sha1', 'SHA-256': 'sha256', 'SHA-384': 'sha384', 'SHA-512': 'sha512' };
export const installSynchronousDigest = (subtle = globalThis.crypto.subtle) => {
  const { digest } = subtle;
  subtle.digest = function (algorithm, data) {
    const name = DIGESTS[(typeof algorithm === 'string' ? algorithm : algorithm?.name)?.toUpperCase()];
    if (!name) return digest.call(this, algorithm, data);
    const bytes = ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength) : new Uint8Array(data);
    const hash = createHash(name).update(bytes).digest();
    return Promise.resolve(hash.buffer.slice(hash.byteOffset, hash.byteOffset + hash.length));
  };
};
