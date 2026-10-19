import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
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

for (const [venue, fixture, frameCount, repeats] of [
  ['binance', 'binance-delivery-prefix.json.gz', 12, 4],
  ['cryptocom', 'cryptocom-delivery-tape.json.gz', 147, 8],
]) test(`recorded ${venue} frames preserve synchronous Store/event order across ${repeats * 2} fresh native accounts`, { timeout: 600000 }, async (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'safe-cex-delivery-order-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const bundle = JSON.parse(gunzipSync(readFileSync(new URL(`./fixtures/live-public/${fixture}`, import.meta.url))));
  bundle.rest = bundle.rest.map(([key, { base64, ...response }]) => [key, { ...response, bytes: Buffer.from(base64, 'base64') }]);
  assert.equal(bundle.tape.frames.length, frameCount);
  if (venue === 'cryptocom') {
    assert.equal(bundle.sourceBundleSha256, 'ad0ed00314ed4764d7223a1e52ebc07601bc5123ab3580c1cd6791500faf4bc3');
    assert.equal(createHash('sha256').update(JSON.stringify(bundle.tape)).digest('hex'), '39c67b53d2da684b1bcb4cfec4b693d19413b86737c4159e30d51ac97b14a495');
  }
  const replay = join(directory, 'prefix.bundle.v8');
  writeFileSync(replay, serialize(bundle));
  let expected;
  for (let run = 0; run < repeats; run++) {
    const output = join(directory, `run-${run}.json`);
    const child = spawn(process.execPath, [
      join(root, 'sdk-rs/scripts/live-diff/streams.mjs'), '--venue', venue,
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
    // They do not excuse incomplete parser/Jobs/raw-task receipts or native order diffs.
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
    const source = deserialize(readFileSync(report.runs.find((entry) => entry.implementation === 'ts').fixture));
    if (venue === 'cryptocom') {
      assert.equal(source.outputs.length, 320);
      for (const [kind, count] of [['store', 186], ['orderbook', 118], ['candles', 16]]) {
        assert.equal(source.outputs.filter(entry => entry.kind === kind).length, count);
      }
    }
    for (const result of report.runs.filter((entry) => entry.implementation === 'rust')) {
      assert.equal(result.receipt.delivered, frameCount);
      assert.equal(result.receipt.dropped, 0);
      assert.equal(result.receipt.processed.rawTasks.version, 1);
      assert.equal(result.receipt.processed.rawTasks.blocking, 0);
      assert.equal(result.receipt.processed.rawTasks.unknown, 0);
      const { outputs } = deserialize(readFileSync(result.fixture));
      assert.deepEqual(outputs.map((entry) => entry.kind), source.outputs.map((entry) => entry.kind), 'combined channel order matches the maintained worker');
      expected ??= outputs;
      assert.deepEqual(outputs, expected, `fresh native account ${run}/${result.label} preserves the entire combined sequence`);
    }
  }
});
