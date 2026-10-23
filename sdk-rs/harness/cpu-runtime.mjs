// Exact replay inputs travel as regular content-addressed blobs. Artifact upload
// permissions and symlink handling therefore cannot change the restored runtime.
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const NODE_DIRECTORY = '.cpu-runtime';
const NODE_PATH = `${NODE_DIRECTORY}/node`;
const SHA = /^[a-f0-9]{64}$/;
const digest = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
}
function fail(message) { throw new Error(`CPU runtime: ${message}`); }
function keys(value, expected, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value) || Object.keys(value).sort().join('\0') !== [...expected].sort().join('\0')) fail(`invalid ${label} fields`);
}
function relative(value) {
  if (typeof value !== 'string' || !value || value.includes('\\') || value.includes('\0') || value.includes(':') || value.split('/').some((part) => !part || part === '.' || part === '..') || value.split('/').includes('.git')) fail(`unsafe path ${JSON.stringify(value)}`);
  return value;
}
const within = (name, directory) => name === directory || name.startsWith(`${directory}/`);
const overlaps = (a, b) => within(a, b) || within(b, a);
function lstat(file) {
  try { return fs.lstatSync(file); } catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
function rootSyntax(root) {
  if (typeof root !== 'string' || !path.isAbsolute(root) || path.resolve(root) !== root || root === path.parse(root).root) fail('workspace must be an absolute canonical path below filesystem root');
  return root;
}
function rootPath(root) {
  rootSyntax(root);
  const resolved = fs.realpathSync(root);
  if (resolved !== root || !fs.statSync(root).isDirectory()) fail('workspace root has a symlink or is not a directory');
  // realpath also rules out symlink ancestors of the workspace.
  return root;
}
function ancestors(root, name) {
  const parts = relative(name).split('/');
  for (let i = 1; i < parts.length; i++) {
    const state = lstat(path.join(root, ...parts.slice(0, i)));
    if (state && (!state.isDirectory() || state.isSymbolicLink())) fail(`symlink/non-directory ancestor of ${name}`);
  }
}
function environmentIdentity(environment) {
  const value = environment ?? {
    ImageOS: process.env.ImageOS,
    ImageVersion: process.env.ImageVersion,
    arch: process.arch,
    nodeVersion: process.version,
    nodeVersions: { ...process.versions },
  };
  keys(value, ['ImageOS', 'ImageVersion', 'arch', 'nodeVersion', 'nodeVersions'], 'environment');
  for (const field of ['ImageOS', 'ImageVersion', 'arch', 'nodeVersion']) if (typeof value[field] !== 'string' || !value[field]) fail(`missing environment ${field}`);
  if (!value.nodeVersions || typeof value.nodeVersions !== 'object' || Array.isArray(value.nodeVersions) || !Object.keys(value.nodeVersions).length || Object.values(value.nodeVersions).some((v) => typeof v !== 'string' || !v)) fail('invalid Node versions');
  if (`v${value.nodeVersions.node}` !== value.nodeVersion) fail('inconsistent Node version');
  return JSON.parse(canonical(value));
}
function selection(paths, cachePath) {
  relative(cachePath);
  if (overlaps(cachePath, NODE_DIRECTORY)) fail('cache path overlaps reserved Node path');
  if (!Array.isArray(paths) || !paths.length) fail('paths must be a nonempty explicit selection');
  const result = paths.map(relative).sort();
  for (let i = 0; i < result.length; i++) {
    if (overlaps(result[i], NODE_DIRECTORY)) fail('reserved Node path');
    if (overlaps(result[i], cachePath)) fail('immutable path/cache overlap');
    if (result.slice(0, i).some((previous) => overlaps(result[i], previous))) fail('duplicate or overlapping paths');
  }
  return result;
}
function anchorSelection(anchors, cachePath) {
  if (!Array.isArray(anchors)) fail('directory anchors must be an array');
  const result = anchors.map(relative).sort();
  for (let index = 0; index < result.length; index++) {
    if (overlaps(result[index], cachePath) || overlaps(result[index], NODE_DIRECTORY)) fail('directory anchor overlaps cache/reserved Node path');
    if (index && result[index] === result[index - 1]) fail('duplicate directory anchor');
  }
  return result;
}
function fileBytes(file) {
  const fd = fs.openSync(file, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const before = fs.fstatSync(fd);
    if (!before.isFile()) fail(`not a regular file: ${file}`);
    // A capsule restores independent files, so accepting shared native inodes
    // would change aliasing between the original and restored workspaces.
    if (before.nlink !== 1) fail(`hardlink/multiple links on regular file: ${file}`);
    const bytes = fs.readFileSync(fd);
    const after = fs.fstatSync(fd);
    if (after.nlink !== 1) fail(`hardlink/multiple links on regular file: ${file}`);
    if (before.dev !== after.dev || before.ino !== after.ino || before.size !== after.size || before.mtimeMs !== after.mtimeMs || before.ctimeMs !== after.ctimeMs || bytes.length !== after.size) fail(`file changed while sealing: ${file}`);
    return bytes;
  } finally { fs.closeSync(fd); }
}
function collect(root, roots, blob, anchors = []) {
  const entries = new Map();
  function visit(name, descend) {
    ancestors(root, name);
    const file = path.join(root, name);
    const state = lstat(file);
    if (!state) fail(`missing runtime path ${name}`);
    if (state.mode & 0o7000) fail(`special mode on ${name}`);
    const mode = state.mode & 0o777;
    let entry;
    if (state.isDirectory()) entry = { path: name, type: 'directory', mode };
    else if (state.isFile()) {
      const bytes = fileBytes(file);
      entry = { path: name, type: 'file', mode, bytes: bytes.length, sha256: digest(bytes) };
      if (blob) blob(entry.sha256, bytes);
    } else if (state.isSymbolicLink()) entry = { path: name, type: 'symlink', mode, target: fs.readlinkSync(file) };
    else fail(`special filesystem entry ${name}`);
    entries.set(name, entry);
    if (descend && state.isDirectory()) for (const child of fs.readdirSync(file).sort()) visit(`${name}/${child}`, true);
  }
  for (const rootName of roots) {
    const parts = rootName.split('/');
    for (let i = 1; i < parts.length; i++) visit(parts.slice(0, i).join('/'), false);
    visit(rootName, true);
  }
  // Workspace package links need their target directory's metadata, not its
  // ignored/generated contents. Selected files beneath it remain explicit.
  for (const anchor of anchors) {
    ancestors(root, anchor);
    if (!lstat(path.join(root, anchor))?.isDirectory()) fail(`directory anchor is not a real directory: ${anchor}`);
    const parts = anchor.split('/');
    for (let i = 1; i < parts.length; i++) visit(parts.slice(0, i).join('/'), false);
    visit(anchor, false);
  }
  return [...entries.values()].sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}
function validateEntries(entries, roots, label, anchors = []) {
  if (!Array.isArray(entries) || !entries.length) fail(`empty ${label} entries`);
  const map = new Map();
  const selectedPaths = new Set(roots), parentPaths = new Set();
  for (const root of [...roots, ...anchors]) {
    const parts = root.split('/');
    for (let i = 1; i <= parts.length; i++) parentPaths.add(parts.slice(0, i).join('/'));
  }
  for (const entry of entries) {
    relative(entry.path);
    const fields = ['path', 'type', 'mode'];
    if (entry.type === 'file') fields.push('bytes', 'sha256');
    else if (entry.type === 'symlink') fields.push('target');
    else if (entry.type !== 'directory') fail(`special ${label} entry`);
    keys(entry, fields, 'entry');
    if (!Number.isInteger(entry.mode) || entry.mode < 0 || entry.mode > 0o777) fail('invalid mode');
    if (entry.type === 'file' && (!Number.isSafeInteger(entry.bytes) || entry.bytes < 0 || !SHA.test(entry.sha256))) fail('invalid file metadata');
    if (entry.type === 'symlink' && (typeof entry.target !== 'string' || !entry.target || entry.target.includes('\0') || entry.target.includes('\\'))) fail('invalid link target');
    if (map.has(entry.path)) fail('duplicate entry path');
    let selected = selectedPaths.has(entry.path), ancestor = entry.path;
    while (!selected && ancestor.includes('/')) {
      ancestor = ancestor.slice(0, ancestor.lastIndexOf('/'));
      selected = selectedPaths.has(ancestor);
    }
    const parent = parentPaths.has(entry.path);
    if (!selected && (!parent || entry.type !== 'directory')) fail(`unselected ${label} entry ${entry.path}`);
    map.set(entry.path, entry);
  }
  if (canonical(entries) !== canonical([...entries].sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0))) fail('unsorted entries');
  for (const name of roots) if (!map.has(name)) fail(`missing selected root ${name}`);
  for (const anchor of anchors) if (map.get(anchor)?.type !== 'directory') fail(`missing/invalid directory anchor ${anchor}`);
  for (const entry of entries) {
    const parts = entry.path.split('/');
    for (let i = 1; i < parts.length; i++) if (map.get(parts.slice(0, i).join('/'))?.type !== 'directory') fail('symlink or missing ancestor');
  }
  return map;
}
function validateLinks(root, map, allowedRoots, anchors = []) {
  // Resolve components in filesystem order: normalizing link/../x first would
  // conceal escapes through intermediate links.
  function resolve(tokens, current = [], visited = new Set()) {
    for (const [index, part] of tokens.entries()) {
      if (!part || part === '.') continue;
      if (part === '..') { if (!current.length) fail('link escapes workspace'); current.pop(); continue; }
      current.push(part);
      const name = current.join('/');
      const entry = map.get(name);
      if (!entry) fail(`link target is not sealed: ${name}`);
      if (entry.type === 'symlink') {
        if (visited.has(name)) fail(`link cycle at ${name}`);
        const next = new Set(visited).add(name);
        let target = entry.target;
        let base = current.slice(0, -1);
        if (path.isAbsolute(target)) {
          if (!target.startsWith(`${root}/`)) fail('absolute link escapes workspace');
          target = target.slice(root.length + 1); base = [];
        }
        current = resolve(target.split('/'), base, next);
      }
      if (index < tokens.length - 1 && map.get(current.join('/'))?.type !== 'directory') fail('link target has a non-directory ancestor');
    }
    return current;
  }
  for (const entry of map.values()) if (entry.type === 'symlink') {
    const target = resolve(entry.path.split('/')).join('/');
    if (!allowedRoots.some((r) => within(target, r)) && !anchors.includes(target)) fail('link target crosses immutable/cache boundary');
  }
}
function validateManifest(manifest) {
  keys(manifest, ['schema', 'workspaceRoot', 'environment', 'immutablePaths', 'directoryAnchors', 'cachePath', 'entries', 'cacheEntries', 'node'], 'manifest');
  if (manifest.schema !== 1 || typeof manifest.workspaceRoot !== 'string' || !path.isAbsolute(manifest.workspaceRoot) || path.resolve(manifest.workspaceRoot) !== manifest.workspaceRoot) fail('invalid schema/workspace');
  environmentIdentity(manifest.environment);
  if (!Array.isArray(manifest.immutablePaths) || !manifest.immutablePaths.includes(NODE_DIRECTORY)) fail('missing Node selection');
  const selected = selection(manifest.immutablePaths.filter((p) => p !== NODE_DIRECTORY), manifest.cachePath);
  const anchors = anchorSelection(manifest.directoryAnchors, manifest.cachePath);
  if (canonical(anchors) !== canonical(manifest.directoryAnchors)) fail('invalid directory anchor ordering');
  if (canonical(manifest.immutablePaths) !== canonical([...selected, NODE_DIRECTORY].sort())) fail('invalid immutable paths');
  const immutable = validateEntries(manifest.entries, manifest.immutablePaths, 'immutable', anchors);
  const cache = validateEntries(manifest.cacheEntries, [manifest.cachePath], 'cache');
  for (const [name, entry] of cache) if (immutable.has(name) && canonical(immutable.get(name)) !== canonical(entry)) fail('conflicting immutable/cache ancestor');
  if (cache.get(manifest.cachePath)?.type !== 'directory') fail('cache path must be a directory');
  validateLinks(manifest.workspaceRoot, immutable, manifest.immutablePaths, anchors);
  validateLinks(manifest.workspaceRoot, cache, [manifest.cachePath]);
  keys(manifest.node, ['path', 'sha256', 'bytes', 'mode'], 'Node');
  const node = immutable.get(NODE_PATH);
  if (manifest.node.path !== NODE_PATH || !node || node.type !== 'file' || !(node.mode & 0o111) || canonical(manifest.node) !== canonical({ path: node.path, sha256: node.sha256, bytes: node.bytes, mode: node.mode })) fail('invalid Node executable binding');
  return manifest;
}

