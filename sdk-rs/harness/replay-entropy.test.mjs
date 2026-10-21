import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import test from 'node:test';
import { initializeBitgetReplay, installReplayEntropy } from './replay-entropy.mjs';

const seed = 'ab'.repeat(32);
const sources = () => ({ webCrypto: { getRandomValues() {} }, nodeCrypto: { randomFillSync() {} } });
const stream = (sha, sizes) => {
  const targets = sources();
  const restore = installReplayEntropy(sha, targets);
  try {
    return Buffer.concat(sizes.map((size) => Buffer.from(targets.webCrypto.getRandomValues(new Uint8Array(size)))));
  } finally { restore(); }
};

test('tape-derived entropy repeats across initialization and differs between tapes', () => {
  assert.deepEqual(stream(seed, [128]), stream(seed, [128]));
  assert.notDeepEqual(stream(seed, [128]), stream('cd'.repeat(32), [128]));
  assert.deepEqual(stream(seed, [0, 1, 31, 64, 32]), stream(seed, [128]));
});

test('successive 21-byte nanoid entropy requests stay distinct and reproducible', () => {
  const bytes = stream(seed, Array(100).fill(21));
  const alphabet = 'useandom-26T198340PX75pxJACKVERYMINDBUSHWOLF_GQZbfghjklqvwyzrict';
  const ids = Array.from({ length: 100 }, (_, index) => [...bytes.subarray(index * 21, (index + 1) * 21)]
    .map((byte) => alphabet[byte & 63]).filter((char) => char !== '-' && char !== '_').join(''));
  assert.equal(new Set(ids).size, 100);
  assert.deepEqual(bytes, stream(seed, [2100]));
});

test('Web Crypto and Node fallback share a stream without writing outside a view', () => {
  const targets = sources(), restore = installReplayEntropy(seed, targets);
  try {
    const bytes = new Uint8Array(70).fill(255), view = bytes.subarray(3, 67);
    assert.equal(targets.webCrypto.getRandomValues(view), view);
    assert.deepEqual(Buffer.from(view), stream(seed, [64]));
    assert.deepEqual([...bytes.subarray(0, 3)], [255, 255, 255]);
    assert.deepEqual([...bytes.subarray(67)], [255, 255, 255]);
    const tail = Buffer.alloc(36);
    assert.equal(targets.nodeCrypto.randomFillSync(tail), tail);
    assert.deepEqual(Buffer.concat([Buffer.from(view), tail]), stream(seed, [100]));
  } finally { restore(); }
});

test('restoring both bindings preserves their original functions', () => {
  const targets = sources();
  const web = targets.webCrypto.getRandomValues, node = targets.nodeCrypto.randomFillSync;
  installReplayEntropy(seed, targets)();
  assert.equal(targets.webCrypto.getRandomValues, web);
  assert.equal(targets.nodeCrypto.randomFillSync, node);
});

test('missing Web Crypto still permits the Node fallback', () => {
  const nodeCrypto = { randomFillSync() {} };
  const restore = installReplayEntropy(seed, { webCrypto: null, nodeCrypto });
  try { assert.deepEqual(nodeCrypto.randomFillSync(Buffer.alloc(64)), stream(seed, [64])); }
  finally { restore(); }
});

test('invalid seeds, unavailable bindings and unexpected call shapes fail closed', () => {
  for (const sha of [undefined, '', 'ab', 'zz'.repeat(32)]) assert.throws(() => installReplayEntropy(sha, sources()), /tape SHA/);
  assert.throws(() => installReplayEntropy(seed, { webCrypto: null, nodeCrypto: null }), /No host entropy/);
  const targets = sources(), restore = installReplayEntropy(seed, targets);
  try {
    assert.throws(() => targets.webCrypto.getRandomValues(new Float64Array(2)), /Uint8Array/);
    assert.throws(() => targets.nodeCrypto.randomFillSync(Buffer.alloc(4), 1), /one Uint8Array/);
  } finally { restore(); }
});

test('family initializer injects only private Rust replay and restores real host bindings', () => {
  const web = globalThis.crypto.getRandomValues, node = crypto.randomFillSync;
  for (const [impl, isPrivate] of [['ts', true], ['rust', false], ['ts', false]]) {
    assert.equal(initializeBitgetReplay(impl, {}, { isPrivate, tapeSha256: seed }), undefined);
    assert.equal(globalThis.crypto.getRandomValues, web);
    assert.equal(crypto.randomFillSync, node);
  }
  const restore = initializeBitgetReplay('rust', {}, { isPrivate: true, tapeSha256: seed });
  try {
    assert.deepEqual(Buffer.from(globalThis.crypto.getRandomValues(new Uint8Array(64))), stream(seed, [64]));
  } finally { restore(); }
  assert.equal(globalThis.crypto.getRandomValues, web);
  assert.equal(crypto.randomFillSync, node);
});
