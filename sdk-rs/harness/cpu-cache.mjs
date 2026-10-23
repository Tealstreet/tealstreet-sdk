// Preserve each final loader cache as inert metadata plus regular SHA256 blobs.
// The live cache is never cleared, copied over, chmodded or traversed via links.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { cacheEntriesDigest } from './cpu-runtime.mjs';
import { parseCohortJson } from './cpu-cohort.mjs';

const MAX_BYTES = 512 * 1024 * 1024, MAX_ENTRIES = 65535, MAX_MANIFEST_BYTES = 16 * 1024 * 1024;
const HASH = /^[a-f0-9]{64}$/;
const sha = (bytes) => createHash('sha256').update(bytes).digest('hex');
const check = (condition, message) => { if (!condition) throw new Error(`CPU cache: ${message}`); };
const canonical = (value) => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
};
const within = (name, directory) => name === directory || name.startsWith(`${directory}/`);
function relative(name) {
  check(typeof name === 'string' && name && !/[\\:\0]/.test(name) &&
    name.split('/').every((part) => part && !['.', '..', '.git'].includes(part)), 'unsafe relative path');
  return name;
}
function rootIdentity(root) {
  check(typeof root === 'string' && path.isAbsolute(root) && path.resolve(root) === root && root !== path.parse(root).root &&
    fs.realpathSync(root) === root && fs.statSync(root).isDirectory(), 'workspace must be a canonical real directory');
}
function state(filename) {
  try { return fs.lstatSync(filename); }
  catch (error) { if (error.code === 'ENOENT') return null; throw error; }
}
function independentBytes(filename, expected, available) {
  const fd = fs.openSync(filename, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW);
  try {
    const before = fs.fstatSync(fd);
    check(before.isFile() && before.nlink === 1 && before.dev === expected.dev && before.ino === expected.ino &&
      before.mode === expected.mode, 'file identity/hardlink changed during capture');
    check(Number.isSafeInteger(before.size) && before.size >= 0 && before.size <= available, 'cache byte limit exceeded');
    const bytes = Buffer.allocUnsafe(before.size);
    for (let offset = 0; offset < bytes.length;) {
      const count = fs.readSync(fd, bytes, offset, bytes.length - offset, offset);
      check(count > 0, 'file truncated during cache capture'); offset += count;
    }
    check(fs.readSync(fd, Buffer.alloc(1), 0, 1, before.size) === 0, 'file grew during cache capture');
    const after = fs.fstatSync(fd);
    check(after.isFile() && after.nlink === 1 && bytes.length === before.size &&
      ['dev', 'ino', 'size', 'mode', 'mtimeMs', 'ctimeMs'].every((field) => before[field] === after[field]), 'file changed during cache capture');
    return bytes;
  } finally { fs.closeSync(fd); }
}
function snapshot(root, cachePath) {
  const entries = [], blobs = new Map(); let totalBytes = 0;
  function visit(name, descend) {
    relative(name);
    const filename = path.join(root, name), stat = fs.lstatSync(filename), mode = stat.mode & 0o777;
    check(!(stat.mode & 0o7000), 'special cache mode');
    check(entries.length < MAX_ENTRIES, 'cache entry limit exceeded');
    if (stat.isDirectory()) entries.push({ path: name, type: 'directory', mode });
    else if (stat.isFile()) {
      const bytes = independentBytes(filename, stat, MAX_BYTES - totalBytes), hash = sha(bytes);
      totalBytes += bytes.length; entries.push({ path: name, type: 'file', mode, bytes: bytes.length, sha256: hash });
      blobs.set(hash, bytes);
    } else if (stat.isSymbolicLink()) entries.push({ path: name, type: 'symlink', mode, target: fs.readlinkSync(filename) });
    else check(false, 'special cache filesystem entry');
    check(descend || stat.isDirectory(), 'symlink/non-directory cache ancestor');
    if (descend && stat.isDirectory()) for (const child of fs.readdirSync(filename).sort()) visit(`${name}/${child}`, true);
  }
  const parts = relative(cachePath).split('/');
  for (let index = 1; index < parts.length; index++) visit(parts.slice(0, index).join('/'), false);
  visit(cachePath, true);
  entries.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  return { entries, blobs, digest: cacheEntriesDigest({ root, cachePath, entries }) };
}
function snapshotPrefix(prefix) {
  relative(prefix);
  check(/^(?:cpu-collection|cpu-second)\/cache\/[a-f0-9]{64}$/.test(prefix), 'snapshot requires a fixed per-case cache prefix');
}
function destinationPath(root, destination, cachePath) {
  check(typeof destination === 'string', 'missing snapshot destination');
  if (!path.isAbsolute(destination)) relative(destination);
  const absolute = path.isAbsolute(destination) ? destination : path.resolve(root, destination);
  check(path.resolve(absolute) === absolute, 'noncanonical absolute destination');
  const local = path.relative(root, absolute).split(path.sep).join('/');
  relative(local);
  const output = 'sdk-rs/harness/out/'; check(local.startsWith(output), 'destination escapes output root');
  snapshotPrefix(local.slice(output.length));
  check(!within(local, cachePath) && !within(cachePath, local), 'snapshot destination overlaps live cache');
  return { absolute, local };
}
function mkdirParents(root, local) {
  let current = root;
  for (const part of local.split('/').slice(0, -1)) {
    current = path.join(current, part);
    const prior = state(current);
    if (prior) check(prior.isDirectory() && !prior.isSymbolicLink(), 'snapshot ancestor is a symlink/non-directory');
    else fs.mkdirSync(current, { mode: 0o755 });
  }
}