export function runtimeFingerprint(manifest) { return digest(canonical(validateManifest(manifest))); }

export function cacheDigest(root, cachePath) {
  root = rootPath(root); relative(cachePath);
  const entries = collect(root, [cachePath]);
  return cacheEntriesDigest({ root, cachePath, entries });
}

/** Validate an already byte-authenticated archive cache inventory without
 * accessing the original workspace. Filesystem and ZIP proofs share one digest. */
export function cacheEntriesDigest({ root, cachePath, entries }) {
  rootSyntax(root); relative(cachePath);
  const map = validateEntries(entries, [cachePath], 'cache');
  if (map.get(cachePath).type !== 'directory') fail('cache path must be a directory');
  validateLinks(root, map, [cachePath]);
  return digest(canonical(entries));
}

function capsuleLocation(root, directory, mustExist) {
  if (typeof directory !== 'string' || !path.isAbsolute(directory) || path.resolve(directory) !== directory || directory === root || directory.startsWith(`${root}/`)) fail('capsule directory must be canonical and outside workspace');
  const parent = path.dirname(directory);
  if (fs.realpathSync(parent) !== parent) fail('capsule has a symlink ancestor');
  const state = lstat(directory);
  if (mustExist ? !state || !state.isDirectory() || state.isSymbolicLink() : state) fail('capsule directory collision or missing directory');
  return directory;
}

