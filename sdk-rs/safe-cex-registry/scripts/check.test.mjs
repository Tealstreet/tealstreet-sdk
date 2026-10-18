import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtempSync, writeFileSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const checker = fileURLToPath(new URL('./check.mjs', import.meta.url));
test('registry oracle rejects corrupted claims and native divergence', () => {
  const built = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path', 'sdk-rs/Cargo.toml', '-p', 'safe-cex-registry', '--example', 'registry-data'], { cwd: root, encoding: 'utf8' });
  assert.equal(built.status, 0, built.error?.message ?? built.stderr);
  const live = spawnSync(process.execPath, [checker], { cwd: root, encoding: 'utf8', env: { ...process.env, CARGO: '/intentionally-invalid-registry-cargo-override' } });
  assert.equal(live.status, 0, `live oracle must ignore CARGO executable overrides: ${live.stderr}`);
  const good = JSON.parse(built.stdout);
  const dir = mkdtempSync(join(tmpdir(), 'registry-oracle-'));
  const path = join(dir, 'actual.json');
  const run = (value, extra = []) => {
    writeFileSync(path, JSON.stringify(value));
    return spawnSync(process.execPath, [checker, '--actual', path, ...extra], { cwd: root, encoding: 'utf8' });
  };
  try {
    const positive = run(good);
    assert.equal(positive.status, 0, positive.stderr);
    const controls = [
      ['module', v => v.registry.find(row => row.name === 'bybitdemo').module = './bybit/bybit.exchange', /Rust registry differs/],
      ['constructor', v => v.registry.find(row => row.name === 'bybitdemo').constructor = 'BybitExchange', /Rust registry differs/],
      ['names/order', v => v.names.reverse(), /registered names\/order differ/],
      ['ping', v => v.ping = 999000, /EXTERNAL_PUBLIC_DATA_PING_THRESHOLD_MS/],
      ['reconnect', v => v.reconnect = 999000000, /EXTERNAL_PUBLIC_DATA_RECONNECT_THRESHOLD_MS/],
      ['subscription keys', v => v.keys[0].key = 'WRONG', /subscription key mapping differs/],
      ['channel members', v => v.channels.push('funding'), /GatewayChannel members differ/],
      ['auth extension', v => v.channels = v.channels.filter(x => x !== 'auth'), /AssertionError/],
      ['auth reply', v => v.authReply.channel = 'ticker', /AssertionError/],
      ['null rejection', v => v.rejectsNullSymbol = false, /Rust rejects the same null field/],
      ['astral truncation', v => v.malformedAstral = ['wrong', 'wrong'], /source UTF-16 slicing with typed String replacement/],
    ];
    for (const [label, mutate, diagnostic] of controls) {
      const bad = structuredClone(good);
      mutate(bad);
      const negative = run(bad);
      assert.equal(negative.status, 1, `oracle accepted ${label}: ${negative.stdout}\n${negative.stderr}`);
      assert.match(negative.stderr, diagnostic);
      assert.doesNotMatch(negative.stdout, /Registry equivalence:/);
    }
    const nativePath = join(dir, 'registry.native.ts');
    const native = readFileSync(join(root, 'packages/safe-cex/src/exchanges/registry.native.ts'), 'utf8');
    const badNative = native.replace('./bybit/bybitdemo.exchange', './bybit/bybit.exchange');
    assert.notEqual(badNative, native);
    writeFileSync(nativePath, badNative);
    const nativeFailure = run(good, ['--native', nativePath]);
    assert.equal(nativeFailure.status, 1, 'oracle accepted divergent native registry');
    assert.match(nativeFailure.stderr, /web\/native descriptors differ/);
    const gatewayPath = join(dir, 'gateway.ts');
    const gateway = readFileSync(join(root, 'packages/safe-cex/src/exchanges/external-public-data.ts'), 'utf8');
    const duplicated = gateway.replace("'ticker' | 'tickers'", "'ticker' | 'ticker' | 'tickers'");
    assert.notEqual(duplicated, gateway);
    writeFileSync(gatewayPath, duplicated);
    const duplicateResult = run(good, ['--gateway', gatewayPath]);
    assert.equal(duplicateResult.status, 0, duplicateResult.stderr);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
