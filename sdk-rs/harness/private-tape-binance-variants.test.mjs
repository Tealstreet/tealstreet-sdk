import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

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

test('PM startup and cancel-all serve the algo API while preserving the legacy TS inputs', () => {
  const records = generate('binancepm').trim().split('\n').map(JSON.parse);
  const rows = (method, path) => records.filter((row) => row.type === 'http' &&
    row.method === method && new URL(row.url).pathname === `/papi/v1/um/${path}`);
  const [legacy] = rows('GET', 'conditional/openOrders');
  const [algo] = rows('GET', 'algo/openAlgoOrders');
  assert.ok(algo, 'Rust startup needs the current PM algo endpoint');
  assert.equal(legacy.impl, 'ts');
  assert.equal(algo.impl, 'rust');
  assert.equal(algo.status, legacy.status);
  assert.equal(algo.body, legacy.body);
  assert.deepEqual(JSON.parse(algo.body), []);
  const [cancel] = rows('DELETE', 'algo/allOpenOrders');
  assert.ok(cancel, 'Rust cancel-all needs the current PM algo endpoint');
  assert.equal(cancel.impl, 'rust');
  assert.deepEqual(JSON.parse(cancel.body), { code: 200, msg: 'success' });
  assert.equal(new URL(cancel.url).searchParams.get('symbol'), 'ETHUSDT');
  assert.equal(rows('DELETE', 'conditional/allOpenOrders').length, 0);
});

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

test('COIN-M migration leaves the Spot and PM tape bytes unchanged', () => {
  for (const [venue, expected] of [
    ['binancespot', '17f14a131f0240857e7a0866432587f8929a9aea084d1f619c65a88bb4309c67'],
    ['binancepm', 'c37534b929ea81c1959f2eb8f585f6305f34c43a03bb7a3bb5107f7319083bda'],
  ]) assert.equal(createHash('sha256').update(generate(venue)).digest('hex'), expected, venue);
});

test('PM migration preserves every original payload, command and checkpoint boundary', () => {
  const tape = generate('binancepm');
  const records = tape.trim().split('\n').map(JSON.parse);
  const original = records.filter((row) => !row.url?.endsWith('/algo/openAlgoOrders')).map((row) => {
    if (row.url?.endsWith('/conditional/openOrders')) {
      const { impl, ...legacy } = row;
      assert.equal(impl, 'ts');
      return legacy;
    }
    if (row.url?.includes('/algo/allOpenOrders?'))
      return { ...row, url: row.url.replace('/algo/allOpenOrders?', '/conditional/allOpenOrders?') };
    return row;
  }).map((row) => JSON.stringify(row)).join('\n') + '\n';
  assert.equal(createHash('sha256').update(original).digest('hex'),
    'aef98183e725ca464ac67b55e40ee27f826d785a705a6dbb728e4717d5b853b1');
  const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url), 'utf8'));
  const entry = manifest.failures.find((row) => row.tape === 'sdk-rs/harness/tapes/binancepm-private.jsonl');
  assert.equal(entry.tapeSha256, createHash('sha256').update(tape).digest('hex'));
});