// Synchronous on purpose: callers seal/verify between replay child processes,
// with no replay or asynchronous native callback running during these operations.
export function sealRuntime({ root, destination, paths, nodePath = process.execPath, cachePath, environment, directoryAnchors = [] }) {
  root = rootPath(root);
  const identity = environmentIdentity(environment);
  const selected = selection(paths, cachePath);
  const anchors = anchorSelection(directoryAnchors, cachePath);
  capsuleLocation(root, destination, false);
  if (lstat(path.join(root, NODE_DIRECTORY))) fail('reserved Node directory collision');
  const nodeSource = fs.realpathSync(nodePath);
  const nodeState = fs.statSync(nodeSource);
  if (!nodeState.isFile() || !(nodeState.mode & 0o111) || nodeState.mode & 0o7000) fail('Node source must be a regular executable');
  // Validate all native paths before creating the controlled executable.
  const initialEntries = collect(root, selected, undefined, anchors);
  const initialCache = collect(root, [cachePath]);
  const immutableMap = validateEntries(initialEntries, selected, 'immutable', anchors);
  validateLinks(root, immutableMap, selected, anchors);
  const cacheMap = validateEntries(initialCache, [cachePath], 'cache');
  if (cacheMap.get(cachePath).type !== 'directory') fail('cache path must be a directory');
  validateLinks(root, cacheMap, [cachePath]);
  const nodeBytes = fileBytes(nodeSource);
  let createdNode = false;
  let createdCapsule = false;
  try {
    fs.mkdirSync(path.join(root, NODE_DIRECTORY), { mode: 0o755 });
    createdNode = true;
    fs.chmodSync(path.join(root, NODE_DIRECTORY), 0o755);
    fs.writeFileSync(path.join(root, NODE_PATH), nodeBytes, { flag: 'wx', mode: nodeState.mode & 0o777 });
    fs.chmodSync(path.join(root, NODE_PATH), nodeState.mode & 0o777);
    fs.mkdirSync(destination, { mode: 0o755 });
    createdCapsule = true;
    fs.mkdirSync(path.join(destination, 'blobs'));
    const blob = (hash, bytes) => {
      const output = path.join(destination, 'blobs', hash);
      if (!lstat(output)) fs.writeFileSync(output, bytes, { flag: 'wx' });
    };
    const immutablePaths = [...selected, NODE_DIRECTORY].sort();
    const entries = collect(root, immutablePaths, blob, anchors);
    const cacheEntries = collect(root, [cachePath], blob);
    const node = entries.find((entry) => entry.path === NODE_PATH);
    const manifest = validateManifest({ schema: 1, workspaceRoot: root, environment: identity, immutablePaths, directoryAnchors: anchors, cachePath, entries, cacheEntries, node: { path: node.path, sha256: node.sha256, bytes: node.bytes, mode: node.mode } });
    fs.writeFileSync(path.join(destination, 'manifest.json'), `${canonical(manifest)}\n`, { flag: 'wx' });
    return verifyRuntime({ root, directory: destination, environment: identity });
  } catch (error) {
    if (createdNode) fs.rmSync(path.join(root, NODE_DIRECTORY), { recursive: true, force: true });
    if (createdCapsule) fs.rmSync(destination, { recursive: true, force: true });
    throw error;
  }
}

