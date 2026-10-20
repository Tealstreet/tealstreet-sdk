import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, writeFileSync, mkdirSync, copyFileSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';

const script = new URL('./build.sh', import.meta.url);
const manifest = readFileSync(new URL('../hosts/wasm/Cargo.toml', import.meta.url), 'utf8');
const families = [...manifest.matchAll(/^venue-([a-z0-9-]+) = /gm)].map((match) => match[1]);

function expectedCargoCommand(family) {
  let command = `cargo build --locked --release -p safe-cex-wasm --target wasm32-unknown-unknown --no-default-features --features venue-${family}`;
  if (family === 'paradex') command += ' --config profile.release.package.safe-cex-wasm.opt-level="z" --config profile.release.package.safe-cex-engine.opt-level=2';
  if (family === 'bitrue') command += ' --config profile.release.package.safe-cex-venue-bitrue.opt-level=3';
  return command;
}

function build(args) {
  const root = mkdtempSync(join(tmpdir(), 'safe-cex-build-'));
  const bin = join(root, 'bin');
  mkdirSync(bin);
  mkdirSync(join(root, 'harness'));
  mkdirSync(join(root, 'hosts/wasm'), { recursive: true });
  copyFileSync(script, join(root, 'harness/build.sh'));
  writeFileSync(join(root, 'hosts/wasm/Cargo.toml'), manifest);
  const log = join(root, 'calls');
  writeFileSync(join(bin, 'rustc'), '#!/bin/sh\nprintf "%s\\n" /unused/toolchain\n', { mode: 0o755 });
  writeFileSync(join(bin, 'cargo'), '#!/bin/sh\nprintf "cargo %s\\n" "$*" >> "$BUILD_TEST_LOG"\n', { mode: 0o755 });
  writeFileSync(join(bin, 'wasm-bindgen'), `#!/bin/sh
printf 'bindgen %s\\n' "$*" >> "$BUILD_TEST_LOG"
while [ "$#" -gt 0 ]; do
  if [ "$1" = --out-dir ]; then shift; mkdir -p "$1"; fi
  shift
done
`, { mode: 0o755 });
  const result = spawnSync('bash', [join(root, 'harness/build.sh'), ...args], {
    env: { ...process.env, PATH: `${bin}:${process.env.PATH}`, BUILD_TEST_LOG: log },
    encoding: 'utf8',
  });
  const calls = existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n') : [];
  return { root, result, calls, dispose: () => rmSync(root, { recursive: true, force: true }) };
}

test('venue selector builds one isolated family and emits both family packages', () => {
  const run = build(['--venue', 'okx']);
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const cargo = run.calls.filter((line) => line.startsWith('cargo '));
    assert.equal(cargo.length, 1);
    assert.equal(cargo[0], expectedCargoCommand('okx'));
    assert.equal(run.calls.filter((line) => line.startsWith('bindgen ')).length, 2);
    assert(run.calls.some((line) => line.includes('--out-dir hosts/wasm/pkg/web/okx')));
    assert.equal(readFileSync(join(run.root, 'hosts/wasm/pkg/node/okx/package.json'), 'utf8').trim(), '{"type":"commonjs"}');
    assert(!existsSync(join(run.root, 'hosts/wasm/pkg/node/bybit')));
  } finally { run.dispose(); }
});

test('default builds each manifest family separately', () => {
  const run = build([]);
  try {
    assert.equal(run.result.status, 0, run.result.stderr);
    const cargo = run.calls.filter((line) => line.startsWith('cargo '));
    assert.deepEqual(cargo, families.map(expectedCargoCommand));
    for (const family of families) {
      assert(existsSync(join(run.root, `hosts/wasm/pkg/node/${family}/package.json`)));
    }
  } finally { run.dispose(); }
});

test('selected Paradex and Bitrue builds preserve their exact isolated profiles', () => {
  for (const family of ['paradex', 'bitrue']) {
    const run = build(['--venue', family]);
    try {
      assert.equal(run.result.status, 0, run.result.stderr);
      assert.deepEqual(run.calls.filter((line) => line.startsWith('cargo ')), [expectedCargoCommand(family)]);
      const bindings = run.calls.filter((line) => line.startsWith('bindgen '));
      assert.equal(bindings.length, 2);
      for (const target of ['web', 'node'])
        assert(bindings.some((line) => line.includes(`--out-dir hosts/wasm/pkg/${target}/${family} `)));
      assert(!existsSync(join(run.root, 'hosts/wasm/pkg/node/okx')));
    } finally { run.dispose(); }
  }
});

test('invalid selectors fail before any build', () => {
  for (const args of [['--venue', '../../okx'], ['--venue'], ['--unknown', 'okx']]) {
    const run = build(args);
    try {
      assert.equal(run.result.status, 2);
      assert.deepEqual(run.calls, []);
    } finally { run.dispose(); }
  }
});
