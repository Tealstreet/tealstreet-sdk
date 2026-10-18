// Run alone: these controls temporarily mutate the production decoder guards.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
function test(name) {
  return spawnSync('cargo', ['test', '--locked', '-p', 'safe-cex-venue-kucoin', '--jobs=1',
    '--test', 'accepted_native_guards', ...(name ? [name, '--', '--exact', '--nocapture'] : [])], {
    cwd: root, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
    env: { ...process.env, RUSTC_BOOTSTRAP: '1' },
  });
}
const baseline = test();
assert.equal(baseline.status, 0, baseline.stdout + baseline.stderr);
const controls = [
  ['spot_types.rs', 'KucoinSpotWsServer', 'spot_server_rejects_valid_sequence'],
  ['spot_types.rs', 'KucoinSpotWsTokenData', 'spot_token_data_rejects_valid_sequence'],
  ['spot_types.rs', 'KucoinSpotWsTokenResponse', 'spot_token_response_rejects_valid_sequence'],
  ['account_mode.rs', 'KucoinClassicRuntimeDecision', 'classic_runtime_rejects_valid_sequence'],
  ['account_mode.rs', 'KucoinAccountMode', 'account_mode_string_shape'],
  ['ws_helpers.rs', 'KucoinPublicRole', 'public_role_string_shape'],
];
for (const [filename, owner, name] of controls) {
  const file = path.join(root, 'safe-cex-venue-kucoin/src', filename);
  const original = fs.readFileSync(file);
  const source = original.toString();
  const start = source.indexOf(`impl<'de> Deserialize<'de> for ${owner} {`);
  assert.ok(start >= 0);
  const end = source.indexOf('\n}\n', start) + 2;
  assert.ok(end > start);
  const body = source.slice(start, end);
  let mutated;
  if (name.endsWith('_string_shape')) {
    const argument = owner === 'KucoinAccountMode' ? 'd' : 'deserializer';
    const pattern = new RegExp(`let text = String::deserialize\\(${argument}\\)\\?;\\s*${owner}Wire::deserialize\\(serde::de::value::StringDeserializer::<D::Error>::new\\(\\s*text,\\s*\\)\\)`, 'g');
    assert.equal([...body.matchAll(pattern)].length, 1, `${owner}: unique string frontdoor`);
    mutated = body.replace(pattern, `${owner}Wire::deserialize(${argument})`);
  } else {
    const pattern = /safe_cex_types::strict_decode::object\(\s*(decoder|d)\s*,?\s*\)/g;
    assert.equal([...body.matchAll(pattern)].length, 1, `${owner}: unique object guard`);
    mutated = body.replace(pattern, '$1');
  }
  try {
    fs.writeFileSync(file, source.slice(0, start) + mutated + source.slice(end));
    const result = test(name);
    const output = result.stdout + result.stderr;
    assert.equal(result.status, 101, output);
    assert.ok(output.includes('test result: FAILED'), output);
    assert.ok(output.includes(name), output);
    assert.ok(output.includes(name.endsWith('_string_shape')
      ? 'tagged shape must be refused'
      : 'array admission must reject a complete valid sequence'), output);
    console.log(`CONTROL ${owner}: compiled native shape assertion rejects guard removal`);
  } finally {
    fs.writeFileSync(file, original);
    assert.deepEqual(fs.readFileSync(file), original);
  }
}
const restored = test();
assert.equal(restored.status, 0, restored.stdout + restored.stderr);
console.log('RESTORED: all seven native field/shape tests pass');
