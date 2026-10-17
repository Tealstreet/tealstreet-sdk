// Run alone: each compiled mutation must fail the constructed candle observation.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import {spawnSync} from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function run(name = 'candles_') {
  return spawnSync('cargo', ['+1.99.0', 'test', '--locked', '-p', 'safe-cex-venue-grvt', '--test', 'exchange_binding', '--jobs=1', name], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: {...process.env, CARGO_INCREMENTAL:'0'},
  });
}
const baseline = run();
assert.equal(baseline.status, 0, baseline.stdout + baseline.stderr);
for (const [file, anchor, replacement, name] of [
  ['data_reads.rs', 'or(or(row.get("vb"), row.get("volume_b")), row.get("volume")),',
    'or(row.get("o"), row.get("open")),', 'rest_candles_keep_source_aliases_time_units_and_distinct_fields'],
  ['public_feeds.rs', 'let volume = or(parser_host.as_ref(), &feed, "vb", "volume_b")?;',
    'let volume = or(parser_host.as_ref(), &feed, "o", "open")?;', 'socket_candles_keep_source_aliases_time_units_and_cleanup'],
]) {
  const target = path.join(root, 'safe-cex-venue-grvt/src', file);
  const original = fs.readFileSync(target);
  const source = original.toString();
  assert.equal(source.split(anchor).length, 2, 'one reviewed production statement');
  try {
    fs.writeFileSync(target, source.replace(anchor, replacement));
    const failure = run(name), output = failure.stdout + failure.stderr;
    assert.equal(failure.status, 101, output);
    assert.ok(output.includes('test result: FAILED') && output.includes('native volume'), output);
    console.log('CONTROL: compiled ' + file + ' mutation fails native volume');
  } finally {
    fs.writeFileSync(target, original);
    assert.deepEqual(fs.readFileSync(target), original);
  }
}
const restored = run();
assert.equal(restored.status, 0, restored.stdout + restored.stderr);
console.log('RESTORED: both constructed candle witnesses pass');