function readCapsule(root, directory, environment) {
  rootPath(root); capsuleLocation(root, directory, true);
  const members = fs.readdirSync(directory).sort();
  if (canonical(members) !== canonical(['blobs', 'manifest.json'])) fail('unexpected capsule members');
  for (const [name, type] of [['manifest.json', 'file'], ['blobs', 'directory']]) {
    const state = fs.lstatSync(path.join(directory, name));
    if (state.isSymbolicLink() || (type === 'file' ? !state.isFile() : !state.isDirectory())) fail('invalid capsule member');
  }
  const manifest = validateManifest(JSON.parse(fileBytes(path.join(directory, 'manifest.json')).toString('utf8')));
  if (manifest.workspaceRoot !== root) fail('workspace identity mismatch');
  if (canonical(manifest.environment) !== canonical(environmentIdentity(environment))) fail('environment identity mismatch');
  const hashes = new Map();
  for (const entry of [...manifest.entries, ...manifest.cacheEntries]) if (entry.type === 'file') {
    if (hashes.has(entry.sha256) && hashes.get(entry.sha256) !== entry.bytes) fail('inconsistent blob length');
    hashes.set(entry.sha256, entry.bytes);
  }
  if (canonical(fs.readdirSync(path.join(directory, 'blobs')).sort()) !== canonical([...hashes.keys()].sort())) fail('unexpected or missing capsule blobs');
  for (const [hash, length] of hashes) {
    const bytes = fileBytes(path.join(directory, 'blobs', hash));
    if (bytes.length !== length || digest(bytes) !== hash) fail(`blob mismatch ${hash}`);
  }
  return manifest;
}
function checkExisting(root, entries, roots, allowMissing) {
  const expected = new Map(entries.map((entry) => [entry.path, entry]));
  const missing = [];
  for (const entry of entries) {
    ancestors(root, entry.path);
    const file = path.join(root, entry.path);
    const state = lstat(file);
    if (!state) { if (!allowMissing) fail(`runtime mismatch: missing ${entry.path}`); missing.push(entry); continue; }
    if ((state.mode & 0o7777) !== entry.mode) fail(`mode collision ${entry.path}`);
    if (entry.type === 'directory' ? !state.isDirectory() : entry.type === 'file' ? !state.isFile() : !state.isSymbolicLink()) fail(`type collision ${entry.path}`);
    if (entry.type === 'file') {
      const bytes = fileBytes(file);
      if (bytes.length !== entry.bytes || digest(bytes) !== entry.sha256) fail(`file collision ${entry.path}`);
    }
    if (entry.type === 'symlink' && fs.readlinkSync(file) !== entry.target) fail(`link collision ${entry.path}`);
  }
  function extra(name) {
    const state = lstat(path.join(root, name));
    if (!state) return;
    if (!expected.has(name)) fail(`unexpected runtime entry ${name}`);
    if (state.isDirectory()) for (const child of fs.readdirSync(path.join(root, name))) extra(`${name}/${child}`);
  }
  for (const name of roots) extra(name);
  return missing;
}
function result(root, manifest) {
  return { manifest, fingerprint: runtimeFingerprint(manifest), nodePath: path.join(root, NODE_PATH), cachePath: path.join(root, manifest.cachePath), initialCacheDigest: digest(canonical(manifest.cacheEntries)), cacheDigest: cacheDigest(root, manifest.cachePath) };
}

