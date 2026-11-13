import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { betaWasmInputPaths, betaWasmInputs, HARNESS_NOT_BUILT } from './beta-wasm-inputs.mjs';

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

test('server-side crates are ignored, but the wasm host and the algos are not', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  for (const dir of [
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
  const host = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  assert.notEqual(host, before);
  // The algos link into every family wasm.
  mkdirSync(join(root, 'sdk-rs/algos/exec/src'), { recursive: true });
  writeFileSync(join(root, 'sdk-rs/algos/exec/src/lib.rs'), 'pub fn exec() {}');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, host);
});

test('a size-budget edit leaves the fingerprint unchanged while a shipped host edit changes it', () => {
  const root = tree();
  const budget = join(root, 'sdk-rs/harness/wasm-budget.mjs');
  const host = join(root, 'sdk-rs/hosts/wasm/js/host.mjs');
  mkdirSync(dirname(host), { recursive: true });
  writeFileSync(budget, 'export const MAX_WASM_BYTES = 100;');
  writeFileSync(host, 'export const host = 1;');
  const before = betaWasmInputs(root, TOOLCHAIN).fingerprint;
  writeFileSync(budget, 'export const MAX_WASM_BYTES = 200;');
  assert.equal(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
  assert.ok(!betaWasmInputPaths(root).includes('sdk-rs/harness/wasm-budget.mjs'));
  writeFileSync(host, 'export const host = 2;');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, before);
});

test('harness files that only gate or measure a release leave the fingerprint alone', () => {
  const root = tree();
  const before = betaWasmInputs(root, TOOLCHAIN);
  for (const path of [
    'sdk-rs/harness/wasm-budget.test.mjs',
    'sdk-rs/harness/wasm-budget.mjs',
    'sdk-rs/harness/beta-wasm-release-guard.mjs',
    'sdk-rs/harness/ci-result.mjs',
    'sdk-rs/harness/cpu-ci.mjs',
    'sdk-rs/harness/bitget-paired-ci.mjs',
    'sdk-rs/harness/family-measure.mjs',
  ]) {
    writeFileSync(join(root, path), 'changed');
    assert.equal(betaWasmInputs(root, TOOLCHAIN).fingerprint, before.fingerprint, path);
  }
  // A build input in the harness, or a test file anywhere else, still counts.
  for (const path of ['sdk-rs/harness/build.sh', 'sdk-rs/harness/fixtures/a.test.mjs', 'sdk-rs/hosts/wasm/js/a.test.mjs']) {
    mkdirSync(join(root, path, '..'), { recursive: true });
    const counted = betaWasmInputs(root, TOOLCHAIN).fingerprint;
    writeFileSync(join(root, path), 'new');
    assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, counted, path);
  }
});

test('replay gate edits leave the fingerprint alone while fingerprint-definition edits still move it', () => {
  const root = tree();
  const gate = join(root, 'sdk-rs/harness/gate.mjs');
  const definition = join(root, 'sdk-rs/harness/beta-wasm-inputs.mjs');
  writeFileSync(gate, '// initial replay verdict');
  writeFileSync(definition, '// initial fingerprint definition');
  const before = betaWasmInputs(root, TOOLCHAIN);
  writeFileSync(gate, '// changed replay verdict');
  assert.deepEqual(betaWasmInputs(root, TOOLCHAIN), before);
  const inputs = betaWasmInputPaths(root);
  assert.ok(!inputs.includes('sdk-rs/harness/gate.mjs'));
  assert.ok(inputs.includes('sdk-rs/harness/beta-wasm-inputs.mjs'));
  writeFileSync(join(root, 'sdk-rs/harness/gate-helper.mjs'), '// still a build input');
  const helper = betaWasmInputs(root, TOOLCHAIN);
  assert.notEqual(helper.fingerprint, before.fingerprint, 'the exclusion matches only gate.mjs');
  writeFileSync(definition, '// changed fingerprint definition');
  assert.notEqual(betaWasmInputs(root, TOOLCHAIN).fingerprint, helper.fingerprint);
});

const REPO = resolve(fileURLToPath(new URL('../../', import.meta.url)));
const read = (rel) => readFileSync(join(REPO, rel), 'utf8');

