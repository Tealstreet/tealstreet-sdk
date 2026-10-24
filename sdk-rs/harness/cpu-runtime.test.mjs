import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';
import { once } from 'node:events';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

// An absent new API is an adoption failure, never evidence of a production bug.
let api = {};
try { api = await import('./cpu-runtime.mjs'); } catch (error) {
  if (error.code !== 'ERR_MODULE_NOT_FOUND') throw error;
}
const environment = { ImageOS: 'fixture-os', ImageVersion: 'fixture-image-1', arch: 'x64', nodeVersion: 'v22.1.0', nodeVersions: { node: '22.1.0', v8: 'fixture-v8' } };
function fixture(t) {
  assert.equal(typeof api.sealRuntime, 'function', 'shared runtime capsule API exists');
  const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cpu-runtime-test-')));
  t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  const root = path.join(temp, 'workspace');
  const destination = path.join(temp, 'capsule');
  const nodePath = path.join(temp, 'fixture-node');
  fs.mkdirSync(root);
  fs.writeFileSync(nodePath, 'exact fixture executable\n', { mode: 0o755 });
  for (const name of ['source', 'packages/pkg', 'node_modules', 'cache']) fs.mkdirSync(path.join(root, name), { recursive: true });
  fs.writeFileSync(path.join(root, 'source/main.mjs'), 'export default 7;\n');
  fs.writeFileSync(path.join(root, 'packages/pkg/.hidden'), 'hidden dependency');
  fs.symlinkSync('../packages/pkg', path.join(root, 'node_modules/pkg'));
  const options = { root, destination, nodePath, paths: ['source', 'packages', 'node_modules'], cachePath: 'cache', environment };
  return { root, destination, options };
}
function removeRuntime(root) {
  for (const name of ['source', 'packages', 'node_modules', 'cache', '.cpu-runtime']) fs.rmSync(path.join(root, name), { recursive: true, force: true });
}
function editManifest(directory, edit) {
  const file = path.join(directory, 'manifest.json');
  const manifest = JSON.parse(fs.readFileSync(file, 'utf8'));
  edit(manifest);
  fs.writeFileSync(file, JSON.stringify(manifest));
}

test('capsule transports exact bytes, executable modes, hidden files and internal workspace links', (t) => {
  const { root, destination, options } = fixture(t);
  const sealed = api.sealRuntime(options);
  assert.equal(sealed.nodePath, path.join(root, '.cpu-runtime/node'));
  assert.equal(fs.readFileSync(sealed.nodePath, 'utf8'), 'exact fixture executable\n');
  const initialCache = api.cacheDigest(root, 'cache');
  removeRuntime(root);
  const restored = api.restoreRuntime({ root, directory: destination, environment });
  assert.equal(restored.fingerprint, sealed.fingerprint);
  assert.equal(fs.readFileSync(path.join(root, 'source/main.mjs'), 'utf8'), 'export default 7;\n');
  assert.equal(fs.readFileSync(path.join(root, 'node_modules/pkg/.hidden'), 'utf8'), 'hidden dependency');
  assert.equal(fs.readlinkSync(path.join(root, 'node_modules/pkg')), '../packages/pkg');
  assert.equal(fs.statSync(restored.nodePath).mode & 0o777, 0o755);
  assert.equal(api.cacheDigest(root, 'cache'), initialCache);
  assert.equal(api.verifyRuntime({ root, directory: destination, environment }).fingerprint, sealed.fingerprint);
  assert.equal(api.restoreRuntime({ root, directory: destination, environment }).fingerprint, sealed.fingerprint);
});

test('ordinary cache warming is separate from immutable runtime verification', (t) => {
  const { root, destination, options } = fixture(t);
  const sealed = api.sealRuntime(options);
  const empty = api.cacheDigest(root, 'cache');
  fs.writeFileSync(path.join(root, 'cache/tsx-loader'), 'ordinary generated loader bytes');
  assert.notEqual(api.cacheDigest(root, 'cache'), empty);
  assert.equal(api.verifyRuntime({ root, directory: destination, environment }).fingerprint, sealed.fingerprint);
  assert.throws(() => api.restoreRuntime({ root, directory: destination, environment }), /collision|unexpected/);
  fs.writeFileSync(path.join(root, 'source/main.mjs'), 'changed runtime');
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /mismatch|collision/);
});

