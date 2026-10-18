// Run alone: mutate one live public-stream operation, then restore exact bytes.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function test(name) {
  return spawnSync('cargo', ['+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-kucoin', '--jobs=1',
    '--test', 'accepted_exchange_members', ...(name ? [name, '--', '--exact', '--nocapture'] : [])], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, CARGO_INCREMENTAL: '0' },
  });
}

const baseline = test();
assert.equal(baseline.status, 0, baseline.stdout + baseline.stderr);
const file = path.join(root, 'safe-cex-venue-kucoin/src/public_streams.rs');
const original = fs.readFileSync(file);
const source = original.toString();
const cases = [
  ['cleanup', 'pub fn listen_order_book<',
    'remove_handler(ops.clone(), topic.clone(), parser.clone(), true)?;',
    '// control: keep the actual futures parser registered',
    'futures_native_book_callback_and_cleanup', 'futures cleanup suppresses valid emitted update'],
  ['duplicate', 'pub fn apply_increment<', 'return Ok(false);', 'return Ok(true);',
    'futures_native_book_callback_and_cleanup', 'duplicate sequence is suppressed'],
  ['gap', 'pub fn apply_increment<',
    'recover_gap(h, s, message, api, symbol, callback)?;',
    'drop((h, s, message, api, symbol, callback));',
    'futures_native_book_callback_and_cleanup', 'sequence gap refetches then replays'],
  ['volume', 'pub fn candle_parser<', '("volume", "5")', '("volume", "4")',
    'public_candle_callbacks_keep_distinct_fields_for_both_families', 'source volume'],
];
for (const [name, functionAnchor, anchor, replacement, testName, assertion] of cases) {
  const start = source.indexOf(functionAnchor);
  assert.ok(start >= 0, 'actual production function ' + functionAnchor);
  const end = source.indexOf('\npub fn ', start + 1);
  const body = source.slice(start, end < 0 ? undefined : end);
  assert.ok(body.includes(anchor), 'production statement ' + name);
  try {
    fs.writeFileSync(file, source.slice(0, start) + body.replace(anchor, replacement)
      + (end < 0 ? '' : source.slice(end)));
    const result = test(testName), output = result.stdout + result.stderr;
    assert.equal(result.status, 101, output);
    assert.ok(output.includes('test result: FAILED'), output);
    assert.ok(output.includes(assertion), output);
    console.log('CONTROL: compiled ' + name + ' mutation fails ' + assertion);
  } finally {
    fs.writeFileSync(file, original);
    assert.deepEqual(fs.readFileSync(file), original);
  }
}
const restored = test();
assert.equal(restored.status, 0, restored.stdout + restored.stderr);
console.log('RESTORED: three native public book/candle tests pass');
