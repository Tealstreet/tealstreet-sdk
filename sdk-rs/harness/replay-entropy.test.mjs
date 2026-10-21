import assert from 'node:assert/strict';
import crypto, { createHash } from 'node:crypto';
import test from 'node:test';
import { createReplayEntropy } from './replay-entropy.mjs';

const seed = 'ab'.repeat(32);
const stream = (sha, sizes) => {
  const source = createReplayEntropy(sha);
  return Buffer.concat(sizes.map((size) => Buffer.from(source(size))));
};

test('tape-derived entropy repeats, differs between tapes and is chunk invariant', () => {
  assert.deepEqual(stream(seed, [128]), stream(seed, [128]));
  assert.notDeepEqual(stream(seed, [128]), stream('cd'.repeat(32), [128]));
  assert.deepEqual(stream(seed, [0, 1, 31, 64, 32]), stream(seed, [128]));
  const counter = Buffer.alloc(8);
  const first = createHash('sha256').update('tealstreet replay entropy v1\0')
    .update(seed, 'hex').update(counter).digest();
  assert.deepEqual(stream(seed, [32]), first, 'preserve the landed Bitget byte stream');
});

test('successive 21-byte nanoid entropy requests stay distinct and reproducible', () => {
  const bytes = stream(seed, Array(100).fill(21));
  const alphabet = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict';
  const ids = Array.from({ length: 100 }, (_, index) => [...bytes.subarray(index * 21, (index + 1) * 21)]
    .map((byte) => alphabet[byte & 63]).filter((char) => char !== '-' && char !== '_').join(''));
  assert.equal(new Set(ids).size, 100);
  assert.deepEqual(bytes, stream(seed, [2100]));
});

test('independent engines cannot consume each other or mutate returned storage', () => {
  const first = createReplayEntropy(seed), second = createReplayEntropy(seed);
  const head = first(64), expected = stream(seed, [100]);
  first(0);
  assert.deepEqual(Buffer.from(second(100)), expected);
  head.fill(255);
  assert.deepEqual(Buffer.from(first(36)), expected.subarray(64));
});

test('isolated entropy never replaces WebCrypto or Node crypto bindings', () => {
  const web = globalThis.crypto.getRandomValues, node = crypto.randomFillSync;
  createReplayEntropy(seed)(64);
  assert.equal(globalThis.crypto.getRandomValues, web);
  assert.equal(crypto.randomFillSync, node);
});

test('explicit identity source works without a global host crypto binding', () => {
  const descriptor = Object.getOwnPropertyDescriptor(globalThis, 'crypto');
  Object.defineProperty(globalThis, 'crypto', { configurable: true, value: undefined });
  try { assert.deepEqual(Buffer.from(createReplayEntropy(seed)(64)), stream(seed, [64])); }
  finally { Object.defineProperty(globalThis, 'crypto', descriptor); }
});

test('invalid seeds and unexpected call shapes fail closed', () => {
  for (const sha of [undefined, '', 'ab', 'zz'.repeat(32)]) assert.throws(() => createReplayEntropy(sha), /tape SHA/);
  const fill = createReplayEntropy(seed);
  for (const size of [new Uint8Array(2), -1, 0.5, Infinity, 65537]) assert.throws(() => fill(size), /byte length/);
  assert.throws(() => fill(4, 1), /byte length/);
});

test('each replay source leaves production signing crypto unchanged', () => {
  const web = globalThis.crypto.getRandomValues, node = crypto.randomFillSync;
  const first = createReplayEntropy(seed), second = createReplayEntropy(seed);
  assert.deepEqual(first(64), second(64));
  assert.equal(globalThis.crypto.getRandomValues, web);
  assert.equal(crypto.randomFillSync, node);
});