test('workspace, image, architecture and exact Node identities fail closed before restore writes', (t) => {
  const { root, destination, options } = fixture(t);
  api.sealRuntime(options);
  removeRuntime(root);
  for (const field of ['ImageOS', 'ImageVersion', 'arch', 'nodeVersion']) {
    assert.throws(() => api.restoreRuntime({ root, directory: destination, environment: { ...environment, [field]: 'other' } }), /environment|version/);
    assert.equal(fs.existsSync(path.join(root, 'source')), false);
  }
  assert.throws(() => api.restoreRuntime({ root, directory: destination, environment: { ...environment, nodeVersions: { ...environment.nodeVersions, v8: 'other' } } }), /environment/);
  const other = path.join(path.dirname(root), 'other');
  fs.mkdirSync(other);
  assert.throws(() => api.restoreRuntime({ root: other, directory: destination, environment }), /workspace/);
});

for (const [label, current, differences] of [
  ['runner image', { ...environment, ImageVersion: 'other-image' },
    [{ field: 'ImageVersion', expected: 'fixture-image-1', actual: 'other-image' }]],
  ['Node component', { ...environment, nodeVersions: { ...environment.nodeVersions, v8: 'other-v8' } },
    [{ field: 'nodeVersions.v8', expected: 'fixture-v8', actual: 'other-v8' }]],
  ['removed Node component', { ...environment, nodeVersions: { node: '22.1.0' } },
    [{ field: 'nodeVersions.v8', expected: 'fixture-v8', actual: '<missing>' }]],
  ['added Node component', { ...environment, nodeVersions: { ...environment.nodeVersions, uv: 'other-uv' } },
    [{ field: 'nodeVersions.uv', expected: '<missing>', actual: 'other-uv' }]],
  ['all differing fields in stable order', { ...environment, arch: 'arm64', ImageVersion: 'other-image',
    nodeVersion: 'v22.2.0', nodeVersions: { node: '22.2.0', v8: 'other-v8' } },
    [{ field: 'ImageVersion', expected: 'fixture-image-1', actual: 'other-image' },
      { field: 'arch', expected: 'x64', actual: 'arm64' },
      { field: 'nodeVersion', expected: 'v22.1.0', actual: 'v22.2.0' },
      { field: 'nodeVersions.node', expected: '22.1.0', actual: '22.2.0' },
      { field: 'nodeVersions.v8', expected: 'fixture-v8', actual: 'other-v8' }]],
]) {
  test(`environment mismatch reports ${label} before any restore writes`, (t) => {
    const { root, destination, options } = fixture(t);
    api.sealRuntime(options);
    removeRuntime(root);
    assert.throws(() => api.restoreRuntime({ root, directory: destination, environment: current }), (error) => {
      const prefix = 'CPU runtime: environment identity mismatch: ';
      assert.ok(error.message.startsWith(prefix), 'mismatch includes field-level evidence');
      assert.deepEqual(JSON.parse(error.message.slice(prefix.length)), differences);
      return true;
    });
    assert.deepEqual(fs.readdirSync(root), [], 'identity failure never starts restoration');
  });
}

test('selection rejects traversal, overlapping roots, reserved Node and mutable cache collisions', (t) => {
  const { options } = fixture(t);
  for (const paths of [['../outside'], ['/absolute'], ['source/../source'], ['source//x'], ['source', 'source/main.mjs'], ['source', 'source-other', 'source/main.mjs'], ['.cpu-runtime'], ['cache'], ['source', 'source']]) {
    assert.throws(() => api.sealRuntime({ ...options, paths }), /path|overlap|reserved|duplicate/);
    assert.equal(fs.existsSync(options.destination), false);
  }
});

test('escaping, dangling, mutable-target and cyclic symlinks are never transported', (t) => {
  const { root, options } = fixture(t);
  for (const target of ['../../outside', '../cache', 'missing', 'bad']) {
    const link = path.join(root, 'source/bad');
    fs.symlinkSync(target, link);
    assert.throws(() => api.sealRuntime(options), /link|target|escape|cycle/);
    fs.unlinkSync(link);
    assert.equal(fs.existsSync(options.destination), false);
  }
});

