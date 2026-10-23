import assert from 'node:assert/strict';
import fs from 'node:fs';
import net from 'node:net';
import { once } from 'node:events';
import os from 'node:os';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { cacheDigest, cacheEntriesDigest } from './cpu-runtime.mjs';

let api = {};
try { api = await import('./cpu-cache.mjs'); }
catch (error) { if (error.code !== 'ERR_MODULE_NOT_FOUND') throw error; }
const hash = (value) => createHash('sha256').update(value).digest('hex');
const key = 'a'.repeat(64), prefix = `cpu-collection/cache/${key}`;
function fixture(t) {
  assert.equal(typeof api.captureCache, 'function', 'cache byte retention API exists');
  assert.equal(typeof api.verifyCache, 'function', 'retained cache verification API exists');
  const temp = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'cpu-cache-'))), root = path.join(temp, 'workspace');
  fs.mkdirSync(root); t.after(() => fs.rmSync(temp, { recursive: true, force: true }));
  const cachePath = 'loader/tmp'; fs.mkdirSync(path.join(root, cachePath), { recursive: true });
  const destination = path.join(root, 'sdk-rs/harness/out', prefix);
  return { root, cachePath, destination, cache: path.join(root, cachePath) };
}
function bundle(destination, name = prefix) {
  const entries = new Map([[`${name}/manifest.json`, fs.readFileSync(path.join(destination, 'manifest.json'))]]);
  for (const blob of fs.readdirSync(path.join(destination, 'blobs')))
    entries.set(`${name}/blobs/${blob}`, fs.readFileSync(path.join(destination, 'blobs', blob)));
  return entries;
}
function editManifest(entries, mutate, name = prefix) {
  const result = new Map(entries), manifest = JSON.parse(result.get(`${name}/manifest.json`));
  mutate(manifest); result.set(`${name}/manifest.json`, Buffer.from(JSON.stringify(manifest))); return result;
}

test('empty final cache retains its complete ancestor metadata and exact runtime digest', (t) => {
  const options = fixture(t), expected = cacheDigest(options.root, options.cachePath);
  const captured = api.captureCache(options);
  assert.equal(captured.digest, expected); assert.equal(cacheDigest(options.root, options.cachePath), expected);
  const entries = bundle(options.destination), verified = api.verifyCache({ entries, prefix, expectedDigest: expected });
  assert.equal(verified.digest, expected); assert.equal(verified.manifest.workspaceRoot, options.root);
  assert.deepEqual(verified.manifest.entries.map((entry) => [entry.path, entry.type]), [['loader', 'directory'], ['loader/tmp', 'directory']]);
  assert.equal(entries.size, 1, 'empty caches still retain a manifest with no invented blob');
});

test('all binary, hidden, zero-length and repeated cache files retain bytes and modes without altering cache', (t) => {
  const options = fixture(t); fs.mkdirSync(path.join(options.cache, 'nested'));
  const bytes = Buffer.from([0, 255, 1, 10]);
  fs.writeFileSync(path.join(options.cache, 'nested/loader.bin'), bytes, { mode: 0o640 });
  fs.writeFileSync(path.join(options.cache, '.hidden'), 'generated transform');
  fs.writeFileSync(path.join(options.cache, 'duplicate'), bytes);
  fs.writeFileSync(path.join(options.cache, 'empty'), ''); fs.chmodSync(options.cache, 0o750);
  const expected = cacheDigest(options.root, options.cachePath), original = fs.readFileSync(path.join(options.cache, 'nested/loader.bin'));
  const capture = api.captureCache(options), entries = bundle(options.destination);
  assert.equal(capture.digest, expected); assert.equal(cacheDigest(options.root, options.cachePath), expected);
  assert.deepEqual(fs.readFileSync(path.join(options.cache, 'nested/loader.bin')), original);
  assert.deepEqual(entries.get(`${prefix}/blobs/${hash(bytes)}`), bytes);
  assert.equal(entries.size, 4, 'identical independent files share one content blob');
  const checked = api.verifyCache({ entries, prefix, expectedDigest: expected });
  assert.equal(checked.manifest.entries.find((entry) => entry.path.endsWith('loader.bin')).mode, 0o640);
  assert.equal(checked.manifest.entries.find((entry) => entry.path === options.cachePath).mode, 0o750);
});