export function verifyRuntime({ root, directory, environment }) {
  const manifest = readCapsule(root, directory, environment);
  checkExisting(root, manifest.entries, manifest.immutablePaths, false);
  return result(root, manifest);
}

export function restoreRuntime({ root, directory, environment }) {
  const manifest = readCapsule(root, directory, environment);
  const merged = new Map();
  for (const entry of [...manifest.entries, ...manifest.cacheEntries]) {
    if (merged.has(entry.path) && canonical(merged.get(entry.path)) !== canonical(entry)) fail('conflicting immutable/cache ancestor');
    merged.set(entry.path, entry);
  }
  const all = [...merged.values()];
  const missing = checkExisting(root, all, [...manifest.immutablePaths, manifest.cachePath], true);
  const created = [];
  try {
    const order = { directory: 0, file: 1, symlink: 2 };
    missing.sort((a, b) => order[a.type] - order[b.type] || a.path.split('/').length - b.path.split('/').length);
    for (const entry of missing) {
      ancestors(root, entry.path);
      const file = path.join(root, entry.path);
      if (lstat(file)) fail(`restore collision ${entry.path}`);
      if (entry.type === 'directory') fs.mkdirSync(file, { mode: 0o700 });
      else if (entry.type === 'file') fs.copyFileSync(path.join(directory, 'blobs', entry.sha256), file, fs.constants.COPYFILE_EXCL);
      else fs.symlinkSync(entry.target, file);
      created.push(entry);
      if (entry.type === 'file') fs.chmodSync(file, entry.mode);
    }
    for (const entry of [...created].reverse()) if (entry.type === 'directory') fs.chmodSync(path.join(root, entry.path), entry.mode);
    return verifyRuntime({ root, directory, environment });
  } catch (error) {
    for (const entry of created.reverse()) {
      try { if (entry.type === 'directory') fs.rmdirSync(path.join(root, entry.path)); else fs.unlinkSync(path.join(root, entry.path)); } catch { /* Preserve unrelated concurrent changes. */ }
    }
    throw error;
  }
}