test('absolute internal links preserve their original target only at the exact original workspace', (t) => {
  const { root, options, destination } = fixture(t);
  fs.symlinkSync(path.join(root, 'packages/pkg'), path.join(root, 'source/absolute'));
  api.sealRuntime(options);
  removeRuntime(root);
  api.restoreRuntime({ root, directory: destination, environment });
  assert.equal(fs.readlinkSync(path.join(root, 'source/absolute')), path.join(root, 'packages/pkg'));
});

test('preflight rejects conflicting existing files and symlink ancestors without overwriting peers', (t) => {
  const { root, options, destination } = fixture(t);
  api.sealRuntime(options);
  removeRuntime(root);
  fs.mkdirSync(path.join(root, 'source'));
  fs.writeFileSync(path.join(root, 'source/main.mjs'), 'keep me');
  assert.throws(() => api.restoreRuntime({ root, directory: destination, environment }), /collision/);
  assert.equal(fs.readFileSync(path.join(root, 'source/main.mjs'), 'utf8'), 'keep me');
  assert.equal(fs.existsSync(path.join(root, 'packages')), false);
  fs.rmSync(path.join(root, 'source'), { recursive: true });
  fs.symlinkSync(path.dirname(root), path.join(root, 'source'));
  assert.throws(() => api.restoreRuntime({ root, directory: destination, environment }), /collision|symlink/);
});

test('manifest traversal, duplicate entries and unsealed capsule members are rejected', (t) => {
  const { root, options, destination } = fixture(t);
  api.sealRuntime(options);
  const original = fs.readFileSync(path.join(destination, 'manifest.json'));
  for (const mutate of [
    (m) => { m.entries[0].path = '../outside'; },
    (m) => { m.entries.push(m.entries[0]); },
    (m) => { m.node.path = '/arbitrary-node'; },
    (m) => { m.extra = true; },
  ]) {
    editManifest(destination, mutate);
    assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }));
    fs.writeFileSync(path.join(destination, 'manifest.json'), original);
  }
  fs.writeFileSync(path.join(destination, 'unclaimed'), 'not sealed');
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /capsule|unexpected/);
});

test('blob corruption, executable mode drift and extra immutable members are detected', (t) => {
  const { root, options, destination } = fixture(t);
  const sealed = api.sealRuntime(options);
  fs.chmodSync(sealed.nodePath, 0o644);
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /collision|mismatch/);
  fs.chmodSync(sealed.nodePath, 0o755);
  fs.writeFileSync(path.join(root, 'source/extra'), 'unclaimed runtime input');
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /unexpected/);
  fs.unlinkSync(path.join(root, 'source/extra'));
  const blob = path.join(destination, 'blobs', sealed.manifest.node.sha256);
  fs.writeFileSync(blob, 'wrong Node bytes');
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /blob/);
});

test('fingerprints ignore filesystem mtimes and caller selection ordering, but bind initial cache bytes', (t) => {
  const { root, options, destination } = fixture(t);
  const first = api.sealRuntime(options);
  const cloned = JSON.parse(JSON.stringify(first.manifest));
  assert.equal(api.runtimeFingerprint(cloned), first.fingerprint);
  fs.utimesSync(path.join(root, 'source/main.mjs'), new Date(0), new Date(0));
  assert.equal(api.verifyRuntime({ root, directory: destination, environment }).fingerprint, first.fingerprint);
  fs.rmSync(path.join(root, '.cpu-runtime'), { recursive: true });
  fs.rmSync(destination, { recursive: true });
  const second = api.sealRuntime({ ...options, paths: [...options.paths].reverse() });
  assert.equal(second.fingerprint, first.fingerprint);
  fs.rmSync(path.join(root, '.cpu-runtime'), { recursive: true });
  fs.rmSync(destination, { recursive: true });
  fs.writeFileSync(path.join(root, 'cache/initial-loader'), 'different initial state');
  assert.notEqual(api.sealRuntime(options).fingerprint, first.fingerprint);
});

test('nested immutable/cache siblings share parent metadata without restoring it twice', (t) => {
  const { root, options, destination } = fixture(t);
  fs.mkdirSync(path.join(root, 'nested/runtime'), { recursive: true });
  fs.mkdirSync(path.join(root, 'nested/cache'));
  fs.writeFileSync(path.join(root, 'nested/runtime/main'), 'nested bytes');
  api.sealRuntime({ ...options, paths: ['nested/runtime'], cachePath: 'nested/cache' });
  fs.rmSync(path.join(root, 'nested'), { recursive: true });
  fs.rmSync(path.join(root, '.cpu-runtime'), { recursive: true });
  api.restoreRuntime({ root, directory: destination, environment });
  assert.equal(fs.readFileSync(path.join(root, 'nested/runtime/main'), 'utf8'), 'nested bytes');
});

