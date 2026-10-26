import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

for (const check of ['test', 'clippy']) test(`a leaf venue ${check} also checks its registry and native host consumers`, t => {
  const root = mkdtempSync(join(tmpdir(), 'native-ci-consumers-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'bin'), log = join(root, 'cargo.jsonl');
  mkdirSync(bin);
  writeFileSync(join(bin, 'cargo'), `#!${process.execPath}\nrequire('node:fs').appendFileSync(process.env.TEST_CARGO_LOG, JSON.stringify(process.argv.slice(2))+'\\n');\n`);
  chmodSync(join(bin, 'cargo'), 0o755);
  const child = spawnSync(process.execPath, [fileURLToPath(new URL('./run-native-ci.mjs', import.meta.url))], {
    cwd: root, encoding: 'utf8', env: { ...process.env, PATH: `${bin}:${process.env.PATH}`,
      TEST_CARGO_LOG: log, NATIVE_FULL: 'false', NATIVE_CRATES: '["binance"]', NATIVE_CHECK: check },
  });
  assert.equal(child.status, 0, child.stderr);
  const calls = readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls.map(args => args[args.indexOf('-p') + 1]), ['safe-cex-venue-binance', 'safe-cex-venues', 'safe-cex-native']);
  for (const args of calls) { assert.equal(args[0], check); assert(args.includes('--all-targets')); }
  assert(!calls[2].includes('--no-default-features'));
});
