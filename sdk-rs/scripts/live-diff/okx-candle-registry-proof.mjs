#!/usr/bin/env node
// Focused recorded-prefix proof; this does not qualify the full 60-second venue cell.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { deserialize, serialize } from 'node:v8';

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
    assert.ok(report.runs.every((run) => run.receipt.delivered === 105));
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
  assert.deepEqual(green.rows[2], green.rows[0], 'complete candidate output and final state equal source');
  assert.equal(green.report.runs[2].receipt.inputHash, red.report.runs[2].receipt.inputHash);
  assert.notEqual(green.report.addonSha256, red.report.addonSha256, 'different compiled production addons');
  console.log('PASS: compiled RED -> GREEN on exact recorded 105-frame prefix; full venue coverage not claimed');
} else {
  throw new Error('Usage: okx-candle-registry-proof.mjs prefix INPUT.bundle.v8 OUTPUT.bundle.v8 | check RED.json GREEN.json');
}
