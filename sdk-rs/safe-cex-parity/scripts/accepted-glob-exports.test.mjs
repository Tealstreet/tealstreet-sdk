import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { dependencyExportProvider, publicRootExportNames } from './accepted-glob-exports.mjs';

function fixture(check) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'dependency-glob-'));
  try {
    const source = path.join(directory, 'lib.rs');
    const artifact = path.join(directory, 'libdomain.rmeta');
    fs.writeFileSync(source, 'pub struct Market;\n');
    fs.writeFileSync(artifact, 'compiler identity fixture');
    const library = { name: 'domain', src_path: source, kind: ['lib'] };
    const pkg = { id: 'provider', name: 'domain-package', targets: [library] };
    const consumer = { id: 'consumer' };
    const metadata = { packages: [pkg], workspace_members: [pkg.id], resolve: { nodes: [{ id: consumer.id, deps: [{ name: 'domain_alias', pkg: pkg.id }] }] } };
    const doc = { external_crates: { 1: { name: 'domain', path: artifact } } };
    const artifacts = [{ package_id: pkg.id, target: library, profile: { test: false }, filenames: [artifact], features: ['native'] }];
    check({ directory, source, library, pkg, consumer, metadata, doc, artifacts });
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
}

test('dependency glob providers require the active binding compiler artifact and feature set', () => fixture(context => {
  const found = dependencyExportProvider('domain_alias', context);
  assert.equal(found.pkg.id, 'provider');
  assert.deepEqual(found.features, ['native']);
  assert.equal(dependencyExportProvider('missing', context), undefined);
  for (const mutate of [
    value => { value.metadata.resolve.nodes[0].deps.push(value.metadata.resolve.nodes[0].deps[0]); },
    value => { value.artifacts = []; },
    value => { value.artifacts.push(value.artifacts[0]); },
    value => { value.artifacts[0].package_id = 'substitution'; },
    value => { value.artifacts[0].profile.test = true; },
    value => { value.doc.external_crates[1].path = value.source; },
    value => { value.doc.external_crates[2] = value.doc.external_crates[1]; },
    value => { value.doc.external_crates[1].name = 'substitution'; },
    value => { value.artifacts[0].features = ['native', 'native']; },
  ]) {
    const changed = structuredClone(context); mutate(changed);
    assert.throws(() => dependencyExportProvider('domain_alias', changed), /dependency export assertion/);
  }
  const external = structuredClone(context); external.metadata.workspace_members = [];
  assert.equal(dependencyExportProvider('domain_alias', external), undefined);
}));

test('dependency glob names include public reexports and macros and refuse incomplete inventories', () => fixture(({ directory, source, library }) => {
  const doc = { root: 0, index: {
    0: { crate_id: 0, name: 'domain', span: { filename: source }, inner: { module: { is_crate: true, is_stripped: false, items: [1, 2, 3, 4] } } },
    1: { crate_id: 0, visibility: 'public', name: 'Market', inner: { struct: {} } },
    2: { crate_id: 0, visibility: 'public', name: null, inner: { use: { name: 'ModelField', is_glob: false } } },
    3: { crate_id: 0, visibility: 'crate', name: 'Private', inner: { struct: {} } },
    4: { crate_id: 0, visibility: 'public', name: 'format', inner: { macro: {} } },
  } };
  const context = { workspace: directory, library };
  assert.deepEqual(publicRootExportNames(doc, context), ['Market', 'ModelField', 'format']);
  const raw = structuredClone(doc); raw.index[4].name = 'r#format';
  assert.deepEqual(publicRootExportNames(raw, context), ['Market', 'ModelField', 'format']);
  for (const mutate of [
    value => { delete value.index[1]; },
    value => { delete value.index[4].visibility; },
    value => { value.index[0].inner.module.is_stripped = true; },
    value => { value.index[0].span.filename = 'missing.rs'; },
    value => { value.index[0].name = 'substitution'; },
    value => { value.index[2].inner.use.is_glob = true; },
    value => { delete value.index[2].inner.use.name; },
    value => { value.index[4].name = 'Market'; },
    value => { value.index[4].crate_id = 1; },
  ]) {
    const changed = structuredClone(doc); mutate(changed);
    assert.throws(() => publicRootExportNames(changed, context), /dependency export assertion/);
  }
}));

