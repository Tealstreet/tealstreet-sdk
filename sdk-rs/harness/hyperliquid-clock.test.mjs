import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Tape, expectedCheckpointPlan } from './tape.mjs';
import { venueOf } from './venues.mjs';
import { signHyperliquidFrame } from './hyperliquid-test-utils.mjs';

const originalHashes = [
  'a44f3bf62cb55f571d3fa6eb7f617a5499137d2e24b37ad49d29e038c9a4daa4',
  '4d735e68ad388c615fcdb85fc0fabeec9eecbfccc573077b62a25d61419902e2',
];
const generate = (spot) => {
  const directory = mkdtempSync(join(tmpdir(), 'hyperliquid-clock-'));
  try {
    const out = join(directory, 'private.jsonl');
    const result = spawnSync(process.execPath, [fileURLToPath(new URL('./private-tape-hyperliquid.mjs', import.meta.url)),
      ...(spot ? ['--spot'] : []), '--out', out], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return readFileSync(out, 'utf8').trim().split('\n').map(JSON.parse);
  } finally { rmSync(directory, { recursive: true, force: true }); }
};

for (const spot of [false, true]) {
  const label = spot ? 'embedded Spot' : 'perp';
  test(`Hyperliquid ${label} fences reconciliation behind command ACKs despite timer pumping`, async () => {
    const records = generate(spot);
    const options = records.find((record) => record.type === 'config').options;
    const tape = new Tape(records, venueOf('hyperliquid'), true, 30000, undefined, [], options);
    const reads = records.filter((record) => record.type === 'http' &&
      JSON.parse(record.requestBody).type === 'frontendOpenOrders');
    const core = reads.find((record) => !JSON.parse(record.requestBody).dex);
    const hip3 = reads.find((record) => JSON.parse(record.requestBody).dex === 'xyz');
    const ids = (record) => {
      const response = tape.respond(record.method, record.url, record.requestBody);
      assert.equal(response.status, 200);
      return JSON.parse(response.body).map((row) => row.oid);
    };
    tape.replayClock = { active: true, time: 6200, epoch: 0 };
    tape.commandIndex = 0;
    assert.deepEqual(ids(core), [], 'startup must not read future order3002');
    tape.commandIndex = 1;
    assert.deepEqual(ids(core), [], 'command1 before ACK must not read future order3002');
    // Even a clock beyond every snapshot cannot bypass command/ACK causality.
    tape.replayClock.time = 1e6;
    const writes = records.filter((record) => record.type === 'http' && new URL(record.url).pathname === '/exchange');
    const stages = [
      [[], []], [[3001], []], [[], []], [[3002], []], [[], []],
      [[], [3003]], [[], []], [[], []], ...(spot ? [[[3005], []], [[], []]] : []),
    ];
    const nonceBase = Date.now();
    for (const [index, write] of writes.entries()) {
      tape.commandIndex = index + 1;
      tape.settledCommands = index;
      assert.deepEqual([ids(core), ids(hip3)], stages[index], `before ACK ${index + 1}`);
      const frame = await signHyperliquidFrame(JSON.parse(write.requestBody).action, { nonce: nonceBase + index });
      assert.equal(tape.respond(write.method, write.url, JSON.stringify(frame)).status, 200);
      assert.deepEqual([ids(core), ids(hip3)], stages[index + 1], `after ACK ${index + 1}`);
      tape.settledCommands = index + 1;
      assert.deepEqual([ids(core), ids(hip3)], stages[index + 1], `next command not issued after ${index + 1}`);
    }
    tape.assertWritesConsumed();
  });

  test(`Hyperliquid ${label} command fences preserve every original payload, time and ordinal`, () => {
    const records = generate(spot);
    const stripped = records.map(({ afterCommand, ...record }) => record);
    const bytes = `${stripped.map((record) => JSON.stringify(record)).join('\n')}\n`;
    assert.equal(createHash('sha256').update(bytes).digest('hex'), originalHashes[Number(spot)]);
    assert.deepEqual(expectedCheckpointPlan(records), expectedCheckpointPlan(stripped));
  });
}
