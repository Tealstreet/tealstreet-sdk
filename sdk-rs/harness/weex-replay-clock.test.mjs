import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { deterministicReplay } from './polling.mjs';
import { selectResponse } from './requests.mjs';
import { VENUES } from './venues.mjs';

test('WEEX and ApeX Omni private replays run on the virtual tape clock', () => {
  for (const name of ['weex', 'apexomni']) assert.equal(deterministicReplay(VENUES[name], true), true, name);
});

// TS ticks every 2 s from a frozen startup, so a poll lands at t=4000 just
// before the cancel command there. It must still read the pre-cancel book.
test('a WEEX order snapshot is served only after the command whose outcome it shows', () => {
  const directory = mkdtempSync(join(tmpdir(), 'weex-tape-'));
  try {
    const out = join(directory, 'weex-private.jsonl');
    const generated = spawnSync(process.execPath, ['sdk-rs/harness/private-tape-weex.mjs', '--out', out], { encoding: 'utf8' });
    assert.equal(generated.status, 0, generated.stderr);
    const records = readFileSync(out, 'utf8').trim().split('\n').map((line) => JSON.parse(line));
    const queue = (path) => records.filter((record) => record.type === 'http' && record.url.endsWith(path));
    const ids = (record) => JSON.parse(record.body).map((row) => row.orderId ?? row.algoId);
    const orders = queue('/capi/v3/openOrders?limit=100&page=0');
    assert.deepEqual(ids(selectResponse(orders, 4000, { commandIndex: 2 })),
      ['702345678901234560', '702345678901234561', '702345678901234562']);
    assert.deepEqual(ids(selectResponse(orders, 4000, { commandIndex: 3 })), ['702345678901234560', '702345678901234562']);
    const algos = queue('/capi/v3/openAlgoOrders?limit=100&page=1');
    assert.deepEqual(ids(selectResponse(algos, 6000, { commandIndex: 5 })), []);
    assert.deepEqual(ids(selectResponse(algos, 6000, { commandIndex: 6 })), ['702345678901234565']);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
