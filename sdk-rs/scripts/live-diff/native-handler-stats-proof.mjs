#!/usr/bin/env node
// Focused Apex startup witness: exact recorded bytes, strict drains and outputs.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { deserialize } from 'node:v8';
import { fileURLToPath } from 'node:url';
import { differences } from './compare.mjs';
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
// These are OPEN order-only FIXes owned separately. Pin each exact old/new
// observation rather than sorting keys or claiming ordered output parity.
const storeTS = ['latency','balance','tickers','displayedPositions','displayedMargin','ordersRisk','activeChasers','activeTWAPs','messageHandlerStats','requestStats','fetchedLeverage','loaded','options','updateIds','socketHealth','zkLinkConfig'];
const storeRust = ['latency','balance','tickers','zkLinkConfig','displayedPositions','displayedMargin','ordersRisk','activeChasers','activeTWAPs','loaded','options','updateIds','messageHandlerStats','requestStats','fetchedLeverage','socketHealth'];
const tickerTS = ['id','symbol','bid','ask','last','volume','quoteVolume','mark','index','percentage','openInterest','fundingRate','fundingTime'];
const tickerRust = ['id','symbol','bid','ask','last','mark','index','percentage','openInterest','fundingRate','fundingTime','volume','quoteVolume'];
const expectedOrder = [];
for (let output = 0; output < 2; output++) {
  expectedOrder.push({path:`$.${output}.data`,ts:storeTS,rust:storeRust});
  for (let row = 0; row < 89; row++) expectedOrder.push({path:`$.${output}.data.tickers.${row}`,ts:tickerTS,rust:tickerRust});
}
function orderDeltas(a, b, path='$', seen=new WeakMap(), out=[]) {
  if (!a || typeof a !== 'object') return out;
  if (seen.get(a) === b) return out;
  seen.set(a, b);
  const x=Reflect.ownKeys(a), y=Reflect.ownKeys(b);
  if (x.length !== y.length || x.some((key,index) => key !== y[index])) out.push({path,ts:x,rust:y});
  for (const key of x) orderDeltas(a[key],b[key],path+'.'+key,seen,out);
  return out;
}
const red = read(redPath), green = read(greenPath);
assert.deepEqual(red.report.parity.combined.diffs, [{path:'$.1.data.messageHandlerStats.public.pong.hadHandlers', ts:true, rust:false}]);
assert.equal(red.outputs['ts-1'][1].data.messageHandlerStats.public.pong.hadHandlers, true);
assert.equal(red.outputs['rust-1'][1].data.messageHandlerStats.public.pong.hadHandlers, false);
assert.deepEqual(green.report.parity.combined.diffs, []);
assert.deepEqual(green.outputs['ts-1'], green.outputs['rust-1'], 'all values and own-property presence');
assert.deepEqual(differences(red.outputs['ts-1'],red.outputs['rust-1']),red.report.parity.combined.diffs);
for (const phase of [red,green]) assert.deepEqual(orderDeltas(phase.outputs['ts-1'],phase.outputs['rust-1']),expectedOrder);
assert.equal(expectedOrder.length,180);
completeEqual(red.outputs['ts-1'], green.outputs['ts-1']);
console.log('PASS: recorded three-frame Apex pong value RED -> GREEN, complete value/presence equality and strict drains; all 180 inherited order-only FIXes unchanged. No ordered whole-output or full-minute parity claim.');
