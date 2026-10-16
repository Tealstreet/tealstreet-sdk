import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const workspace = path.join(root, 'sdk-rs');
const script = fileURLToPath(new URL('./types.mjs', import.meta.url));

for (const override of [false, true]) test(override
  ? 'Binance explicit relative RUSTFMT retains its caller directory'
  : 'Binance formatter runs inside the pinned Rust workspace from a root oracle invocation', () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'binance-rustfmt-'));
  try {
    const selected = spawnSync('rustup', ['which', 'rustfmt'], {cwd: workspace, encoding: 'utf8'});
    assert.equal(selected.status, 0, selected.stderr);
    const formatter = path.join(temporary, 'rustfmt');
    // Model CI's isolated root toolchain without modifying any installed component.
    // Successful invocations still run the real formatter and compare all generated types.
    fs.writeFileSync(formatter, `#!${process.execPath}
const {spawnSync} = require('node:child_process');
if (process.cwd() !== process.env.BINANCE_EXPECTED_WORKSPACE) {
  console.error('root toolchain has no rustfmt; expected pinned workspace');
  process.exit(71);
}
const result = spawnSync(process.env.BINANCE_REAL_RUSTFMT, process.argv.slice(2), {stdio: 'inherit'});
if (result.error) throw result.error;
process.exit(result.status ?? 1);
`, { mode: 0o755 });
    const env = {
      ...process.env,
      BINANCE_EXPECTED_WORKSPACE: override ? root.replace(/\/$/, '') : workspace,
      BINANCE_REAL_RUSTFMT: selected.stdout.trim(),
      PATH: temporary + path.delimiter + process.env.PATH,
    };
    delete env.RUSTFMT;
    if (override) env.RUSTFMT = path.relative(root, formatter);
    const result = spawnSync(process.execPath, [script, '--check'], {
      cwd: root,
      encoding: 'utf8',
      env,
    });
    assert.equal(result.status, 0,
      `Binance formatter must select the pinned workspace before formatting: ${result.stderr}`);
    assert.equal(result.stdout.trim(), 'Binance types: three live source modules match generated Rust.');
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});
