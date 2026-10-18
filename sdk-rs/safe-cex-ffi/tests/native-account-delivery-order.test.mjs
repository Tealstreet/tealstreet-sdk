import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { deserialize, serialize } from 'node:v8';
import { gunzipSync } from 'node:zlib';
import test from 'node:test';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const addon = process.env.LIVE_DIFF_ADDON ?? fileURLToPath(new URL('../dist/node/safe_cex_ffi.node', import.meta.url));

test('recorded Binance frames preserve combined Store/event order across eight fresh native accounts', { timeout: 600000 }, async (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'safe-cex-delivery-order-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const bundle = JSON.parse(gunzipSync(readFileSync(new URL('./fixtures/live-public/binance-delivery-prefix.json.gz', import.meta.url))));
  bundle.rest = bundle.rest.map(([key, { base64, ...response }]) => [key, { ...response, bytes: Buffer.from(base64, 'base64') }]);
  assert.equal(bundle.tape.frames.length, 12);
  const replay = join(directory, 'prefix.bundle.v8');
  writeFileSync(replay, serialize(bundle));
  let expected;
  for (let run = 0; run < 4; run++) {
    const output = join(directory, `run-${run}.json`);
    const child = spawn(process.execPath, [
      join(root, 'sdk-rs/scripts/live-diff/streams.mjs'), '--venue', 'binance',
      '--replay', replay, '--addon', addon, '--timeout-ms', '60000', '--output', output,
    ], { cwd: root, stdio: ['ignore', 'pipe', 'pipe'] });
    let log = '';
    child.stdout.on('data', (bytes) => { log += bytes; });
    child.stderr.on('data', (bytes) => { log += bytes; });
    const kill = () => child.kill('SIGTERM');
    t.after(kill);
    const code = await new Promise((resolve, reject) => {
      child.once('error', reject);
      child.once('close', resolve);
    });
    // Existing qualified metadata differences can make streams.mjs exit 1.
    // They do not excuse incomplete parser/Jobs receipts or native order diffs.
    // General raw-runtime-task classification is outside these legacy receipts.
    assert.ok(code === 0 || code === 1, log);
    const report = JSON.parse(readFileSync(output));
    assert.equal(report.nativeDrainVerified, true, log);
    assert.equal(report.comparisonsProvisional, false, log);
    assert.equal(report.repeatability.ts.combined.status, 'PASS', 'the reference repeats exactly');
    assert.equal(report.repeatability.rust.combined.status, 'PASS', JSON.stringify(report.repeatability.rust.combined));
    for (const channel of ['store:tickers', 'orderbook', 'candles']) {
      assert.equal(report.parity[channel].qualifiedStatus, 'PASS', channel);
      assert.ok(report.parity[channel].leftCount > 0, channel);
    }
    for (const result of report.runs.filter((entry) => entry.implementation === 'rust')) {
      assert.equal(result.receipt.delivered, 12);
      assert.equal(result.receipt.dropped, 0);
      const { outputs } = deserialize(readFileSync(result.fixture));
      expected ??= outputs;
      assert.deepEqual(outputs, expected, `fresh native account ${run}/${result.label} preserves the entire combined sequence`);
    }
  }
});
