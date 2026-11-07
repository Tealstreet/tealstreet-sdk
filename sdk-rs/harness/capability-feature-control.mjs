// CI-only negative control. The mutant is built in a disposable detached tree;
// normal family artifacts and tracked manifests are never overwritten.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';

export function withoutMexcBookIngress(manifest) {
  const feature = ', "command-order-book"';
  assert.equal(manifest.split(feature).length, 2, 'exactly one MEXC book decoder opt-in is required');
  return manifest.replace(feature, '');
}

export function assertMexcDecoderControl(Engine) {
  const instance = new Engine('mexc', JSON.stringify({ publicDataOnly: true }));
  try {
    assert.equal(JSON.parse(instance.capabilities()).orderBookQuery, true, 'mutant must still advertise the book query');
    assert.throws(() => instance.command(JSON.stringify({
      op: 'fetchOrderBook', command: 1, symbol: 'BTC/USDT', limit: 1,
    }), 0), /unknown variant [`'"]fetchOrderBook[`'"]/, 'only the missing book decoder is the expected failure');
  } finally { instance.free(); }
}

function run(root, command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: 'inherit' });
  assert.equal(result.status, 0, `${command} ${args.join(' ')} failed; not decoder evidence`);
}

export function runMissingFeatureControl(root = process.cwd()) {
  assert.equal(process.env.GITHUB_ACTIONS, 'true', 'release mutation control runs only in CI');
  const scratch = mkdtempSync(join(process.env.RUNNER_TEMP || tmpdir(), 'mexc-ingress-control-'));
  const tree = join(scratch, 'source');
  let added = false;
  try {
    run(root, 'git', ['worktree', 'add', '--detach', tree, 'HEAD']);
    added = true;
    const path = join(tree, 'sdk-rs/venues/mexc/Cargo.toml');
    writeFileSync(path, withoutMexcBookIngress(readFileSync(path, 'utf8')));
    run(tree, 'corepack', ['yarn', '-s', 'rust:build-wasm', '--venue', 'mexc']);
    const module = createRequire(import.meta.url)(join(tree, 'sdk-rs/hosts/wasm/pkg/node/safe_cex_wasm.js'));
    assertMexcDecoderControl(module.WasmEngine);
    console.log('MEXC isolated missing-command-order-book control: expected decoder rejection verified');
  } finally {
    if (added) run(root, 'git', ['worktree', 'remove', '--force', tree]);
    rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) runMissingFeatureControl();
