#!/usr/bin/env node
// Focused Apex startup witness: exact recorded bytes, strict drains and outputs.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { deserialize } from 'node:v8';
import { fileURLToPath } from 'node:url';
const [redPath, greenPath, bundlePath] = process.argv.slice(2);
assert(redPath && greenPath && bundlePath, 'usage: proof RED.json GREEN.json original.bundle.v8');
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
assert.equal(sha(readFileSync(bundlePath)), '4acfab66eea4b2e2df457b477f2d1384a8e3dcdea3bb9e286a4cce4be1ba1aaf');
const fixture = JSON.parse(readFileSync(fileURLToPath(new URL('../../safe-cex-venue-apexomni/tests/recorded-pong.json', import.meta.url))));
assert.equal(Buffer.from(fixture.frame.base64, 'base64').toString(), fixture.payload);
assert.equal(sha(Buffer.from(fixture.payload)), fixture.sha256);
function exactKeys(a, b, seen = new WeakMap()) {
  if (!a || typeof a !== 'object') return;
  if (seen.get(a) === b) return;
  seen.set(a, b);
  assert.deepEqual(Reflect.ownKeys(a), Reflect.ownKeys(b), 'ordered own properties');
  for (const key of Reflect.ownKeys(a)) exactKeys(a[key], b[key], seen);
}
function completeEqual(a, b) {
  assert.deepEqual(a, b);
  exactKeys(a, b);
}
function read(path) {
  const report = JSON.parse(readFileSync(path));
  assert.equal(report.exchange, 'apexomni');
  assert.equal(report.nativeDrainVerified, true);
  assert.equal(report.comparisonsProvisional, false);
  assert.equal(report.runs.length, 4);
  const outputs = {};
  for (const run of report.runs) {
    assert.equal(run.receipt.delivered, 3);
    assert.equal(run.receipt.dropped, 0);
    assert.equal(run.receipt.inputHash, '15d262924e5f9ddc5d7951f143f9589068bff48c3cd0c88215fc154440e95e5b');
    assert.equal(run.receipt.applicationDrainVerified, true);
    assert.equal(run.requests.length, 1);
    assert(run.requests.every((r) => r.cacheHit && r.status === 200));
    if (run.implementation === 'rust') {
      assert.equal(run.receipt.nativeTransportVerified, true);
      const receipt = run.receipt.processed;
      assert.equal(receipt.pendingJobs, 0);
      assert.equal(receipt.rawTasks.version, 1);
      for (const key of ['unknown', 'finite', 'ready', 'blocking']) assert.equal(receipt.rawTasks[key], 0);
    }
    outputs[run.label] = deserialize(readFileSync(run.fixture)).outputs;
    assert.equal(outputs[run.label].length, 2);
  }
  completeEqual(outputs['ts-1'], outputs['ts-2']);
  completeEqual(outputs['rust-1'], outputs['rust-2']);
  return { report, outputs };
}
const red = read(redPath), green = read(greenPath);
assert.deepEqual(red.report.parity.combined.diffs, [{path:'$.1.data.messageHandlerStats.public.pong.hadHandlers', ts:true, rust:false}]);
assert.equal(red.outputs['ts-1'][1].data.messageHandlerStats.public.pong.hadHandlers, true);
assert.equal(red.outputs['rust-1'][1].data.messageHandlerStats.public.pong.hadHandlers, false);
assert.deepEqual(green.report.parity.combined.diffs, []);
completeEqual(green.outputs['ts-1'], green.outputs['rust-1']);
completeEqual(red.outputs['ts-1'], green.outputs['ts-1']);
console.log('PASS: recorded three-frame Apex binding RED -> whole-output GREEN, ordered own properties and strict drains');
