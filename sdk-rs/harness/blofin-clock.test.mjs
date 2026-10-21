import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ReplayClock, expectedCheckpointPlan } from './tape.mjs';

const tape = () => {
  const directory = mkdtempSync(join(tmpdir(), 'blofin-clock-'));
  try {
    const out = join(directory, 'blofin-private.jsonl');
    const generated = spawnSync(process.execPath, [fileURLToPath(new URL('./private-tape-blofin.mjs', import.meta.url)), '--out', out], { encoding: 'utf8' });
    assert.equal(generated.status, 0, generated.stderr);
    return readFileSync(out, 'utf8').trim().split('\n').map(JSON.parse);
  } finally { rmSync(directory, { recursive: true, force: true }); }
};

test('Blofin missing fill timestamps use one tape epoch across different wall starts', async () => {
  const records = tape();
  const metadata = records.find((record) => record.type === 'meta');
  assert.equal(metadata?.private, true);
  const history = records.find((record) => record.op === 'fetchFills');
  assert.equal(metadata?.epoch, history.from);
  // Live fill rows intentionally lack venue timestamps, exercising the TS
  // receiver's Date.now fallback without changing those recorded wire payloads.
  const live = records.filter((record) => record.type === 'ws').map((record) => JSON.parse(record.text))
    .flatMap((message) => message.data ?? []).find((row) => Number(row.filledSize) > 0 && !row.fillTime && !row.uTime && !row.cTime);
  assert.ok(live);
  for (const wallStart of [1800000000000, 1900000000000]) {
    const clock = new ReplayClock(metadata?.epoch ?? wallStart, true);
    clock.install();
    try {
      assert.equal(Date.now(), 1700000000000);
      await clock.advance(1300);
      assert.equal(Math.floor(Date.now() / 1000), 1700000001);
    } finally { clock.restore(); }
  }
});

test('Blofin epoch metadata adds no delivered frame or checkpoint boundary', () => {
  const records = tape(), original = records.filter((record) => record.type !== 'meta');
  const current = expectedCheckpointPlan(records), prior = expectedCheckpointPlan(original);
  assert.deepEqual(current.map(({ ordinal, t }) => ({ ordinal, t })), prior.map(({ ordinal, t }) => ({ ordinal, t })));
  for (let index = 1; index < current.length; index++) assert.equal(current[index].recordIndex, prior[index].recordIndex + 1);
});
