import assert from 'node:assert/strict';
import { createHash, createHmac } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { Tape } from './tape.mjs';
import { VENUES } from './venues.mjs';

const generate = (venue) => {
  const directory = mkdtempSync(join(tmpdir(), 'binance-variant-tape-'));
  try {
    const out = join(directory, 'tape.jsonl');
    const result = spawnSync(process.execPath, [
      new URL('./private-tape-binance-variants.mjs', import.meta.url).pathname,
      '--venue', venue, '--out', out,
    ], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return readFileSync(out, 'utf8');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};

const respond = (tape, method, url) => {
  const parsed = new URL(url);
  parsed.searchParams.set('timestamp', String(Date.now()));
  parsed.searchParams.set('recvWindow', '5000');
  const signature = createHmac('sha256', 'replay-secret').update(parsed.search.slice(1)).digest('hex');
  parsed.searchParams.set('signature', signature);
  return tape.respond(method, parsed.href, undefined, { 'X-MBX-APIKEY': 'replay-key' });
};

for (const impl of ['ts', 'rust']) {
  test(`PM ${impl} snapshot receives the shared algo response through the real Tape`, () => {
    const records = generate('binancepm').trim().split('\n').map(JSON.parse);
    const tape = new Tape(records, VENUES.binancepm, true, 30000, impl);
    const response = respond(tape, 'GET', 'https://papi.binance.com/papi/v1/um/algo/openAlgoOrders');
    assert.equal(response.status, 200, 'Both migrated receivers must finish conditional-order startup');
    assert.deepEqual(JSON.parse(response.body), []);
    assert.equal(respond(tape, 'GET', 'https://papi.binance.com/papi/v1/um/conditional/openOrders').status, 599);
  });

  test(`PM ${impl} cancel-all serves both families once and refuses a different symbol`, () => {
    const records = generate('binancepm').trim().split('\n').map(JSON.parse);
    const tape = new Tape(records, VENUES.binancepm, true, 30000, impl);
    for (const family of ['allOpenOrders', 'algo/allOpenOrders']) {
      const endpoint = `https://papi.binance.com/papi/v1/um/${family}`;
      assert.equal(respond(tape, 'DELETE', `${endpoint}?symbol=BTCUSDT`).status, 599);
      const response = respond(tape, 'DELETE', `${endpoint}?symbol=ETHUSDT`);
      assert.equal(response.status, 200, 'Both migrated receivers must acknowledge each cancel-all family');
      assert.deepEqual(JSON.parse(response.body), { code: 200, msg: 'success' });
      assert.equal(respond(tape, 'DELETE', `${endpoint}?symbol=ETHUSDT`).status, 599);
    }
  });
}

test('COIN-M startup serves the empty algo census without changing any legacy tape record', () => {
  const tape = generate('binanceinverse');
  const records = tape.trim().split('\n').map(JSON.parse);
  const algoRows = records.filter((row) => row.type === 'http' &&
    new URL(row.url).pathname === '/dapi/v1/openAlgoOrders');
  assert.equal(algoRows.length, 1, 'Rust startup needs the COIN-M algo endpoint');
  const [algo] = algoRows;
  assert.equal(algo.impl, 'rust');
  assert.equal(algo.method, 'GET');
  assert.equal(algo.url, 'https://dapi.binance.com/dapi/v1/openAlgoOrders?algoType=CONDITIONAL');
  assert.equal(algo.status, 200);
  assert.equal(algo.body, '[]');
  const legacy = records.filter((row) => row !== algo).map((row) => JSON.stringify(row)).join('\n') + '\n';
  assert.equal(createHash('sha256').update(legacy).digest('hex'),
    'a0a35640cbdb5fdd8f43a14188cc4113290683b1e625e131f6da0e9b8b38f94b');
  const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  const entry = manifest.failures.find((row) => row.tape === 'sdk-rs/harness/tapes/binanceinverse-private.jsonl');
  assert.equal(entry.tapeSha256, createHash('sha256').update(tape).digest('hex'));
});

test('PM migration leaves the Spot tape bytes unchanged', () => {
  assert.equal(createHash('sha256').update(generate('binancespot')).digest('hex'),
    '17f14a131f0240857e7a0866432587f8929a9aea084d1f619c65a88bb4309c67');
});

test('PM migration preserves every original payload, command and checkpoint boundary', () => {
  const tape = generate('binancepm');
  const records = tape.trim().split('\n').map(JSON.parse);
  const original = records.map((row) => {
    if (row.url?.endsWith('/algo/openAlgoOrders'))
      return { ...row, url: row.url.replace('/algo/openAlgoOrders', '/conditional/openOrders') };
    if (row.url?.includes('/algo/allOpenOrders?'))
      return { ...row, url: row.url.replace('/algo/allOpenOrders?', '/conditional/allOpenOrders?'), impl: 'rust' };
    return row;
  }).map((row) => JSON.stringify(row)).join('\n') + '\n';
  assert.equal(createHash('sha256').update(original).digest('hex'),
    'aef98183e725ca464ac67b55e40ee27f826d785a705a6dbb728e4717d5b853b1');
  const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  const entry = manifest.failures.find((row) => row.tape === 'sdk-rs/harness/tapes/binancepm-private.jsonl');
  assert.equal(entry.tapeSha256, createHash('sha256').update(tape).digest('hex'));
});
