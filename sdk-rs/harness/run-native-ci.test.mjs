import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

for (const crates of [['weex'], ['weex', 'bybit']]) for (const check of ['test', 'clippy']) test(`${crates.join('+')} ${check} keeps selected libraries and full registry/host integration`, t => {
  const root = mkdtempSync(join(tmpdir(), 'native-ci-consumers-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const bin = join(root, 'bin'), log = join(root, 'cargo.jsonl');
  mkdirSync(bin);
  writeFileSync(join(bin, 'cargo'), `#!${process.execPath}\nrequire('node:fs').appendFileSync(process.env.TEST_CARGO_LOG, JSON.stringify(process.argv.slice(2))+'\\n');\n`);
  chmodSync(join(bin, 'cargo'), 0o755);
  const child = spawnSync(process.execPath, [fileURLToPath(new URL('./run-native-ci.mjs', import.meta.url))], {
    cwd: root, encoding: 'utf8', env: { ...process.env, PATH: `${bin}:${process.env.PATH}`,
      TEST_CARGO_LOG: log, NATIVE_FULL: 'false', NATIVE_CRATES: JSON.stringify(crates), NATIVE_CHECK: check },
  });
  assert.equal(child.status, 0, child.stderr);
  const calls = readFileSync(log, 'utf8').trim().split('\n').map(JSON.parse);
  assert.deepEqual(calls.map(args => args[args.indexOf('-p') + 1]), [
    ...crates.flatMap(crate => [`safe-cex-venue-${crate}`, 'safe-cex-venues']), 'safe-cex-venues',
  ]);
  for (const args of calls) {
    assert.equal(args[0], check);
    if (check === 'clippy') assert.deepEqual(args.slice(-3), ['--', '-D', 'warnings']);
  }
  for (const [index, crate] of crates.entries()) {
    assert(calls[index * 2].includes('--all-targets'));
    const selected = calls[index * 2 + 1];
    assert(selected.includes('--lib'));
    assert(!selected.includes('--all-targets'));
    assert(selected.includes('--no-default-features'));
    assert.equal(selected[selected.indexOf('--features') + 1], crate);
  }
  const consumers = calls.at(-1);
  assert.deepEqual(consumers.slice(0, 7), [check, '--locked', '-p', 'safe-cex-venues', '-p', 'safe-cex-native', '--all-targets']);
  assert(!consumers.includes('--no-default-features'));
  assert(!consumers.includes('--features'));

});
