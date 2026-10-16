// Run alone: temporarily disconnect the production futures handler removal.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function test(name) {
  return spawnSync('cargo', ['test', '--locked', '-p', 'safe-cex-venue-kucoin', '--jobs=1',
    '--test', 'accepted_exchange_members', ...(name ? [name, '--', '--exact', '--nocapture'] : [])], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1' },
  });
}

const baseline = test();
assert.equal(baseline.status, 0, baseline.stdout + baseline.stderr);
const file = path.join(root, 'safe-cex-venue-kucoin/src/public_streams.rs');
const original = fs.readFileSync(file);
const source = original.toString();
const start = source.indexOf('pub fn listen_order_book<');
assert.ok(start >= 0, 'unique production futures listener');
const body = source.slice(start);
const anchor = 'remove_handler(ops.clone(), topic.clone(), parser.clone(), true)?;';
assert.equal(body.split(anchor).length, 2, 'one futures handler-removal statement');
try {
  fs.writeFileSync(file, source.slice(0, start)
    + body.replace(anchor, '// control: keep the actual futures parser registered'));
  const result = test('futures_native_book_callback_and_cleanup');
  const output = result.stdout + result.stderr;
  assert.equal(result.status, 101, output);
  assert.ok(output.includes('test result: FAILED'), output);
  assert.ok(output.includes('futures cleanup suppresses valid emitted update'), output);
  console.log('CONTROL: compiled handler-removal mutation emits after cleanup and fails the native callback assertion');
} finally {
  fs.writeFileSync(file, original);
  assert.deepEqual(fs.readFileSync(file), original);
}
const restored = test();
assert.equal(restored.status, 0, restored.stdout + restored.stderr);
console.log('RESTORED: both native book callback/cleanup tests pass');