test('link/parent components resolve after following links, and file ancestors cannot conceal targets', (t) => {
  const { root, options } = fixture(t);
  fs.symlinkSync('../packages/pkg', path.join(root, 'source/alias'));
  fs.symlinkSync('alias/../pkg/.hidden', path.join(root, 'source/via-parent'));
  const sealed = api.sealRuntime(options);
  assert.equal(api.verifyRuntime({ root, directory: options.destination, environment }).fingerprint, sealed.fingerprint);
  fs.rmSync(path.join(root, '.cpu-runtime'), { recursive: true });
  fs.rmSync(options.destination, { recursive: true });
  fs.unlinkSync(path.join(root, 'source/via-parent'));
  fs.symlinkSync('../packages/pkg/.hidden/child', path.join(root, 'source/via-parent'));
  assert.throws(() => api.sealRuntime(options), /non-directory/);
});

test('selected symlink ancestors and symlinked capsule members fail closed', (t) => {
  const { root, options, destination } = fixture(t);
  assert.throws(() => api.sealRuntime({ ...options, paths: ['node_modules/pkg/.hidden'] }), /ancestor/);
  api.sealRuntime(options);
  const blob = fs.readdirSync(path.join(destination, 'blobs'))[0];
  const blobPath = path.join(destination, 'blobs', blob);
  fs.unlinkSync(blobPath);
  fs.symlinkSync(options.nodePath, blobPath);
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }));
});

test('actual socket entries and privileged file modes cannot enter a capsule', async (t) => {
  const { root, options } = fixture(t);
  const server = net.createServer();
  server.listen(path.join(root, 'source/socket'));
  await once(server, 'listening');
  try { assert.throws(() => api.sealRuntime(options), /special filesystem/); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
  const source = path.join(root, 'source/main.mjs');
  fs.chmodSync(source, 0o4755);
  try { assert.throws(() => api.sealRuntime(options), /special mode/); }
  finally { fs.chmodSync(source, 0o644); }
});

test('malicious entry below a symlink and special entry types are rejected before restoring files', (t) => {
  const { root, options, destination } = fixture(t);
  api.sealRuntime(options);
  removeRuntime(root);
  const original = fs.readFileSync(path.join(destination, 'manifest.json'));
  for (const mutate of [
    (m) => { m.entries.push({ path: 'node_modules/pkg/child', type: 'directory', mode: 0o755 }); m.entries.sort((a, b) => a.path < b.path ? -1 : 1); },
    (m) => { m.entries[0].type = 'socket'; },
    (m) => { m.entries[0].mode = 0o4755; },
  ]) {
    editManifest(destination, mutate);
    assert.throws(() => api.restoreRuntime({ root, directory: destination, environment }));
    assert.equal(fs.existsSync(path.join(root, '.cpu-runtime')), false);
    fs.writeFileSync(path.join(destination, 'manifest.json'), original);
  }
});

test('seal rejects hardlink aliases between immutable source and mutable cache', (t) => {
  const { root, options } = fixture(t);
  fs.linkSync(path.join(root, 'source/main.mjs'), path.join(root, 'cache/alias'));
  assert.throws(() => api.sealRuntime(options), /hardlink|multiple links/);
  assert.equal(fs.existsSync(options.destination), false);
  assert.equal(fs.existsSync(path.join(root, '.cpu-runtime')), false);
});

test('verify rejects a hardlink alias introduced after sealing', (t) => {
  const { root, options, destination } = fixture(t);
  api.sealRuntime(options);
  fs.linkSync(path.join(root, 'source/main.mjs'), path.join(root, 'cache/alias'));
  assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }), /hardlink|multiple links/);
});

