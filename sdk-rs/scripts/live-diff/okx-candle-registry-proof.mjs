#!/usr/bin/env node
// Focused recorded-prefix proof; this does not qualify the full 60-second venue cell.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { deserialize, serialize } from 'node:v8';
import { differences } from './compare.mjs';

const topic = 'candle.1m.BTC-USDT-SWAP';
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const [mode, first, second] = process.argv.slice(2);
if (mode === 'prefix') {
  const bytes = readFileSync(first);
  assert.equal(digest(bytes), '39615880a701fea83d1b66e9b14fe15f118a8738f36e8fdb15478bf3debf49fd');
  const bundle = deserialize(bytes);
  const end = bundle.tape.frames.findIndex((frame) => {
    const text = Buffer.from(frame.base64 || '', 'base64').toString();
    return text.includes('"channel":"tickers"') && text.includes('"data"');
  });
  assert.equal(end, 104, 'original prefix ends after the first ticker publishes candle statistics');
  bundle.tape.frames = bundle.tape.frames.slice(0, end + 1);
  bundle.tape.durationMs = bundle.tape.frames.at(-1).atMs;
  writeFileSync(second, serialize(bundle));
  console.log('Wrote exact original 105-frame prefix, with unchanged frame times and HTTP bytes');
} else if (mode === 'check') {
  function load(path) {
    const report = JSON.parse(readFileSync(path));
    assert.equal(report.nativeDrainVerified, true);
    assert.equal(report.comparisonsProvisional, false);
    for (const implementation of ['ts', 'rust']) {
      assert.equal(report.repeatability[implementation].combined.status, 'PASS');
    }
    const rows = report.runs.map((run) => deserialize(readFileSync(run.fixture)));
    assert.deepEqual(rows[0], rows[1], 'complete TS repeat');
    assert.deepEqual(rows[2], rows[3], 'complete native repeat');
    for (const run of report.runs) {
      assert.equal(run.receipt.delivered,105);
      assert.equal(run.receipt.dropped,0);
      assert.equal(run.receipt.inputHash,'e185651088a29adead92d1287d924e3fe41d8ccb4113c28e0ea9b279f574e9f5');
      assert.equal(run.receipt.applicationDrainVerified,true);
      assert.equal(run.requests.length,2);
      assert(run.requests.every((request) => request.cacheHit && request.status === 200));
      if (run.implementation === 'rust') {
        assert.equal(run.receipt.nativeTransportVerified,true);
        assert.equal(run.receipt.processed.pendingJobs,0);
        assert.equal(run.receipt.processed.rawTasks.version,1);
        for (const key of ['unknown','finite','ready','blocking']) assert.equal(run.receipt.processed.rawTasks[key],0);
      }
    }
    return { report, rows };
  }
  const red = load(first);
  const green = load(second);
  const expected = '$.2.data.messageHandlerStats.business.candle.1m.BTC-USDT-SWAP.hadHandlers';
  assert.deepEqual(red.report.parity.combined.diffs, [{ path: expected, ts: true, rust: false }]);
  assert.equal(green.report.parity.combined.status, 'PASS');
  const entry = (row) => row.outputs[2].data.messageHandlerStats.business[topic];
  assert.equal(entry(red.rows[0]).hadHandlers, true);
  assert.equal(entry(red.rows[2]).hadHandlers, false);
  assert.equal(entry(green.rows[2]).hadHandlers, true);
  assert.deepEqual(green.rows[0], red.rows[0], 'maintained source observations unchanged');
  assert.deepEqual(green.rows[2].outputs, green.rows[0].outputs, 'emitted values and own-property presence equal source');
  const stateCandle = '$.messageHandlerStats.business.candle.1m.BTC-USDT-SWAP.hadHandlers';
  const inheritedState = {path:'$.starkConfig', ts:'<undefined>', rust:'<absent>'};
  assert.deepEqual(differences(red.rows[0].state, red.rows[2].state), [
    {path:stateCandle, ts:true, rust:false}, inheritedState,
  ]);
  assert.deepEqual(differences(green.rows[0].state, green.rows[2].state), [inheritedState]);
  function keyDeltas(a, b, path='$', seen=new WeakMap(), out=[]) {
    if (!a || typeof a !== 'object' || !b || typeof b !== 'object' || seen.get(a) === b) return out;
    seen.set(a,b);
    const x=Reflect.ownKeys(a), y=Reflect.ownKeys(b);
    if (x.length !== y.length || x.some((key,index) => key !== y[index])) out.push({path,ts:x,rust:y});
    for (const key of x) keyDeltas(a[key],b[key],path+'.'+key,seen,out);
    return out;
  }
  for (const phase of [red,green]) {
    assert.deepEqual(keyDeltas(phase.rows[0],phase.rows[1]), [], 'ordered source repeat');
    assert.deepEqual(keyDeltas(phase.rows[2],phase.rows[3]), [], 'ordered native repeat');
  }
  assert.deepEqual(keyDeltas(red.rows[0],green.rows[0]), [], 'source property order unchanged');
  const inheritedKeys=keyDeltas(red.rows[0],red.rows[2]);
  assert.equal(inheritedKeys.length,2041, 'unchanged Store/Ticker/Market order plus state presence FIXes');
  assert.deepEqual(keyDeltas(green.rows[0],green.rows[2]), inheritedKeys, 'each exact pre-existing key-list delta unchanged');
  assert.equal(green.report.runs[2].receipt.inputHash, red.report.runs[2].receipt.inputHash);
  assert.notEqual(green.report.addonSha256, red.report.addonSha256, 'different compiled production addons');
  console.log('PASS: compiled candle RED -> GREEN on exact 105-frame prefix, emitted value/presence equality and strict drains; inherited final-state starkConfig presence and all 2041 key-list deltas pinned unchanged. No ordered whole-output, full-state or full-minute parity claim.');
} else {
  throw new Error('Usage: okx-candle-registry-proof.mjs prefix INPUT.bundle.v8 OUTPUT.bundle.v8 | check RED.json GREEN.json');
}
