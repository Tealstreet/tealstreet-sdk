import { test } from 'node:test';
import assert from 'node:assert/strict';
import { retainedMemory } from './memory.mjs';

test('retained frame backing buffers are included', () => {
  const frame = new ArrayBuffer(16 * 1024 * 1024);
  // GC may free heap while a transferred frame remains retained.
  const before = { heapUsed: 100_000, arrayBuffers: 400_000 };
  const after = { heapUsed: 90_000, arrayBuffers: 400_000 + frame.byteLength };
  assert.equal(retainedMemory(before, after), frame.byteLength - 10_000);
});
test('linear wasm memory is counted exactly once', () => {
  const before = process.memoryUsage();
  const memory = new WebAssembly.Memory({ initial: 16 });
  const bytes = memory.buffer.byteLength;
  const after = process.memoryUsage();
  assert.equal(after.arrayBuffers - before.arrayBuffers, 0);
  assert.equal(retainedMemory(before, after, bytes), after.heapUsed - before.heapUsed + bytes);
});