test('restore rejects a preexisting exact-byte hardlinked file before writing peers', (t) => {
  const { root, options, destination } = fixture(t);
  api.sealRuntime(options);
  removeRuntime(root);
  fs.mkdirSync(path.join(root, 'source'));
  const existing = path.join(root, 'source/main.mjs');
  fs.writeFileSync(existing, 'export default 7;\n');
  fs.linkSync(existing, path.join(path.dirname(root), 'outside-alias'));
  assert.throws(() => api.restoreRuntime({ root, directory: destination, environment }), /hardlink|multiple links/);
  assert.equal(fs.existsSync(path.join(root, 'packages')), false);
  assert.equal(fs.readFileSync(existing, 'utf8'), 'export default 7;\n');
});

test('shallow directory anchors permit package links without capturing ignored package files', (t) => {
  const { root, options, destination } = fixture(t);
  fs.mkdirSync(path.join(root, 'packages/pkg/src'));
  fs.writeFileSync(path.join(root, 'packages/pkg/package.json'), '{"name":"fixture"}');
  fs.writeFileSync(path.join(root, 'packages/pkg/src/main.mjs'), 'export default 1;');
  fs.writeFileSync(path.join(root, 'packages/pkg/ignored.log'), 'never transport ambient package logs');
  const paths = ['node_modules', 'packages/pkg/package.json', 'packages/pkg/src/main.mjs'];
  const sealed = api.sealRuntime({ ...options, paths, directoryAnchors: ['packages/pkg'] });
  assert.deepEqual(sealed.manifest.directoryAnchors, ['packages/pkg']);
  assert.equal(sealed.manifest.entries.some((entry) => entry.path.endsWith('ignored.log') || entry.path.endsWith('.hidden')), false);
  assert.equal(api.verifyRuntime({ root, directory: destination, environment }).fingerprint, sealed.fingerprint);
  removeRuntime(root);
  api.restoreRuntime({ root, directory: destination, environment });
  assert.equal(fs.readFileSync(path.join(root, 'node_modules/pkg/package.json'), 'utf8'), '{"name":"fixture"}');
  assert.equal(fs.readFileSync(path.join(root, 'node_modules/pkg/src/main.mjs'), 'utf8'), 'export default 1;');
  assert.equal(fs.existsSync(path.join(root, 'packages/pkg/ignored.log')), false);
  // Anchors permit unrelated files to exist; they never expand the sealed set.
  fs.writeFileSync(path.join(root, 'packages/pkg/later-ignored.log'), 'still unselected');
  assert.equal(api.verifyRuntime({ root, directory: destination, environment }).fingerprint, sealed.fingerprint);
  fs.symlinkSync('../packages/pkg/later-ignored.log', path.join(root, 'node_modules/unselected'));
  fs.rmSync(path.join(root, '.cpu-runtime'), { recursive: true });
  fs.rmSync(destination, { recursive: true });
  assert.throws(() => api.sealRuntime({ ...options, paths, directoryAnchors: ['packages/pkg'] }), /target.*not sealed/);
});

test('directory anchors reject duplicate, unsafe, mutable, special and non-directory roots', (t) => {
  const { root, options } = fixture(t);
  for (const directoryAnchors of [['packages/pkg', 'packages/pkg'], ['../outside'], ['cache'], ['.cpu-runtime'], ['source/main.mjs'], ['node_modules/pkg']]) {
    assert.throws(() => api.sealRuntime({ ...options, directoryAnchors }), /anchor|path|directory|reserved/);
    assert.equal(fs.existsSync(options.destination), false);
    assert.equal(fs.existsSync(path.join(root, '.cpu-runtime')), false);
  }
});

test('anchor manifest metadata cannot smuggle an unselected file or change anchor type', (t) => {
  const { root, options, destination } = fixture(t);
  const sealed = api.sealRuntime({ ...options, paths: ['source'], directoryAnchors: ['packages/pkg'] });
  assert.deepEqual(sealed.manifest.directoryAnchors, ['packages/pkg']);
  const original = fs.readFileSync(path.join(destination, 'manifest.json'));
  for (const mutate of [
    (m) => { m.directoryAnchors.push('packages/pkg'); },
    (m) => { m.directoryAnchors = ['cache']; },
    (m) => { m.entries.push({ path: 'packages/pkg/injected', type: 'file', mode: 0o644, sha256: sealed.manifest.node.sha256, bytes: sealed.manifest.node.bytes }); m.entries.sort((a, b) => a.path < b.path ? -1 : 1); },
    (m) => { const entry = m.entries.find((value) => value.path === 'packages/pkg'); entry.type = 'file'; entry.sha256 = m.node.sha256; entry.bytes = m.node.bytes; },
  ]) {
    editManifest(destination, mutate);
    assert.throws(() => api.verifyRuntime({ root, directory: destination, environment }));
    fs.writeFileSync(path.join(destination, 'manifest.json'), original);
  }
});

