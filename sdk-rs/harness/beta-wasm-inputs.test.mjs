import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { betaWasmInputs } from './beta-wasm-inputs.mjs';

const TOOLCHAIN = { wasmBindgen: '0.2.129', wasmOpt: 'required', profile: 'release', target: 'wasm32-unknown-unknown' };

const tree = () => {
  const root = mkdtempSync(join(tmpdir(), 'beta-inputs-'));
  for (const [path, text] of [
    ['sdk-rs/Cargo.lock', 'lock'],
    ['sdk-rs/rust-toolchain.toml', 'channel = "1.99.0"'],
    ['sdk-rs/engine/src/lib.rs', 'pub fn a() {}'],
    ['sdk-rs/harness/install-binaryen.sh', 'version=132'],
  ]) {
    mkdirSync(join(root, path, '..'), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
};

test('the fingerprint is deterministic and counts every input', () => {
  const root = tree();
  const first = betaWasmInputs(root, TOOLCHAIN);
  assert.equal(first.files, 4);
  assert.equal(betaWasmInputs(root, { ...TOOLCHAIN }).fingerprint, first.fingerprint);
});

test('a source mutation changes the fingerprint (stale pins are refused)', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  writeFileSync(join(root, 'sdk-rs/engine/src/lib.rs'), 'pub fn b() {}');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
});

test('an added source file or a toolchain change also changes it', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  writeFileSync(join(root, 'sdk-rs/engine/src/extra.rs'), '');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
  const clean = tree();
  assert.notEqual(
    betaWasmInputs(clean, { ...TOOLCHAIN, wasmBindgen: '0.2.130' }).fingerprint,
    betaWasmInputs(clean, TOOLCHAIN).fingerprint,
  );
});

test('generated trees are ignored, so installing the packages does not move it', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  for (const dir of ['sdk-rs/hosts/wasm/pkg/web/okx', 'sdk-rs/target/release', 'sdk-rs/harness/out/ci']) {
    mkdirSync(join(root, dir), { recursive: true });
    writeFileSync(join(root, dir, 'artifact'), 'generated');
  }
  assert.equal(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
});

test('symlinks and an incomplete toolchain record are refused', () => {
  const root = tree();
  symlinkSync('/etc/hosts', join(root, 'sdk-rs/engine/src/linked.rs'));
  assert.throws(() => betaWasmInputs(root, TOOLCHAIN), /symlink/);
  assert.throws(() => betaWasmInputs(tree(), { ...TOOLCHAIN, profile: '' }), /lacks profile/);
});

test('server-side crates are ignored, but the wasm host is not', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  for (const dir of [
    'sdk-rs/algos/exec/src',
    'sdk-rs/hosts/server/src',
    'sdk-rs/hosts/candle-server/src',
    'sdk-rs/hosts/remote-signer/src',
    'sdk-rs/hosts/native/src',
  ]) {
    mkdirSync(join(root, dir), { recursive: true });
    writeFileSync(join(root, dir, 'lib.rs'), 'pub fn server() {}');
  }
  assert.equal(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
  mkdirSync(join(root, 'sdk-rs/hosts/wasm/src'), { recursive: true });
  writeFileSync(join(root, 'sdk-rs/hosts/wasm/src/lib.rs'), 'pub fn host() {}');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
});