test('safe cache links preserve exact targets and runtime graph semantics', (t) => {
  const options = fixture(t); fs.mkdirSync(path.join(options.cache, 'nested'));
  fs.writeFileSync(path.join(options.cache, 'nested/transform'), 'generated');
  fs.symlinkSync('nested/transform', path.join(options.cache, 'relative'));
  fs.symlinkSync(path.join(options.cache, 'nested'), path.join(options.cache, 'absolute'));
  fs.symlinkSync('absolute/../nested/transform', path.join(options.cache, 'via-parent'));
  const expected = cacheDigest(options.root, options.cachePath); api.captureCache(options);
  const verified = api.verifyCache({ entries: bundle(options.destination), prefix, expectedDigest: expected });
  assert.equal(verified.digest, expected);
  assert.equal(verified.manifest.entries.find((entry) => entry.path.endsWith('/absolute')).target, path.join(options.cache, 'nested'));
  assert.equal(fs.readlinkSync(path.join(options.cache, 'relative')), 'nested/transform');
});

test('snapshot destinations are immutable per case and second-run snapshots stay distinct', (t) => {
  const options = fixture(t), first = api.captureCache(options), retained = bundle(options.destination);
  assert.throws(() => api.captureCache(options), /collision|exist/);
  assert.deepEqual(bundle(options.destination), retained);
  fs.writeFileSync(path.join(options.cache, 'later'), 'another case final cache');
  const otherPrefix = `cpu-second/cache/${'b'.repeat(64)}`, destination = path.join(options.root, 'sdk-rs/harness/out', otherPrefix);
  const second = api.captureCache({ ...options, destination }); assert.notEqual(second.digest, first.digest);
  assert.equal(api.verifyCache({ entries: bundle(destination, otherPrefix), prefix: otherPrefix, expectedDigest: second.digest }).digest, second.digest);
  assert.equal(api.verifyCache({ entries: retained, prefix, expectedDigest: first.digest }).digest, first.digest);
});

test('missing, added, renamed, truncated and corrupted blobs cannot satisfy a retained digest', (t) => {
  const options = fixture(t); fs.writeFileSync(path.join(options.cache, 'transform'), 'generated bytes');
  const { digest } = api.captureCache(options), entries = bundle(options.destination), blob = [...entries.keys()].find((name) => name.includes('/blobs/'));
  for (const mutate of [
    (map) => map.delete(blob),
    (map) => { const damaged = Buffer.from(map.get(blob)); damaged[0] ^= 1; map.set(blob, damaged); },
    (map) => map.set(blob, map.get(blob).subarray(0, -1)),
    (map) => map.set(`${prefix}/blobs/${'f'.repeat(64)}`, Buffer.from('extra')),
    (map) => map.set(`${prefix}/unknown`, Buffer.from('extra')),
    (map) => { map.set(`${blob}x`, map.get(blob)); map.delete(blob); },
    (map) => map.delete(`${prefix}/manifest.json`),
  ]) { const bad = new Map(entries); mutate(bad); assert.throws(() => api.verifyCache({ entries: bad, prefix, expectedDigest: digest })); }
  assert.throws(() => api.verifyCache({ entries, prefix, expectedDigest: '0'.repeat(64) }), /digest/);
});

test('path, metadata, link graph and strict JSON tampering fail closed', (t) => {
  const options = fixture(t); fs.writeFileSync(path.join(options.cache, 'transform'), 'generated'); api.captureCache(options);
  const entries = bundle(options.destination), expectedDigest = cacheDigest(options.root, options.cachePath);
  for (const mutate of [
    (manifest) => { manifest.extra = true; },
    (manifest) => { manifest.entries[0].path = '../outside'; },
    (manifest) => { manifest.cachePath = '../outside'; },
    (manifest) => { manifest.entries.push(manifest.entries[0]); },
    (manifest) => { manifest.entries[0].mode = 0o4755; },
    (manifest) => { manifest.entries[0].mode ^= 1; },
    (manifest) => { manifest.entries.reverse(); },
    (manifest) => { manifest.entries.pop(); },
    (manifest) => { manifest.entries[0].type = 'socket'; },
    (manifest) => { manifest.entries.push({ path: 'loader/tmp/link', type: 'symlink', mode: 0o777, target: '../../outside' }); manifest.entries.sort((a, b) => a.path < b.path ? -1 : 1); },
  ]) assert.throws(() => api.verifyCache({ entries: editManifest(entries, mutate), prefix, expectedDigest }));
  for (const raw of [Buffer.from([0xff]), Buffer.from('{"schema":1,"schema":1}'), Buffer.from('{"schema":1e999}')]) {
    const bad = new Map(entries); bad.set(`${prefix}/manifest.json`, raw);
    assert.throws(() => api.verifyCache({ entries: bad, prefix, expectedDigest }));
  }
  for (const name of ['../outside', '/absolute', `${prefix}/../escape`, `${prefix}/blobs/../../escape`]) {
    const bad = new Map(entries); bad.set(name, Buffer.from('data'));
    assert.throws(() => api.verifyCache({ entries: bad, prefix, expectedDigest }));
  }
});

