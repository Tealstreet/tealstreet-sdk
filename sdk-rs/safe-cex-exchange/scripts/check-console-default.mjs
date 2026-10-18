// Fresh production compiler inventory; not a whole-package accepted-set replay.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { inventory, receiverItemSha256 } from '../../safe-cex-parity/scripts/accepted-inventory.mjs';
import { loadPolicy } from '../../safe-cex-parity/scripts/accepted-policy-files.mjs';

assert.ok(process.argv.length === 2 || (process.argv.length === 3 && process.argv[2] === '--expect-missing'));
const workspace = fs.realpathSync(fileURLToPath(new URL('../..', import.meta.url)));
const target = path.join(workspace, 'target/accepted-inventory');
const compile = spawnSync('cargo', ['rustdoc', '--locked', '-p', 'safe-cex-exchange', '--jobs=1', '--lib', '--',
  '-Z', 'unstable-options', '--output-format=json', '--document-private-items', '--document-hidden-items', '-A', 'rustdoc::all'], {
  cwd: workspace, env: { ...process.env, RUSTC_BOOTSTRAP: '1', CARGO_BUILD_JOBS: '1', CARGO_TARGET_DIR: target },
  encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
});
assert.equal(compile.status, 0, 'fresh core rustdoc: ' + (compile.error ?? '') + '\n' + compile.stderr);
const doc = JSON.parse(fs.readFileSync(path.join(target, 'doc/safe_cex_exchange.json')));
const trait = Object.values(doc.index).find(item => item.name === 'ExchangeRuntime' && item.inner.trait && item.crate_id === 0);
assert.ok(trait);
const method = trait.inner.trait.items.map(id => doc.index[String(id)]).find(item => item.name === 'console');
assert.ok(method.inner.function.has_body, 'the real runtime default must be inventoried');
assert.equal(method.span.filename, 'safe-cex-exchange/src/runtime.rs');
const key = 'safe_cex_exchange::runtime::ExchangeRuntime::console';
const policyRoot = path.join(workspace, 'safe-cex-parity/accepted-set');
const defaultTraits = loadPolicy('default-traits', policyRoot);
const receiverTraits = loadPolicy('receiver-traits', policyRoot);
const options = {
  defaultTraits,
  receiverTraits,
  readSource: file => fs.readFileSync(path.resolve(workspace, file)),
};
if (process.argv[2] === '--expect-missing') {
  assert.equal(receiverTraits[key], undefined, 'baseline must not already classify this default');
  assert.throws(() => inventory(doc, options), /ExchangeRuntime::console: accepted-set assertion: uninstantiated default decoder requires production coverage/);
  console.log('Compiled RED: production console default has no reviewed coverage.');
  console.log('Reviewed item SHA-256: ' + receiverItemSha256(method.span, options.readSource(method.span.filename)));
  process.exit(0);
}
assert.equal(receiverTraits[key]?.category, 'non-decoder');
inventory(doc, options);
const missing = { ...receiverTraits };
delete missing[key];
assert.throws(() => inventory(doc, { ...options, receiverTraits: missing }),
  /ExchangeRuntime::console: accepted-set assertion: uninstantiated default decoder requires production coverage/);
const stale = { ...receiverTraits, [key]: { ...receiverTraits[key], itemSha256: '0'.repeat(64) } };
assert.throws(() => inventory(doc, { ...options, receiverTraits: stale }), /reviewed item source changed/);
console.log('Reviewed console default inventory passes; missing classification and stale item-pin controls fail.');
console.log('Native output is independently witnessed by tests/native_console_default.rs with feature native.');
