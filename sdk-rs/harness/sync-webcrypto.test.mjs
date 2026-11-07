import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { installSynchronousDigest } from './sync-webcrypto.mjs';

// Settled before the current turn's microtasks drain: no thread-pool wait.
const settlesInTurn = async (promise) => {
  let settled = false;
  promise.then(() => { settled = true; });
  for (let step = 0; step < 4; step++) await Promise.resolve();
  return settled;
};

test('replay WebCrypto digests settle in the calling turn with the same bytes', async () => {
  const { subtle } = globalThis.crypto;
  const message = new TextEncoder().encode('replay message');
  assert.equal(await settlesInTurn(subtle.digest('SHA-512', message)), false, 'the native digest waits on the thread pool');
  try {
    installSynchronousDigest(subtle);
    for (const [name, node] of [['SHA-256', 'sha256'], [{ name: 'SHA-512' }, 'sha512']]) {
      const digest = subtle.digest(name, message);
      assert.equal(await settlesInTurn(digest), true);
      assert.deepEqual(Buffer.from(await digest), createHash(node).update(message).digest());
    }
  } finally {
    delete subtle.digest;
  }
});
