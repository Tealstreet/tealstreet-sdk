import { test } from 'node:test';
import assert from 'node:assert/strict';
import { retainedMemory } from './memory.mjs';

test('retained frame backing buffers are included', () => {
  const before = process.memoryUsage();
  const frame = new ArrayBuffer(16 * 1024 * 1024);
  const after = process.memoryUsage();
  assert(retainedMemory(before, after) >= frame.byteLength);
});
test('linear wasm memory is counted exactly once', () => {
  const before = process.memoryUsage();
  const memory = new WebAssembly.Memory({ initial: 16 });
  const bytes = memory.buffer.byteLength;
  const after = process.memoryUsage();
  assert.equal(after.arrayBuffers - before.arrayBuffers, 0);
  assert.equal(retainedMemory(before, after, bytes), after.heapUsed - before.heapUsed + bytes);
});
