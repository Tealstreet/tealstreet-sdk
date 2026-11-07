import assert from 'node:assert/strict';
import test from 'node:test';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createLighterReplayRandom } from './lighter-replay-random.mjs';

test('Lighter TS replay random repeats per tape with independent cursors', () => {
  const first = createLighterReplayRandom('ab'.repeat(32));
  const second = createLighterReplayRandom('ab'.repeat(32));
  const different = createLighterReplayRandom('cd'.repeat(32));
  const values = Array.from({ length: 32 }, () => first());
  assert.deepEqual(values, Array.from({ length: 32 }, () => second()));
  assert.notDeepEqual(values, Array.from({ length: 32 }, () => different()));
  assert.ok(values.every((value) => value >= 0 && value < 1));
  assert.equal(new Set(values).size, values.length);
});

test('constructing a Lighter replay source leaves global randomness and signing untouched', () => {
  const random = Math.random, crypto = globalThis.crypto.getRandomValues;
  createLighterReplayRandom('ab'.repeat(32))();
  assert.equal(Math.random, random);
  assert.equal(globalThis.crypto.getRandomValues, crypto);
  for (const seed of [undefined, '', 'ab', 'gg'.repeat(32)])
    assert.throws(() => createLighterReplayRandom(seed), /tape SHA-256/);
});

for (const venue of ['lighter', 'lighter-robinhood']) test(`private TS ${venue} replay replaces colliding randomness after crypto init without weakening verification`, () => {
  const directory = mkdtempSync(join(tmpdir(), 'lighter-replay-random-'));
  try {
    const tape = join(directory, 'lighter-private.jsonl');
    const preload = join(directory, 'colliding-random.mjs');
    const output = join(directory, 'output.json');
    const trace = join(directory, 'random.jsonl');
    const generator = fileURLToPath(new URL('./private-tape-lighter.mjs', import.meta.url));
    const generated = spawnSync(process.execPath, [generator, '--venue', venue, '--out', tape], { encoding: 'utf8' });
    assert.equal(generated.status, 0, generated.stderr);
    // Without the replay initializer, entry/TP/SL draw the same suffix at the
    // same tape millisecond and the real verifier rejects the reused order ID.
    // Record every call to the installed source to verify the actual tape seed.
    writeFileSync(preload, `import { appendFileSync } from 'node:fs';
let source = () => 0.5;
Object.defineProperty(Math, 'random', {
  configurable: true,
  get: () => source,
  set(value) { source = () => {
    const result = value();
    appendFileSync(${JSON.stringify(trace)}, JSON.stringify(result) + '\\n');
    return result;
  }; },
});
`);
    const replay = fileURLToPath(new URL('./replay.mjs', import.meta.url));
    const result = spawnSync(process.execPath, ['--expose-gc', '--import', 'tsx', '--import', preload,
      replay, '--impl', 'ts', '--venue', venue, '--tape', tape, '--books', '', '--out', output],
    { encoding: 'utf8', timeout: 30_000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } });
    assert.equal(result.status, 0, result.error?.message ?? result.stderr);
    const raw = JSON.parse(readFileSync(output));
    assert.equal(raw.commands.length, 8);
    assert.deepEqual(raw.commands[4], { error: 'Not implemented' });
    assert.ok(raw.commands.filter((_, index) => index !== 4).every((command) => Object.hasOwn(command, 'ok')),
      JSON.stringify(raw.commands));
    assert.equal(raw.commands[5].ok.length, 3, 'entry, stop-loss and take-profit are all acknowledged');
    assert.equal(raw.commands[7].ok.length, 2, 'the batch is acknowledged in full');
    const draws = readFileSync(trace, 'utf8').trim().split('\n').map(JSON.parse);
    assert.ok(draws.length >= 7, 'exercise the actual order-ID random calls');
    const expected = createLighterReplayRandom(createHash('sha256').update(readFileSync(tape)).digest('hex'));
    assert.deepEqual(draws, draws.map(() => expected()));
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