test('pure cache entry digest agrees with disk and needs no original filesystem', (t) => {
  const { root, options } = fixture(t);
  fs.writeFileSync(path.join(root, 'cache/loader'), 'retained generated loader');
  fs.symlinkSync('loader', path.join(root, 'cache/loader-link'));
  const sealed = api.sealRuntime(options);
  const entries = sealed.manifest.cacheEntries;
  const expected = api.cacheDigest(root, 'cache');
  assert.equal(api.cacheEntriesDigest({ root, cachePath: 'cache', entries }), expected);
  removeRuntime(root);
  fs.rmdirSync(root);
  assert.equal(api.cacheEntriesDigest({ root, cachePath: 'cache', entries }), expected);
  for (const badRoot of ['relative', '/', `${root}/..`, `${root}//nested`])
    assert.throws(() => api.cacheEntriesDigest({ root: badRoot, cachePath: 'cache', entries }), /workspace/);
  for (const mutate of [
    (e) => { e.push(e[0]); },
    (e) => { e[1].path = '../outside'; },
    (e) => { e[0] = { path: 'cache', type: 'file', mode: 0o644, bytes: 0, sha256: '0'.repeat(64) }; },
    (e) => { e.find((entry) => entry.type === 'symlink').target = '../unsealed'; },
    (e) => { e.reverse(); },
  ]) {
    const changed = structuredClone(entries); mutate(changed);
    assert.throws(() => api.cacheEntriesDigest({ root, cachePath: 'cache', entries: changed }));
  }
  assert.throws(() => api.cacheEntriesDigest({ root, cachePath: '/cache', entries }), /path/);
});

test('sparse selected files retain only their real directory ancestors and explicit anchors', t => {
  const { root, destination, options } = fixture(t);
  options.paths = ['source/main.mjs', 'packages/pkg/.hidden', 'node_modules'];
  options.directoryAnchors = ['packages/pkg'];
  const sealed = api.sealRuntime(options);
  assert.equal(api.runtimeFingerprint(sealed.manifest), sealed.fingerprint);
  assert.ok(sealed.manifest.entries.some(entry => entry.path === 'source' && entry.type === 'directory'));
  assert.ok(sealed.manifest.entries.some(entry => entry.path === 'packages/pkg' && entry.type === 'directory'));
  for (const name of ['source/main.mjs-extra', 'source/other.mjs', 'packages/pkg/.hidden-extra', 'packages/peer']) {
    const manifest = structuredClone(sealed.manifest);
    manifest.entries.push({ path: name, type: 'file', mode: 0o644, bytes: 0, sha256: '0'.repeat(64) });
    manifest.entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
    assert.throws(() => api.runtimeFingerprint(manifest), /unselected immutable entry/);
  }
  const poisoned = structuredClone(sealed.manifest);
  Object.assign(poisoned.entries.find(entry => entry.path === 'source'), { type: 'symlink', target: 'packages' });
  assert.throws(() => api.runtimeFingerprint(poisoned), /unselected immutable entry/);
});

test('selected directory descendants are admitted without admitting adjacent prefix names', t => {
  const { options } = fixture(t);
  const sealed = api.sealRuntime(options);
  const nested = structuredClone(sealed.manifest);
  nested.entries.push({ path: 'source/nested', type: 'directory', mode: 0o755 });
  nested.entries.push({ path: 'source/nested/file', type: 'file', mode: 0o644, bytes: 0, sha256: '0'.repeat(64) });
  nested.entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  assert.equal(typeof api.runtimeFingerprint(nested), 'string');
  for (const name of ['source-extra', 'node_modules-extra', 'packages-extra']) {
    const adjacent = structuredClone(sealed.manifest);
    adjacent.entries.push({ path: name, type: 'directory', mode: 0o755 });
    adjacent.entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
    assert.throws(() => api.runtimeFingerprint(adjacent), /unselected immutable entry/);
  }
});
