import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildAsterTape } from './private-tape-asterdex.mjs';

const root = fileURLToPath(new URL('../../', import.meta.url));

test('private replay keeps venue signing clocks valid after delayed readiness', { timeout: 30000 }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'asterdex-replay-clock-'));
  try {
    const tape = join(directory, 'clock.jsonl');
    const output = join(directory, 'output.json');
    const preload = join(directory, 'delay-ready.mjs');
    // Exercise the real TS virtual clock and request signer. A slow account
    // startup after its time sync must not move signatures into the future.
    const clockModule = new URL(
      '../../packages/safe-cex/src/exchanges/asterdex/asterdex.virtualClock.ts',
      import.meta.url,
    );
    writeFileSync(
      preload,
      `
      import { asterDEXVirtualClock } from ${JSON.stringify(clockModule.href)};
      const start = asterDEXVirtualClock.start;
      asterDEXVirtualClock.start = async (...args) => {
        await start(...args);
        await new Promise(resolve => setTimeout(resolve, 1500));
      };
    `,
    );
    let keptOrder = false;
    const records = buildAsterTape().filter((record) => {
      if (record.type !== 'http') return record.t <= 600;
      if (record.method === 'GET' || new URL(record.url).pathname.endsWith('/listenKey')) return true;
      if (record.method === 'POST' && !keptOrder) {
        keptOrder = true;
        return true;
      }
      return false;
    });
    writeFileSync(tape, `${records.map((record) => JSON.stringify(record)).join('\n')}\n`);
    const child = spawnSync(
      process.execPath,
      [
        '--expose-gc',
        '--import',
        'tsx',
        '--import',
        preload,
        'sdk-rs/harness/replay.mjs',
        '--impl',
        'ts',
        '--venue',
        'asterdex',
        '--tape',
        tape,
        '--books',
        '',
        '--out',
        output,
      ],
      { cwd: root, encoding: 'utf8', timeout: 25000, env: { ...process.env, ESBUILD_WORKER_THREADS: '0' } },
    );
    assert.ifError(child.error);
    assert.equal(child.status, 0, `${child.stdout}\n${child.stderr}`);
    const replay = JSON.parse(readFileSync(output, 'utf8'));
    assert.equal(replay.invalidSignatures, 0);
    assert.equal(replay.unserved, 0);
    assert.equal(replay.unusedWrites, 0);
    assert.deepEqual(replay.commands, [{ ok: ['1002'] }]);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
});
