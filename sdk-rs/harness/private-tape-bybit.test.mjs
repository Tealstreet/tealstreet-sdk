import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { requestKey } from './requests.mjs';
import { venueOf } from './venues.mjs';

test('Bybit private gate requires terminal history missing from unfiltered results', () => {
  const directory = mkdtempSync(join(tmpdir(), 'bybit-history-tape-'));
  try {
    const out = join(directory, 'tape.jsonl');
    execFileSync(process.execPath, [new URL('./private-tape-bybit.mjs', import.meta.url).pathname, '--history-only', '--out', out]);
    const records = readFileSync(out, 'utf8').trim().split('\n').map(JSON.parse);
    assert.equal(records.filter((record) => record.type === 'command').length, 1);
    assert(records.filter((record) => record.type === 'http').every((record) => record.method === 'GET'));
    const command = records.find((record) => record.type === 'command' && record.op === 'fetchOrderHistory');
    assert(command, 'the focused CI tape must exercise the actual history command');
    const requests = records.filter((record) => record.type === 'http' && new URL(record.url).pathname === '/v5/order/history');
    assert.equal(requests.length, 7);
    const [unfiltered] = requests;
    assert.equal(new URL(unfiltered.url).searchParams.get('orderStatus'), null);
    assert.deepEqual(JSON.parse(unfiltered.body).result.list.map((row) => row.orderStatus), ['Filled']);
    const venue = venueOf('bybit');
    const keys = requests.map((record) => requestKey('GET', record.url, undefined, new Set(), venue));
    assert.equal(new Set(keys).size, 7, 'status and cursor must remain part of tape request identity');
    const ids = new Set();
    for (const status of ['Cancelled', 'Rejected', 'Deactivated']) {
      const pages = requests.filter((record) => new URL(record.url).searchParams.get('orderStatus') === status);
      assert.equal(pages.length, 2);
      assert.deepEqual(pages.map((record) => new URL(record.url).searchParams.get('cursor')), [null, 'shared']);
      assert.equal(JSON.parse(pages[0].body).result.nextPageCursor, 'shared');
      for (const page of pages) {
        for (const row of JSON.parse(page.body).result.list) {
          assert.equal(row.orderStatus, status);
          ids.add(row.orderId);
        }
      }
    }
    assert.equal(ids.size, 3, 'each filtered terminal row is deliberately duplicated across pages');
    assert.deepEqual(new Set(command.expected.map((row) => row.id)), new Set(['history-Filled', ...ids]));
    execFileSync(process.execPath, [new URL('./private-tape-bybit.mjs', import.meta.url).pathname, '--out', out]);
    const manifest = JSON.parse(readFileSync(new URL('./known-gate-failures.json', import.meta.url)));
    const original = manifest.failures.find((entry) => entry.venue === 'bybit' && entry.tape === 'sdk-rs/harness/tapes/bybit-private.jsonl');
    assert.equal(createHash('sha256').update(readFileSync(out)).digest('hex'), original.tapeSha256,
      'adding a history gate must preserve the original lifecycle tape and its declared diagnostics');
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