/** Relative module specifiers a file imports: static, side-effect and literal dynamic imports. */
const specifiers = (text) =>
  [...text.matchAll(/(?:\bfrom\s*|\bimport\s*\(\s*|^\s*import\s+)['"](\.{1,2}\/[^'"]+)['"]/gm)].map((match) => match[1]);

/** Every sdk-rs file reachable from `roots` by relative imports. */
const importGraph = (roots) => {
  const seen = new Set();
  const pending = [...roots];
  while (pending.length > 0) {
    const rel = pending.pop();
    if (seen.has(rel)) continue;
    seen.add(rel);
    if (!/\.m?[jt]s$/.test(rel)) continue;
    // A template placeholder is generated source the script writes, not an import of its own.
    for (const specifier of specifiers(read(rel)).filter((spec) => !spec.includes('${'))) {
      const target = relative(REPO, resolve(REPO, dirname(rel), specifier)).split('\\').join('/');
      // Generated bindings are outputs, not source fingerprint inputs.
      if (target.startsWith('sdk-rs/hosts/wasm/pkg/')) continue;
      assert.ok(existsSync(join(REPO, target)), `${rel} imports missing ${target}`);
      if (target.startsWith('sdk-rs/')) pending.push(target);
    }
  }
  return seen;
};

// Build and loader inputs, plus the fingerprint the NX prebuild recomputes.
const BUILD_ROOTS = [
  'sdk-rs/harness/build.sh',
  'sdk-rs/hosts/wasm/js/venues.mjs',
  'sdk-rs/hosts/wasm/js/generate-beta-loader.mjs',
  'sdk-rs/harness/beta-wasm-inputs.mjs',
];

const exportFiles = (entry) => typeof entry === 'string' ? [entry] : Object.values(entry).flatMap(exportFiles);
const HOST_ROOTS = [...new Set(exportFiles(JSON.parse(read('sdk-rs/hosts/wasm/package.json')).exports)
  .filter((path) => path.startsWith('./js/') && path.endsWith('.mjs'))
  .map((path) => `sdk-rs/hosts/wasm/${path.slice(2)}`))];

test('the replay gate is unreachable from build, loader and shipped host imports', () => {
  const reached = importGraph([...BUILD_ROOTS, ...HOST_ROOTS]);
  assert.ok(reached.has('sdk-rs/hosts/wasm/js/managed-http.mjs'), 'transitive host dependencies are followed');
  assert.ok(!reached.has('sdk-rs/harness/gate.mjs'), 'replay verdicts never shape a built or shipped module');
});

test('the build roots are the scripts the release actually runs', () => {
  assert.match(read('sdk-rs/harness/build.sh'), /import\('\.\/hosts\/wasm\/js\/venues\.mjs'\)/);
  assert.deepEqual(specifiers(read('sdk-rs/harness/build.sh')), ['./hosts/wasm/js/venues.mjs']);
  const scripts = JSON.parse(read('package.json')).scripts;
  assert.equal(scripts['rust:build-wasm'], 'bash sdk-rs/harness/build.sh');
  assert.equal(scripts['rust:test-families'], 'node --test sdk-rs/harness/families.integration.mjs');
  const workflow = read('.github/workflows/rust-beta-wasm-release.yml');
  for (const used of ['rust:build-wasm', 'rust:test-families', 'sdk-rs/hosts/wasm/js/generate-beta-loader.mjs', 'sdk-rs/harness/beta-wasm-inputs.mjs']) {
    assert.ok(workflow.includes(used), used);
  }
});

test('every module the build, loader and shipped host import stays inside the fingerprint', () => {
  const inputs = new Set(betaWasmInputPaths(REPO));
  const reached = importGraph([...BUILD_ROOTS, ...HOST_ROOTS]);
  assert.ok(reached.has('sdk-rs/hosts/wasm/js/managed-http.mjs') && reached.has('sdk-rs/hosts/wasm/js/beta-venues.mjs'), 'the graph is followed');
  assert.ok(!reached.has('sdk-rs/harness/wasm-budget.mjs'), 'size gates do not shape artifacts');
  const outside = [...reached].filter((rel) => !inputs.has(rel));
  assert.deepEqual(outside, [], 'build inputs missing from the fingerprint');
});

test('release family tests import the size gate without making it a build input', () => {
  const inputs = new Set(betaWasmInputPaths(REPO));
  const reached = importGraph(['sdk-rs/harness/families.integration.mjs']);
  const budget = 'sdk-rs/harness/wasm-budget.mjs';
  assert.ok(reached.has(budget));
  assert.deepEqual([...reached].filter((rel) => !inputs.has(rel)), [budget]);
});

test('each excluded harness pattern still names a real file', () => {
  const harness = readdirSync(join(REPO, 'sdk-rs/harness')).map((name) => `sdk-rs/harness/${name}`);
  for (const pattern of HARNESS_NOT_BUILT) assert.ok(harness.some((rel) => pattern.test(rel)), String(pattern));
});