test('unsafe cache trees refuse before snapshot creation and preserve source files', (t) => {
  for (const kind of ['outside-link', 'dangling-link', 'cycle', 'hardlink', 'special-mode']) {
    const options = fixture(t), original = path.join(options.cache, 'original'); fs.writeFileSync(original, 'keep');
    if (kind === 'outside-link') { fs.writeFileSync(path.join(options.root, 'outside'), 'outside'); fs.symlinkSync('../../outside', path.join(options.cache, 'bad')); }
    if (kind === 'dangling-link') fs.symlinkSync('missing', path.join(options.cache, 'bad'));
    if (kind === 'cycle') fs.symlinkSync('bad', path.join(options.cache, 'bad'));
    if (kind === 'hardlink') fs.linkSync(original, path.join(options.cache, 'bad'));
    if (kind === 'special-mode') fs.chmodSync(original, 0o4755);
    assert.throws(() => api.captureCache(options));
    assert.equal(fs.existsSync(options.destination), false); assert.equal(fs.readFileSync(original, 'utf8'), 'keep');
  }
});

test('socket caches and symlink/parent destination tricks cannot write snapshots', async (t) => {
  const options = fixture(t), server = net.createServer(); server.listen(path.join(options.cache, 'socket')); await once(server, 'listening');
  try { assert.throws(() => api.captureCache(options), /special/); }
  finally { await new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve())); }
  const external = path.join(path.dirname(options.root), 'external'); fs.mkdirSync(external);
  fs.mkdirSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection'), { recursive: true });
  fs.symlinkSync(external, path.join(options.root, 'sdk-rs/harness/out/cpu-collection/cache'));
  assert.throws(() => api.captureCache(options), /symlink|directory/); assert.deepEqual(fs.readdirSync(external), []);
  fs.unlinkSync(path.join(options.root, 'sdk-rs/harness/out/cpu-collection/cache'));
  for (const destination of [path.join(external, key), options.destination + '/../' + key, path.join(options.cache, key)])
    assert.throws(() => api.captureCache({ ...options, destination }));
});

test('oversized sparse files and declared archive caches are refused before large reads', (t) => {
  const options = fixture(t), large = path.join(options.cache, 'large');
  fs.writeFileSync(large, ''); fs.truncateSync(large, 512 * 1024 * 1024 + 1);
  assert.throws(() => api.captureCache(options), /byte limit/); assert.equal(fs.existsSync(options.destination), false);
  fs.unlinkSync(large); fs.writeFileSync(path.join(options.cache, 'transform'), 'small');
  api.captureCache(options); const entries = bundle(options.destination);
  const bad = editManifest(entries, (manifest) => { manifest.entries.find((entry) => entry.type === 'file').bytes = 512 * 1024 * 1024 + 1; });
  const manifest = JSON.parse(bad.get(`${prefix}/manifest.json`));
  const expectedDigest = cacheEntriesDigest({ root: options.root, cachePath: options.cachePath, entries: manifest.entries });
  assert.throws(() => api.verifyCache({ entries: bad, prefix, expectedDigest }), /byte limit/);
  const oversized = new Map(entries); oversized.set(`${prefix}/manifest.json`, Buffer.alloc(16 * 1024 * 1024 + 1));
  assert.throws(() => api.verifyCache({ entries: oversized, prefix, expectedDigest }), /oversized cache manifest/);
});