test('Bitrue dependency glob executes a real exported macro and refuses its hidden origin', { timeout: 1200000 }, () => {
  const sdk = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
  const types = path.join(sdk, 'safe-cex-types/src/lib.rs');
  const binding = path.join(sdk, 'safe-cex-venue-bitrue/src/binding/exchange.rs');
  const originalTypes = fs.readFileSync(types);
  const originalBinding = fs.readFileSync(binding);
  const run = (program, args, env = {}) => {
    const result = spawnSync(program, args, { cwd: sdk, env: { ...process.env, CARGO_BUILD_JOBS: '1', ...env }, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
    const output = `${result.stdout ?? ''}\n${result.stderr ?? ''}`;
    assert.ok(!result.error, output);
    assert.doesNotMatch(output, /error\[E\d+\]|could not compile/, `compile errors are not controls\n${output}`);
    return { ...result, output };
  };
  const inventory = () => run(process.execPath, ['safe-cex-parity/scripts/accepted-set.mjs', '--package=safe-cex-venue-bitrue', '--inventory', '--jobs=1'], {
    RUSTC_BOOTSTRAP: '1', CARGO_TARGET_DIR: path.join(sdk, 'target/accepted-inventory'),
  });
  const baseline = () => {
    const result = inventory();
    assert.equal(result.status, 0, result.output);
    assert.match(result.output, /safe-cex-venue-bitrue: [1-9]\d* compiler-confirmed Deserialize types/);
  };
  for (const [label, declaration, expected, refusal] of [
    ['workspace dependency glob macro shadow',
      '\n#[macro_export]\nmacro_rules! format { ($($arg:tt)*) => { ::std::format!("GLOB_EXPORT_SHADOW:{}", ::std::format!($($arg)*)) }; }\n',
      'GLOB_EXPORT_SHADOW:proof 7',
      /safe-cex-venue-bitrue\/src\/binding\/exchange\.rs::UNSUPPORTED_MACRO_IMPORT_SCOPE/],
    ['workspace dependency glob hidden provider expansion',
      '\nmacro_rules! dependency_hidden_exports { () => { #[cfg(not(doc))] pub use std::format; }; } dependency_hidden_exports!();\n',
      'proof 7',
      /safe-cex-types\/src\/lib\.rs::UNSUPPORTED_MODULE_MACRO:dependency_hidden_exports.*dependency export source effect/],
  ]) {
    try {
      baseline();
      fs.writeFileSync(types, Buffer.concat([originalTypes, Buffer.from(declaration)]));
      fs.writeFileSync(binding, Buffer.concat([originalBinding, Buffer.from(`\n#[cfg(test)] mod dependency_glob_native_control { #[test] fn actual_binding_import_selects_dependency_macro() { assert_eq!(super::format!("proof {}", 7), ${JSON.stringify(expected)}); } }\n`)]));
      // The binding's actual glob must select and execute the dependency macro.
      const native = run('cargo', ['test', '--locked', '--jobs', '1', '-p', 'safe-cex-venue-bitrue', '--lib', 'binding::exchange::dependency_glob_native_control::actual_binding_import_selects_dependency_macro', '--', '--exact', '--nocapture']);
      assert.equal(native.status, 0, native.output);
      assert.match(native.output, /1 passed; 0 failed/);
      const changed = inventory();
      assert.equal(changed.status, 1, changed.output);
      assert.match(changed.output, refusal);
      console.log(`${label}: named assertion; process exit 1`);
    } finally {
      fs.writeFileSync(types, originalTypes);
      fs.writeFileSync(binding, originalBinding);
    }
    baseline();
    assert.deepEqual(fs.readFileSync(types), originalTypes);
    assert.deepEqual(fs.readFileSync(binding), originalBinding);
  }
});