/** Capture immutable per-case evidence without changing the final live cache. */
export function captureCache({ root, cachePath, destination }) {
  rootIdentity(root); relative(cachePath);
  const output = destinationPath(root, destination, cachePath);
  const captured = snapshot(root, cachePath);
  const manifest = { schema: 1, workspaceRoot: root, cachePath, entries: captured.entries };
  const manifestBytes = Buffer.from(canonical(manifest) + '\n');
  check(manifestBytes.length <= MAX_MANIFEST_BYTES, 'cache manifest byte limit exceeded');
  mkdirParents(root, output.local);
  check(!state(output.absolute), 'snapshot destination collision');
  fs.mkdirSync(output.absolute, { mode: 0o755 });
  try {
    fs.mkdirSync(path.join(output.absolute, 'blobs'), { mode: 0o755 });
    for (const [hash, bytes] of captured.blobs) fs.writeFileSync(path.join(output.absolute, 'blobs', hash), bytes, { flag: 'wx', mode: 0o644 });
    fs.writeFileSync(path.join(output.absolute, 'manifest.json'), manifestBytes, { flag: 'wx', mode: 0o644 });
    check(snapshot(root, cachePath).digest === captured.digest, 'live cache changed while retaining snapshot');
    return { schema: 1, digest: captured.digest, manifest, manifestSha256: sha(manifestBytes), destination: output.absolute };
  } catch (error) {
    fs.rmSync(output.absolute, { recursive: true, force: true }); throw error;
  }
}

/** Verify one cache subtree in a fully validated artifact ZIP Map. Other cases
 * may coexist in that map; no files are extracted or runtime state changed. */
export function verifyCache({ entries, prefix, expectedDigest }) {
  check(entries instanceof Map && HASH.test(expectedDigest), 'missing validated cache entries/digest'); snapshotPrefix(prefix);
  const members = new Map();
  for (const [name, bytes] of entries) {
    relative(name); check(Buffer.isBuffer(bytes), 'cache ZIP member is not bytes');
    if (within(name, prefix)) members.set(name, bytes);
  }
  const manifestBytes = members.get(`${prefix}/manifest.json`);
  check(manifestBytes && manifestBytes.length <= MAX_MANIFEST_BYTES, 'missing/oversized cache manifest');
  const manifest = parseCohortJson(manifestBytes);
  check(manifest && !Array.isArray(manifest) && canonical(Object.keys(manifest).sort()) ===
    canonical(['schema', 'workspaceRoot', 'cachePath', 'entries'].sort()) && manifest.schema === 1 &&
    Array.isArray(manifest.entries) && manifest.entries.length <= MAX_ENTRIES, 'invalid cache manifest schema/entry count');
  const digest = cacheEntriesDigest({ root: manifest.workspaceRoot, cachePath: manifest.cachePath, entries: manifest.entries });
  check(digest === expectedDigest, 'retained cache digest differs');
  const hashes = new Map(); let totalBytes = 0;
  for (const entry of manifest.entries) if (entry.type === 'file') {
    totalBytes += entry.bytes; check(totalBytes <= MAX_BYTES, 'cache byte limit exceeded');
    check(!hashes.has(entry.sha256) || hashes.get(entry.sha256) === entry.bytes, 'inconsistent cache blob length');
    hashes.set(entry.sha256, entry.bytes);
  }
  check(members.size === hashes.size + 1 && [...members.keys()].every((name) => name === `${prefix}/manifest.json` ||
    name.startsWith(`${prefix}/blobs/`) && hashes.has(name.slice(`${prefix}/blobs/`.length))), 'unexpected or missing retained cache members');
  for (const [hash, length] of hashes) {
    const data = members.get(`${prefix}/blobs/${hash}`);
    check(data && data.length === length && sha(data) === hash, 'cache blob bytes/hash differ');
  }
  return { schema: 1, digest, manifest, manifestSha256: sha(manifestBytes) };
}
